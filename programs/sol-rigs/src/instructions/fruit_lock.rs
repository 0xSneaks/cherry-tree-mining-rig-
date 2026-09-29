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
//! per generation. Lock roots, claims, terminal resolution and close are
//! wired in P7-D; until then the generic paths keep refusing lock pools.

use anchor_lang::prelude::*;
use rigs_engine::{
    award_lock_registration, cancel_lock, commit_lock_pool as engine_commit_lock_pool,
    finalize_lock_allocation as engine_finalize_lock_allocation, fruit_t0, post_lock_weight,
    register_lock as engine_register_lock, seal_lock_awards as engine_seal_lock_awards,
    seal_lock_weights as engine_seal_lock_weights,
    veto_lock_allocation as engine_veto_lock_allocation, FruitPool, LedgerKind,
    LockRegistrationState, DAY_SECONDS,
};

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::*;
use crate::fruit_state::{
    require_version, FruitConfig, FruitVault, Ledger, LockRegistration, LockSeason, Pool,
};
use crate::instructions::fruit_ledger::check_solvent;
use crate::state::Config;

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
