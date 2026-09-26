//! Protocol fee architecture — architecture §7a (founder-approved amendment).
//!
//! * Two independent fee rates: `purchase_fee_bps` (fresh `buy_rigs`) and
//!   `compound_fee_bps` (`compound` only), with
//!   `compound_fee_bps <= purchase_fee_bps <= MAX_PURCHASE_FEE_BPS` at every
//!   moment, including across a scheduled change.
//! * After `initialize`, both rates change only through a 72-hour timelock:
//!   schedule → (wait) → apply, or cancel. A new schedule replaces the
//!   pending one and restarts the full delay. Nothing shortens it.
//! * Referral commission: one direct level, `c = 20%` of the eligible fee,
//!   clamped so the Protocol Treasury always keeps `m = 8%` of the
//!   fee-bearing principal volume.
//! * Early-claim fee split: 60% of the fee moves REWARD → FEE (Protocol
//!   Treasury revenue), 40% stays in REWARD for the grove.
//!
//! The early-claim schedule, its split and the referral parameters are
//! program constants: changing them needs a disclosed program upgrade, never
//! a configuration call.

use crate::error::{EngineError, EngineResult};
use crate::pricing::{BPS_DENOMINATOR, MAX_PURCHASE_FEE_BPS};

/// Fee-change timelock: 72 hours.
pub const FEE_CHANGE_TIMELOCK_SECONDS: i64 = 259_200;

/// Referral commission `c`: 20% of the eligible fee.
pub const REFERRAL_COMMISSION_BPS: u64 = 2_000;
/// Retained margin `m`: 8% of fee-bearing principal volume, always kept by
/// the Protocol Treasury.
pub const REFERRAL_RETAINED_MARGIN_BPS: u64 = 800;

/// Treasury share of an early-claim fee: 60%. The rest (40%) stays in the
/// reward vault for the other planters.
pub const EARLY_CLAIM_TREASURY_BPS: u64 = 6_000;

/// Validate a `(purchase_fee_bps, compound_fee_bps)` pair:
/// `compound <= purchase <= MAX_PURCHASE_FEE_BPS`.
pub fn validate_fee_pair(purchase_fee_bps: u16, compound_fee_bps: u16) -> EngineResult<()> {
    if purchase_fee_bps > MAX_PURCHASE_FEE_BPS {
        return Err(EngineError::InvalidConfig);
    }
    if compound_fee_bps > purchase_fee_bps {
        return Err(EngineError::CompoundFeeAbovePurchaseFee);
    }
    Ok(())
}

/// The fee rates and the pending timelocked change. Lives in the Config PDA.
///
/// `effective_at == 0` means no change is pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeeSchedule {
    pub purchase_fee_bps: u16,
    pub compound_fee_bps: u16,
    pub pending_purchase_fee_bps: u16,
    pub pending_compound_fee_bps: u16,
    pub effective_at: i64,
}

impl FeeSchedule {
    pub fn has_pending(&self) -> bool {
        self.effective_at != 0
    }

    /// `schedule_fee_change`: validate both values against the ceiling and
    /// each other, store them as the pending change with
    /// `effective_at = now + 72h`. Replaces any pending change and restarts
    /// the full delay. Returns `effective_at`.
    pub fn schedule(
        &mut self,
        purchase_fee_bps: u16,
        compound_fee_bps: u16,
        now: i64,
    ) -> EngineResult<i64> {
        validate_fee_pair(purchase_fee_bps, compound_fee_bps)?;
        let effective_at = now
            .checked_add(FEE_CHANGE_TIMELOCK_SECONDS)
            .ok_or(EngineError::MathOverflow)?;
        // `0` is the "none pending" sentinel; a chain clock this far in the
        // past cannot occur, but never let it alias.
        if effective_at <= 0 {
            return Err(EngineError::InvalidConfig);
        }
        self.pending_purchase_fee_bps = purchase_fee_bps;
        self.pending_compound_fee_bps = compound_fee_bps;
        self.effective_at = effective_at;
        Ok(effective_at)
    }

    /// `apply_fee_change`: only at or after `effective_at`; writes both
    /// values together and clears the pending change.
    pub fn apply(&mut self, now: i64) -> EngineResult<(u16, u16)> {
        if !self.has_pending() {
            return Err(EngineError::NoPendingFeeChange);
        }
        if now < self.effective_at {
            return Err(EngineError::FeeTimelockActive);
        }
        // Validated at schedule time; re-checked so a corrupted pending pair
        // can never land.
        validate_fee_pair(self.pending_purchase_fee_bps, self.pending_compound_fee_bps)?;
        self.purchase_fee_bps = self.pending_purchase_fee_bps;
        self.compound_fee_bps = self.pending_compound_fee_bps;
        self.clear_pending();
        Ok((self.purchase_fee_bps, self.compound_fee_bps))
    }

    /// `cancel_fee_change`.
    pub fn cancel(&mut self) -> EngineResult<()> {
        if !self.has_pending() {
            return Err(EngineError::NoPendingFeeChange);
        }
        self.clear_pending();
        Ok(())
    }

    fn clear_pending(&mut self) {
        self.pending_purchase_fee_bps = 0;
        self.pending_compound_fee_bps = 0;
        self.effective_at = 0;
    }
}

/// Referral commission on one eligible fee event (architecture §7 / §7a):
///
/// ```text
/// required_margin = ceil(0.08 v)
/// commission      = min(floor(0.20 f), max(0, f - required_margin))
/// retained        = f - commission >= min(f, required_margin)
/// ```
///
/// `fee` is the actual fee received in FEE for the event; `volume` is the
/// event's fee-bearing principal (the capital portion). Integer rounding can
/// only shrink the commission, never the margin.
pub fn referral_commission(fee: u64, volume: u64) -> EngineResult<u64> {
    let raw = (fee as u128)
        .checked_mul(REFERRAL_COMMISSION_BPS as u128)
        .ok_or(EngineError::MathOverflow)?
        / BPS_DENOMINATOR as u128;
    let margin = (volume as u128)
        .checked_mul(REFERRAL_RETAINED_MARGIN_BPS as u128)
        .ok_or(EngineError::MathOverflow)?
        .div_ceil(BPS_DENOMINATOR as u128);
    let headroom = (fee as u128).saturating_sub(margin);
    u64::try_from(raw.min(headroom)).map_err(|_| EngineError::MathOverflow)
}

/// Split an early-claim fee `x`: `treasury = floor(0.60 x)` (REWARD → FEE),
/// `grove = x - treasury` (stays in REWARD, redistributed). Rounding favours
/// the grove. The parts sum exactly to `x`.
pub fn early_claim_fee_split(fee: u64) -> (u64, u64) {
    let treasury =
        ((fee as u128) * EARLY_CLAIM_TREASURY_BPS as u128 / BPS_DENOMINATOR as u128) as u64;
    (treasury, fee - treasury)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commission_at_ten_percent_is_exactly_twenty_percent() {
        // v = 1 SOL, f = 10%.
        assert_eq!(
            referral_commission(100_000_000, 1_000_000_000).unwrap(),
            20_000_000
        );
    }

    #[test]
    fn split_sums() {
        for x in [0u64, 1, 2, 3, 5, 9, 10, 99, 1_000_001, u64::MAX] {
            let (t, g) = early_claim_fee_split(x);
            assert_eq!(t + g, x);
            assert_eq!(t as u128, x as u128 * 6 / 10);
        }
    }
}
