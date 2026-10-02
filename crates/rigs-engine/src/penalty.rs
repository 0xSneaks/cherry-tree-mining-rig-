//! Early-claim fee (architecture §7a item 5): a claim soon after the
//! position's most recent fresh purchase pays a disclosed fee on the whole
//! claimed amount.
//!
//! ```text
//! age = now − last_planted_at            (chain clock only)
//!   0 ≤ age < 24h   30.00%
//!  24h ≤ age < 48h  20.00%
//!  48h ≤ age < 72h  10.00%
//!  72h ≤ age          0%
//! x = floor(gross × bps / 10_000)
//! ```
//!
//! Reset rule: `last_planted_at` is set to the chain time of every
//! `buy_rigs` on the position (new capital restarts the clock for the whole
//! position). Claims do not move it. Compounds do not move it, and a compound
//! never pays the fee. A negative age (clock behind the anchor) is treated as
//! age 0 — the strict direction.
//!
//! Where it goes ([`crate::fees::early_claim_fee_split`]): `floor(0.60 x)`
//! moves REWARD → FEE in the claim instruction as Protocol Treasury revenue;
//! the rest stays in the reward vault and is re-credited through the reward
//! accumulator to every OTHER planter's shares — never the claimer's, or
//! repeated same-slot claims could recover it. If there is no other planter
//! the grove share is credited to `undistributed_dust`: it stays in the
//! reward vault, unpaid to anyone.
//!
//! The function names keep the V1 "penalty" wording so the on-chain field
//! and event names (`total_claim_penalties`, `Claim.penalty`) stay stable.

use crate::accumulator::{RewardGlobals, PRECISION};
use crate::error::{EngineError, EngineResult};
use crate::pricing::BPS_DENOMINATOR;

pub const PENALTY_WINDOW_SECONDS: i64 = 24 * 60 * 60;

/// Early-claim fee in basis points for a claim at `now` on a position last
/// freshly purchased at `last_planted_at`. Boundaries are half-open.
pub fn claim_penalty_bps(last_planted_at: i64, now: i64) -> u64 {
    let age = now.saturating_sub(last_planted_at).max(0);
    if age < PENALTY_WINDOW_SECONDS {
        3_000
    } else if age < 2 * PENALTY_WINDOW_SECONDS {
        2_000
    } else if age < 3 * PENALTY_WINDOW_SECONDS {
        1_000
    } else {
        0
    }
}

/// Early-claim fee `x = floor(gross × bps / 10_000)`. Never more than
/// `gross`.
pub fn penalty_amount(gross: u64, bps: u64) -> EngineResult<u64> {
    let num = (gross as u128)
        .checked_mul(bps as u128)
        .ok_or(EngineError::MathOverflow)?;
    let p = num / BPS_DENOMINATOR as u128;
    u64::try_from(p.min(gross as u128)).map_err(|_| EngineError::MathOverflow)
}

impl RewardGlobals {
    /// Re-credit `amount` lamports already in the reward vault to every share
    /// except `excluded_shares` (the claimer's). The caller must then set the
    /// claimer's watermark to the new `reward_per_rig_stored`, so the
    /// claimer's own shares receive none of it.
    ///
    /// Flooring leaves < 1 lamport per share in the vault, attributed to no
    /// one — the over-collateralised direction. With no other shares it is
    /// credited to dust.
    pub fn redistribute_excluding(
        &mut self,
        amount: u64,
        excluded_shares: u64,
    ) -> EngineResult<()> {
        if amount == 0 {
            return Ok(());
        }
        let others = self.total_shares.saturating_sub(excluded_shares);
        if others == 0 {
            self.undistributed_dust = self
                .undistributed_dust
                .checked_add(amount)
                .ok_or(EngineError::MathOverflow)?;
            return Ok(());
        }
        let per_share = (amount as u128)
            .checked_mul(PRECISION)
            .ok_or(EngineError::MathOverflow)?
            / others as u128;
        self.reward_per_rig_stored = self
            .reward_per_rig_stored
            .checked_add(per_share)
            .ok_or(EngineError::MathOverflow)?;
        Ok(())
    }
}
