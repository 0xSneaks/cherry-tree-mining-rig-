//! Lock Season off-chain allocation (Pass #7 P7-E): `t_s` snapshot, surviving
//! low-water mark, vesting / forfeiture, and parity with the on-chain engine
//! (`rigs_engine::fruit_lock`), run end to end through the real tally.

use std::collections::BTreeMap;

use proptest::prelude::*;

use crate::lock_alloc::{snapshot_segments, vesting_segments};
use crate::replay::{IneligibleReason, PendingReason};
use crate::*;
use rigs_engine as rigs;

const A: Addr = Addr::fixture(0x51);
const B: Addr = Addr::fixture(0x52);
const C: Addr = Addr::fixture(0x53);
const D: Addr = Addr::fixture(0x54);
const SOL: u64 = 1_000_000_000;
const TS: i64 = 10_000;
const TE: i64 = TS + lock_alloc::LOCK_DURATION_SECONDS;
const BUDGET: u64 = 428_082_192;

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
        wallets: wallets.iter().copied().collect::<BTreeMap<_, _>>(),
    }
}

fn seeds(ws: &[(Addr, u64)]) -> SeedTimeline {
    let mut s = SeedTimeline::new();
    for (w, n) in ws {
        s.observe(*w, 0, *n).unwrap();
    }
    s
}

fn req(wallet: Addr, requested: u64) -> LockRequest {
    LockRequest { wallet, requested }
}

// ------------------------------------------------------------ t_s snapshot

#[test]
fn snapshot_is_min_of_request_and_recognized_weight() {
    let segs = [
        seg(0, TS, &[(A, ver(u128::from(SOL)))]),
        seg(
            TS,
            TE,
            &[
                (A, ver(u128::from(5 * SOL))),
                (B, ver(u128::from(SOL / 2))),
                (C, ver(u128::from(5 * SOL))),
            ],
        ),
    ];
    // A: 250 seeds -> K = 2.5 SOL, X = 5 -> w = 2.5, requested 2 -> 2.
    // B: X = 0.5 -> w = 0.5, requested 2.5 -> 0.5.
    // C: 160 seeds -> K = 1.6 SOL -> w = 1.6.
    // D: registered for the lock but no Fruit registration in the Season
    //    -> known weight 0.
    let s = seeds(&[(A, 250), (B, 250), (C, 160), (D, 250)]);
    let regs = [
        req(A, 2 * SOL),
        req(B, rigs::LOCK_KMAX_LAMPORTS),
        req(C, rigs::LOCK_KMAX_LAMPORTS),
        req(D, SOL),
    ];
    let got = snapshot_segments(&segs, &s, TS, &regs).unwrap();
    let ells: Vec<u64> = match got {
        LockSnapshot::Ready(v) => v.iter().map(|w| w.ell).collect(),
        p => panic!("{p:?}"),
    };
    assert_eq!(ells, vec![2 * SOL, SOL / 2, 1_600_000_000, 0]);
    // Half-open: the segment ending at t_s does not count, the one starting
    // at t_s does (A's weight is 2 SOL, not 1).
    // Below the seed threshold: weight 0.
    let s = seeds(&[(A, 149), (B, 250), (C, 160), (D, 250)]);
    match snapshot_segments(&segs, &s, TS, &regs).unwrap() {
        LockSnapshot::Ready(v) => assert_eq!(v[0].ell, 0),
        p => panic!("{p:?}"),
    }
}

#[test]
fn snapshot_is_pending_unless_fully_verified() {
    let s = seeds(&[(A, 250), (B, 250)]);
    let regs = [req(A, SOL), req(B, SOL)];
    // Unreconciled segment at t_s: every registrant pending (test 53).
    let mut gap = seg(TS, TE, &[(A, ver(u128::from(SOL)))]);
    gap.status = IntervalStatus::Unresolved;
    assert_eq!(
        snapshot_segments(&[gap], &s, TS, &regs).unwrap(),
        LockSnapshot::Pending(vec![A, B])
    );
    // No segment at t_s at all: pending.
    assert_eq!(
        snapshot_segments(&[seg(0, TS, &[])], &s, TS, &regs).unwrap(),
        LockSnapshot::Pending(vec![A, B])
    );
    // A PENDING registrant, an INELIGIBLE one, a missing seed record.
    let segs = [seg(
        TS,
        TE,
        &[
            (A, Eligibility::Pending(PendingReason::RouteHalted)),
            (
                B,
                Eligibility::Ineligible(IneligibleReason::InactiveNoReference),
            ),
        ],
    )];
    assert_eq!(
        snapshot_segments(&segs, &s, TS, &regs).unwrap(),
        LockSnapshot::Pending(vec![A, B])
    );
    let segs = [seg(TS, TE, &[(A, ver(u128::from(SOL)))])];
    assert_eq!(
        snapshot_segments(&segs, &seeds(&[]), TS, &[req(A, SOL)]).unwrap(),
        LockSnapshot::Pending(vec![A])
    );
    // Invalid requests and duplicates are refused.
    for bad in [
        vec![req(A, 0)],
        vec![req(A, rigs::LOCK_KMAX_LAMPORTS + 1)],
        vec![req(A, SOL), req(A, SOL)],
    ] {
        assert!(snapshot_segments(&segs, &s, TS, &bad).is_err());
    }
}

// ------------------------------------------------------------ vesting

fn award(wallet: Addr, ell: u64, a: u64) -> LockAward {
    LockAward {
        wallet,
        ell,
        award: a,
    }
}

fn ready(v: LockVesting) -> (Vec<LockVest>, u64, u64) {
    match v {
        LockVesting::Ready {
            vests,
            total,
            forfeited,
        } => (vests, total, forfeited),
        p => panic!("{p:?}"),
    }
}

/// Test 50: low-water mark, no resurrection on re-top-up; tests 88/89: the
/// loss is exactly the unvested award of the lost units.
#[test]
fn vesting_uses_the_low_water_mark_without_resurrection() {
    let x = |sol: u64| ver(u128::from(sol) * u128::from(SOL));
    let mid = TS + 100_000;
    let segs = [
        seg(TS, mid, &[(A, x(3)), (B, x(3)), (C, x(3))]),
        // B dips to 1 SOL, then tops back up; C leaves the Fruit selection.
        seg(mid, mid + 10, &[(A, x(3)), (B, x(1))]),
        seg(mid + 10, TE, &[(A, x(3)), (B, x(3))]),
    ];
    let s = seeds(&[(A, 250), (B, 250), (C, 250)]);
    let awards = [
        award(A, 2 * SOL, 1_000),
        award(B, 2 * SOL, 1_000),
        award(C, 2 * SOL, 999),
    ];
    let (vests, total, forfeited) = ready(vesting_segments(&segs, &s, TS, &awards).unwrap());
    assert_eq!(
        vests
            .iter()
            .map(|v| (v.ell_surv, v.vested, v.forfeited))
            .collect::<Vec<_>>(),
        vec![(2 * SOL, 1_000, 0), (SOL, 500, 500), (0, 0, 999)]
    );
    assert_eq!((total, forfeited), (1_500, 1_499));
    assert_eq!(total + forfeited, 2_999, "every assigned lamport accounted");
}

#[test]
fn seed_drop_inside_a_segment_counts() {
    let segs = [seg(TS, TE, &[(A, ver(u128::from(5 * SOL)))])];
    let mut s = SeedTimeline::new();
    s.observe(A, 0, 250).unwrap();
    s.observe(A, TS + 50, 200).unwrap(); // K = 2.0 SOL
    s.observe(A, TS + 60, 250).unwrap(); // restored: no resurrection
    let (v, _, _) =
        ready(vesting_segments(&segs, &s, TS, &[award(A, 2_500_000_000, 1_000)]).unwrap());
    assert_eq!((v[0].ell_surv, v[0].vested), (2 * SOL, 800));
}

#[test]
fn unknown_or_involuntary_history_is_held_not_forfeited() {
    let s = seeds(&[(A, 250), (B, 250)]);
    let awards = [award(A, SOL, 100), award(B, SOL, 100)];
    let ok = ver(u128::from(SOL));
    for e in [
        Eligibility::Pending(PendingReason::RouteHalted),
        Eligibility::Pending(PendingReason::UnresolvedHistory),
        Eligibility::Ineligible(IneligibleReason::InactiveNoReference),
    ] {
        let segs = [
            seg(TS, TS + 5, &[(A, ok), (B, ok)]),
            seg(TS + 5, TE, &[(A, ok), (B, e)]),
        ];
        assert_eq!(
            vesting_segments(&segs, &s, TS, &awards).unwrap(),
            LockVesting::Pending(vec![B])
        );
    }
    let mut gap = seg(TS + 5, TE, &[(A, ok), (B, ok)]);
    gap.status = IntervalStatus::NotReconciled;
    let segs = [seg(TS, TS + 5, &[(A, ok), (B, ok)]), gap];
    assert_eq!(
        vesting_segments(&segs, &s, TS, &awards).unwrap(),
        LockVesting::Pending(vec![A, B])
    );
    // A replay that does not cover the Season is refused.
    let short = [seg(TS, TE - 1, &[(A, ok), (B, ok)])];
    assert!(vesting_segments(&short, &s, TS, &awards).is_err());
    let holed = [seg(TS, TS + 5, &[]), seg(TS + 6, TE, &[])];
    assert!(vesting_segments(&holed, &s, TS, &awards).is_err());
    // Inconsistent award records are refused.
    assert!(vesting_segments(&[seg(TS, TE, &[])], &s, TS, &[award(A, 0, 1)]).is_err());
}

// ------------------------------------------------------------ parity, end to end

/// The snapshot's weights, posted to the real engine tally, produce awards
/// equal to this crate's mirror; the vesting's total and forfeiture are
/// accepted by the on-chain root checks, and the close conserves every
/// lamport with 80% of the forfeiture to the treasury.
#[test]
fn end_to_end_with_the_on_chain_engine() {
    let x = |l: u64| ver(u128::from(l));
    let segs = [
        seg(
            TS,
            TS + 1_000,
            &[
                (A, x(2_500_000_000)),
                (B, x(1_000_000_007)),
                (C, x(700_000_000)),
            ],
        ),
        seg(
            TS + 1_000,
            TE,
            &[
                (A, x(2_500_000_000)),
                (B, x(500_000_000)),
                (C, x(700_000_000)),
            ],
        ),
    ];
    let s = seeds(&[(A, 250), (B, 250), (C, 250)]);
    let regs = [
        req(A, rigs::LOCK_KMAX_LAMPORTS),
        req(B, 1_111_111_111),
        req(C, 777_777_777),
    ];
    let snap = match snapshot_segments(&segs, &s, TS, &regs).unwrap() {
        LockSnapshot::Ready(v) => v,
        p => panic!("{p:?}"),
    };
    // On-chain engine: commit, register, tally with the snapshot, finalize.
    let mut l = rigs::FruitLedger::default();
    l.fund(BUDGET).unwrap();
    let fruit_t0 = TS - rigs::LOCK_SEASON_OFFSET;
    let mut pool = rigs::commit_lock_pool(&mut l, BUDGET, fruit_t0, TS - 1).unwrap();
    let mut chain: Vec<rigs::LockRegistrationState> = regs
        .iter()
        .map(|r| rigs::register_lock(&mut pool, r.requested, fruit_t0).unwrap())
        .collect();
    for (c, w) in chain.iter_mut().zip(&snap) {
        rigs::post_lock_weight(&mut pool, c, w.ell, TS).unwrap();
    }
    rigs::seal_lock_weights(&mut pool, TS).unwrap();
    for c in chain.iter_mut() {
        rigs::award_lock_registration(&mut pool, c).unwrap();
    }
    rigs::seal_lock_awards(&mut pool, TS, 60).unwrap();
    let fin = rigs::finalize_lock_allocation(&mut pool, TS + 60).unwrap();
    // Mirror parity.
    let h = lock_alloc::lock_h(snap.iter().map(|w| w.ell).sum()).unwrap();
    assert_eq!(h, fin.h_lock);
    let awards: Vec<LockAward> = snap
        .iter()
        .zip(&chain)
        .map(|(w, c)| {
            assert_eq!(lock_alloc::lock_award(BUDGET, w.ell, h).unwrap(), c.award);
            award(w.wallet, w.ell, c.award_for(fin.generation))
        })
        .collect();
    // Vesting -> complete root accepted by the engine -> claims -> close.
    let (vests, total, forfeited) = ready(vesting_segments(&segs, &s, TS, &awards).unwrap());
    assert!(forfeited > 0, "B halved: a real shortfall");
    let off = rigs::SafetyRelease::Disabled;
    let mut r =
        rigs::post_lock_root(&mut pool, 1, TE, total, true, forfeited, TE, 60, off).unwrap();
    rigs::finalize_root(&mut l, &mut pool, &mut r, TE + 60).unwrap();
    let mut vault = BUDGET;
    for (v, c) in vests.iter().zip(&chain) {
        let mut claimed = 0;
        vault -= rigs::claim_lock(&mut l, &mut pool, c, &mut claimed, v.vested).unwrap();
    }
    let out = rigs::close_pool(&mut l, &mut pool, TE + 60).unwrap();
    vault -= out.treasury_release;
    assert_eq!(out.treasury_release, forfeited * 8 / 10);
    assert_eq!(vault, l.obligations().unwrap());
    assert_eq!((l.committed, l.liabilities), (0, 0));
}

proptest! {
    /// The mirror award equals the on-chain engine's award everywhere.
    #[test]
    fn award_mirror_matches_engine(
        budget in 0u64..=u64::MAX,
        ells in prop::collection::vec(1u64..=rigs::LOCK_KMAX_LAMPORTS, 1..30),
    ) {
        let sum: u64 = ells.iter().sum();
        let h = lock_alloc::lock_h(sum).unwrap();
        prop_assert_eq!(h, rigs::lock_h(sum).unwrap());
        for &e in &ells {
            prop_assert_eq!(
                lock_alloc::lock_award(budget, e, h).unwrap(),
                rigs::lock_award(budget, e, h).unwrap()
            );
        }
    }

    /// Vesting conserves: sum vested + forfeited = sum A_i, every vest <= A_i,
    /// and the surviving weight never exceeds ell or any verified weight.
    #[test]
    fn vesting_conserves(
        ells in prop::collection::vec(1u64..=rigs::LOCK_KMAX_LAMPORTS, 1..6),
        dips in prop::collection::vec(0u64..=3 * SOL, 6),
        award_scale in 1u64..=1_000_000,
    ) {
        let wallets: Vec<Addr> = (0..ells.len()).map(|i| Addr::fixture(0x60 + i as u8)).collect();
        let full: Vec<(Addr, Eligibility)> =
            wallets.iter().map(|w| (*w, ver(u128::from(3 * SOL)))).collect();
        let dipped: Vec<(Addr, Eligibility)> = wallets
            .iter()
            .zip(&dips)
            .map(|(w, d)| (*w, ver(u128::from(*d))))
            .collect();
        let segs = [seg(TS, TS + 7, &full), seg(TS + 7, TS + 9, &dipped), seg(TS + 9, TE, &full)];
        let s = seeds(&wallets.iter().map(|w| (*w, 250)).collect::<Vec<_>>());
        let awards: Vec<LockAward> = wallets
            .iter()
            .zip(&ells)
            .map(|(w, e)| award(*w, *e, (e / 1_000).max(1) * award_scale / 1_000 + 1))
            .collect();
        let (vests, total, forfeited) = ready(vesting_segments(&segs, &s, TS, &awards).unwrap());
        prop_assert_eq!(total + forfeited, awards.iter().map(|a| a.award).sum::<u64>());
        for ((v, a), d) in vests.iter().zip(&awards).zip(&dips) {
            prop_assert!(v.vested <= a.award);
            prop_assert!(v.ell_surv <= a.ell && v.ell_surv <= (*d).min(rigs::LOCK_KMAX_LAMPORTS));
        }
    }
}
