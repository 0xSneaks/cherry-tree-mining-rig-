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
        };
        f.write_str(s)
    }
}

pub type EngineResult<T> = Result<T, EngineError>;
