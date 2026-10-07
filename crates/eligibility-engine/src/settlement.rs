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
//! * **Exact arithmetic only.** Allocations accumulate as reduced exact
//!   fractions (`u128`, checked); the leaf is `floor(exact cumulative)`.
//!   Flooring happens ONCE, on the cumulative amount, so checkpoint
//!   frequency cannot change entitlement (§10 "carried remainders").
//! * **Never round up.** `leaf ≤ exact`; `Σ leaves ≤ E`;
//!   `leaf_i ≤ floor(E/5)` (the `qmax = 1/5` per-wallet ceiling).
//! * **The rounding remainder is not redistributed.** Whatever the floors
//!   leave behind stays unallocated and returns `U → Z` at pool close —
//!   never to other wallets (test 45).
//! * **An unverified shared denominator reserves the whole interval.** If
//!   any wallet-level PENDING condition that data or replay could later
//!   clear taints the interval, NO wallet settles against the incomplete
//!   `H` (P6-D6): the interval's scheduled capacity stays in `U`. A
//!   VERIFIED route halt is the one carve-out: the affected asset's weight
//!   is 0 for the halt (known, not unknown), recognition stops there, and
//!   resumption is not retroactive — other wallets settle normally.
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
use crate::replay::{Eligibility, IntervalStatus, PendingReason, Segment};
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
    NotEarned,
}

/// Classify one replayed [`Segment`]:
///
/// * `Unresolved` / `NotReconciled` segments — an indexer gap or the
///   post-checkpoint tail — are **earned but reserved** (data outage
///   semantics, §3): [`IntervalVerdict::Reserve`].
/// * A `Verified` segment settles UNLESS some wallet is PENDING for a
///   reason data or replay could still clear (debt unreadable, program
///   UNKNOWN, unpriced additions, NAV unverified, …): any such wallet makes
///   `H` unverified, so the whole interval reserves (P6-D6).
/// * The one carve-out: `Pending(RouteHalted)` is a VERIFIED fact, not
///   missing data — the affected weight is 0 for the halt and resumption is
///   not retroactive (P6-D6), so the interval still settles for everyone
///   else. `Ineligible` wallets are likewise known weight 0 (test 32).
pub fn classify_segment(seg: &Segment) -> IntervalVerdict {
    match seg.status {
        IntervalStatus::Unresolved | IntervalStatus::NotReconciled => IntervalVerdict::Reserve,
        IntervalStatus::Verified => {
            let reserving = seg.wallets.values().any(|e| {
                matches!(e, Eligibility::Pending(r) if !matches!(r, PendingReason::RouteHalted))
            });
            if reserving {
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
    /// Weight inputs. Meaningful for `Settle` intervals; ignored elsewhere.
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

/// Reduced exact fraction (den > 0). Overflow fails closed: settlement is
/// never silently inexact (P6-D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Frac {
    num: u128,
    den: u128,
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

impl Frac {
    const ZERO: Frac = Frac { num: 0, den: 1 };

    fn new(num: u128, den: u128) -> Frac {
        let g = gcd(num, den).max(1);
        Frac {
            num: num / g,
            den: den / g,
        }
    }

    fn add(self, other: Frac) -> EligibilityResult<Frac> {
        let g = gcd(self.den, other.den);
        let den_l = other.den / g;
        let den_r = self.den / g;
        let num = self
            .num
            .checked_mul(den_l)
            .and_then(|a| other.num.checked_mul(den_r).and_then(|b| a.checked_add(b)))
            .ok_or(EligibilityError::MathOverflow)?;
        let den = self
            .den
            .checked_mul(den_l)
            .ok_or(EligibilityError::MathOverflow)?;
        Ok(Frac::new(num, den))
    }

    fn floor_u64(self) -> EligibilityResult<u64> {
        u64::try_from(self.num / self.den).map_err(|_| EligibilityError::MathOverflow)
    }
}

/// One wallet's settlement outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WalletSettlement {
    pub wallet: Addr,
    /// The leaf amount: `floor` of the exact cumulative allocation, capped
    /// at `floor(E/5)` (P6-D5). Paid through cumulative Merkle leaves.
    pub leaf_lamports: u64,
    /// The exact cumulative allocation (always ≥ the leaf).
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
/// Exactness note (P6-D5): each wallet's cumulative allocation is an exact
/// reduced fraction in `u128` checked arithmetic. At production scales
/// (billions of lamports over ≥ 86,400-second epochs) the accumulators stay
/// far below `u128`; a hypothetical overflow fails closed with
/// [`EligibilityError::MathOverflow`] rather than rounding.
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

    let mut exact: BTreeMap<Addr, Frac> = BTreeMap::new();
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
                for (wallet, w) in weights {
                    if w == 0 {
                        continue;
                    }
                    let term = Frac::new(
                        u128::from(budget)
                            .checked_mul(w)
                            .and_then(|v| v.checked_mul(dt))
                            .ok_or(EligibilityError::MathOverflow)?,
                        h.checked_mul(duration)
                            .ok_or(EligibilityError::MathOverflow)?,
                    );
                    let acc = exact.entry(wallet).or_insert(Frac::ZERO);
                    *acc = acc.add(term)?;
                }
                last_settle_observation = Some((cw, h, P_REF_LAMPORTS * cn_seeds));
            }
            IntervalVerdict::Reserve => reserve_claims.push((iv.start, iv.end)),
            IntervalVerdict::NotEarned => not_earned_claims.push((iv.start, iv.end)),
        }
    }

    // Leaves: floor of the exact cumulative, capped at the E/5 wallet
    // ceiling. The cap is defensive — `w ≤ Kmax` and `H ≥ 5·Kmax` already
    // bound `exact ≤ E/5` — but settlement never trusts its inputs' shape.
    let wallet_cap = budget / 5;
    let mut allocations: Vec<WalletSettlement> = Vec::with_capacity(exact.len());
    let mut allocated: u64 = 0;
    for (wallet, f) in &exact {
        let leaf = f.floor_u64()?.min(wallet_cap);
        allocated = allocated
            .checked_add(leaf)
            .ok_or(EligibilityError::MathOverflow)?;
        allocations.push(WalletSettlement {
            wallet: *wallet,
            leaf_lamports: leaf,
            exact: Ratio::new(f.num, f.den),
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
