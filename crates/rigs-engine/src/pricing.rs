//! Rig pricing. Integer lamports only, no floating point anywhere.
//!
//! Fee sits *on top* of capital, so the user pays `capital + fee`:
//!
//! ```text
//! capital = count * rig_price_lamports
//! fee     = capital * purchase_fee_bps / 10_000     (floor)
//! total   = capital + fee
//! ```
//!
//! The frontend quote panel and the program MUST agree exactly; both call
//! [`quote_purchase`].

use crate::error::{EngineError, EngineResult};

/// Basis-point denominator.
pub const BPS_DENOMINATOR: u64 = 10_000;

/// Hard ceiling on the configurable purchase fee: 10.00%.
pub const MAX_PURCHASE_FEE_BPS: u16 = 1_000;

/// Upper bound on rigs bought in a single instruction. Keeps quote arithmetic
/// far away from the u64 ceiling and bounds the cost of a fat-fingered tx.
pub const MAX_RIGS_PER_PURCHASE: u64 = 1_000_000;

/// Upper bound on the global rig count. Bounds `rigs * reward_per_rig_delta`
/// inside the u128 accumulator with many orders of magnitude to spare.
pub const MAX_TOTAL_RIGS: u64 = 1_000_000_000_000;

/// Permanent per-wallet holding cap: `Miner.rigs <= MAX_WALLET_RIGS` after
/// every purchase and compound. A current-holding cap, not a purchase-volume
/// cap: it never expires, and compounding counts. Seeds are non-transferable
/// and non-redeemable, so the only ways a position grows are
/// [`crate::position::buy_rigs`] and [`crate::position::compound`], and both
/// enforce it. A legacy position already above the cap is never clipped;
/// its headroom is simply zero.
pub const MAX_WALLET_RIGS: u64 = 250;

/// Seeds `rigs` may still gain under [`MAX_WALLET_RIGS`]. Zero at or above
/// the cap (a legacy position above it is frozen, never reduced).
pub const fn wallet_headroom(rigs: u64) -> u64 {
    MAX_WALLET_RIGS.saturating_sub(rigs)
}

/// Exact lamport breakdown of a rig purchase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PurchaseQuote {
    pub count: u64,
    /// Routed to the CAPITAL vault.
    pub capital_lamports: u64,
    /// Routed to the FEE vault.
    pub fee_lamports: u64,
    /// `capital_lamports + fee_lamports`. What the user actually pays.
    pub total_lamports: u64,
}

/// Validate a `(rig_price_lamports, purchase_fee_bps)` pair.
pub fn validate_pricing(rig_price_lamports: u64, purchase_fee_bps: u16) -> EngineResult<()> {
    if rig_price_lamports == 0 {
        return Err(EngineError::InvalidConfig);
    }
    if purchase_fee_bps > MAX_PURCHASE_FEE_BPS {
        return Err(EngineError::InvalidConfig);
    }
    Ok(())
}

/// Exact cost of `count` rigs. This is the single source of truth for pricing.
pub fn quote_purchase(
    count: u64,
    rig_price_lamports: u64,
    purchase_fee_bps: u16,
) -> EngineResult<PurchaseQuote> {
    validate_pricing(rig_price_lamports, purchase_fee_bps)?;
    if count == 0 || count > MAX_RIGS_PER_PURCHASE {
        return Err(EngineError::InvalidRigCount);
    }

    let capital = (count as u128)
        .checked_mul(rig_price_lamports as u128)
        .ok_or(EngineError::MathOverflow)?;
    let fee = capital
        .checked_mul(purchase_fee_bps as u128)
        .ok_or(EngineError::MathOverflow)?
        / BPS_DENOMINATOR as u128;
    let total = capital.checked_add(fee).ok_or(EngineError::MathOverflow)?;

    Ok(PurchaseQuote {
        count,
        capital_lamports: u64::try_from(capital).map_err(|_| EngineError::MathOverflow)?,
        fee_lamports: u64::try_from(fee).map_err(|_| EngineError::MathOverflow)?,
        total_lamports: u64::try_from(total).map_err(|_| EngineError::MathOverflow)?,
    })
}

/// Largest `n` such that `quote_purchase(n).total_lamports <= budget`.
///
/// Used by `compound` to spend the ore bin down into whole rigs. The remainder
/// is never discarded — the caller subtracts only the exact quoted total.
///
/// Derivation: with `denom = price * (10_000 + bps)`, the real-valued cost of
/// `n` rigs is `n * denom / 10_000`, and the integer total is that value minus
/// a fractional part in `[0, 1)`. So `n0 = floor(budget * 10_000 / denom)`
/// always fits, and `n0 + 1` can only fit in the exact-equality case — never
/// `n0 + 2`. One upward probe is therefore sufficient and complete.
pub fn rigs_affordable(
    budget: u64,
    rig_price_lamports: u64,
    purchase_fee_bps: u16,
) -> EngineResult<u64> {
    validate_pricing(rig_price_lamports, purchase_fee_bps)?;

    let denom = (rig_price_lamports as u128)
        .checked_mul(BPS_DENOMINATOR as u128 + purchase_fee_bps as u128)
        .ok_or(EngineError::MathOverflow)?;

    let mut n = (budget as u128)
        .checked_mul(BPS_DENOMINATOR as u128)
        .ok_or(EngineError::MathOverflow)?
        / denom;

    if n > MAX_RIGS_PER_PURCHASE as u128 {
        n = MAX_RIGS_PER_PURCHASE as u128;
    }
    let mut n = n as u64;

    // Upward probe for the exact-equality case described above.
    if n < MAX_RIGS_PER_PURCHASE {
        if let Ok(next) = quote_purchase(n + 1, rig_price_lamports, purchase_fee_bps) {
            if next.total_lamports <= budget {
                n += 1;
            }
        }
    }

    // Defensive: the derivation guarantees this holds, but a config change must
    // never be able to over-spend the ore bin.
    while n > 0 {
        let q = quote_purchase(n, rig_price_lamports, purchase_fee_bps)?;
        if q.total_lamports <= budget {
            break;
        }
        n -= 1;
    }

    Ok(n)
}
