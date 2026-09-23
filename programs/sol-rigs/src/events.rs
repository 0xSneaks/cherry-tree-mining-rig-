//! Emitted events. The off-chain indexer derives every displayed figure from
//! these plus account state — never from a parallel database of its own.

use anchor_lang::prelude::*;

#[event]
pub struct Initialized {
    pub authority: Pubkey,
    pub reward_funder: Pubkey,
    pub fee_recipient: Pubkey,
    pub rig_price_lamports: u64,
    pub purchase_fee_bps: u16,
    pub founder: Pubkey,
}

#[event]
pub struct FeeRecipientUpdated {
    pub previous: Pubkey,
    pub current: Pubkey,
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

/// THE CHERRY TREE is live. Emitted exactly once, by the founder activation
/// purchase, in the same instruction that writes `launch_t0`.
#[event]
pub struct LaunchActivated {
    pub founder: Pubkey,
    pub launch_t0: i64,
    pub rigs: u64,
    pub total_lamports: u64,
}

#[event]
pub struct ReferralBound {
    pub miner: Pubkey,
    pub referrer: Pubkey,
    pub weight: u16,
}
