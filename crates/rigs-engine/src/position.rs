//! Miner position state and the mandatory settle-then-transition ordering.

use crate::accumulator::{RewardGlobals, RewardLedger, PRECISION};
use crate::error::{EngineError, EngineResult};
use crate::pricing::{quote_purchase, rigs_affordable, PurchaseQuote};

/// Per-user position. Lives inside the MINER POSITION PDA on-chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MinerPosition {
    pub rigs: u64,
    /// Accumulator watermark this position has already been credited up to.
    pub reward_per_rig_paid: u128,
    /// Settled, unspent lamports. The "ore bin".
    pub pending_rewards: u64,

    pub lifetime_rigs_bought: u64,
    pub lifetime_rigs_compounded: u64,
    pub lifetime_sol_spent: u64,
    pub lifetime_sol_compounded: u64,
    pub lifetime_sol_claimed: u64,
}

/// What a `compound` actually did. Every field is exact lamports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompoundOutcome {
    pub rigs_minted: u64,
    /// Reward vault -> capital vault.
    pub capital_lamports: u64,
    /// Reward vault -> fee vault.
    pub fee_lamports: u64,
    /// `capital + fee`. Exactly what leaves the reward vault.
    pub total_lamports: u64,
    /// Ore bin left over. Never discarded.
    pub remainder_lamports: u64,
}

impl MinerPosition {
    /// `earned = pending_rewards + rigs * (stored - paid) / PRECISION`
    ///
    /// Read-only. Call [`RewardGlobals::sync`] first for a live figure.
    pub fn earned(&self, globals: &RewardGlobals) -> EngineResult<u64> {
        let delta = globals
            .reward_per_rig_stored
            .checked_sub(self.reward_per_rig_paid)
            .ok_or(EngineError::MathOverflow)?;

        let accrued = (self.rigs as u128)
            .checked_mul(delta)
            .ok_or(EngineError::MathOverflow)?
            / PRECISION;

        let total = (self.pending_rewards as u128)
            .checked_add(accrued)
            .ok_or(EngineError::MathOverflow)?;

        u64::try_from(total).map_err(|_| EngineError::MathOverflow)
    }

    /// Steps 1-4 of the mandatory ordering (spec §4):
    ///
    /// 1. update global reward state
    /// 2. compute user earned
    /// 3. write `pending_rewards`
    /// 4. set `reward_per_rig_paid = reward_per_rig_stored`
    ///
    /// The caller then performs the requested state transition as step 5.
    /// Because step 4 runs before any rig count changes, a rig purchased in
    /// this instruction cannot be credited with emission that predates it.
    pub fn settle(&mut self, globals: &mut RewardGlobals, now: i64) -> EngineResult<()> {
        globals.sync(now)?;
        self.pending_rewards = self.earned(globals)?;
        self.reward_per_rig_paid = globals.reward_per_rig_stored;
        Ok(())
    }
}

/// `buy_rigs(count)` — state side.
///
/// Returns the exact quote; the caller moves `capital_lamports` to the capital
/// vault and `fee_lamports` to the fee vault out of the *user's* wallet. No
/// purchase lamport ever touches the reward vault.
pub fn buy_rigs(
    globals: &mut RewardGlobals,
    position: &mut MinerPosition,
    now: i64,
    count: u64,
    rig_price_lamports: u64,
    purchase_fee_bps: u16,
) -> EngineResult<PurchaseQuote> {
    position.settle(globals, now)?;

    let quote = quote_purchase(count, rig_price_lamports, purchase_fee_bps)?;

    globals.add_rigs(count)?;
    position.rigs = position
        .rigs
        .checked_add(count)
        .ok_or(EngineError::MathOverflow)?;
    position.lifetime_rigs_bought = position
        .lifetime_rigs_bought
        .checked_add(count)
        .ok_or(EngineError::MathOverflow)?;
    position.lifetime_sol_spent = position
        .lifetime_sol_spent
        .checked_add(quote.total_lamports)
        .ok_or(EngineError::MathOverflow)?;

    Ok(quote)
}

/// `claim` — state side.
///
/// Zeroes `pending_rewards` *before* the caller performs the external
/// transfer, so a replay finds nothing left to pay. Rigs are untouched.
pub fn claim(
    globals: &mut RewardGlobals,
    position: &mut MinerPosition,
    ledger: &mut RewardLedger,
    now: i64,
) -> EngineResult<u64> {
    position.settle(globals, now)?;

    let amount = position.pending_rewards;
    if amount == 0 {
        return Err(EngineError::NothingToClaim);
    }

    position.pending_rewards = 0;
    position.lifetime_sol_claimed = position
        .lifetime_sol_claimed
        .checked_add(amount)
        .ok_or(EngineError::MathOverflow)?;
    ledger.total_reward_claimed = ledger
        .total_reward_claimed
        .checked_add(amount)
        .ok_or(EngineError::MathOverflow)?;

    Ok(amount)
}

/// `compound` — state side.
///
/// Spends as much of the ore bin as buys whole rigs, preserves the exact
/// remainder, and tells the caller precisely how many lamports must move out
/// of the reward vault and into the capital and fee vaults.
pub fn compound(
    globals: &mut RewardGlobals,
    position: &mut MinerPosition,
    ledger: &mut RewardLedger,
    now: i64,
    rig_price_lamports: u64,
    purchase_fee_bps: u16,
) -> EngineResult<CompoundOutcome> {
    position.settle(globals, now)?;

    let count = rigs_affordable(
        position.pending_rewards,
        rig_price_lamports,
        purchase_fee_bps,
    )?;
    if count == 0 {
        return Err(EngineError::NothingToCompound);
    }

    let quote = quote_purchase(count, rig_price_lamports, purchase_fee_bps)?;

    // Deduct only the exact amount spent. The remainder stays claimable.
    position.pending_rewards = position
        .pending_rewards
        .checked_sub(quote.total_lamports)
        .ok_or(EngineError::MathOverflow)?;

    globals.add_rigs(count)?;
    position.rigs = position
        .rigs
        .checked_add(count)
        .ok_or(EngineError::MathOverflow)?;
    position.lifetime_rigs_compounded = position
        .lifetime_rigs_compounded
        .checked_add(count)
        .ok_or(EngineError::MathOverflow)?;
    position.lifetime_sol_compounded = position
        .lifetime_sol_compounded
        .checked_add(quote.total_lamports)
        .ok_or(EngineError::MathOverflow)?;

    ledger.total_reward_compounded = ledger
        .total_reward_compounded
        .checked_add(quote.total_lamports)
        .ok_or(EngineError::MathOverflow)?;

    Ok(CompoundOutcome {
        rigs_minted: count,
        capital_lamports: quote.capital_lamports,
        fee_lamports: quote.fee_lamports,
        total_lamports: quote.total_lamports,
        remainder_lamports: position.pending_rewards,
    })
}

/// `estimated_output = (user_rigs / total_rigs) * reward_rate`, in lamports per
/// second. Display-only; the chain never uses this figure.
pub fn estimated_output_per_second(globals: &RewardGlobals, position: &MinerPosition) -> u64 {
    if globals.total_rigs == 0 {
        return 0;
    }
    let v = (position.rigs as u128) * (globals.reward_rate as u128) / globals.total_rigs as u128;
    u64::try_from(v).unwrap_or(u64::MAX)
}
