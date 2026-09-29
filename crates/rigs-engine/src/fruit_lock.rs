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
//! assigned_total    = sum_i A_i                        (engine-accumulated)
//! rounding_dust     = gross_allocatable - assigned_total
//! unallocated       = B - assigned_total               (dust included; 100% to Z)
//! forfeited        <= assigned_total - final_total
//! A_i <= floor(B/5)
//! ```
//!
//! Pass #7 P7-B (closes V-1 / V-2 by construction): `assigned_total` is never
//! an input. Each active registration is weighed once per generation (the
//! poster posts `ell_i <= requested_i`), weights are sealed only when every
//! active registration is weighed, and then each award `A_i` is computed by
//! the engine and added exactly once. `assigned_total` is therefore exactly
//! `sum_i floor(A_i)`, dust is exactly `gross_allocatable - assigned_total`,
//! and no poster value can move dust into forfeiture.
//!
//! Timing (P7-L1, D-B1): registration and cancellation are open on
//! `[FRUIT_T0, t_s)`; at `t_s` the active registration set is frozen.
//! Weights are posted only at or after `t_s`. The allocation becomes Final
//! only after its challenge window without a veto, and is then immutable — a
//! Final zero allocation included. Until it is Final the lock pool accepts no
//! root, so all of `B_lock` stays reserved.
//!
//! Lifecycle (pool-owned, [`LockAllocationStatus`]): see that type. A veto
//! retires the generation; the re-tally runs over the same frozen active set.
//!
//! Capacity: a cliff. `0` before `t_s + 5 days`, exactly `assigned_total`
//! from then. The earned-to-date safety-release path (linear) exists only
//! when [`SafetyRelease::Enabled`], which requires the EXTREME dwell period to
//! be explicitly set; with it unset the cliff is the only schedule.

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
        return Err(EngineError::InvalidLockWeight);
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
        return Err(EngineError::InvalidLockWeight);
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
        return Err(EngineError::InvalidLockWeight);
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
        return Err(EngineError::LockSafetyReleaseDisabled);
    }
    if ell_i == 0 || ell_surv > ell_i {
        return Err(EngineError::InvalidLockWeight);
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

/// Pool-owned allocation lifecycle (Pass #7 P7-B). `Final` is terminal and
/// immutable.
///
/// ```text
/// Registering --t_s--> Weighing(g) --seal--> Awarding(g) --seal--> Pending(g) --window--> Final
///                         ^                                            |
///                         +---------- veto at threshold: g+1 ----------+
/// ```
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LockAllocationStatus {
    /// Registration window `[FRUIT_T0, t_s)`; no weight yet.
    #[default]
    Registering = 0,
    /// Weights of generation `g` are being posted (at or after `t_s`).
    Weighing = 1,
    /// `H_lock` and `gross_allocatable` are sealed; awards are computed.
    Awarding = 2,
    /// Every active registration is awarded; challenge window open.
    Pending = 3,
    /// Immutable.
    Final = 4,
}

impl LockAllocationStatus {
    pub fn from_u8(v: u8) -> EngineResult<Self> {
        match v {
            0 => Ok(LockAllocationStatus::Registering),
            1 => Ok(LockAllocationStatus::Weighing),
            2 => Ok(LockAllocationStatus::Awarding),
            3 => Ok(LockAllocationStatus::Pending),
            4 => Ok(LockAllocationStatus::Final),
            _ => Err(EngineError::LockTallyInvalid),
        }
    }
}

/// Lock Season state carried by every [`FruitPool`]; meaningful only on a
/// [`LedgerKind::Lock`] pool. The program persists it in the pool's
/// `LockSeason` account.
///
/// Registration counters change only inside `[FRUIT_T0, t_s)` and are frozen
/// from `t_s`; every tally generation runs over that same frozen active set.
/// `assigned_total` is never an input: it is the sum of the engine-computed
/// awards, each added exactly once per generation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockState {
    pub status: LockAllocationStatus,
    /// `B_lock` fixed at commit; must equal the pool's `E` on every read.
    pub budget: u64,
    /// Current tally generation, `>= 1`; a veto retires it (`g + 1`).
    pub generation: u64,
    /// Registrations ever made (cancelled included). Never decreases.
    pub total_reg_count: u64,
    /// Registrations not cancelled.
    pub active_reg_count: u64,
    /// `sum requested_i` over active registrations.
    pub active_requested_total: u64,
    /// Active registrations weighed in this generation.
    pub weighed_count: u64,
    /// Of those, the ones with `ell_i > 0`.
    pub positive_count: u64,
    /// `sum ell_i` of this generation.
    pub sum_ell: u64,
    /// `H_lock`, lamport-days; set when weights are sealed.
    pub h_lock: u64,
    /// `floor(B * sum u / H_lock)`; set when weights are sealed.
    pub gross_allocatable: u64,
    /// Active registrations awarded in this generation.
    pub awarded_count: u64,
    /// `sum_i floor(B * u_i / H_lock)`, accumulated by the engine.
    pub assigned_total: u64,
    pub posted_at: i64,
    pub final_after: i64,
    /// Distinct veto signatures on the Pending allocation.
    pub veto_count: u8,
}

/// One wallet's registration (the program's `LockRegistration` account).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockRegistrationState {
    /// Requested weight, `0 < requested <= Kmax`. Immutable.
    pub requested: u64,
    /// Cancelled before `t_s`; permanent.
    pub cancelled: bool,
    /// `ell_i` posted for generation `weight_gen`.
    pub ell: u64,
    /// Generation `ell` belongs to; `0` = never weighed.
    pub weight_gen: u64,
    /// `A_i` computed for generation `award_gen`.
    pub award: u64,
    /// Generation `award` belongs to; `0` = never awarded.
    pub award_gen: u64,
}

impl LockRegistrationState {
    /// The award of the Final generation `generation`, or 0 if this
    /// registration holds none (cancelled, or stale).
    pub fn award_for(&self, generation: u64) -> u64 {
        if !self.cancelled && self.award_gen == generation && self.weight_gen == generation {
            self.award
        } else {
            0
        }
    }
}

/// Read-only summary of a Final (or sealed) allocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LockAllocation {
    pub generation: u64,
    /// `B_lock` (the pool's `E`).
    pub budget: u64,
    pub active_reg_count: u64,
    pub positive_count: u64,
    pub active_requested_total: u64,
    pub sum_ell: u64,
    pub h_lock: u64,
    pub gross_allocatable: u64,
    pub assigned_total: u64,
}

impl LockAllocation {
    /// `gross_allocatable - assigned_total`: never assigned to a wallet.
    /// Exact (the tally guarantees `assigned_total <= gross_allocatable`).
    pub fn rounding_dust(&self) -> u64 {
        self.gross_allocatable - self.assigned_total
    }

    /// `B - assigned_total` (rounding dust included): returns 100% to `Z`.
    pub fn unallocated(&self) -> u64 {
        self.budget - self.assigned_total
    }
}

/// COMMIT the Season's lock pool: `[t_s, t_s + 5 days)` with
/// `t_s = FRUIT_T0 + 6 days`, before `t_s`. The only way to make a
/// [`LedgerKind::Lock`] pool; its tally starts at generation 1.
pub fn commit_lock_pool(
    ledger: &mut FruitLedger,
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
        budget,
        generation: 1,
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

/// `[FRUIT_T0, t_s)` of this pool (`FRUIT_T0 = t_s - 6 days`).
fn require_registration_open(pool: &FruitPool, now: i64) -> EngineResult<()> {
    let t_s = pool.t_start;
    let fruit_t0 = t_s
        .checked_sub(LOCK_SEASON_OFFSET)
        .ok_or(EngineError::MathOverflow)?;
    if !lock_registration_open(fruit_t0, t_s, now) {
        return Err(EngineError::LockRegistrationClosed);
    }
    if pool.lock.status != LockAllocationStatus::Registering {
        return Err(EngineError::LockAllocationPhase);
    }
    Ok(())
}

fn add(a: u64, b: u64) -> EngineResult<u64> {
    a.checked_add(b).ok_or(EngineError::MathOverflow)
}

/// REGISTER: one wallet's intent, inside `[FRUIT_T0, t_s)`, with
/// `0 < requested <= Kmax`. Returns the new registration. The caller
/// guarantees one registration per wallet per Season (the program's `init`).
pub fn register_lock(
    pool: &mut FruitPool,
    requested: u64,
    now: i64,
) -> EngineResult<LockRegistrationState> {
    require_lock_pool(pool)?;
    require_registration_open(pool, now)?;
    if requested == 0 || requested > LOCK_KMAX_LAMPORTS {
        return Err(EngineError::InvalidLockRequest);
    }
    let mut s = pool.lock;
    s.total_reg_count = add(s.total_reg_count, 1)?;
    s.active_reg_count = add(s.active_reg_count, 1)?;
    s.active_requested_total = add(s.active_requested_total, requested)?;
    pool.lock = s;
    Ok(LockRegistrationState {
        requested,
        ..LockRegistrationState::default()
    })
}

/// CANCEL (D-B1): inside `[FRUIT_T0, t_s)` only, once. Marks the
/// registration cancelled and removes it from the active count and the
/// active requested total, atomically. The registration is kept, so the
/// wallet can never register again for this Season.
pub fn cancel_lock(
    pool: &mut FruitPool,
    reg: &mut LockRegistrationState,
    now: i64,
) -> EngineResult<()> {
    require_lock_pool(pool)?;
    require_registration_open(pool, now)?;
    if reg.cancelled {
        return Err(EngineError::LockRegistrationCancelled);
    }
    if reg.requested == 0 || reg.requested > LOCK_KMAX_LAMPORTS {
        return Err(EngineError::LockTallyInvalid);
    }
    let mut s = pool.lock;
    s.active_reg_count = s
        .active_reg_count
        .checked_sub(1)
        .ok_or(EngineError::LockTallyInvalid)?;
    s.active_requested_total = s
        .active_requested_total
        .checked_sub(reg.requested)
        .ok_or(EngineError::LockTallyInvalid)?;
    pool.lock = s;
    reg.cancelled = true;
    Ok(())
}

/// POST WEIGHT (poster): `ell_i = min(requested_i, w_i(t_s))` for one active
/// registration, at or after `t_s`, once per generation, `ell_i <= requested_i`.
pub fn post_lock_weight(
    pool: &mut FruitPool,
    reg: &mut LockRegistrationState,
    ell: u64,
    now: i64,
) -> EngineResult<()> {
    require_lock_pool(pool)?;
    if now < pool.t_start {
        return Err(EngineError::LockSeasonNotStarted);
    }
    let mut s = pool.lock;
    if !matches!(
        s.status,
        LockAllocationStatus::Registering | LockAllocationStatus::Weighing
    ) {
        return Err(EngineError::LockAllocationPhase);
    }
    if reg.cancelled {
        return Err(EngineError::LockRegistrationCancelled);
    }
    if reg.weight_gen > s.generation {
        return Err(EngineError::LockTallyInvalid);
    }
    if reg.weight_gen == s.generation {
        return Err(EngineError::LockAlreadyWeighed);
    }
    if ell > reg.requested {
        return Err(EngineError::LockWeightExceedsRequest);
    }
    s.weighed_count = add(s.weighed_count, 1)?;
    s.sum_ell = add(s.sum_ell, ell)?;
    if ell > 0 {
        s.positive_count = add(s.positive_count, 1)?;
    }
    if s.weighed_count > s.active_reg_count || s.sum_ell > s.active_requested_total {
        return Err(EngineError::LockTallyInvalid);
    }
    s.status = LockAllocationStatus::Weighing;
    pool.lock = s;
    reg.ell = ell;
    reg.weight_gen = s.generation;
    reg.award = 0;
    Ok(())
}

/// SEAL WEIGHTS: every active registration weighed in this generation (the
/// zero-active path seals an empty tally). Computes `H_lock` and
/// `gross_allocatable`.
pub fn seal_lock_weights(pool: &mut FruitPool, now: i64) -> EngineResult<()> {
    require_lock_pool(pool)?;
    if now < pool.t_start {
        return Err(EngineError::LockSeasonNotStarted);
    }
    let mut s = pool.lock;
    if !matches!(
        s.status,
        LockAllocationStatus::Registering | LockAllocationStatus::Weighing
    ) {
        return Err(EngineError::LockAllocationPhase);
    }
    if s.weighed_count != s.active_reg_count {
        return Err(EngineError::LockTallyIncomplete);
    }
    let (h_lock, gross) = lock_allocation(pool.budget, s.sum_ell)?;
    s.h_lock = h_lock;
    s.gross_allocatable = gross;
    s.status = LockAllocationStatus::Awarding;
    pool.lock = s;
    Ok(())
}

/// AWARD (permissionless): `A_i = floor(B * 5 ell_i / H_lock)` for one
/// registration weighed in this generation, once. Adds it to
/// `assigned_total`. Returns `A_i`.
pub fn award_lock_registration(
    pool: &mut FruitPool,
    reg: &mut LockRegistrationState,
) -> EngineResult<u64> {
    require_lock_pool(pool)?;
    let mut s = pool.lock;
    if s.status != LockAllocationStatus::Awarding {
        return Err(EngineError::LockAllocationPhase);
    }
    if reg.cancelled {
        return Err(EngineError::LockRegistrationCancelled);
    }
    if reg.weight_gen != s.generation {
        return Err(EngineError::LockNotWeighed);
    }
    if reg.award_gen > s.generation {
        return Err(EngineError::LockTallyInvalid);
    }
    if reg.award_gen == s.generation {
        return Err(EngineError::LockAlreadyAwarded);
    }
    let a = if reg.ell == 0 {
        0
    } else {
        lock_award(pool.budget, reg.ell, s.h_lock)?
    };
    s.awarded_count = add(s.awarded_count, 1)?;
    s.assigned_total = add(s.assigned_total, a)?;
    if s.awarded_count > s.active_reg_count || s.assigned_total > s.gross_allocatable {
        return Err(EngineError::LockTallyInvalid);
    }
    pool.lock = s;
    reg.award = a;
    reg.award_gen = s.generation;
    Ok(a)
}

/// Every tally invariant against the pool: complete counts, recomputed
/// `H_lock` / `gross_allocatable`, `assigned_total <= gross <= B`, and the
/// rounding bound `assigned_total >= gross - (positive_count - 1)`.
fn validate_tally(pool: &FruitPool, s: &LockState) -> EngineResult<()> {
    let bad = || EngineError::LockTallyInvalid;
    if s.generation == 0
        || s.budget != pool.budget
        || s.active_reg_count > s.total_reg_count
        || s.weighed_count != s.active_reg_count
        || s.awarded_count != s.active_reg_count
        || s.positive_count > s.weighed_count
        || s.sum_ell > s.active_requested_total
        || (s.active_requested_total as u128)
            > (s.active_reg_count as u128) * (LOCK_KMAX_LAMPORTS as u128)
        || (s.sum_ell as u128) > (s.positive_count as u128) * (LOCK_KMAX_LAMPORTS as u128)
        || s.sum_ell < s.positive_count
    {
        return Err(bad());
    }
    let (h_lock, gross) = lock_allocation(pool.budget, s.sum_ell)?;
    if s.h_lock != h_lock
        || s.gross_allocatable != gross
        || s.assigned_total > s.gross_allocatable
        || s.gross_allocatable > pool.budget
    {
        return Err(bad());
    }
    // sum_i floor(x_i) > floor(sum_i x_i) - n over the n positive weights.
    if s.positive_count == 0 {
        if s.assigned_total != 0 || s.gross_allocatable != 0 {
            return Err(bad());
        }
    } else if (s.assigned_total as u128) + (s.positive_count as u128 - 1)
        < s.gross_allocatable as u128
    {
        return Err(bad());
    }
    Ok(())
}

/// SEAL AWARDS (permissionless): every active registration awarded. Opens
/// the allocation's challenge window.
pub fn seal_lock_awards(pool: &mut FruitPool, now: i64, challenge_window: i64) -> EngineResult<()> {
    require_lock_pool(pool)?;
    let mut s = pool.lock;
    if s.status != LockAllocationStatus::Awarding {
        return Err(EngineError::LockAllocationPhase);
    }
    if s.awarded_count != s.active_reg_count {
        return Err(EngineError::LockTallyIncomplete);
    }
    if challenge_window <= 0 {
        return Err(EngineError::InvalidFruitGovernance);
    }
    validate_tally(pool, &s)?;
    s.final_after = now
        .checked_add(challenge_window)
        .ok_or(EngineError::MathOverflow)?;
    s.posted_at = now;
    s.veto_count = 0;
    s.status = LockAllocationStatus::Pending;
    pool.lock = s;
    Ok(())
}

/// One distinct veto signature on the Pending allocation (the caller
/// guarantees distinctness). At `threshold` the generation is retired and the
/// tally restarts at `g + 1` over the same frozen active registration set.
pub fn veto_lock_allocation(pool: &mut FruitPool, threshold: u8, now: i64) -> EngineResult<bool> {
    require_lock_pool(pool)?;
    let s = pool.lock;
    if s.status != LockAllocationStatus::Pending {
        return Err(EngineError::LockAllocationPhase);
    }
    if now >= s.final_after {
        return Err(EngineError::ChallengeWindowClosed);
    }
    if threshold == 0 {
        return Err(EngineError::InvalidFruitGovernance);
    }
    let votes = s
        .veto_count
        .checked_add(1)
        .ok_or(EngineError::MathOverflow)?;
    if votes < threshold {
        pool.lock.veto_count = votes;
        return Ok(false);
    }
    let generation = add(s.generation, 1)?;
    pool.lock = LockState {
        status: LockAllocationStatus::Weighing,
        budget: s.budget,
        generation,
        total_reg_count: s.total_reg_count,
        active_reg_count: s.active_reg_count,
        active_requested_total: s.active_requested_total,
        ..LockState::default()
    };
    Ok(true)
}

fn summary(pool: &FruitPool) -> LockAllocation {
    let s = pool.lock;
    LockAllocation {
        generation: s.generation,
        budget: pool.budget,
        active_reg_count: s.active_reg_count,
        positive_count: s.positive_count,
        active_requested_total: s.active_requested_total,
        sum_ell: s.sum_ell,
        h_lock: s.h_lock,
        gross_allocatable: s.gross_allocatable,
        assigned_total: s.assigned_total,
    }
}

/// FINALIZE ALLOCATION (permissionless) after the challenge window, before
/// any root: revalidates the whole tally, then Final for good. Moves no
/// lamports and does not touch `U` or `Z` (P7-L2).
pub fn finalize_lock_allocation(pool: &mut FruitPool, now: i64) -> EngineResult<LockAllocation> {
    require_lock_pool(pool)?;
    if pool.lock.status != LockAllocationStatus::Pending {
        return Err(EngineError::LockAllocationPhase);
    }
    if now < pool.lock.final_after {
        return Err(EngineError::ChallengeWindowOpen);
    }
    if pool.latest_version != 0 {
        return Err(EngineError::LockTallyInvalid);
    }
    validate_tally(pool, &pool.lock)?;
    pool.lock.status = LockAllocationStatus::Final;
    Ok(summary(pool))
}

/// The pool's Final allocation, fully revalidated. The only source of lock
/// capacity and of the forfeiture bound.
pub fn lock_final_allocation(pool: &FruitPool) -> EngineResult<LockAllocation> {
    if pool.kind != LedgerKind::Lock {
        return Err(EngineError::InvalidLedgerKind);
    }
    if pool.lock.status != LockAllocationStatus::Final {
        return Err(EngineError::LockAllocationNotFinal);
    }
    validate_tally(pool, &pool.lock)?;
    Ok(summary(pool))
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
