//! Genesis yield: APR, phase, decay threshold and compounding projection,
//! derived from the same on-chain state the accumulator pays out of.
//!
//! Nothing in this module can make a lamport. It reads
//! [`RewardGlobals`] and pricing, and — for the projection — drives the real
//! [`compound`] transition on a *copy* of state. Every figure it produces is
//! therefore something the reward vault is already funded to pay.
//!
//! ## The model
//!
//! ```text
//! current_apr = base_reward_rate × phase_multiplier × utilization_multiplier
//!                                                     × compound_multiplier
//!
//! base_reward_rate       reward_rate × SECONDS_PER_YEAR / cost_per_rig
//!                        (funded lamports per second, set only by fund_rewards,
//!                         which refuses a schedule the vault cannot back)
//! phase_multiplier       the funded schedule itself. V1 applies no on-chain
//!                        multiplier: a front-loaded genesis is expressed by the
//!                        funder scheduling a higher rate early (fund_rewards
//!                        windows), never by scaling emission past what is funded.
//! utilization_multiplier weight / total_shares — every rig planted dilutes
//!                        every rig. A referred rig's weight is 1.05× a base
//!                        rig's; it takes a larger slice of the SAME emission.
//!                        This is the decay curve: APR falls hyperbolically as
//!                        participation grows, and the earliest planters hold
//!                        the largest share of emission while it is highest.
//! compound_multiplier    APY / APR, obtained by running `compound` forward
//!                        ([`project_compounding`]), never from a formula like
//!                        (1 + r/n)^n that ignores fees, whole-rig truncation,
//!                        self-dilution and the end of the funded window.
//! ```
//!
//! ## Phases
//!
//! A phase is a *label on the current APR*, not a state the program stores.
//! It is a pure function of global state (`reward_rate`, `total_rigs`,
//! pricing, `reward_period_finish`, the clock), so no per-user action — split
//! purchases, claim-and-rebuy, compounding — can reset it. Rigs are
//! non-redeemable, so between funding events `total_rigs` only grows and the
//! phase only moves forward. Only a new `fund_rewards` can raise it.
//!
//! Integer math only; `u128` intermediates; results saturate rather than wrap.

use crate::accumulator::{RewardGlobals, WEIGHT_BASE};
use crate::error::{EngineError, EngineResult};
use crate::position::{compound, MinerPosition};
use crate::pricing::BPS_DENOMINATOR;
use crate::RewardLedger;

/// 365 days. APR is annualised on this, not on a calendar year.
pub const SECONDS_PER_YEAR: u64 = 365 * 24 * 60 * 60;

/// Upper bound on projection steps. Bounds the work any caller — including a
/// browser rendering a panel — can be asked to do.
pub const MAX_PROJECTION_STEPS: u64 = 1_000;

/// Named yield phases, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum YieldPhase {
    /// APR ≥ 1,200%.
    Genesis,
    /// 600% ≤ APR < 1,200%.
    EarlyGrove,
    /// 250% ≤ APR < 600%.
    Growth,
    /// 0% < APR < 250%.
    Mature,
    /// No funded emission right now: the window has ended or never started.
    Dormant,
}

impl YieldPhase {
    /// Lowest APR, in basis points, that still counts as this phase.
    pub const fn floor_bps(self) -> u64 {
        match self {
            YieldPhase::Genesis => 120_000,
            YieldPhase::EarlyGrove => 60_000,
            YieldPhase::Growth => 25_000,
            YieldPhase::Mature => 1,
            YieldPhase::Dormant => 0,
        }
    }

    /// The phase this one decays into.
    pub const fn next(self) -> YieldPhase {
        match self {
            YieldPhase::Genesis => YieldPhase::EarlyGrove,
            YieldPhase::EarlyGrove => YieldPhase::Growth,
            YieldPhase::Growth => YieldPhase::Mature,
            YieldPhase::Mature | YieldPhase::Dormant => YieldPhase::Dormant,
        }
    }

    /// Stable code shared with the TypeScript port and the test vectors.
    pub const fn code(self) -> u8 {
        match self {
            YieldPhase::Genesis => 0,
            YieldPhase::EarlyGrove => 1,
            YieldPhase::Growth => 2,
            YieldPhase::Mature => 3,
            YieldPhase::Dormant => 4,
        }
    }

    pub fn from_apr_bps(apr_bps: u64) -> YieldPhase {
        [
            YieldPhase::Genesis,
            YieldPhase::EarlyGrove,
            YieldPhase::Growth,
            YieldPhase::Mature,
        ]
        .into_iter()
        .find(|p| apr_bps >= p.floor_bps())
        .unwrap_or(YieldPhase::Dormant)
    }
}

/// The global state APR is computed from. Every field is on-chain state or
/// the chain clock — nothing a user can set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YieldInputs {
    pub reward_rate: u64,
    pub reward_period_finish: i64,
    /// `Σ rigs × weight`. A base rig is [`WEIGHT_BASE`] shares.
    pub total_shares: u64,
    pub rig_price_lamports: u64,
    pub purchase_fee_bps: u16,
    pub now: i64,
}

impl YieldInputs {
    pub fn from_globals(
        globals: &RewardGlobals,
        rig_price_lamports: u64,
        purchase_fee_bps: u16,
        now: i64,
    ) -> Self {
        YieldInputs {
            reward_rate: globals.reward_rate,
            reward_period_finish: globals.reward_period_finish,
            total_shares: globals.total_shares,
            rig_price_lamports,
            purchase_fee_bps,
            now,
        }
    }

    fn emitting(&self) -> bool {
        self.reward_rate > 0 && self.now < self.reward_period_finish
    }

    /// `reward_rate × SECONDS_PER_YEAR × 10_000 × 10_000`: annual emission,
    /// scaled for a bps result and for a fee-inclusive cost denominator.
    fn numerator(&self) -> u128 {
        // ≤ 1e12 × 3.2e7 × 1e8 ≈ 3.2e27 — far inside u128.
        self.reward_rate as u128
            * SECONDS_PER_YEAR as u128
            * BPS_DENOMINATOR as u128
            * BPS_DENOMINATOR as u128
    }

    /// `rig_price × (10_000 + fee_bps)`: the fee-inclusive cost of one rig,
    /// scaled by 10_000. APR is quoted on what a planter actually pays.
    fn cost_per_rig_scaled(&self) -> EngineResult<u128> {
        if self.rig_price_lamports == 0 {
            return Err(EngineError::InvalidConfig);
        }
        Ok(self.rig_price_lamports as u128
            * (BPS_DENOMINATOR as u128 + self.purchase_fee_bps as u128))
    }
}

/// APR in basis points on the fee-inclusive price of a **base-weight** rig,
/// if `extra_rigs` more base rigs were planted.
///
/// `extra_rigs = 0` is the rate a base rig earns now. `extra_rigs = n` is the
/// rate *after* you plant `n` — including the dilution your own rigs cause.
/// With nothing planted, the quote is the first rig's.
///
/// Zero whenever there is no funded emission. Floors; saturates at `u64::MAX`.
pub fn apr_bps(inputs: &YieldInputs, extra_rigs: u64) -> EngineResult<u64> {
    apr_bps_weighted(inputs, extra_rigs, WEIGHT_BASE)
}

/// APR for one rig of `weight` (100 = base, 105 = referred), after
/// `extra_rigs` more rigs of that same weight are planted.
///
/// `numerator × weight / ((total_shares + extra_rigs × weight) × cost)` —
/// a referred rig reads exactly 1.05× the base rate, because it takes 1.05×
/// the share of the same emission.
pub fn apr_bps_weighted(inputs: &YieldInputs, extra_rigs: u64, weight: u64) -> EngineResult<u64> {
    let cost = inputs.cost_per_rig_scaled()?;
    if weight == 0 {
        return Err(EngineError::InvalidConfig);
    }
    if !inputs.emitting() {
        return Ok(0);
    }
    let shares = (inputs.total_shares as u128)
        .saturating_add((extra_rigs as u128).saturating_mul(weight as u128));
    let shares = core::cmp::max(shares, weight as u128);
    // ≤ 1.05e14 shares × 1.8e19 × 11_000 ≈ 2e37 — inside u128.
    let denom = shares.checked_mul(cost).ok_or(EngineError::MathOverflow)?;
    let num = inputs
        .numerator()
        .checked_mul(weight as u128)
        .ok_or(EngineError::MathOverflow)?;
    Ok(u64::try_from(num / denom).unwrap_or(u64::MAX))
}

/// Phase of the current base-rig APR.
pub fn current_phase(inputs: &YieldInputs) -> EngineResult<YieldPhase> {
    Ok(YieldPhase::from_apr_bps(apr_bps(inputs, 0)?))
}

/// Where the current phase ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextThreshold {
    /// The base APR drops into `phase` once global shares reach
    /// `at_total_shares` — the smallest share count whose base APR is below
    /// the current phase's floor. A base rig is [`WEIGHT_BASE`] shares.
    Shares {
        phase: YieldPhase,
        at_total_shares: u64,
    },
    /// Mature: emission (and so the phase) ends at the funded window's close.
    WindowEnds { at: i64 },
    /// Dormant: nothing further until new funding arrives.
    None,
}

/// The next decay threshold, exact to the share.
pub fn next_threshold(inputs: &YieldInputs) -> EngineResult<NextThreshold> {
    let phase = current_phase(inputs)?;
    match phase {
        YieldPhase::Dormant => Ok(NextThreshold::None),
        YieldPhase::Mature => Ok(NextThreshold::WindowEnds {
            at: inputs.reward_period_finish,
        }),
        _ => {
            // Smallest S with numerator × 100 / (S × cost) < floor, i.e.
            // S = floor(numerator × 100 / (floor × cost)) + 1.
            let cost = inputs.cost_per_rig_scaled()?;
            let per = (phase.floor_bps() as u128)
                .checked_mul(cost)
                .ok_or(EngineError::MathOverflow)?;
            let t = (inputs.numerator() * WEIGHT_BASE as u128 / per).saturating_add(1);
            Ok(NextThreshold::Shares {
                phase: phase.next(),
                at_total_shares: u64::try_from(t).unwrap_or(u64::MAX),
            })
        }
    }
}

/// Result of running a position forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Projection {
    /// Seconds actually projected: `min(horizon, funded runway)`.
    pub horizon_seconds: u64,
    pub start_rigs: u64,
    pub end_rigs: u64,
    /// Every lamport of reward the position received over the horizon —
    /// compounded into rigs or left claimable.
    pub reward_lamports: u64,
    /// Fee-inclusive replacement cost of `start_rigs`.
    pub cost_basis_lamports: u64,
    /// `reward / cost_basis`, in bps, over `horizon_seconds`. Not annualised
    /// beyond the funded window: a yield the vault is not funded to pay is
    /// never shown.
    pub yield_bps: u64,
}

/// Run `position` forward from `now` by repeatedly applying the real
/// [`compound`] transition every `interval_seconds` (`0` = never compound),
/// for `min(horizon_seconds, funded runway)`.
///
/// Every other planter is held where they are. That is the one assumption,
/// and it is the generous direction only for the planter's *share*; the
/// emission itself can never exceed what is funded, because it is the
/// accumulator that pays it.
pub fn project_compounding(
    globals: &RewardGlobals,
    position: &MinerPosition,
    now: i64,
    rig_price_lamports: u64,
    purchase_fee_bps: u16,
    interval_seconds: u64,
    horizon_seconds: u64,
) -> EngineResult<Projection> {
    if position.rigs == 0 {
        return Err(EngineError::InvalidRigCount);
    }
    let runway = if now < globals.reward_period_finish {
        (globals.reward_period_finish - now) as u64
    } else {
        0
    };
    let horizon = core::cmp::min(horizon_seconds, runway);
    if interval_seconds > 0 && horizon / interval_seconds > MAX_PROJECTION_STEPS {
        return Err(EngineError::InvalidDuration);
    }

    let mut g = *globals;
    let mut p = *position;
    let mut ledger = RewardLedger::default();
    p.settle(&mut g, now)?;
    let start_pending = p.pending_rewards;
    let start_rigs = p.rigs;

    let end = now
        .checked_add(i64::try_from(horizon).map_err(|_| EngineError::MathOverflow)?)
        .ok_or(EngineError::MathOverflow)?;
    let mut compounded: u64 = 0;
    if interval_seconds > 0 {
        let step = i64::try_from(interval_seconds).map_err(|_| EngineError::MathOverflow)?;
        let mut t = now;
        while t.saturating_add(step) <= end {
            t += step;
            match compound(
                &mut g,
                &mut p,
                &mut ledger,
                t,
                rig_price_lamports,
                purchase_fee_bps,
            ) {
                Ok(out) => {
                    compounded = compounded
                        .checked_add(out.total_lamports)
                        .ok_or(EngineError::MathOverflow)?
                }
                // Nothing affordable yet, or the wallet is at the 250 cap: no
                // seed this step; rewards keep accruing as claimable.
                Err(EngineError::NothingToCompound | EngineError::WalletCapExceeded) => {}
                Err(e) => return Err(e),
            }
        }
    }
    p.settle(&mut g, end)?;

    let reward = (p.pending_rewards as u128 + compounded as u128)
        .checked_sub(start_pending as u128)
        .ok_or(EngineError::MathOverflow)?;
    let cost = (start_rigs as u128)
        * rig_price_lamports as u128
        * (BPS_DENOMINATOR as u128 + purchase_fee_bps as u128)
        / BPS_DENOMINATOR as u128;
    let yield_bps = (reward * BPS_DENOMINATOR as u128)
        .checked_div(cost)
        .unwrap_or(0);

    Ok(Projection {
        horizon_seconds: horizon,
        start_rigs,
        end_rigs: p.rigs,
        reward_lamports: u64::try_from(reward).map_err(|_| EngineError::MathOverflow)?,
        cost_basis_lamports: u64::try_from(cost).unwrap_or(u64::MAX),
        yield_bps: u64::try_from(yield_bps).unwrap_or(u64::MAX),
    })
}

/// A planter's realised rate: every reward lamport they have received or are
/// owed, over the SOL they paid in, annualised over the life of the position.
///
/// `None` for the first hour, where annualising a few seconds of accrual
/// would print a meaningless number.
pub fn effective_rate_bps(
    position: &MinerPosition,
    claimable_lamports: u64,
    created_at: i64,
    now: i64,
) -> Option<u64> {
    const MIN_AGE_SECONDS: i64 = 3_600;
    let age = now.checked_sub(created_at)?;
    if age < MIN_AGE_SECONDS || position.lifetime_sol_spent == 0 {
        return None;
    }
    let received = position.lifetime_sol_claimed as u128
        + position.lifetime_sol_compounded as u128
        + claimable_lamports as u128;
    let v = received * SECONDS_PER_YEAR as u128 * BPS_DENOMINATOR as u128
        / (position.lifetime_sol_spent as u128 * age as u128);
    Some(u64::try_from(v).unwrap_or(u64::MAX))
}

/// Annual emission, in lamports, needed to hold `apr_bps` over
/// `capital_lamports` of planted (fee-inclusive) rig cost. The inverse of
/// [`apr_bps`], used to state what a target phase costs the reward pool.
pub fn required_annual_emission(apr_bps: u64, capital_lamports: u64) -> u128 {
    apr_bps as u128 * capital_lamports as u128 / BPS_DENOMINATOR as u128
}
