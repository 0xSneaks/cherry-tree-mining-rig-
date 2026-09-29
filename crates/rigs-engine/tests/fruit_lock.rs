//! Lock Season engine (architecture §8 / §10a; Pass #7 decisions P7-L1,
//! P7-L2, F7 and the approved rounding model).
//!
//! Covers the engine parts of tests 50, 53, 77, 88 and 89 and the P7-A
//! invariants:
//!
//! * `assigned_total <= gross_allocatable <= B` and `A_i <= floor(B/5)`;
//! * the allocation lifecycle lives on the pool: post only at or after
//!   `t_s`, veto retires the exact Pending generation, finalize only the
//!   pool's own Pending commitment after the challenge window, then Final
//!   and immutable (a Final zero included);
//! * cliff capacity: nothing before `t_s + 5 days`, exactly `assigned_total`
//!   from it;
//! * the generic root/terminal paths refuse lock pools;
//! * rounding dust is unallocated (100% to `Z`), never forfeiture (F7);
//! * safety release unavailable while the EXTREME dwell is unset;
//! * allocation never moves `U`/`Z`; close releases `B - final_total` once;
//! * `V >= L + U + Z` throughout.
//!
//! The `p*` / `m*` tests reproduce the Pass #7-A verification probes and
//! surviving mutations and pin their fixes.

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

/// A funded lock ledger with Season pool `id` committed before `t_s`.
fn season_id(budget: u64, id: u64) -> (FruitLedger, FruitPool) {
    let mut l = FruitLedger::default();
    l.fund(budget).unwrap();
    let p = commit_lock_pool(&mut l, id, budget, FRUIT_T0, t_s() - 1).unwrap();
    (l, p)
}

fn season(budget: u64) -> (FruitLedger, FruitPool) {
    season_id(budget, 1)
}

/// Sum of the wallet awards `sum_i floor(B u_i / H_lock)` for `ells`.
fn assigned(budget: u64, ells: &[u64]) -> u64 {
    let sum: u64 = ells.iter().sum();
    let (h, _) = lock_allocation(budget, sum).unwrap();
    ells.iter()
        .map(|&e| lock_award(budget, e, h).unwrap())
        .sum()
}

/// Post and finalize an allocation of `ells` (exact wallet awards).
fn finalized(pool: &mut FruitPool, ells: &[u64]) -> LockAllocation {
    let sum: u64 = ells.iter().sum();
    let ts = pool.t_start;
    let a = post_lock_allocation(pool, sum, sum, assigned(pool.budget, ells), ts, WINDOW).unwrap();
    finalize_lock_allocation(pool, &a, ts + WINDOW).unwrap()
}

fn solvent(vault: u64, l: &FruitLedger) {
    assert!(vault >= l.obligations().unwrap(), "V < L + U + Z");
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
    assert_eq!(lock_award(B, 0, h), Err(EngineError::InvalidFruitState));
    assert_eq!(
        lock_award(B, LOCK_KMAX_LAMPORTS + 1, h),
        Err(EngineError::InvalidFruitState)
    );
    assert_eq!(
        lock_award(B, 1, LOCK_H_FLOOR_LAMPORT_DAYS - 1),
        Err(EngineError::InvalidFruitState)
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
    // commit_lock_pool derives the window from FRUIT_T0.
    let (_, p) = season(B);
    assert_eq!(p.kind, LedgerKind::Lock);
    assert_eq!((p.t_start, p.t_end), (ts, ts + LOCK_DURATION_SECONDS));
    // It must be committed before t_s.
    let mut l = FruitLedger::default();
    l.fund(B).unwrap();
    assert_eq!(
        commit_lock_pool(&mut l, 1, B, FRUIT_T0, ts),
        Err(EngineError::FruitPoolStarted)
    );
    assert_eq!((l.committed, l.reserve), (0, B));
    // A lock pool whose window was tampered cannot take an allocation.
    let (_, mut p) = season(B);
    p.t_end = ts + 3 * DAY_SECONDS;
    assert_eq!(
        post_lock_allocation(&mut p, SOL, SOL, 0, ts, WINDOW),
        Err(EngineError::InvalidPoolWindow)
    );
    // A core pool is not a lock pool.
    let mut l = FruitLedger::default();
    l.fund(B).unwrap();
    let mut core = commit_pool(&mut l, B, ts, ts + LOCK_DURATION_SECONDS, ts - 1).unwrap();
    assert_eq!(
        post_lock_allocation(&mut core, SOL, SOL, 0, ts, WINDOW),
        Err(EngineError::InvalidLedgerKind)
    );
}

#[test]
fn allocation_posts_at_or_after_t_s_and_is_write_once() {
    let (ledger0, mut pool) = season(B);
    let ts = pool.t_start;
    let before = pool;
    // Before t_s: the t_s snapshot does not exist.
    assert_eq!(
        post_lock_allocation(&mut pool, SOL, SOL, 0, ts - 1, WINDOW),
        Err(EngineError::FutureCheckpoint)
    );
    // More weight than was registered.
    assert_eq!(
        post_lock_allocation(&mut pool, SOL + 1, SOL, 0, ts, WINDOW),
        Err(EngineError::RootExceedsCapacity)
    );
    assert_eq!(
        post_lock_allocation(&mut pool, SOL, SOL, 0, ts, 0),
        Err(EngineError::InvalidFruitGovernance)
    );
    assert_eq!(pool, before);
    let gross = (B as u128 * 5 * SOL as u128 / 62_500_000_000) as u64;
    let a = post_lock_allocation(&mut pool, SOL, 2 * SOL, gross, ts, WINDOW).unwrap();
    assert_eq!(pool.lock.status, LockAllocationStatus::Pending);
    assert_eq!(pool.lock.allocation, a);
    assert_eq!(a.generation, 1);
    assert_eq!(a.h_lock, LOCK_H_FLOOR_LAMPORT_DAYS);
    assert_eq!(a.gross_allocatable, gross);
    // One pending at a time.
    assert_eq!(
        post_lock_allocation(&mut pool, SOL, 2 * SOL, 0, ts, WINDOW),
        Err(EngineError::RootAlreadyPending)
    );
    // Challenge window.
    assert_eq!(
        finalize_lock_allocation(&mut pool, &a, ts + WINDOW - 1),
        Err(EngineError::ChallengeWindowOpen)
    );
    // Pending allocation gives no capacity: no root at all.
    let mut p2 = pool;
    assert_eq!(
        post_lock_root(
            &mut p2,
            1,
            ts + LOCK_DURATION_SECONDS,
            0,
            true,
            0,
            ts + LOCK_DURATION_SECONDS,
            WINDOW,
            OFF
        ),
        Err(EngineError::InvalidFruitState)
    );
    let f = finalize_lock_allocation(&mut pool, &a, ts + WINDOW).unwrap();
    assert_eq!(f, a);
    assert_eq!(pool.lock.status, LockAllocationStatus::Final);
    let frozen = pool;
    // Immutable: no repost, no refinalize, no veto.
    assert_eq!(
        post_lock_allocation(&mut pool, 2 * SOL, 2 * SOL, 0, ts + WINDOW, WINDOW),
        Err(EngineError::InvalidFruitState)
    );
    assert_eq!(
        finalize_lock_allocation(&mut pool, &a, ts + 2 * WINDOW),
        Err(EngineError::RootNotPending)
    );
    assert_eq!(
        veto_lock_allocation(&mut pool, &a, 1, ts + WINDOW),
        Err(EngineError::RootNotPending)
    );
    assert_eq!(pool, frozen);
    // P7-L2: the allocation API never takes the ledger; U/Z are as committed.
    assert_eq!((ledger0.committed, ledger0.reserve), (B, 0));
}

#[test]
fn allocation_after_a_root_is_refused() {
    // Lock pools post roots only against a Final allocation, so a root
    // before the allocation can only be tampered state; refuse it.
    let (_, mut pool) = season(B);
    pool.latest_version = 1;
    let ts = pool.t_start;
    assert_eq!(
        post_lock_allocation(&mut pool, SOL, SOL, 0, ts, WINDOW),
        Err(EngineError::InvalidFruitState)
    );
}

#[test]
fn vetoed_allocation_never_finalizes_and_can_be_reposted() {
    let (_, mut pool) = season(B);
    let ts = pool.t_start;
    let a = post_lock_allocation(&mut pool, 3 * SOL, 3 * SOL, 0, ts, WINDOW).unwrap();
    assert_eq!(veto_lock_allocation(&mut pool, &a, 2, ts + 1), Ok(false));
    assert_eq!(pool.lock.veto_count, 1);
    assert_eq!(veto_lock_allocation(&mut pool, &a, 2, ts + 2), Ok(true));
    assert_eq!(pool.lock.status, LockAllocationStatus::Uninitialized);
    assert_eq!(pool.lock.allocation, LockAllocation::default());
    assert_eq!(pool.lock.generation, 1, "generation retired, not reused");
    assert_eq!(
        finalize_lock_allocation(&mut pool, &a, ts + WINDOW),
        Err(EngineError::RootNotPending)
    );
    // Repost: a fresh generation. Veto after the window is refused.
    let b = post_lock_allocation(&mut pool, 2 * SOL, 3 * SOL, 0, ts + 10, WINDOW).unwrap();
    assert_eq!(b.generation, 2);
    assert_eq!(
        veto_lock_allocation(&mut pool, &b, 1, ts + 10 + WINDOW),
        Err(EngineError::ChallengeWindowClosed)
    );
    assert_eq!(
        veto_lock_allocation(&mut pool, &b, 0, ts + 11),
        Err(EngineError::InvalidFruitGovernance)
    );
    finalize_lock_allocation(&mut pool, &b, ts + 10 + WINDOW).unwrap();
    assert_eq!(pool.lock.allocation.sum_ell, 2 * SOL);
}

/// Test 53: a data gap at `t_s` means no allocation is posted; the pool then
/// accepts no root, so all of `B_lock` stays in `U`.
#[test]
fn gap_at_t_s_keeps_all_of_b_lock_reserved() {
    let (mut l, mut pool) = season(B);
    let te = pool.t_end;
    assert_eq!(
        post_lock_root(&mut pool, 1, te, 0, true, 0, te, WINDOW, OFF),
        Err(EngineError::InvalidFruitState)
    );
    assert_eq!(
        terminal_lock_resolution(&mut l, &mut pool, 1, te, 0, te),
        Err(EngineError::InvalidFruitState)
    );
    assert_eq!(
        close_pool(&mut l, &mut pool, te),
        Err(EngineError::FruitPoolNotReconciled)
    );
    assert_eq!((l.committed, l.reserve, l.liabilities), (B, 0, 0));
}

// ------------------------------------------------ verification regressions

/// P1: a Final zero allocation stays Final; nothing can overwrite it.
#[test]
fn p1_finalized_zero_allocation_cannot_be_overwritten() {
    let (_, mut pool) = season(B);
    let ts = pool.t_start;
    let z = finalized(&mut pool, &[]);
    assert_eq!((z.gross_allocatable, z.assigned_total), (0, 0));
    assert_eq!(pool.lock.status, LockAllocationStatus::Final);
    let frozen = pool;
    // Every path to a second allocation is refused.
    assert_eq!(
        post_lock_allocation(&mut pool, 5 * SOL, 5 * SOL, 0, ts + WINDOW, WINDOW),
        Err(EngineError::InvalidFruitState)
    );
    let mut forged = z;
    forged.sum_ell = 5 * SOL;
    forged.gross_allocatable = B * 2 / 5;
    forged.assigned_total = B * 2 / 5;
    assert_eq!(
        finalize_lock_allocation(&mut pool, &forged, ts + 2 * WINDOW),
        Err(EngineError::RootNotPending)
    );
    assert_eq!(
        finalize_lock_allocation(&mut pool, &z, ts + 2 * WINDOW),
        Err(EngineError::RootNotPending)
    );
    assert_eq!(pool, frozen);
    assert_eq!(lock_final_allocation(&pool).unwrap(), z);
}

/// P2: a fabricated "Final, 0" object has no path in: roots, terminal
/// resolution and close read only the pool's own lifecycle.
#[test]
fn p2_fabricated_final_zero_cannot_post_resolve_or_release() {
    let (mut l, mut pool) = season(B);
    let te = pool.t_end;
    // The public lock API takes no allocation object at all for roots,
    // terminal resolution or close; the pool is Uninitialized.
    assert_eq!(pool.lock.status, LockAllocationStatus::Uninitialized);
    assert_eq!(
        post_lock_root(&mut pool, 1, te, 0, true, 0, te, WINDOW, OFF),
        Err(EngineError::InvalidFruitState)
    );
    assert_eq!(
        terminal_lock_resolution(&mut l, &mut pool, 1, te, 0, te),
        Err(EngineError::InvalidFruitState)
    );
    // Nor can the fabricated object be finalized into the pool.
    let forged = LockAllocation {
        pool_id: 1,
        generation: 1,
        budget: B,
        h_lock: LOCK_H_FLOOR_LAMPORT_DAYS,
        ..LockAllocation::default()
    };
    assert_eq!(
        finalize_lock_allocation(&mut pool, &forged, te),
        Err(EngineError::RootNotPending)
    );
    // Tampering only the status (no committed allocation) is caught too.
    let mut t = pool;
    t.lock.status = LockAllocationStatus::Final;
    assert_eq!(
        post_lock_root(&mut t, 1, te, 0, true, 0, te, WINDOW, OFF),
        Err(EngineError::InvalidFruitState)
    );
    assert_eq!(
        terminal_lock_resolution(&mut l, &mut t, 1, te, 0, te),
        Err(EngineError::InvalidFruitState)
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
    // Each award floors to 0; together they floor to gross = 1.
    let ells = [1_000_000u64, 999_999];
    let sum: u64 = ells.iter().sum();
    let (_, gross) = lock_allocation(budget, sum).unwrap();
    assert_eq!(assigned(budget, &ells), 0);
    assert_eq!(gross, 1);
    let te = pool.t_end;
    let a = finalized(&mut pool, &ells);
    assert_eq!((a.gross_allocatable, a.assigned_total), (1, 0));
    assert_eq!(a.rounding_dust(), 1);
    assert_eq!(pool.lock.status, LockAllocationStatus::Final);
    assert_eq!(
        post_lock_allocation(&mut pool, sum, sum, 0, te, WINDOW),
        Err(EngineError::InvalidFruitState)
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

/// P6: an allocation posted on pool A cannot finalize (or veto) pool B, even
/// with identical budgets, timing and allocation content.
#[test]
fn p6_allocation_from_pool_a_cannot_finalize_into_pool_b() {
    let (_, mut pool_a) = season_id(B, 7);
    let (_, mut pool_b) = season_id(B, 8);
    let ts = pool_a.t_start;
    assert_eq!(
        (pool_a.t_start, pool_a.budget),
        (pool_b.t_start, pool_b.budget)
    );
    let a = post_lock_allocation(&mut pool_a, SOL, SOL, 0, ts, WINDOW).unwrap();
    // B has nothing pending.
    assert_eq!(
        finalize_lock_allocation(&mut pool_b, &a, ts + WINDOW),
        Err(EngineError::RootNotPending)
    );
    // B has its own, otherwise identical, pending allocation.
    let b = post_lock_allocation(&mut pool_b, SOL, SOL, 0, ts, WINDOW).unwrap();
    assert_eq!(
        LockAllocation { pool_id: 0, ..a },
        LockAllocation { pool_id: 0, ..b }
    );
    assert_eq!(
        finalize_lock_allocation(&mut pool_b, &a, ts + WINDOW),
        Err(EngineError::RootNotPending)
    );
    assert_eq!(
        veto_lock_allocation(&mut pool_b, &a, 1, ts + 1),
        Err(EngineError::RootNotPending)
    );
    assert_eq!(pool_b.lock.status, LockAllocationStatus::Pending);
    // Each pool finalizes only its own.
    finalize_lock_allocation(&mut pool_b, &b, ts + WINDOW).unwrap();
    finalize_lock_allocation(&mut pool_a, &a, ts + WINDOW).unwrap();
}

/// P11: a copy of the Pending allocation taken before a veto cannot finalize
/// afterwards — not while the pool is Uninitialized, and not after a repost
/// of the very same content.
#[test]
fn p11_stale_pending_copy_cannot_finalize_after_veto() {
    let (_, mut pool) = season(B);
    let ts = pool.t_start;
    let a = post_lock_allocation(&mut pool, SOL, SOL, 0, ts, WINDOW).unwrap();
    let stale = a;
    assert!(veto_lock_allocation(&mut pool, &a, 1, ts + 1).unwrap());
    assert_eq!(
        finalize_lock_allocation(&mut pool, &stale, ts + WINDOW),
        Err(EngineError::RootNotPending)
    );
    // Same content reposted at the same time: new generation, stale refused.
    let b = post_lock_allocation(&mut pool, SOL, SOL, 0, ts, WINDOW).unwrap();
    assert_eq!(
        LockAllocation {
            generation: 0,
            ..stale
        },
        LockAllocation { generation: 0, ..b }
    );
    assert_ne!(stale.generation, b.generation);
    assert_eq!(
        finalize_lock_allocation(&mut pool, &stale, ts + WINDOW),
        Err(EngineError::RootNotPending)
    );
    assert_eq!(
        veto_lock_allocation(&mut pool, &stale, 1, ts + 2),
        Err(EngineError::RootNotPending)
    );
    assert_eq!(pool.lock.status, LockAllocationStatus::Pending);
    finalize_lock_allocation(&mut pool, &b, ts + WINDOW).unwrap();
}

/// M13: finalization revalidates every committed field against the pool —
/// budget, identity, generation, recomputed `H_lock`/gross, and
/// `assigned_total <= gross`.
#[test]
fn m13_finalize_validates_budget_and_committed_fields() {
    let (_, pool0) = season(B);
    let ts = pool0.t_start;
    let tamper = |f: &dyn Fn(&mut LockAllocation)| {
        let mut pool = pool0;
        post_lock_allocation(&mut pool, 5 * SOL, 5 * SOL, B / 4, ts, WINDOW).unwrap();
        f(&mut pool.lock.allocation);
        let expected = pool.lock.allocation;
        let before = pool;
        let r = finalize_lock_allocation(&mut pool, &expected, ts + WINDOW);
        if r.is_err() {
            assert_eq!(pool, before, "a refused finalize changes nothing");
        }
        r
    };
    let bad = Err(EngineError::InvalidFruitState);
    assert_eq!(tamper(&|a| a.budget = B + 1), bad);
    assert_eq!(tamper(&|a| a.budget = B - 1), bad);
    assert_eq!(tamper(&|a| a.pool_id = 2), bad);
    assert_eq!(tamper(&|a| a.generation = 0), bad);
    assert_eq!(tamper(&|a| a.h_lock += 1), bad);
    assert_eq!(tamper(&|a| a.gross_allocatable += 1), bad);
    assert_eq!(tamper(&|a| a.assigned_total = a.gross_allocatable + 1), bad);
    assert_eq!(tamper(&|a| a.requested_total = a.sum_ell - 1), bad);
    // The pool budget itself changing is caught the same way.
    let mut pool = pool0;
    let a = post_lock_allocation(&mut pool, 5 * SOL, 5 * SOL, B / 4, ts, WINDOW).unwrap();
    pool.budget += 1;
    assert_eq!(finalize_lock_allocation(&mut pool, &a, ts + WINDOW), bad);
    // Untampered succeeds.
    assert!(tamper(&|_| {}).is_ok());
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
        LockAllocationStatus::Pending,
        LockAllocationStatus::Uninitialized,
    ] {
        let mut p = pool;
        p.lock.status = status;
        let mut l = l0;
        assert_eq!(
            lock_capacity(&p, te, OFF),
            Err(EngineError::InvalidFruitState)
        );
        assert_eq!(
            post_lock_root(&mut p, 1, te, 0, true, 0, te, WINDOW, OFF),
            Err(EngineError::InvalidFruitState)
        );
        assert_eq!(
            terminal_lock_resolution(&mut l, &mut p, 1, te, 0, te),
            Err(EngineError::InvalidFruitState)
        );
    }
    // A Final pool closes only while it is still Final.
    let mut l = l0;
    let mut r = post_lock_root(&mut pool, 1, te, 0, true, 0, te, WINDOW, OFF).unwrap();
    finalize_root(&mut l, &mut pool, &mut r, te + WINDOW).unwrap();
    let mut p = pool;
    p.lock.status = LockAllocationStatus::Pending;
    assert_eq!(
        close_pool(&mut l.clone(), &mut p, te + WINDOW),
        Err(EngineError::InvalidFruitState)
    );
    // A Final allocation whose committed fields no longer revalidate.
    let mut p = pool;
    p.lock.allocation.assigned_total = p.lock.allocation.gross_allocatable + 1;
    assert_eq!(
        close_pool(&mut l.clone(), &mut p, te + WINDOW),
        Err(EngineError::InvalidFruitState)
    );
    close_pool(&mut l, &mut pool, te + WINDOW).unwrap();
}

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
    let a = finalized(&mut pool, &ells);
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
    vault -= claim(&mut l, &mut pool, &mut c, total / 10).unwrap();
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
        Err(EngineError::InvalidConfig)
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
        Err(EngineError::InvalidFruitState)
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
        Err(EngineError::InvalidFruitState)
    );
    assert_eq!(
        lock_vest_at_maturity(1_000, 0, 0),
        Err(EngineError::InvalidFruitState)
    );
}

#[test]
fn full_season_lifecycle_releases_once_and_splits_forfeit() {
    let (mut l, mut pool) = season(B);
    let vault = B;
    // Three registrants; wallet 2 halves its weight (voluntary shortfall).
    let ells = [LOCK_KMAX_LAMPORTS, SOL, 500_000_000];
    let a = finalized(&mut pool, &ells);
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
        let paid = claim(&mut l, &mut pool, &mut claimed[i], vested[i]).unwrap();
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
        post_lock_allocation(&mut pool, 0, 0, 0, te + WINDOW, WINDOW),
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
        let a = finalized(&mut pool, &ells);
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
        for v in vested {
            let mut c = 0;
            vault -= claim(&mut l, &mut pool, &mut c, v).unwrap();
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
}
