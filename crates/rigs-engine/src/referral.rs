//! Referral V1: a 1.05× reward weight for a validly referred planter.
//!
//! * The weight redistributes the same funded emission; it never adds any.
//! * No emission payout to the referrer, no levels, no stacking: the weight
//!   is either [`WEIGHT_BASE`] or [`WEIGHT_REFERRED`], decided once. The
//!   referrer's fee commission is separate (`fees.rs`, architecture §7a).
//! * One wallet, one immutable referrer, bound at its first purchase only.
//! * The referrer must already be a planter. Because a referrer must exist
//!   before the referred position does, and binding happens only at creation,
//!   a referral cycle (A→B→A) cannot be formed.
//!
//! What this does NOT do: prove distinct humans. One person with two wallets
//! can refer themselves across them. The damage is bounded — at most a 5%
//! larger slice of emission for the second wallet, paid for by every other
//! planter's share, never by new emission.

use crate::accumulator::{WEIGHT_BASE, WEIGHT_REFERRED};
use crate::error::{EngineError, EngineResult};

/// What the program knows about a proposed referrer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferrerCheck {
    pub referrer: [u8; 32],
    /// The referrer's position exists and holds at least one rig.
    pub referrer_is_planter: bool,
}

/// Weight for a position, enforcing every referral rule.
///
/// `is_new` is true only when this purchase creates the position.
pub fn referral_weight(
    owner: [u8; 32],
    is_new: bool,
    referrer: Option<ReferrerCheck>,
) -> EngineResult<u64> {
    let Some(r) = referrer else {
        return Ok(WEIGHT_BASE);
    };
    if !is_new {
        return Err(EngineError::ReferrerAlreadyBound);
    }
    if r.referrer == owner {
        return Err(EngineError::SelfReferral);
    }
    if r.referrer == [0u8; 32] || !r.referrer_is_planter {
        return Err(EngineError::InvalidReferrer);
    }
    Ok(WEIGHT_REFERRED)
}
