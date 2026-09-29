//! Per-epoch Fruit settlement — architecture §1 (weights, denominator, APR),
//! §3 (outage split), §10 (solvency, exact cumulative entitlements); Pass #6
//! frozen decisions P6-D5 (rounding), P6-D6 (pending wallets / shared
//! denominator), P6-D10 (outage evidence); canonical tests 44–46.
//!
//! This module is the off-chain allocation half of section 10a: it computes,
//! from a replayed epoch, each wallet's exact cumulative allocation
//!
//! ```text
//! R_i = E · Σ_j w_ij Δt_j / (H_j · T)        over settling intervals j
//! w_i = 1[S_i ≥ T_seed] min(P_ref·min(S_i, M), X_i)
//! H   = max(Cw, 12.5 SOL-equivalent)
//! ```
//!
//! and hands the leaf amounts (`floor` of the exact cumulative, P6-D5) to the
//! on-chain Fruit ledger as cumulative Merkle leaves. The on-chain envelope
//! ([`crate`]-external, `rigs_engine::fruit`) enforces the budget.
//!
//! Accounting rules, exactly as frozen:
//!
//! * **Fixed-precision accumulator (P6-D5).** Per settled segment
//!   `[a, b)`: `acc_seg = floor(E · (b−a) · SCALE / (D · H_seg))` with
//!   `SCALE = 10^18`. Each wallet keeps ONE cumulative integer
//!   `Σ_seg w_i,seg · acc_seg` (`u128`, checked), and the leaf is
//!   `floor(cumulative / SCALE)`, capped at `floor(E/5)`. Flooring of the
//!   wallet happens ONCE, on the cumulative amount, so checkpoint
//!   frequency cannot change entitlement beyond the frozen sub-lamport
//!   drift of the per-segment `acc` floors (`≤ w/SCALE` per segment,
//!   §10 "carried remainders"). No exact-fraction arithmetic: unreduced
//!   fraction denominators compound across segments and overflow at
//!   realistic scales.
//! * **Never round up.** Every operation floors: `leaf ≤ exact`;
//!   `Σ leaves ≤ E`; `leaf_i ≤ floor(E/5)` (the `qmax = 1/5` per-wallet
//!   ceiling).
//! * **The rounding remainder is not redistributed.** Whatever the floors
//!   leave behind stays unallocated and returns `U → Z` at pool close —
//!   exactly once, never to other wallets (test 45).
//! * **An unverified shared denominator reserves the whole interval.** If
//!   any seed-qualified registered wallet is PENDING — for ANY reason,
//!   `RouteHalted` included (P6-D6 item 5: no Pass #6 resolution semantics
//!   for specific Pending reasons) — NO wallet settles against the
//!   incomplete `H` (P6-D6): the interval's scheduled capacity stays in
//!   `U`, and the pending wallet's share is never redistributed to the
//!   others. A PENDING wallet below the seed threshold (`S_i < 150`) has
//!   weight 0 by definition and cannot change `H`, so it does not block.
//! * **Outage classification (P6-D10).** Only finalized on-chain Fruit
//!   program state — the canonical registration pause flag, or the program
//!   absent / non-executable — makes an interval **not earned**; its
//!   capacity is released to the reserve (`Z`) at close. RPC, indexer,
//!   frontend, operator, backend and per-ledger pause evidence are DATA
//!   outages: the interval stays **earned-but-reserved** in `U` and settles
//!   when the replay completes (test 46).

use std::collections::BTreeMap;

use crate::error::{EligibilityError, EligibilityResult};
use crate::rational::Ratio;
use crate::replay::{Eligibility, IntervalStatus, Segment};
use crate::types::Addr;

/// Frozen seed-price reference `P_ref` (§1): lamports per seed. The same
/// frozen constant as `rigs_engine::fruit::P_REF_LAMPORTS`; the two crates
/// are deliberately independent, so each states its own copy with the
/// citation.
pub const P_REF_LAMPORTS: u128 = 10_000_000;
/// Fruit benchmark `M = 250` seeds (§1): `K_i = P_ref · min(S_i, M)`.
pub const SEED_BENCHMARK_M: u64 = 250;
/// Qualification threshold `T = ceil(3M/5) = 150` current seeds (§1).
pub const SEED_THRESHOLD_T: u64 = 150;
/// `Hfloor = Kmax/qmax = 12.5 SOL-equivalent`, in lamports (§1). With
/// `Kmax = 2_500_000_000` this is exactly `5 · Kmax`, which is what makes
/// `w_i/H ≤ 1/5` hold for every possible weight.
pub const DENOMINATOR_FLOOR: u128 = 12_500_000_000;
/// One day in seconds (§1); APR is quoted per year from a per-day rate.
pub const DAY_SECONDS: u128 = 86_400;
/// APR scale: `APR_weight = 36,500 · r/H` in percent per year (§1).
pub const APR_PERCENT_SCALE: u128 = 36_500;
/// Fixed-point scale of the settlement accumulator (P6-D5): per settled
/// segment `acc_seg = floor(E·Δt·SCALE/(D·H_seg))`, and each wallet's leaf
/// is `floor(Σ_seg w_i,seg·acc_seg / SCALE)`.
pub const ACCUMULATOR_SCALE: u128 = 1_000_000_000_000_000_000;

/// `w_i(t) = 1[S_i ≥ T] · min(K_i, X_i)` with `K_i = P_ref · min(S_i, M)`
/// (§1). Weight 0 for a wallet below the seed threshold or with no equity.
pub fn recognized_weight(seeds: u64, x_lamports: u128) -> u128 {
    if seeds < SEED_THRESHOLD_T {
        return 0;
    }
    let k = P_REF_LAMPORTS * u128::from(seeds.min(SEED_BENCHMARK_M));
    k.min(x_lamports)
}

/// `H(t) = max(Cw(t), 12.5 SOL-equivalent)` (§1): the shared denominator.
pub const fn shared_denominator(cw: u128) -> u128 {
    if cw > DENOMINATOR_FLOOR {
        cw
    } else {
        DENOMINATOR_FLOOR
    }
}

/// `APR_weight = 36,500 · r/H` percent per year (§1), exact. `r` is the
/// pool's rate in lamports per day (`r_p = E/D`).
pub fn apr_weight(r_lamports_per_day: u128, h: u128) -> EligibilityResult<Ratio> {
    Ok(Ratio::new(
        APR_PERCENT_SCALE
            .checked_mul(r_lamports_per_day)
            .ok_or(EligibilityError::MathOverflow)?,
        h,
    ))
}

/// `APR_pool = 36,500 · r · Cw / (H · Cn)` percent per year (§1). **N/A**
/// (`None`) with zero participants (`Cn = 0`) — never a fabricated number
/// (test 44).
pub fn apr_pool(
    r_lamports_per_day: u128,
    cw: u128,
    h: u128,
    cn_lamports: u128,
) -> EligibilityResult<Option<Ratio>> {
    if cn_lamports == 0 {
        return Ok(None);
    }
    let num = APR_PERCENT_SCALE
        .checked_mul(r_lamports_per_day)
        .and_then(|v| v.checked_mul(cw))
        .ok_or(EligibilityError::MathOverflow)?;
    let den = h
        .checked_mul(cn_lamports)
        .ok_or(EligibilityError::MathOverflow)?;
    Ok(Some(Ratio::new(num, den)))
}

/// Evidence that a registration/selection program outage was claimed for
/// some interval (P6-D10). Only the first three variants are finalized
/// ON-CHAIN Fruit-program state; everything else is a data outage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutageEvidence {
    /// The canonical registration pause flag of the Fruit program.
    RegistrationPauseFlag,
    /// The Fruit program account does not exist.
    FruitProgramAbsent,
    /// The Fruit program exists but is not executable.
    FruitProgramNonExecutable,
    /// RPC was unreachable. Data outage.
    RpcOutage,
    /// The indexer was down or behind. Data outage.
    IndexerOutage,
    /// The frontend was down. Data outage.
    FrontendOutage,
    /// An operator reported an outage. Data outage — a report is not
    /// finalized on-chain state.
    OperatorReport,
    /// A backend service failed. Data outage.
    BackendFailure,
    /// A per-ledger pause flag (not the canonical registration pause).
    /// Data outage.
    PerLedgerPauseFlag,
}

/// The classification an outage evidence piece forces (P6-D10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutageClass {
    /// Registration/program outage: the interval is **not earned**; its
    /// capacity flows to the reserve at close.
    RegistrationProgram,
    /// Data outage: the interval **is earned**; it stays reserved/pending
    /// and settles when data replay completes.
    Data,
}

/// Classify outage evidence. Only finalized on-chain Fruit program state
/// establishes a registration-program outage (P6-D10).
pub const fn classify_outage(e: OutageEvidence) -> OutageClass {
    match e {
        OutageEvidence::RegistrationPauseFlag
        | OutageEvidence::FruitProgramAbsent
        | OutageEvidence::FruitProgramNonExecutable => OutageClass::RegistrationProgram,
        OutageEvidence::RpcOutage
        | OutageEvidence::IndexerOutage
        | OutageEvidence::FrontendOutage
        | OutageEvidence::OperatorReport
        | OutageEvidence::BackendFailure
        | OutageEvidence::PerLedgerPauseFlag => OutageClass::Data,
    }
}

/// How one constant-state interval settles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntervalVerdict {
    /// Verified history, no reserving PENDING wallet: allocate
    /// `E · w_i/H · Δt/T` exactly (P6-D5).
    Settle,
    /// The shared denominator is unverified for this interval (an
    /// unresolvable wallet-level PENDING, or history not yet reconciled):
    /// the ENTIRE interval's scheduled capacity stays in `U` (P6-D6).
    /// No wallet settles against an incomplete `H`.
    Reserve,
    /// A verified registration-program outage (P6-D10): the interval is
    /// **not earned**; its capacity is released to `Z` at pool close.
    /// Reachable ONLY through [`classify_interval`] with canonical
    /// on-chain evidence over reconciled history containing no finalized
    /// registration — never a caller's bare label.
    NotEarned,
}

/// Classify one interval for settlement from the replayed [`Segment`], the
/// seed counts, and any outage evidence observed for it (P6-D6 + P6-D10).
///
/// * No outage evidence — the plain P6-D6 rule of [`classify_segment`].
/// * A DATA outage (RPC/indexer/frontend/operator/backend/per-ledger) —
///   the interval is **earned**: verdict is still [`classify_segment`]'s
///   (an unreconciled or pending-tainted segment reserves and stays
///   replayable; a clean one settles). Operator and indexer input can
///   never declare earned capacity "not earned".
/// * A REGISTRATION-PROGRAM outage (only the finalized on-chain forms:
///   the canonical pause flag, the program absent, the program
///   non-executable) — **not earned** only when BOTH hold:
///   1. the segment's history is reconciled (`IntervalStatus::Verified`;
///      P6-D10 guard 2 — absence of an OBSERVED registration inside an
///      unreconciled window is Pending, never "not earned"), and
///   2. no finalized registration exists in the segment (`wallets` empty
///      — presence means registered on chain). Wallets already registered
///      keep accruing during a registration-program outage (P6-D10 item
///      3), so a registered segment falls back to the normal rule
///      instead.
pub fn classify_interval(
    seg: &Segment,
    seeds: &BTreeMap<Addr, u64>,
    outage: Option<OutageEvidence>,
) -> IntervalVerdict {
    let Some(e) = outage else {
        return classify_segment(seg, seeds);
    };
    match classify_outage(e) {
        OutageClass::Data => classify_segment(seg, seeds),
        OutageClass::RegistrationProgram => {
            if seg.status == IntervalStatus::Verified && seg.wallets.is_empty() {
                IntervalVerdict::NotEarned
            } else {
                classify_segment(seg, seeds)
            }
        }
    }
}

/// Classify one replayed [`Segment`] (P6-D6):
///
/// * `Unresolved` / `NotReconciled` segments — an indexer gap or the
///   post-checkpoint tail — are **earned but reserved** (data outage
///   semantics, §3): [`IntervalVerdict::Reserve`].
/// * A `Verified` segment settles UNLESS some wallet registered in it is
///   PENDING **with qualifying weight potential**: seed-qualified
///   (`S_i ≥ 150`, from `seeds`) — its `w_i ∈ [0, K_i]` is unknown, so
///   `H_seg` is unverified and the whole interval reserves (P6-D6 item 1).
///   EVERY PENDING reason blocks, `RouteHalted` included: Pass #6 adds no
///   resolution semantics for specific Pending reasons, they stay reserved
///   until the eligibility engine resolves them (P6-D6 item 5) — a halt is
///   never silently converted into weight 0, which would redistribute the
///   halted wallet's share to everyone else.
/// * A PENDING wallet below the seed threshold has `w_i = 0` BY DEFINITION
///   (`w_i = 1[S_i ≥ T]·min(K_i, X_i)`): it cannot change `H_seg` and does
///   NOT block the interval — an ineligible/non-qualifying wallet never
///   halts everyone else's settlement (P6-D6 item 1, P6R-7).
///   `Ineligible` wallets are likewise known weight 0 (test 32).
pub fn classify_segment(seg: &Segment, seeds: &BTreeMap<Addr, u64>) -> IntervalVerdict {
    match seg.status {
        IntervalStatus::Unresolved | IntervalStatus::NotReconciled => IntervalVerdict::Reserve,
        IntervalStatus::Verified => {
            let blocking = seg.wallets.iter().any(|(w, e)| {
                matches!(e, Eligibility::Pending(_))
                    && seeds.get(w).copied().unwrap_or(0) >= SEED_THRESHOLD_T
            });
            if blocking {
                IntervalVerdict::Reserve
            } else {
                IntervalVerdict::Settle
            }
        }
    }
}

/// One wallet's weight inputs on one interval: its seed count (`Miner.rigs`,
/// P6-D3 — the seed timeline runs on the same timeline as the replay) and
/// its verified eligible equity `X_i`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Participant {
    pub wallet: Addr,
    pub seeds: u64,
    pub x_lamports: u128,
}

/// One constant-state interval of the epoch, classified for settlement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpochInterval {
    pub start: i64,
    pub end: i64,
    pub verdict: IntervalVerdict,
    /// Weight inputs. Meaningful for `Settle` intervals; must be EMPTY for
    /// `NotEarned` ones (a not-earned interval holds no finalized
    /// registration, P6-D10 — non-empty input is rejected as an attempt to
    /// erase registered accrual with a bare label); ignored for `Reserve`.
    pub participants: Vec<Participant>,
}

/// Weight inputs extracted from one replayed segment: every VERIFIED
/// wallet's `X`, joined with its seed count. PENDING and INELIGIBLE wallets
/// contribute nothing (weight 0; a reserving PENDING must instead make the
/// whole interval [`IntervalVerdict::Reserve`] via [`classify_segment`]).
pub fn weight_inputs(seg: &Segment, seeds: &BTreeMap<Addr, u64>) -> Vec<Participant> {
    seg.wallets
        .iter()
        .filter_map(|(w, e)| match e {
            Eligibility::Verified { x_lamports, .. } => Some(Participant {
                wallet: *w,
                seeds: seeds.get(w).copied().unwrap_or(0),
                x_lamports: *x_lamports,
            }),
            _ => None,
        })
        .collect()
}

/// `floor(E · (min(t, t_end) - t_start) / (t_end - t_start))` — §10's
/// cumulative scheduled capacity (the same formula the on-chain root check
/// enforces; identical to `rigs_engine::scheduled_capacity`).
fn capacity(budget: u64, t_start: i64, t_end: i64, t: i64) -> EligibilityResult<u64> {
    let duration = i128::from(t_end) - i128::from(t_start);
    let elapsed = (i128::from(t.min(t_end)) - i128::from(t_start)).max(0);
    let cap = i128::from(budget) * elapsed / duration;
    u64::try_from(cap).map_err(|_| EligibilityError::MathOverflow)
}

/// One wallet's settlement outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalletSettlement {
    pub wallet: Addr,
    /// The leaf amount: `floor` of the cumulative fixed-precision
    /// allocation, capped at `floor(E/5)` (P6-D5). Paid through cumulative
    /// Merkle leaves.
    pub leaf_lamports: u64,
    /// The cumulative allocation `Σ_seg w_i,seg · acc_seg` over
    /// `ACCUMULATOR_SCALE` (always ≥ the leaf).
    pub exact: Ratio,
}

/// APR figures of the epoch, from its final settling interval (§1). The
/// pool APR is `None` (N/A) with zero active participants (test 44).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EpochApr {
    pub weight: Ratio,
    pub pool: Option<Ratio>,
    pub cw: u128,
    pub h: u128,
    /// `Cn = P_ref · Σ S_i` over wallets at/above the seed threshold (§1).
    pub cn_lamports: u128,
}

/// The complete settlement of one funded epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpochSettlement {
    /// Per-wallet outcomes, address order. Only wallets that appeared in a
    /// settling interval are listed.
    pub allocations: Vec<WalletSettlement>,
    /// `Σ leaf_lamports` — never above `E`.
    pub allocated_lamports: u64,
    /// Scheduled capacity of reserved intervals, held in `U` until the
    /// replay completes (data outages, unverified `H`; P6-D6).
    pub reserved_lamports: u64,
    /// Scheduled capacity of not-earned intervals, released `U → Z` at
    /// pool close (registration-program outages; P6-D10).
    pub not_earned_lamports: u64,
    /// Everything else, released `U → Z` at pool close: floor remainders,
    /// the below-floor share of the denominator, and capacity of instants
    /// with no participants (tests 44–45). NEVER redistributed.
    pub unallocated_lamports: u64,
    pub apr: EpochApr,
}

impl EpochSettlement {
    /// Conservation: every lamport of `E` is exactly one of allocated,
    /// reserved, not-earned or unallocated (P6-D5).
    pub fn conserves(&self, budget: u64) -> bool {
        self.allocated_lamports
            .saturating_add(self.reserved_lamports)
            .saturating_add(self.not_earned_lamports)
            .saturating_add(self.unallocated_lamports)
            == budget
    }
}

/// Settle one epoch. `intervals` must partition `[t_start, t_end)` exactly
/// (contiguous, in order, no gaps, no overlaps): every second of the epoch
/// is classified by the caller — a stretch with no verdict at all is not
/// free capacity, it is missing input, and is rejected.
///
/// Fixed-precision note (P6-D5): the accumulator is the frozen integer
/// `acc_seg = floor(E·Δt_seg·SCALE/(D·H_seg))` per segment and one
/// cumulative `Σ_seg w_i,seg·acc_seg` per wallet, in `u128` checked
/// arithmetic. Reachable production bounds cannot overflow it: the largest
/// possible epoch budget is the Storm ceiling `8,561,643,836` lamports and
/// the longest epoch is the core end `950,400` seconds, so the `acc`
/// numerator is ≤ `8.6e9 · 9.5e5 · 1e18 ≈ 8.2e33` and the wallet-side
/// cumulative is ≤ `Kmax · E·SCALE/Hfloor ≈ 1.8e27`, both far below
/// `u128::MAX ≈ 3.4e38`. A hypothetical out-of-range input fails closed
/// with [`EligibilityError::MathOverflow`] rather than rounding.
pub fn settle_epoch(
    budget: u64,
    t_start: i64,
    t_end: i64,
    intervals: &[EpochInterval],
) -> EligibilityResult<EpochSettlement> {
    if budget == 0 {
        return Err(EligibilityError::InvariantViolation("zero-budget epoch"));
    }
    if t_end <= t_start {
        return Err(EligibilityError::InvariantViolation("empty epoch window"));
    }
    let duration = u128::try_from(t_end - t_start).map_err(|_| EligibilityError::MathOverflow)?;
    let mut cursor = t_start;
    for iv in intervals {
        if iv.start != cursor || iv.end <= iv.start {
            return Err(EligibilityError::InvariantViolation(
                "intervals must partition the epoch exactly",
            ));
        }
        cursor = iv.end;
    }
    if cursor != t_end {
        return Err(EligibilityError::InvariantViolation(
            "intervals must partition the epoch exactly",
        ));
    }

    // Per-wallet cumulative fixed-point entitlement: Σ_seg w_i,seg · acc_seg
    // (P6-D5). One integer per wallet; flooring happens once, at the leaf.
    let mut cumulative: BTreeMap<Addr, u128> = BTreeMap::new();
    let mut last_settle_observation: Option<(u128, u128, u128)> = None; // (cw, h, cn)
    let mut reserve_claims: Vec<(i64, i64)> = Vec::new(); // timeline order
    let mut not_earned_claims: Vec<(i64, i64)> = Vec::new();

    for iv in intervals {
        let dt = u128::try_from(iv.end - iv.start).map_err(|_| EligibilityError::MathOverflow)?;
        match iv.verdict {
            IntervalVerdict::Settle => {
                // Two passes: the shared denominator is the interval's FULL
                // H = max(Cw, floor), so every term must divide by the same
                // final H, not by the running partial sum.
                let mut weights: Vec<(Addr, u128)> = Vec::with_capacity(iv.participants.len());
                let mut cw: u128 = 0;
                let mut cn_seeds: u128 = 0;
                let mut seen: BTreeMap<Addr, ()> = BTreeMap::new();
                for p in &iv.participants {
                    if seen.insert(p.wallet, ()).is_some() {
                        return Err(EligibilityError::InvariantViolation(
                            "duplicate wallet in one interval",
                        ));
                    }
                    let w = recognized_weight(p.seeds, p.x_lamports);
                    cw = cw.checked_add(w).ok_or(EligibilityError::MathOverflow)?;
                    if p.seeds >= SEED_THRESHOLD_T {
                        cn_seeds = cn_seeds
                            .checked_add(u128::from(p.seeds))
                            .ok_or(EligibilityError::MathOverflow)?;
                    }
                    weights.push((p.wallet, w));
                }
                let h = shared_denominator(cw);
                // acc_seg = floor(E · Δt · SCALE / (D · H_seg)), downward
                // only (P6-D5); drift ≤ 1/SCALE per segment.
                let acc_seg = (u128::from(budget)
                    .checked_mul(dt)
                    .and_then(|v| v.checked_mul(ACCUMULATOR_SCALE))
                    .ok_or(EligibilityError::MathOverflow)?)
                    / (duration
                        .checked_mul(h)
                        .ok_or(EligibilityError::MathOverflow)?);
                for (wallet, w) in weights {
                    if w == 0 {
                        continue;
                    }
                    let term = w
                        .checked_mul(acc_seg)
                        .ok_or(EligibilityError::MathOverflow)?;
                    let acc = cumulative.entry(wallet).or_insert(0);
                    *acc = acc
                        .checked_add(term)
                        .ok_or(EligibilityError::MathOverflow)?;
                }
                last_settle_observation = Some((cw, h, P_REF_LAMPORTS * cn_seeds));
            }
            IntervalVerdict::Reserve => reserve_claims.push((iv.start, iv.end)),
            IntervalVerdict::NotEarned => {
                // P6-D10: terminal not-earned treatment needs canonical
                // reconciled evidence; weight inputs for a not-earned
                // interval mean registered wallets whose accrual the label
                // would erase. Refuse — caller-controlled classification
                // must not decide economics.
                if !iv.participants.is_empty() {
                    return Err(EligibilityError::InvariantViolation(
                        "not-earned interval cannot carry registered wallets",
                    ));
                }
                not_earned_claims.push((iv.start, iv.end));
            }
        }
    }

    // Leaves: floor of the cumulative fixed-point entitlement, capped at the
    // E/5 wallet ceiling. The cap is defensive — `w ≤ Kmax` and
    // `H ≥ 5·Kmax` already bound `exact ≤ E/5` — but settlement never
    // trusts its inputs' shape.
    let wallet_cap = budget / 5;
    let mut allocations: Vec<WalletSettlement> = Vec::with_capacity(cumulative.len());
    let mut allocated: u64 = 0;
    for (wallet, c) in &cumulative {
        let leaf = u64::try_from(c / ACCUMULATOR_SCALE)
            .map_err(|_| EligibilityError::MathOverflow)?
            .min(wallet_cap);
        allocated = allocated
            .checked_add(leaf)
            .ok_or(EligibilityError::MathOverflow)?;
        allocations.push(WalletSettlement {
            wallet: *wallet,
            leaf_lamports: leaf,
            exact: Ratio::new(*c, ACCUMULATOR_SCALE),
        });
    }

    // Reserved / not-earned capacity, each interval's scheduled capacity
    // `cap(end) - cap(start)`, clamped in timeline order (reserved first —
    // it may still be owed — then not-earned) so that
    // allocated + reserved + not_earned ≤ E holds EXACTLY even in the
    // degenerate sub-lamport regime where cumulative wallet floors can
    // outrun a single interval's capacity difference (possible only when
    // `E/T` is below one lamport per second-scale fractions; at production
    // scales the clamp provably never engages — the property tests assert
    // it). What the clamp removes is simply unallocated instead.
    let mut remaining =
        budget
            .checked_sub(allocated)
            .ok_or(EligibilityError::InvariantViolation(
                "allocations exceed budget",
            ))?;
    let claim = |windows: &[(i64, i64)], remaining: &mut u64| -> EligibilityResult<u64> {
        let mut total = 0u64;
        for &(s, e) in windows {
            let diff = capacity(budget, t_start, t_end, e)? - capacity(budget, t_start, t_end, s)?;
            let take = diff.min(*remaining);
            *remaining -= take;
            total += take;
        }
        Ok(total)
    };
    let reserved = claim(&reserve_claims, &mut remaining)?;
    let not_earned = claim(&not_earned_claims, &mut remaining)?;
    let unallocated = remaining;

    // APR (§1) from the final settling interval; with no settling interval
    // at all there are no participants: Cn = 0 and the pool APR is N/A.
    let r_per_day = u128::from(budget)
        .checked_mul(DAY_SECONDS)
        .and_then(|v| v.checked_div(duration))
        .ok_or(EligibilityError::MathOverflow)?;
    let (cw, h, cn) = last_settle_observation.unwrap_or((0, DENOMINATOR_FLOOR, 0));
    let apr = EpochApr {
        weight: apr_weight(r_per_day, h)?,
        pool: apr_pool(r_per_day, cw, h, cn)?,
        cw,
        h,
        cn_lamports: cn,
    };

    Ok(EpochSettlement {
        allocations,
        allocated_lamports: allocated,
        reserved_lamports: reserved,
        not_earned_lamports: not_earned,
        unallocated_lamports: unallocated,
        apr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const W1: Addr = Addr::fixture(0x51);

    fn participant(wallet: Addr, seeds: u64, x: u128) -> Participant {
        Participant {
            wallet,
            seeds,
            x_lamports: x,
        }
    }

    #[test]
    fn weights_follow_the_frozen_equation() {
        // Below the seed threshold: weight 0 however rich.
        assert_eq!(recognized_weight(149, 10_000_000_000), 0);
        // At the threshold, fully covered: K = P·S.
        assert_eq!(recognized_weight(150, 3_000_000_000), 1_500_000_000);
        // Benchmark cap: K = P·min(S, 250) even at 10,000 seeds.
        assert_eq!(recognized_weight(10_000, 3_000_000_000), 2_500_000_000);
        // Dust equity: weight is dust-proportional.
        assert_eq!(recognized_weight(200, 200_000_000), 200_000_000);
        // The denominator floor is 12.5 SOL-equivalent exactly.
        assert_eq!(shared_denominator(0), DENOMINATOR_FLOOR);
        assert_eq!(shared_denominator(12_500_000_000), DENOMINATOR_FLOOR);
        assert_eq!(shared_denominator(12_500_000_001), 12_500_000_001);
    }

    #[test]
    fn apr_pool_is_none_exactly_when_no_participants() {
        let r = 342u128; // lamports/day fixture (small enough that the
                         // exact cross-multiplied comparison below stays inside u128)
        assert!(apr_pool(r, 0, DENOMINATOR_FLOOR, 0).unwrap().is_none());
        assert!(apr_pool(r, 4_000_000_000, DENOMINATOR_FLOOR, 0)
            .unwrap()
            .is_none());
        let some = apr_pool(r, 4_000_000_000, 12_500_000_000, 1_500_000_000)
            .unwrap()
            .unwrap();
        // 36,500 · r · Cw / (H · Cn), cross-multiplied equality is exact.
        let expect = Ratio::new(
            36_500 * r * 4_000_000_000,
            12_500_000_000u128 * 1_500_000_000,
        );
        assert!(some.le(&expect).unwrap() && expect.le(&some).unwrap());
    }

    #[test]
    fn outage_evidence_splits_exactly_as_frozen() {
        use OutageEvidence as E;
        for e in [
            E::RegistrationPauseFlag,
            E::FruitProgramAbsent,
            E::FruitProgramNonExecutable,
        ] {
            assert_eq!(classify_outage(e), OutageClass::RegistrationProgram);
        }
        for e in [
            E::RpcOutage,
            E::IndexerOutage,
            E::FrontendOutage,
            E::OperatorReport,
            E::BackendFailure,
            E::PerLedgerPauseFlag,
        ] {
            assert_eq!(classify_outage(e), OutageClass::Data);
        }
    }

    #[test]
    fn intervals_must_partition_the_epoch_exactly() {
        let p = vec![participant(W1, 250, 2_500_000_000)];
        let ok = vec![EpochInterval {
            start: 0,
            end: 100,
            verdict: IntervalVerdict::Settle,
            participants: p.clone(),
        }];
        assert!(settle_epoch(1_000, 0, 100, &ok).is_ok());
        // Gap at the end.
        let gapped = vec![EpochInterval {
            start: 0,
            end: 90,
            verdict: IntervalVerdict::Settle,
            participants: p.clone(),
        }];
        assert!(settle_epoch(1_000, 0, 100, &gapped).is_err());
        // Overlap.
        let overlap = vec![
            EpochInterval {
                start: 0,
                end: 60,
                verdict: IntervalVerdict::Settle,
                participants: p.clone(),
            },
            EpochInterval {
                start: 50,
                end: 100,
                verdict: IntervalVerdict::Settle,
                participants: p,
            },
        ];
        assert!(settle_epoch(1_000, 0, 100, &overlap).is_err());
        // A zero-budget epoch is not a pool.
        assert!(settle_epoch(0, 0, 100, &[]).is_err());
    }

    #[test]
    fn full_epoch_single_wallet_low_denominator_floors_and_conserves() {
        // Cw = 2.5 SOL < 12.5 SOL: the floor binds; only E/5 can ever leave;
        // the rest is unallocated and never redistributed (test 45 core).
        let e = 1_000_000_007u64;
        let ivs = vec![EpochInterval {
            start: 0,
            end: 86_400,
            verdict: IntervalVerdict::Settle,
            participants: vec![participant(W1, 250, 2_500_000_000)],
        }];
        let s = settle_epoch(e, 0, 86_400, &ivs).unwrap();
        assert_eq!(s.allocations.len(), 1);
        let a = &s.allocations[0];
        // exact = E · 2.5/12.5 = E/5 = 200_000_001.4 → floor, at the cap.
        assert_eq!(a.leaf_lamports, 200_000_001);
        assert_eq!(a.leaf_lamports, e / 5);
        assert_eq!(s.allocated_lamports, 200_000_001);
        assert_eq!(s.reserved_lamports, 0);
        assert_eq!(s.not_earned_lamports, 0);
        assert_eq!(s.unallocated_lamports, e - 200_000_001);
        assert!(s.conserves(e));
    }
}
