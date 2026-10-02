//! Settlement KERNEL tests (Pass #6 R2 relocation). The fixed-point
//! accounting of P6-D5 is exercised here on synthetic, already-classified
//! intervals: since Pass #6 R2 the kernel (`settle_intervals`) and its
//! `EpochInterval` input are crate-private — the public `settle_epoch`
//! derives intervals from the replay only (P6-D10) — so tests that need
//! hand-built weights live inside the crate. The bodies of canonical tests
//! 44–46, P6R-1, P6R-3, P6R-7 and the P6-D5 invariants are unchanged from
//! `tests/epochs_timeline.rs` at the R1 commit apart from calling the
//! kernel by its name. The public, replay-driven path is covered in
//! `tests/epochs_timeline.rs` (P6R-4 / R2 provenance tests).

use std::collections::BTreeMap;

use crate::settlement::{settle_intervals, EpochInterval};
use crate::*;
use rigs_engine as rigs;

const A: Addr = Addr::fixture(0x41);
const B: Addr = Addr::fixture(0x42);
const C: Addr = Addr::fixture(0x43);

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
    let s = settle_intervals(
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
    let s = settle_intervals(
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
    let s = settle_intervals(
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
        let solo = settle_intervals(
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
    let control = settle_intervals(
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
    let program = settle_intervals(
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
    let data = settle_intervals(
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
    let replayed = settle_intervals(
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
    let plain = settle_intervals(
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
    let held = settle_intervals(
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
    let solo = settle_intervals(
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
    let with_pending = settle_intervals(
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

// ------------------------------------------------- P6R-4 kernel self-check

/// P6R-4 (kernel half) — the kernel itself refuses a not-earned interval
/// carrying registered wallets, and accounts a genuine not-earned interval
/// as released capacity. `settle_epoch` can never build either shape from
/// a caller label (it derives verdicts); this pins the kernel's own
/// self-check and test 46's accounting.
#[test]
fn p6r4_kernel_rejects_not_earned_over_registered_wallets() {
    // (4) The economics layer refuses the erase: an already-finalized
    // registration cannot be removed by a caller-supplied outage label
    // passed straight to the kernel (the preserved R1 implementation
    // accepted it and silently dropped the wallets' accrual).
    let err = settle_intervals(
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
    let s = settle_intervals(
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

        let s = settle_intervals(budget, t_start, t_end, &intervals)
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
            let p = settle_intervals(budget, t_start, t_end, &clipped).unwrap();
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
        let s2 = settle_intervals(budget, t_start, t_end, &refined)
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
                let s3 = settle_intervals(budget, t_start, t_end, &without).unwrap();
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
    let s = settle_intervals(e, t_start, t_end, &intervals)
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
    let s = settle_intervals(e, 0, t_end, &intervals)
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
