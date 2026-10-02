//! Synthetix-style global reward-per-rig accumulator, fully funded,
//! integer math only.

use crate::error::{EngineError, EngineResult};
use crate::pricing::MAX_TOTAL_RIGS;

/// Fixed-point scale for [`RewardGlobals::reward_per_rig_stored`].
///
/// Chosen as 1e12, not 1e18. Headroom check for the hot expression
/// `elapsed * reward_rate * PRECISION`, evaluated in u128 (max ~3.4e38):
///
/// | term          | pessimistic bound          |
/// |---------------|----------------------------|
/// | `elapsed`     | 1e9 s  (~31 years)         |
/// | `reward_rate` | 1e12 lamports/s (1k SOL/s) |
/// | `PRECISION`   | 1e12                       |
///
/// Product ≈ 1e33, leaving ~5 orders of magnitude of headroom. At 1e18 the
/// same bound is ~1e39 and overflows, which is why the spec's fallback scale
/// is the one used here.
pub const PRECISION: u128 = 1_000_000_000_000;

/// Upper bound on a funding window: 10 years. Bounds `elapsed`.
pub const MAX_REWARD_DURATION_SECONDS: u64 = 10 * 365 * 24 * 60 * 60;

/// Upper bound on the emission rate: 1000 SOL/second. Bounds `reward_rate`.
pub const MAX_REWARD_RATE_LAMPORTS_PER_SECOND: u64 = 1_000_000_000_000;

/// What happens to emission that elapses while `total_rigs == 0`.
///
/// **Decision (spec §4): the elapsed slice is credited to dust.**
///
/// The schedule does not stall and `reward_period_finish` is never pushed
/// outward. Emission that elapses with no rigs in existence is added to
/// [`RewardGlobals::undistributed_dust`]: those lamports stay in the reward
/// vault, are attributed to nobody, and are unreachable in V1 because V1 ships
/// no recovery instruction. The vault is therefore over-collateralised rather
/// than under-collateralised, which is the direction that is always safe.
///
/// The rejected alternative — stalling by extending `period_finish` — mutates
/// the advertised funded-until date under users and makes the runway shown in
/// the transparency panel a moving target.
pub const ZERO_RIG_POLICY: &str = "credited-to-dust";

/// Reward weight of a rig with no referral, per rig. Shares are
/// `rigs × weight`, and emission is split by shares.
pub const WEIGHT_BASE: u64 = 100;

/// Reward weight of a rig owned by a validly referred planter: 1.05×.
///
/// This redistributes the same funded emission — a referred rig takes a
/// 5% larger slice of it at everyone else's expense. It never adds emission,
/// so `paid + owed ≤ funded` is untouched.
pub const WEIGHT_REFERRED: u64 = 105;

/// Global reward state. Lives inside the Config PDA on-chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RewardGlobals {
    pub total_rigs: u64,
    /// `Σ rigs × weight` over every position. Emission is divided by this.
    pub total_shares: u64,
    /// Lamports per second for the current funded window.
    pub reward_rate: u64,
    /// Unix timestamp at which the current funded window ends.
    pub reward_period_finish: i64,
    /// Unix timestamp of the last accumulator sync.
    pub reward_last_update: i64,
    /// Cumulative lamports-per-share, scaled by [`PRECISION`]. (The name is
    /// kept from V0, when one rig was one share; the on-chain field is too.)
    pub reward_per_rig_stored: u128,
    /// Emission that elapsed while `total_rigs == 0`. Unattributable, stays in
    /// the vault, never claimable in V1. See [`ZERO_RIG_POLICY`].
    pub undistributed_dust: u64,
}

/// Cumulative reward-vault bookkeeping. Lives inside the Config PDA.
///
/// The fully-funded invariant is checkable in O(1) from these counters:
/// every lamport that entered the reward vault did so through `fund_rewards`,
/// and every lamport that left did so through `claim` (to the claimant, and
/// the early-claim fee's treasury share to FEE) or `compound`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RewardLedger {
    pub total_reward_funded: u64,
    pub total_reward_claimed: u64,
    pub total_reward_compounded: u64,
    /// Early-claim fee treasury shares moved REWARD → FEE (architecture §7a
    /// item 5). Each settles the claim liability it was taken from.
    pub total_reward_to_fees: u64,
}

impl RewardLedger {
    /// Lamports the reward vault still owes: everything funded that has not yet
    /// left through a claim, an early-claim fee treasury share or a compound.
    ///
    /// This is an upper bound on user-claimable value, because it also covers
    /// not-yet-accrued future emission, per-second flooring residue and
    /// `undistributed_dust`. Over-stating liabilities is the safe direction.
    pub fn outstanding(&self) -> EngineResult<u64> {
        self.total_reward_funded
            .checked_sub(self.total_reward_claimed)
            .and_then(|v| v.checked_sub(self.total_reward_compounded))
            .and_then(|v| v.checked_sub(self.total_reward_to_fees))
            .ok_or(EngineError::MathOverflow)
    }
}

/// The fully-funded invariant, asserted at the end of every instruction that
/// touches reward state.
///
/// `distributable` is the reward vault's lamport balance minus its
/// rent-exempt reserve.
pub fn assert_fully_funded(distributable: u64, ledger: &RewardLedger) -> EngineResult<()> {
    if distributable < ledger.outstanding()? {
        return Err(EngineError::InsufficientRewardBacking);
    }
    Ok(())
}

impl RewardGlobals {
    /// Advance the accumulator to `now`.
    ///
    /// ```text
    /// applicable = min(now, reward_period_finish)
    /// elapsed    = applicable - reward_last_update
    /// if total_rigs > 0: reward_per_rig_stored += elapsed * rate * PRECISION / total_rigs
    /// else:              undistributed_dust    += elapsed * rate
    /// reward_last_update = applicable
    /// ```
    ///
    /// Integer division truncates, so the sub-lamport remainder stays in the
    /// vault. Rounding always favours the vault, never the user.
    pub fn sync(&mut self, now: i64) -> EngineResult<()> {
        let applicable = core::cmp::min(now, self.reward_period_finish);

        // Past `period_finish`, `reward_last_update` is already pinned to the
        // finish time and nothing further accrues. Never move it backwards.
        if applicable <= self.reward_last_update {
            return Ok(());
        }

        let elapsed = u64::try_from(applicable - self.reward_last_update)
            .map_err(|_| EngineError::MathOverflow)?;

        let emission = (elapsed as u128)
            .checked_mul(self.reward_rate as u128)
            .ok_or(EngineError::MathOverflow)?;

        if self.total_shares > 0 {
            let scaled = emission
                .checked_mul(PRECISION)
                .ok_or(EngineError::MathOverflow)?;
            let per_rig = scaled / self.total_shares as u128;
            self.reward_per_rig_stored = self
                .reward_per_rig_stored
                .checked_add(per_rig)
                .ok_or(EngineError::MathOverflow)?;
        } else {
            let emission = u64::try_from(emission).map_err(|_| EngineError::MathOverflow)?;
            self.undistributed_dust = self
                .undistributed_dust
                .checked_add(emission)
                .ok_or(EngineError::MathOverflow)?;
        }

        self.reward_last_update = applicable;
        Ok(())
    }

    /// `fund_rewards(amount, duration)` — schedule side only.
    ///
    /// The caller MUST have already moved `amount` lamports into the reward
    /// vault, and MUST call [`assert_fully_funded`] afterwards. The schedule is
    /// never written in the hope that SOL arrives later.
    pub fn fund(
        &mut self,
        ledger: &mut RewardLedger,
        now: i64,
        amount: u64,
        duration: u64,
    ) -> EngineResult<()> {
        if amount == 0 {
            return Err(EngineError::InvalidFundingAmount);
        }
        if duration == 0 || duration > MAX_REWARD_DURATION_SECONDS {
            return Err(EngineError::InvalidDuration);
        }

        self.sync(now)?;

        let leftover: u128 = if now < self.reward_period_finish {
            let remaining = (self.reward_period_finish - now) as u128;
            remaining
                .checked_mul(self.reward_rate as u128)
                .ok_or(EngineError::MathOverflow)?
        } else {
            0
        };

        let total_funding = leftover
            .checked_add(amount as u128)
            .ok_or(EngineError::MathOverflow)?;

        let rate = total_funding / duration as u128;
        if rate == 0 {
            return Err(EngineError::RewardRateZero);
        }
        if rate > MAX_REWARD_RATE_LAMPORTS_PER_SECOND as u128 {
            return Err(EngineError::MathOverflow);
        }

        self.reward_rate = rate as u64;
        self.reward_period_finish = now
            .checked_add(i64::try_from(duration).map_err(|_| EngineError::MathOverflow)?)
            .ok_or(EngineError::MathOverflow)?;
        self.reward_last_update = now;

        ledger.total_reward_funded = ledger
            .total_reward_funded
            .checked_add(amount)
            .ok_or(EngineError::MathOverflow)?;

        Ok(())
    }

    pub(crate) fn add_rigs(&mut self, count: u64, weight: u64) -> EngineResult<()> {
        let next = self
            .total_rigs
            .checked_add(count)
            .ok_or(EngineError::MathOverflow)?;
        if next > MAX_TOTAL_RIGS {
            return Err(EngineError::RigSupplyExceeded);
        }
        let shares = count
            .checked_mul(weight)
            .and_then(|s| self.total_shares.checked_add(s))
            .ok_or(EngineError::MathOverflow)?;
        self.total_rigs = next;
        self.total_shares = shares;
        Ok(())
    }
}
