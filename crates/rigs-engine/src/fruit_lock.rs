//! Lock Season engine — architecture §8 / §10a, Pass #7 decisions P7-L1,
//! P7-L2 and the approved rounding model
//! (`docs/tokenomics/PASS7_LOCK_DECISIONS.md`).
//!
//! Pure integer math and a write-once allocation state machine on top of
//! [`crate::fruit`]. Nothing here moves lamports or touches a
//! [`FruitLedger`]: the whole budget `B_lock` stays in `U` from commit until
//! [`crate::fruit::close_pool`] (P7-L2), which releases `B_lock - final_total`
//! exactly once, so allocation finalization cannot release capacity twice.
//!
//! Units: weights `ell` are lamports of SOL-equivalent; `u = ell * 5` and
//! `H_lock` are lamport-days.
//!
//! ```text
//! u_i    = ell_i * 5
//! H_lock = max(sum u_i, Hfloor * 5) = max(5 sum ell_i, 62.5 SOL-days)
//! sum A  = floor(B * sum u_i / H_lock)          (engine-computed, not posted)
//! A_i    = floor(B * u_i / H_lock)              (never rounded up)
//! sum_i floor(A_i) <= sum A <= B,   A_i <= floor(B/5)
//! ```
//!
//! Timing (P7-L1): registration is open on `[FRUIT_T0, t_s)` and closes at
//! exactly `t_s`. The allocation is computed from the `t_s` snapshot, so it
//! can only be posted at or after `t_s`; it becomes Final only after its
//! challenge window without a veto, and is then immutable. Until it is Final
//! the lock pool accepts no root, so all of `B_lock` stays reserved.
//!
//! Capacity: a cliff. `0` before `t_s + 5 days`, exactly `sum A` from then.
//! The earned-to-date safety-release path (linear) exists only when
//! [`SafetyRelease::Enabled`], which requires the EXTREME dwell period to be
//! explicitly set; with it unset the cliff is the only schedule.
//!
//! Error codes reuse existing [`EngineError`] variants: the on-chain program
//! maps them exhaustively, and P7-A does not modify the program.

use crate::error::{EngineError, EngineResult};
use crate::fruit::{
    post_root, scheduled_capacity, terminal_resolution, wallet_cap, FruitLedger, FruitPool,
    FruitRoot, LedgerKind, PoolStatus, FRUIT_M, P_REF_LAMPORTS, QMAX_DEN,
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

/// Aggregate allocation `sum A = floor(B * sum u / H_lock)` and its `H_lock`.
/// Computed by the engine from `sum_ell`; never taken from a poster.
pub fn lock_allocation(budget: u64, sum_ell: u64) -> EngineResult<(u64, u64)> {
    let h_lock = lock_h(sum_ell)?;
    let sum_u = sum_ell
        .checked_mul(LOCK_DAYS)
        .ok_or(EngineError::MathOverflow)?;
    let allocated = floor_share(budget, sum_u, h_lock)?;
    Ok((h_lock, allocated))
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
/// while [`SafetyRelease::Disabled`].
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
    let v = (award as u128) * (ell_surv as u128) * (elapsed as u128)
        / ((ell_i as u128) * (LOCK_DURATION_SECONDS as u128));
    u64::try_from(v).map_err(|_| EngineError::MathOverflow)
}

/// Allocation lifecycle. `Final` is terminal and immutable.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LockAllocationStatus {
    #[default]
    Unposted = 0,
    Pending = 1,
    Final = 2,
}

/// Write-once Season allocation (`H_lock`, `sum A`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockAllocation {
    pub status: LockAllocationStatus,
    /// Budget the allocation was computed against (the pool's `E`).
    pub budget: u64,
    /// `sum ell_i` at `t_s`.
    pub sum_ell: u64,
    /// Frozen `H_lock`, lamport-days.
    pub h_lock: u64,
    /// Engine-computed `sum A`.
    pub allocated: u64,
    pub posted_at: i64,
    pub final_after: i64,
    pub veto_count: u8,
}

fn require_lock_pool(pool: &FruitPool) -> EngineResult<()> {
    if pool.status == PoolStatus::Closed {
        return Err(EngineError::FruitPoolClosed);
    }
    validate_lock_window(pool.t_start, pool.t_end)
}

/// POST ALLOCATION (Pending). Only at or after `t_s` (P7-L1), with
/// `sum_ell <= requested_total` (the registered requests), no Pending
/// allocation and none Final. `H_lock` and `sum A` are computed here.
pub fn post_lock_allocation(
    pool: &FruitPool,
    alloc: &mut LockAllocation,
    sum_ell: u64,
    requested_total: u64,
    now: i64,
    challenge_window: i64,
) -> EngineResult<()> {
    require_lock_pool(pool)?;
    match alloc.status {
        LockAllocationStatus::Final => return Err(EngineError::InvalidFruitState),
        LockAllocationStatus::Pending => return Err(EngineError::RootAlreadyPending),
        LockAllocationStatus::Unposted => {}
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
    let (h_lock, allocated) = lock_allocation(pool.budget, sum_ell)?;
    *alloc = LockAllocation {
        status: LockAllocationStatus::Pending,
        budget: pool.budget,
        sum_ell,
        h_lock,
        allocated,
        posted_at: now,
        final_after,
        veto_count: 0,
    };
    Ok(())
}

/// One distinct veto signature on the Pending allocation. At `threshold` the
/// allocation returns to Unposted (it never finalizes) and may be reposted.
pub fn veto_lock_allocation(
    alloc: &mut LockAllocation,
    threshold: u8,
    now: i64,
) -> EngineResult<bool> {
    if alloc.status != LockAllocationStatus::Pending {
        return Err(EngineError::RootNotPending);
    }
    if now >= alloc.final_after {
        return Err(EngineError::ChallengeWindowClosed);
    }
    if threshold == 0 {
        return Err(EngineError::InvalidFruitGovernance);
    }
    let votes = alloc
        .veto_count
        .checked_add(1)
        .ok_or(EngineError::MathOverflow)?;
    if votes >= threshold {
        *alloc = LockAllocation::default();
        return Ok(true);
    }
    alloc.veto_count = votes;
    Ok(false)
}

/// FINALIZE ALLOCATION after its window: writes `sum A` into
/// [`FruitPool::lock_allocated`] exactly once. Moves no lamports and does not
/// touch `U` or `Z` (P7-L2).
pub fn finalize_lock_allocation(
    pool: &mut FruitPool,
    alloc: &mut LockAllocation,
    now: i64,
) -> EngineResult<()> {
    require_lock_pool(pool)?;
    if alloc.status != LockAllocationStatus::Pending {
        return Err(EngineError::RootNotPending);
    }
    if now < alloc.final_after {
        return Err(EngineError::ChallengeWindowOpen);
    }
    if pool.lock_allocated != 0
        || pool.latest_version != 0
        || alloc.budget != pool.budget
        || alloc.allocated > pool.budget
    {
        return Err(EngineError::InvalidFruitState);
    }
    pool.lock_allocated = alloc.allocated;
    alloc.status = LockAllocationStatus::Final;
    Ok(())
}

fn require_final_allocation(pool: &FruitPool, alloc: &LockAllocation) -> EngineResult<()> {
    if alloc.status != LockAllocationStatus::Final
        || alloc.budget != pool.budget
        || alloc.allocated != pool.lock_allocated
        || alloc.allocated > pool.budget
    {
        return Err(EngineError::InvalidFruitState);
    }
    Ok(())
}

/// Lock cliff capacity at `checkpoint_time` for a Final allocation: `0`
/// before `t_end`, exactly `sum A` from `t_end`. With safety release enabled,
/// the pre-maturity bound is `floor(sum A * elapsed / D)` (earned-to-date).
pub fn lock_capacity(
    pool: &FruitPool,
    alloc: &LockAllocation,
    checkpoint_time: i64,
    safety: SafetyRelease,
) -> EngineResult<u64> {
    validate_lock_window(pool.t_start, pool.t_end)?;
    require_final_allocation(pool, alloc)?;
    if checkpoint_time >= pool.t_end {
        return Ok(alloc.allocated);
    }
    match safety {
        SafetyRelease::Disabled => Ok(0),
        SafetyRelease::Enabled { .. } => {
            scheduled_capacity(alloc.allocated, pool.t_start, pool.t_end, checkpoint_time)
        }
    }
}

/// Root capacity by ledger kind. Core and loyalty use the §10a linear
/// schedule (and take no allocation); lock uses [`lock_capacity`] and
/// requires a Final allocation.
pub fn root_capacity(
    kind: LedgerKind,
    pool: &FruitPool,
    alloc: Option<&LockAllocation>,
    checkpoint_time: i64,
    safety: SafetyRelease,
) -> EngineResult<u64> {
    match (kind, alloc) {
        (LedgerKind::Core | LedgerKind::Loyalty, None) => {
            scheduled_capacity(pool.budget, pool.t_start, pool.t_end, checkpoint_time)
        }
        (LedgerKind::Lock, Some(a)) => lock_capacity(pool, a, checkpoint_time, safety),
        (LedgerKind::Lock, None) => Err(EngineError::InvalidFruitState),
        (_, Some(_)) => Err(EngineError::InvalidLedgerKind),
    }
}

/// POST ROOT on a lock pool: the lock cliff capacity, then every generic
/// [`post_root`] check (version, pending, monotone finality, completeness,
/// `forfeited <= sum A - total`). The cliff is never above the generic
/// linear bound, so both hold.
#[allow(clippy::too_many_arguments)]
pub fn post_lock_root(
    pool: &mut FruitPool,
    alloc: &LockAllocation,
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
    if total > lock_capacity(pool, alloc, checkpoint_time, safety)? {
        return Err(EngineError::RootExceedsCapacity);
    }
    post_root(
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

/// TERMINAL RESOLUTION on a lock pool: requires a Final allocation and caps
/// `total` at `sum A`, then the generic [`terminal_resolution`]. The generic
/// path alone would allow up to `E`, which exceeds `sum A`.
pub fn terminal_lock_resolution(
    ledger: &mut FruitLedger,
    pool: &mut FruitPool,
    alloc: &LockAllocation,
    version: u64,
    checkpoint_time: i64,
    total: u64,
    now: i64,
) -> EngineResult<(FruitRoot, u64)> {
    require_lock_pool(pool)?;
    if total > lock_capacity(pool, alloc, checkpoint_time, SafetyRelease::Disabled)? {
        return Err(EngineError::RootExceedsCapacity);
    }
    terminal_resolution(ledger, pool, version, checkpoint_time, total, now)
}
