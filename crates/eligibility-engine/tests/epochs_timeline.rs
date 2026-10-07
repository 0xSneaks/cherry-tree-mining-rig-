//! Canonical tests 40, 42–46 (architecture §14.3, "Epochs and timeline").
//! All addresses, budgets, timestamps and weights are FIXTURES (gate
//! 14.2-D); nothing here is a production calibration.
//!
//! Tests 40 and 42–43 exercise the rigs-engine fruit schedule planner;
//! tests 44–46 exercise this crate's settlement. Alongside the canonical
//! six, the Pass #6 frozen architecture decisions are pinned executably:
//!
//! * P6-D3 — one Fruit per wallet per epoch; a switch (including after an
//!   exit) is effective NEXT epoch, never within the current one;
//! * P6-D6 — an unverified shared denominator reserves the whole interval:
//!   ANY seed-qualifying PENDING wallet (a route halt included) holds the
//!   segment for everyone; a sub-threshold (`S < 150`) pending wallet
//!   cannot change `H` and does not block;
//! * P6-D7 — route halts persist across the epoch boundary until a
//!   `RouteResumed` event, and resumption is not retroactive;
//! * P6-D5 — settlement invariants under random partitions (conservation,
//!   flooring, cap, checkpoint-frequency independence within the frozen
//!   sub-lamport drift, no redistribution).
//!
//! Test 41 is the Lock registration-window cutoff and is DEFERRED OUTSIDE
//! PASS #6 to the Lock package (P6-D2); no Lock registration-window
//! behavior or test lives here. The Season start `t_s` as a half-open
//! boundary is already asserted by test 42.

mod common;

use common::*;
use eligibility_engine::*;
use rigs_engine as rigs;
use std::collections::BTreeMap;

// ------------------------------------------------------------------ 40

/// TEST 40 — Day-1 / Fruit activation. At `T0+86,399` the Day-1 `.skr`
/// window still governs purchases (genesis cap, `.skr` identity) and NO
/// Fruit accrual exists; at `T0+86,400` — `FRUIT_T0`, the boundary instant
/// belonging to the later interval — access is public for everyone and
/// Storm accrual begins.
#[test]
fn t40_day1_cap_before_fruit_t0_public_phase_and_storm_accrual_at_fruit_t0() {
    let t0 = 1_700_000_000i64;
    let ft0 = rigs::fruit_t0(t0);
    assert_eq!(ft0, t0 + 86_400);

    // One second before public opening: still the .skr Genesis window.
    let just_before = t0 + 86_399;
    assert_eq!(
        rigs::access_phase(t0, just_before),
        rigs::AccessPhase::SkrGenesis
    );
    assert_eq!(rigs::seconds_to_next_phase(t0, just_before), Some(1));
    // The Day-1 cumulative cap binds at the last Genesis second...
    assert!(rigs::genesis_cap_check(t0, just_before, 250, 1, 250).is_err());
    // ...and the very same request is fine one second later, in Public.
    assert!(rigs::genesis_cap_check(t0, ft0, 250, 1, 250).is_ok());
    // Only a proven .skr identity buys during Day-1; Public needs nothing.
    assert!(rigs::purchase_allowed(
        rigs::AccessPhase::SkrGenesis,
        rigs::ProvenIdentity::Skr
    ));
    assert!(!rigs::purchase_allowed(
        rigs::AccessPhase::SkrGenesis,
        rigs::ProvenIdentity::None
    ));

    // No Fruit accrual exists before FRUIT_T0 (fixture funding above R,
    // so Storm is the first epoch).
    let plan = rigs::plan_fruit(t0, rigs::CONTINUATION_BUDGET_R + 1_000_000_000);
    assert!(plan.epoch_at(t0).is_none());
    assert!(plan.epoch_at(just_before).is_none());
    assert!(!plan.accrual_active(just_before));

    // At FRUIT_T0 exactly: public access, and Storm accrual BEGINS — the
    // boundary instant belongs to the later interval.
    assert_eq!(rigs::access_phase(t0, ft0), rigs::AccessPhase::Public);
    assert!(rigs::purchase_allowed(
        rigs::AccessPhase::Public,
        rigs::ProvenIdentity::None
    ));
    let storm = plan.epoch_at(ft0).expect("Storm owns FRUIT_T0");
    assert_eq!(storm.kind, rigs::EpochKind::Storm);
    assert_eq!(storm.t_start, ft0);
    assert!(storm.contains(ft0));
}

// ------------------------------------------------------------------ 42

/// TEST 42 — every canonical boundary at 86,400 / 259,200 / 518,400 /
/// 950,400 from FRUIT_T0 is half-open: the instant belongs to the LATER
/// interval, with no ambiguous inclusive/exclusive behavior.
#[test]
fn t42_core_epoch_boundaries_are_half_open() {
    let t0 = 1_700_000_000i64;
    let ft0 = rigs::fruit_t0(t0);
    let plan = rigs::plan_fruit(t0, rigs::CONTINUATION_BUDGET_R + 5_000_000_000);

    for (offset, before, after) in [
        (
            86_400i64,
            rigs::EpochKind::Storm,
            Some(rigs::EpochKind::Harvest),
        ),
        (
            259_200,
            rigs::EpochKind::Harvest,
            Some(rigs::EpochKind::Superfruit),
        ),
        (
            518_400,
            rigs::EpochKind::Superfruit,
            Some(rigs::EpochKind::Season),
        ),
        (950_400, rigs::EpochKind::Season, None),
    ] {
        // Immediately before: the earlier interval still owns the instant.
        assert_eq!(
            plan.epoch_at(ft0 + offset - 1).map(|e| e.kind),
            Some(before),
            "just before +{offset}"
        );
        // Exactly at the boundary: the LATER interval owns it.
        assert_eq!(
            plan.epoch_at(ft0 + offset).map(|e| e.kind),
            after,
            "at +{offset}"
        );
        // Immediately after: unambiguously the later interval (or past the
        // core end, where nothing accrues).
        let next = plan.epoch_at(ft0 + offset + 1).map(|e| e.kind);
        match after {
            Some(k) => assert_eq!(next, Some(k), "just after +{offset}"),
            None => assert!(next.is_none(), "just after the core end"),
        }
    }

    // Every epoch window is [start, end): its own start is inside, its own
    // end is not.
    for e in &plan.epochs {
        assert!(e.contains(e.t_start), "{:?}", e.kind);
        assert!(!e.contains(e.t_end), "{:?}", e.kind);
    }

    // The fallback plan's single boundary behaves the same way: its end
    // instant belongs to nothing.
    let fb = rigs::plan_fruit(t0, rigs::CONTINUATION_BUDGET_R - 1);
    assert_eq!(
        fb.epoch_at(ft0 + 172_799).map(|e| e.kind),
        Some(rigs::EpochKind::Harvest)
    );
    assert!(fb.epoch_at(ft0 + 172_800).is_none());

    // At F = R the zero-budget Storm is omitted and its window belongs to
    // no epoch, while the continuation keeps its published offsets.
    let eq = rigs::plan_fruit(t0, rigs::CONTINUATION_BUDGET_R);
    assert!(eq.epoch_at(ft0 + 43_200).is_none());
    assert_eq!(
        eq.epoch_at(ft0 + 86_400).map(|e| e.kind),
        Some(rigs::EpochKind::Harvest)
    );
}

// ------------------------------------------------------------------ 43

/// TEST 43 — funding branches: `F > R` (below and above the Storm
/// ceiling), `F = R` (Storm emits zero; offsets retained), `0 < F < R`
/// (including the canonical `F = R - 1` lamport: one 48-hour fallback
/// Harvest of `F`, no Season or lock) and `F = 0` (inactive). The planner
/// never generates an invalid zero-budget pool, and funding is conserved.
#[test]
fn t43_funding_branches_f_r_boundary_states_and_conservation() {
    let t0 = 1_700_000_000i64;
    let ft0 = rigs::fruit_t0(t0);
    let r = rigs::CONTINUATION_BUDGET_R;

    // F = 0: Fruit remains inactive.
    let p = rigs::plan_fruit(t0, 0);
    assert_eq!(p.kind, rigs::PlanKind::Inactive);
    assert!(p.epochs.is_empty());
    assert_eq!((p.total_committed(), p.z_future), (0, 0));

    // 0 < F < R — the canonical F = R - 1 lamport: ONE 48-hour Harvest of
    // exactly F beginning at FRUIT_T0; no four-stage sequence, no Season,
    // no lock anywhere in the plan.
    let p = rigs::plan_fruit(t0, r - 1);
    assert_eq!(p.kind, rigs::PlanKind::FallbackHarvest);
    assert_eq!(p.epochs.len(), 1);
    assert_eq!(p.epochs[0].kind, rigs::EpochKind::Harvest);
    assert_eq!(
        (p.epochs[0].t_start, p.epochs[0].t_end),
        (ft0, ft0 + 172_800)
    );
    assert_eq!(p.epochs[0].budget, r - 1);
    assert!(!p.epochs.iter().any(|e| e.kind == rigs::EpochKind::Season));

    // F = R: Storm emits zero — the (invalid) zero-budget pool is omitted
    // entirely — and the continuation retains its published offsets.
    let p = rigs::plan_fruit(t0, r);
    assert_eq!(p.kind, rigs::PlanKind::CoreSequence);
    assert!(!p.epochs.iter().any(|e| e.kind == rigs::EpochKind::Storm));
    assert_eq!(p.epochs.len(), 3);
    assert_eq!(p.total_committed(), r);
    assert_eq!(p.z_future, 0);
    assert_eq!(p.epochs[0].t_start, ft0 + 86_400);
    assert_eq!(p.epochs.last().unwrap().t_end, ft0 + 950_400);

    // F > R below the Storm ceiling: E_storm = F - R, nothing beyond.
    let p = rigs::plan_fruit(t0, r + 2_500_000_000);
    assert_eq!(p.epochs[0].kind, rigs::EpochKind::Storm);
    assert_eq!(p.epochs[0].budget, 2_500_000_000);
    assert_eq!(p.epochs[0].t_start, ft0);
    assert_eq!(p.z_future, 0);

    // F > R above the ceiling: E_storm is capped, the excess is Z_future.
    let p = rigs::plan_fruit(t0, r + rigs::STORM_CEILING_LAMPORTS + 7);
    assert_eq!(p.epochs[0].budget, rigs::STORM_CEILING_LAMPORTS);
    assert_eq!(p.z_future, 7);
    assert_eq!(p.epochs.len(), 4);

    // Conservation and validity across every branch (F >= R is the
    // canonical grouped branch, with F = R inside it).
    for f in [
        0,
        1,
        r - 1,
        r,
        r + 1,
        r + rigs::STORM_CEILING_LAMPORTS,
        r + rigs::STORM_CEILING_LAMPORTS + 123_456,
    ] {
        let p = rigs::plan_fruit(t0, f);
        assert!(
            p.epochs.iter().all(|e| e.budget > 0),
            "zero-budget epoch at F = {f}"
        );
        rigs::validate_plan(&p).unwrap();
        if f >= r {
            assert_eq!(p.total_committed() + p.z_future, f, "F = {f}");
        } else {
            assert_eq!((p.total_committed(), p.z_future), (f, 0), "F = {f}");
        }
    }
}

// ------------------------------------------------------------------ 44

fn part(wallet: Addr, seeds: u64, x: u128) -> Participant {
    Participant {
        wallet,
        seeds,
        x_lamports: x,
    }
}

fn interval(
    start: i64,
    end: i64,
    verdict: IntervalVerdict,
    parts: Vec<Participant>,
) -> EpochInterval {
    EpochInterval {
        start,
        end,
        verdict,
        participants: parts,
    }
}

fn leaf_of(s: &EpochSettlement, wallet: &Addr) -> u64 {
    s.allocations
        .iter()
        .find(|a| &a.wallet == wallet)
        .map(|a| a.leaf_lamports)
        .unwrap_or(0)
}

/// TEST 44 — zero-participant epoch: no allocation at all, every lamport of
/// capacity returns `U → Z` at close, and the pool APR is explicitly N/A
/// (`None`), never a fabricated number.
#[test]
fn t44_zero_participant_epoch_all_capacity_to_z_and_apr_is_none() {
    let e = 684_931_507u64; // Harvest-budget fixture
    let (t_start, t_end) = (1_000i64, 1_000 + 172_800);

    // A whole epoch with no participants at all.
    let s = settle_epoch(
        e,
        t_start,
        t_end,
        &[interval(t_start, t_end, IntervalVerdict::Settle, vec![])],
    )
    .unwrap();
    assert!(s.allocations.is_empty());
    assert_eq!(s.allocated_lamports, 0);
    assert_eq!((s.reserved_lamports, s.not_earned_lamports), (0, 0));
    assert_eq!(s.unallocated_lamports, e); // all capacity U → Z at close
    assert!(s.conserves(e));
    assert!(s.apr.pool.is_none());
    assert_eq!(s.apr.cn_lamports, 0);

    // Same outcome with wallets present but none qualifying: below the
    // 150-seed threshold a wallet is weight 0 and not active — Cn = 0 and
    // the pool APR is still N/A (§1: "With zero participants, show N/A and
    // zero accrual").
    let s = settle_epoch(
        e,
        t_start,
        t_end,
        &[interval(
            t_start,
            t_end,
            IntervalVerdict::Settle,
            vec![part(A, 149, 10_000_000_000), part(B, 10, 0)],
        )],
    )
    .unwrap();
    assert!(s.allocations.is_empty());
    assert_eq!(s.allocated_lamports, 0);
    assert_eq!(s.unallocated_lamports, e);
    assert!(s.conserves(e));
    assert!(s.apr.pool.is_none());
}

// ------------------------------------------------------------------ 45

/// TEST 45 — low denominator: with `Cw < 12.5` SOL-equivalent the canonical
/// denominator floor applies, each wallet receives exactly its
/// `E · w/H` share floored, and the unallocated remainder returns `U → Z` —
/// it is never redistributed among the active wallets.
#[test]
fn t45_low_denominator_floor_unallocated_to_z_never_redistributed() {
    let e = 1_000_000_007u64;
    let (t_start, t_end) = (0i64, 86_400);
    // Cw = 1.5 + 2.5 + 0.2 = 4.2 SOL-equivalent < 12.5 → H = the floor.
    let wallets = [
        (A, 150u64, 1_500_000_000u128, 1_500_000_000u128), // K = P·150
        (B, 250, 2_500_000_000, 2_500_000_000),            // at the cap
        (C, 200, 200_000_000, 200_000_000),                // dust equity
    ];
    let parts: Vec<Participant> = wallets.iter().map(|&(w, s, x, _)| part(w, s, x)).collect();
    let s = settle_epoch(
        e,
        t_start,
        t_end,
        &[interval(t_start, t_end, IntervalVerdict::Settle, parts)],
    )
    .unwrap();
    assert_eq!(s.apr.cw, 4_200_000_000);
    assert_eq!(s.apr.h, DENOMINATOR_FLOOR);

    // Each leaf is exactly floor(E · w/12.5 SOL) — nobody receives any of
    // anyone else's unallocated capacity — and every wallet is inside the
    // E/5 ceiling (B exactly at it).
    for &(w, _, _, weight) in &wallets {
        let expect = (u128::from(e) * weight / DENOMINATOR_FLOOR) as u64;
        assert_eq!(leaf_of(&s, &w), expect, "{w:?}");
        assert!(leaf_of(&s, &w) <= e / 5, "{w:?}");
    }
    assert_eq!(leaf_of(&s, &B), e / 5);
    let allocated: u64 = wallets.iter().map(|&(w, _, _, _)| leaf_of(&s, &w)).sum();
    assert_eq!(s.allocated_lamports, allocated);
    assert_eq!(s.unallocated_lamports, e - allocated); // → Z at close
    assert!(s.conserves(e));

    // No redistribution, executably: in the floor regime H does not depend
    // on who else participates, so each wallet ALONE in the same epoch
    // receives exactly the leaf it receives in company.
    for &(w, s_seeds, x, _) in &wallets {
        let solo = settle_epoch(
            e,
            t_start,
            t_end,
            &[interval(
                t_start,
                t_end,
                IntervalVerdict::Settle,
                vec![part(w, s_seeds, x)],
            )],
        )
        .unwrap();
        assert_eq!(leaf_of(&solo, &w), leaf_of(&s, &w), "{w:?} solo vs company");
    }
}

// ------------------------------------------------------------------ 46

/// TEST 46 — outage distinction (P6-D10). A registration-program outage
/// (only finalized on-chain Fruit program state: the canonical pause flag,
/// or the program absent/non-executable) makes the interval **not earned**:
/// its capacity is released to the reserve at close. A data-only outage
/// (RPC/indexer/frontend/operator/backend, per-ledger pause flags) leaves
/// the interval **earned**: capacity is held reserved in `U` and settles
/// when the replay completes — the replayed total equals the never-broken
/// control exactly.
#[test]
fn t46_registration_program_outage_not_earned_data_outage_earned_and_replayed() {
    // (1) Classification: exactly the three on-chain forms are
    // registration-program outages; everything else is a data outage.
    assert_eq!(
        classify_outage(OutageEvidence::RegistrationPauseFlag),
        OutageClass::RegistrationProgram
    );
    assert_eq!(
        classify_outage(OutageEvidence::FruitProgramAbsent),
        OutageClass::RegistrationProgram
    );
    assert_eq!(
        classify_outage(OutageEvidence::FruitProgramNonExecutable),
        OutageClass::RegistrationProgram
    );
    for e in [
        OutageEvidence::RpcOutage,
        OutageEvidence::IndexerOutage,
        OutageEvidence::FrontendOutage,
        OutageEvidence::OperatorReport,
        OutageEvidence::BackendFailure,
        OutageEvidence::PerLedgerPauseFlag,
    ] {
        assert_eq!(classify_outage(e), OutageClass::Data, "{e:?}");
    }

    // A three-interval epoch (Superfruit-shaped fixture; E divides evenly
    // into thirds) with the middle interval hit by an outage.
    let e = 513_698_631u64;
    let (t0, t1) = (0i64, 259_200);
    let third = 86_400;
    let parts = || vec![part(A, 250, 2_500_000_000), part(B, 200, 1_600_000_000)];
    let middle_cap = (u128::from(e) * (2 * third as u128) / (t1 - t0) as u128
        - u128::from(e) * third as u128 / (t1 - t0) as u128) as u64;

    // Control: the never-broken epoch.
    let control = settle_epoch(
        e,
        t0,
        t1,
        &[
            interval(t0, third, IntervalVerdict::Settle, parts()),
            interval(third, 2 * third, IntervalVerdict::Settle, parts()),
            interval(2 * third, t1, IntervalVerdict::Settle, parts()),
        ],
    )
    .unwrap();

    // (2) Registration-program outage over the middle interval: NOT
    // earned. Nobody accrues during it, and its scheduled capacity is
    // released (U → Z at close), not held.
    let program = settle_epoch(
        e,
        t0,
        t1,
        &[
            interval(t0, third, IntervalVerdict::Settle, parts()),
            interval(third, 2 * third, IntervalVerdict::NotEarned, vec![]),
            interval(2 * third, t1, IntervalVerdict::Settle, parts()),
        ],
    )
    .unwrap();
    assert_eq!(program.not_earned_lamports, middle_cap);
    assert_eq!(program.reserved_lamports, 0);
    assert!(program.conserves(e));
    assert_eq!(
        leaf_of(&program, &A) + leaf_of(&program, &B),
        program.allocated_lamports
    );

    // (3) Data-only outage over the same interval: earned, so its capacity
    // is HELD in U pending replay — and allocations still exclude it
    // (never settle against an unverified denominator, P6-D6). The
    // distinction is explicit: same timeline, different evidence,
    // different accounting.
    let data = settle_epoch(
        e,
        t0,
        t1,
        &[
            interval(t0, third, IntervalVerdict::Settle, parts()),
            interval(third, 2 * third, IntervalVerdict::Reserve, vec![]),
            interval(2 * third, t1, IntervalVerdict::Settle, parts()),
        ],
    )
    .unwrap();
    assert_eq!(data.reserved_lamports, middle_cap);
    assert_eq!(data.not_earned_lamports, 0);
    assert!(data.conserves(e));
    // While data is missing, both outage classes pay the same outer
    // accrual — the middle interval settles for nobody either way.
    for w in [&A, &B] {
        assert_eq!(leaf_of(&data, w), leaf_of(&program, w));
        assert!(leaf_of(&data, w) < leaf_of(&control, w));
    }

    // (4) Replayed when data becomes available: repairing the interval
    // settles it, and the result equals the never-broken control exactly —
    // earned, reserved, replayed.
    let replayed = settle_epoch(
        e,
        t0,
        t1,
        &[
            interval(t0, third, IntervalVerdict::Settle, parts()),
            interval(third, 2 * third, IntervalVerdict::Settle, parts()),
            interval(2 * third, t1, IntervalVerdict::Settle, parts()),
        ],
    )
    .unwrap();
    assert_eq!(replayed.reserved_lamports, 0);
    assert_eq!(replayed.allocated_lamports, control.allocated_lamports);
    for w in [&A, &B] {
        assert_eq!(leaf_of(&replayed, w), leaf_of(&control, w));
    }
    assert_eq!(replayed.unallocated_lamports, control.unallocated_lamports);
}

// ------------------------------------------------- P6-D6 (frozen decision)

/// P6-D6 / P6R-3 — EVERY wallet-level PENDING blocks the interval for
/// everyone, `RouteHalted` included (P6-D6 item 5: Pass #6 adds no
/// resolution semantics for specific Pending reasons — they stay reserved
/// until the eligibility engine resolves them). A seed-qualifying halted
/// wallet's weight is UNKNOWN (`w ∈ [0, K]`), so `H` is unverified and the
/// halted wallet's share is never redistributed to the others. The one
/// non-blocking shape is a PENDING wallet BELOW the seed threshold
/// (`S < 150`): its weight is 0 by definition and it cannot change `H`
/// (P6R-7).
#[test]
fn p6d6_any_qualifying_pending_wallet_reserves_the_whole_interval() {
    let mk = |elig, b_seeds: Option<u64>| {
        let mut wallets = BTreeMap::new();
        wallets.insert(
            A,
            Eligibility::Verified {
                units: 100,
                gross_lamports: 200,
                debt_lamports: 0,
                x_lamports: 200,
            },
        );
        wallets.insert(B, elig);
        let mut seeds = BTreeMap::new();
        seeds.insert(A, 250u64);
        if let Some(s) = b_seeds {
            seeds.insert(B, s);
        }
        (
            seeds,
            Segment {
                start: 0,
                end: 100,
                status: IntervalStatus::Verified,
                wallets,
            },
        )
    };

    // EVERY Pending reason blocks when the wallet is seed-qualified —
    // RouteHalted included: a halt is not a silent weight-0 carve-out.
    for reason in [
        PendingReason::RouteHalted,
        PendingReason::Debt(crate::debt::DebtPending::Unreadable),
        PendingReason::DebtUnpriced,
        PendingReason::AdapterProgramUnknown,
        PendingReason::StakeUnverified,
        PendingReason::AdditionUnpriced,
        PendingReason::NavUnverified,
        PendingReason::UnresolvedHistory,
        PendingReason::NotReconciled,
    ] {
        let (seeds, seg) = mk(Eligibility::Pending(reason), Some(250));
        assert_eq!(
            classify_segment(&seg, &seeds),
            IntervalVerdict::Reserve,
            "{reason:?}"
        );
    }

    // A PENDING wallet below the seed threshold cannot change H and does
    // NOT block (P6R-7, both sides of the boundary).
    for (s, expect) in [
        (149u64, IntervalVerdict::Settle),
        (150, IntervalVerdict::Reserve),
    ] {
        let (seeds, seg) = mk(Eligibility::Pending(PendingReason::RouteHalted), Some(s));
        assert_eq!(classify_segment(&seg, &seeds), expect);
        let (seeds, seg) = mk(
            Eligibility::Pending(PendingReason::Debt(crate::debt::DebtPending::Unreadable)),
            Some(s),
        );
        assert_eq!(classify_segment(&seg, &seeds), expect);
    }
    // A PENDING wallet with no seed record at all (unknown seeds = 0 to
    // the weight equation) does not block either.
    let (seeds, seg) = mk(Eligibility::Pending(PendingReason::RouteHalted), None);
    assert_eq!(classify_segment(&seg, &seeds), IntervalVerdict::Settle);

    // An INELIGIBLE wallet is known weight 0: never a blocker (test 32).
    let (seeds, seg) = mk(
        Eligibility::Ineligible(IneligibleReason::InactiveNoReference),
        Some(250),
    );
    assert_eq!(classify_segment(&seg, &seeds), IntervalVerdict::Settle);
    let inputs = weight_inputs(&seg, &seeds);
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].wallet, A);

    // Unreconciled / unresolved history (an indexer gap, or the
    // post-checkpoint tail) also reserves: earned, held, replayed (§3).
    for status in [IntervalStatus::Unresolved, IntervalStatus::NotReconciled] {
        let (seeds, mut seg) = mk(Eligibility::Pending(PendingReason::RouteHalted), Some(149));
        seg.status = status;
        assert_eq!(
            classify_segment(&seg, &seeds),
            IntervalVerdict::Reserve,
            "{status:?}"
        );
    }
}

// ------------------------------------------------- P6R-3/P6R-7 (remediation R1, D6)

/// P6R-3 — executable consequence: a route-halted QUALIFYING wallet holds
/// the ENTIRE interval reserved — nobody settles, nobody receives the
/// halted wallet's share — while the identical interval without the halted
/// wallet settles normally. The reserve persists until the halt resolves;
/// there is no same-interval redistribution, ever.
#[test]
fn p6r3_route_halted_qualifying_wallet_holds_the_segment_for_everyone() {
    let e = 684_931_507u64; // Harvest-budget fixture
    let (t_start, t_end) = (0i64, 86_400);

    // The same epoch settles normally when the halted wallet is simply
    // absent (it is not registered in the segment).
    let plain = settle_epoch(
        e,
        t_start,
        t_end,
        &[interval(
            t_start,
            t_end,
            IntervalVerdict::Settle,
            vec![part(A, 250, 2_500_000_000)],
        )],
    )
    .unwrap();
    assert!(plain.allocated_lamports > 0);
    assert_eq!(plain.reserved_lamports, 0);

    // With the qualifying halted wallet present the interval reserves for
    // EVERYONE: nothing allocated, the full scheduled capacity held in U,
    // conservation intact. A's leaf is zero — it does not absorb B's share.
    let held = settle_epoch(
        e,
        t_start,
        t_end,
        &[interval(t_start, t_end, IntervalVerdict::Reserve, vec![])],
    )
    .unwrap();
    assert_eq!(held.allocated_lamports, 0);
    assert_eq!(held.reserved_lamports, e);
    assert!(held.conserves(e));
    assert_eq!(leaf_of(&held, &A), 0);

    // And the shape the preserved implementation allowed — other wallets
    // settling against H computed without the halted wallet — is exactly
    // what classify_segment now refuses to produce.
    let mut wallets = BTreeMap::new();
    wallets.insert(
        A,
        Eligibility::Verified {
            units: 100,
            gross_lamports: 2_500_000_000,
            debt_lamports: 0,
            x_lamports: 2_500_000_000,
        },
    );
    wallets.insert(B, Eligibility::Pending(PendingReason::RouteHalted));
    let seg = Segment {
        start: t_start,
        end: t_end,
        status: IntervalStatus::Verified,
        wallets,
    };
    let mut seeds = BTreeMap::new();
    seeds.insert(A, 250u64);
    seeds.insert(B, 250u64);
    assert_eq!(classify_segment(&seg, &seeds), IntervalVerdict::Reserve);
}

/// P6R-7 — boundary around the qualification threshold: a PENDING wallet
/// with fewer than 150 seeds reserves NOTHING (its weight is 0 by
/// definition, `H` is unaffected), at exactly 150 it does. The other
/// wallets' leaves are identical with and without a sub-threshold pending
/// wallet present.
#[test]
fn p6r7_subthreshold_pending_wallet_cannot_halt_settlement() {
    let e = 684_931_507u64;
    let (t_start, t_end) = (0i64, 86_400);

    let mk = |b_seeds: u64, expect: IntervalVerdict| {
        let mut wallets = BTreeMap::new();
        wallets.insert(
            A,
            Eligibility::Verified {
                units: 100,
                gross_lamports: 2_500_000_000,
                debt_lamports: 0,
                x_lamports: 2_500_000_000,
            },
        );
        wallets.insert(
            B,
            Eligibility::Pending(PendingReason::Debt(crate::debt::DebtPending::Unreadable)),
        );
        let seg = Segment {
            start: t_start,
            end: t_end,
            status: IntervalStatus::Verified,
            wallets,
        };
        let mut seeds = BTreeMap::new();
        seeds.insert(A, 250u64);
        seeds.insert(B, b_seeds);
        assert_eq!(classify_segment(&seg, &seeds), expect, "B seeds {b_seeds}");
    };
    // Both sides of the threshold, and the immediate neighborhood.
    for s in [0u64, 1, 100, 148, 149] {
        mk(s, IntervalVerdict::Settle);
    }
    for s in [150u64, 151, 250, 10_000] {
        mk(s, IntervalVerdict::Reserve);
    }

    // Executable: A's leaf is IDENTICAL whether the sub-threshold pending
    // wallet is present or not, and nothing is reserved on its account.
    let solo = settle_epoch(
        e,
        t_start,
        t_end,
        &[interval(
            t_start,
            t_end,
            IntervalVerdict::Settle,
            vec![part(A, 250, 2_500_000_000)],
        )],
    )
    .unwrap();
    let with_pending = settle_epoch(
        e,
        t_start,
        t_end,
        &[interval(
            t_start,
            t_end,
            IntervalVerdict::Settle,
            vec![part(A, 250, 2_500_000_000), part(B, 149, 9_000_000_000)],
        )],
    )
    .unwrap();
    assert_eq!(leaf_of(&with_pending, &A), leaf_of(&solo, &A));
    assert_eq!(with_pending.reserved_lamports, 0);
    assert!(with_pending.conserves(e));
}

// ------------------------------------------------- P6R-4 (remediation R1, D10)

/// P6R-4 — terminal "not earned" treatment requires canonical evidence,
/// and it can never erase accrual that a finalized registration already
/// established (P6-D10). Operator/indexer input is a DATA outage: earned,
/// reserved, replayable. The three finalized on-chain forms alone are
/// registration-program evidence, and even they only apply to a RECONCILED
/// segment holding NO finalized registration — wallets already registered
/// keep accruing through a registration-program outage.
#[test]
fn p6r4_not_earned_requires_canonical_evidence_and_registered_accrual_survives() {
    use OutageEvidence as E;

    let verified_with_registration = Segment {
        start: 0,
        end: 100,
        status: IntervalStatus::Verified,
        wallets: BTreeMap::from([(
            A,
            Eligibility::Verified {
                units: 100,
                gross_lamports: 2_500_000_000,
                debt_lamports: 0,
                x_lamports: 2_500_000_000,
            },
        )]),
    };
    let verified_empty = Segment {
        start: 0,
        end: 100,
        status: IntervalStatus::Verified,
        wallets: BTreeMap::new(),
    };
    let unreconciled_with_registration = Segment {
        status: IntervalStatus::NotReconciled,
        ..verified_with_registration.clone()
    };
    let unreconciled_empty = Segment {
        status: IntervalStatus::NotReconciled,
        ..verified_empty.clone()
    };
    let seeds = BTreeMap::from([(A, 250u64)]);

    // (1) Caller-supplied DATA labels can NEVER declare earned capacity
    // not earned — not over a registered segment, not anywhere.
    for e in [
        E::OperatorReport,
        E::IndexerOutage,
        E::RpcOutage,
        E::BackendFailure,
    ] {
        assert_eq!(
            classify_interval(&verified_with_registration, &seeds, Some(e)),
            IntervalVerdict::Settle,
            "{e:?}: registered accrual must survive a data-outage label"
        );
        assert_eq!(
            classify_interval(&verified_empty, &seeds, Some(e)),
            IntervalVerdict::Settle,
            "{e:?}"
        );
    }

    // (2) The canonical on-chain forms over a registered segment: the
    // wallets were already registered, so they keep accruing (P6-D10
    // item 3) — the interval settles, it does not become not-earned.
    for e in [
        E::RegistrationPauseFlag,
        E::FruitProgramAbsent,
        E::FruitProgramNonExecutable,
    ] {
        assert_eq!(
            classify_interval(&verified_with_registration, &seeds, Some(e)),
            IntervalVerdict::Settle,
            "{e:?}: already-registered wallets keep accruing"
        );
        // Over a reconciled, registration-free segment: not earned.
        assert_eq!(
            classify_interval(&verified_empty, &seeds, Some(e)),
            IntervalVerdict::NotEarned,
            "{e:?}"
        );
        // Guard 2: an unreconciled window is Pending even for canonical
        // evidence — absence of an OBSERVED registration is not absence.
        assert_eq!(
            classify_interval(&unreconciled_empty, &seeds, Some(e)),
            IntervalVerdict::Reserve,
            "{e:?}"
        );
    }

    // (3) Indexer-only misses stay replayable/reservable (test 46's
    // wallet-B story): any outage label over unreconciled history reserves.
    for e in [
        E::IndexerOutage,
        E::OperatorReport,
        E::RegistrationPauseFlag,
    ] {
        assert_eq!(
            classify_interval(&unreconciled_with_registration, &seeds, Some(e)),
            IntervalVerdict::Reserve,
            "{e:?}"
        );
    }
    // No evidence at all: the plain P6-D6 rule.
    assert_eq!(
        classify_interval(&verified_with_registration, &seeds, None),
        IntervalVerdict::Settle
    );
    assert_eq!(
        classify_interval(&unreconciled_with_registration, &seeds, None),
        IntervalVerdict::Reserve
    );

    // (4) The economics layer refuses the erase: an already-finalized
    // registration cannot be removed by a caller-supplied outage label
    // passed straight to settle_epoch (the preserved implementation
    // accepted it and silently dropped the wallets' accrual).
    let err = settle_epoch(
        513_698_631,
        0,
        259_200,
        &[
            interval(
                0,
                86_400,
                IntervalVerdict::Settle,
                vec![part(A, 250, 2_500_000_000)],
            ),
            interval(
                86_400,
                172_800,
                IntervalVerdict::NotEarned,
                vec![part(A, 250, 2_500_000_000)],
            ),
            interval(
                172_800,
                259_200,
                IntervalVerdict::Settle,
                vec![part(A, 250, 2_500_000_000)],
            ),
        ],
    );
    assert!(
        err.is_err(),
        "not-earned label over registered wallets must be rejected"
    );

    // The legitimate shape still works: an empty reconciled segment with
    // canonical evidence releases its capacity as not-earned (test 46's
    // accounting, unchanged).
    let s = settle_epoch(
        513_698_631,
        0,
        259_200,
        &[
            interval(
                0,
                86_400,
                IntervalVerdict::Settle,
                vec![part(A, 250, 2_500_000_000)],
            ),
            interval(86_400, 172_800, IntervalVerdict::NotEarned, vec![]),
            interval(
                172_800,
                259_200,
                IntervalVerdict::Settle,
                vec![part(A, 250, 2_500_000_000)],
            ),
        ],
    )
    .unwrap();
    assert!(s.not_earned_lamports > 0);
    assert!(s.conserves(513_698_631));
}

// ------------------------------------------------- P6-D3 (frozen decision)

fn two_asset_cfg() -> EligibilityConfig {
    config(
        vec![skr_adapter(StakeSupport::None), xsol_adapter()],
        &[LENDER_A],
    )
}

/// P6-D3 — one Fruit per wallet per epoch. A switch to another asset is
/// effective NEXT epoch: registering xSOL while SKR is this epoch's
/// selection changes nothing now (accrual stays on SKR, nothing reprices),
/// and the NEXT epoch — carrying the selection across the boundary — runs
/// on xSOL from its first instant with no registration event of its own.
#[test]
fn p6d3_switch_takes_effect_next_epoch_not_same_epoch() {
    let cfg = two_asset_cfg();
    let epoch1 = EpochReferences::new()
        .with(ref_commit_at(SKR_MINT, price(2, 1), 500))
        .with(ref_commit_at(XSOL_MINT, nav(3, 1), 500));

    // Epoch 1: A is registered for SKR before the epoch starts (the
    // boundary participant), then tries to switch to xSOL mid-epoch.
    let mut t = Tl::new();
    t.slot(1, 100)
        .init(A, SKR_MINT)
        .init(A, XSOL_MINT)
        .mint(A, SKR_MINT, 100)
        .mint(A, XSOL_MINT, 100)
        .pinned(&[XSOL_PROGRAM, LENDER_A])
        .no_debt(A, &[LENDER_A])
        .register(A, AssetKind::Skr);
    // The epoch-start checkpoint: the epoch begins from chain state
    // observed exactly at its boundary (P6-D7 / P6R-8).
    t.slot(2, 500).checkpoint(
        vec![snap(A, SKR_MINT, 100), snap(A, XSOL_MINT, 100)],
        vec![],
        vec![supply(SKR_MINT, 100), supply(XSOL_MINT, 100)],
    );
    t.slot(3, 700).register(A, AssetKind::XSol); // the switch attempt
    t.slot(4, 900).close();
    let er1 = replay_epoch(
        &cfg,
        &epoch1,
        EpochCarry::default(),
        500,
        t.events.clone(),
        900,
    )
    .unwrap();

    // The switch never activates within the epoch: A accrues SKR to the
    // end (100 SKR units at the 2-lamport reference; the 100 xSOL units
    // are invisible under the SKR selection).
    for ts in [650u64, 750, 890] {
        match x_at(&er1.report, &A, ts as i64) {
            Some(Eligibility::Verified {
                units, x_lamports, ..
            }) => {
                assert_eq!((units, x_lamports), (100, 200), "at {ts}");
            }
            o => panic!("expected VERIFIED at {ts}, got {o:?}"),
        }
    }
    assert_eq!(er1.report.wallet(&A).unit_seconds, 100 * 800);
    // The deferred choice rides into the next epoch.
    assert_eq!(er1.carry.selections.get(&A), Some(&AssetKind::XSol));

    // Epoch 2: fresh references (per-epoch price basis, P6-D7), holdings
    // carried by a checkpoint at the boundary, NO registration event — the
    // carried switch is in force from the first instant.
    let epoch2 = EpochReferences::new()
        .with(ref_commit_at(SKR_MINT, price(2, 1), 1000))
        .with(ref_commit_at(XSOL_MINT, nav(3, 1), 1000));
    let mut t2 = Tl::new();
    t2.slot(1, 1000)
        .pinned(&[XSOL_PROGRAM, LENDER_A])
        .checkpoint(
            vec![snap(A, SKR_MINT, 100), snap(A, XSOL_MINT, 100)],
            vec![],
            vec![supply(SKR_MINT, 100), supply(XSOL_MINT, 100)],
        )
        .no_debt(A, &[LENDER_A])
        .pinned(&[LENDER_A]);
    t2.slot(2, 1100).register(A, AssetKind::XSol); // redundant re-registration
    t2.slot(3, 1200).close();
    let er2 = replay_epoch(
        &cfg,
        &epoch2,
        er1.carry.clone(),
        1000,
        t2.events.clone(),
        1200,
    )
    .unwrap();

    // Now the selection is xSOL: the 100 xSOL units at NAV 3 count (300)
    // and the SKR units do not — from the boundary, with no registration
    // event of its own.
    match x_at(&er2.report, &A, 1050) {
        Some(Eligibility::Verified {
            units, x_lamports, ..
        }) => {
            assert_eq!((units, x_lamports), (100, 300));
        }
        o => panic!("expected VERIFIED at 1050, got {o:?}"),
    }
    // The redundant mid-epoch re-registration reprices nothing: the
    // boundary-carried selection anchors adopted holdings exactly like a
    // registration at the boundary would (a naive re-registration would
    // taint them unpriced — there is no fresh xSOL observation here).
    match x_at(&er2.report, &A, 1150) {
        Some(Eligibility::Verified {
            units, x_lamports, ..
        }) => {
            assert_eq!((units, x_lamports), (100, 300));
        }
        o => panic!("expected VERIFIED at 1150, got {o:?}"),
    }
    assert_eq!(er2.report.wallet(&A).unit_seconds, 100 * 200);

    // Control: WITHOUT the carry the wallet is unregistered in epoch 2 —
    // the carry is what makes the deferred switch effective.
    let plain = replay_epoch(
        &cfg,
        &epoch2,
        EpochCarry::default(),
        1000,
        t2.events.clone(),
        1200,
    )
    .unwrap();
    assert_eq!(x_at(&plain.report, &A, 1050), None);

    // Determinism.
    assert_eq!(
        replay_epoch(
            &cfg,
            &epoch2,
            er1.carry.clone(),
            1000,
            t2.events.clone(),
            1200
        )
        .unwrap(),
        er2
    );
}

/// P6-D3 — exiting is immediate (eligibility ends at the exit event), and
/// re-registering works for the SAME asset (accrual resumes at the
/// re-registration, §5 "a later join accrues only from its finalized
/// registration") — but exit + re-register of ANOTHER asset still cannot
/// buy an immediate same-epoch switch.
#[test]
fn p6d3_exit_is_immediate_and_cannot_buy_a_same_epoch_switch() {
    let cfg = two_asset_cfg();
    let refs = EpochReferences::new()
        .with(ref_commit_at(SKR_MINT, price(2, 1), 500))
        .with(ref_commit_at(XSOL_MINT, nav(3, 1), 500));

    // (a) Exit ends eligibility immediately; a same-asset rejoin resumes
    // at the re-registration event. The first registration is mid-epoch
    // with a fresh verified observation, so the late join prices at
    // min(reference, observed) = 2 (test 30 semantics).
    let mut t = Tl::new();
    t.slot(1, 100)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 100)
        .no_debt(A, &[LENDER_A])
        .pinned(&[LENDER_A]);
    t.slot(2, 550).observe(SKR_MINT, price(2, 1));
    t.slot(3, 600).register(A, AssetKind::Skr);
    t.slot(4, 700).ev(EventKind::Exit { wallet: A });
    t.slot(5, 800).register(A, AssetKind::Skr);
    t.slot(6, 900).close();
    let r = run_with(&cfg, &refs, &t, 900);
    assert_eq!(r.wallet(&A).unit_seconds, 100 * (100 + 100)); // [600,700)+[800,900)

    // (b) Exit, then registering a DIFFERENT asset: still deferred — the
    // epoch's anchor survives the exit (P6-D3).
    let mut t = Tl::new();
    t.slot(1, 100)
        .init(A, SKR_MINT)
        .init(A, XSOL_MINT)
        .mint(A, SKR_MINT, 100)
        .mint(A, XSOL_MINT, 100)
        .pinned(&[XSOL_PROGRAM, LENDER_A])
        .no_debt(A, &[LENDER_A])
        .pinned(&[LENDER_A]);
    t.slot(2, 500).checkpoint(
        vec![snap(A, SKR_MINT, 100), snap(A, XSOL_MINT, 100)],
        vec![],
        vec![supply(SKR_MINT, 100), supply(XSOL_MINT, 100)],
    );
    t.slot(3, 550).observe(SKR_MINT, price(2, 1));
    t.slot(4, 600).register(A, AssetKind::Skr);
    t.slot(5, 700).ev(EventKind::Exit { wallet: A });
    t.slot(6, 800).register(A, AssetKind::XSol);
    t.slot(7, 900).close();
    let er = replay_epoch(&cfg, &refs, EpochCarry::default(), 500, t.events, 900).unwrap();
    // Accrual is only the pre-exit SKR stretch; after the exit nothing is
    // active (the xSOL registration is a deferred switch, not a join).
    assert_eq!(er.report.wallet(&A).unit_seconds, 100 * 100); // [600,700)
    assert_eq!(x_at(&er.report, &A, 850), None);
    // And the switch is queued for next epoch, exactly as without the exit.
    assert_eq!(er.carry.selections.get(&A), Some(&AssetKind::XSol));
}

// ------------------------------------------------- P6-D7 (frozen decision)

/// P6-D7 — a verified route halt persists across the epoch boundary (it is
/// not erased by the new epoch's committed reference); resumption takes
/// effect from the `RouteResumed` event and is not retroactive.
#[test]
fn p6d7_route_halts_persist_across_the_epoch_boundary_until_resumed() {
    let cfg = config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A]);

    // Epoch 1: registered pre-epoch, the route halts mid-epoch.
    let refs1 = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 100));
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 100)
        .no_debt(A, &[LENDER_A])
        .pinned(&[LENDER_A]);
    t.slot(2, 60).register(A, AssetKind::Skr);
    t.slot(3, 100).checkpoint(
        vec![snap(A, SKR_MINT, 100)],
        vec![],
        vec![supply(SKR_MINT, 100)],
    );
    t.slot(4, 150).route_halt(SKR_MINT);
    t.slot(5, 200).close();
    let er1 = replay_epoch(&cfg, &refs1, EpochCarry::default(), 100, t.events, 200).unwrap();
    assert_eq!(
        x_at(&er1.report, &A, 140),
        Some(Eligibility::Verified {
            units: 100,
            gross_lamports: 200,
            debt_lamports: 0,
            x_lamports: 200,
        })
    );
    assert_eq!(
        x_at(&er1.report, &A, 160),
        Some(Eligibility::Pending(PendingReason::RouteHalted))
    );
    assert!(er1.carry.halted_mints.contains(&SKR_MINT));

    // Epoch 2: the halt rides the boundary — the wallet is PENDING from
    // the first instant of the epoch, before any new event.
    let refs2 = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 200));
    let mut t2 = Tl::new();
    t2.slot(1, 200)
        .checkpoint(
            vec![snap(A, SKR_MINT, 100)],
            vec![],
            vec![supply(SKR_MINT, 100)],
        )
        .no_debt(A, &[LENDER_A])
        .pinned(&[LENDER_A]);
    t2.slot(2, 250).route_resumed(SKR_MINT, price(2, 1));
    t2.slot(3, 300).close();
    let er2 = replay_epoch(&cfg, &refs2, er1.carry.clone(), 200, t2.events, 300).unwrap();
    assert_eq!(
        x_at(&er2.report, &A, 210),
        Some(Eligibility::Pending(PendingReason::RouteHalted)),
        "the halt persists across the epoch boundary"
    );
    // Resumption is NOT retroactive: [200, 250) stays reserved (pending),
    // and verified accrual exists only from the resume event on.
    assert_eq!(
        x_at(&er2.report, &A, 260),
        Some(Eligibility::Verified {
            units: 100,
            gross_lamports: 200,
            debt_lamports: 0,
            x_lamports: 200,
        })
    );
    let acc = er2.report.wallet(&A);
    assert_eq!(acc.pending_seconds, 50); // [200, 250)
    assert_eq!(acc.unit_seconds, 100 * 50); // [250, 300)
                                            // Nothing is carried out once resumed.
    assert!(!er2.carry.halted_mints.contains(&SKR_MINT));
}

// ------------------------------------------------- P6-D5 (settlement math)

/// Deterministic xorshift fixture RNG (same shape as the Pass #5 property
/// suite): reproducible, no external dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const PROP_WALLETS: [Addr; 6] = [
    Addr::fixture(0x41),
    Addr::fixture(0x42),
    Addr::fixture(0x43),
    Addr::fixture(0x44),
    Addr::fixture(0x45),
    Addr::fixture(0x46),
];

/// P6-D5 — exact settlement invariants under random epoch shapes:
///
/// * conservation — `allocated + reserved + not_earned + unallocated == E`
///   exactly, in every configuration;
/// * never round up — every leaf is `floor` of the wallet's exact
///   cumulative allocation, capped at `E/5`; `Σ leaves ≤ E`;
/// * scheduled-capacity bound — settling any prefix keeps
///   `Σ leaves ≤ floor(E · elapsed / T)` (the on-chain root bound, §10a);
/// * checkpoint-frequency independence — flooring of the WALLET happens
///   once on the cumulative, so refining the partition changes no leaf
///   beyond the frozen sub-lamport drift of the per-segment `acc` floors
///   (P6-D5: drift ≤ `w/SCALE` per segment — refinement can only floor
///   MORE, and by less than one lamport at the leaf);
/// * no redistribution — in the floor regime (every `Cw ≤ Hfloor`) each
///   wallet's leaf is independent of who else participates; and at
///   production scales the reserve/release claims are exactly the
///   intervals' scheduled capacity (the envelope clamp never engages).
#[test]
fn settlement_invariants_conserve_floor_and_never_redistribute() {
    for seed in 1..=48u64 {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed);
        let budget = 1 + rng.below(9_000_000_000);
        let t_start = 0i64;
        let duration = 1 + rng.below(950_400) as i64;
        let t_end = duration;
        let n_iv = 1 + rng.below(10) as usize;

        // A random ordered partition of the epoch into n_iv intervals.
        let mut cuts: Vec<i64> = Vec::new();
        for _ in 1..n_iv {
            cuts.push(1 + rng.below((duration - 1) as u64) as i64);
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut bounds: Vec<i64> = cuts;
        bounds.push(t_end);
        let mut intervals: Vec<EpochInterval> = Vec::new();
        let mut prev = t_start;
        let mut floor_regime = true;
        for &end in &bounds {
            // 0/1 settle, 2 reserve, 3 not-earned.
            let verdict = match rng.below(4) {
                0 | 1 => IntervalVerdict::Settle,
                2 => IntervalVerdict::Reserve,
                _ => IntervalVerdict::NotEarned,
            };
            let mut parts = Vec::new();
            if verdict == IntervalVerdict::Settle {
                let n = rng.below(6);
                let mut cw: u128 = 0;
                for i in 0..=n {
                    let w = PROP_WALLETS[i as usize];
                    parts.push(part(w, rng.below(400), rng.below(3_000_000_000) as u128));
                    let seeds = parts.last().unwrap().seeds;
                    let x = parts.last().unwrap().x_lamports;
                    cw += recognized_weight(seeds, x);
                }
                if cw > DENOMINATOR_FLOOR {
                    floor_regime = false;
                }
            }
            intervals.push(interval(prev, end, verdict, parts));
            prev = end;
        }

        let s = settle_epoch(budget, t_start, t_end, &intervals)
            .unwrap_or_else(|e| panic!("seed {seed}: {e:?}"));

        // Conservation, always.
        assert!(s.conserves(budget), "seed {seed}");

        // Never round up; leaves are floors of the exact cumulative, capped
        // at E/5; the sum never exceeds the budget.
        let mut total = 0u64;
        for ws in &s.allocations {
            assert!(
                ws.leaf_lamports <= budget / 5,
                "seed {seed} {:?}",
                ws.wallet
            );
            assert_eq!(
                ws.leaf_lamports,
                ((ws.exact.num / ws.exact.den) as u64).min(budget / 5),
                "seed {seed} {:?}: leaf must be floor(exact)",
                ws.wallet
            );
            total += ws.leaf_lamports;
        }
        assert_eq!(total, s.allocated_lamports);
        assert!(s.allocated_lamports <= budget, "seed {seed}");

        // Scheduled-capacity bound on every prefix (§10a root rule): a
        // root posted at checkpoint `cut` may total at most
        // floor(E·cut/T). The prefix settlement keeps the FULL epoch
        // duration (the un-elapsed tail is simply unclassified), so the
        // accrual rate stays E/T.
        for &cut in &bounds {
            let mut clipped: Vec<EpochInterval> = intervals
                .iter()
                .filter(|iv| iv.start < cut)
                .map(|iv| {
                    let mut c = iv.clone();
                    c.end = c.end.min(cut);
                    c
                })
                .collect();
            if cut < t_end {
                clipped.push(interval(cut, t_end, IntervalVerdict::Reserve, vec![]));
            }
            let p = settle_epoch(budget, t_start, t_end, &clipped).unwrap();
            let cap = (u128::from(budget) * cut as u128 / duration as u128) as u64;
            assert!(
                p.allocated_lamports <= cap,
                "seed {seed} prefix {cut}: {} > {cap}",
                p.allocated_lamports
            );
        }

        // Checkpoint-frequency independence (P6-D5): refine every settling
        // interval into two halves. The per-segment `acc` floors make the
        // refined cumulative slightly SMALLER (floor(x)+floor(y) ≤
        // floor(x+y)), by at most `w/SCALE` per refined segment — strictly
        // below one lamport at the leaf, which therefore never drops by
        // more than 1.
        let mut refined: Vec<EpochInterval> = Vec::new();
        for iv in &intervals {
            if iv.verdict == IntervalVerdict::Settle && iv.end - iv.start >= 2 {
                let mid = iv.start + (iv.end - iv.start) / 2;
                refined.push(interval(iv.start, mid, iv.verdict, iv.participants.clone()));
                refined.push(interval(mid, iv.end, iv.verdict, iv.participants.clone()));
            } else {
                refined.push(iv.clone());
            }
        }
        let s2 = settle_epoch(budget, t_start, t_end, &refined)
            .unwrap_or_else(|e| panic!("seed {seed} refined: {e:?}"));
        assert_eq!(s2.allocations.len(), s.allocations.len(), "seed {seed}");
        for (a, b) in s.allocations.iter().zip(&s2.allocations) {
            assert_eq!(a.wallet, b.wallet);
            assert!(b.leaf_lamports <= a.leaf_lamports, "seed {seed}");
            assert!(
                a.leaf_lamports - b.leaf_lamports <= 1,
                "seed {seed} {:?}: refinement drift {} exceeds the frozen bound",
                a.wallet,
                a.leaf_lamports - b.leaf_lamports
            );
        }
        assert!(
            s.reserved_lamports + s.not_earned_lamports
                <= s2.reserved_lamports + s2.not_earned_lamports + 1
                && s2.reserved_lamports + s2.not_earned_lamports
                    <= s.reserved_lamports + s.not_earned_lamports + 1,
            "seed {seed}: held capacity moved by more than the frozen drift"
        );

        // No redistribution (floor regime): each wallet's leaf is
        // independent of the other participants.
        if floor_regime {
            for w in PROP_WALLETS {
                if !s.allocations.iter().any(|a| a.wallet == w) {
                    continue;
                }
                let without: Vec<EpochInterval> = intervals
                    .iter()
                    .map(|iv| EpochInterval {
                        start: iv.start,
                        end: iv.end,
                        verdict: iv.verdict,
                        participants: iv
                            .participants
                            .iter()
                            .filter(|p| p.wallet != w)
                            .copied()
                            .collect(),
                    })
                    .collect();
                let s3 = settle_epoch(budget, t_start, t_end, &without).unwrap();
                for ws in &s3.allocations {
                    assert_eq!(
                        ws.leaf_lamports,
                        leaf_of(&s, &ws.wallet),
                        "seed {seed} {:?}: removing {w:?} must not change leaves",
                        ws.wallet
                    );
                }
            }
        }

        // At production scales the envelope clamp never engages: reserved
        // and not-earned are exactly the intervals' scheduled capacity.
        if budget / duration as u64 >= intervals.len() as u64 {
            let cap_at = |t: i64| (u128::from(budget) * t as u128 / duration as u128) as u64;
            let want_reserved: u64 = intervals
                .iter()
                .filter(|iv| iv.verdict == IntervalVerdict::Reserve)
                .map(|iv| cap_at(iv.end) - cap_at(iv.start))
                .sum();
            let want_not_earned: u64 = intervals
                .iter()
                .filter(|iv| iv.verdict == IntervalVerdict::NotEarned)
                .map(|iv| cap_at(iv.end) - cap_at(iv.start))
                .sum();
            assert_eq!(s.reserved_lamports, want_reserved, "seed {seed}");
            assert_eq!(s.not_earned_lamports, want_not_earned, "seed {seed}");
        }
    }
}

// ------------------------------------------------- P6R-1 (remediation R1, D5)

/// The wallets of the P6R-1 fixtures: addresses never used by the shared
/// `A/B/C` constants, so allocation assertions cannot collide.
const R1: [Addr; 8] = [
    Addr::fixture(0x51),
    Addr::fixture(0x52),
    Addr::fixture(0x53),
    Addr::fixture(0x54),
    Addr::fixture(0x55),
    Addr::fixture(0x56),
    Addr::fixture(0x57),
    Addr::fixture(0x58),
];

/// P6R-1 — the settlement accumulator is the FROZEN fixed-precision integer
/// of P6-D5 (`SCALE = 10^18`, downward rounding only, one cumulative wallet
/// sum, floor once at the leaf), NOT exact fractions: exact-fraction
/// denominators compound across segments and overflow `u128` on a realistic
/// four-interval Harvest epoch whose per-interval `H_seg` values are
/// distinct — exactly the shape the preserved implementation failed on with
/// `MathOverflow`. The leaves must equal the D5 formula computed
/// independently here: `floor(Σ_seg w_i,seg · acc_seg / SCALE)` with
/// `acc_seg = floor(E·Δt·SCALE/(T·H_seg))`.
#[test]
fn p6r1_fixed_point_accumulator_settles_a_realistic_harvest() {
    let e = 684_931_507u64; // Harvest budget fixture (§2 table)
    let (t_start, t_end) = (0i64, 172_800i64); // Harvest window, 2 days
    let q = t_end / 4;

    // Realistic odd lamport equities; every interval's Cw sits above the
    // 12.5 SOL-equivalent floor with a distinct, near-coprime H_seg (the
    // exact-fraction killer: denominators share almost no common factors).
    let xs: [[u128; 8]; 4] = [
        [
            1_918_374_129,
            2_114_965_837,
            1_662_309_871,
            2_003_117_249,
            1_785_660_413,
            2_296_542_087,
            1_547_936_221,
            2_573_582_039,
        ],
        [
            2_330_459_271,
            1_871_436_253,
            1_957_683_319,
            2_267_913_457,
            1_630_258_809,
            2_478_191_527,
            1_519_651_893,
            2_288_838_931,
        ],
        [
            2_401_558_313,
            1_760_285_361,
            2_187_470_939,
            1_923_117_047,
            2_517_846_529,
            1_584_620_077,
            2_097_438_651,
            2_150_188_487,
        ],
        [
            2_046_729_377,
            2_459_016_543,
            1_687_342_159,
            2_285_607_491,
            1_962_485_277,
            2_360_754_823,
            1_551_968_437,
            1_969_720_019,
        ],
    ];
    let intervals: Vec<EpochInterval> = xs
        .iter()
        .enumerate()
        .map(|(i, row)| {
            interval(
                i as i64 * q,
                (i + 1) as i64 * q,
                IntervalVerdict::Settle,
                row.iter()
                    .enumerate()
                    .map(|(k, &x)| part(R1[k], 250, x))
                    .collect(),
            )
        })
        .collect();

    // No overflow; conservation; nothing reserved or not-earned.
    let s = settle_epoch(e, t_start, t_end, &intervals)
        .unwrap_or_else(|err| panic!("P6R-1 regression: {err:?}"));
    assert!(s.conserves(e));
    assert_eq!(s.reserved_lamports, 0);
    assert_eq!(s.not_earned_lamports, 0);

    // The D5 formula, computed independently per wallet.
    let mut expect = BTreeMap::<Addr, u128>::new();
    for row in &xs {
        let h = shared_denominator(row.iter().map(|&x| recognized_weight(250, x)).sum());
        let acc = u128::from(e) * (q as u128) * ACCUMULATOR_SCALE / ((t_end - t_start) as u128 * h);
        for (k, &x) in row.iter().enumerate() {
            *expect.entry(R1[k]).or_insert(0) += recognized_weight(250, x) * acc;
        }
    }
    for ws in &s.allocations {
        let want = ((expect[&ws.wallet] / ACCUMULATOR_SCALE) as u64).min(e / 5);
        assert_eq!(ws.leaf_lamports, want, "{:?}", ws.wallet);
        assert!(ws.leaf_lamports <= e / 5, "{:?}", ws.wallet);
    }
    assert_eq!(
        s.allocated_lamports,
        s.allocations.iter().map(|w| w.leaf_lamports).sum::<u64>()
    );
}

/// P6R-1 — reachable production bounds cannot overflow the fixed-point
/// accumulator. The largest possible epoch budget is the Storm ceiling and
/// the longest epoch is the 11-day core (fallback Harvests are strictly
/// smaller on both axes), so the `acc` numerator is bounded by
/// `8,561,643,836 · 950,400 · 10^18 ≈ 8.1e33` and the wallet-side
/// cumulative by `Kmax · E · SCALE / Hfloor ≈ 1.8e27`, both far below
/// `u128::MAX ≈ 3.4e38`. Exercised at the bound: a maximal epoch with many
/// settling segments settles, conserves and never exceeds the budget.
#[test]
fn p6r1_production_bounds_cannot_overflow_the_accumulator() {
    let e = rigs::STORM_CEILING_LAMPORTS; // the largest reachable E
    let t_end = rigs::CORE_END_OFFSET; // 950,400 s, the longest epoch
    let t_end_u = u128::try_from(t_end).unwrap();
    // The bound itself, executably.
    assert!(u128::from(e) * t_end_u * ACCUMULATOR_SCALE < u128::MAX);
    assert!(2_500_000_000u128 * u128::from(e) * ACCUMULATOR_SCALE / DENOMINATOR_FLOOR < u128::MAX);

    // A maximal epoch finely partitioned (a checkpoint every ~6.6 minutes),
    // each segment carrying eight maximum-weight wallets: Cw = 20 SOL > the
    // 12.5 floor, so H = 20 SOL and each wallet's exact share is E/8.
    let segs = 240i64;
    let dt = t_end / segs;
    let heavy: Vec<Participant> = (0..8)
        .map(|k| part(R1[k as usize], 250, 2_500_000_000))
        .collect();
    let intervals: Vec<EpochInterval> = (0..segs)
        .map(|i| interval(i * dt, (i + 1) * dt, IntervalVerdict::Settle, heavy.clone()))
        .collect();
    let s = settle_epoch(e, 0, t_end, &intervals)
        .unwrap_or_else(|err| panic!("production-bound epoch must settle, got {err:?}"));
    assert!(s.conserves(e));
    assert_eq!(s.allocations.len(), 8);
    for ws in &s.allocations {
        assert_eq!(ws.leaf_lamports, e / 8, "{:?}", ws.wallet);
        assert!(ws.leaf_lamports <= e / 5, "{:?}", ws.wallet);
    }
    assert_eq!(
        s.allocated_lamports,
        s.allocations.iter().map(|w| w.leaf_lamports).sum::<u64>()
    );
    // Only the floor dust stays unallocated (U → Z exactly once at close).
    assert_eq!(s.unallocated_lamports, e - 8 * (e / 8));
    assert!(s.unallocated_lamports < 8);
}

// ------------------------------------------------- P6R-2 (remediation R1, D7)

/// P6R-2 — Safe Mode state crosses the epoch boundary unchanged (P6-D7
/// item 5). A new epoch's committed reference does NOT clear a
/// still-active emergency NAV: the next epoch's valuation stays at the
/// emergency price (here 100, not the stale committed 300 — the exact 3x
/// overvaluation the preserved implementation produced by dropping the
/// carried override when a reference was committed). Settled epoch-1
/// history is never repriced, and only a published recovery (`RouteResumed`)
/// lifts the emergency price — prospectively.
#[test]
fn p6r2_emergency_nav_stays_authoritative_over_a_new_epoch_reference() {
    let cfg = config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A]);

    // Epoch 1 [100, 200): committed reference 3; a verified collapse
    // re-prices SKR to 1 mid-epoch.
    let refs1 = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(3, 1), 100));
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 100)
        .no_debt(A, &[LENDER_A])
        .pinned(&[LENDER_A]);
    t.slot(2, 60).register(A, AssetKind::Skr);
    t.slot(3, 100).checkpoint(
        vec![snap(A, SKR_MINT, 100)],
        vec![],
        vec![supply(SKR_MINT, 100)],
    );
    t.slot(4, 120).emergency_nav(SKR_MINT, price(1, 1));
    t.slot(5, 200).close();
    let er1 = replay_epoch(&cfg, &refs1, EpochCarry::default(), 100, t.events, 200).unwrap();
    // Settled history is never repriced: before the collapse the units are
    // valued at the reference 3; from the collapse on, at the emergency 1.
    assert_eq!(
        x_at(&er1.report, &A, 110),
        Some(Eligibility::Verified {
            units: 100,
            gross_lamports: 300,
            debt_lamports: 0,
            x_lamports: 300,
        })
    );
    assert_eq!(
        x_at(&er1.report, &A, 150),
        Some(Eligibility::Verified {
            units: 100,
            gross_lamports: 100,
            debt_lamports: 0,
            x_lamports: 100,
        })
    );

    // Epoch 2 [200, 320): a FRESH reference is committed at 3 (a stale
    // oracle), the boundary checkpoint carries the 100 units across, and a
    // published recovery lands mid-epoch.
    let refs2 = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(3, 1), 200));
    let mut t2 = Tl::new();
    t2.slot(1, 200)
        .checkpoint(
            vec![snap(A, SKR_MINT, 100)],
            vec![],
            vec![supply(SKR_MINT, 100)],
        )
        .no_debt(A, &[LENDER_A])
        .pinned(&[LENDER_A]);
    t2.slot(2, 260).route_resumed(SKR_MINT, price(3, 1));
    t2.slot(3, 320).close();
    let er2 = replay_epoch(&cfg, &refs2, er1.carry.clone(), 200, t2.events, 320).unwrap();

    // The emergency NAV stays authoritative over the new reference until
    // the recovery: 100 units at 1, NOT at the committed 3.
    assert_eq!(
        x_at(&er2.report, &A, 250),
        Some(Eligibility::Verified {
            units: 100,
            gross_lamports: 100,
            debt_lamports: 0,
            x_lamports: 100,
        }),
        "P6R-2: the carried emergency price must win over the new commit"
    );
    // The published recovery lifts it — prospectively only (the [200,260)
    // segment above is untouched), exactly like an in-epoch reset.
    assert_eq!(
        x_at(&er2.report, &A, 300),
        Some(Eligibility::Verified {
            units: 100,
            gross_lamports: 300,
            debt_lamports: 0,
            x_lamports: 300,
        })
    );
    // And nothing survives the recovery into a third epoch's carry.
    assert!(er2.carry.active_prices.is_empty());
    assert!(!er2.carry.nav_invalid.contains(&SKR_MINT));
}

/// P6R-2 — the other half of Safe Mode state: a mint whose latest NAV/reset
/// datum was UNUSABLE (wrong basis) at the boundary keeps recognition
/// paused in the next epoch despite its fresh committed reference, until a
/// basis-correct reset datum arrives (fail closed; P6-D7 item 5).
#[test]
fn p6r2_unusable_nav_datum_pauses_recognition_across_the_boundary() {
    let cfg = config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A]);

    // Epoch 1 ends with a wrong-basis emergency datum: recognition pauses.
    let refs1 = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 100));
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 100)
        .no_debt(A, &[LENDER_A])
        .pinned(&[LENDER_A]);
    t.slot(2, 60).register(A, AssetKind::Skr);
    t.slot(3, 100).checkpoint(
        vec![snap(A, SKR_MINT, 100)],
        vec![],
        vec![supply(SKR_MINT, 100)],
    );
    t.slot(4, 150).emergency_nav(SKR_MINT, nav(1, 1)); // wrong basis for SKR
    t.slot(5, 200).close();
    let er1 = replay_epoch(&cfg, &refs1, EpochCarry::default(), 100, t.events, 200).unwrap();
    assert_eq!(
        x_at(&er1.report, &A, 180),
        Some(Eligibility::Pending(PendingReason::NavUnverified))
    );
    assert!(er1.carry.nav_invalid.contains(&SKR_MINT));

    // Epoch 2 commits a fresh reference — which must NOT clear the pause.
    let refs2 = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 200));
    let mut t2 = Tl::new();
    t2.slot(1, 200)
        .checkpoint(
            vec![snap(A, SKR_MINT, 100)],
            vec![],
            vec![supply(SKR_MINT, 100)],
        )
        .no_debt(A, &[LENDER_A])
        .pinned(&[LENDER_A]);
    t2.slot(2, 240).emergency_nav(SKR_MINT, price(2, 1)); // basis-correct datum
    t2.slot(3, 300).close();
    let er2 = replay_epoch(&cfg, &refs2, er1.carry.clone(), 200, t2.events, 300).unwrap();
    assert_eq!(
        x_at(&er2.report, &A, 220),
        Some(Eligibility::Pending(PendingReason::NavUnverified)),
        "P6R-2: an unusable NAV datum must survive a new committed reference"
    );
    // A basis-correct reset datum lifts the pause from its event on.
    assert_eq!(
        x_at(&er2.report, &A, 260),
        Some(Eligibility::Verified {
            units: 100,
            gross_lamports: 200,
            debt_lamports: 0,
            x_lamports: 200,
        })
    );
    assert!(!er2.carry.nav_invalid.contains(&SKR_MINT));
}

// ------------------------------------------------- P6R-8 (remediation R1, D7)

/// P6R-8 — an epoch's replay must be anchored by a finalized checkpoint
/// at EXACTLY its start boundary (P6-D7 item 1): the epoch begins from
/// chain state observed at its own boundary, never from an arbitrary
/// earlier or later state. Missing anchors, anchors merely NEAR the
/// boundary (before or after), a horizon at or before the boundary, and
/// references committed for a different epoch are all rejected.
#[test]
fn p6r8_epoch_replay_requires_a_checkpoint_at_its_exact_start_boundary() {
    let cfg = config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A]);
    let refs = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 500));

    let anchored = |ts: i64| {
        let mut t = Tl::new();
        t.slot(1, 100)
            .init(A, SKR_MINT)
            .mint(A, SKR_MINT, 100)
            .no_debt(A, &[LENDER_A])
            .pinned(&[LENDER_A]);
        t.slot(2, 150).register(A, AssetKind::Skr);
        t.slot(3, ts).checkpoint(
            vec![snap(A, SKR_MINT, 100)],
            vec![],
            vec![supply(SKR_MINT, 100)],
        );
        t.slot(4, 600).close();
        t.events
    };

    // Positive control: the anchor exactly at the boundary replays.
    assert!(replay_epoch(&cfg, &refs, EpochCarry::default(), 500, anchored(500), 600).is_ok());

    // Missing: no checkpoint anywhere near the boundary (only the close).
    let none = {
        let mut t = Tl::new();
        t.slot(1, 100)
            .init(A, SKR_MINT)
            .mint(A, SKR_MINT, 100)
            .no_debt(A, &[LENDER_A])
            .pinned(&[LENDER_A]);
        t.slot(2, 150).register(A, AssetKind::Skr);
        t.slot(3, 600).close();
        t.events
    };
    assert!(replay_epoch(&cfg, &refs, EpochCarry::default(), 500, none, 600).is_err());

    // Misaligned late / early: a checkpoint NEAR the boundary does not
    // anchor the epoch — replay may not begin from an arbitrary state.
    for near in [400i64, 499, 501, 550] {
        assert!(
            replay_epoch(&cfg, &refs, EpochCarry::default(), 500, anchored(near), 600).is_err(),
            "checkpoint at {near} must not anchor an epoch starting at 500"
        );
    }

    // The horizon must lie beyond the boundary.
    assert!(replay_epoch(&cfg, &refs, EpochCarry::default(), 500, anchored(500), 500).is_err());
    assert!(replay_epoch(&cfg, &refs, EpochCarry::default(), 500, anchored(500), 400).is_err());

    // References committed for a DIFFERENT epoch do not belong here.
    let other_refs = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 700));
    assert!(replay_epoch(
        &cfg,
        &other_refs,
        EpochCarry::default(),
        500,
        anchored(500),
        600
    )
    .is_err());
    // Disagreeing commits inside one EpochReferences are rejected outright.
    let disagreeing = EpochReferences::new()
        .with(ref_commit_at(SKR_MINT, price(2, 1), 500))
        .with(ref_commit_at(XSOL_MINT, nav(3, 1), 600));
    assert!(replay_epoch(
        &cfg,
        &disagreeing,
        EpochCarry::default(),
        500,
        anchored(500),
        600
    )
    .is_err());

    // Events strictly before the boundary are legitimate pre-epoch history
    // as long as the boundary itself is checkpoint-anchored (the positive
    // control above has its registration at 60 < 500).
}

// ------------------------------------------------- P6R-5 (remediation R1, D3)

/// P6R-5 — the deferred-switch state machine, exactly as frozen (P6-D3):
/// join begins when finalized, a switch is effective NEXT epoch, exit is
/// immediate, and an abandoned or replaced switch intent never survives.
/// The two bugs the preserved implementation had are pinned first: an exit
/// must not auto-register the queued switch next epoch, and switching back
/// to the current asset before the boundary must cancel the earlier
/// switch.
#[test]
fn p6r5_exit_abandons_a_queued_switch_and_switch_back_cancels_it() {
    let cfg = two_asset_cfg();
    let refs = || {
        EpochReferences::new()
            .with(ref_commit_at(SKR_MINT, price(2, 1), 500))
            .with(ref_commit_at(XSOL_MINT, nav(3, 1), 500))
    };

    // (A) switch → exit: the exit ABANDONS the queued switch; next epoch
    // the wallet is simply unregistered.
    let mut t = Tl::new();
    t.slot(1, 100)
        .init(A, SKR_MINT)
        .init(A, XSOL_MINT)
        .mint(A, SKR_MINT, 100)
        .mint(A, XSOL_MINT, 100)
        .pinned(&[XSOL_PROGRAM, LENDER_A])
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 150).register(A, AssetKind::Skr);
    t.slot(3, 500).checkpoint(
        vec![snap(A, SKR_MINT, 100), snap(A, XSOL_MINT, 100)],
        vec![],
        vec![supply(SKR_MINT, 100), supply(XSOL_MINT, 100)],
    );
    t.slot(4, 600).register(A, AssetKind::XSol); // the switch request
    t.slot(5, 700).ev(EventKind::Exit { wallet: A }); // ...abandoned by the exit
    t.slot(6, 900).close();
    let er = replay_epoch(&cfg, &refs(), EpochCarry::default(), 500, t.events, 900).unwrap();
    assert!(
        !er.carry.selections.contains_key(&A),
        "P6R-5(A): exit must not imply future re-registration"
    );

    // (B) switch A→B, then switch back to A before the boundary: the
    // earlier switch is abandoned; next epoch runs on A.
    let mut t = Tl::new();
    t.slot(1, 100)
        .init(A, SKR_MINT)
        .init(A, XSOL_MINT)
        .mint(A, SKR_MINT, 100)
        .mint(A, XSOL_MINT, 100)
        .pinned(&[XSOL_PROGRAM, LENDER_A])
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 150).register(A, AssetKind::Skr);
    t.slot(3, 500).checkpoint(
        vec![snap(A, SKR_MINT, 100), snap(A, XSOL_MINT, 100)],
        vec![],
        vec![supply(SKR_MINT, 100), supply(XSOL_MINT, 100)],
    );
    t.slot(4, 600).register(A, AssetKind::XSol); // switch away
    t.slot(5, 700).register(A, AssetKind::Skr); // ...and back before the boundary
    t.slot(6, 900).close();
    let er_b = replay_epoch(&cfg, &refs(), EpochCarry::default(), 500, t.events, 900).unwrap();
    assert_eq!(
        er_b.carry.selections.get(&A),
        Some(&AssetKind::Skr),
        "P6R-5(B): the abandoned switch must not activate next epoch"
    );
    // And epoch 2 really runs on SKR: 100 SKR at the 2-lamport reference,
    // the xSOL holding invisible, no registration event of its own.
    let refs2 = EpochReferences::new()
        .with(ref_commit_at(SKR_MINT, price(2, 1), 900))
        .with(ref_commit_at(XSOL_MINT, nav(3, 1), 900));
    let mut t2 = Tl::new();
    t2.slot(1, 900)
        .checkpoint(
            vec![snap(A, SKR_MINT, 100), snap(A, XSOL_MINT, 100)],
            vec![],
            vec![supply(SKR_MINT, 100), supply(XSOL_MINT, 100)],
        )
        .pinned(&[XSOL_PROGRAM, LENDER_A])
        .no_debt(A, &[LENDER_A]);
    t2.slot(2, 1000).close();
    let er2 = replay_epoch(
        &cfg,
        &refs2,
        er_b.carry.clone(),
        900,
        t2.events.clone(),
        1000,
    )
    .unwrap();
    assert_eq!(
        x_at(&er2.report, &A, 950),
        Some(Eligibility::Verified {
            units: 100,
            gross_lamports: 200,
            debt_lamports: 0,
            x_lamports: 200,
        })
    );

    // (A, continued) epoch 2 WITHOUT the exited wallet: it is not
    // re-registered by the carry — the same timeline leaves it out.
    let er2a = replay_epoch(&cfg, &refs2, er.carry.clone(), 900, t2.events, 1000).unwrap();
    assert_eq!(x_at(&er2a.report, &A, 950), None);
}

/// P6R-5 — the rest of the deferred-switch matrix: multiple switch
/// requests (latest wins), same-asset re-registration, exit + explicit
/// re-register of a different asset (a live switch again), and
/// boundary-time requests (a first registration AT the epoch start is an
/// immediate join; a switch requested AT the epoch start is deferred to
/// the epoch after).
#[test]
fn p6r5_switch_request_matrix_latest_intent_wins() {
    let cfg = two_asset_cfg();
    let refs = || {
        EpochReferences::new()
            .with(ref_commit_at(SKR_MINT, price(2, 1), 500))
            .with(ref_commit_at(XSOL_MINT, nav(3, 1), 500))
    };
    let base = |tail: &dyn Fn(&mut Tl)| {
        let mut t = Tl::new();
        t.slot(1, 100)
            .init(A, SKR_MINT)
            .init(A, XSOL_MINT)
            .mint(A, SKR_MINT, 100)
            .mint(A, XSOL_MINT, 100)
            .pinned(&[XSOL_PROGRAM, LENDER_A])
            .no_debt(A, &[LENDER_A]);
        t.slot(2, 150).register(A, AssetKind::Skr);
        t.slot(3, 500).checkpoint(
            vec![snap(A, SKR_MINT, 100), snap(A, XSOL_MINT, 100)],
            vec![],
            vec![supply(SKR_MINT, 100), supply(XSOL_MINT, 100)],
        );
        tail(&mut t);
        t.slot(9, 900).close();
        t.events
    };
    let carry_of = |events| {
        replay_epoch(&cfg, &refs(), EpochCarry::default(), 500, events, 900)
            .unwrap()
            .carry
            .selections
            .get(&A)
            .copied()
    };

    // Multiple switch requests: away, back, away again — the LAST request
    // wins.
    let events = base(&|t| {
        t.slot(4, 600).register(A, AssetKind::XSol);
        t.slot(5, 650).register(A, AssetKind::Skr);
        t.slot(6, 700).register(A, AssetKind::XSol);
    });
    assert_eq!(carry_of(events), Some(AssetKind::XSol));

    // A same-asset re-registration is not a switch: accrual is continuous
    // on SKR and nothing is deferred.
    let events = base(&|t| {
        t.slot(4, 600).register(A, AssetKind::Skr);
    });
    let er = replay_epoch(&cfg, &refs(), EpochCarry::default(), 500, events, 900).unwrap();
    assert_eq!(er.carry.selections.get(&A), Some(&AssetKind::Skr));
    assert_eq!(er.report.wallet(&A).unit_seconds, 100 * 750); // [150, 900)

    // Exit + explicit re-register of a DIFFERENT asset: the wallet's last
    // finalized intent is the switch, so it IS live again (queued for the
    // next epoch — never same-epoch, see p6d3).
    let events = base(&|t| {
        t.slot(4, 600).register(A, AssetKind::XSol);
        t.slot(5, 650).ev(EventKind::Exit { wallet: A });
        t.slot(6, 700).register(A, AssetKind::XSol);
    });
    assert_eq!(carry_of(events), Some(AssetKind::XSol));
    // ...while exit alone (no re-register) leaves nothing queued, and
    // exit + same-asset rejoin re-registers the SAME asset.
    let events = base(&|t| {
        t.slot(4, 600).ev(EventKind::Exit { wallet: A });
        t.slot(5, 700).register(A, AssetKind::Skr);
    });
    assert_eq!(carry_of(events), Some(AssetKind::Skr));

    // Boundary-time requests: a FIRST registration exactly at t_start is
    // an immediate join of the new epoch (present at the reference
    // timestamp, reference-priced)...
    let refs2 = EpochReferences::new()
        .with(ref_commit_at(SKR_MINT, price(2, 1), 900))
        .with(ref_commit_at(XSOL_MINT, nav(3, 1), 900));
    let mut t2 = Tl::new();
    t2.slot(1, 900)
        .checkpoint(
            vec![snap(B, XSOL_MINT, 100)],
            vec![],
            vec![supply(XSOL_MINT, 100)],
        )
        .pinned(&[XSOL_PROGRAM, LENDER_A])
        .no_debt(B, &[LENDER_A]);
    t2.slot(2, 900).register(B, AssetKind::XSol); // exactly at the boundary
    t2.slot(3, 1000).close();
    let er2 = replay_epoch(&cfg, &refs2, EpochCarry::default(), 900, t2.events, 1000).unwrap();
    assert_eq!(
        x_at(&er2.report, &B, 950),
        Some(Eligibility::Verified {
            units: 100,
            gross_lamports: 300,
            debt_lamports: 0,
            x_lamports: 300,
        }),
        "a registration exactly at t_start is an immediate, reference-priced join"
    );

    // ...while a SWITCH requested exactly at t_start(p+1) belongs to epoch
    // p+1 and is therefore effective only at t_start(p+2).
    let mut t3 = Tl::new();
    t3.slot(1, 900)
        .checkpoint(
            vec![snap(A, SKR_MINT, 100), snap(A, XSOL_MINT, 100)],
            vec![],
            vec![supply(SKR_MINT, 100), supply(XSOL_MINT, 100)],
        )
        .pinned(&[XSOL_PROGRAM, LENDER_A])
        .no_debt(A, &[LENDER_A]);
    t3.slot(2, 900).register(A, AssetKind::XSol); // switch at the boundary
    t3.slot(3, 1000).close();
    let carry = EpochCarry {
        selections: BTreeMap::from([(A, AssetKind::Skr)]),
        ..EpochCarry::default()
    };
    let er3 = replay_epoch(&cfg, &refs2, carry, 900, t3.events, 1000).unwrap();
    assert_eq!(
        x_at(&er3.report, &A, 950),
        Some(Eligibility::Verified {
            units: 100,
            gross_lamports: 200,
            debt_lamports: 0,
            x_lamports: 200,
        }),
        "a boundary-time switch request keeps the epoch on its anchored asset"
    );
    assert_eq!(
        er3.carry.selections.get(&A),
        Some(&AssetKind::XSol),
        "...and takes effect at the NEXT boundary"
    );
}
