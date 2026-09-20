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
