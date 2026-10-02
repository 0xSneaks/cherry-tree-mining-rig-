//! Lock Season program surface (architecture §8; Pass #7 P7-B): commit the
//! Season's lock pool, register / cancel, and the on-chain allocation tally
//! that ends in a Final, immutable allocation.
//!
//! Every handler reads the pool and its PDA-bound `LockSeason` into the
//! engine ([`Pool::lock_accounting`]), runs one [`rigs_engine::fruit_lock`]
//! transition, and writes both back only on success. The pool kind comes
//! from the stored `ledger_kind`, which is bound into the pool PDA; no
//! caller-supplied value can relabel a pool.
//!
//! `assigned_total` is never an input (V-1 / V-2 closed by construction):
//! the poster posts only `ell_i <= requested_i` per active registration, and
//! the permissionless award pass computes each `A_i` and adds it exactly once
//! per generation.
//!
//! P7-D: lock roots, finalization, terminal resolution, claims and close go
//! through the lock instructions below, which load the Season; the generic
//! Fruit instructions refuse lock pools. Vetoes on lock roots use the
//! generic `veto_fruit_root`.

use anchor_lang::prelude::*;
use rigs_engine::{
    award_lock_registration, cancel_lock, claim_lock as engine_claim_lock, close_pool,
    commit_lock_pool as engine_commit_lock_pool,
    finalize_lock_allocation as engine_finalize_lock_allocation, finalize_root, fruit_t0,
    post_lock_root as engine_post_lock_root, post_lock_weight,
    register_lock as engine_register_lock, seal_lock_awards as engine_seal_lock_awards,
    seal_lock_weights as engine_seal_lock_weights, terminal_lock_resolution,
    veto_lock_allocation as engine_veto_lock_allocation, FruitPool, LedgerKind,
    LockRegistrationState, SafetyRelease, DAY_SECONDS,
};

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::*;
use crate::fruit_merkle;
use crate::fruit_state::{
    require_version, ClaimRecord, FruitConfig, FruitVault, Ledger, LockRegistration, LockSeason,
    Pool, Root,
};
use crate::instructions::fruit_ledger::check_solvent;
use crate::state::Config;
use crate::vault::{move_lamports, require_external_destination};

const LOCK_KIND: u8 = LedgerKind::Lock as u8;

fn load(pool: &Account<Pool>, season: &Account<LockSeason>) -> Result<FruitPool> {
    require_version(pool.state_version)?;
    pool.lock_accounting(&pool.key(), season)
}

fn require_lock_unpaused(fc: &FruitConfig) -> Result<()> {
    require_version(fc.state_version)?;
    require!(!fc.is_paused(LOCK_KIND), RigsError::FruitPaused);
    Ok(())
}

/// Run `f` over the batch of `LockRegistration` accounts in
/// `remaining_accounts`: each must be writable, owned by this program, of
/// this pool, and appear once. Each is written back right after `f`.
fn for_each_registration<'info>(
    pool_key: &Pubkey,
    accounts: &'info [AccountInfo<'info>],
    mut f: impl FnMut(usize, &mut LockRegistrationState) -> Result<()>,
) -> Result<()> {
    require!(
        !accounts.is_empty() && accounts.len() <= MAX_LOCK_BATCH,
        RigsError::LockBatchMismatch
    );
    let mut seen: Vec<Pubkey> = Vec::with_capacity(accounts.len());
    for (i, info) in accounts.iter().enumerate() {
        require!(
            !seen.contains(info.key),
            RigsError::DuplicateLockRegistration
        );
        seen.push(*info.key);
        require!(info.is_writable, RigsError::LockRegistrationMismatch);
        let mut reg: Account<'info, LockRegistration> = Account::try_from(info)?;
        require_version(reg.state_version)?;
        require_keys_eq!(reg.pool, *pool_key, RigsError::LockRegistrationMismatch);
        let mut state = reg.state();
        f(i, &mut state)?;
        reg.write_state(&state);
        reg.exit(&crate::ID)?;
    }
    Ok(())
}

// ------------------------------------------------------------ commit lock pool

#[derive(Accounts)]
#[instruction(pool_id: u64)]
pub struct CommitLockPool<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [FRUIT_CONFIG_SEED],
        bump = fruit_config.bump,
        has_one = admin @ RigsError::UnauthorizedFruitAdmin,
    )]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    /// The base config: the Season is derived from its `launch_t0`.
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, Config>>,

    #[account(
        mut,
        seeds = [FRUIT_LEDGER_SEED, &[LOCK_KIND]],
        bump = ledger.bump,
        has_one = vault @ RigsError::FruitAccountMismatch,
    )]
    pub ledger: Account<'info, Ledger>,

    #[account(seeds = [FRUIT_VAULT_SEED, &[LOCK_KIND]], bump = ledger.vault_bump)]
    pub vault: Account<'info, FruitVault>,

    /// `init`: a pool id can be committed once, ever.
    #[account(
        init,
        payer = admin,
        space = 8 + Pool::INIT_SPACE,
        seeds = [FRUIT_POOL_SEED, &[LOCK_KIND], &pool_id.to_le_bytes()],
        bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        init,
        payer = admin,
        space = 8 + LockSeason::INIT_SPACE,
        seeds = [LOCK_SEASON_SEED, pool.key().as_ref()],
        bump,
    )]
    pub season: Box<Account<'info, LockSeason>>,

    pub system_program: Program<'info, System>,
}

/// COMMIT LOCK POOL: `[t_s, t_s + 5 days)` with `t_s = FRUIT_T0 + 6 days`
/// derived from the base `launch_t0`, before `t_s`; `Z -= E; U += E`. Creates
/// the pool and its Season together.
pub fn commit_lock_pool(
    ctx: Context<CommitLockPool>,
    pool_id: u64,
    budget: u64,
    reference_hash: [u8; 32],
) -> Result<()> {
    require_lock_unpaused(&ctx.accounts.fruit_config)?;
    require_version(ctx.accounts.ledger.state_version)?;
    let launch_t0 = ctx.accounts.config.launch_t0;
    require!(launch_t0 != 0, RigsError::LockSeasonUnscheduled);
    // FRUIT_T0 = T0 + 1 day (§3), checked.
    let season_t0 = launch_t0
        .checked_add(DAY_SECONDS)
        .ok_or(error!(RigsError::MathOverflow))?;
    debug_assert_eq!(season_t0, fruit_t0(launch_t0));
    // B-1: one lock pool per Season. The pool id is the Season's FRUIT_T0,
    // so a second lock pool for the same Season hits the same PDA (`init`
    // fails) and one wallet can make only one election per Season (§8).
    require!(pool_id == season_t0 as u64, RigsError::LockPoolIdMismatch);
    let now = Clock::get()?.unix_timestamp;

    let mut acct = ctx.accounts.ledger.accounting();
    let pool_state = engine_commit_lock_pool(&mut acct, budget, season_t0, now).anchor()?;
    check_solvent(&ctx.accounts.vault.to_account_info(), &acct)?;
    ctx.accounts.ledger.write_accounting(&acct);

    let pool_key = ctx.accounts.pool.key();
    let season_key = ctx.accounts.season.key();
    let pool = &mut ctx.accounts.pool;
    pool.state_version = FRUIT_STATE_VERSION;
    pool.ledger_kind = LOCK_KIND;
    pool.pool_id = pool_id;
    pool.reference_hash = reference_hash;
    pool.final_root = [0; 32];
    pool.committed_at = now;
    pool.bump = ctx.bumps.pool;
    let season = &mut ctx.accounts.season;
    season.state_version = FRUIT_STATE_VERSION;
    season.pool = pool_key;
    season.pool_id = pool_id;
    season.fruit_t0 = season_t0;
    season.veto_bitmap = 0;
    season.bump = ctx.bumps.season;
    pool.write_lock_accounting(season, &pool_state);

    emit!(LockPoolCommitted {
        pool: pool_key,
        season: season_key,
        pool_id,
        budget,
        t_start: pool_state.t_start,
        t_end: pool_state.t_end,
        fruit_t0: season_t0,
    });
    Ok(())
}

// ------------------------------------------------------------ register / cancel

#[derive(Accounts)]
pub struct RegisterLock<'info> {
    /// The registering wallet; pays the registration's rent.
    #[account(mut)]
    pub wallet: Signer<'info>,

    #[account(seeds = [FRUIT_CONFIG_SEED], bump = fruit_config.bump)]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_POOL_SEED, &[pool.ledger_kind], &pool.pool_id.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(mut, seeds = [LOCK_SEASON_SEED, pool.key().as_ref()], bump = season.bump)]
    pub season: Box<Account<'info, LockSeason>>,

    /// `init`: one registration per wallet per Season, ever — a cancelled
    /// registration keeps this address taken.
    #[account(
        init,
        payer = wallet,
        space = 8 + LockRegistration::INIT_SPACE,
        seeds = [LOCK_REG_SEED, pool.key().as_ref(), wallet.key().as_ref()],
        bump,
    )]
    pub registration: Box<Account<'info, LockRegistration>>,

    pub system_program: Program<'info, System>,
}

/// REGISTER: inside `[FRUIT_T0, t_s)`, `0 < requested <= 2.5 SOL`. Stores
/// intent only (§8); `ell_i` is fixed from the `t_s` snapshot later.
pub fn register_lock(ctx: Context<RegisterLock>, requested: u64) -> Result<()> {
    require_lock_unpaused(&ctx.accounts.fruit_config)?;
    let now = Clock::get()?.unix_timestamp;
    let mut p = load(&ctx.accounts.pool, &ctx.accounts.season)?;
    let reg_state = engine_register_lock(&mut p, requested, now).anchor()?;

    let pool_key = ctx.accounts.pool.key();
    let wallet = ctx.accounts.wallet.key();
    let reg = &mut ctx.accounts.registration;
    reg.state_version = FRUIT_STATE_VERSION;
    reg.pool = pool_key;
    reg.wallet = wallet;
    reg.registered_at = now;
    reg.cancelled_at = 0;
    reg.bump = ctx.bumps.registration;
    reg.write_state(&reg_state);
    ctx.accounts
        .pool
        .write_lock_accounting(&mut ctx.accounts.season, &p);

    emit!(LockRegistered {
        pool: pool_key,
        wallet,
        requested,
        active_reg_count: p.lock.active_reg_count,
        active_requested_total: p.lock.active_requested_total,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct CancelLock<'info> {
    pub wallet: Signer<'info>,

    #[account(seeds = [FRUIT_CONFIG_SEED], bump = fruit_config.bump)]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_POOL_SEED, &[pool.ledger_kind], &pool.pool_id.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(mut, seeds = [LOCK_SEASON_SEED, pool.key().as_ref()], bump = season.bump)]
    pub season: Box<Account<'info, LockSeason>>,

    #[account(
        mut,
        seeds = [LOCK_REG_SEED, pool.key().as_ref(), wallet.key().as_ref()],
        bump = registration.bump,
        constraint = registration.pool == pool.key() @ RigsError::LockRegistrationMismatch,
        constraint = registration.wallet == wallet.key() @ RigsError::LockRegistrationMismatch,
    )]
    pub registration: Box<Account<'info, LockRegistration>>,
}

/// CANCEL (D-B1): inside `[FRUIT_T0, t_s)` only, once. Atomically marks the
/// registration cancelled and removes it from the active count and the
/// active requested total. The account stays, so the wallet can never
/// register again for this Season. Not blocked by pause: it only withdraws
/// intent.
pub fn cancel_lock_registration(ctx: Context<CancelLock>) -> Result<()> {
    require_version(ctx.accounts.fruit_config.state_version)?;
    require_version(ctx.accounts.registration.state_version)?;
    let now = Clock::get()?.unix_timestamp;
    let mut p = load(&ctx.accounts.pool, &ctx.accounts.season)?;
    let mut reg_state = ctx.accounts.registration.state();
    cancel_lock(&mut p, &mut reg_state, now).anchor()?;

    let reg = &mut ctx.accounts.registration;
    reg.write_state(&reg_state);
    reg.cancelled_at = now;
    let requested = reg.requested;
    ctx.accounts
        .pool
        .write_lock_accounting(&mut ctx.accounts.season, &p);

    emit!(LockCancelled {
        pool: ctx.accounts.pool.key(),
        wallet: ctx.accounts.wallet.key(),
        requested,
        active_reg_count: p.lock.active_reg_count,
        active_requested_total: p.lock.active_requested_total,
    });
    Ok(())
}

// ------------------------------------------------------------ tally

#[derive(Accounts)]
pub struct PostLockWeights<'info> {
    pub poster: Signer<'info>,

    #[account(
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

    #[account(mut, seeds = [LOCK_SEASON_SEED, pool.key().as_ref()], bump = season.bump)]
    pub season: Box<Account<'info, LockSeason>>,
}

/// POST WEIGHTS (root poster): `weights[i]` is `ell_i` for the i-th
/// registration in the remaining accounts. At or after `t_s`, once per
/// registration per generation, `ell_i <= requested_i`, never for a cancelled
/// registration.
pub fn post_lock_weights<'info>(
    ctx: Context<'_, '_, 'info, 'info, PostLockWeights<'info>>,
    weights: Vec<u64>,
) -> Result<()> {
    require_lock_unpaused(&ctx.accounts.fruit_config)?;
    require!(
        weights.len() == ctx.remaining_accounts.len(),
        RigsError::LockBatchMismatch
    );
    let now = Clock::get()?.unix_timestamp;
    let pool_key = ctx.accounts.pool.key();
    let mut p = load(&ctx.accounts.pool, &ctx.accounts.season)?;
    for_each_registration(&pool_key, ctx.remaining_accounts, |i, reg| {
        post_lock_weight(&mut p, reg, weights[i], now).anchor()
    })?;
    ctx.accounts
        .pool
        .write_lock_accounting(&mut ctx.accounts.season, &p);

    emit!(LockWeightsPosted {
        pool: pool_key,
        generation: p.lock.generation,
        count: weights.len() as u32,
        weighed_count: p.lock.weighed_count,
        sum_ell: p.lock.sum_ell,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct LockTallyStep<'info> {
    #[account(seeds = [FRUIT_CONFIG_SEED], bump = fruit_config.bump)]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_POOL_SEED, &[pool.ledger_kind], &pool.pool_id.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(mut, seeds = [LOCK_SEASON_SEED, pool.key().as_ref()], bump = season.bump)]
    pub season: Box<Account<'info, LockSeason>>,
}

/// SEAL WEIGHTS (permissionless): every active registration is weighed in
/// this generation (with no active registration, an empty tally). Fixes
/// `H_lock` and `gross_allocatable` for the award pass.
pub fn seal_lock_weights(ctx: Context<LockTallyStep>) -> Result<()> {
    require_lock_unpaused(&ctx.accounts.fruit_config)?;
    let now = Clock::get()?.unix_timestamp;
    let mut p = load(&ctx.accounts.pool, &ctx.accounts.season)?;
    engine_seal_lock_weights(&mut p, now).anchor()?;
    ctx.accounts
        .pool
        .write_lock_accounting(&mut ctx.accounts.season, &p);

    emit!(LockWeightsSealed {
        pool: ctx.accounts.pool.key(),
        generation: p.lock.generation,
        sum_ell: p.lock.sum_ell,
        h_lock: p.lock.h_lock,
        gross_allocatable: p.lock.gross_allocatable,
    });
    Ok(())
}

/// AWARD (permissionless, D-B4): computes `A_i = floor(B * 5 ell_i / H_lock)`
/// for each registration in the remaining accounts and adds it to
/// `assigned_total`, once per generation.
pub fn award_lock_registrations<'info>(
    ctx: Context<'_, '_, 'info, 'info, LockTallyStep<'info>>,
) -> Result<()> {
    require_lock_unpaused(&ctx.accounts.fruit_config)?;
    let pool_key = ctx.accounts.pool.key();
    let mut p = load(&ctx.accounts.pool, &ctx.accounts.season)?;
    let count = ctx.remaining_accounts.len();
    for_each_registration(&pool_key, ctx.remaining_accounts, |_, reg| {
        award_lock_registration(&mut p, reg).anchor().map(|_| ())
    })?;
    ctx.accounts
        .pool
        .write_lock_accounting(&mut ctx.accounts.season, &p);

    emit!(LockAwardsComputed {
        pool: pool_key,
        generation: p.lock.generation,
        count: count as u32,
        awarded_count: p.lock.awarded_count,
        assigned_total: p.lock.assigned_total,
    });
    Ok(())
}

/// SEAL AWARDS (permissionless): every active registration awarded; the
/// whole tally revalidates. Opens the allocation's challenge window.
pub fn seal_lock_awards(ctx: Context<LockTallyStep>) -> Result<()> {
    require_lock_unpaused(&ctx.accounts.fruit_config)?;
    let window = ctx.accounts.fruit_config.challenge_window_secs;
    let now = Clock::get()?.unix_timestamp;
    let mut p = load(&ctx.accounts.pool, &ctx.accounts.season)?;
    engine_seal_lock_awards(&mut p, now, window).anchor()?;
    ctx.accounts
        .pool
        .write_lock_accounting(&mut ctx.accounts.season, &p);

    emit!(LockAwardsSealed {
        pool: ctx.accounts.pool.key(),
        generation: p.lock.generation,
        assigned_total: p.lock.assigned_total,
        rounding_dust: p.lock.gross_allocatable - p.lock.assigned_total,
        final_after: p.lock.final_after,
    });
    Ok(())
}

// ------------------------------------------------------------ veto / finalize

#[derive(Accounts)]
pub struct VetoLockAllocation<'info> {
    pub veto_signer: Signer<'info>,

    #[account(seeds = [FRUIT_CONFIG_SEED], bump = fruit_config.bump)]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_POOL_SEED, &[pool.ledger_kind], &pool.pool_id.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(mut, seeds = [LOCK_SEASON_SEED, pool.key().as_ref()], bump = season.bump)]
    pub season: Box<Account<'info, LockSeason>>,
}

/// One veto signature against the Pending allocation, inside its window. At
/// the threshold the generation is retired and the tally restarts over the
/// same frozen active registration set. Not affected by pause.
pub fn veto_lock_allocation(ctx: Context<VetoLockAllocation>) -> Result<()> {
    let fc = &ctx.accounts.fruit_config;
    require_version(fc.state_version)?;
    let signer = ctx.accounts.veto_signer.key();
    require_keys_neq!(signer, fc.root_poster, RigsError::NotVetoSigner);
    let index = fc
        .veto_index(&signer)
        .ok_or(error!(RigsError::NotVetoSigner))?;
    let bit = 1u8 << index;
    require!(
        ctx.accounts.season.veto_bitmap & bit == 0,
        RigsError::DuplicateVeto
    );
    let threshold = fc.veto_threshold;
    let now = Clock::get()?.unix_timestamp;
    let mut p = load(&ctx.accounts.pool, &ctx.accounts.season)?;
    let generation = p.lock.generation;
    let vetoed = engine_veto_lock_allocation(&mut p, threshold, now).anchor()?;

    let season = &mut ctx.accounts.season;
    season.veto_bitmap |= bit;
    ctx.accounts.pool.write_lock_accounting(season, &p);

    emit!(LockAllocationVetoVote {
        pool: ctx.accounts.pool.key(),
        generation,
        signer,
        veto_count: if vetoed { threshold } else { p.lock.veto_count },
        vetoed,
    });
    Ok(())
}

/// FINALIZE (permissionless) after the window without a veto: the tally
/// revalidates in full and becomes Final for good; `Pool.h_lock` mirrors
/// `H_lock`. Moves no lamports and does not touch `U` or `Z` (P7-L2).
pub fn finalize_lock_allocation(ctx: Context<LockTallyStep>) -> Result<()> {
    require_lock_unpaused(&ctx.accounts.fruit_config)?;
    let now = Clock::get()?.unix_timestamp;
    let mut p = load(&ctx.accounts.pool, &ctx.accounts.season)?;
    let a = engine_finalize_lock_allocation(&mut p, now).anchor()?;
    ctx.accounts
        .pool
        .write_lock_accounting(&mut ctx.accounts.season, &p);

    emit!(LockAllocationFinalized {
        pool: ctx.accounts.pool.key(),
        generation: a.generation,
        h_lock: a.h_lock,
        gross_allocatable: a.gross_allocatable,
        assigned_total: a.assigned_total,
        rounding_dust: a.rounding_dust(),
    });
    Ok(())
}

// =================================================================== P7-D: settlement

/// Safety release stays disabled: the EXTREME minimum-dwell period is unset
/// and no instruction can set it (D-B5, deferred). Lock capacity is the
/// cliff only.
const LOCK_SAFETY: SafetyRelease = SafetyRelease::Disabled;

#[derive(Accounts)]
#[instruction(version: u64)]
pub struct PostLockRoot<'info> {
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

    #[account(seeds = [LOCK_SEASON_SEED, pool.key().as_ref()], bump = season.bump)]
    pub season: Box<Account<'info, LockSeason>>,

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

/// POST LOCK ROOT (root poster): against the pool's Final allocation, with
/// the lock cliff capacity (`0` before `t_s + 5 days`, `assigned_total`
/// from it) and `forfeited <= assigned_total - total` on a complete root.
pub fn post_lock_root(
    ctx: Context<PostLockRoot>,
    version: u64,
    merkle_root: [u8; 32],
    checkpoint_time: i64,
    total: u64,
    complete: bool,
    forfeited: u64,
) -> Result<()> {
    require_lock_unpaused(&ctx.accounts.fruit_config)?;
    let window = ctx.accounts.fruit_config.challenge_window_secs;
    let now = Clock::get()?.unix_timestamp;
    let mut p = load(&ctx.accounts.pool, &ctx.accounts.season)?;
    let root_state = engine_post_lock_root(
        &mut p,
        version,
        checkpoint_time,
        total,
        complete,
        forfeited,
        now,
        window,
        LOCK_SAFETY,
    )
    .anchor()?;

    let pool_key = ctx.accounts.pool.key();
    ctx.accounts.pool.write_accounting(&p);
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

#[derive(Accounts)]
pub struct FinalizeLockRoot<'info> {
    #[account(mut, seeds = [FRUIT_CONFIG_SEED], bump = fruit_config.bump)]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_POOL_SEED, &[pool.ledger_kind], &pool.pool_id.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(seeds = [LOCK_SEASON_SEED, pool.key().as_ref()], bump = season.bump)]
    pub season: Box<Account<'info, LockSeason>>,

    #[account(
        mut,
        seeds = [FRUIT_ROOT_SEED, pool.key().as_ref(), &root.version.to_le_bytes()],
        bump = root.bump,
        constraint = root.pool == pool.key() @ RigsError::FruitAccountMismatch,
    )]
    pub root: Box<Account<'info, Root>>,

    #[account(
        mut,
        seeds = [FRUIT_LEDGER_SEED, &[LOCK_KIND]],
        bump = ledger.bump,
        has_one = vault @ RigsError::FruitAccountMismatch,
    )]
    pub ledger: Account<'info, Ledger>,

    #[account(seeds = [FRUIT_VAULT_SEED, &[LOCK_KIND]], bump = ledger.vault_bump)]
    pub vault: Account<'info, FruitVault>,
}

/// FINALIZE LOCK ROOT (permissionless) after its window: `U -= Δ; L += Δ`,
/// re-capped at the Final `assigned_total`. Vetoes use `veto_fruit_root`.
pub fn finalize_lock_root(ctx: Context<FinalizeLockRoot>) -> Result<()> {
    require_lock_unpaused(&ctx.accounts.fruit_config)?;
    require_version(ctx.accounts.root.state_version)?;
    require_version(ctx.accounts.ledger.state_version)?;
    let now = Clock::get()?.unix_timestamp;
    let mut acct = ctx.accounts.ledger.accounting();
    let mut p = load(&ctx.accounts.pool, &ctx.accounts.season)?;
    let mut root_state = ctx.accounts.root.accounting()?;
    let established = finalize_root(&mut acct, &mut p, &mut root_state, now).anchor()?;
    check_solvent(&ctx.accounts.vault.to_account_info(), &acct)?;

    let merkle_root = ctx.accounts.root.merkle_root;
    ctx.accounts.ledger.write_accounting(&acct);
    let pool = &mut ctx.accounts.pool;
    pool.write_accounting(&p);
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

#[derive(Accounts)]
#[instruction(version: u64)]
pub struct ResolveLockPoolTerminal<'info> {
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

    #[account(seeds = [LOCK_SEASON_SEED, pool.key().as_ref()], bump = season.bump)]
    pub season: Box<Account<'info, LockSeason>>,

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
        seeds = [FRUIT_LEDGER_SEED, &[LOCK_KIND]],
        bump = ledger.bump,
        has_one = vault @ RigsError::FruitAccountMismatch,
    )]
    pub ledger: Account<'info, Ledger>,

    #[account(seeds = [FRUIT_VAULT_SEED, &[LOCK_KIND]], bump = ledger.vault_bump)]
    pub vault: Account<'info, FruitVault>,

    pub system_program: Program<'info, System>,
    // remaining_accounts: distinct veto signers, each a transaction signer,
    // at least `veto_threshold` of them.
}

/// §10 terminal resolution of a lock pool: veto quorum, after `t_end`,
/// against the Final allocation, `total <= assigned_total`, no forfeiture.
pub fn resolve_lock_pool_terminal<'info>(
    ctx: Context<'_, '_, 'info, 'info, ResolveLockPoolTerminal<'info>>,
    version: u64,
    merkle_root: [u8; 32],
    checkpoint_time: i64,
    total: u64,
) -> Result<()> {
    let fc = &ctx.accounts.fruit_config;
    require_lock_unpaused(fc)?;
    require_version(ctx.accounts.ledger.state_version)?;
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
    let mut p = load(&ctx.accounts.pool, &ctx.accounts.season)?;
    let (root_state, established) =
        terminal_lock_resolution(&mut acct, &mut p, version, checkpoint_time, total, now)
            .anchor()?;
    check_solvent(&ctx.accounts.vault.to_account_info(), &acct)?;

    let pool_key = ctx.accounts.pool.key();
    ctx.accounts.ledger.write_accounting(&acct);
    let pool = &mut ctx.accounts.pool;
    pool.write_accounting(&p);
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

#[derive(Accounts)]
pub struct ClaimLock<'info> {
    /// The claimant: signs, pays the claim record's rent, receives the
    /// payment, and is bound into the leaf and the registration PDA.
    #[account(mut)]
    pub wallet: Signer<'info>,

    #[account(
        mut,
        seeds = [FRUIT_POOL_SEED, &[pool.ledger_kind], &pool.pool_id.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(seeds = [LOCK_SEASON_SEED, pool.key().as_ref()], bump = season.bump)]
    pub season: Box<Account<'info, LockSeason>>,

    #[account(
        seeds = [LOCK_REG_SEED, pool.key().as_ref(), wallet.key().as_ref()],
        bump = registration.bump,
        constraint = registration.pool == pool.key() @ RigsError::LockRegistrationMismatch,
        constraint = registration.wallet == wallet.key() @ RigsError::LockRegistrationMismatch,
    )]
    pub registration: Box<Account<'info, LockRegistration>>,

    #[account(
        mut,
        seeds = [FRUIT_LEDGER_SEED, &[LOCK_KIND]],
        bump = ledger.bump,
        has_one = vault @ RigsError::FruitAccountMismatch,
    )]
    pub ledger: Account<'info, Ledger>,

    #[account(mut, seeds = [FRUIT_VAULT_SEED, &[LOCK_KIND]], bump = ledger.vault_bump)]
    pub vault: Account<'info, FruitVault>,

    #[account(
        init_if_needed,
        payer = wallet,
        space = 8 + ClaimRecord::INIT_SPACE,
        seeds = [FRUIT_CLAIM_SEED, pool.key().as_ref(), wallet.key().as_ref()],
        bump,
    )]
    pub claim_record: Box<Account<'info, ClaimRecord>>,

    pub system_program: Program<'info, System>,
}

/// CLAIM LOCK: cumulative, replay-protected, against the latest Final root,
/// and never above the wallet's own Final award `A_i`. Not gated by pause.
pub fn claim_lock(
    ctx: Context<ClaimLock>,
    cumulative_amount: u64,
    proof: Vec<[u8; 32]>,
) -> Result<()> {
    require_version(ctx.accounts.ledger.state_version)?;
    require_version(ctx.accounts.registration.state_version)?;
    require!(
        proof.len() <= MAX_FRUIT_PROOF_LEN,
        RigsError::FruitProofTooLong
    );
    let wallet = ctx.accounts.wallet.key();
    let pool_key = ctx.accounts.pool.key();
    let record = &mut ctx.accounts.claim_record;
    if record.state_version == 0 {
        record.state_version = FRUIT_STATE_VERSION;
        record.pool = pool_key;
        record.wallet = wallet;
        record.claimed_cum = 0;
        record.bump = ctx.bumps.claim_record;
    }
    require_version(record.state_version)?;
    require!(
        record.pool == pool_key && record.wallet == wallet,
        RigsError::FruitAccountMismatch
    );

    let pool = &ctx.accounts.pool;
    require!(pool.final_version != 0, RigsError::NoFinalRoot);
    let leaf = fruit_merkle::leaf(
        &crate::ID,
        pool.ledger_kind,
        pool.pool_id,
        &wallet,
        cumulative_amount,
    );
    require!(
        fruit_merkle::verify(&proof, &pool.final_root, leaf),
        RigsError::InvalidMerkleProof
    );

    let mut acct = ctx.accounts.ledger.accounting();
    let mut p = load(&ctx.accounts.pool, &ctx.accounts.season)?;
    let reg = ctx.accounts.registration.state();
    let mut claimed = ctx.accounts.claim_record.claimed_cum;
    let payment =
        engine_claim_lock(&mut acct, &mut p, &reg, &mut claimed, cumulative_amount).anchor()?;

    let vault_ai = ctx.accounts.vault.to_account_info();
    move_lamports(&vault_ai, &ctx.accounts.wallet.to_account_info(), payment)?;
    check_solvent(&vault_ai, &acct)?;

    ctx.accounts.ledger.write_accounting(&acct);
    ctx.accounts.pool.write_accounting(&p);
    ctx.accounts.claim_record.claimed_cum = claimed;

    emit!(FruitClaimed {
        pool: pool_key,
        wallet,
        cumulative_amount,
        payment,
        claimed_cum: claimed,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct CloseLockPool<'info> {
    #[account(seeds = [FRUIT_CONFIG_SEED], bump = fruit_config.bump)]
    pub fruit_config: Box<Account<'info, FruitConfig>>,

    #[account(
        mut,
        seeds = [FRUIT_POOL_SEED, &[pool.ledger_kind], &pool.pool_id.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(seeds = [LOCK_SEASON_SEED, pool.key().as_ref()], bump = season.bump)]
    pub season: Box<Account<'info, LockSeason>>,

    #[account(
        mut,
        seeds = [FRUIT_LEDGER_SEED, &[LOCK_KIND]],
        bump = ledger.bump,
        has_one = vault @ RigsError::FruitAccountMismatch,
    )]
    pub ledger: Account<'info, Ledger>,

    #[account(mut, seeds = [FRUIT_VAULT_SEED, &[LOCK_KIND]], bump = ledger.vault_bump)]
    pub vault: Account<'info, FruitVault>,

    /// Base config, read-only: names the Protocol Treasury.
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, Config>>,

    /// CHECK: only credited, pinned to `config.fee_recipient`, and must not be
    /// program-owned (checked in the handler).
    #[account(
        mut,
        constraint = treasury.key() == config.fee_recipient @ RigsError::FeeRecipientMismatch,
    )]
    pub treasury: UncheckedAccount<'info>,
}

/// CLOSE LOCK POOL (permissionless): after `t_end`, no Pending root, a
/// complete Final root, against the Final allocation: `U -= B - final_total;
/// Z += B - final_total` exactly once (P7-L2), then `floor(0.80 forfeited)`
/// from `Z` to the Protocol Treasury. Unallocated capacity and rounding dust
/// stay 100% in `Z`.
pub fn close_lock_pool(ctx: Context<CloseLockPool>) -> Result<()> {
    require_lock_unpaused(&ctx.accounts.fruit_config)?;
    require_version(ctx.accounts.ledger.state_version)?;
    let now = Clock::get()?.unix_timestamp;
    let mut acct = ctx.accounts.ledger.accounting();
    let mut p = load(&ctx.accounts.pool, &ctx.accounts.season)?;
    let out = close_pool(&mut acct, &mut p, now).anchor()?;
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
    ctx.accounts.pool.write_accounting(&p);

    emit!(FruitPoolClosed {
        pool: ctx.accounts.pool.key(),
        final_total: p.final_total,
        released: out.released,
    });
    if out.treasury_release > 0 {
        emit!(LockForfeitReleased {
            pool: ctx.accounts.pool.key(),
            forfeited: p.final_forfeited,
            treasury_release: out.treasury_release,
            treasury: ctx.accounts.treasury.key(),
        });
    }
    Ok(())
}
