//! Lock Season engine (architecture §8 / §10a; Pass #7 decisions P7-L1,
//! P7-L2, F7, D-B1 and the approved rounding model).
//!
//! Covers the engine parts of tests 50, 53, 77, 88 and 89 and the P7-A / P7-B
//! invariants:
//!
//! * `assigned_total = sum_i floor(B u_i / H_lock)` exactly, accumulated by the
//!   engine from per-registration state; never an input (V-1 / V-2 closed);
//! * `rounding_dust = gross_allocatable - assigned_total`, never forfeitable;
//! * registration and cancellation only on `[FRUIT_T0, t_s)`; cancellation
//!   updates the active count and active requested total exactly once;
//! * the tally covers the frozen active set exactly once per generation; a
//!   veto re-tallies the same set;
//! * cliff capacity, F3 isolation, conservation and failure atomicity.

use proptest::prelude::*;
use rigs_engine::fruit::claim;
use rigs_engine::*;

const SOL: u64 = 1_000_000_000;
const WINDOW: i64 = 3_600;
const FRUIT_T0: i64 = 1_800_000_000;
const B: u64 = SEASON_BUDGET_LAMPORTS;
const OFF: SafetyRelease = SafetyRelease::Disabled;

fn t_s() -> i64 {
    lock_season_start(FRUIT_T0).unwrap()
}

/// A funded lock ledger with the Season pool committed before `t_s`.
fn season(budget: u64) -> (FruitLedger, FruitPool) {
    let mut l = FruitLedger::default();
    l.fund(budget).unwrap();
    let p = commit_lock_pool(&mut l, budget, FRUIT_T0, t_s() - 1).unwrap();
    (l, p)
}

/// Register one wallet per entry of `reqs` at `FRUIT_T0`.
fn register_all(pool: &mut FruitPool, reqs: &[u64]) -> Vec<LockRegistrationState> {
    reqs.iter()
        .map(|&r| register_lock(pool, r, FRUIT_T0).unwrap())
        .collect()
}

/// Weigh every active registration (`ells[i]` for `regs[i]`), seal, award,
/// seal awards — the complete tally of the current generation, at `now`.
fn tally_at(pool: &mut FruitPool, regs: &mut [LockRegistrationState], ells: &[u64], now: i64) {
    for (r, &e) in regs.iter_mut().zip(ells) {
        if !r.cancelled {
            post_lock_weight(pool, r, e, now).unwrap();
        }
    }
    seal_lock_weights(pool, now).unwrap();
    for r in regs.iter_mut().filter(|r| !r.cancelled) {
        award_lock_registration(pool, r).unwrap();
    }
    seal_lock_awards(pool, now, WINDOW).unwrap();
}

/// Registrations requesting `ells` (0 requests `Kmax`), weighed at exactly
/// `ells`, tallied and finalized. Returns the Final allocation.
fn finalized_regs(
    pool: &mut FruitPool,
    ells: &[u64],
) -> (LockAllocation, Vec<LockRegistrationState>) {
    let reqs: Vec<u64> = ells
        .iter()
        .map(|&e| if e == 0 { LOCK_KMAX_LAMPORTS } else { e })
        .collect();
    let mut regs = register_all(pool, &reqs);
    let ts = pool.t_start;
    tally_at(pool, &mut regs, ells, ts);
    let a = finalize_lock_allocation(pool, ts + WINDOW).unwrap();
    (a, regs)
}

fn finalized(pool: &mut FruitPool, ells: &[u64]) -> LockAllocation {
    finalized_regs(pool, ells).0
}

/// Sum of the wallet awards `sum_i floor(B u_i / H_lock)` for `ells`.
fn assigned(budget: u64, ells: &[u64]) -> u64 {
    let sum: u64 = ells.iter().sum();
    let (h, _) = lock_allocation(budget, sum).unwrap();
    ells.iter()
        .filter(|&&e| e > 0)
        .map(|&e| lock_award(budget, e, h).unwrap())
        .sum()
}

fn solvent(vault: u64, l: &FruitLedger) {
    assert!(vault >= l.obligations().unwrap(), "V < L + U + Z");
}

/// Assert `call` fails with `err` and leaves `state` unchanged.
macro_rules! atomic {
    ($state:expr, $err:expr, $call:expr) => {{
        let before = $state.clone();
        assert_eq!($call, Err($err));
        assert_eq!($state, before, "state mutated by a rejected call");
    }};
}

#[test]
fn frozen_lock_constants() {
    assert_eq!(LOCK_DAYS, 5);
    assert_eq!(LOCK_DURATION_SECONDS, 432_000);
    assert_eq!(LOCK_SEASON_OFFSET, 6 * DAY_SECONDS);
    assert_eq!(LOCK_KMAX_LAMPORTS, 2_500_000_000);
    assert_eq!(LOCK_HFLOOR_LAMPORTS, 12_500_000_000);
    assert_eq!(LOCK_H_FLOOR_LAMPORT_DAYS, 62_500_000_000);
    // t_s = FRUIT_T0 + 6 days = T0 + 7 days (§3 Season row).
    let t0 = FRUIT_T0 - DAY_SECONDS;
    assert_eq!(
        lock_season_start(fruit_t0(t0)).unwrap(),
        t0 + 7 * DAY_SECONDS
    );
    assert_eq!(lock_season_start(i64::MAX), Err(EngineError::MathOverflow));
}

// ------------------------------------------------ allocation formula (50)

#[test]
fn allocation_below_floor_at_floor_and_above() {
    // Nobody locking: H_lock at the floor, gross 0 (all of B returns to Z
    // at close).
    assert_eq!(
        lock_allocation(B, 0).unwrap(),
        (LOCK_H_FLOOR_LAMPORT_DAYS, 0)
    );
    // Two registrants, 5 SOL total: H_lock = 62.5, gross = floor(0.4 B).
    let (h, a) = lock_allocation(B, 5 * SOL).unwrap();
    assert_eq!(h, LOCK_H_FLOOR_LAMPORT_DAYS);
    // 428,082,192 * 0.4 = 171,232,876.8 -> floor.
    assert_eq!(a, B * 2 / 5);
    // Exactly at the floor: all of B.
    assert_eq!(lock_allocation(B, 12_500_000_000).unwrap().1, B);
    // Above the floor: H = 5 sum ell, gross = B.
    let (h, a) = lock_allocation(B, 20 * SOL).unwrap();
    assert_eq!(h, 100 * SOL);
    assert_eq!(a, B);
}

#[test]
fn per_wallet_awards_floor_and_cap() {
    // Five max wallets at the floor: each exactly floor(B/5).
    let (h, a) = lock_allocation(B, 5 * LOCK_KMAX_LAMPORTS).unwrap();
    assert_eq!(a, B);
    let ai = lock_award(B, LOCK_KMAX_LAMPORTS, h).unwrap();
    assert_eq!(ai, B / 5);
    assert_eq!(ai, wallet_cap(B));
    // A lone max wallet cannot exceed B/5 because of the floor.
    let (h1, a1) = lock_allocation(B, LOCK_KMAX_LAMPORTS).unwrap();
    assert_eq!(lock_award(B, LOCK_KMAX_LAMPORTS, h1).unwrap(), B / 5);
    assert_eq!(a1, B / 5);
    // Odd weights: sum floor(A_i) <= gross, rounding never up.
    let ells = [1u64, 7, 333_333_333, 2_499_999_999, 1_000_000_001];
    let sum: u64 = ells.iter().sum();
    let (h, a) = lock_allocation(B, sum).unwrap();
    let s: u64 = ells.iter().map(|&e| lock_award(B, e, h).unwrap()).sum();
    assert!(s <= a && a <= B);
    // Invalid per-wallet inputs.
    assert_eq!(lock_award(B, 0, h), Err(EngineError::InvalidLockWeight));
    assert_eq!(
        lock_award(B, LOCK_KMAX_LAMPORTS + 1, h),
        Err(EngineError::InvalidLockWeight)
    );
    assert_eq!(
        lock_award(B, 1, LOCK_H_FLOOR_LAMPORT_DAYS - 1),
        Err(EngineError::InvalidLockWeight)
    );
}

#[test]
fn allocation_overflow_is_checked() {
    assert_eq!(lock_h(u64::MAX), Err(EngineError::MathOverflow));
    assert_eq!(
        lock_allocation(B, u64::MAX / 4),
        Err(EngineError::MathOverflow)
    );
    assert!(lock_allocation(B, u64::MAX / LOCK_DAYS).is_ok());
    assert_eq!(
        lock_allocation(B, u64::MAX / LOCK_DAYS + 1),
        Err(EngineError::MathOverflow)
    );
    // u128 intermediate: maximal budget does not overflow.
    let max_ell = u64::MAX / LOCK_DAYS;
    assert_eq!(lock_allocation(u64::MAX, max_ell).unwrap().1, u64::MAX);
    let (h, _) = lock_allocation(u64::MAX, max_ell).unwrap();
    assert!(lock_award(u64::MAX, LOCK_KMAX_LAMPORTS, h).unwrap() <= u64::MAX / 5);
}

// ------------------------------------------------ timing

// ------------------------------------------------ timing / write-once (P7-L1, 53, 77)

#[test]
fn registration_window_closes_at_t_s() {
    let ts = t_s();
    assert!(!lock_registration_open(FRUIT_T0, ts, FRUIT_T0 - 1));
    assert!(lock_registration_open(FRUIT_T0, ts, FRUIT_T0));
    assert!(lock_registration_open(FRUIT_T0, ts, ts - 1));
    assert!(!lock_registration_open(FRUIT_T0, ts, ts));
}

#[test]
fn only_five_day_lock_windows_bound_to_fruit_t0() {
    let ts = t_s();
    for d in [
        DAY_SECONDS,
        2 * DAY_SECONDS,
        3 * DAY_SECONDS,
        LOCK_DURATION_SECONDS + 1,
    ] {
        assert_eq!(
            validate_lock_window(ts, ts + d),
            Err(EngineError::InvalidPoolWindow)
        );
    }
    validate_lock_window(ts, ts + LOCK_DURATION_SECONDS).unwrap();
    // commit_lock_pool derives the window from FRUIT_T0; generation 1.
    let (_, p) = season(B);
    assert_eq!(p.kind, LedgerKind::Lock);
    assert_eq!((p.t_start, p.t_end), (ts, ts + LOCK_DURATION_SECONDS));
    assert_eq!(p.lock.status, LockAllocationStatus::Registering);
    assert_eq!(p.lock.generation, 1);
    // It must be committed before t_s.
    let mut l = FruitLedger::default();
    l.fund(B).unwrap();
    assert_eq!(
        commit_lock_pool(&mut l, B, FRUIT_T0, ts),
        Err(EngineError::FruitPoolStarted)
    );
    assert_eq!((l.committed, l.reserve), (0, B));
    // A lock pool whose window was tampered takes no registration.
    let (_, mut p) = season(B);
    p.t_end = ts + 3 * DAY_SECONDS;
    atomic!(
        p,
        EngineError::InvalidPoolWindow,
        register_lock(&mut p, SOL, FRUIT_T0)
    );
    // A core pool is not a lock pool.
    let mut l = FruitLedger::default();
    l.fund(B).unwrap();
    let mut core = commit_pool(&mut l, B, ts, ts + LOCK_DURATION_SECONDS, ts - 1).unwrap();
    atomic!(
        core,
        EngineError::InvalidLedgerKind,
        register_lock(&mut core, SOL, FRUIT_T0)
    );
    let mut r = LockRegistrationState {
        requested: SOL,
        ..Default::default()
    };
    atomic!(
        (core, r),
        EngineError::InvalidLedgerKind,
        post_lock_weight(&mut core, &mut r, SOL, ts)
    );
    atomic!(
        core,
        EngineError::InvalidLedgerKind,
        seal_lock_weights(&mut core, ts)
    );
}

// ------------------------------------------------ registration / cancellation (P7-L1, D-B1)

#[test]
fn registration_boundaries_and_request_bounds() {
    let (_, mut pool) = season(B);
    let ts = pool.t_start;
    atomic!(
        pool,
        EngineError::LockRegistrationClosed,
        register_lock(&mut pool, SOL, FRUIT_T0 - 1)
    );
    atomic!(
        pool,
        EngineError::LockRegistrationClosed,
        register_lock(&mut pool, SOL, ts)
    );
    atomic!(
        pool,
        EngineError::InvalidLockRequest,
        register_lock(&mut pool, 0, FRUIT_T0)
    );
    atomic!(
        pool,
        EngineError::InvalidLockRequest,
        register_lock(&mut pool, LOCK_KMAX_LAMPORTS + 1, FRUIT_T0)
    );
    let a = register_lock(&mut pool, 1, FRUIT_T0).unwrap();
    let b = register_lock(&mut pool, LOCK_KMAX_LAMPORTS, ts - 1).unwrap();
    assert_eq!((a.requested, b.requested), (1, LOCK_KMAX_LAMPORTS));
    assert!(!a.cancelled && a.weight_gen == 0 && a.award_gen == 0);
    let s = pool.lock;
    assert_eq!(
        (
            s.total_reg_count,
            s.active_reg_count,
            s.active_requested_total
        ),
        (2, 2, 1 + LOCK_KMAX_LAMPORTS)
    );
    // Counter overflow is checked and atomic.
    let mut p = pool;
    p.lock.total_reg_count = u64::MAX;
    atomic!(
        p,
        EngineError::MathOverflow,
        register_lock(&mut p, SOL, FRUIT_T0)
    );
    let mut p = pool;
    p.lock.active_requested_total = u64::MAX;
    atomic!(
        p,
        EngineError::MathOverflow,
        register_lock(&mut p, SOL, FRUIT_T0)
    );
}

#[test]
fn cancellation_boundaries_and_exact_accounting() {
    let (_, mut pool) = season(B);
    let ts = pool.t_start;
    let mut regs = register_all(&mut pool, &[SOL, 2 * SOL, 700_000_001, 5]);
    let before = pool.lock;
    // FRUIT_T0 - 1: closed.
    atomic!(
        (pool, regs[0]),
        EngineError::LockRegistrationClosed,
        cancel_lock(&mut pool, &mut regs[0], FRUIT_T0 - 1)
    );
    // FRUIT_T0: open.
    cancel_lock(&mut pool, &mut regs[0], FRUIT_T0).unwrap();
    assert!(regs[0].cancelled);
    assert_eq!(pool.lock.active_reg_count, before.active_reg_count - 1);
    assert_eq!(
        pool.lock.active_requested_total,
        before.active_requested_total - SOL
    );
    assert_eq!(pool.lock.total_reg_count, before.total_reg_count);
    // Repeated cancellation: refused, nothing changes.
    atomic!(
        (pool, regs[0]),
        EngineError::LockRegistrationCancelled,
        cancel_lock(&mut pool, &mut regs[0], FRUIT_T0 + 1)
    );
    // t_s - 1: open.
    cancel_lock(&mut pool, &mut regs[1], ts - 1).unwrap();
    // t_s: closed (the active set is frozen).
    atomic!(
        (pool, regs[2]),
        EngineError::LockRegistrationClosed,
        cancel_lock(&mut pool, &mut regs[2], ts)
    );
    let s = pool.lock;
    assert_eq!(
        (
            s.total_reg_count,
            s.active_reg_count,
            s.active_requested_total
        ),
        (4, 2, 700_000_001 + 5)
    );
    // Registration also closes at t_s: counters are frozen from t_s.
    atomic!(
        pool,
        EngineError::LockRegistrationClosed,
        register_lock(&mut pool, SOL, ts)
    );
}

#[test]
fn cancellation_cannot_underflow() {
    let (_, mut pool) = season(B);
    let mut regs = register_all(&mut pool, &[SOL]);
    // Corrupted: active count already 0 with a live registration.
    let mut p = pool;
    p.lock.active_reg_count = 0;
    atomic!(
        (p, regs[0]),
        EngineError::LockTallyInvalid,
        cancel_lock(&mut p, &mut regs[0], FRUIT_T0)
    );
    // Corrupted: requested total below the registration's request.
    let mut p = pool;
    p.lock.active_requested_total = SOL - 1;
    atomic!(
        (p, regs[0]),
        EngineError::LockTallyInvalid,
        cancel_lock(&mut p, &mut regs[0], FRUIT_T0)
    );
    // Corrupted registration request.
    let mut bad = regs[0];
    bad.requested = 0;
    atomic!(
        (pool, bad),
        EngineError::LockTallyInvalid,
        cancel_lock(&mut pool, &mut bad, FRUIT_T0)
    );
    cancel_lock(&mut pool, &mut regs[0], FRUIT_T0).unwrap();
    assert_eq!(
        (pool.lock.active_reg_count, pool.lock.active_requested_total),
        (0, 0)
    );
}

#[test]
fn cancelled_registration_cannot_be_weighed_or_awarded() {
    let (_, mut pool) = season(B);
    let ts = pool.t_start;
    let mut regs = register_all(&mut pool, &[SOL, 2 * SOL]);
    cancel_lock(&mut pool, &mut regs[0], FRUIT_T0).unwrap();
    atomic!(
        (pool, regs[0]),
        EngineError::LockRegistrationCancelled,
        post_lock_weight(&mut pool, &mut regs[0], SOL, ts)
    );
    post_lock_weight(&mut pool, &mut regs[1], 2 * SOL, ts).unwrap();
    seal_lock_weights(&mut pool, ts).unwrap();
    atomic!(
        (pool, regs[0]),
        EngineError::LockRegistrationCancelled,
        award_lock_registration(&mut pool, &mut regs[0])
    );
    award_lock_registration(&mut pool, &mut regs[1]).unwrap();
    seal_lock_awards(&mut pool, ts, WINDOW).unwrap();
    let a = finalize_lock_allocation(&mut pool, ts + WINDOW).unwrap();
    assert_eq!(a.active_reg_count, 1);
    assert_eq!(a.assigned_total, assigned(B, &[2 * SOL]));
    assert_eq!(regs[0].award_for(a.generation), 0);
    assert_eq!(regs[1].award_for(a.generation), a.assigned_total);
}

#[test]
fn mixed_active_and_cancelled_seal_on_the_active_count() {
    let (_, mut pool) = season(B);
    let ts = pool.t_start;
    let reqs = [LOCK_KMAX_LAMPORTS, 1_111_111_111, 777_777_777, SOL, 3];
    let mut regs = register_all(&mut pool, &reqs);
    cancel_lock(&mut pool, &mut regs[1], FRUIT_T0 + 5).unwrap();
    cancel_lock(&mut pool, &mut regs[3], ts - 1).unwrap();
    assert_eq!(
        (pool.lock.total_reg_count, pool.lock.active_reg_count),
        (5, 3)
    );
    // Weigh two of the three active: sealing is refused.
    post_lock_weight(&mut pool, &mut regs[0], LOCK_KMAX_LAMPORTS, ts).unwrap();
    post_lock_weight(&mut pool, &mut regs[2], 700_000_000, ts).unwrap();
    atomic!(
        pool,
        EngineError::LockTallyIncomplete,
        seal_lock_weights(&mut pool, ts)
    );
    post_lock_weight(&mut pool, &mut regs[4], 3, ts).unwrap();
    seal_lock_weights(&mut pool, ts).unwrap();
    award_lock_registration(&mut pool, &mut regs[0]).unwrap();
    award_lock_registration(&mut pool, &mut regs[4]).unwrap();
    atomic!(
        pool,
        EngineError::LockTallyIncomplete,
        seal_lock_awards(&mut pool, ts, WINDOW)
    );
    award_lock_registration(&mut pool, &mut regs[2]).unwrap();
    seal_lock_awards(&mut pool, ts, WINDOW).unwrap();
    let a = finalize_lock_allocation(&mut pool, ts + WINDOW).unwrap();
    let ells = [LOCK_KMAX_LAMPORTS, 700_000_000, 3];
    assert_eq!(a.sum_ell, ells.iter().sum::<u64>());
    assert_eq!(a.assigned_total, assigned(B, &ells));
    assert_eq!(
        regs.iter().map(|r| r.award_for(a.generation)).sum::<u64>(),
        a.assigned_total
    );
}

/// Zero-active path (documented in PASS7_LOCK_DECISIONS.md): with every
/// registration cancelled — or none made — the tally is empty. At or after
/// `t_s` it seals with `sum_ell = 0`, `H_lock` at the floor,
/// `gross = assigned = 0`, and finalizes after the challenge window like
/// any allocation. A zero-total complete root then closes the pool with all
/// of `B_lock` back in `Z` and nothing to the treasury.
#[test]
fn all_cancelled_follows_the_zero_active_path() {
    for cancel_all in [true, false] {
        let (mut l, mut pool) = season(B);
        let ts = pool.t_start;
        let te = pool.t_end;
        if cancel_all {
            let mut regs = register_all(&mut pool, &[SOL, 2 * SOL]);
            for r in regs.iter_mut() {
                cancel_lock(&mut pool, r, FRUIT_T0 + 1).unwrap();
            }
            assert_eq!(pool.lock.total_reg_count, 2);
        }
        assert_eq!(
            (pool.lock.active_reg_count, pool.lock.active_requested_total),
            (0, 0)
        );
        // Not before t_s.
        atomic!(
            pool,
            EngineError::LockSeasonNotStarted,
            seal_lock_weights(&mut pool, ts - 1)
        );
        seal_lock_weights(&mut pool, ts).unwrap();
        assert_eq!(pool.lock.status, LockAllocationStatus::Awarding);
        assert_eq!(pool.lock.h_lock, LOCK_H_FLOOR_LAMPORT_DAYS);
        seal_lock_awards(&mut pool, ts, WINDOW).unwrap();
        let a = finalize_lock_allocation(&mut pool, ts + WINDOW).unwrap();
        assert_eq!(
            (a.gross_allocatable, a.assigned_total, a.rounding_dust()),
            (0, 0, 0)
        );
        assert_eq!(a.unallocated(), B);
        assert_eq!(lock_capacity(&pool, te, OFF).unwrap(), 0);
        let mut r = post_lock_root(&mut pool, 1, te, 0, true, 0, te, WINDOW, OFF).unwrap();
        finalize_root(&mut l, &mut pool, &mut r, te + WINDOW).unwrap();
        let out = close_pool(&mut l, &mut pool, te + WINDOW).unwrap();
        assert_eq!(
            out,
            CloseOutcome {
                released: B,
                treasury_release: 0
            }
        );
        assert_eq!((l.committed, l.reserve, l.liabilities), (0, B, 0));
    }
}

// ------------------------------------------------ tally (V-1 / V-2 closure)

#[test]
fn weights_post_only_from_t_s_once_and_within_request() {
    let (_, mut pool) = season(B);
    let ts = pool.t_start;
    let mut regs = register_all(&mut pool, &[SOL, 2 * SOL]);
    atomic!(
        (pool, regs[0]),
        EngineError::LockSeasonNotStarted,
        post_lock_weight(&mut pool, &mut regs[0], SOL, ts - 1)
    );
    atomic!(
        (pool, regs[0]),
        EngineError::LockWeightExceedsRequest,
        post_lock_weight(&mut pool, &mut regs[0], SOL + 1, ts)
    );
    post_lock_weight(&mut pool, &mut regs[0], SOL / 2, ts).unwrap();
    assert_eq!(pool.lock.status, LockAllocationStatus::Weighing);
    atomic!(
        (pool, regs[0]),
        EngineError::LockAlreadyWeighed,
        post_lock_weight(&mut pool, &mut regs[0], SOL / 2, ts)
    );
    // A registration record from the future generation is corrupt.
    let mut fut = regs[1];
    fut.weight_gen = 9;
    atomic!(
        (pool, fut),
        EngineError::LockTallyInvalid,
        post_lock_weight(&mut pool, &mut fut, SOL, ts)
    );
    // A stale / fabricated extra registration cannot push the tally past the
    // frozen active set or its requested total.
    post_lock_weight(&mut pool, &mut regs[1], 2 * SOL, ts).unwrap();
    let mut extra = LockRegistrationState {
        requested: SOL,
        ..Default::default()
    };
    atomic!(
        (pool, extra),
        EngineError::LockTallyInvalid,
        post_lock_weight(&mut pool, &mut extra, 0, ts)
    );
    // Award before sealing: wrong phase.
    atomic!(
        (pool, regs[0]),
        EngineError::LockAllocationPhase,
        award_lock_registration(&mut pool, &mut regs[0])
    );
    seal_lock_weights(&mut pool, ts).unwrap();
    // After sealing, weights are closed for this generation.
    atomic!(
        (pool, extra),
        EngineError::LockAllocationPhase,
        post_lock_weight(&mut pool, &mut extra, 0, ts)
    );
    // An unweighed registration cannot be awarded.
    atomic!(
        (pool, extra),
        EngineError::LockNotWeighed,
        award_lock_registration(&mut pool, &mut extra)
    );
    award_lock_registration(&mut pool, &mut regs[0]).unwrap();
    atomic!(
        (pool, regs[0]),
        EngineError::LockAlreadyAwarded,
        award_lock_registration(&mut pool, &mut regs[0])
    );
    award_lock_registration(&mut pool, &mut regs[1]).unwrap();
    atomic!(
        pool,
        EngineError::InvalidFruitGovernance,
        seal_lock_awards(&mut pool, ts, 0)
    );
    seal_lock_awards(&mut pool, ts, WINDOW).unwrap();
    atomic!(
        pool,
        EngineError::ChallengeWindowOpen,
        finalize_lock_allocation(&mut pool, ts + WINDOW - 1)
    );
    // Pending gives no capacity: no root.
    let te = pool.t_end;
    atomic!(
        pool,
        EngineError::LockAllocationNotFinal,
        post_lock_root(&mut pool, 1, te, 0, true, 0, te, WINDOW, OFF)
    );
    finalize_lock_allocation(&mut pool, ts + WINDOW).unwrap();
}

/// V-1 / V-2: `assigned_total` is exactly the sum of the engine-computed
/// awards; there is no input through which a poster could state a lower
/// (under-allocation) or higher (dust overstatement) total.
#[test]
fn v1_v2_assigned_total_is_the_exact_sum_of_awards() {
    let (_, mut pool) = season(B);
    let ells = [123_456_789u64; 10];
    let (a, regs) = finalized_regs(&mut pool, &ells);
    let per: Vec<u64> = regs.iter().map(|r| r.award_for(a.generation)).collect();
    let (h, gross) = lock_allocation(B, ells.iter().sum()).unwrap();
    for (r, &e) in regs.iter().zip(&ells) {
        assert_eq!(r.award, lock_award(B, e, h).unwrap());
    }
    assert_eq!(a.assigned_total, per.iter().sum::<u64>());
    assert_eq!(a.gross_allocatable, gross);
    let dust = a.rounding_dust();
    assert!(dust > 0, "fixture must produce dust");
    assert_eq!(dust, gross - a.assigned_total);
    // Lower-bound invariant holds (and is not what closes V-1).
    assert!(a.assigned_total + (a.positive_count - 1) >= a.gross_allocatable);
    // Stored-state edits outside the arithmetic envelope fail closed (inside
    // it, integrity comes from the program: only engine transitions write
    // the Season). Overstating past gross, or zeroing, is caught.
    let mut p = pool;
    p.lock.assigned_total = p.lock.gross_allocatable + 1;
    assert_eq!(
        lock_final_allocation(&p),
        Err(EngineError::LockTallyInvalid)
    );
    let mut p = pool;
    p.lock.assigned_total = 0;
    assert_eq!(
        lock_final_allocation(&p),
        Err(EngineError::LockTallyInvalid)
    );
}

#[test]
fn veto_retallies_the_same_frozen_active_set() {
    let (_, mut pool) = season(B);
    let ts = pool.t_start;
    let mut regs = register_all(&mut pool, &[SOL, 2 * SOL, LOCK_KMAX_LAMPORTS]);
    cancel_lock(&mut pool, &mut regs[2], FRUIT_T0).unwrap();
    let frozen = (
        pool.lock.total_reg_count,
        pool.lock.active_reg_count,
        pool.lock.active_requested_total,
    );
    tally_at(&mut pool, &mut regs, &[SOL, 2 * SOL, 0], ts);
    let g1 = pool.lock.generation;
    let a1 = pool.lock.assigned_total;
    // Veto: wrong phases and threshold 0 refused.
    atomic!(
        pool,
        EngineError::InvalidFruitGovernance,
        veto_lock_allocation(&mut pool, 0, ts + 1)
    );
    assert_eq!(veto_lock_allocation(&mut pool, 2, ts + 1), Ok(false));
    assert_eq!(pool.lock.veto_count, 1);
    assert_eq!(veto_lock_allocation(&mut pool, 2, ts + 2), Ok(true));
    // Same frozen set, new generation, empty tally.
    let s = pool.lock;
    assert_eq!(s.generation, g1 + 1);
    assert_eq!(s.status, LockAllocationStatus::Weighing);
    assert_eq!(
        (
            s.total_reg_count,
            s.active_reg_count,
            s.active_requested_total
        ),
        frozen
    );
    assert_eq!(
        (
            s.weighed_count,
            s.sum_ell,
            s.awarded_count,
            s.assigned_total,
            s.veto_count
        ),
        (0, 0, 0, 0, 0)
    );
    atomic!(
        pool,
        EngineError::LockAllocationPhase,
        veto_lock_allocation(&mut pool, 1, ts + 3)
    );
    atomic!(
        pool,
        EngineError::LockAllocationPhase,
        finalize_lock_allocation(&mut pool, ts + WINDOW)
    );
    // Old-generation weights do not count: sealing needs the re-tally.
    atomic!(
        pool,
        EngineError::LockTallyIncomplete,
        seal_lock_weights(&mut pool, ts + 3)
    );
    // Re-tally with different weights; the cancelled one stays out.
    tally_at(&mut pool, &mut regs, &[SOL / 2, 2 * SOL, 0], ts + 10);
    let a = finalize_lock_allocation(&mut pool, ts + 10 + WINDOW).unwrap();
    assert_eq!(a.generation, g1 + 1);
    assert_eq!(a.assigned_total, assigned(B, &[SOL / 2, 2 * SOL]));
    assert!(a.assigned_total < a1);
    assert_eq!(regs[2].award_for(a.generation), 0);
    // A veto after the window is refused; Final is immutable.
    atomic!(
        pool,
        EngineError::LockAllocationPhase,
        veto_lock_allocation(&mut pool, 1, ts + 11)
    );
}

#[test]
fn veto_after_window_and_generation_overflow_are_refused() {
    let (_, mut pool) = season(B);
    let ts = pool.t_start;
    let mut regs = register_all(&mut pool, &[SOL]);
    tally_at(&mut pool, &mut regs, &[SOL], ts);
    atomic!(
        pool,
        EngineError::ChallengeWindowClosed,
        veto_lock_allocation(&mut pool, 1, ts + WINDOW)
    );
    let mut p = pool;
    p.lock.generation = u64::MAX;
    atomic!(
        p,
        EngineError::MathOverflow,
        veto_lock_allocation(&mut p, 1, ts + 1)
    );
}

/// Finalization and every later read revalidate the whole stored tally.
#[test]
fn tampered_tally_fails_closed_and_atomically() {
    let (_, mut pending) = season(B);
    let ts = pending.t_start;
    let mut regs = register_all(&mut pending, &[LOCK_KMAX_LAMPORTS, 1_111_111_111, 3]);
    tally_at(
        &mut pending,
        &mut regs,
        &[LOCK_KMAX_LAMPORTS, 1_000_000_007, 3],
        ts,
    );
    type Tamper = fn(&mut LockState);
    let tampers: [(&str, Tamper); 14] = [
        ("generation 0", |s| s.generation = 0),
        ("budget", |s| s.budget += 1),
        ("active > total", |s| {
            s.active_reg_count = s.total_reg_count + 1
        }),
        ("weighed", |s| s.weighed_count -= 1),
        ("awarded", |s| s.awarded_count += 1),
        ("positive", |s| s.positive_count = s.weighed_count + 1),
        ("sum_ell up", |s| s.sum_ell += SOL),
        ("sum_ell > requested", |s| {
            s.sum_ell = s.active_requested_total + 1
        }),
        ("h_lock", |s| s.h_lock += 1),
        ("gross", |s| s.gross_allocatable -= 1),
        ("assigned up", |s| {
            s.assigned_total = s.gross_allocatable + 1
        }),
        ("assigned down", |s| s.assigned_total = 0),
        ("requested", |s| s.active_requested_total = u64::MAX),
        ("positive 0", |s| s.positive_count = 0),
    ];
    for (label, f) in tampers {
        let mut p = pending;
        f(&mut p.lock);
        let before = p;
        assert_eq!(
            finalize_lock_allocation(&mut p, ts + WINDOW),
            Err(EngineError::LockTallyInvalid),
            "{label}"
        );
        assert_eq!(p, before, "{label}: refused finalize changed state");
    }
    // A pool budget change is caught the same way.
    let mut p = pending;
    p.budget += 1;
    assert_eq!(
        finalize_lock_allocation(&mut p, ts + WINDOW),
        Err(EngineError::LockTallyInvalid)
    );
    // A root before the allocation is tampered state.
    let mut p = pending;
    p.latest_version = 1;
    atomic!(
        p,
        EngineError::LockTallyInvalid,
        finalize_lock_allocation(&mut p, ts + WINDOW)
    );
    // Final: every tamper also fails every read.
    let mut fin = pending;
    finalize_lock_allocation(&mut fin, ts + WINDOW).unwrap();
    let te = fin.t_end;
    for (label, f) in tampers {
        let mut p = fin;
        f(&mut p.lock);
        assert_eq!(
            lock_capacity(&p, te, OFF),
            Err(EngineError::LockTallyInvalid),
            "{label}"
        );
    }
}

/// M14: roots, capacity, terminal resolution and close require the pool's
/// own Final status, not just matching numbers.
#[test]
fn m14_final_status_is_required() {
    let (l0, mut pool) = season(B);
    let a = finalized(&mut pool, &[LOCK_KMAX_LAMPORTS, SOL]);
    let te = pool.t_end;
    assert!(a.assigned_total > 0);
    for status in [
        LockAllocationStatus::Registering,
        LockAllocationStatus::Weighing,
        LockAllocationStatus::Awarding,
        LockAllocationStatus::Pending,
    ] {
        let mut p = pool;
        p.lock.status = status;
        let mut l = l0;
        assert_eq!(
            lock_capacity(&p, te, OFF),
            Err(EngineError::LockAllocationNotFinal)
        );
        atomic!(
            p,
            EngineError::LockAllocationNotFinal,
            post_lock_root(&mut p, 1, te, 0, true, 0, te, WINDOW, OFF)
        );
        atomic!(
            (l, p),
            EngineError::LockAllocationNotFinal,
            terminal_lock_resolution(&mut l, &mut p, 1, te, 0, te)
        );
    }
    // A Final pool closes only while it is still Final and valid.
    let mut l = l0;
    let mut r = post_lock_root(&mut pool, 1, te, 0, true, 0, te, WINDOW, OFF).unwrap();
    finalize_root(&mut l, &mut pool, &mut r, te + WINDOW).unwrap();
    let mut p = pool;
    p.lock.status = LockAllocationStatus::Pending;
    atomic!(
        (l, p),
        EngineError::LockAllocationNotFinal,
        close_pool(&mut l, &mut p, te + WINDOW)
    );
    let mut p = pool;
    p.lock.assigned_total += 1;
    atomic!(
        (l, p),
        EngineError::LockTallyInvalid,
        close_pool(&mut l, &mut p, te + WINDOW)
    );
    close_pool(&mut l, &mut pool, te + WINDOW).unwrap();
}

/// Test 53: a data gap at `t_s` means no weights are posted; the pool then
/// accepts no root, so all of `B_lock` stays in `U`.
#[test]
fn gap_at_t_s_keeps_all_of_b_lock_reserved() {
    let (mut l, mut pool) = season(B);
    register_all(&mut pool, &[SOL]);
    let te = pool.t_end;
    atomic!(
        pool,
        EngineError::LockAllocationNotFinal,
        post_lock_root(&mut pool, 1, te, 0, true, 0, te, WINDOW, OFF)
    );
    atomic!(
        (l, pool),
        EngineError::LockAllocationNotFinal,
        terminal_lock_resolution(&mut l, &mut pool, 1, te, 0, te)
    );
    atomic!(
        pool,
        EngineError::LockTallyIncomplete,
        seal_lock_weights(&mut pool, te)
    );
    assert_eq!(
        close_pool(&mut l, &mut pool, te),
        Err(EngineError::FruitPoolNotReconciled)
    );
    assert_eq!((l.committed, l.reserve, l.liabilities), (B, 0, 0));
}

/// P3: a legitimate zero allocation (registrants whose awards all floor to
/// zero) is Final, write-once, and returns all of B to Z.
#[test]
fn p3_legitimate_zero_allocation_is_final_and_write_once() {
    let budget = 12_499;
    let (mut l, mut pool) = season(budget);
    let ells = [1_000_000u64, 999_999];
    let (_, gross) = lock_allocation(budget, ells.iter().sum()).unwrap();
    assert_eq!(assigned(budget, &ells), 0);
    assert_eq!(gross, 1);
    let te = pool.t_end;
    let a = finalized(&mut pool, &ells);
    assert_eq!((a.gross_allocatable, a.assigned_total), (1, 0));
    assert_eq!(a.rounding_dust(), 1);
    assert_eq!(pool.lock.status, LockAllocationStatus::Final);
    // Nothing reopens it.
    atomic!(
        pool,
        EngineError::LockAllocationPhase,
        seal_lock_weights(&mut pool, te)
    );
    atomic!(
        pool,
        EngineError::LockAllocationPhase,
        seal_lock_awards(&mut pool, te, WINDOW)
    );
    atomic!(
        pool,
        EngineError::LockAllocationPhase,
        finalize_lock_allocation(&mut pool, te)
    );
    assert_eq!(lock_capacity(&pool, te, OFF).unwrap(), 0);
    assert_eq!(
        post_lock_root(&mut pool.clone(), 1, te, 1, true, 0, te, WINDOW, OFF),
        Err(EngineError::RootExceedsCapacity)
    );
    let mut r = post_lock_root(&mut pool, 1, te, 0, true, 0, te, WINDOW, OFF).unwrap();
    finalize_root(&mut l, &mut pool, &mut r, te + WINDOW).unwrap();
    let out = close_pool(&mut l, &mut pool, te + WINDOW).unwrap();
    assert_eq!(out.released, budget);
    assert_eq!(out.treasury_release, 0);
    assert_eq!((l.committed, l.reserve), (0, budget));
}

// ------------------------------------------------ F3 / dust / cliff / close

/// F3: the generic root and terminal paths refuse lock pools, so they can
/// neither vest nor (via a complete root) close lock capacity.
#[test]
fn generic_root_and_terminal_paths_reject_lock_pools() {
    let (mut l, mut pool) = season(B);
    let a = finalized(&mut pool, &[LOCK_KMAX_LAMPORTS, SOL]);
    let (te, mid) = (pool.t_end, pool.t_start + LOCK_DURATION_SECONDS / 2);
    let before = (l, pool);
    // Linear capacity would allow ~B/2 at mid-Season and all of E at t_end.
    assert_eq!(
        post_root(&mut pool, 1, mid, 1, false, 0, mid, WINDOW),
        Err(EngineError::InvalidLedgerKind)
    );
    assert_eq!(
        post_root(&mut pool, 1, te, B, true, 0, te, WINDOW),
        Err(EngineError::InvalidLedgerKind)
    );
    assert_eq!(
        post_root(&mut pool, 1, te, 0, true, 0, te, WINDOW),
        Err(EngineError::InvalidLedgerKind)
    );
    assert_eq!(
        terminal_resolution(&mut l, &mut pool, 1, te, B, te),
        Err(EngineError::InvalidLedgerKind)
    );
    assert_eq!(
        terminal_resolution(&mut l, &mut pool, 1, te, 0, te),
        Err(EngineError::InvalidLedgerKind)
    );
    assert_eq!((l, pool), before);
    // No complete root, so no close.
    assert_eq!(
        close_pool(&mut l, &mut pool, te + WINDOW),
        Err(EngineError::FruitPoolNotReconciled)
    );
    // finalize_root re-caps a caller-built lock root at assigned_total.
    let mut r =
        post_lock_root(&mut pool, 1, te, a.assigned_total, true, 0, te, WINDOW, OFF).unwrap();
    let mut forged = r;
    forged.total = a.assigned_total + 1;
    assert_eq!(
        finalize_root(&mut l.clone(), &mut pool.clone(), &mut forged, te + WINDOW),
        Err(EngineError::RootExceedsCapacity)
    );
    finalize_root(&mut l, &mut pool, &mut r, te + WINDOW).unwrap();
    assert_eq!(l.liabilities, a.assigned_total);
    // root_capacity follows the pool's kind.
    assert_eq!(root_capacity(&pool, mid, OFF).unwrap(), 0);
    assert_eq!(root_capacity(&pool, te, OFF).unwrap(), a.assigned_total);
}

/// F7: rounding dust is unallocated, returns 100% to Z, and can never be
/// carried as forfeiture.
#[test]
fn rounding_dust_returns_entirely_to_z_never_to_treasury() {
    let (mut l, mut pool) = season(B);
    let ells = [123_456_789u64; 10];
    let (a, regs) = finalized_regs(&mut pool, &ells);
    let dust = a.rounding_dust();
    assert!(dust > 0, "fixture must produce dust");
    assert_eq!(dust, a.gross_allocatable - a.assigned_total);
    assert_eq!(a.unallocated(), B - a.assigned_total);
    // Everyone survives: every assigned award vests.
    let te = pool.t_end;
    let total = a.assigned_total;
    for f in [1, dust] {
        assert_eq!(
            post_lock_root(&mut pool.clone(), 1, te, total, true, f, te, WINDOW, OFF),
            Err(EngineError::InvalidForfeited)
        );
    }
    // Capacity is the assigned awards, never the gross.
    assert_eq!(
        post_lock_root(
            &mut pool.clone(),
            1,
            te,
            total + 1,
            true,
            0,
            te,
            WINDOW,
            OFF
        ),
        Err(EngineError::RootExceedsCapacity)
    );
    let mut r = post_lock_root(&mut pool, 1, te, total, true, 0, te, WINDOW, OFF).unwrap();
    finalize_root(&mut l, &mut pool, &mut r, te + WINDOW).unwrap();
    let mut vault = B;
    let mut c = 0;
    vault -= claim_lock(&mut l, &mut pool, &regs[0], &mut c, total / 10).unwrap();
    let out = close_pool(&mut l, &mut pool, te + WINDOW).unwrap();
    assert_eq!(out.treasury_release, 0);
    assert_eq!(out.released, B - total);
    // Z holds the unallocated capacity, dust included, in full.
    assert_eq!(l.reserve, a.unallocated());
    assert_eq!(l.committed, 0);
    assert_eq!(vault, l.obligations().unwrap());
}

/// F7: actual forfeiture is bounded by assigned awards minus final surviving
/// awards (`assigned_total - final_total`), not by gross or by `E`.
#[test]
fn forfeiture_bounded_by_assigned_minus_final() {
    let (mut l, mut pool) = season(B);
    let ells = [LOCK_KMAX_LAMPORTS, 1_111_111_111, 777_777_777];
    let a = finalized(&mut pool, &ells);
    assert!(a.rounding_dust() > 0 && a.unallocated() > a.rounding_dust());
    let awards: Vec<u64> = ells
        .iter()
        .map(|&e| lock_award(B, e, a.h_lock).unwrap())
        .collect();
    // Wallet 0 keeps a third; the others survive in full.
    let vested = [
        lock_vest_at_maturity(awards[0], ells[0] / 3, ells[0]).unwrap(),
        awards[1],
        awards[2],
    ];
    let total: u64 = vested.iter().sum();
    let forfeited = awards[0] - vested[0];
    let bound = a.assigned_total - total;
    assert_eq!(
        bound, forfeited,
        "every assigned lamport is vested or forfeited"
    );
    let te = pool.t_end;
    assert_eq!(
        post_lock_root(
            &mut pool.clone(),
            1,
            te,
            total,
            true,
            bound + 1,
            te,
            WINDOW,
            OFF
        ),
        Err(EngineError::InvalidForfeited)
    );
    assert_eq!(
        post_lock_root(
            &mut pool.clone(),
            1,
            te,
            total,
            true,
            a.gross_allocatable - total,
            te,
            WINDOW,
            OFF
        ),
        Err(EngineError::InvalidForfeited)
    );
    let mut r = post_lock_root(&mut pool, 1, te, total, true, forfeited, te, WINDOW, OFF).unwrap();
    finalize_root(&mut l, &mut pool, &mut r, te + WINDOW).unwrap();
    let out = close_pool(&mut l, &mut pool, te + WINDOW).unwrap();
    assert_eq!(out.treasury_release, forfeited * 8 / 10);
    // Z: unallocated (dust included) in full + the 20% of forfeiture.
    assert_eq!(
        l.reserve,
        a.unallocated() + (forfeited - forfeited * 8 / 10)
    );
    assert_eq!(l.liabilities, total);
}

// ------------------------------------------------ cliff capacity

#[test]
fn cliff_capacity_nothing_before_maturity_exactly_assigned_at_it() {
    let (_, mut pool) = season(B);
    let a = finalized(&mut pool, &[LOCK_KMAX_LAMPORTS, LOCK_KMAX_LAMPORTS]);
    let (ts, te) = (pool.t_start, pool.t_end);
    assert_eq!(lock_capacity(&pool, ts, OFF).unwrap(), 0);
    assert_eq!(lock_capacity(&pool, te - 1, OFF).unwrap(), 0);
    assert_eq!(lock_capacity(&pool, te, OFF).unwrap(), a.assigned_total);
    assert_eq!(
        lock_capacity(&pool, i64::MAX, OFF).unwrap(),
        a.assigned_total
    );
    // Roots: 1 lamport before maturity is rejected even though the linear
    // schedule would allow ~all of E.
    let mut p = pool;
    assert_eq!(
        post_lock_root(&mut p, 1, te - 1, 1, false, 0, te - 1, WINDOW, OFF),
        Err(EngineError::RootExceedsCapacity)
    );
    assert!(scheduled_capacity(B, ts, te, te - 1).unwrap() > a.assigned_total);
    // At maturity: above assigned rejected although <= E.
    assert_eq!(
        post_lock_root(
            &mut p,
            1,
            te,
            a.assigned_total + 1,
            true,
            0,
            te,
            WINDOW,
            OFF
        ),
        Err(EngineError::RootExceedsCapacity)
    );
    post_lock_root(&mut p, 1, te, a.assigned_total, true, 0, te, WINDOW, OFF).unwrap();
    // Future checkpoint.
    let mut p = pool;
    assert_eq!(
        post_lock_root(&mut p, 1, te, 0, true, 0, te - 1, WINDOW, OFF),
        Err(EngineError::FutureCheckpoint)
    );
}

// ------------------------------------------------ safety release gate (37)

#[test]
fn safety_release_disabled_while_dwell_unset() {
    assert_eq!(SafetyRelease::from_dwell(None), SafetyRelease::Disabled);
    assert_eq!(SafetyRelease::from_dwell(Some(0)), SafetyRelease::Disabled);
    assert_eq!(SafetyRelease::from_dwell(Some(-1)), SafetyRelease::Disabled);
    let on = SafetyRelease::from_dwell(Some(3_600));
    assert_eq!(
        on,
        SafetyRelease::Enabled {
            extreme_dwell_seconds: 3_600
        }
    );

    let ts = t_s();
    let half = ts + LOCK_DURATION_SECONDS / 2;
    assert_eq!(
        lock_safety_vest(SafetyRelease::Disabled, 1_000, 2, 2, ts, half),
        Err(EngineError::LockSafetyReleaseDisabled)
    );
    // Enabled: earned-to-date on the last valid surviving weight.
    assert_eq!(lock_safety_vest(on, 1_000, 2, 2, ts, half).unwrap(), 500);
    assert_eq!(lock_safety_vest(on, 1_000, 1, 2, ts, half).unwrap(), 250);
    assert_eq!(lock_safety_vest(on, 1_000, 2, 2, ts, ts - 5).unwrap(), 0);
    assert_eq!(
        lock_safety_vest(on, 1_000, 2, 2, ts, i64::MAX).unwrap(),
        1_000
    );
    assert_eq!(
        lock_safety_vest(on, 1_000, 3, 2, ts, half),
        Err(EngineError::InvalidLockWeight)
    );
    // Checked arithmetic: out-of-domain inputs error instead of panicking.
    assert_eq!(
        lock_safety_vest(on, u64::MAX, u64::MAX, u64::MAX, 0, LOCK_DURATION_SECONDS),
        Err(EngineError::MathOverflow)
    );
    assert_eq!(
        lock_safety_vest(
            on,
            u64::MAX / 5,
            LOCK_KMAX_LAMPORTS,
            LOCK_KMAX_LAMPORTS,
            0,
            LOCK_DURATION_SECONDS
        )
        .unwrap(),
        u64::MAX / 5
    );

    // Pool capacity: disabled -> cliff; enabled -> linear earned-to-date bound.
    let (_, mut pool) = season(B);
    let a = finalized(&mut pool, &[LOCK_KMAX_LAMPORTS, LOCK_KMAX_LAMPORTS]);
    let mid = pool.t_start + LOCK_DURATION_SECONDS / 2;
    assert_eq!(
        lock_capacity(&pool, mid, SafetyRelease::Disabled).unwrap(),
        0
    );
    assert_eq!(lock_capacity(&pool, mid, on).unwrap(), a.assigned_total / 2);
    // Terminal resolution never uses the safety path.
    let mut l = FruitLedger::default();
    assert_eq!(
        terminal_lock_resolution(&mut l, &mut pool, 1, mid, 0, mid),
        Err(EngineError::FruitPoolNotEnded)
    );
}

// ------------------------------------------------ vesting / forfeit (88, 89) and close (P7-L2)

#[test]
fn maturity_vesting_and_forfeit_bounds() {
    assert_eq!(lock_vest_at_maturity(1_000, 2, 2).unwrap(), 1_000);
    assert_eq!(lock_vest_at_maturity(1_000, 1, 3).unwrap(), 333);
    assert_eq!(lock_vest_at_maturity(1_000, 0, 3).unwrap(), 0);
    assert_eq!(
        lock_vest_at_maturity(1_000, 4, 3),
        Err(EngineError::InvalidLockWeight)
    );
    assert_eq!(
        lock_vest_at_maturity(1_000, 0, 0),
        Err(EngineError::InvalidLockWeight)
    );
}

#[test]
fn full_season_lifecycle_releases_once_and_splits_forfeit() {
    let (mut l, mut pool) = season(B);
    let vault = B;
    // Three registrants; wallet 2 halves its weight (voluntary shortfall).
    let ells = [LOCK_KMAX_LAMPORTS, SOL, 500_000_000];
    let (a, regs) = finalized_regs(&mut pool, &ells);
    solvent(vault, &l);
    assert_eq!((l.committed, l.reserve), (B, 0));
    let awards: Vec<u64> = ells
        .iter()
        .map(|&e| lock_award(B, e, a.h_lock).unwrap())
        .collect();
    assert_eq!(awards.iter().sum::<u64>(), a.assigned_total);
    let surv = [ells[0], ells[1] / 2, ells[2]];
    let vested: Vec<u64> = (0..3)
        .map(|i| lock_vest_at_maturity(awards[i], surv[i], ells[i]).unwrap())
        .collect();
    let total: u64 = vested.iter().sum();
    let forfeited: u64 = (0..3).map(|i| awards[i] - vested[i]).sum();
    assert!(forfeited <= a.assigned_total - total);

    let te = pool.t_end;
    // forfeited above assigned - total is rejected (test 89).
    let mut p = pool;
    assert_eq!(
        post_lock_root(
            &mut p,
            1,
            te,
            total,
            true,
            a.assigned_total - total + 1,
            te,
            WINDOW,
            OFF
        ),
        Err(EngineError::InvalidForfeited)
    );
    let mut root =
        post_lock_root(&mut pool, 1, te, total, true, forfeited, te, WINDOW, OFF).unwrap();
    assert_eq!(
        finalize_root(&mut l, &mut pool, &mut root, te + WINDOW).unwrap(),
        total
    );
    solvent(vault, &l);
    let mut vault = vault;
    let mut claimed = [0u64; 3];
    for i in 0..3 {
        // A claim above the wallet's own award is refused (P7-D).
        assert_eq!(
            claim_lock(&mut l, &mut pool, &regs[i], &mut claimed[i], awards[i] + 1),
            Err(EngineError::LockClaimAboveAward)
        );
        // The generic claim path refuses lock pools.
        assert_eq!(
            claim(&mut l, &mut pool, &mut claimed[i], vested[i]),
            Err(EngineError::InvalidLedgerKind)
        );
        let paid = claim_lock(&mut l, &mut pool, &regs[i], &mut claimed[i], vested[i]).unwrap();
        vault -= paid;
        solvent(vault, &l);
    }
    let out = close_pool(&mut l, &mut pool, te + WINDOW).unwrap();
    assert_eq!(out.released, B - total);
    assert_eq!(out.treasury_release, forfeited * 8 / 10);
    vault -= out.treasury_release;
    solvent(vault, &l);
    // Everything not paid and not sent to the treasury is in Z; nothing in U.
    assert_eq!(l.committed, 0);
    assert_eq!(l.liabilities, 0);
    assert_eq!(l.reserve, B - total - out.treasury_release);
    assert_eq!(vault, l.obligations().unwrap());
    // Exactly once.
    assert_eq!(
        close_pool(&mut l, &mut pool, te + WINDOW),
        Err(EngineError::FruitPoolClosed)
    );
    assert_eq!(
        register_lock(&mut pool, SOL, FRUIT_T0),
        Err(EngineError::FruitPoolClosed)
    );
    assert_eq!(
        seal_lock_weights(&mut pool, te + WINDOW),
        Err(EngineError::FruitPoolClosed)
    );
}

#[test]
fn nobody_locking_returns_all_of_b_to_z_at_close() {
    let (mut l, mut pool) = season(B);
    let a = finalized(&mut pool, &[]);
    assert_eq!(a.assigned_total, 0);
    let te = pool.t_end;
    let mut p = pool;
    assert_eq!(
        post_lock_root(&mut p, 1, te, 0, true, 1, te, WINDOW, OFF),
        Err(EngineError::InvalidForfeited)
    );
    let mut r = post_lock_root(&mut pool, 1, te, 0, true, 0, te, WINDOW, OFF).unwrap();
    finalize_root(&mut l, &mut pool, &mut r, te + WINDOW).unwrap();
    let out = close_pool(&mut l, &mut pool, te + WINDOW).unwrap();
    assert_eq!(
        out,
        CloseOutcome {
            released: B,
            treasury_release: 0
        }
    );
    assert_eq!((l.committed, l.reserve), (0, B));
}

#[test]
fn terminal_lock_resolution_is_capped_at_assigned() {
    let (mut l, mut pool) = season(B);
    let a = finalized(&mut pool, &[LOCK_KMAX_LAMPORTS, LOCK_KMAX_LAMPORTS]);
    let te = pool.t_end;
    assert_eq!(
        terminal_lock_resolution(&mut l, &mut pool, 1, te, a.assigned_total + 1, te),
        Err(EngineError::RootExceedsCapacity)
    );
    let (_, d) = terminal_lock_resolution(&mut l, &mut pool, 1, te, a.assigned_total, te).unwrap();
    assert_eq!(d, a.assigned_total);
    assert_eq!(pool.final_forfeited, 0);
    let out = close_pool(&mut l, &mut pool, te).unwrap();
    assert_eq!(out.treasury_release, 0);
    assert_eq!(l.reserve, B - a.assigned_total);
}

// ------------------------------------------------ properties

proptest! {
    #[test]
    fn prop_rounding_bounds(
        budget in 0u64..=u64::MAX,
        ells in prop::collection::vec(1u64..=LOCK_KMAX_LAMPORTS, 0..40),
    ) {
        let sum: u64 = ells.iter().sum();
        let (h, gross) = lock_allocation(budget, sum).unwrap();
        prop_assert!(h >= LOCK_H_FLOOR_LAMPORT_DAYS);
        prop_assert!(gross <= budget);
        let mut s: u128 = 0;
        for &e in &ells {
            let ai = lock_award(budget, e, h).unwrap();
            prop_assert!(ai <= budget / 5);
            s += ai as u128;
        }
        prop_assert!(s <= gross as u128);
        // Dust is strictly below one lamport per wallet.
        prop_assert!((gross as u128) - s < (ells.len() as u128).max(1));
        // Never above the exact rational share.
        prop_assert!((gross as u128) * (h as u128) <= (budget as u128) * (sum as u128 * 5));
    }

    #[test]
    fn prop_season_conserves_and_releases_once(
        budget in 1u64..=100 * SOL,
        ells in prop::collection::vec(1u64..=LOCK_KMAX_LAMPORTS, 0..12),
        keep_bps in prop::collection::vec(0u64..=10_000, 12),
        forfeit_frac in 0u64..=100,
    ) {
        let (mut l, mut pool) = season(budget);
        let (a, regs) = finalized_regs(&mut pool, &ells);
        prop_assert_eq!((l.committed, l.reserve), (budget, 0));
        let mut total = 0u64;
        let mut forfeited = 0u64;
        let mut vested = Vec::new();
        for (i, &e) in ells.iter().enumerate() {
            let ai = lock_award(budget, e, a.h_lock).unwrap();
            let surv = (e as u128 * keep_bps[i] as u128 / 10_000) as u64;
            let v = lock_vest_at_maturity(ai, surv, e).unwrap();
            total += v;
            forfeited += ai - v;
            vested.push(v);
        }
        prop_assert_eq!(total + forfeited, a.assigned_total);
        let te = pool.t_end;
        // Before maturity nothing can be established.
        let mut p = pool;
        if a.assigned_total > 0 {
            prop_assert_eq!(
                post_lock_root(&mut p, 1, te - 1, 1, false, 0, te - 1, WINDOW, OFF),
                Err(EngineError::RootExceedsCapacity)
            );
        }
        // Dust is never forfeitable.
        prop_assert_eq!(
            post_lock_root(&mut pool.clone(), 1, te, total, true, forfeited + 1, te, WINDOW, OFF),
            Err(EngineError::InvalidForfeited)
        );
        let f = forfeited * forfeit_frac / 100;
        let mut r = post_lock_root(&mut pool, 1, te, total, true, f, te, WINDOW, OFF).unwrap();
        finalize_root(&mut l, &mut pool, &mut r, te + WINDOW).unwrap();
        let mut vault = budget;
        prop_assert!(vault >= l.obligations().unwrap());
        for (r, v) in regs.iter().zip(vested) {
            let mut c = 0;
            vault -= claim_lock(&mut l, &mut pool, r, &mut c, v).unwrap();
            prop_assert!(vault >= l.obligations().unwrap());
        }
        let out = close_pool(&mut l, &mut pool, te + WINDOW).unwrap();
        vault -= out.treasury_release;
        prop_assert_eq!(out.released, budget - total);
        prop_assert_eq!(out.treasury_release, f * 8 / 10);
        // Z = unallocated (dust included) + unforfeited-to-treasury remainder.
        prop_assert_eq!(l.reserve, a.unallocated() + (forfeited - f * 8 / 10));
        prop_assert_eq!(vault, l.obligations().unwrap());
        prop_assert_eq!(l.committed, 0);
        prop_assert_eq!(l.liabilities, 0);
        prop_assert!(close_pool(&mut l, &mut pool, te + WINDOW).is_err());
    }
    /// Random registrations, cancellations before t_s, weights <= request and
    /// an optional veto + re-tally: the Final aggregate is exactly the sum of
    /// the active registrations' awards, cancelled registrations hold none,
    /// dust is exact, and the counters match the active set.
    #[test]
    fn prop_tally_with_cancellations_and_veto(
        budget in 1u64..=100 * SOL,
        reqs in prop::collection::vec(1u64..=LOCK_KMAX_LAMPORTS, 0..16),
        cancel_mask in prop::collection::vec(any::<bool>(), 16),
        keep_bps in prop::collection::vec(0u64..=10_000, 16),
        keep2_bps in prop::collection::vec(0u64..=10_000, 16),
        veto in any::<bool>(),
    ) {
        let (_, mut pool) = season(budget);
        let ts = pool.t_start;
        let mut regs = register_all(&mut pool, &reqs);
        for (i, r) in regs.iter_mut().enumerate() {
            if cancel_mask[i] {
                cancel_lock(&mut pool, r, ts - 1).unwrap();
            }
        }
        let active: Vec<usize> = (0..regs.len()).filter(|&i| !cancel_mask[i]).collect();
        prop_assert_eq!(pool.lock.total_reg_count, reqs.len() as u64);
        prop_assert_eq!(pool.lock.active_reg_count, active.len() as u64);
        prop_assert_eq!(
            pool.lock.active_requested_total,
            active.iter().map(|&i| reqs[i]).sum::<u64>()
        );
        let ells = |bps: &[u64]| -> Vec<u64> {
            (0..reqs.len())
                .map(|i| (reqs[i] as u128 * bps[i] as u128 / 10_000) as u64)
                .collect()
        };
        let mut e = ells(&keep_bps);
        tally_at(&mut pool, &mut regs, &e, ts);
        let mut now = ts;
        if veto {
            prop_assert!(veto_lock_allocation(&mut pool, 1, ts + 1).unwrap());
            e = ells(&keep2_bps);
            now = ts + 2;
            tally_at(&mut pool, &mut regs, &e, now);
        }
        let a = finalize_lock_allocation(&mut pool, now + WINDOW).unwrap();
        let act_ells: Vec<u64> = active.iter().map(|&i| e[i]).collect();
        prop_assert_eq!(a.sum_ell, act_ells.iter().sum::<u64>());
        prop_assert_eq!(a.assigned_total, assigned(budget, &act_ells));
        let held: u64 = regs.iter().map(|r| r.award_for(a.generation)).sum();
        prop_assert_eq!(held, a.assigned_total);
        for (i, r) in regs.iter().enumerate() {
            if cancel_mask[i] {
                prop_assert_eq!(r.award_for(a.generation), 0);
            }
        }
        prop_assert_eq!(a.rounding_dust(), a.gross_allocatable - a.assigned_total);
        prop_assert!(a.assigned_total <= a.gross_allocatable && a.gross_allocatable <= budget);
        if a.positive_count > 0 {
            prop_assert!(a.assigned_total + (a.positive_count - 1) >= a.gross_allocatable);
        }
        prop_assert_eq!(lock_final_allocation(&pool).unwrap(), a);
    }
}
