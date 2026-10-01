//! Loyalty bonus (architecture §6; Pass #7 P7-C, frozen decision DQ-1).
//!
//! **Tenure (DQ-1).** Tenure counts whole completed 24-hour days on the grid
//! `[FRUIT_T0 + k·86,400, FRUIT_T0 + (k+1)·86,400)`. A day *qualifies* only
//! when the wallet is verified with recognized weight `w >= w_tenure_min`
//! for the entire day. Day outcomes (§6 cause table):
//!
//! | Day contains | Effect on `d` |
//! |---|---|
//! | a data gap (unreconciled segment, data-class PENDING, unknown seeds) | pending: this and every later day are unknown until the replay resolves it |
//! | a voluntary loss (verified `w < w_tenure_min`, leaving the Fruit selection, a penalized base claim) | reset to 0 |
//! | an involuntary loss (asset inactive / INELIGIBLE, adapter UNKNOWN, EXTREME NAV reset, route halt) | frozen: unchanged |
//! | none of the above (qualifying all day) | `d + 1` |
//!
//! `beta_i = min(1, d_i/11)` steps once per completed day, so during day `j`
//! it uses the days before `j`.
//!
//! **Bonus.** `v_i = w_i beta_i`, `H_loy = max(sum v_i, 12.5)`,
//! `dQ_i/dt = (B_loy/D) v_i/H_loy`. Settlement reuses the core kernel with
//! the integer weight `11 v_i = w_i min(d_i, 11)` and floor `11 * 12.5 SOL`,
//! which is exactly the same ratio; `sum Q <= B_loy` and `Q_i <= B_loy/5`
//! carry over (`11 v_i <= 11 Kmax = (11 Hfloor)/5`). An interval in which a
//! qualifying wallet's tenure is pending reserves as a whole, like core.
//!
//! **Fail closed.** While `w_tenure_min` is unset (or zero), loyalty is
//! [`LoyaltySettlement::Inactive`]: no leaves are produced and `B_loy` must
//! be 0 (§2, §6).
//!
//! Penalized base claims are read from base instruction / account data by
//! the indexer (§6) and passed in as timestamps per wallet.

use std::collections::BTreeMap;

use crate::error::{EligibilityError, EligibilityResult};
use crate::replay::{Eligibility, IntervalStatus, PendingReason, ReplayReport, Segment};
use crate::settlement::{
    derive_intervals, recognized_weight, settle_intervals_weighted, EpochSettlement,
    IntervalVerdict, SeedTimeline, DENOMINATOR_FLOOR,
};
use crate::types::Addr;

/// One tenure day, in seconds.
pub const TENURE_DAY_SECONDS: i64 = 86_400;
/// `d_cap = 11 days` (§6): `beta = min(1, d/11)`.
pub const TENURE_DAY_CAP: u64 = 11;

/// Loyalty configuration. `w_tenure_min` is a founder-set non-dust
/// threshold; `None` (or 0) means unset and loyalty is inactive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoyaltyConfig {
    pub fruit_t0: i64,
    pub w_tenure_min: Option<u128>,
}

impl LoyaltyConfig {
    fn threshold(&self) -> Option<u128> {
        self.w_tenure_min.filter(|&m| m > 0)
    }
}

/// How one day ended for one wallet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayOutcome {
    Qualified,
    Reset,
    Frozen,
    Pending,
}

/// Completed qualifying days `d` before each day of the schedule; `None`
/// once a data gap makes it unknown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TenureSchedule {
    /// First day start (on the grid).
    pub from: i64,
    /// Per wallet: `days[k]` is the outcome of day `from + k·86,400`.
    pub outcomes: BTreeMap<Addr, Vec<DayOutcome>>,
}

impl TenureSchedule {
    /// `d` in force at `t` (completed days strictly before `t`'s day), or
    /// `None` if unknown. A wallet with no outcomes has `d = 0`.
    pub fn days_at(&self, wallet: &Addr, t: i64) -> Option<u64> {
        let Some(days) = self.outcomes.get(wallet) else {
            return Some(0);
        };
        let k = usize::try_from((t - self.from).div_euclid(TENURE_DAY_SECONDS)).unwrap_or(0);
        let mut d = 0u64;
        for o in days.iter().take(k) {
            match o {
                DayOutcome::Qualified => d += 1,
                DayOutcome::Reset => d = 0,
                DayOutcome::Frozen => {}
                DayOutcome::Pending => return None,
            }
        }
        Some(d)
    }
}

/// Data-class PENDING (settlement pending, no reset) versus involuntary
/// loss of eligibility (freeze), per the §6 cause table.
fn pending_is_gap(r: &PendingReason) -> bool {
    match r {
        PendingReason::UnresolvedHistory
        | PendingReason::NotReconciled
        | PendingReason::Debt(_)
        | PendingReason::DebtUnpriced
        | PendingReason::AdditionUnpriced => true,
        PendingReason::AdapterProgramUnknown
        | PendingReason::StakeUnverified
        | PendingReason::RouteHalted
        | PendingReason::NavUnverified => false,
    }
}

fn check_grid(cfg: &LoyaltyConfig, t: i64) -> EligibilityResult<()> {
    if (t - cfg.fruit_t0).rem_euclid(TENURE_DAY_SECONDS) != 0 {
        return Err(EligibilityError::InvariantViolation(
            "tenure days are aligned to FRUIT_T0",
        ));
    }
    Ok(())
}

/// Kernel of [`tenure_schedule`]: outcomes of the days in `[from, to)` for
/// every wallet seen in the segments.
pub(crate) fn tenure_segments(
    segs: &[Segment],
    seeds: &SeedTimeline,
    cfg: &LoyaltyConfig,
    min: u128,
    from: i64,
    to: i64,
    penalized_claims: &BTreeMap<Addr, Vec<i64>>,
) -> EligibilityResult<TenureSchedule> {
    check_grid(cfg, from)?;
    check_grid(cfg, to)?;
    if to < from {
        return Err(EligibilityError::InvariantViolation("empty tenure window"));
    }
    let wallets: Vec<Addr> = {
        let mut w: Vec<Addr> = segs
            .iter()
            .flat_map(|s| s.wallets.keys().copied())
            .collect();
        w.sort_unstable();
        w.dedup();
        w
    };
    let mut outcomes: BTreeMap<Addr, Vec<DayOutcome>> = BTreeMap::new();
    let mut day = from;
    while day < to {
        let end = day + TENURE_DAY_SECONDS;
        // Pieces covering the day; a day the replay does not fully cover
        // is a data gap.
        let mut cursor = day;
        let mut pieces: Vec<&Segment> = Vec::new();
        for s in segs {
            let (a, b) = (s.start.max(day), s.end.min(end));
            if a >= b {
                continue;
            }
            if a != cursor {
                break;
            }
            pieces.push(s);
            cursor = b;
        }
        let covered = cursor == end;
        for w in &wallets {
            let claimed = penalized_claims
                .get(w)
                .is_some_and(|ts| ts.iter().any(|&t| day <= t && t < end));
            let (mut gap, mut voluntary, mut frozen) = (!covered, claimed, false);
            for s in &pieces {
                if s.status != IntervalStatus::Verified {
                    gap = true;
                    continue;
                }
                let (a, b) = (s.start.max(day), s.end.min(end));
                match s.wallets.get(w) {
                    None => voluntary = true,
                    Some(Eligibility::Verified { x_lamports, .. }) => {
                        match seeds.min_within(w, a, b) {
                            None => gap = true,
                            Some(n) => {
                                if recognized_weight(n, *x_lamports) < min {
                                    voluntary = true;
                                }
                            }
                        }
                    }
                    Some(Eligibility::Pending(r)) if pending_is_gap(r) => gap = true,
                    Some(Eligibility::Pending(_)) | Some(Eligibility::Ineligible(_)) => {
                        frozen = true
                    }
                }
            }
            let o = if gap {
                DayOutcome::Pending
            } else if voluntary {
                DayOutcome::Reset
            } else if frozen {
                DayOutcome::Frozen
            } else {
                DayOutcome::Qualified
            };
            outcomes.entry(*w).or_default().push(o);
        }
        day = end;
    }
    Ok(TenureSchedule { from, outcomes })
}

/// DQ-1 tenure outcomes for the days in `[from, to)` (both on the
/// `FRUIT_T0` grid). `None` while loyalty is inactive.
pub fn tenure_schedule(
    report: &ReplayReport,
    seeds: &SeedTimeline,
    cfg: &LoyaltyConfig,
    from: i64,
    to: i64,
    penalized_claims: &BTreeMap<Addr, Vec<i64>>,
) -> EligibilityResult<Option<TenureSchedule>> {
    match cfg.threshold() {
        None => Ok(None),
        Some(min) => tenure_segments(
            report.segments(),
            seeds,
            cfg,
            min,
            from,
            to,
            penalized_claims,
        )
        .map(Some),
    }
}

/// Loyalty settlement of one funded loyalty pool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoyaltySettlement {
    /// `w_tenure_min` is unset: loyalty does not activate; post nothing.
    Inactive,
    /// The pool's allocations, reserve and unallocated capacity, as for
    /// core Fruit. (APR fields are in the loyalty-scaled units `11 v`.)
    Settled(EpochSettlement),
}

/// Kernel of [`settle_loyalty_epoch`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn loyalty_segments(
    budget: u64,
    t_start: i64,
    t_end: i64,
    segs: &[Segment],
    seeds: &SeedTimeline,
    cfg: &LoyaltyConfig,
    tenure_from: i64,
    penalized_claims: &BTreeMap<Addr, Vec<i64>>,
) -> EligibilityResult<LoyaltySettlement> {
    let Some(min) = cfg.threshold() else {
        return Ok(LoyaltySettlement::Inactive);
    };
    if t_end <= t_start || tenure_from > t_start {
        return Err(EligibilityError::InvariantViolation(
            "invalid loyalty window",
        ));
    }
    // Tenure for every completed day up to the last day boundary <= t_end.
    let last_boundary = t_end - (t_end - cfg.fruit_t0).rem_euclid(TENURE_DAY_SECONDS);
    let tenure = tenure_segments(
        segs,
        seeds,
        cfg,
        min,
        tenure_from,
        last_boundary.max(tenure_from),
        penalized_claims,
    )?;
    // Day boundaries inside the epoch: beta changes there.
    let mut cuts = Vec::new();
    let mut b = t_start - (t_start - cfg.fruit_t0).rem_euclid(TENURE_DAY_SECONDS);
    while b < t_end {
        if b > t_start {
            cuts.push(b);
        }
        b += TENURE_DAY_SECONDS;
    }
    let mut intervals = derive_intervals(t_start, t_end, segs, seeds, &cuts)?;
    // multipliers[idx][wallet] = min(d, 11); a settling interval in which a
    // weighted wallet's tenure is unknown reserves as a whole.
    let mut multipliers: Vec<BTreeMap<Addr, u128>> = Vec::with_capacity(intervals.len());
    for iv in intervals.iter_mut() {
        let mut m = BTreeMap::new();
        if iv.verdict == IntervalVerdict::Settle {
            let mut unknown = false;
            for p in &iv.participants {
                if recognized_weight(p.seeds, p.x_lamports) == 0 {
                    continue;
                }
                match tenure.days_at(&p.wallet, iv.start) {
                    Some(d) => {
                        m.insert(p.wallet, u128::from(d.min(TENURE_DAY_CAP)));
                    }
                    None => unknown = true,
                }
            }
            if unknown {
                iv.verdict = IntervalVerdict::Reserve;
                iv.participants.clear();
                m.clear();
            }
        }
        multipliers.push(m);
    }
    let floor = DENOMINATOR_FLOOR * u128::from(TENURE_DAY_CAP);
    let settlement =
        settle_intervals_weighted(budget, t_start, t_end, &intervals, floor, &|idx, p| {
            let w = recognized_weight(p.seeds, p.x_lamports);
            let m = multipliers[idx].get(&p.wallet).copied().unwrap_or(0);
            w.checked_mul(m).ok_or(EligibilityError::MathOverflow)
        })?;
    Ok(LoyaltySettlement::Settled(settlement))
}

/// Settle one loyalty pool `[t_start, t_end)` with budget `B_loy` from the
/// Season replay (§6, DQ-1). `tenure_from` is the first tenure day (on the
/// `FRUIT_T0` grid, at or before `t_start`); the replay must cover
/// `[tenure_from, t_end)`.
#[allow(clippy::too_many_arguments)]
pub fn settle_loyalty_epoch(
    budget: u64,
    t_start: i64,
    t_end: i64,
    report: &ReplayReport,
    seeds: &SeedTimeline,
    cfg: &LoyaltyConfig,
    tenure_from: i64,
    penalized_claims: &BTreeMap<Addr, Vec<i64>>,
) -> EligibilityResult<LoyaltySettlement> {
    loyalty_segments(
        budget,
        t_start,
        t_end,
        report.segments(),
        seeds,
        cfg,
        tenure_from,
        penalized_claims,
    )
}
