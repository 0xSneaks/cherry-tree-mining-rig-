//! Engine error codes.
//!
//! These are deliberately plain (no Solana / Anchor types) so the engine stays
//! unit-testable on a host target. The on-chain program maps each variant onto
//! an Anchor error code one-for-one.

use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineError {
    /// Any checked integer operation that would have overflowed or underflowed.
    MathOverflow,
    /// `rig_price_lamports` is zero, or `purchase_fee_bps` exceeds the cap.
    InvalidConfig,
    /// `buy_rigs(0)` / a count above `MAX_RIGS_PER_PURCHASE`.
    InvalidRigCount,
    /// Global rig count would exceed `MAX_TOTAL_RIGS`.
    RigSupplyExceeded,
    /// `fund_rewards` called with a zero or absurd duration.
    InvalidDuration,
    /// `fund_rewards` called with zero lamports.
    InvalidFundingAmount,
    /// Funding is so small relative to duration that the per-second rate floors
    /// to zero. Rejected rather than silently stalling the schedule.
    RewardRateZero,
    /// `claim` with nothing settled.
    NothingToClaim,
    /// `compound` when the ore bin cannot afford a single whole rig.
    NothingToCompound,
    /// The reward vault does not hold enough lamports to back outstanding
    /// liabilities. Fatal: the instruction must abort.
    InsufficientRewardBacking,
    /// Clock moved backwards past a recorded checkpoint.
    ClockWentBackwards,
    /// A wallet named itself as its own referrer.
    SelfReferral,
    /// A referrer was supplied for a position that already exists. The
    /// referrer is bound once, at the first purchase, and never replaced.
    ReferrerAlreadyBound,
    /// The referrer is not an existing planter.
    InvalidReferrer,
    /// Buying is closed: pre-launch (and not the founder activation), or the
    /// current access phase requires an identity the buyer did not prove.
    AccessDenied,
    /// The founder activation purchase was attempted without an active,
    /// funded reward schedule.
    ActivationUnfunded,
    /// The wallet's cumulative fresh purchases during the gated launch period
    /// would exceed the configured Genesis cap. Appended for Production V2;
    /// existing codes are unchanged.
    GenesisCapExceeded,
    /// The purchase would take the wallet above the permanent holding cap
    /// (`MAX_WALLET_RIGS`), or a compound was attempted with zero headroom.
    /// Appended for the hard-cap build; existing codes are unchanged.
    WalletCapExceeded,
    /// Day-1 access: this `.skr` domain is already bound to another wallet.
    /// Appended for the hard-cap build; existing codes are unchanged.
    SkrDomainBound,
    // --- Fruit ledger (§10 / §10a). Appended; existing codes are unchanged. ---
    /// A Fruit vault's spendable lamports fell below `L + U + Z`.
    FruitInsolvent,
    /// Committing more than the ledger's uncommitted reserve `Z`.
    InsufficientFruitReserve,
    /// A transition would take `U` below zero.
    FruitCommitmentUnderflow,
    /// A transition would take `L` below zero.
    FruitLiabilityUnderflow,
    /// Reserve withdrawal above `Z`; `L` and `U` are never withdrawable.
    ReserveWithdrawalExceedsReserve,
    /// Unknown ledger kind.
    InvalidLedgerKind,
    /// Stored pool/root status byte is not a known value.
    InvalidFruitState,
    /// Pool window with `t_end <= t_start`.
    InvalidPoolWindow,
    /// Pool committed at or after its `t_start`.
    FruitPoolStarted,
    /// Root checkpoint in the future.
    FutureCheckpoint,
    /// Root version not strictly above every earlier version for the pool.
    StaleRootVersion,
    /// A pool already has a Pending root.
    RootAlreadyPending,
    /// Root total above the pool's scheduled capacity at its checkpoint.
    RootExceedsCapacity,
    /// Root total below the pool's already-final total.
    RootBelowFinalTotal,
    /// A root marked complete must checkpoint at or after `t_end`.
    IncompleteCheckpoint,
    /// Veto or finalize on a root that is not the pool's Pending root.
    RootNotPending,
    /// Finalize before the challenge window has elapsed.
    ChallengeWindowOpen,
    /// Veto after the challenge window has elapsed.
    ChallengeWindowClosed,
    /// Claim on a pool with no Final root.
    NoFinalRoot,
    /// Claim leaf above the pool's per-wallet cap `floor(E/5)`.
    LeafAboveWalletCap,
    /// A payment would take the pool's `paid_cum` above `final_total`.
    PaidExceedsFinalTotal,
    /// The pool is Closed.
    FruitPoolClosed,
    /// The pool has not reached `t_end`.
    FruitPoolNotEnded,
    /// Close without a complete, Final, unvetoed root.
    FruitPoolNotReconciled,
    /// Invalid governance configuration (veto set, threshold, window, poster).
    InvalidFruitGovernance,
    /// `compound_fee_bps > purchase_fee_bps` (architecture §7a item 2).
    CompoundFeeAbovePurchaseFee,
    /// Apply or cancel with no scheduled fee change.
    NoPendingFeeChange,
    /// Apply before the 72-hour fee-change timelock has elapsed.
    FeeTimelockActive,
    /// A root's `forfeited` exceeds the capacity it releases, or is set on a
    /// root that is not marked complete.
    InvalidForfeited,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            EngineError::MathOverflow => "checked math overflowed",
            EngineError::SelfReferral => "a wallet cannot refer itself",
            EngineError::ReferrerAlreadyBound => "referrer is bound at the first purchase only",
            EngineError::InvalidReferrer => "referrer is not an existing planter",
            EngineError::AccessDenied => "buying is not open to this wallet in the current phase",
            EngineError::ActivationUnfunded => {
                "activation requires an active funded reward schedule"
            }
            EngineError::InvalidConfig => "invalid config",
            EngineError::InvalidRigCount => "invalid rig count",
            EngineError::RigSupplyExceeded => "rig supply exceeded",
            EngineError::InvalidDuration => "invalid reward duration",
            EngineError::InvalidFundingAmount => "invalid funding amount",
            EngineError::RewardRateZero => "reward rate floors to zero",
            EngineError::NothingToClaim => "nothing to claim",
            EngineError::NothingToCompound => "nothing to compound",
            EngineError::InsufficientRewardBacking => "insufficient reward backing",
            EngineError::ClockWentBackwards => "clock went backwards",
            EngineError::GenesisCapExceeded => "genesis cumulative fresh-purchase cap exceeded",
            EngineError::WalletCapExceeded => "permanent per-wallet seed cap reached",
            EngineError::SkrDomainBound => "this .skr domain is bound to another wallet for Day 1",
            EngineError::FruitInsolvent => "fruit vault does not cover L + U + Z",
            EngineError::InsufficientFruitReserve => "fruit reserve Z is below the pool budget",
            EngineError::FruitCommitmentUnderflow => "fruit committed capacity U would go negative",
            EngineError::FruitLiabilityUnderflow => "fruit liabilities L would go negative",
            EngineError::ReserveWithdrawalExceedsReserve => "reserve withdrawal exceeds Z",
            EngineError::InvalidLedgerKind => "unknown fruit ledger kind",
            EngineError::InvalidFruitState => "unknown fruit pool or root status",
            EngineError::InvalidPoolWindow => "pool window must have t_end > t_start",
            EngineError::FruitPoolStarted => "pool cannot be committed at or after t_start",
            EngineError::FutureCheckpoint => "root checkpoint is in the future",
            EngineError::StaleRootVersion => "root version must strictly increase",
            EngineError::RootAlreadyPending => "pool already has a pending root",
            EngineError::RootExceedsCapacity => "root total exceeds scheduled capacity",
            EngineError::RootBelowFinalTotal => "root total is below the final total",
            EngineError::IncompleteCheckpoint => {
                "a complete root must checkpoint at or after t_end"
            }
            EngineError::RootNotPending => "root is not the pending root",
            EngineError::ChallengeWindowOpen => "challenge window has not elapsed",
            EngineError::ChallengeWindowClosed => "challenge window has elapsed",
            EngineError::NoFinalRoot => "pool has no final root",
            EngineError::LeafAboveWalletCap => "leaf exceeds the per-wallet cap",
            EngineError::PaidExceedsFinalTotal => "payment would exceed the final total",
            EngineError::FruitPoolClosed => "pool is closed",
            EngineError::FruitPoolNotEnded => "pool has not ended",
            EngineError::FruitPoolNotReconciled => "pool has no complete final root",
            EngineError::InvalidFruitGovernance => "invalid fruit governance configuration",
            EngineError::CompoundFeeAbovePurchaseFee => {
                "compound fee must not exceed the purchase fee"
            }
            EngineError::NoPendingFeeChange => "no fee change is scheduled",
            EngineError::FeeTimelockActive => "the 72-hour fee-change timelock has not elapsed",
            EngineError::InvalidForfeited => {
                "forfeited must be on a complete root and within released capacity"
            }
        };
        f.write_str(s)
    }
}

pub type EngineResult<T> = Result<T, EngineError>;
