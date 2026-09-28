//! SOL-equivalent valuation primitives.
//!
//! A supplied price is a ratio `lamports_num / units_den` applied with floor
//! rounding. Which prices are in force for a settlement window is Pass #5's
//! reference-commitment layer ([`crate::pricing`]: committed epoch
//! references, in-epoch observations, verified resets); lot-level
//! application of the min rule lives in [`crate::lots`].

use crate::error::{EligibilityError, EligibilityResult};
use crate::types::Addr;

/// What a price key refers to: native SOL (lamports, always 1:1) or a mint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PriceKey {
    Sol,
    Mint(Addr),
}

/// What a supplied price measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceBasis {
    /// Committed manipulation-resistant reference price of the token itself.
    Reference,
    /// Verified NAV of the owned token (protocol-native xAssets).
    OwnedTokenNav,
    /// Notional of the *underlying* leveraged exposure. Never admissible as
    /// owned equity (section 5, test 29); represented only so that feeding
    /// it by mistake is rejected rather than silently used.
    UnderlyingNotional,
}

/// A verified per-unit price: `lamports_num / units_den` lamports per base
/// unit, applied with floor rounding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitPrice {
    pub lamports_num: u128,
    pub units_den: u128,
    pub basis: PriceBasis,
}

impl UnitPrice {
    pub fn value(&self, units: u64) -> EligibilityResult<u128> {
        if self.units_den == 0 {
            return Err(EligibilityError::InvalidAdapter);
        }
        (units as u128)
            .checked_mul(self.lamports_num)
            .map(|v| v / self.units_den)
            .ok_or(EligibilityError::MathOverflow)
    }
}
