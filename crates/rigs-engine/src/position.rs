//! Miner position state and the mandatory settle-then-transition ordering.

use crate::accumulator::{RewardGlobals, RewardLedger, PRECISION, WEIGHT_BASE};
use crate::error::{EngineError, EngineResult};
use crate::fees::early_claim_fee_split;
use crate::penalty::{claim_penalty_bps, penalty_amount};
use crate::pricing::{quote_purchase, rigs_affordable, wallet_headroom, PurchaseQuote};

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
    /// Reward weight per rig: [`WEIGHT_BASE`] or `WEIGHT_REFERRED`. Fixed when
    /// the position is created. `0` (a default-constructed position) is read
    /// as [`WEIGHT_BASE`].
    pub weight: u64,
    /// Chain time of the most recent purchase — the claim-penalty anchor.
    /// See [`crate::penalty`].
    pub last_planted_at: i64,
}

/// What a `claim` did. Every field is exact lamports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaimOutcome {
    /// Everything settled in the ore bin.
    pub gross: u64,
    /// Early-claim fee rate applied.
    pub penalty_bps: u64,
    /// Early-claim fee `x`: `penalty_treasury + penalty_grove`.
    pub penalty: u64,
    /// `floor(0.60 x)`: reward vault -> fee vault, Protocol Treasury revenue.
    pub penalty_treasury: u64,
    /// `x - penalty_treasury`: stays in the reward vault, re-credited to the
    /// other planters (or dust).
    pub penalty_grove: u64,
    /// Paid to the planter: `gross − penalty`.
    pub net: u64,
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
    /// Weight per rig, with `0` read as the base weight.
    pub fn effective_weight(&self) -> u64 {
        if self.weight == 0 {
            WEIGHT_BASE
        } else {
            self.weight
        }
    }

    /// `rigs × weight`: this position's claim on emission.
    pub fn shares(&self) -> EngineResult<u64> {
        self.rigs
            .checked_mul(self.effective_weight())
            .ok_or(EngineError::MathOverflow)
    }

    /// `earned = pending_rewards + shares * (stored - paid) / PRECISION`
    ///
    /// Read-only. Call [`RewardGlobals::sync`] first for a live figure.
    pub fn earned(&self, globals: &RewardGlobals) -> EngineResult<u64> {
        let delta = globals
            .reward_per_rig_stored
            .checked_sub(self.reward_per_rig_paid)
            .ok_or(EngineError::MathOverflow)?;

        let accrued = (self.shares()? as u128)
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
///
/// Enforces the permanent holding cap: `rigs + count > MAX_WALLET_RIGS` is
/// [`EngineError::WalletCapExceeded`], decided before anything is mutated —
/// no partial fill, no settlement, no state change.
pub fn buy_rigs(
    globals: &mut RewardGlobals,
    position: &mut MinerPosition,
    now: i64,
    count: u64,
    rig_price_lamports: u64,
    purchase_fee_bps: u16,
) -> EngineResult<PurchaseQuote> {
    let quote = quote_purchase(count, rig_price_lamports, purchase_fee_bps)?;
    if count > wallet_headroom(position.rigs) {
        return Err(EngineError::WalletCapExceeded);
    }

    position.settle(globals, now)?;

    globals.add_rigs(count, position.effective_weight())?;
    position.rigs = position
        .rigs
        .checked_add(count)
        .ok_or(EngineError::MathOverflow)?;
    // Early-claim fee anchor reset: new capital restarts the clock for the
    // whole position (architecture §7a item 6; disclosed before a top-up).
    position.last_planted_at = now;
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
///
/// Applies the early-claim fee ([`crate::penalty`]): the caller transfers
/// `net` to the claimant and `penalty_treasury` to the fee vault; the grove
/// share never leaves the reward vault and is re-credited to every other
/// planter's shares here — then the claimer's watermark is moved past it, so
/// the claimer recovers none of it.
pub fn claim(
    globals: &mut RewardGlobals,
    position: &mut MinerPosition,
    ledger: &mut RewardLedger,
    now: i64,
) -> EngineResult<ClaimOutcome> {
    position.settle(globals, now)?;

    let gross = position.pending_rewards;
    if gross == 0 {
        return Err(EngineError::NothingToClaim);
    }
    let penalty_bps = claim_penalty_bps(position.last_planted_at, now);
    let penalty = penalty_amount(gross, penalty_bps)?;
    let (penalty_treasury, penalty_grove) = early_claim_fee_split(penalty);
    let net = gross - penalty;

    position.pending_rewards = 0;
    globals.redistribute_excluding(penalty_grove, position.shares()?)?;
    position.reward_per_rig_paid = globals.reward_per_rig_stored;

    position.lifetime_sol_claimed = position
        .lifetime_sol_claimed
        .checked_add(net)
        .ok_or(EngineError::MathOverflow)?;
    ledger.total_reward_claimed = ledger
        .total_reward_claimed
        .checked_add(net)
        .ok_or(EngineError::MathOverflow)?;
    ledger.total_reward_to_fees = ledger
        .total_reward_to_fees
        .checked_add(penalty_treasury)
        .ok_or(EngineError::MathOverflow)?;

    Ok(ClaimOutcome {
        gross,
        penalty_bps,
        penalty,
        penalty_treasury,
        penalty_grove,
        net,
    })
}

/// `compound` — state side.
///
/// Spends as much of the ore bin as buys whole rigs, preserves the exact
/// remainder, and tells the caller precisely how many lamports must move out
/// of the reward vault and into the capital and fee vaults.
///
/// Priced at `compound_fee_bps` (architecture §7a item 2), never the purchase
/// fee. Never touches `last_planted_at` and never pays the early-claim fee.
///
/// Capped: mints `min(affordable, MAX_WALLET_RIGS - rigs)` and debits only
/// the exact quote for that count; everything else stays claimable. With zero
/// headroom (at or above the cap) it is [`EngineError::WalletCapExceeded`]:
/// the transaction reverts, so no seed is created, no fee is charged, no
/// lamport moves and no anchor changes.
pub fn compound(
    globals: &mut RewardGlobals,
    position: &mut MinerPosition,
    ledger: &mut RewardLedger,
    now: i64,
    rig_price_lamports: u64,
    compound_fee_bps: u16,
) -> EngineResult<CompoundOutcome> {
    let headroom = wallet_headroom(position.rigs);
    if headroom == 0 {
        return Err(EngineError::WalletCapExceeded);
    }

    position.settle(globals, now)?;

    let affordable = rigs_affordable(
        position.pending_rewards,
        rig_price_lamports,
        compound_fee_bps,
    )?;
    let count = affordable.min(headroom);
    if count == 0 {
        return Err(EngineError::NothingToCompound);
    }

    let quote = quote_purchase(count, rig_price_lamports, compound_fee_bps)?;

    // Deduct only the exact amount spent. The remainder stays claimable.
    position.pending_rewards = position
        .pending_rewards
        .checked_sub(quote.total_lamports)
        .ok_or(EngineError::MathOverflow)?;

    globals.add_rigs(count, position.effective_weight())?;
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

/// `estimated_output = (user_shares / total_shares) * reward_rate`, in lamports
/// per second. Display-only; the chain never uses this figure.
pub fn estimated_output_per_second(globals: &RewardGlobals, position: &MinerPosition) -> u64 {
    if globals.total_shares == 0 {
        return 0;
    }
    let shares = position.shares().unwrap_or(u64::MAX);
    let v = (shares as u128) * (globals.reward_rate as u128) / globals.total_shares as u128;
    u64::try_from(v).unwrap_or(u64::MAX)
}
