//! Lock Season engine — architecture §8 / §10a, Pass #7 decisions P7-L1,
//! P7-L2, F7 and the approved rounding model
//! (`docs/tokenomics/PASS7_LOCK_DECISIONS.md`).
//!
//! Pure integer math and a write-once allocation state machine that lives on
//! the [`FruitPool`] itself ([`FruitPool::lock`]). Nothing here moves
//! lamports: the whole budget `B_lock` stays in `U` from commit until
//! [`crate::fruit::close_pool`] (P7-L2), which releases `B_lock - final_total`
//! exactly once, so allocation finalization cannot release capacity twice.
//!
//! Units: weights `ell` are lamports of SOL-equivalent; `u = ell * 5` and
//! `H_lock` are lamport-days.
//!
//! ```text
//! u_i               = ell_i * 5
//! H_lock            = max(sum u_i, Hfloor * 5) = max(5 sum ell_i, 62.5 SOL-days)
//! gross_allocatable = floor(B * sum u_i / H_lock)      (engine-computed)
//! A_i               = floor(B * u_i / H_lock)          (never rounded up)
//! assigned_total    = sum_i A_i                        (committed, challenged)
//! rounding_dust     = gross_allocatable - assigned_total
//! unallocated       = B - assigned_total               (dust included; 100% to Z)
//! forfeited        <= assigned_total - final_total
//! A_i <= floor(B/5)
//! ```
//!
//! The engine only sees `sum ell`, so it cannot recompute `assigned_total`;
//! the poster commits it with the allocation, bounded by `gross_allocatable`,
//! and it goes through the same challenge/veto window as `H_lock` (F7).
//!
//! Timing (P7-L1): registration is open on `[FRUIT_T0, t_s)` and closes at
//! exactly `t_s`. The allocation is computed from the `t_s` snapshot, so it
//! can only be posted at or after `t_s`; it becomes Final only after its
//! challenge window without a veto, and is then immutable — a Final zero
//! allocation included. Until it is Final the lock pool accepts no root, so
//! all of `B_lock` stays reserved.
//!
//! Lifecycle (pool-owned, [`LockAllocationStatus`]):
//!
//! ```text
//! Uninitialized --post--> Pending(generation g) --finalize--> Final (terminal)
//!                            |
//!                            +--veto at threshold--> Uninitialized (g retired)
//! ```
//!
//! Every post takes a fresh generation, and veto and finalize act only on the
//! pool's own Pending allocation and must be handed exactly that commitment,
//! so a stale or copied allocation (or one from another pool) cannot
//! finalize.
//!
//! Capacity: a cliff. `0` before `t_s + 5 days`, exactly `assigned_total`
//! from then. The earned-to-date safety-release path (linear) exists only
//! when [`SafetyRelease::Enabled`], which requires the EXTREME dwell period to
//! be explicitly set; with it unset the cliff is the only schedule.
//!
//! Error codes reuse existing [`EngineError`] variants: the on-chain program
//! maps them exhaustively and does not expose lock instructions yet.

use crate::error::{EngineError, EngineResult};
use crate::fruit::{
    commit_pool, post_root_unchecked, scheduled_capacity, terminal_resolution_unchecked,
    wallet_cap, FruitLedger, FruitPool, FruitRoot, LedgerKind, PoolStatus, FRUIT_M, P_REF_LAMPORTS,
    QMAX_DEN,
};
use crate::fruit_schedule::DAY_SECONDS;

/// The only lock duration, in days (`D_lock = 5`).
pub const LOCK_DAYS: u64 = 5;
/// `D_lock` in seconds: 432,000.
pub const LOCK_DURATION_SECONDS: i64 = LOCK_DAYS as i64 * DAY_SECONDS;
/// Season start offset from `FRUIT_T0`: `t_s = FRUIT_T0 + 6 days`.
pub const LOCK_SEASON_OFFSET: i64 = 6 * DAY_SECONDS;
/// `Kmax = P_ref * M` = 2.5 SOL-equivalent, the largest `ell_i`.
pub const LOCK_KMAX_LAMPORTS: u64 = P_REF_LAMPORTS * FRUIT_M;
/// `Hfloor = Kmax / qmax` = 12.5 SOL-equivalent.
pub const LOCK_HFLOOR_LAMPORTS: u64 = LOCK_KMAX_LAMPORTS * QMAX_DEN;
/// `Hfloor * 5 days` = 62.5 SOL-days, in lamport-days.
pub const LOCK_H_FLOOR_LAMPORT_DAYS: u64 = LOCK_HFLOOR_LAMPORTS * LOCK_DAYS;

const _: () = assert!(LOCK_KMAX_LAMPORTS == 2_500_000_000);
const _: () = assert!(LOCK_HFLOOR_LAMPORTS == 12_500_000_000);
const _: () = assert!(LOCK_H_FLOOR_LAMPORT_DAYS == 62_500_000_000);
const _: () = assert!(LOCK_DURATION_SECONDS == 432_000);

/// `t_s` for a given public `FRUIT_T0`.
pub fn lock_season_start(fruit_t0: i64) -> EngineResult<i64> {
    fruit_t0
        .checked_add(LOCK_SEASON_OFFSET)
        .ok_or(EngineError::MathOverflow)
}

/// Registration window `[FRUIT_T0, t_s)` (P7-L1). A boundary equality belongs
/// to the later interval, so `now == t_s` is closed.
pub fn lock_registration_open(fruit_t0: i64, t_s: i64, now: i64) -> bool {
    fruit_t0 <= now && now < t_s
}

/// A lock pool's window is exactly `[t_s, t_s + 5 days)`.
pub fn validate_lock_window(t_start: i64, t_end: i64) -> EngineResult<()> {
    if (t_end as i128) - (t_start as i128) != LOCK_DURATION_SECONDS as i128 {
        return Err(EngineError::InvalidPoolWindow);
    }
    Ok(())
}

/// `H_lock = max(5 * sum_ell, 62.5 SOL-days)` in lamport-days.
pub fn lock_h(sum_ell: u64) -> EngineResult<u64> {
    let sum_u = sum_ell
        .checked_mul(LOCK_DAYS)
        .ok_or(EngineError::MathOverflow)?;
    Ok(sum_u.max(LOCK_H_FLOOR_LAMPORT_DAYS))
}

/// `floor(budget * u / h_lock)` with a u128 intermediate. Requires
/// `u <= h_lock`, so the result never exceeds `budget`.
fn floor_share(budget: u64, u: u64, h_lock: u64) -> EngineResult<u64> {
    if h_lock < LOCK_H_FLOOR_LAMPORT_DAYS || u > h_lock {
        return Err(EngineError::InvalidFruitState);
    }
    let v = (budget as u128)
        .checked_mul(u as u128)
        .ok_or(EngineError::MathOverflow)?
        / h_lock as u128;
    u64::try_from(v).map_err(|_| EngineError::MathOverflow)
}

/// `(H_lock, gross_allocatable)` with
/// `gross_allocatable = floor(B * sum u / H_lock)`. Computed by the engine
/// from `sum_ell`; never taken from a poster. It is an upper bound for the
/// sum of wallet awards, not that sum (F7).
pub fn lock_allocation(budget: u64, sum_ell: u64) -> EngineResult<(u64, u64)> {
    let h_lock = lock_h(sum_ell)?;
    let sum_u = sum_ell
        .checked_mul(LOCK_DAYS)
        .ok_or(EngineError::MathOverflow)?;
    let gross = floor_share(budget, sum_u, h_lock)?;
    Ok((h_lock, gross))
}

/// One wallet's award `A_i = floor(B * u_i / H_lock)`, `0 < ell_i <= Kmax`.
/// Never above `floor(B/5)` (checked, not only proved).
pub fn lock_award(budget: u64, ell_i: u64, h_lock: u64) -> EngineResult<u64> {
    if ell_i == 0 || ell_i > LOCK_KMAX_LAMPORTS {
        return Err(EngineError::InvalidFruitState);
    }
    let u_i = ell_i * LOCK_DAYS; // <= 12.5e9, cannot overflow
    let a = floor_share(budget, u_i, h_lock)?;
    if a > wallet_cap(budget) {
        return Err(EngineError::LeafAboveWalletCap);
    }
    Ok(a)
}

/// Vested amount at ordinary maturity for surviving units:
/// `floor(A_i * ell_surv / ell_i)`, `ell_surv <= ell_i`.
pub fn lock_vest_at_maturity(award: u64, ell_surv: u64, ell_i: u64) -> EngineResult<u64> {
    if ell_i == 0 || ell_surv > ell_i {
        return Err(EngineError::InvalidFruitState);
    }
    let v = (award as u128) * (ell_surv as u128) / (ell_i as u128);
    u64::try_from(v).map_err(|_| EngineError::MathOverflow)
}

/// Whether the EXTREME safety-release path is available. It is enabled only
/// by an explicitly set, positive EXTREME minimum-dwell period (§9); unset or
/// non-positive fails closed to [`SafetyRelease::Disabled`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafetyRelease {
    Disabled,
    Enabled { extreme_dwell_seconds: i64 },
}

impl SafetyRelease {
    pub fn from_dwell(extreme_dwell_seconds: Option<i64>) -> Self {
        match extreme_dwell_seconds {
            Some(d) if d > 0 => SafetyRelease::Enabled {
                extreme_dwell_seconds: d,
            },
            _ => SafetyRelease::Disabled,
        }
    }
}

/// Safety-release vest of earned-to-date progress at safety time `t_event`:
/// `floor(A_i * ell_surv / ell_i * min(t_event - t_s, D) / D)`, with
/// `ell_surv` the last valid surviving weight before the event. Rejected
/// while [`SafetyRelease::Disabled`]. Checked arithmetic throughout.
pub fn lock_safety_vest(
    safety: SafetyRelease,
    award: u64,
    ell_surv: u64,
    ell_i: u64,
    t_s: i64,
    t_event: i64,
) -> EngineResult<u64> {
    if safety == SafetyRelease::Disabled {
        return Err(EngineError::InvalidConfig);
    }
    if ell_i == 0 || ell_surv > ell_i {
        return Err(EngineError::InvalidFruitState);
    }
    let elapsed = ((t_event as i128) - (t_s as i128)).clamp(0, LOCK_DURATION_SECONDS as i128);
    let num = (award as u128)
        .checked_mul(ell_surv as u128)
        .and_then(|x| x.checked_mul(elapsed as u128))
        .ok_or(EngineError::MathOverflow)?;
    let den = (ell_i as u128)
        .checked_mul(LOCK_DURATION_SECONDS as u128)
        .ok_or(EngineError::MathOverflow)?;
    u64::try_from(num / den).map_err(|_| EngineError::MathOverflow)
}

/// Pool-owned allocation lifecycle. `Final` is terminal and immutable.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LockAllocationStatus {
    #[default]
    Uninitialized = 0,
    Pending = 1,
    Final = 2,
}

/// One posted Season allocation: the whole commitment that is challenged,
/// vetoed or finalized. Veto and finalize must be handed exactly the pool's
/// current Pending commitment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockAllocation {
    /// The pool this allocation was posted on ([`LockState::pool_id`]).
    pub pool_id: u64,
    /// Post counter on that pool; a vetoed generation is never reused.
    pub generation: u64,
    /// Budget `B_lock` it was computed against (the pool's `E`).
    pub budget: u64,
    /// `sum ell_i` at `t_s`.
    pub sum_ell: u64,
    /// Registered requested weight; `sum_ell <= requested_total`.
    pub requested_total: u64,
    /// Frozen `H_lock`, lamport-days.
    pub h_lock: u64,
    /// Engine-computed `floor(B * sum u / H_lock)`.
    pub gross_allocatable: u64,
    /// Committed `sum_i floor(B * u_i / H_lock)`, `<= gross_allocatable`.
    pub assigned_total: u64,
    pub posted_at: i64,
    pub final_after: i64,
}

impl LockAllocation {
    /// `gross_allocatable - assigned_total`: never assigned to a wallet.
    pub fn rounding_dust(&self) -> u64 {
        self.gross_allocatable.saturating_sub(self.assigned_total)
    }

    /// `B - assigned_total` (rounding dust included): returns 100% to `Z`.
    pub fn unallocated(&self) -> u64 {
        self.budget.saturating_sub(self.assigned_total)
    }
}

/// Lock state carried by every [`FruitPool`]; meaningful only on a
/// [`LedgerKind::Lock`] pool.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockState {
    /// Pool identity fixed at [`commit_lock_pool`].
    pub pool_id: u64,
    pub status: LockAllocationStatus,
    /// Last generation issued; never decreases.
    pub generation: u64,
    /// Distinct veto signatures on the Pending allocation.
    pub veto_count: u8,
    /// The Pending or Final allocation; default while Uninitialized.
    pub allocation: LockAllocation,
}

/// COMMIT the Season's lock pool: `[t_s, t_s + 5 days)` with
/// `t_s = FRUIT_T0 + 6 days`, before `t_s`. The only way to make a
/// [`LedgerKind::Lock`] pool.
pub fn commit_lock_pool(
    ledger: &mut FruitLedger,
    pool_id: u64,
    budget: u64,
    fruit_t0: i64,
    now: i64,
) -> EngineResult<FruitPool> {
    let t_s = lock_season_start(fruit_t0)?;
    let t_end = t_s
        .checked_add(LOCK_DURATION_SECONDS)
        .ok_or(EngineError::MathOverflow)?;
    let mut next = *ledger;
    let mut pool = commit_pool(&mut next, budget, t_s, t_end, now)?;
    pool.kind = LedgerKind::Lock;
    pool.lock = LockState {
        pool_id,
        ..LockState::default()
    };
    *ledger = next;
    Ok(pool)
}

fn require_lock_pool(pool: &FruitPool) -> EngineResult<()> {
    if pool.kind != LedgerKind::Lock {
        return Err(EngineError::InvalidLedgerKind);
    }
    if pool.status == PoolStatus::Closed {
        return Err(EngineError::FruitPoolClosed);
    }
    validate_lock_window(pool.t_start, pool.t_end)
}

/// Every committed field of `a` against the pool: identity, generation,
/// budget, recomputed `H_lock` / `gross_allocatable`, and
/// `assigned_total <= gross_allocatable <= budget`.
fn validate_allocation(pool: &FruitPool, a: &LockAllocation) -> EngineResult<()> {
    if a.pool_id != pool.lock.pool_id
        || a.generation != pool.lock.generation
        || a.generation == 0
        || a.budget != pool.budget
        || a.sum_ell > a.requested_total
    {
        return Err(EngineError::InvalidFruitState);
    }
    let (h_lock, gross) = lock_allocation(a.budget, a.sum_ell)?;
    if a.h_lock != h_lock
        || a.gross_allocatable != gross
        || a.assigned_total > a.gross_allocatable
        || a.gross_allocatable > a.budget
    {
        return Err(EngineError::InvalidFruitState);
    }
    Ok(())
}

/// The pool's Final allocation, fully revalidated. The only source of lock
/// capacity and of the forfeiture bound; a caller-built allocation object
/// cannot stand in for it.
pub fn lock_final_allocation(pool: &FruitPool) -> EngineResult<LockAllocation> {
    if pool.kind != LedgerKind::Lock || pool.lock.status != LockAllocationStatus::Final {
        return Err(EngineError::InvalidFruitState);
    }
    let a = pool.lock.allocation;
    validate_allocation(pool, &a)?;
    Ok(a)
}

/// POST ALLOCATION (Pending) on the pool. Only at or after `t_s` (P7-L1),
/// only while Uninitialized and before any root, with
/// `sum_ell <= requested_total` and
/// `assigned_total <= gross_allocatable`. `H_lock` and `gross_allocatable`
/// are computed here; `assigned_total` is the poster's commitment (F7),
/// challenged with the rest. Returns the commitment.
pub fn post_lock_allocation(
    pool: &mut FruitPool,
    sum_ell: u64,
    requested_total: u64,
    assigned_total: u64,
    now: i64,
    challenge_window: i64,
) -> EngineResult<LockAllocation> {
    require_lock_pool(pool)?;
    match pool.lock.status {
        LockAllocationStatus::Final => return Err(EngineError::InvalidFruitState),
        LockAllocationStatus::Pending => return Err(EngineError::RootAlreadyPending),
        LockAllocationStatus::Uninitialized => {}
    }
    if pool.latest_version != 0 {
        return Err(EngineError::InvalidFruitState);
    }
    if now < pool.t_start {
        return Err(EngineError::FutureCheckpoint);
    }
    if sum_ell > requested_total {
        return Err(EngineError::RootExceedsCapacity);
    }
    if challenge_window <= 0 {
        return Err(EngineError::InvalidFruitGovernance);
    }
    let final_after = now
        .checked_add(challenge_window)
        .ok_or(EngineError::MathOverflow)?;
    let (h_lock, gross_allocatable) = lock_allocation(pool.budget, sum_ell)?;
    if assigned_total > gross_allocatable {
        return Err(EngineError::RootExceedsCapacity);
    }
    let generation = pool
        .lock
        .generation
        .checked_add(1)
        .ok_or(EngineError::MathOverflow)?;
    let allocation = LockAllocation {
        pool_id: pool.lock.pool_id,
        generation,
        budget: pool.budget,
        sum_ell,
        requested_total,
        h_lock,
        gross_allocatable,
        assigned_total,
        posted_at: now,
        final_after,
    };
    pool.lock = LockState {
        pool_id: pool.lock.pool_id,
        status: LockAllocationStatus::Pending,
        generation,
        veto_count: 0,
        allocation,
    };
    Ok(allocation)
}

fn require_pending(pool: &FruitPool, expected: &LockAllocation) -> EngineResult<()> {
    if pool.lock.status != LockAllocationStatus::Pending || pool.lock.allocation != *expected {
        return Err(EngineError::RootNotPending);
    }
    Ok(())
}

/// One distinct veto signature on the pool's Pending allocation `expected`.
/// At `threshold` the allocation is discarded (it never finalizes), the pool
/// returns to Uninitialized and the generation is retired.
pub fn veto_lock_allocation(
    pool: &mut FruitPool,
    expected: &LockAllocation,
    threshold: u8,
    now: i64,
) -> EngineResult<bool> {
    require_lock_pool(pool)?;
    require_pending(pool, expected)?;
    if now >= pool.lock.allocation.final_after {
        return Err(EngineError::ChallengeWindowClosed);
    }
    if threshold == 0 {
        return Err(EngineError::InvalidFruitGovernance);
    }
    let votes = pool
        .lock
        .veto_count
        .checked_add(1)
        .ok_or(EngineError::MathOverflow)?;
    if votes >= threshold {
        pool.lock = LockState {
            pool_id: pool.lock.pool_id,
            generation: pool.lock.generation,
            ..LockState::default()
        };
        return Ok(true);
    }
    pool.lock.veto_count = votes;
    Ok(false)
}

/// FINALIZE ALLOCATION after its window: the pool's Pending allocation must
/// equal `expected` and revalidate in full; it then becomes Final, for good.
/// Moves no lamports and does not touch `U` or `Z` (P7-L2).
pub fn finalize_lock_allocation(
    pool: &mut FruitPool,
    expected: &LockAllocation,
    now: i64,
) -> EngineResult<LockAllocation> {
    require_lock_pool(pool)?;
    require_pending(pool, expected)?;
    if now < pool.lock.allocation.final_after {
        return Err(EngineError::ChallengeWindowOpen);
    }
    if pool.latest_version != 0 {
        return Err(EngineError::InvalidFruitState);
    }
    validate_allocation(pool, &pool.lock.allocation)?;
    pool.lock.status = LockAllocationStatus::Final;
    Ok(pool.lock.allocation)
}

/// Lock cliff capacity at `checkpoint_time` from the pool's Final
/// allocation: `0` before `t_end`, exactly `assigned_total` from `t_end`.
/// With safety release enabled, the pre-maturity bound is
/// `floor(assigned_total * elapsed / D)` (earned-to-date).
pub fn lock_capacity(
    pool: &FruitPool,
    checkpoint_time: i64,
    safety: SafetyRelease,
) -> EngineResult<u64> {
    validate_lock_window(pool.t_start, pool.t_end)?;
    let a = lock_final_allocation(pool)?;
    if checkpoint_time >= pool.t_end {
        return Ok(a.assigned_total);
    }
    match safety {
        SafetyRelease::Disabled => Ok(0),
        SafetyRelease::Enabled { .. } => {
            scheduled_capacity(a.assigned_total, pool.t_start, pool.t_end, checkpoint_time)
        }
    }
}

/// Root capacity by the pool's ledger kind. Core and loyalty use the §10a
/// linear schedule; lock uses [`lock_capacity`].
pub fn root_capacity(
    pool: &FruitPool,
    checkpoint_time: i64,
    safety: SafetyRelease,
) -> EngineResult<u64> {
    match pool.kind {
        LedgerKind::Core | LedgerKind::Loyalty => {
            scheduled_capacity(pool.budget, pool.t_start, pool.t_end, checkpoint_time)
        }
        LedgerKind::Lock => lock_capacity(pool, checkpoint_time, safety),
    }
}

/// POST ROOT on a lock pool: the lock cliff capacity, then every generic
/// root check (version, pending, monotone finality, completeness,
/// `forfeited <= assigned_total - total`). The cliff is never above the
/// generic linear bound, so both hold.
#[allow(clippy::too_many_arguments)]
pub fn post_lock_root(
    pool: &mut FruitPool,
    version: u64,
    checkpoint_time: i64,
    total: u64,
    complete: bool,
    forfeited: u64,
    now: i64,
    challenge_window: i64,
    safety: SafetyRelease,
) -> EngineResult<FruitRoot> {
    require_lock_pool(pool)?;
    if checkpoint_time > now {
        return Err(EngineError::FutureCheckpoint);
    }
    if total > lock_capacity(pool, checkpoint_time, safety)? {
        return Err(EngineError::RootExceedsCapacity);
    }
    post_root_unchecked(
        pool,
        version,
        checkpoint_time,
        total,
        complete,
        forfeited,
        now,
        challenge_window,
    )
}

/// TERMINAL RESOLUTION on a lock pool: requires the pool's Final allocation
/// and caps `total` at `assigned_total`, then the generic terminal checks.
/// The generic [`crate::fruit::terminal_resolution`] refuses lock pools.
pub fn terminal_lock_resolution(
    ledger: &mut FruitLedger,
    pool: &mut FruitPool,
    version: u64,
    checkpoint_time: i64,
    total: u64,
    now: i64,
) -> EngineResult<(FruitRoot, u64)> {
    require_lock_pool(pool)?;
    if total > lock_capacity(pool, checkpoint_time, SafetyRelease::Disabled)? {
        return Err(EngineError::RootExceedsCapacity);
    }
    terminal_resolution_unchecked(ledger, pool, version, checkpoint_time, total, now)
}
