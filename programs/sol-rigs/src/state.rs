//! On-chain account layouts, and the bridge to the pure engine.
//!
//! Account structs are dumb storage. Every mutation that decides lamports goes
//! through [`rigs_engine`]: read state out, run the transition, write it back.

use anchor_lang::prelude::*;
use rigs_engine::{MinerPosition, RewardGlobals, RewardLedger};

use crate::constants::VaultKind;

/// Singleton CONFIG PDA — seeds `["config"]`.
#[account]
#[derive(InitSpace, Debug)]
pub struct Config {
    // --- authority ---
    pub authority: Pubkey,
    /// Two-step authority handover; `Pubkey::default()` when none is pending.
    pub pending_authority: Pubkey,
    /// The only signer permitted to call `fund_rewards`.
    pub reward_funder: Pubkey,
    /// The only account protocol fees may be withdrawn TO.
    ///
    /// Deliberately not the same identity as `authority`: the authority can
    /// trigger a withdrawal but cannot choose where it lands, and this account
    /// never signs anything, so collecting fees never requires its key. Moving
    /// it is an authority action and emits an event.
    pub fee_recipient: Pubkey,
    /// Gates `buy_rigs` / `claim` / `compound`. Funding stays open so backing
    /// can be topped up while user actions are halted.
    pub paused: bool,

    // --- pricing (configurable; never hardcoded outside test fixtures) ---
    pub rig_price_lamports: u64,
    pub purchase_fee_bps: u16,

    // --- reward accumulator (mirrors rigs_engine::RewardGlobals) ---
    pub total_rigs: u64,
    pub reward_rate: u64,
    pub reward_period_finish: i64,
    pub reward_last_update: i64,
    pub reward_per_rig_stored: u128,
    pub undistributed_dust: u64,

    // --- cumulative ledgers ---
    pub total_reward_funded: u64,
    pub total_reward_claimed: u64,
    pub total_reward_compounded: u64,
    pub total_capital_received: u64,
    pub total_protocol_fees: u64,
    pub total_fees_withdrawn: u64,
    pub miner_count: u64,

    // --- bumps ---
    pub capital_vault_bump: u8,
    pub reward_vault_bump: u8,
    pub fee_vault_bump: u8,
    pub version: u16,
    pub bump: u8,

    // --- launch (appended) ---
    /// The only wallet whose purchase can activate launch. Pinned at
    /// `initialize` and never writable afterwards: there is no setter.
    pub founder: Pubkey,
    /// T0: chain time of the founder activation purchase. `0` until then.
    /// Written exactly once, inside that purchase; no instruction resets,
    /// replaces or moves it.
    pub launch_t0: i64,
    /// `Σ rigs × weight` (mirrors `RewardGlobals::total_shares`).
    pub total_shares: u64,

    // --- claim penalty (appended) ---
    /// Lamports forfeited by early claims, all still in the reward vault and
    /// re-credited to other planters (or dust). Never fee revenue.
    pub total_claim_penalties: u64,
}

impl Config {
    pub fn globals(&self) -> RewardGlobals {
        RewardGlobals {
            total_rigs: self.total_rigs,
            total_shares: self.total_shares,
            reward_rate: self.reward_rate,
            reward_period_finish: self.reward_period_finish,
            reward_last_update: self.reward_last_update,
            reward_per_rig_stored: self.reward_per_rig_stored,
            undistributed_dust: self.undistributed_dust,
        }
    }

    pub fn write_globals(&mut self, g: &RewardGlobals) {
        self.total_rigs = g.total_rigs;
        self.total_shares = g.total_shares;
        self.reward_rate = g.reward_rate;
        self.reward_period_finish = g.reward_period_finish;
        self.reward_last_update = g.reward_last_update;
        self.reward_per_rig_stored = g.reward_per_rig_stored;
        self.undistributed_dust = g.undistributed_dust;
    }

    pub fn ledger(&self) -> RewardLedger {
        RewardLedger {
            total_reward_funded: self.total_reward_funded,
            total_reward_claimed: self.total_reward_claimed,
            total_reward_compounded: self.total_reward_compounded,
        }
    }

    pub fn write_ledger(&mut self, l: &RewardLedger) {
        self.total_reward_funded = l.total_reward_funded;
        self.total_reward_claimed = l.total_reward_claimed;
        self.total_reward_compounded = l.total_reward_compounded;
    }
}

/// MINER POSITION PDA — seeds `["miner", owner]`.
#[account]
#[derive(InitSpace, Debug)]
pub struct Miner {
    pub owner: Pubkey,
    pub rigs: u64,
    pub reward_per_rig_paid: u128,
    pub pending_rewards: u64,
    pub lifetime_rigs_bought: u64,
    pub lifetime_rigs_compounded: u64,
    pub lifetime_sol_spent: u64,
    pub lifetime_sol_compounded: u64,
    pub lifetime_sol_claimed: u64,
    pub created_at: i64,
    pub last_action_at: i64,
    pub bump: u8,

    // --- referral (appended) ---
    /// Reward weight per rig: 100 (base) or 105 (validly referred). Fixed at
    /// the position's first purchase.
    pub weight: u16,
    /// Immutable referrer, or `Pubkey::default()` for none.
    pub referrer: Pubkey,

    // --- claim penalty (appended) ---
    /// Chain time of this position's most recent purchase: the claim-penalty
    /// anchor. Claims and replants never move it.
    pub last_planted_at: i64,
}

impl Miner {
    pub fn position(&self) -> MinerPosition {
        MinerPosition {
            rigs: self.rigs,
            reward_per_rig_paid: self.reward_per_rig_paid,
            pending_rewards: self.pending_rewards,
            lifetime_rigs_bought: self.lifetime_rigs_bought,
            lifetime_rigs_compounded: self.lifetime_rigs_compounded,
            lifetime_sol_spent: self.lifetime_sol_spent,
            lifetime_sol_compounded: self.lifetime_sol_compounded,
            lifetime_sol_claimed: self.lifetime_sol_claimed,
            weight: self.weight as u64,
            last_planted_at: self.last_planted_at,
        }
    }

    pub fn write_position(&mut self, p: &MinerPosition) {
        self.rigs = p.rigs;
        self.reward_per_rig_paid = p.reward_per_rig_paid;
        self.pending_rewards = p.pending_rewards;
        self.lifetime_rigs_bought = p.lifetime_rigs_bought;
        self.lifetime_rigs_compounded = p.lifetime_rigs_compounded;
        self.lifetime_sol_spent = p.lifetime_sol_spent;
        self.lifetime_sol_compounded = p.lifetime_sol_compounded;
        self.lifetime_sol_claimed = p.lifetime_sol_claimed;
        self.last_planted_at = p.last_planted_at;
    }
}

/// A program-owned SOL vault. Carries no balance field — the authoritative
/// balance is the account's own lamports.
#[account]
#[derive(InitSpace, Debug)]
pub struct Vault {
    /// [`VaultKind`] discriminant.
    pub kind: u8,
    pub bump: u8,
}

impl Vault {
    pub fn is(&self, kind: VaultKind) -> bool {
        self.kind == kind as u8
    }
}
