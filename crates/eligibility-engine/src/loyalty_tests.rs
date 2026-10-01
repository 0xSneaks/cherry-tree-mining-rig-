//! Loyalty (architecture §6; Pass #7 P7-C, DQ-1): tests 47, 48, 49 and the
//! DQ-1 acceptance cases — day boundary at `FRUIT_T0 + 86,400 k`, a
//! partial-day dip, reset vs freeze vs pending, beta at day 10 / 11 / 12,
//! fail-closed while `w_tenure_min` is unset, and the `B_loy` bounds.

use std::collections::BTreeMap;

use proptest::prelude::*;

use crate::loyalty::*;
use crate::replay::{IneligibleReason, PendingReason};
use crate::*;

const A: Addr = Addr::fixture(0x71);
const B: Addr = Addr::fixture(0x72);
const T0F: i64 = 1_000_000;
const DAY: i64 = TENURE_DAY_SECONDS;
const SOL: u128 = 1_000_000_000;
const MIN: u128 = SOL / 10; // fixture threshold: 0.1 SOL-equivalent
const BUDGET: u64 = 100_000_000;

fn ver(x: u128) -> Eligibility {
    Eligibility::Verified {
        units: 1,
        gross_lamports: x,
        debt_lamports: 0,
        x_lamports: x,
    }
}

fn seg(start: i64, end: i64, wallets: &[(Addr, Eligibility)]) -> Segment {
    Segment {
        start,
        end,
        status: IntervalStatus::Verified,
        wallets: wallets.iter().copied().collect(),
    }
}

fn seeds(ws: &[Addr]) -> SeedTimeline {
    let mut s = SeedTimeline::new();
    for w in ws {
        s.observe(*w, 0, 250).unwrap();
    }
    s
}

fn cfg(min: Option<u128>) -> LoyaltyConfig {
    LoyaltyConfig {
        fruit_t0: T0F,
        w_tenure_min: min,
    }
}

fn no_claims() -> BTreeMap<Addr, Vec<i64>> {
    BTreeMap::new()
}

fn schedule(segs: &[Segment], days: i64, claims: &BTreeMap<Addr, Vec<i64>>) -> TenureSchedule {
    crate::loyalty::tenure_segments(
        segs,
        &seeds(&[A, B]),
        &cfg(Some(MIN)),
        MIN,
        T0F,
        T0F + days * DAY,
        claims,
    )
    .unwrap()
}

fn settle(segs: &[Segment], t0: i64, t1: i64, min: Option<u128>) -> LoyaltySettlement {
    crate::loyalty::loyalty_segments(
        BUDGET,
        t0,
        t1,
        segs,
        &seeds(&[A, B]),
        &cfg(min),
        T0F,
        &no_claims(),
    )
    .unwrap()
}

fn leaf(s: &EpochSettlement, w: &Addr) -> u64 {
    s.allocations
        .iter()
        .find(|a| &a.wallet == w)
        .map_or(0, |a| a.leaf_lamports)
}

// ------------------------------------------------------------ fail closed (47)

#[test]
fn inactive_while_threshold_unset() {
    let segs = [seg(T0F, T0F + 20 * DAY, &[(A, ver(2 * SOL))])];
    for min in [None, Some(0)] {
        assert_eq!(
            settle(&segs, T0F + 12 * DAY, T0F + 13 * DAY, min),
            LoyaltySettlement::Inactive
        );
    }
}

/// Test 47: a dust weight below `w_tenure_min` accrues no tenure days, so
/// its beta stays 0 and it earns no loyalty bonus.
#[test]
fn t47_dust_below_threshold_accrues_no_tenure() {
    let segs = [seg(
        T0F,
        T0F + 20 * DAY,
        &[(A, ver(2 * SOL)), (B, ver(MIN - 1))],
    )];
    let s = schedule(&segs, 20, &no_claims());
    assert_eq!(s.days_at(&B, T0F + 20 * DAY), Some(0));
    assert_eq!(s.days_at(&A, T0F + 20 * DAY), Some(20));
    match settle(&segs, T0F + 15 * DAY, T0F + 16 * DAY, Some(MIN)) {
        LoyaltySettlement::Settled(st) => {
            assert_eq!(leaf(&st, &B), 0);
            assert!(leaf(&st, &A) > 0);
        }
        o => panic!("{o:?}"),
    }
}

// ------------------------------------------------------------ DQ-1 grid

#[test]
fn days_step_exactly_at_the_fruit_t0_grid() {
    let segs = [seg(T0F, T0F + 5 * DAY, &[(A, ver(SOL))])];
    let s = schedule(&segs, 5, &no_claims());
    for k in 1..5 {
        assert_eq!(s.days_at(&A, T0F + k * DAY - 1), Some((k - 1) as u64));
        assert_eq!(s.days_at(&A, T0F + k * DAY), Some(k as u64));
    }
    // Misaligned tenure windows are refused.
    assert!(crate::loyalty::tenure_segments(
        &segs,
        &seeds(&[A]),
        &cfg(Some(MIN)),
        MIN,
        T0F + 1,
        T0F + DAY + 1,
        &no_claims()
    )
    .is_err());
}

/// A one-second dip below the threshold anywhere in a day disqualifies the
/// day and (being voluntary) resets tenure.
#[test]
fn partial_day_dip_disqualifies_and_resets() {
    let dip = T0F + 3 * DAY + 40_000;
    let segs = [
        seg(T0F, dip, &[(A, ver(SOL))]),
        seg(dip, dip + 1, &[(A, ver(MIN - 1))]),
        seg(dip + 1, T0F + 6 * DAY, &[(A, ver(SOL))]),
    ];
    let s = schedule(&segs, 6, &no_claims());
    assert_eq!(
        s.outcomes[&A],
        vec![
            DayOutcome::Qualified,
            DayOutcome::Qualified,
            DayOutcome::Qualified,
            DayOutcome::Reset,
            DayOutcome::Qualified,
            DayOutcome::Qualified,
        ]
    );
    assert_eq!(s.days_at(&A, T0F + 6 * DAY), Some(2));
}

// ------------------------------------------------------------ cause table (48, 49)

/// Test 48: a penalized (early-fee) claim resets tenure; an ordinary claim
/// is simply not an input.
#[test]
fn t48_penalized_claim_resets() {
    let segs = [seg(T0F, T0F + 5 * DAY, &[(A, ver(SOL)), (B, ver(SOL))])];
    let mut claims = BTreeMap::new();
    claims.insert(A, vec![T0F + 2 * DAY + 5]);
    let s = schedule(&segs, 5, &claims);
    assert_eq!(s.days_at(&A, T0F + 5 * DAY), Some(2));
    assert_eq!(s.days_at(&B, T0F + 5 * DAY), Some(5));
}

/// Test 49: each cause-table row produces its classification.
#[test]
fn t49_cause_table_reset_freeze_pending() {
    let day3 = T0F + 2 * DAY;
    let mid = day3 + 1_000;
    let run = |e: Option<Eligibility>| {
        let mut mid_seg = vec![(B, ver(SOL))];
        if let Some(e) = e {
            mid_seg.push((A, e));
        }
        let segs = [
            seg(T0F, mid, &[(A, ver(SOL)), (B, ver(SOL))]),
            seg(mid, mid + 10, &mid_seg),
            seg(mid + 10, T0F + 4 * DAY, &[(A, ver(SOL)), (B, ver(SOL))]),
        ];
        let s = schedule(&segs, 4, &no_claims());
        (s.outcomes[&A][2], s.days_at(&A, T0F + 4 * DAY))
    };
    // Voluntary: leaving the selection / deregistering, debt pushing the
    // weight below the threshold.
    assert_eq!(run(None), (DayOutcome::Reset, Some(1)));
    assert_eq!(run(Some(ver(MIN - 1))), (DayOutcome::Reset, Some(1)));
    // Involuntary: freeze (no day added, none erased).
    for e in [
        Eligibility::Pending(PendingReason::RouteHalted),
        Eligibility::Pending(PendingReason::AdapterProgramUnknown),
        Eligibility::Pending(PendingReason::NavUnverified),
        Eligibility::Ineligible(IneligibleReason::InactiveNoReference),
    ] {
        assert_eq!(run(Some(e)), (DayOutcome::Frozen, Some(3)), "{e:?}");
    }
    // Data gap: pending, no tenure until resolved.
    for e in [
        Eligibility::Pending(PendingReason::UnresolvedHistory),
        Eligibility::Pending(PendingReason::NotReconciled),
    ] {
        assert_eq!(run(Some(e)), (DayOutcome::Pending, None), "{e:?}");
    }
    // An unreconciled segment is a gap for everyone.
    let mut gap = seg(mid, mid + 10, &[(A, ver(SOL)), (B, ver(SOL))]);
    gap.status = IntervalStatus::NotReconciled;
    let segs = [
        seg(T0F, mid, &[(A, ver(SOL)), (B, ver(SOL))]),
        gap,
        seg(mid + 10, T0F + 4 * DAY, &[(A, ver(SOL)), (B, ver(SOL))]),
    ];
    let s = schedule(&segs, 4, &no_claims());
    assert_eq!(
        (s.days_at(&A, T0F + 4 * DAY), s.days_at(&B, T0F + 4 * DAY)),
        (None, None)
    );
    assert_eq!(s.days_at(&A, day3), Some(2), "days before the gap stand");
}

// ------------------------------------------------------------ beta 10 / 11 / 12

#[test]
fn beta_at_day_10_11_12() {
    // A joins one day after B; both hold 1 SOL throughout. In day 11 (index
    // 11) B has d = 11 (beta 1) and A has d = 10 (beta 10/11); in day 12
    // both are capped at beta 1.
    let segs = [
        seg(T0F, T0F + DAY, &[(B, ver(SOL))]),
        seg(T0F + DAY, T0F + 14 * DAY, &[(A, ver(SOL)), (B, ver(SOL))]),
    ];
    let one_day = |k: i64| match settle(&segs, T0F + k * DAY, T0F + (k + 1) * DAY, Some(MIN)) {
        LoyaltySettlement::Settled(s) => (leaf(&s, &A), leaf(&s, &B)),
        o => panic!("{o:?}"),
    };
    let (a11, b11) = one_day(11);
    // Floor binds (sum v < 12.5): leaf = B * v / 12.5, floored once more by
    // the fixed-point accumulator (never upward, at most one lamport).
    let exact_b = (u128::from(BUDGET) * SOL / (12 * SOL + SOL / 2)) as u64;
    let exact_a = (u128::from(BUDGET) * 10 * SOL / (11 * (12 * SOL + SOL / 2))) as u64;
    assert!(b11 <= exact_b && exact_b - b11 <= 1, "{b11} vs {exact_b}");
    assert!(a11 <= exact_a && exact_a - a11 <= 1, "{a11} vs {exact_a}");
    let (a12, b12) = one_day(12);
    assert_eq!((a12, b12), (b11, b11), "beta caps at 1 from day 11");
    let (a10, b10) = one_day(10);
    assert_eq!(b10, a11, "B at day 10 equals A at day 11");
    assert!(a10 < a11);
}

/// A qualifying wallet with unknown tenure makes the interval reserve as a
/// whole; nothing settles against an unverified `H_loy`.
#[test]
fn pending_tenure_reserves_the_interval() {
    let mut gap = seg(T0F + DAY, T0F + DAY + 10, &[(A, ver(SOL)), (B, ver(SOL))]);
    gap.status = IntervalStatus::Unresolved;
    let segs = [
        seg(T0F, T0F + DAY, &[(A, ver(SOL)), (B, ver(SOL))]),
        gap,
        seg(
            T0F + DAY + 10,
            T0F + 6 * DAY,
            &[(A, ver(SOL)), (B, ver(SOL))],
        ),
    ];
    match settle(&segs, T0F + 5 * DAY, T0F + 6 * DAY, Some(MIN)) {
        LoyaltySettlement::Settled(s) => {
            assert!(s.allocations.iter().all(|a| a.leaf_lamports == 0));
            assert_eq!(s.reserved_lamports, BUDGET);
            assert!(s.conserves(BUDGET));
        }
        o => panic!("{o:?}"),
    }
}

proptest! {
    /// `sum Q <= B_loy`, `Q_i <= B_loy/5`, conservation, for any weights and
    /// tenure histories.
    #[test]
    fn loyalty_bounds_hold(
        xs in prop::collection::vec(0u128..=5 * SOL, 4),
        join in prop::collection::vec(0i64..12, 4),
        k in 0i64..15,
    ) {
        let ws: Vec<Addr> = (0..4).map(|i| Addr::fixture(0x80 + i as u8)).collect();
        let mut segs = Vec::new();
        for d in 0..16 {
            let present: Vec<(Addr, Eligibility)> = ws
                .iter()
                .zip(&xs)
                .zip(&join)
                .filter(|(_, j)| **j <= d)
                .map(|((w, x), _)| (*w, ver(*x)))
                .collect();
            segs.push(seg(T0F + d * DAY, T0F + (d + 1) * DAY, &present));
        }
        let s = crate::loyalty::loyalty_segments(
            BUDGET,
            T0F + k * DAY,
            T0F + (k + 1) * DAY,
            &segs,
            &seeds(&ws),
            &cfg(Some(MIN)),
            T0F,
            &no_claims(),
        )
        .unwrap();
        let LoyaltySettlement::Settled(s) = s else { panic!() };
        prop_assert!(s.conserves(BUDGET));
        prop_assert!(s.allocated_lamports <= BUDGET);
        for a in &s.allocations {
            prop_assert!(a.leaf_lamports <= BUDGET / 5);
        }
    }
}
