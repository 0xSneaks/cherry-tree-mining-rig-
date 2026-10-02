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
    // A PENDING wallet with NO seed record is unreadable state, not zero
    // seeds (§5 → §10 reserve): its K is unknown, so it blocks (Pass #6 R2
    // low advisory — the R1 code treated it as 0 and failed open). An
    // explicit 0 is known weight 0 and does not block.
    let (seeds, seg) = mk(Eligibility::Pending(PendingReason::RouteHalted), None);
    assert_eq!(classify_segment(&seg, &seeds), IntervalVerdict::Reserve);
    let (seeds, seg) = mk(Eligibility::Pending(PendingReason::RouteHalted), Some(0));
    assert_eq!(classify_segment(&seg, &seeds), IntervalVerdict::Settle);
    // Likewise a VERIFIED wallet with no seed record: its weight is
    // unknown, so H is unverified — reserve, never silently weight 0.
    let (mut seeds, seg) = mk(Eligibility::Pending(PendingReason::RouteHalted), Some(0));
    seeds.remove(&A);
    assert_eq!(classify_segment(&seg, &seeds), IntervalVerdict::Reserve);

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

// ------------------------------------------ P6R-4 / NEW-1 (remediation R2, D10)
//
// Settlement provenance (P6-D10). Since Pass #6 R2 the ONLY public
// settlement entry point is `settle_epoch(budget, window, &ReplayReport,
// &SeedTimeline)`: every interval's verdict and participant set is derived
// from the replay. There is no public interval type, no verdict input and
// no participant list, and a `ReplayReport`'s segments are private to the
// replay (see the `compile_fail` doctests on `settle_epoch`). These tests
// drive the canonical path end to end on real replays.

const R2_ES: i64 = 100; // epoch start (checkpoint-anchored, P6R-8)
const R2_DAY: i64 = 86_400;
const R2_EE: i64 = R2_ES + 3 * R2_DAY; // three one-day thirds
const R2_E: u64 = 513_698_631; // Superfruit-shaped fixture budget
/// 1.25e9 SKR units at the 2-lamport reference: X = 2.5 SOL-equivalent,
/// so a 250-seed wallet has w = K = 2.5e9 and, alone under the floor,
/// exactly E/5 of any settled stretch.
const R2_UNITS: u64 = 1_250_000_000;

/// A one-epoch replay: A, B and C each hold `R2_UNITS` SKR from before the
/// epoch, register at the given times (a time before `R2_ES` = registered
/// before the epoch), and truthful checkpoints land at `R2_ES` and at each
/// of `checkpoints`. The replay runs to `end`; anything after the last
/// checkpoint is the unreconciled tail.
fn r2_replay(regs: &[(Addr, i64)], checkpoints: &[i64], end: i64) -> ReplayReport {
    #[derive(Clone, Copy)]
    enum Act {
        Reg(Addr),
        Check,
    }
    let cfg = config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A]);
    let refs = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), R2_ES));
    let mut acts: Vec<(i64, u8, Act)> = regs.iter().map(|&(w, t)| (t, 0, Act::Reg(w))).collect();
    acts.push((R2_ES, 1, Act::Check));
    acts.extend(checkpoints.iter().map(|&t| (t, 1, Act::Check)));
    acts.sort_by_key(|&(t, o, _)| (t, o));
    let mut t = Tl::new();
    t.slot(1, 10);
    for w in [A, B, C] {
        t.init(w, SKR_MINT)
            .mint(w, SKR_MINT, R2_UNITS)
            .no_debt(w, &[LENDER_A]);
    }
    t.pinned(&[LENDER_A]);
    for (i, (ts, _, act)) in acts.into_iter().enumerate() {
        t.slot(2 + i as u64, ts);
        match act {
            Act::Reg(w) => {
                t.observe(SKR_MINT, price(2, 1)).register(w, AssetKind::Skr);
            }
            Act::Check => {
                t.close();
            }
        }
    }
    replay_epoch(&cfg, &refs, EpochCarry::default(), R2_ES, t.events, end)
        .unwrap()
        .report
}

fn r2_seeds(entries: &[(Addr, u64)]) -> SeedTimeline {
    let mut s = SeedTimeline::new();
    for &(w, n) in entries {
        s.observe(w, 0, n).unwrap();
    }
    s
}

fn r2_leaf(s: &EpochSettlement, wallet: &Addr) -> u64 {
    s.allocations
        .iter()
        .find(|a| &a.wallet == wallet)
        .map(|a| a.leaf_lamports)
        .unwrap_or(0)
}

/// `floor(E·(b−a)/T)` scheduled capacity of `[a, b)` in the R2 epoch.
fn r2_cap(a: i64, b: i64) -> u64 {
    let c = |t: i64| (u128::from(R2_E) * (t - R2_ES) as u128 / (R2_EE - R2_ES) as u128) as u64;
    c(b) - c(a)
}

/// R2-A — a legitimately registered, qualifying wallet over reconciled
/// history is paid its D5 share, and nothing a caller supplies can turn
/// that interval into not-earned: the verdict is derived from the replayed
/// segment (it holds A's finalized registration), so it is `Settle`.
#[test]
fn r2a_registered_earned_segment_cannot_be_forced_to_not_earned() {
    let report = r2_replay(&[(A, 60)], &[R2_EE], R2_EE);
    let seeds = r2_seeds(&[(A, 250), (B, 250), (C, 250)]);
    let s = settle_epoch(R2_E, R2_ES, R2_EE, &report, &seeds).unwrap();
    // w = 2.5e9 under the 12.5e9 floor: exactly floor(E/5), nothing held,
    // nothing labelled not-earned.
    assert_eq!(r2_leaf(&s, &A), R2_E / 5);
    assert_eq!(s.allocated_lamports, R2_E / 5);
    assert_eq!((s.reserved_lamports, s.not_earned_lamports), (0, 0));
    assert_eq!(s.unallocated_lamports, R2_E - R2_E / 5);
    assert!(s.conserves(R2_E));
    // Every replayed segment in the window is registered and classifies
    // Settle — there is no NotEarned anywhere for settlement to use.
    for seg in report.segments().iter().filter(|g| g.end > R2_ES) {
        assert_eq!(
            classify_segment(seg, &BTreeMap::from([(A, 250u64)])),
            IntervalVerdict::Settle
        );
    }
}

/// R2-B — the empty-participant spoof has no representation: participants
/// are derived from the replayed segment. The only remaining caller input
/// about a wallet, its seed record, cannot silently drop it either: a
/// MISSING seed record makes the wallet's weight unknown, so the interval
/// is HELD in `U` (reserve) — never paid out as unallocated/Z capacity.
#[test]
fn r2b_empty_participant_spoof_cannot_send_earned_capacity_to_z() {
    let report = r2_replay(&[(A, 60)], &[R2_EE], R2_EE);
    let honest = settle_epoch(R2_E, R2_ES, R2_EE, &report, &r2_seeds(&[(A, 250)])).unwrap();
    assert_eq!(r2_leaf(&honest, &A), R2_E / 5);

    // Omit A's seed record: fail closed.
    let omitted = settle_epoch(R2_E, R2_ES, R2_EE, &report, &SeedTimeline::new()).unwrap();
    assert_eq!(omitted.allocated_lamports, 0);
    assert_eq!(omitted.reserved_lamports, R2_E, "held in U, not released");
    assert_eq!(omitted.not_earned_lamports, 0);
    assert_eq!(omitted.unallocated_lamports, 0, "nothing reaches Z");
    assert!(omitted.conserves(R2_E));
}

/// R2-C — an unreconciled interval (the D10 wallet-B indexer-miss shape:
/// the post-checkpoint tail) is canonically `Reserve`, and there is no
/// input that relabels it: even with NO registration observed in it
/// (guard 2 — absence of an OBSERVED registration is Pending, never "not
/// earned") its whole capacity stays reserved in `U`.
#[test]
fn r2c_reserve_cannot_be_relabelled_not_earned() {
    // Nobody registered, checkpoint only at the epoch start: [ES, EE) is
    // the unreconciled tail.
    let report = r2_replay(&[], &[], R2_EE);
    let s = settle_epoch(R2_E, R2_ES, R2_EE, &report, &SeedTimeline::new()).unwrap();
    assert_eq!(s.reserved_lamports, R2_E);
    assert_eq!(s.not_earned_lamports, 0);
    assert_eq!(s.unallocated_lamports, 0);
    assert!(s.conserves(R2_E));

    // With B registered but unreconciled: still reserved in full, B's
    // entitlement intact for the later replay.
    let report = r2_replay(&[(B, R2_ES + R2_DAY)], &[R2_ES + R2_DAY - 1], R2_EE);
    let s = settle_epoch(R2_E, R2_ES, R2_EE, &report, &r2_seeds(&[(B, 250)])).unwrap();
    // [ES, ES+DAY−1) is reconciled with no registration: not earned.
    // Everything after the last checkpoint is held.
    assert_eq!(s.not_earned_lamports, r2_cap(R2_ES, R2_ES + R2_DAY - 1));
    assert_eq!(s.reserved_lamports, r2_cap(R2_ES + R2_DAY - 1, R2_EE));
    assert_eq!(s.allocated_lamports, 0);
    assert!(s.conserves(R2_E));
}

/// R2-D — a bare, caller-created outage-evidence value authorizes nothing:
/// `OutageEvidence` is a descriptive taxonomy with no path into
/// settlement, and a FABRICATED segment (Verified, registration-free —
/// the shape that classifies not-earned) cannot be handed to settlement
/// either: `settle_epoch` reads only the replay's own segments.
#[test]
fn r2d_fabricated_outage_evidence_cannot_authorize_terminal_loss() {
    // The taxonomy is intact (test 46 part 1) …
    for e in [
        OutageEvidence::RegistrationPauseFlag,
        OutageEvidence::FruitProgramAbsent,
        OutageEvidence::FruitProgramNonExecutable,
    ] {
        assert_eq!(classify_outage(e), OutageClass::RegistrationProgram);
    }
    // … and a fabricated registration-free Verified segment does classify
    // not-earned …
    let report = r2_replay(&[(A, 60)], &[R2_EE], R2_EE);
    let mut forged = report.segments().last().unwrap().clone();
    forged.wallets.clear();
    assert_eq!(
        classify_segment(&forged, &BTreeMap::new()),
        IntervalVerdict::NotEarned
    );
    // … but it has no route into settlement: the genuine report still
    // pays A in full, with nothing not-earned.
    let s = settle_epoch(R2_E, R2_ES, R2_EE, &report, &r2_seeds(&[(A, 250)])).unwrap();
    assert_eq!(r2_leaf(&s, &A), R2_E / 5);
    assert_eq!(s.not_earned_lamports, 0);
}

/// R2-E — canonical not-earned (test 46 wallet A, P6-D10 item 1): no
/// registration could land during `[ES, t2)`; history is reconciled; A
/// registers at `t2` and earns from `t2` only, with no catch-up. The
/// registration-free reconciled stretch is not earned (released to Z at
/// close). With wallet C ALREADY registered through the outage, C keeps
/// accruing (item 3) and nothing is not-earned.
#[test]
fn r2e_canonical_not_earned_still_behaves_correctly() {
    let t2 = R2_ES + R2_DAY;
    let seeds = r2_seeds(&[(A, 250), (C, 250)]);

    let report = r2_replay(&[(A, t2)], &[t2, R2_EE], R2_EE);
    let s = settle_epoch(R2_E, R2_ES, R2_EE, &report, &seeds).unwrap();
    assert_eq!(s.not_earned_lamports, r2_cap(R2_ES, t2));
    assert_eq!(s.reserved_lamports, 0);
    // A alone under the floor from t2: floor(cap(t2, EE)/5) up to the
    // per-segment acc floors (≤ 1 lamport).
    let want = r2_cap(t2, R2_EE) / 5;
    assert!(r2_leaf(&s, &A) <= want && want - r2_leaf(&s, &A) <= 1);
    assert!(s.conserves(R2_E));

    // C registered before the epoch: segments are never registration-free.
    let report = r2_replay(&[(C, 60), (A, t2)], &[t2, R2_EE], R2_EE);
    let s = settle_epoch(R2_E, R2_ES, R2_EE, &report, &seeds).unwrap();
    assert_eq!(s.not_earned_lamports, 0);
    assert_eq!(s.reserved_lamports, 0);
    // C: E/5 over [ES, t2) alone, then A joins; both under the floor, so
    // C's leaf is E/5 overall and A's is its [t2, EE) share.
    assert!(R2_E / 5 - r2_leaf(&s, &C) <= 1);
    assert!(want - r2_leaf(&s, &A) <= 1);
    assert!(s.conserves(R2_E));
}

/// R2-F — late reconciliation (test 46 wallet B): B's registration is
/// finalized at `t1` but the window after the last checkpoint is not yet
/// reconciled, so the capacity is RESERVED. When a later replay reconciles
/// it, settlement is recomputed from the full replay: B is paid from `t1`,
/// the reserve is released, and the cumulative leaf equals the
/// never-broken control EXACTLY — no double allocation, nothing lost.
#[test]
fn r2f_late_reconciliation_settles_once_without_double_allocation() {
    let t1 = R2_ES + R2_DAY;
    let seeds = r2_seeds(&[(A, 250), (B, 250)]);

    // First pass: checkpoint only at t1 − 1; B's stretch is unreconciled.
    let early = r2_replay(&[(A, 60), (B, t1)], &[t1 - 1], R2_EE);
    let s1 = settle_epoch(R2_E, R2_ES, R2_EE, &early, &seeds).unwrap();
    assert_eq!(s1.reserved_lamports, r2_cap(t1 - 1, R2_EE));
    assert_eq!(r2_leaf(&s1, &B), 0);
    assert!(s1.conserves(R2_E));

    // Later replay reconciles the window (checkpoints found).
    let late = r2_replay(&[(A, 60), (B, t1)], &[t1 - 1, R2_EE], R2_EE);
    let s2 = settle_epoch(R2_E, R2_ES, R2_EE, &late, &seeds).unwrap();
    assert_eq!(s2.reserved_lamports, 0);
    assert!(s2.conserves(R2_E));
    assert!(r2_leaf(&s2, &B) > 0);

    // Control: the never-broken world — the same finalized events,
    // reconciled at every checkpoint from the start — and the independent
    // expectation: both wallets sit under the floor, so each earns exactly
    // E/5 of every settled stretch it is registered in.
    let control_report = r2_replay(&[(A, 60), (B, t1)], &[t1 - 1, t1, R2_EE], R2_EE);
    let control = settle_epoch(R2_E, R2_ES, R2_EE, &control_report, &seeds).unwrap();
    let want_b = r2_cap(t1, R2_EE) / 5;
    assert!(
        want_b - r2_leaf(&s2, &B) <= 1,
        "B paid from t1, exactly once"
    );
    assert!(R2_E / 5 - r2_leaf(&s2, &A) <= 1);
    for w in [A, B] {
        assert!(r2_leaf(&s2, &w).abs_diff(r2_leaf(&control, &w)) <= 1);
        // Leaves are cumulative: the later root REPLACES the earlier one;
        // the earlier leaf is a lower bound, never an extra payment.
        assert!(r2_leaf(&s1, &w) <= r2_leaf(&s2, &w));
    }
    assert!(s2.allocated_lamports <= R2_E);
    // The reserve became allocation (and floor dust), never double-counted.
    assert_eq!(
        s2.allocated_lamports + s2.unallocated_lamports + s2.not_earned_lamports,
        R2_E
    );
}

/// R2 — seed counts follow the replay timeline: a segment is split where a
/// wallet's seed count changes, so a wallet below the threshold earns
/// nothing until it qualifies and earns its exact share after; a
/// contradictory seed observation is rejected; and settlement refuses a
/// window the replay does not cover (no free capacity from missing input).
#[test]
fn r2_seed_timeline_splits_segments_and_coverage_is_enforced() {
    let report = r2_replay(&[(A, 60)], &[R2_EE], R2_EE);
    let t_q = R2_ES + R2_DAY;
    let mut seeds = SeedTimeline::new();
    seeds.observe(A, 0, 100).unwrap(); // below T = 150
    seeds.observe(A, t_q, 250).unwrap();
    assert!(
        seeds.observe(A, t_q, 249).is_err(),
        "conflicting observation"
    );
    assert_eq!(seeds.at(&A, t_q - 1), Some(100));
    assert_eq!(seeds.at(&A, t_q), Some(250));
    assert_eq!(seeds.at(&B, t_q), None);

    let s = settle_epoch(R2_E, R2_ES, R2_EE, &report, &seeds).unwrap();
    let want = r2_cap(t_q, R2_EE) / 5;
    assert!(r2_leaf(&s, &A) <= want && want - r2_leaf(&s, &A) <= 1);
    assert_eq!(s.reserved_lamports, 0);
    assert!(s.conserves(R2_E));

    // Coverage: the replay starts at the first event (t = 10); a window
    // opening earlier is unreplayed input and is rejected, as is one
    // running past the replay's end.
    let one = r2_seeds(&[(A, 250)]);
    assert!(settle_epoch(R2_E, 0, R2_EE, &report, &one).is_err());
    assert!(settle_epoch(R2_E, R2_ES, R2_EE + 1, &report, &one).is_err());
    assert!(settle_epoch(R2_E, R2_ES, R2_ES, &report, &one).is_err());
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
