//! SOL-equivalent valuation interface (the clean seam Pass #5 fills).
//!
//! Pass #4 does not produce prices: reference-price commitment, oracle
//! confidence and TWAP windows are Pass #5. It only defines how a supplied,
//! already-verified per-unit price is applied, identically to holdings and
//! to debt (section 5: debt is "valued in SOL at the same reference prices").

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

/// The prices in force for one settlement window.
#[derive(Clone, Debug, Default)]
pub struct PriceTable {
    prices: std::collections::BTreeMap<PriceKey, UnitPrice>,
}

impl PriceTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, key: PriceKey, price: UnitPrice) -> Self {
        self.prices.insert(key, price);
        self
    }

    /// `None` when no price is committed. SOL is always 1 lamport per lamport.
    pub fn get(&self, key: PriceKey) -> Option<UnitPrice> {
        match key {
            PriceKey::Sol => Some(UnitPrice {
                lamports_num: 1,
                units_den: 1,
                basis: PriceBasis::Reference,
            }),
            k => self.prices.get(&k).copied(),
        }
    }
}
