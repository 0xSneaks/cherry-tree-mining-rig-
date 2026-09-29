//! Anchor error codes, one-for-one with [`rigs_engine::EngineError`] plus the
//! account-level checks the engine cannot see.

use anchor_lang::prelude::*;
use rigs_engine::EngineError;

#[error_code]
pub enum RigsError {
    #[msg("Checked math overflowed or underflowed")]
    MathOverflow,
    #[msg("Invalid config: rig price must be non-zero and fee must be within the cap")]
    InvalidConfig,
    #[msg("Rig count must be between 1 and MAX_RIGS_PER_PURCHASE")]
    InvalidRigCount,
    #[msg("Global rig supply cap exceeded")]
    RigSupplyExceeded,
    #[msg("Reward duration must be non-zero and within the maximum window")]
    InvalidDuration,
    #[msg("Funding amount must be non-zero")]
    InvalidFundingAmount,
    #[msg(
        "Funding is too small for the requested duration; the per-second rate would floor to zero"
    )]
    RewardRateZero,
    #[msg("Nothing to claim")]
    NothingToClaim,
    #[msg("Ore bin cannot afford a whole rig")]
    NothingToCompound,
    #[msg("Reward vault does not back outstanding liabilities")]
    InsufficientRewardBacking,
    #[msg("Clock went backwards")]
    ClockWentBackwards,

    #[msg("Program is paused")]
    ProgramPaused,
    #[msg("Signer is not the authorized reward funder")]
    UnauthorizedFunder,
    #[msg("Signer is not the pending authority")]
    UnauthorizedPendingAuthority,
    #[msg("Miner position does not belong to the signer")]
    MinerOwnerMismatch,
    #[msg("Vault account is the wrong kind")]
    VaultKindMismatch,
    #[msg("Vault would drop below its rent-exempt reserve")]
    VaultRentReserve,
    #[msg("Fee vault holds less than the requested withdrawal")]
    InsufficientFeeBalance,
    #[msg("Withdrawal amount must be non-zero")]
    ZeroAmount,
    #[msg("Fee withdrawal destination is not the configured fee recipient")]
    FeeRecipientMismatch,
    // --- launch + referral (appended: existing codes are unchanged) ---
    #[msg("A wallet cannot refer itself")]
    SelfReferral,
    #[msg("The referrer is bound at the first purchase only and is never replaced")]
    ReferrerAlreadyBound,
    #[msg("Referrer is not an existing planter")]
    InvalidReferrer,
    #[msg("Buying is not open to this wallet in the current launch phase")]
    AccessDenied,
    #[msg("Founder activation requires an active, funded reward schedule")]
    ActivationUnfunded,
    #[msg("A referrer was named but its miner account was not supplied")]
    MissingReferrerAccount,
    // --- Production V2 (appended: existing codes are unchanged) ---
    #[msg("Genesis cumulative fresh-purchase cap exceeded")]
    GenesisCapExceeded,
    #[msg(
        "V1 -> V2 migration rejected: not an exact, unmigrated V1 Config signed by its authority"
    )]
    MigrationRejected,
    // --- Security hardening (appended: existing codes are unchanged) ---
    #[msg("Initialize must be signed by the program's upgrade authority")]
    UnauthorizedInitializer,
    // --- Hard cap + Day-1 .skr binding (appended: existing codes are unchanged) ---
    #[msg("Permanent per-wallet seed cap (250) reached")]
    WalletCapExceeded,
    #[msg("This .skr domain is bound to another wallet for Day-1 access")]
    SkrDomainBound,
    #[msg("Day-1 .skr purchase requires the domain's binding account")]
    SkrBindingRequired,
    #[msg("Account is not the .skr binding PDA for this domain")]
    SkrBindingInvalid,
    // --- Fruit ledger §10 / §10a (appended: existing codes are unchanged) ---
    #[msg("Fruit vault does not cover L + U + Z")]
    FruitInsolvent,
    #[msg("Fruit reserve Z is below the pool budget")]
    InsufficientFruitReserve,
    #[msg("Fruit committed capacity U would go negative")]
    FruitCommitmentUnderflow,
    #[msg("Fruit liabilities L would go negative")]
    FruitLiabilityUnderflow,
    #[msg("Reserve withdrawal exceeds uncommitted reserve Z")]
    ReserveWithdrawalExceedsReserve,
    #[msg("Unknown Fruit ledger kind")]
    InvalidLedgerKind,
    #[msg("Unknown Fruit pool or root status")]
    InvalidFruitState,
    #[msg("Pool window must have t_end > t_start")]
    InvalidPoolWindow,
    #[msg("A pool cannot be committed at or after its t_start")]
    FruitPoolStarted,
    #[msg("Root checkpoint is in the future")]
    FutureCheckpoint,
    #[msg("Root version must strictly increase")]
    StaleRootVersion,
    #[msg("Pool already has a Pending root")]
    RootAlreadyPending,
    #[msg("Root total exceeds the pool's scheduled capacity")]
    RootExceedsCapacity,
    #[msg("Root total is below the pool's final total")]
    RootBelowFinalTotal,
    #[msg("A complete root must checkpoint at or after t_end")]
    IncompleteCheckpoint,
    #[msg("Root is not the pool's Pending root")]
    RootNotPending,
    #[msg("Challenge window has not elapsed")]
    ChallengeWindowOpen,
    #[msg("Challenge window has elapsed")]
    ChallengeWindowClosed,
    #[msg("Pool has no Final root")]
    NoFinalRoot,
    #[msg("Claim exceeds the pool's per-wallet cap")]
    LeafAboveWalletCap,
    #[msg("Payment would exceed the pool's final total")]
    PaidExceedsFinalTotal,
    #[msg("Pool is closed")]
    FruitPoolClosed,
    #[msg("Pool has not ended")]
    FruitPoolNotEnded,
    #[msg("Pool has no complete Final root")]
    FruitPoolNotReconciled,
    #[msg("Invalid Fruit governance configuration")]
    InvalidFruitGovernance,
    #[msg("This Fruit ledger is paused")]
    FruitPaused,
    #[msg("Signer is not the Fruit admin")]
    UnauthorizedFruitAdmin,
    #[msg("Signer is not the Fruit root poster")]
    UnauthorizedRootPoster,
    #[msg("Signer is not a Fruit veto signer")]
    NotVetoSigner,
    #[msg("This veto signer already vetoed this root")]
    DuplicateVeto,
    #[msg("Terminal resolution needs a veto-signer quorum")]
    TerminalQuorumNotMet,
    #[msg("Merkle proof does not match the pool's Final root")]
    InvalidMerkleProof,
    #[msg("Merkle proof is too long")]
    FruitProofTooLong,
    #[msg("Fruit account does not belong to this ledger or pool")]
    FruitAccountMismatch,
    #[msg("Fruit account layout version is not supported")]
    FruitVersionMismatch,
    #[msg("Lock pools and lock forfeiture go only through the lock instructions")]
    LockPoolsNotImplemented,
    #[msg("Reserve withdrawal destination is not the configured reserve recipient")]
    ReserveRecipientMismatch,
    #[msg("Compound fee must not exceed the purchase fee")]
    CompoundFeeAbovePurchaseFee,
    #[msg("No fee change is scheduled")]
    NoPendingFeeChange,
    #[msg("The 72-hour fee-change timelock has not elapsed")]
    FeeTimelockActive,
    #[msg("Fee rates change only through schedule_fee_change")]
    FeeChangeRequiresTimelock,
    #[msg("Referred position: the referrer's referral ledger account is required")]
    MissingReferralLedger,
    #[msg("Referral ledger account is not the canonical PDA for this referrer")]
    ReferralLedgerInvalid,
    #[msg("No unpaid referral commission")]
    NothingToClaimCommission,
    #[msg("Fee vault does not cover reserved referral commissions")]
    CommissionReservationBroken,
    #[msg("Protocol Treasury destination must not be a program-owned account")]
    TreasuryDestinationInvalid,
    #[msg("forfeited must be on a complete root and within released capacity")]
    InvalidForfeited,
    #[msg("Only lock-ledger roots may carry forfeiture")]
    ForfeitOnlyOnLockLedger,
    // --- Lock Season (Pass #7 P7-B). Appended; existing codes unchanged. ---
    #[msg("Lock registration and cancellation are open only on [FRUIT_T0, t_s)")]
    LockRegistrationClosed,
    #[msg("Requested lock weight must be in (0, 2.5 SOL]")]
    InvalidLockRequest,
    #[msg("Lock weight, award input or H_lock out of range")]
    InvalidLockWeight,
    #[msg("Lock registration is cancelled")]
    LockRegistrationCancelled,
    #[msg("Lock allocation is in the wrong phase for this operation")]
    LockAllocationPhase,
    #[msg("Lock weights are posted and sealed only at or after t_s")]
    LockSeasonNotStarted,
    #[msg("Lock registration already weighed in this generation")]
    LockAlreadyWeighed,
    #[msg("Lock weight exceeds the requested weight")]
    LockWeightExceedsRequest,
    #[msg("Lock tally does not cover every active registration")]
    LockTallyIncomplete,
    #[msg("Lock registration not weighed in this generation")]
    LockNotWeighed,
    #[msg("Lock registration already awarded in this generation")]
    LockAlreadyAwarded,
    #[msg("Lock pool has no Final allocation")]
    LockAllocationNotFinal,
    #[msg("Lock tally failed revalidation")]
    LockTallyInvalid,
    #[msg("Lock safety release is disabled until the EXTREME dwell is set")]
    LockSafetyReleaseDisabled,
    #[msg("Launch T0 is not set; the lock Season has no FRUIT_T0")]
    LockSeasonUnscheduled,
    #[msg("Lock registration account is not this pool's registration")]
    LockRegistrationMismatch,
    #[msg("A lock registration appears twice in one batch")]
    DuplicateLockRegistration,
    #[msg("Lock batch weights and registration accounts do not match")]
    LockBatchMismatch,
}

impl From<EngineError> for RigsError {
    fn from(e: EngineError) -> Self {
        match e {
            EngineError::MathOverflow => RigsError::MathOverflow,
            EngineError::InvalidConfig => RigsError::InvalidConfig,
            EngineError::InvalidRigCount => RigsError::InvalidRigCount,
            EngineError::RigSupplyExceeded => RigsError::RigSupplyExceeded,
            EngineError::InvalidDuration => RigsError::InvalidDuration,
            EngineError::InvalidFundingAmount => RigsError::InvalidFundingAmount,
            EngineError::RewardRateZero => RigsError::RewardRateZero,
            EngineError::NothingToClaim => RigsError::NothingToClaim,
            EngineError::NothingToCompound => RigsError::NothingToCompound,
            EngineError::InsufficientRewardBacking => RigsError::InsufficientRewardBacking,
            EngineError::ClockWentBackwards => RigsError::ClockWentBackwards,
            EngineError::SelfReferral => RigsError::SelfReferral,
            EngineError::ReferrerAlreadyBound => RigsError::ReferrerAlreadyBound,
            EngineError::InvalidReferrer => RigsError::InvalidReferrer,
            EngineError::AccessDenied => RigsError::AccessDenied,
            EngineError::ActivationUnfunded => RigsError::ActivationUnfunded,
            EngineError::GenesisCapExceeded => RigsError::GenesisCapExceeded,
            EngineError::WalletCapExceeded => RigsError::WalletCapExceeded,
            EngineError::SkrDomainBound => RigsError::SkrDomainBound,
            EngineError::FruitInsolvent => RigsError::FruitInsolvent,
            EngineError::InsufficientFruitReserve => RigsError::InsufficientFruitReserve,
            EngineError::FruitCommitmentUnderflow => RigsError::FruitCommitmentUnderflow,
            EngineError::FruitLiabilityUnderflow => RigsError::FruitLiabilityUnderflow,
            EngineError::ReserveWithdrawalExceedsReserve => {
                RigsError::ReserveWithdrawalExceedsReserve
            }
            EngineError::InvalidLedgerKind => RigsError::InvalidLedgerKind,
            EngineError::InvalidFruitState => RigsError::InvalidFruitState,
            EngineError::InvalidPoolWindow => RigsError::InvalidPoolWindow,
            EngineError::FruitPoolStarted => RigsError::FruitPoolStarted,
            EngineError::FutureCheckpoint => RigsError::FutureCheckpoint,
            EngineError::StaleRootVersion => RigsError::StaleRootVersion,
            EngineError::RootAlreadyPending => RigsError::RootAlreadyPending,
            EngineError::RootExceedsCapacity => RigsError::RootExceedsCapacity,
            EngineError::RootBelowFinalTotal => RigsError::RootBelowFinalTotal,
            EngineError::IncompleteCheckpoint => RigsError::IncompleteCheckpoint,
            EngineError::RootNotPending => RigsError::RootNotPending,
            EngineError::ChallengeWindowOpen => RigsError::ChallengeWindowOpen,
            EngineError::ChallengeWindowClosed => RigsError::ChallengeWindowClosed,
            EngineError::NoFinalRoot => RigsError::NoFinalRoot,
            EngineError::LeafAboveWalletCap => RigsError::LeafAboveWalletCap,
            EngineError::PaidExceedsFinalTotal => RigsError::PaidExceedsFinalTotal,
            EngineError::FruitPoolClosed => RigsError::FruitPoolClosed,
            EngineError::FruitPoolNotEnded => RigsError::FruitPoolNotEnded,
            EngineError::FruitPoolNotReconciled => RigsError::FruitPoolNotReconciled,
            EngineError::InvalidFruitGovernance => RigsError::InvalidFruitGovernance,
            EngineError::CompoundFeeAbovePurchaseFee => RigsError::CompoundFeeAbovePurchaseFee,
            EngineError::NoPendingFeeChange => RigsError::NoPendingFeeChange,
            EngineError::FeeTimelockActive => RigsError::FeeTimelockActive,
            EngineError::InvalidForfeited => RigsError::InvalidForfeited,
            EngineError::LockRegistrationClosed => RigsError::LockRegistrationClosed,
            EngineError::InvalidLockRequest => RigsError::InvalidLockRequest,
            EngineError::InvalidLockWeight => RigsError::InvalidLockWeight,
            EngineError::LockRegistrationCancelled => RigsError::LockRegistrationCancelled,
            EngineError::LockAllocationPhase => RigsError::LockAllocationPhase,
            EngineError::LockSeasonNotStarted => RigsError::LockSeasonNotStarted,
            EngineError::LockAlreadyWeighed => RigsError::LockAlreadyWeighed,
            EngineError::LockWeightExceedsRequest => RigsError::LockWeightExceedsRequest,
            EngineError::LockTallyIncomplete => RigsError::LockTallyIncomplete,
            EngineError::LockNotWeighed => RigsError::LockNotWeighed,
            EngineError::LockAlreadyAwarded => RigsError::LockAlreadyAwarded,
            EngineError::LockAllocationNotFinal => RigsError::LockAllocationNotFinal,
            EngineError::LockTallyInvalid => RigsError::LockTallyInvalid,
            EngineError::LockSafetyReleaseDisabled => RigsError::LockSafetyReleaseDisabled,
        }
    }
}

/// Lift an engine result into an Anchor result.
pub trait IntoAnchor<T> {
    fn anchor(self) -> Result<T>;
}

impl<T> IntoAnchor<T> for core::result::Result<T, EngineError> {
    fn anchor(self) -> Result<T> {
        self.map_err(|e| error!(RigsError::from(e)))
    }
}
