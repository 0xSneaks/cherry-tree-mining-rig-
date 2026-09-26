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
    /// Net lamports paid to the owner.
    pub amount: u64,
    /// Settled ore bin before the penalty.
    pub gross: u64,
    /// Forfeited, kept in the reward vault for the other planters.
    pub penalty: u64,
    pub penalty_bps: u16,
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

/// The cumulative per-wallet Genesis fresh-purchase cap was pinned: by a
/// fresh V2 `initialize`, or by a successful V1 -> V2 `migrate_v2`. The
/// historical V1 `Initialized` schema is frozen and never carries the cap.
#[event]
pub struct GenesisCapConfigured {
    pub genesis_cap_rigs: u64,
}

/// A `.skr` domain was bound to its one Day-1 wallet by the first Day-1
/// purchase that proved it.
#[event]
pub struct SkrDomainBound {
    pub name_account: Pubkey,
    pub wallet: Pubkey,
    pub bound_at: i64,
}

// --- Fruit ledger (§10 / §10a) ---

#[event]
pub struct FruitConfigInitialized {
    pub admin: Pubkey,
    pub root_poster: Pubkey,
    pub veto_signer_count: u8,
    pub veto_threshold: u8,
    pub challenge_window_secs: i64,
    pub reserve_recipient: Pubkey,
}

#[event]
pub struct FruitLedgerInitialized {
    pub kind: u8,
    pub ledger: Pubkey,
    pub vault: Pubkey,
}

#[event]
pub struct FruitFunded {
    pub kind: u8,
    pub funder: Pubkey,
    pub amount: u64,
    pub reserve: u64,
}

#[event]
pub struct FruitPoolCommitted {
    pub kind: u8,
    pub pool: Pubkey,
    pub pool_id: u64,
    pub budget: u64,
    pub t_start: i64,
    pub t_end: i64,
    pub wallet_cap: u64,
    pub reference_hash: [u8; 32],
}

#[event]
pub struct FruitRootPosted {
    pub pool: Pubkey,
    pub version: u64,
    pub merkle_root: [u8; 32],
    pub checkpoint_time: i64,
    pub total: u64,
    pub complete: bool,
    pub final_after: i64,
}

#[event]
pub struct FruitRootVetoVote {
    pub pool: Pubkey,
    pub version: u64,
    pub signer: Pubkey,
    pub veto_count: u8,
    pub vetoed: bool,
}

#[event]
pub struct FruitRootFinalized {
    pub pool: Pubkey,
    pub version: u64,
    pub total: u64,
    pub established: u64,
    pub complete: bool,
    pub terminal: bool,
}

#[event]
pub struct FruitClaimed {
    pub pool: Pubkey,
    pub wallet: Pubkey,
    pub cumulative_amount: u64,
    pub payment: u64,
    pub claimed_cum: u64,
}

#[event]
pub struct FruitPoolClosed {
    pub pool: Pubkey,
    pub final_total: u64,
    pub released: u64,
}

#[event]
pub struct FruitReserveWithdrawn {
    pub kind: u8,
    pub destination: Pubkey,
    pub amount: u64,
    pub reserve: u64,
}

#[event]
pub struct FruitPausedSet {
    pub kind: u8,
    pub paused: bool,
}
