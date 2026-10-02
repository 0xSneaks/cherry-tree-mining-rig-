//! Fruit ledger setup, funding, pool commitment, reserve withdrawal, pause.
//!
//! Every handler reads the engine state out of the accounts, runs one
//! [`rigs_engine::fruit`] transition, moves lamports only between the ledger's
//! own FRUIT VAULT and a user/recipient, re-checks `V >= L + U + Z` against
//! the vault's actual balance, and only then writes state back.

use anchor_lang::prelude::*;
use rigs_engine::{
    commit_pool, FruitLedger, LedgerKind, FRUIT_M, P_REF_LAMPORTS, QMAX_DEN, QMAX_NUM,
};

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::*;
use crate::fruit_state::{require_version, FruitConfig, FruitVault, Ledger, Pool};
use crate::state::Config;
use crate::vault::{deposit_from_wallet, distributable, move_lamports};

/// `V >= L + U + Z` against the vault's real spendable balance.
pub fn check_solvent(vault: &AccountInfo, ledger: &FruitLedger) -> Result<()> {
    ledger.check_solvent(distributable(vault)?).anchor()
}

// ------------------------------------------------------------ init config

#[derive(Accounts)]
pub struct InitFruitConfig<'info> {
    /// The base program authority; becomes the Fruit admin.
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump, has_one = authority)]
    pub config: Account<'info, Config>,

    #[account(
        init,
        payer = authority,
        space = 8 + FruitConfig::INIT_SPACE,
        seeds = [FRUIT_CONFIG_SEED],
        bump,
    )]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    pub system_program: Program<'info, System>,
}

/// Validate the governance inputs. None is chosen here; all are required.
///
/// The poster is never a veto signer and never the admin, so the poster alone
/// can neither veto nor reconfigure. The threshold is at least 1 and at most
/// the set size, so the set can always act.
pub fn validate_governance(
    admin: &Pubkey,
    root_poster: &Pubkey,
    veto_signers: &[Pubkey],
    veto_threshold: u8,
    challenge_window_secs: i64,
    reserve_recipient: &Pubkey,
) -> Result<()> {
    let bad = || error!(RigsError::InvalidFruitGovernance);
    if *root_poster == Pubkey::default()
        || *reserve_recipient == Pubkey::default()
        || root_poster == admin
        || veto_signers.is_empty()
        || veto_signers.len() > MAX_VETO_SIGNERS
        || veto_threshold == 0
        || veto_threshold as usize > veto_signers.len()
        || challenge_window_secs <= 0
    {
        return Err(bad());
    }
    for (i, k) in veto_signers.iter().enumerate() {
        if *k == Pubkey::default() || k == root_poster || veto_signers[..i].contains(k) {
            return Err(bad());
        }
    }
    Ok(())
}

pub fn init_fruit_config(
    ctx: Context<InitFruitConfig>,
    root_poster: Pubkey,
    veto_signers: Vec<Pubkey>,
    veto_threshold: u8,
    challenge_window_secs: i64,
    reserve_recipient: Pubkey,
) -> Result<()> {
    let admin = ctx.accounts.authority.key();
    validate_governance(
        &admin,
        &root_poster,
        &veto_signers,
        veto_threshold,
        challenge_window_secs,
        &reserve_recipient,
    )?;

    let fc = &mut ctx.accounts.fruit_config;
    fc.state_version = FRUIT_STATE_VERSION;
    fc.admin = admin;
    fc.root_poster = root_poster;
    fc.veto_signers = [Pubkey::default(); MAX_VETO_SIGNERS];
    fc.veto_signers[..veto_signers.len()].copy_from_slice(&veto_signers);
    fc.veto_signer_count = veto_signers.len() as u8;
    fc.veto_threshold = veto_threshold;
    fc.challenge_window_secs = challenge_window_secs;
    fc.p_ref_lamports = P_REF_LAMPORTS;
    fc.m_seeds = FRUIT_M;
    fc.qmax_num = QMAX_NUM as u8;
    fc.qmax_den = QMAX_DEN as u8;
    fc.paused = [false; 3];
    fc.pending_roots = 0;
    fc.reserve_recipient = reserve_recipient;
    fc.bump = ctx.bumps.fruit_config;

    emit!(FruitConfigInitialized {
        admin,
        root_poster,
        veto_signer_count: fc.veto_signer_count,
        veto_threshold,
        challenge_window_secs,
        reserve_recipient,
    });
    Ok(())
}

// ------------------------------------------------------------ init ledger

#[derive(Accounts)]
#[instruction(kind: u8)]
pub struct InitFruitLedger<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [FRUIT_CONFIG_SEED],
        bump = fruit_config.bump,
        has_one = admin @ RigsError::UnauthorizedFruitAdmin,
    )]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        init,
        payer = admin,
        space = 8 + Ledger::INIT_SPACE,
        seeds = [FRUIT_LEDGER_SEED, &[kind]],
        bump,
    )]
    pub ledger: Account<'info, Ledger>,

    #[account(
        init,
        payer = admin,
        space = 8 + FruitVault::INIT_SPACE,
        seeds = [FRUIT_VAULT_SEED, &[kind]],
        bump,
    )]
    pub vault: Account<'info, FruitVault>,

    pub system_program: Program<'info, System>,
}

pub fn init_fruit_ledger(ctx: Context<InitFruitLedger>, kind: u8) -> Result<()> {
    require_version(ctx.accounts.fruit_config.state_version)?;
    LedgerKind::from_u8(kind).anchor()?;

    let vault_key = ctx.accounts.vault.key();
    let vault = &mut ctx.accounts.vault;
    vault.kind = kind;
    vault.bump = ctx.bumps.vault;

    let ledger = &mut ctx.accounts.ledger;
    ledger.state_version = FRUIT_STATE_VERSION;
    ledger.kind = kind;
    ledger.vault = vault_key;
    ledger.vault_bump = ctx.bumps.vault;
    ledger.bump = ctx.bumps.ledger;

    emit!(FruitLedgerInitialized {
        kind,
        ledger: ledger.key(),
        vault: vault_key,
    });
    Ok(())
}

// ------------------------------------------------------------ ledger + vault

/// A ledger and its own vault, cross-checked. Shared by every handler that
/// moves Fruit lamports or accounting.
#[derive(Accounts)]
#[instruction(kind: u8)]
pub struct FundFruit<'info> {
    #[account(mut)]
    pub funder: Signer<'info>,

    #[account(seeds = [FRUIT_CONFIG_SEED], bump = fruit_config.bump)]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_LEDGER_SEED, &[kind]],
        bump = ledger.bump,
        has_one = vault @ RigsError::FruitAccountMismatch,
    )]
    pub ledger: Account<'info, Ledger>,

    #[account(mut, seeds = [FRUIT_VAULT_SEED, &[kind]], bump = ledger.vault_bump)]
    pub vault: Account<'info, FruitVault>,

    pub system_program: Program<'info, System>,
}

/// FUND: `V += a; Z += a`. Permissionless: a deposit can only add reserve.
pub fn fund_fruit(ctx: Context<FundFruit>, kind: u8, amount: u64) -> Result<()> {
    require_version(ctx.accounts.fruit_config.state_version)?;
    require_version(ctx.accounts.ledger.state_version)?;

    let mut acct = ctx.accounts.ledger.accounting();
    acct.fund(amount).anchor()?;

    let vault_ai = ctx.accounts.vault.to_account_info();
    deposit_from_wallet(
        &ctx.accounts.system_program,
        &ctx.accounts.funder,
        &vault_ai,
        amount,
    )?;
    check_solvent(&vault_ai, &acct)?;

    let ledger = &mut ctx.accounts.ledger;
    ledger.write_accounting(&acct);
    ledger.funded_cum = ledger
        .funded_cum
        .checked_add(amount)
        .ok_or(error!(RigsError::MathOverflow))?;

    emit!(FruitFunded {
        kind,
        funder: ctx.accounts.funder.key(),
        amount,
        reserve: acct.reserve,
    });
    Ok(())
}

// ------------------------------------------------------------ commit pool

#[derive(Accounts)]
#[instruction(kind: u8, pool_id: u64)]
pub struct CommitFruitPool<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [FRUIT_CONFIG_SEED],
        bump = fruit_config.bump,
        has_one = admin @ RigsError::UnauthorizedFruitAdmin,
    )]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_LEDGER_SEED, &[kind]],
        bump = ledger.bump,
        has_one = vault @ RigsError::FruitAccountMismatch,
    )]
    pub ledger: Account<'info, Ledger>,

    #[account(seeds = [FRUIT_VAULT_SEED, &[kind]], bump = ledger.vault_bump)]
    pub vault: Account<'info, FruitVault>,

    /// `init`: a pool id can be committed once, ever.
    #[account(
        init,
        payer = admin,
        space = 8 + Pool::INIT_SPACE,
        seeds = [FRUIT_POOL_SEED, &[kind], &pool_id.to_le_bytes()],
        bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    pub system_program: Program<'info, System>,
}

/// COMMIT POOL: requires `Z >= E` and `now < t_start`; `Z -= E; U += E`.
pub fn commit_fruit_pool(
    ctx: Context<CommitFruitPool>,
    kind: u8,
    pool_id: u64,
    budget: u64,
    t_start: i64,
    t_end: i64,
    reference_hash: [u8; 32],
) -> Result<()> {
    let fc = &ctx.accounts.fruit_config;
    require_version(fc.state_version)?;
    require_version(ctx.accounts.ledger.state_version)?;
    require!(!fc.is_paused(kind), RigsError::FruitPaused);
    // Lock pools have their own window, Season account and tally (§8); they
    // are committed only by `commit_lock_pool`.
    require!(
        LedgerKind::from_u8(kind).anchor()? != LedgerKind::Lock,
        RigsError::LockPoolsNotImplemented
    );
    let now = Clock::get()?.unix_timestamp;

    let mut acct = ctx.accounts.ledger.accounting();
    let pool_state = commit_pool(&mut acct, budget, t_start, t_end, now).anchor()?;
    check_solvent(&ctx.accounts.vault.to_account_info(), &acct)?;
    ctx.accounts.ledger.write_accounting(&acct);

    let pool_key = ctx.accounts.pool.key();
    let pool = &mut ctx.accounts.pool;
    pool.state_version = FRUIT_STATE_VERSION;
    pool.ledger_kind = kind;
    pool.pool_id = pool_id;
    pool.reference_hash = reference_hash;
    pool.h_lock = 0;
    pool.final_root = [0; 32];
    pool.committed_at = now;
    pool.bump = ctx.bumps.pool;
    pool.write_accounting(&pool_state);

    emit!(FruitPoolCommitted {
        kind,
        pool: pool_key,
        pool_id,
        budget,
        t_start,
        t_end,
        wallet_cap: pool_state.wallet_cap,
        reference_hash,
    });
    Ok(())
}

// ------------------------------------------------------------ withdraw reserve

#[derive(Accounts)]
#[instruction(kind: u8)]
pub struct WithdrawFruitReserve<'info> {
    pub admin: Signer<'info>,

    #[account(
        seeds = [FRUIT_CONFIG_SEED],
        bump = fruit_config.bump,
        has_one = admin @ RigsError::UnauthorizedFruitAdmin,
    )]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_LEDGER_SEED, &[kind]],
        bump = ledger.bump,
        has_one = vault @ RigsError::FruitAccountMismatch,
    )]
    pub ledger: Account<'info, Ledger>,

    #[account(mut, seeds = [FRUIT_VAULT_SEED, &[kind]], bump = ledger.vault_bump)]
    pub vault: Account<'info, FruitVault>,

    /// CHECK: only credited; pinned to `fruit_config.reserve_recipient`.
    #[account(
        mut,
        constraint = destination.key() == fruit_config.reserve_recipient
            @ RigsError::ReserveRecipientMismatch,
    )]
    pub destination: UncheckedAccount<'info>,
}

/// RESERVE WITHDRAWAL: `V -= z; Z -= z`. Never touches `L` or `U`.
pub fn withdraw_fruit_reserve(
    ctx: Context<WithdrawFruitReserve>,
    kind: u8,
    amount: u64,
) -> Result<()> {
    require_version(ctx.accounts.fruit_config.state_version)?;
    require_version(ctx.accounts.ledger.state_version)?;

    let mut acct = ctx.accounts.ledger.accounting();
    acct.withdraw_reserve(amount).anchor()?;

    let vault_ai = ctx.accounts.vault.to_account_info();
    move_lamports(
        &vault_ai,
        &ctx.accounts.destination.to_account_info(),
        amount,
    )?;
    check_solvent(&vault_ai, &acct)?;

    let ledger = &mut ctx.accounts.ledger;
    ledger.write_accounting(&acct);
    ledger.withdrawn_cum = ledger
        .withdrawn_cum
        .checked_add(amount)
        .ok_or(error!(RigsError::MathOverflow))?;

    emit!(FruitReserveWithdrawn {
        kind,
        destination: ctx.accounts.destination.key(),
        amount,
        reserve: acct.reserve,
    });
    Ok(())
}

// ------------------------------------------------------------ pause

#[derive(Accounts)]
pub struct FruitAdminOnly<'info> {
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [FRUIT_CONFIG_SEED],
        bump = fruit_config.bump,
        has_one = admin @ RigsError::UnauthorizedFruitAdmin,
    )]
    pub fruit_config: Box<Account<'info, FruitConfig>>,
}

/// Pause stops new pools, roots, finalization and close on one ledger. It
/// never touches `L`, and never blocks claims.
pub fn set_fruit_paused(ctx: Context<FruitAdminOnly>, kind: u8, paused: bool) -> Result<()> {
    require_version(ctx.accounts.fruit_config.state_version)?;
    LedgerKind::from_u8(kind).anchor()?;
    ctx.accounts.fruit_config.paused[kind as usize] = paused;
    emit!(FruitPausedSet { kind, paused });
    Ok(())
}
