//! Canonical tests 40–46 (architecture §14.3, "Epochs and timeline").
//! All addresses, budgets, timestamps and weights are FIXTURES (gate
//! 14.2-D); nothing here is a production calibration.
//!
//! Tests 40–43 exercise the rigs-engine fruit schedule planner; tests
//! 44–46 exercise this crate's settlement. Alongside the canonical seven,
//! the Pass #6 frozen architecture decisions are pinned executably:
//!
//! * P6-D3 — one Fruit per wallet per epoch; a switch (including after an
//!   exit) is effective NEXT epoch, never within the current one;
//! * P6-D6 — an unverified shared denominator reserves the whole interval,
//!   while a VERIFIED route halt is known weight 0;
//! * P6-D7 — route halts persist across the epoch boundary until a
//!   `RouteResumed` event, and resumption is not retroactive;
//! * P6-D5 — exact settlement invariants under random partitions
//!   (conservation, flooring, cap, checkpoint-frequency independence, no
//!   redistribution).
//!
//! Test 41 is PRIMITIVE ONLY (P6-D2): the half-open `[FRUIT_T0, t_s)`
//! cutoff. Lock cohort closure (requested-weight capture, durations,
//! vesting, forfeiture routing, lock election) belongs to the later Lock
//! package and is deliberately NOT implemented or claimed here.

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

// ------------------------------------------------------------------ 41

/// TEST 41 — PRIMITIVE ONLY (P6-D2). The Lock registration window is the
/// half-open `[FRUIT_T0, t_s)` with `t_s` the Season start (§8). A
/// registration finalized strictly before `t_s` is included; AT `t_s` it
/// is excluded. This is the cutoff primitive ONLY — no Lock cohort
/// subsystem is implemented in Pass #6.
#[test]
fn t41_lock_registration_cutoff_primitive_only() {
    let t0 = 1_700_000_000i64;
    let ft0 = rigs::fruit_t0(t0);
    let (open, close) = rigs::registration_window(t0);
    assert_eq!((open, close), (ft0, ft0 + 518_400));
    // t_s IS the Season start (§8: "For the five-day Season beginning at
    // t_s=FRUIT_T0+6 days").
    let plan = rigs::plan_fruit(t0, rigs::CONTINUATION_BUDGET_R + 1);
    let season = plan.epoch_at(close).unwrap();
    assert_eq!(season.kind, rigs::EpochKind::Season);
    assert_eq!(season.t_start, close);
    assert_eq!(season.t_end, close + 5 * 86_400);

    // Included strictly before t_s; excluded at t_s (test 41).
    assert!(rigs::registration_finalized_in_window(t0, close - 1));
    assert!(rigs::registration_finalized_in_window(t0, open));
    assert!(!rigs::registration_finalized_in_window(t0, close));
    assert!(!rigs::registration_finalized_in_window(t0, close + 1));
    // Before public opening there is no registration window at all.
    assert!(!rigs::registration_finalized_in_window(t0, open - 1));
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

/// P6-D6 — a VERIFIED route halt is different from a wallet-level PENDING
/// condition. The halt is a known fact: the affected weight is 0 for the
/// halt period (resumption not retroactive), so the shared denominator
/// stays verifiable and the interval settles for everyone else. Any PENDING
/// that data or replay could later clear makes `H` unverified, and the
/// ENTIRE interval reserves.
#[test]
fn p6d6_verified_route_halt_is_weight_zero_other_pending_reserves() {
    let mk = |elig| {
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
        Segment {
            start: 0,
            end: 100,
            status: IntervalStatus::Verified,
            wallets,
        }
    };

    // Verified route halt: the interval still SETTLES; the halted wallet
    // contributes no weight.
    let halted = mk(Eligibility::Pending(PendingReason::RouteHalted));
    assert_eq!(classify_segment(&halted), IntervalVerdict::Settle);
    let inputs = weight_inputs(&halted, &BTreeMap::new());
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].wallet, A);

    // Every wallet-level PENDING that data or replay could later clear
    // reserves the whole interval.
    for reason in [
        PendingReason::Debt(crate::debt::DebtPending::Unreadable),
        PendingReason::DebtUnpriced,
        PendingReason::AdapterProgramUnknown,
        PendingReason::StakeUnverified,
        PendingReason::AdditionUnpriced,
        PendingReason::NavUnverified,
    ] {
        assert_eq!(
            classify_segment(&mk(Eligibility::Pending(reason))),
            IntervalVerdict::Reserve,
            "{reason:?}"
        );
    }
    // Unreconciled / unresolved history (an indexer gap, or the
    // post-checkpoint tail) also reserves: earned, held, replayed (§3).
    for status in [IntervalStatus::Unresolved, IntervalStatus::NotReconciled] {
        let mut seg = mk(Eligibility::Pending(PendingReason::RouteHalted));
        seg.status = status;
        assert_eq!(
            classify_segment(&seg),
            IntervalVerdict::Reserve,
            "{status:?}"
        );
    }

    // Executable consequence: a halted-wallet epoch settles normally with
    // H from the remaining wallets; a debt-pending-wallet epoch holds the
    // ENTIRE capacity reserved — nobody settles against an incomplete H.
    let e = 684_931_507u64;
    let settled = settle_epoch(
        e,
        0,
        86_400,
        &[interval(
            0,
            86_400,
            IntervalVerdict::Settle,
            vec![part(A, 200, 1_600_000_000)],
        )],
    )
    .unwrap();
    assert!(settled.allocated_lamports > 0);
    assert_eq!(settled.reserved_lamports, 0);
    let held = settle_epoch(
        e,
        0,
        86_400,
        &[interval(0, 86_400, IntervalVerdict::Reserve, vec![])],
    )
    .unwrap();
    assert_eq!(held.allocated_lamports, 0);
    assert_eq!(held.reserved_lamports, e);
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
    t.slot(2, 700).register(A, AssetKind::XSol); // the switch attempt
    t.slot(3, 900).close();
    let er1 = replay_epoch(&cfg, &epoch1, EpochCarry::default(), t.events.clone(), 900).unwrap();

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
    let er2 = replay_epoch(&cfg, &epoch2, er1.carry.clone(), t2.events.clone(), 1200).unwrap();

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
        t2.events.clone(),
        1200,
    )
    .unwrap();
    assert_eq!(x_at(&plain.report, &A, 1050), None);

    // Determinism.
    assert_eq!(
        replay_epoch(&cfg, &epoch2, er1.carry.clone(), t2.events.clone(), 1200).unwrap(),
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
    t.slot(2, 550).observe(SKR_MINT, price(2, 1));
    t.slot(3, 600).register(A, AssetKind::Skr);
    t.slot(4, 700).ev(EventKind::Exit { wallet: A });
    t.slot(5, 800).register(A, AssetKind::XSol);
    t.slot(6, 900).close();
    let er = replay_epoch(&cfg, &refs, EpochCarry::default(), t.events, 900).unwrap();
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
    t.slot(3, 150).route_halt(SKR_MINT);
    t.slot(4, 200).close();
    let er1 = replay_epoch(&cfg, &refs1, EpochCarry::default(), t.events, 200).unwrap();
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
    let er2 = replay_epoch(&cfg, &refs2, er1.carry.clone(), t2.events, 300).unwrap();
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
/// * checkpoint-frequency independence — flooring happens once on the
///   cumulative, so refining the partition changes no exact value and no
///   leaf at all;
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

        // Checkpoint-frequency independence: refine every settling
        // interval into two halves; exact values and leaves are IDENTICAL.
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
            assert_eq!((a.exact.num, a.exact.den), (b.exact.num, b.exact.den));
            assert_eq!(a.leaf_lamports, b.leaf_lamports, "seed {seed}");
        }
        assert_eq!(s2.reserved_lamports, s.reserved_lamports, "seed {seed}");
        assert_eq!(s2.not_earned_lamports, s.not_earned_lamports, "seed {seed}");

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
