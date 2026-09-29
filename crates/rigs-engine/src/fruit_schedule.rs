//! Fruit schedule planning — architecture §3 (launch timeline), canonical
//! tests 40–43.
//!
//! The planner is a pure function of the activation timestamp `T0` and the
//! cleared core funding `F`. It fixes, before anything runs, which epochs
//! exist, when each starts and ends, and what each one's budget is:
//!
//! ```text
//! FRUIT_T0 = T0 + 86,400            (public opening; Day-1 ends there)
//! Storm       [FRUIT_T0 +      0, FRUIT_T0 +  86,400)   1 day    E_storm
//! Harvest     [FRUIT_T0 + 86,400, FRUIT_T0 + 259,200)   2 days     684,931,507
//! Superfruit  [FRUIT_T0 +259,200, FRUIT_T0 + 518,400)   3 days     513,698,631
//! Season      [FRUIT_T0 +518,400, FRUIT_T0 + 950,400)   5 days     428,082,192
//! ```
//!
//! Funding boundary states (§3) — these are states, not menus:
//!
//! * `F = 0`: Fruit stays inactive; no epoch, no pool, no accrual.
//! * `0 < F < R`: the sole deterministic fallback — ONE 48-hour Harvest
//!   beginning at FRUIT_T0 with budget `F`; no four-stage sequence, no
//!   Season and no lock.
//! * `F >= R`: `E_storm = min(F - R, 8.561643836)` and
//!   `Z_future = F - R - E_storm`. At `F = R` Storm emits zero — the
//!   zero-budget Storm pool is omitted entirely (a pool with `E = 0` is
//!   invalid) while the continuation keeps its published offsets.
//!
//! Every interval is half-open `[t_start, t_end)`; a boundary instant
//! belongs to the LATER interval (§3 time convention, test 42).
//!
//! Scope note (Pass #6, P6-D2): test 41 — the Lock registration window
//! `[FRUIT_T0, t_s)` and its half-open cutoff — belongs to the later Lock
//! package and is deliberately NOT implemented here. The Season start
//! offset itself is part of the published epoch table above and is
//! asserted by test 42.

use crate::error::{EngineError, EngineResult};

/// One day in seconds (§1: "One day is 86,400 seconds"). Numerically equal
/// to the launch access window — distinct constants, one meaning each.
pub const DAY_SECONDS: i64 = 86_400;

/// Harvest epoch budget (§2 table): `ceil(10 × 12.5 × 2/365 SOL)`.
pub const HARVEST_BUDGET_LAMPORTS: u64 = 684_931_507;
/// Superfruit epoch budget (§2 table): `ceil(5 × 12.5 × 3/365 SOL)`.
pub const SUPERFRUIT_BUDGET_LAMPORTS: u64 = 513_698_631;
/// Season epoch budget (§2 table): `ceil(2.5 × 12.5 × 5/365 SOL)`.
pub const SEASON_BUDGET_LAMPORTS: u64 = 428_082_192;
/// Continuation requirement `R` (§2 table): the sum of the three
/// continuation budgets above.
pub const CONTINUATION_BUDGET_R: u64 =
    HARVEST_BUDGET_LAMPORTS + SUPERFRUIT_BUDGET_LAMPORTS + SEASON_BUDGET_LAMPORTS;
/// Storm ceiling (§2 table): `ceil(250 × 12.5/365 SOL)`.
pub const STORM_CEILING_LAMPORTS: u64 = 8_561_643_836;

/// Core start offsets from FRUIT_T0 (§12 table), in chain order:
/// Storm +0, Harvest +1 day, Superfruit +3 days, Season +6 days.
pub const CORE_START_OFFSETS: [i64; 4] = [0, DAY_SECONDS, 3 * DAY_SECONDS, 6 * DAY_SECONDS];
/// Core end: `FRUIT_T0 + 950,400` (§12 table) = the Season's end.
pub const CORE_END_OFFSET: i64 = 11 * DAY_SECONDS;

/// Public opening and the start of every Fruit schedule (§3):
/// `FRUIT_T0 = T0 + 86,400 seconds`.
pub const fn fruit_t0(launch_t0: i64) -> i64 {
    launch_t0 + DAY_SECONDS
}

/// The four core epoch kinds, in chain order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EpochKind {
    Storm,
    Harvest,
    Superfruit,
    Season,
}

/// One planned epoch: a half-open window and its committed budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FruitEpoch {
    pub kind: EpochKind,
    pub t_start: i64,
    pub t_end: i64,
    /// `E`: the committed budget. Always > 0 — the planner never emits an
    /// invalid zero-budget pool (a zero-budget Storm at `F = R` is omitted,
    /// not emitted as an empty pool).
    pub budget: u64,
}

impl FruitEpoch {
    /// Half-open membership `[t_start, t_end)`: the start instant belongs
    /// to this epoch, the end instant to whatever follows (§3, test 42).
    pub const fn contains(&self, t: i64) -> bool {
        t >= self.t_start && t < self.t_end
    }

    pub const fn duration_seconds(&self) -> i64 {
        self.t_end - self.t_start
    }
}

/// Which funding boundary state a plan came from (§3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanKind {
    /// `F = 0`: Fruit inactive.
    Inactive,
    /// `0 < F < R`: the single 48-hour fallback Harvest.
    FallbackHarvest,
    /// `F >= R`: the full published sequence (minus a zero-budget Storm).
    CoreSequence,
}

/// A fully planned Fruit schedule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FruitPlan {
    pub launch_t0: i64,
    pub fruit_t0: i64,
    pub kind: PlanKind,
    /// The non-zero-budget epochs, in chain order. Empty when inactive.
    pub epochs: Vec<FruitEpoch>,
    /// `Z_future = F - R - E_storm` (only non-zero for `F > R`): funding
    /// beyond the Storm ceiling that stays in the uncommitted reserve `Z`.
    pub z_future: u64,
}

impl FruitPlan {
    /// Total committed budget across all epochs.
    pub fn total_committed(&self) -> u64 {
        self.epochs.iter().map(|e| e.budget).sum()
    }

    /// The epoch that owns `t` under half-open semantics, if any. The
    /// instant `t = t_start` of an epoch belongs to THAT epoch; the instant
    /// `t = t_end` belongs to the next interval (or to nothing once the
    /// sequence is over). A gap epoch (Storm omitted at `F = R`) owns no
    /// instant: nothing accrues there.
    pub fn epoch_at(&self, t: i64) -> Option<&FruitEpoch> {
        self.epochs.iter().find(|e| e.contains(t))
    }

    /// Whether Fruit accrual is live at `t` (any epoch contains `t`).
    pub fn accrual_active(&self, t: i64) -> bool {
        self.epoch_at(t).is_some()
    }
}

/// Plan the Fruit schedule from `launch_t0` (`T0`) and the cleared core
/// funding `F` (§3). Pure; validates nothing about `T0` beyond arithmetic
/// (activation itself is the launch module's concern).
pub fn plan_fruit(launch_t0: i64, funding_lamports: u64) -> FruitPlan {
    let ft0 = fruit_t0(launch_t0);
    let mut plan = FruitPlan {
        launch_t0,
        fruit_t0: ft0,
        kind: PlanKind::Inactive,
        epochs: Vec::new(),
        z_future: 0,
    };
    if funding_lamports == 0 {
        return plan; // F = 0: inactive, nothing planned, nothing committed
    }
    if funding_lamports < CONTINUATION_BUDGET_R {
        // 0 < F < R: the sole deterministic fallback — one 48-hour Harvest
        // at FRUIT_T0 with budget F. No four-stage sequence, no lock Season.
        plan.kind = PlanKind::FallbackHarvest;
        plan.epochs.push(FruitEpoch {
            kind: EpochKind::Harvest,
            t_start: ft0,
            t_end: ft0 + 2 * DAY_SECONDS,
            budget: funding_lamports,
        });
        return plan;
    }
    // F >= R
    plan.kind = PlanKind::CoreSequence;
    let storm_budget = (funding_lamports - CONTINUATION_BUDGET_R).min(STORM_CEILING_LAMPORTS);
    plan.z_future = funding_lamports - CONTINUATION_BUDGET_R - storm_budget;
    if storm_budget > 0 {
        plan.epochs.push(FruitEpoch {
            kind: EpochKind::Storm,
            t_start: ft0,
            t_end: ft0 + DAY_SECONDS,
            budget: storm_budget,
        });
    }
    // At F = R Storm emits zero: the (invalid) zero-budget pool is simply
    // not planned, and the continuation keeps its published offsets.
    plan.epochs.push(FruitEpoch {
        kind: EpochKind::Harvest,
        t_start: ft0 + DAY_SECONDS,
        t_end: ft0 + 3 * DAY_SECONDS,
        budget: HARVEST_BUDGET_LAMPORTS,
    });
    plan.epochs.push(FruitEpoch {
        kind: EpochKind::Superfruit,
        t_start: ft0 + 3 * DAY_SECONDS,
        t_end: ft0 + 6 * DAY_SECONDS,
        budget: SUPERFRUIT_BUDGET_LAMPORTS,
    });
    plan.epochs.push(FruitEpoch {
        kind: EpochKind::Season,
        t_start: ft0 + 6 * DAY_SECONDS,
        t_end: ft0 + CORE_END_OFFSET,
        budget: SEASON_BUDGET_LAMPORTS,
    });
    plan
}

/// Validate that a plan is internally coherent: contiguous epoch windows in
/// chain order (allowing the `F = R` gap where Storm was omitted), positive
/// budgets, and the published core end. Exercised by the planner's own
/// tests; exported so the indexer can sanity-check a hand-assembled plan.
pub fn validate_plan(plan: &FruitPlan) -> EngineResult<()> {
    if plan.epochs.iter().any(|e| e.budget == 0) {
        return Err(EngineError::InvalidFundingAmount);
    }
    for w in plan.epochs.windows(2) {
        if w[0].t_end > w[1].t_start {
            return Err(EngineError::InvalidPoolWindow);
        }
    }
    if let Some(last) = plan.epochs.last() {
        if last.t_end > plan.fruit_t0 + CORE_END_OFFSET {
            return Err(EngineError::InvalidPoolWindow);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_700_000_000;

    #[test]
    fn funding_branches_are_the_published_states() {
        // F = 0: inactive, nothing committed.
        let p = plan_fruit(T0, 0);
        assert_eq!(p.kind, PlanKind::Inactive);
        assert!(p.epochs.is_empty());
        assert_eq!(p.total_committed(), 0);
        assert_eq!(p.z_future, 0);
        assert!(!p.accrual_active(fruit_t0(T0)));

        // 0 < F < R, including the canonical F = R - 1 lamport: one
        // 48-hour fallback Harvest of exactly F at FRUIT_T0.
        for f in [1, 123, CONTINUATION_BUDGET_R - 1] {
            let p = plan_fruit(T0, f);
            assert_eq!(p.kind, PlanKind::FallbackHarvest, "F = {f}");
            assert_eq!(p.epochs.len(), 1, "F = {f}");
            let e = p.epochs[0];
            assert_eq!(e.kind, EpochKind::Harvest);
            assert_eq!((e.t_start, e.t_end), (fruit_t0(T0), fruit_t0(T0) + 172_800));
            assert_eq!(e.budget, f);
            assert_eq!(p.total_committed(), f);
            assert_eq!(p.z_future, 0);
            assert!(!p.epochs.iter().any(|e| e.kind == EpochKind::Season));
        }

        // F = R: Storm emits zero — omitted, not zero-budget — and the
        // continuation keeps its published offsets.
        let p = plan_fruit(T0, CONTINUATION_BUDGET_R);
        assert_eq!(p.kind, PlanKind::CoreSequence);
        assert_eq!(p.total_committed(), CONTINUATION_BUDGET_R);
        assert_eq!(p.z_future, 0);
        assert!(!p.epochs.iter().any(|e| e.kind == EpochKind::Storm));
        assert_eq!(p.epochs.len(), 3);
        assert_eq!(p.epochs[0].t_start, fruit_t0(T0) + DAY_SECONDS);
        assert_eq!(
            p.epochs.last().unwrap().t_end,
            fruit_t0(T0) + CORE_END_OFFSET
        );

        // F > R below the ceiling: E_storm = F - R, nothing beyond.
        let f = CONTINUATION_BUDGET_R + 5_000_000_000;
        let p = plan_fruit(T0, f);
        assert_eq!(p.kind, PlanKind::CoreSequence);
        assert_eq!(p.epochs[0].kind, EpochKind::Storm);
        assert_eq!(p.epochs[0].budget, 5_000_000_000);
        assert_eq!(p.epochs[0].t_start, fruit_t0(T0));
        assert_eq!(p.z_future, 0);
        assert_eq!(p.total_committed() + p.z_future, f);

        // F > R above the ceiling: E_storm capped, excess to Z_future.
        let f = CONTINUATION_BUDGET_R + STORM_CEILING_LAMPORTS + 7;
        let p = plan_fruit(T0, f);
        assert_eq!(p.epochs[0].budget, STORM_CEILING_LAMPORTS);
        assert_eq!(p.z_future, 7);
        assert_eq!(p.total_committed() + p.z_future, f);
        assert_eq!(p.epochs.len(), 4);
    }

    #[test]
    fn planner_never_emits_a_zero_budget_pool() {
        for f in [
            0,
            1,
            CONTINUATION_BUDGET_R,
            CONTINUATION_BUDGET_R + 1,
            u32::MAX as u64,
        ] {
            let p = plan_fruit(T0, f);
            assert!(
                p.epochs.iter().all(|e| e.budget > 0),
                "zero-budget epoch at F = {f}"
            );
            validate_plan(&p).unwrap();
        }
    }

    #[test]
    fn epoch_windows_are_half_open_with_later_interval_owning_boundaries() {
        let p = plan_fruit(T0, CONTINUATION_BUDGET_R + 1);
        // The four canonical boundaries from FRUIT_T0 (test 42).
        for (offset, before, after) in [
            (DAY_SECONDS, EpochKind::Storm, Some(EpochKind::Harvest)),
            (
                3 * DAY_SECONDS,
                EpochKind::Harvest,
                Some(EpochKind::Superfruit),
            ),
            (
                6 * DAY_SECONDS,
                EpochKind::Superfruit,
                Some(EpochKind::Season),
            ),
            (CORE_END_OFFSET, EpochKind::Season, None),
        ] {
            let ft0 = fruit_t0(T0);
            assert_eq!(
                p.epoch_at(ft0 + offset - 1).map(|e| e.kind),
                Some(before),
                "just before +{offset}"
            );
            assert_eq!(
                p.epoch_at(ft0 + offset).map(|e| e.kind),
                after,
                "at +{offset}: the boundary instant belongs to the LATER interval"
            );
        }
        for e in &p.epochs {
            assert!(e.contains(e.t_start));
            assert!(!e.contains(e.t_end));
        }
        // Durations 1/2/3/5 days (§2).
        let d: Vec<i64> = p.epochs.iter().map(|e| e.duration_seconds()).collect();
        assert_eq!(d, vec![86_400, 172_800, 259_200, 432_000]);
    }
}
