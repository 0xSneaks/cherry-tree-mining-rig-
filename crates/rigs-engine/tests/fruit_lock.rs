//! Lock Season engine (architecture §8 / §10a; Pass #7 decisions P7-L1,
//! P7-L2 and the approved rounding model).
//!
//! Covers the engine parts of tests 50, 53, 77, 88 and 89 and the P7-A
//! invariants:
//!
//! * `sum floor(A_i) <= sum A <= B` and `A_i <= floor(B/5)`;
//! * `H_lock`/`sum A` post only at or after `t_s`, finalize only after the
//!   challenge window, then are immutable;
//! * cliff capacity: nothing before `t_s + 5 days`, exactly `sum A` from it;
//! * safety release unavailable while the EXTREME dwell is unset;
//! * allocation never moves `U`/`Z`; close releases `B - final_total` once;
//! * `V >= L + U + Z` throughout.

use proptest::prelude::*;
use rigs_engine::fruit::claim;
use rigs_engine::*;

const SOL: u64 = 1_000_000_000;
const WINDOW: i64 = 3_600;
const FRUIT_T0: i64 = 1_800_000_000;
const B: u64 = SEASON_BUDGET_LAMPORTS;

fn t_s() -> i64 {
    lock_season_start(FRUIT_T0).unwrap()
}

/// A funded lock ledger with the Season pool committed before `t_s`.
fn season(budget: u64) -> (FruitLedger, FruitPool) {
    let mut l = FruitLedger::default();
    l.fund(budget).unwrap();
    let ts = t_s();
    let p = commit_pool(&mut l, budget, ts, ts + LOCK_DURATION_SECONDS, ts - 1).unwrap();
    (l, p)
}

fn finalized(pool: &mut FruitPool, sum_ell: u64) -> LockAllocation {
    let mut a = LockAllocation::default();
    let ts = pool.t_start;
    post_lock_allocation(pool, &mut a, sum_ell, sum_ell, ts, WINDOW).unwrap();
    finalize_lock_allocation(pool, &mut a, ts + WINDOW).unwrap();
    a
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
    // Nobody locking: H_lock at the floor, sum A = 0 (all of B returns to Z
    // at close).
    assert_eq!(
        lock_allocation(B, 0).unwrap(),
        (LOCK_H_FLOOR_LAMPORT_DAYS, 0)
    );
    // Two registrants, 5 SOL total: H_lock = 62.5, sum A = floor(0.4 B).
    let (h, a) = lock_allocation(B, 5 * SOL).unwrap();
    assert_eq!(h, LOCK_H_FLOOR_LAMPORT_DAYS);
    // 428,082,192 * 0.4 = 171,232,876.8 -> floor.
    assert_eq!(a, B * 2 / 5);
    // Exactly at the floor: all of B.
    assert_eq!(lock_allocation(B, 12_500_000_000).unwrap().1, B);
    // Above the floor: H = 5 sum ell, sum A = B.
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
    // Odd weights: sum floor(A_i) <= sum A, rounding never up.
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
fn only_five_day_lock_windows() {
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
    // A pool with any other window cannot take an allocation.
    let mut l = FruitLedger::default();
    l.fund(B).unwrap();
    let p = commit_pool(&mut l, B, ts, ts + 3 * DAY_SECONDS, ts - 1).unwrap();
    let mut a = LockAllocation::default();
    assert_eq!(
        post_lock_allocation(&p, &mut a, SOL, SOL, ts, WINDOW),
        Err(EngineError::InvalidPoolWindow)
    );
}

#[test]
fn allocation_posts_at_or_after_t_s_and_is_write_once() {
    let (ledger0, mut pool) = season(B);
    let ts = pool.t_start;
    let mut a = LockAllocation::default();
    // Before t_s: the t_s snapshot does not exist.
    assert_eq!(
        post_lock_allocation(&pool, &mut a, SOL, SOL, ts - 1, WINDOW),
        Err(EngineError::FutureCheckpoint)
    );
    // More weight than was registered.
    assert_eq!(
        post_lock_allocation(&pool, &mut a, SOL + 1, SOL, ts, WINDOW),
        Err(EngineError::RootExceedsCapacity)
    );
    assert_eq!(
        post_lock_allocation(&pool, &mut a, SOL, SOL, ts, 0),
        Err(EngineError::InvalidFruitGovernance)
    );
    assert_eq!(a, LockAllocation::default());
    post_lock_allocation(&pool, &mut a, SOL, 2 * SOL, ts, WINDOW).unwrap();
    assert_eq!(a.status, LockAllocationStatus::Pending);
    assert_eq!(a.h_lock, LOCK_H_FLOOR_LAMPORT_DAYS);
    assert_eq!(
        a.allocated,
        (B as u128 * 5 * SOL as u128 / 62_500_000_000) as u64
    );
    // One pending at a time.
    assert_eq!(
        post_lock_allocation(&pool, &mut a, SOL, 2 * SOL, ts, WINDOW),
        Err(EngineError::RootAlreadyPending)
    );
    // Challenge window.
    assert_eq!(
        finalize_lock_allocation(&mut pool, &mut a, ts + WINDOW - 1),
        Err(EngineError::ChallengeWindowOpen)
    );
    // Pending allocation gives no capacity: no root at all.
    let mut p2 = pool;
    assert_eq!(
        post_lock_root(
            &mut p2,
            &a,
            1,
            ts + LOCK_DURATION_SECONDS,
            0,
            true,
            0,
            ts + LOCK_DURATION_SECONDS,
            WINDOW,
            SafetyRelease::Disabled
        ),
        Err(EngineError::InvalidFruitState)
    );
    finalize_lock_allocation(&mut pool, &mut a, ts + WINDOW).unwrap();
    assert_eq!(a.status, LockAllocationStatus::Final);
    assert_eq!(pool.lock_allocated, a.allocated);
    let frozen = a;
    // Immutable: no repost, no refinalize, no veto.
    assert_eq!(
        post_lock_allocation(&pool, &mut a, 2 * SOL, 2 * SOL, ts + WINDOW, WINDOW),
        Err(EngineError::InvalidFruitState)
    );
    assert_eq!(
        finalize_lock_allocation(&mut pool, &mut a, ts + 2 * WINDOW),
        Err(EngineError::RootNotPending)
    );
    assert_eq!(
        veto_lock_allocation(&mut a, 1, ts + WINDOW),
        Err(EngineError::RootNotPending)
    );
    // A forged second Pending allocation cannot overwrite the pool.
    let mut forged = frozen;
    forged.status = LockAllocationStatus::Pending;
    forged.allocated = B;
    assert_eq!(
        finalize_lock_allocation(&mut pool, &mut forged, ts + 2 * WINDOW),
        Err(EngineError::InvalidFruitState)
    );
    assert_eq!(a, frozen);
    assert_eq!(pool.lock_allocated, frozen.allocated);
    // P7-L2: allocation moved nothing between U and Z.
    let (ledger1, _) = season(B);
    assert_eq!(ledger0, ledger1);
    assert_eq!(ledger0.committed, B);
    assert_eq!(ledger0.reserve, 0);
}

#[test]
fn vetoed_allocation_never_finalizes_and_can_be_reposted() {
    let (_, mut pool) = season(B);
    let ts = pool.t_start;
    let mut a = LockAllocation::default();
    post_lock_allocation(&pool, &mut a, 3 * SOL, 3 * SOL, ts, WINDOW).unwrap();
    assert_eq!(veto_lock_allocation(&mut a, 2, ts + 1), Ok(false));
    assert_eq!(veto_lock_allocation(&mut a, 2, ts + 2), Ok(true));
    assert_eq!(a, LockAllocation::default());
    assert_eq!(
        finalize_lock_allocation(&mut pool, &mut a, ts + WINDOW),
        Err(EngineError::RootNotPending)
    );
    assert_eq!(pool.lock_allocated, 0);
    // Veto after the window is refused.
    post_lock_allocation(&pool, &mut a, 2 * SOL, 3 * SOL, ts + 10, WINDOW).unwrap();
    assert_eq!(
        veto_lock_allocation(&mut a, 1, ts + 10 + WINDOW),
        Err(EngineError::ChallengeWindowClosed)
    );
    finalize_lock_allocation(&mut pool, &mut a, ts + 10 + WINDOW).unwrap();
    assert_eq!(a.sum_ell, 2 * SOL);
}

/// Test 53: a data gap at `t_s` means no allocation is posted; the pool then
/// accepts no root, so all of `B_lock` stays in `U`.
#[test]
fn gap_at_t_s_keeps_all_of_b_lock_reserved() {
    let (mut l, mut pool) = season(B);
    let a = LockAllocation::default();
    let te = pool.t_end;
    assert_eq!(
        post_lock_root(
            &mut pool,
            &a,
            1,
            te,
            0,
            true,
            0,
            te,
            WINDOW,
            SafetyRelease::Disabled
        ),
        Err(EngineError::InvalidFruitState)
    );
    assert_eq!(
        terminal_lock_resolution(&mut l, &mut pool, &a, 1, te, 0, te),
        Err(EngineError::InvalidFruitState)
    );
    assert_eq!(
        close_pool(&mut l, &mut pool, te),
        Err(EngineError::FruitPoolNotReconciled)
    );
    assert_eq!((l.committed, l.reserve, l.liabilities), (B, 0, 0));
}

// ------------------------------------------------ cliff capacity

#[test]
fn cliff_capacity_nothing_before_maturity_exactly_sum_a_at_it() {
    let (_, mut pool) = season(B);
    let a = finalized(&mut pool, 5 * SOL);
    let (ts, te) = (pool.t_start, pool.t_end);
    let off = SafetyRelease::Disabled;
    assert_eq!(lock_capacity(&pool, &a, ts, off).unwrap(), 0);
    assert_eq!(lock_capacity(&pool, &a, te - 1, off).unwrap(), 0);
    assert_eq!(lock_capacity(&pool, &a, te, off).unwrap(), a.allocated);
    assert_eq!(
        lock_capacity(&pool, &a, i64::MAX, off).unwrap(),
        a.allocated
    );
    // Roots: 1 lamport before maturity is rejected even though the linear
    // schedule would allow ~all of E.
    let mut p = pool;
    assert_eq!(
        post_lock_root(&mut p, &a, 1, te - 1, 1, false, 0, te - 1, WINDOW, off),
        Err(EngineError::RootExceedsCapacity)
    );
    assert!(scheduled_capacity(B, ts, te, te - 1).unwrap() > a.allocated);
    // At maturity: above sum A rejected although <= E.
    assert_eq!(
        post_lock_root(&mut p, &a, 1, te, a.allocated + 1, true, 0, te, WINDOW, off),
        Err(EngineError::RootExceedsCapacity)
    );
    post_lock_root(&mut p, &a, 1, te, a.allocated, true, 0, te, WINDOW, off).unwrap();
    // Future checkpoint.
    let mut p = pool;
    assert_eq!(
        post_lock_root(&mut p, &a, 1, te, 0, true, 0, te - 1, WINDOW, off),
        Err(EngineError::FutureCheckpoint)
    );
}

#[test]
fn root_capacity_by_ledger_kind() {
    let (_, mut pool) = season(B);
    let a = finalized(&mut pool, 5 * SOL);
    let mid = pool.t_start + LOCK_DURATION_SECONDS / 2;
    let off = SafetyRelease::Disabled;
    assert_eq!(
        root_capacity(LedgerKind::Core, &pool, None, mid, off).unwrap(),
        B / 2
    );
    assert_eq!(
        root_capacity(LedgerKind::Loyalty, &pool, None, mid, off).unwrap(),
        B / 2
    );
    assert_eq!(
        root_capacity(LedgerKind::Lock, &pool, Some(&a), mid, off).unwrap(),
        0
    );
    assert_eq!(
        root_capacity(LedgerKind::Lock, &pool, None, mid, off),
        Err(EngineError::InvalidFruitState)
    );
    assert_eq!(
        root_capacity(LedgerKind::Core, &pool, Some(&a), mid, off),
        Err(EngineError::InvalidLedgerKind)
    );
    // An allocation that does not match the pool's written sum A.
    let mut bad = a;
    bad.allocated += 1;
    assert_eq!(
        root_capacity(LedgerKind::Lock, &pool, Some(&bad), pool.t_end, off),
        Err(EngineError::InvalidFruitState)
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

    // Pool capacity: disabled -> cliff; enabled -> linear earned-to-date bound.
    let (_, mut pool) = season(B);
    let a = finalized(&mut pool, 5 * SOL);
    let mid = pool.t_start + LOCK_DURATION_SECONDS / 2;
    assert_eq!(
        lock_capacity(&pool, &a, mid, SafetyRelease::Disabled).unwrap(),
        0
    );
    assert_eq!(lock_capacity(&pool, &a, mid, on).unwrap(), a.allocated / 2);
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
    let sum: u64 = ells.iter().sum();
    let a = finalized(&mut pool, sum);
    solvent(vault, &l);
    assert_eq!((l.committed, l.reserve), (B, 0));
    let awards: Vec<u64> = ells
        .iter()
        .map(|&e| lock_award(B, e, a.h_lock).unwrap())
        .collect();
    let surv = [ells[0], ells[1] / 2, ells[2]];
    let vested: Vec<u64> = (0..3)
        .map(|i| lock_vest_at_maturity(awards[i], surv[i], ells[i]).unwrap())
        .collect();
    let total: u64 = vested.iter().sum();
    let forfeited: u64 = (0..3).map(|i| awards[i] - vested[i]).sum();
    assert!(awards.iter().sum::<u64>() <= a.allocated);
    assert!(forfeited <= a.allocated - total);

    let te = pool.t_end;
    let off = SafetyRelease::Disabled;
    // forfeited above sum A - total is rejected (test 89).
    let mut p = pool;
    assert_eq!(
        post_lock_root(
            &mut p,
            &a,
            1,
            te,
            total,
            true,
            a.allocated - total + 1,
            te,
            WINDOW,
            off
        ),
        Err(EngineError::InvalidForfeited)
    );
    let mut root = post_lock_root(
        &mut pool, &a, 1, te, total, true, forfeited, te, WINDOW, off,
    )
    .unwrap();
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
    let mut again = LockAllocation::default();
    assert_eq!(
        post_lock_allocation(&pool, &mut again, 0, 0, te + WINDOW, WINDOW),
        Err(EngineError::FruitPoolClosed)
    );
}

#[test]
fn nobody_locking_returns_all_of_b_to_z_at_close() {
    let (mut l, mut pool) = season(B);
    let a = finalized(&mut pool, 0);
    assert_eq!(a.allocated, 0);
    let te = pool.t_end;
    let off = SafetyRelease::Disabled;
    let mut p = pool;
    assert_eq!(
        post_lock_root(&mut p, &a, 1, te, 0, true, 1, te, WINDOW, off),
        Err(EngineError::InvalidForfeited)
    );
    let mut r = post_lock_root(&mut pool, &a, 1, te, 0, true, 0, te, WINDOW, off).unwrap();
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
fn terminal_lock_resolution_is_capped_at_sum_a() {
    let (mut l, mut pool) = season(B);
    let a = finalized(&mut pool, 5 * SOL);
    let te = pool.t_end;
    // The generic path alone would accept up to E.
    let (mut l2, mut p2) = (l, pool);
    assert!(terminal_resolution(&mut l2, &mut p2, 1, te, a.allocated + 1, te).is_ok());
    assert_eq!(
        terminal_lock_resolution(&mut l, &mut pool, &a, 1, te, a.allocated + 1, te),
        Err(EngineError::RootExceedsCapacity)
    );
    let (_, d) = terminal_lock_resolution(&mut l, &mut pool, &a, 1, te, a.allocated, te).unwrap();
    assert_eq!(d, a.allocated);
    assert_eq!(pool.final_forfeited, 0);
}

// ------------------------------------------------ properties

proptest! {
    #[test]
    fn prop_rounding_bounds(
        budget in 0u64..=u64::MAX,
        ells in prop::collection::vec(1u64..=LOCK_KMAX_LAMPORTS, 0..40),
    ) {
        let sum: u64 = ells.iter().sum();
        let (h, a) = lock_allocation(budget, sum).unwrap();
        prop_assert!(h >= LOCK_H_FLOOR_LAMPORT_DAYS);
        prop_assert!(a <= budget);
        let mut s: u128 = 0;
        for &e in &ells {
            let ai = lock_award(budget, e, h).unwrap();
            prop_assert!(ai <= budget / 5);
            s += ai as u128;
        }
        prop_assert!(s <= a as u128);
        // Never above the exact rational share.
        prop_assert!((a as u128) * (h as u128) <= (budget as u128) * (sum as u128 * 5));
    }

    #[test]
    fn prop_season_conserves_and_releases_once(
        budget in 1u64..=100 * SOL,
        ells in prop::collection::vec(1u64..=LOCK_KMAX_LAMPORTS, 0..12),
        keep_bps in prop::collection::vec(0u64..=10_000, 12),
        forfeit_frac in 0u64..=100,
    ) {
        let (mut l, mut pool) = season(budget);
        let sum: u64 = ells.iter().sum();
        let a = finalized(&mut pool, sum);
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
        prop_assert!(total + forfeited <= a.allocated);
        let bound = a.allocated - total;
        prop_assert!(forfeited <= bound);
        let te = pool.t_end;
        let off = SafetyRelease::Disabled;
        // Before maturity nothing can be established.
        let mut p = pool;
        if a.allocated > 0 {
            prop_assert_eq!(
                post_lock_root(&mut p, &a, 1, te - 1, 1, false, 0, te - 1, WINDOW, off),
                Err(EngineError::RootExceedsCapacity)
            );
        }
        let f = forfeited * forfeit_frac / 100;
        let mut r = post_lock_root(&mut pool, &a, 1, te, total, true, f, te, WINDOW, off).unwrap();
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
        prop_assert_eq!(vault, l.obligations().unwrap());
        prop_assert_eq!(l.committed, 0);
        prop_assert_eq!(l.liabilities, 0);
        prop_assert!(close_pool(&mut l, &mut pool, te + WINDOW).is_err());
    }
}
