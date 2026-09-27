//! Fruit roots: post, veto, finalize, terminal resolution, and pool close.
//!
//! The poster proposes; the program enforces capacity, versioning, monotone
//! finality and the challenge window; an independent veto set can stop a
//! root inside its window. Only a Final root moves `U` into `L`, and only a
//! reconciled close moves unused capacity back to `Z`.

use anchor_lang::prelude::*;
use rigs_engine::{
    close_pool, finalize_root, post_root, terminal_resolution, veto_vote, LedgerKind,
};

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::*;
use crate::fruit_state::{require_version, FruitConfig, FruitVault, Ledger, Pool, Root};
use crate::instructions::fruit_ledger::check_solvent;
use crate::state::Config;
use crate::vault::{move_lamports, require_external_destination};

// ------------------------------------------------------------ post

#[derive(Accounts)]
#[instruction(version: u64)]
pub struct PostFruitRoot<'info> {
    #[account(mut)]
    pub poster: Signer<'info>,

    #[account(
        mut,
        seeds = [FRUIT_CONFIG_SEED],
        bump = fruit_config.bump,
        constraint = fruit_config.root_poster == poster.key() @ RigsError::UnauthorizedRootPoster,
    )]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_POOL_SEED, &[pool.ledger_kind], &pool.pool_id.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// `init`: one account per (pool, version), never closed.
    #[account(
        init,
        payer = poster,
        space = 8 + Root::INIT_SPACE,
        seeds = [FRUIT_ROOT_SEED, pool.key().as_ref(), &version.to_le_bytes()],
        bump,
    )]
    pub root: Box<Account<'info, Root>>,

    pub system_program: Program<'info, System>,
}

pub fn post_fruit_root(
    ctx: Context<PostFruitRoot>,
    version: u64,
    merkle_root: [u8; 32],
    checkpoint_time: i64,
    total: u64,
    complete: bool,
    forfeited: u64,
) -> Result<()> {
    let fc = &ctx.accounts.fruit_config;
    require_version(fc.state_version)?;
    require_version(ctx.accounts.pool.state_version)?;
    require!(
        !fc.is_paused(ctx.accounts.pool.ledger_kind),
        RigsError::FruitPaused
    );
    // Only voluntary lock exits forfeit (§8); core and loyalty roots never
    // carry it, so their close can never release anything out of `Z`.
    require!(
        forfeited == 0
            || LedgerKind::from_u8(ctx.accounts.pool.ledger_kind).anchor()? == LedgerKind::Lock,
        RigsError::ForfeitOnlyOnLockLedger
    );
    // Lock forfeiture is bounded by `sum A_i - final_total` (architecture
    // §8, test 89). `sum A_i` comes from lock registration, which this
    // program version does not have, so no forfeiture can be authenticated:
    // fail closed until the lock engine writes it.
    require!(forfeited == 0, RigsError::LockPoolsNotImplemented);
    let window = fc.challenge_window_secs;
    let now = Clock::get()?.unix_timestamp;

    let mut pool_state = ctx.accounts.pool.accounting()?;
    let root_state = post_root(
        &mut pool_state,
        version,
        checkpoint_time,
        total,
        complete,
        forfeited,
        now,
        window,
    )
    .anchor()?;

    let pool_key = ctx.accounts.pool.key();
    ctx.accounts.pool.write_accounting(&pool_state);
    let fc = &mut ctx.accounts.fruit_config;
    fc.pending_roots = fc
        .pending_roots
        .checked_add(1)
        .ok_or(error!(RigsError::MathOverflow))?;

    let root = &mut ctx.accounts.root;
    root.state_version = FRUIT_STATE_VERSION;
    root.pool = pool_key;
    root.merkle_root = merkle_root;
    root.veto_bitmap = 0;
    root.terminal = false;
    root.bump = ctx.bumps.root;
    root.write_accounting(&root_state);

    emit!(FruitRootPosted {
        pool: pool_key,
        version,
        merkle_root,
        checkpoint_time,
        total,
        complete,
        final_after: root_state.final_after,
    });
    Ok(())
}

// ------------------------------------------------------------ veto

#[derive(Accounts)]
pub struct VetoFruitRoot<'info> {
    pub veto_signer: Signer<'info>,

    #[account(mut, seeds = [FRUIT_CONFIG_SEED], bump = fruit_config.bump)]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_POOL_SEED, &[pool.ledger_kind], &pool.pool_id.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        mut,
        seeds = [FRUIT_ROOT_SEED, pool.key().as_ref(), &root.version.to_le_bytes()],
        bump = root.bump,
        constraint = root.pool == pool.key() @ RigsError::FruitAccountMismatch,
    )]
    pub root: Box<Account<'info, Root>>,
}

/// One veto signature. Not affected by pause: a veto can only stop a root.
pub fn veto_fruit_root(ctx: Context<VetoFruitRoot>) -> Result<()> {
    let fc = &ctx.accounts.fruit_config;
    require_version(fc.state_version)?;
    require_version(ctx.accounts.pool.state_version)?;
    require_version(ctx.accounts.root.state_version)?;
    let signer = ctx.accounts.veto_signer.key();
    // The poster is never in the veto set (validated at init); checked again
    // so the poster can never contribute a veto signature.
    require_keys_neq!(signer, fc.root_poster, RigsError::NotVetoSigner);
    let index = fc
        .veto_index(&signer)
        .ok_or(error!(RigsError::NotVetoSigner))?;
    let bit = 1u8 << index;
    require!(
        ctx.accounts.root.veto_bitmap & bit == 0,
        RigsError::DuplicateVeto
    );
    let threshold = fc.veto_threshold;
    let now = Clock::get()?.unix_timestamp;

    let mut pool_state = ctx.accounts.pool.accounting()?;
    let mut root_state = ctx.accounts.root.accounting()?;
    let vetoed = veto_vote(&mut pool_state, &mut root_state, threshold, now).anchor()?;

    ctx.accounts.pool.write_accounting(&pool_state);
    let root = &mut ctx.accounts.root;
    root.veto_bitmap |= bit;
    root.write_accounting(&root_state);
    if vetoed {
        let fc = &mut ctx.accounts.fruit_config;
        fc.pending_roots = fc.pending_roots.saturating_sub(1);
    }

    emit!(FruitRootVetoVote {
        pool: ctx.accounts.pool.key(),
        version: root_state.version,
        signer,
        veto_count: root_state.veto_count,
        vetoed,
    });
    Ok(())
}

// ------------------------------------------------------------ finalize

#[derive(Accounts)]
pub struct FinalizeFruitRoot<'info> {
    #[account(mut, seeds = [FRUIT_CONFIG_SEED], bump = fruit_config.bump)]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_POOL_SEED, &[pool.ledger_kind], &pool.pool_id.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        mut,
        seeds = [FRUIT_ROOT_SEED, pool.key().as_ref(), &root.version.to_le_bytes()],
        bump = root.bump,
        constraint = root.pool == pool.key() @ RigsError::FruitAccountMismatch,
    )]
    pub root: Box<Account<'info, Root>>,

    #[account(
        mut,
        seeds = [FRUIT_LEDGER_SEED, &[pool.ledger_kind]],
        bump = ledger.bump,
        has_one = vault @ RigsError::FruitAccountMismatch,
    )]
    pub ledger: Account<'info, Ledger>,

    #[account(seeds = [FRUIT_VAULT_SEED, &[pool.ledger_kind]], bump = ledger.vault_bump)]
    pub vault: Account<'info, FruitVault>,
}

/// Permissionless once the window has elapsed without a veto:
/// `U -= Δ; L += Δ; final_total = total`.
pub fn finalize_fruit_root(ctx: Context<FinalizeFruitRoot>) -> Result<()> {
    let fc = &ctx.accounts.fruit_config;
    require_version(fc.state_version)?;
    require_version(ctx.accounts.pool.state_version)?;
    require_version(ctx.accounts.root.state_version)?;
    require_version(ctx.accounts.ledger.state_version)?;
    require!(
        !fc.is_paused(ctx.accounts.pool.ledger_kind),
        RigsError::FruitPaused
    );
    let now = Clock::get()?.unix_timestamp;

    let mut acct = ctx.accounts.ledger.accounting();
    let mut pool_state = ctx.accounts.pool.accounting()?;
    let mut root_state = ctx.accounts.root.accounting()?;
    let established = finalize_root(&mut acct, &mut pool_state, &mut root_state, now).anchor()?;
    check_solvent(&ctx.accounts.vault.to_account_info(), &acct)?;

    let merkle_root = ctx.accounts.root.merkle_root;
    ctx.accounts.ledger.write_accounting(&acct);
    let pool = &mut ctx.accounts.pool;
    pool.write_accounting(&pool_state);
    pool.final_root = merkle_root;
    ctx.accounts.root.write_accounting(&root_state);
    let fc = &mut ctx.accounts.fruit_config;
    fc.pending_roots = fc.pending_roots.saturating_sub(1);

    emit!(FruitRootFinalized {
        pool: ctx.accounts.pool.key(),
        version: root_state.version,
        total: root_state.total,
        established,
        complete: root_state.complete,
        terminal: false,
    });
    Ok(())
}

// ------------------------------------------------------------ terminal resolution

#[derive(Accounts)]
#[instruction(version: u64)]
pub struct ResolveFruitPoolTerminal<'info> {
    /// Pays for the root account. Carries no authority by itself.
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(seeds = [FRUIT_CONFIG_SEED], bump = fruit_config.bump)]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_POOL_SEED, &[pool.ledger_kind], &pool.pool_id.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        init,
        payer = payer,
        space = 8 + Root::INIT_SPACE,
        seeds = [FRUIT_ROOT_SEED, pool.key().as_ref(), &version.to_le_bytes()],
        bump,
    )]
    pub root: Box<Account<'info, Root>>,

    #[account(
        mut,
        seeds = [FRUIT_LEDGER_SEED, &[pool.ledger_kind]],
        bump = ledger.bump,
        has_one = vault @ RigsError::FruitAccountMismatch,
    )]
    pub ledger: Account<'info, Ledger>,

    #[account(seeds = [FRUIT_VAULT_SEED, &[pool.ledger_kind]], bump = ledger.vault_bump)]
    pub vault: Account<'info, FruitVault>,

    pub system_program: Program<'info, System>,
    // remaining_accounts: distinct veto signers, each a transaction signer,
    // at least `veto_threshold` of them.
}

/// §10 terminal resolution: when missing history cannot be recovered, the
/// veto set at its threshold signs a final, complete settlement after `t_end`.
/// It obeys capacity, versioning and monotone finality like any root, so it
/// can only add liabilities (the later close releases the excess to `Z`); it
/// can never reduce `L`. The poster is not a veto signer and cannot do this.
pub fn resolve_fruit_pool_terminal<'info>(
    ctx: Context<'_, '_, 'info, 'info, ResolveFruitPoolTerminal<'info>>,
    version: u64,
    merkle_root: [u8; 32],
    checkpoint_time: i64,
    total: u64,
) -> Result<()> {
    let fc = &ctx.accounts.fruit_config;
    require_version(fc.state_version)?;
    require_version(ctx.accounts.pool.state_version)?;
    require_version(ctx.accounts.ledger.state_version)?;
    require!(
        !fc.is_paused(ctx.accounts.pool.ledger_kind),
        RigsError::FruitPaused
    );

    let mut seen = 0u8;
    for ai in ctx.remaining_accounts {
        require!(ai.is_signer, RigsError::TerminalQuorumNotMet);
        require_keys_neq!(*ai.key, fc.root_poster, RigsError::NotVetoSigner);
        let index = fc
            .veto_index(ai.key)
            .ok_or(error!(RigsError::NotVetoSigner))?;
        let bit = 1u8 << index;
        require!(seen & bit == 0, RigsError::DuplicateVeto);
        seen |= bit;
    }
    require!(
        seen.count_ones() >= u32::from(fc.veto_threshold),
        RigsError::TerminalQuorumNotMet
    );
    let now = Clock::get()?.unix_timestamp;

    let mut acct = ctx.accounts.ledger.accounting();
    let mut pool_state = ctx.accounts.pool.accounting()?;
    let (root_state, established) = terminal_resolution(
        &mut acct,
        &mut pool_state,
        version,
        checkpoint_time,
        total,
        now,
    )
    .anchor()?;
    check_solvent(&ctx.accounts.vault.to_account_info(), &acct)?;

    let pool_key = ctx.accounts.pool.key();
    ctx.accounts.ledger.write_accounting(&acct);
    let pool = &mut ctx.accounts.pool;
    pool.write_accounting(&pool_state);
    pool.final_root = merkle_root;

    let root = &mut ctx.accounts.root;
    root.state_version = FRUIT_STATE_VERSION;
    root.pool = pool_key;
    root.merkle_root = merkle_root;
    root.veto_bitmap = seen;
    root.terminal = true;
    root.bump = ctx.bumps.root;
    root.write_accounting(&root_state);

    emit!(FruitRootFinalized {
        pool: pool_key,
        version,
        total,
        established,
        complete: true,
        terminal: true,
    });
    Ok(())
}

// ------------------------------------------------------------ close

#[derive(Accounts)]
pub struct CloseFruitPool<'info> {
    #[account(seeds = [FRUIT_CONFIG_SEED], bump = fruit_config.bump)]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_POOL_SEED, &[pool.ledger_kind], &pool.pool_id.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        mut,
        seeds = [FRUIT_LEDGER_SEED, &[pool.ledger_kind]],
        bump = ledger.bump,
        has_one = vault @ RigsError::FruitAccountMismatch,
    )]
    pub ledger: Account<'info, Ledger>,

    #[account(
        mut,
        seeds = [FRUIT_VAULT_SEED, &[pool.ledger_kind]],
        bump = ledger.vault_bump,
    )]
    pub vault: Account<'info, FruitVault>,

    /// Base config, read-only: names the Protocol Treasury
    /// (`config.fee_recipient`, architecture §7a). No base vault is touched.
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, Config>>,

    /// CHECK: only credited, and pinned to `config.fee_recipient`; must not
    /// be program-owned (checked in the handler).
    #[account(
        mut,
        constraint = treasury.key() == config.fee_recipient @ RigsError::FeeRecipientMismatch,
    )]
    pub treasury: UncheckedAccount<'info>,
}

/// Permissionless close: `now >= t_end`, no Pending root, a complete Final
/// root; then `U -= E - final_total; Z += E - final_total`, exactly once.
/// For a lock pool whose complete root carries `forfeited`, the close then
/// pays `floor(0.80 forfeited)` out of `Z` to the Protocol Treasury
/// (`V -= t; Z -= t`); the rest stays in `Z` for the next Season. In this
/// version `post_fruit_root` refuses any forfeiture and the engine re-checks
/// `forfeited <= sum A_i - final_total` here with no allocation established,
/// so the release is unreachable until lock registration writes `sum A_i`.
pub fn close_fruit_pool(ctx: Context<CloseFruitPool>) -> Result<()> {
    let fc = &ctx.accounts.fruit_config;
    require_version(fc.state_version)?;
    require_version(ctx.accounts.pool.state_version)?;
    require_version(ctx.accounts.ledger.state_version)?;
    require!(
        !fc.is_paused(ctx.accounts.pool.ledger_kind),
        RigsError::FruitPaused
    );
    let now = Clock::get()?.unix_timestamp;

    let mut acct = ctx.accounts.ledger.accounting();
    let mut pool_state = ctx.accounts.pool.accounting()?;
    let out = close_pool(&mut acct, &mut pool_state, now).anchor()?;
    let vault_ai = ctx.accounts.vault.to_account_info();
    if out.treasury_release > 0 {
        require_external_destination(&ctx.accounts.treasury.to_account_info(), ctx.program_id)?;
        move_lamports(
            &vault_ai,
            &ctx.accounts.treasury.to_account_info(),
            out.treasury_release,
        )?;
    }
    check_solvent(&vault_ai, &acct)?;

    ctx.accounts.ledger.write_accounting(&acct);
    ctx.accounts.pool.write_accounting(&pool_state);

    emit!(FruitPoolClosed {
        pool: ctx.accounts.pool.key(),
        final_total: pool_state.final_total,
        released: out.released,
    });
    if out.treasury_release > 0 {
        emit!(LockForfeitReleased {
            pool: ctx.accounts.pool.key(),
            forfeited: pool_state.final_forfeited,
            treasury_release: out.treasury_release,
            treasury: ctx.accounts.treasury.key(),
        });
    }
    Ok(())
}
