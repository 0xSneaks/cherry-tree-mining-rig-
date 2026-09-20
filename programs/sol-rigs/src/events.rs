//! Emitted events. The off-chain indexer derives every displayed figure from
//! these plus account state — never from a parallel database of its own.

use anchor_lang::prelude::*;

#[event]
pub struct Initialized {
    pub authority: Pubkey,
    pub reward_funder: Pubkey,
    pub rig_price_lamports: u64,
    pub purchase_fee_bps: u16,
}

#[event]
pub struct RewardsFunded {
    pub funder: Pubkey,
    pub amount: u64,
    pub duration: u64,
    pub new_reward_rate: u64,
    pub period_finish: i64,
    pub reward_vault_balance: u64,
    pub outstanding_liabilities: u64,
}

#[event]
pub struct RigsPurchased {
    pub owner: Pubkey,
    pub count: u64,
    pub capital_lamports: u64,
    pub fee_lamports: u64,
    pub total_lamports: u64,
    pub user_rigs: u64,
    pub total_rigs: u64,
}

#[event]
pub struct Claim {
    pub owner: Pubkey,
    pub amount: u64,
    pub user_rigs: u64,
    pub total_reward_claimed: u64,
}

#[event]
pub struct Compound {
    pub owner: Pubkey,
    pub rigs_minted: u64,
    pub capital_lamports: u64,
    pub fee_lamports: u64,
    pub total_lamports: u64,
    pub remainder_lamports: u64,
    pub user_rigs: u64,
    pub total_rigs: u64,
}

#[event]
pub struct FeesWithdrawn {
    pub authority: Pubkey,
    pub destination: Pubkey,
    pub amount: u64,
}

#[event]
pub struct PausedSet {
    pub paused: bool,
}

#[event]
pub struct PricingUpdated {
    pub rig_price_lamports: u64,
    pub purchase_fee_bps: u16,
}

#[event]
pub struct AuthorityTransferred {
    pub previous: Pubkey,
    pub current: Pubkey,
}

#[event]
pub struct RewardFunderUpdated {
    pub reward_funder: Pubkey,
}
