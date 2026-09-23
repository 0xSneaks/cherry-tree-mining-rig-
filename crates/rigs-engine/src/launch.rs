//! Launch activation and access phases.
//!
//! One event flips THE CHERRY TREE from pre-launch to live: the first
//! successful purchase by the founder wallet, which must also prove on-chain
//! that it owns the founder `.skr` name at that moment. That purchase's chain
//! timestamp is T0. Every access phase is a pure function of T0 and the chain
//! clock — nothing else, and nobody, moves it.
//!
//! ```text
//! now < T0 (or T0 unset)      PreLaunch    founder activation only
//! T0 ≤ now < T0 + 24h         SkrGenesis   .skr holders
//! T0 + 24h ≤ now < T0 + 48h   SkrSnsEarly  .skr or SNS holders
//! T0 + 48h ≤ now              Public       everyone, permanently
//! ```

use crate::error::{EngineError, EngineResult};

/// Length of each gated window.
pub const ACCESS_WINDOW_SECONDS: i64 = 24 * 60 * 60;

/// `launch_t0` value meaning "not activated". T0 is a real chain timestamp,
/// so it is always positive once written.
pub const NOT_ACTIVATED: i64 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccessPhase {
    PreLaunch,
    SkrGenesis,
    SkrSnsEarly,
    Public,
}

impl AccessPhase {
    /// Stable code shared with the TypeScript port.
    pub const fn code(self) -> u8 {
        match self {
            AccessPhase::PreLaunch => 0,
            AccessPhase::SkrGenesis => 1,
            AccessPhase::SkrSnsEarly => 2,
            AccessPhase::Public => 3,
        }
    }
}

/// An identity the buyer proved on-chain in this instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenIdentity {
    None,
    /// Owns a `.skr` (AllDomains) name record.
    Skr,
    /// Owns a top-level SNS name record (SPL Name Service root).
    Sns,
}

/// The access phase at `now`, from T0 alone.
pub fn access_phase(launch_t0: i64, now: i64) -> AccessPhase {
    if launch_t0 <= NOT_ACTIVATED || now < launch_t0 {
        return AccessPhase::PreLaunch;
    }
    // now ≥ t0 > 0, so the subtraction cannot overflow.
    let since = now - launch_t0;
    if since < ACCESS_WINDOW_SECONDS {
        AccessPhase::SkrGenesis
    } else if since < 2 * ACCESS_WINDOW_SECONDS {
        AccessPhase::SkrSnsEarly
    } else {
        AccessPhase::Public
    }
}

/// Seconds until the next phase, or `None` once public (or pre-launch, where
/// the next phase has no time — it waits on the founder).
pub fn seconds_to_next_phase(launch_t0: i64, now: i64) -> Option<i64> {
    match access_phase(launch_t0, now) {
        AccessPhase::SkrGenesis => Some(launch_t0 + ACCESS_WINDOW_SECONDS - now),
        AccessPhase::SkrSnsEarly => Some(launch_t0 + 2 * ACCESS_WINDOW_SECONDS - now),
        AccessPhase::PreLaunch | AccessPhase::Public => None,
    }
}

/// Whether a post-activation purchase with `identity` may proceed.
pub fn purchase_allowed(phase: AccessPhase, identity: ProvenIdentity) -> bool {
    match phase {
        AccessPhase::PreLaunch => false,
        AccessPhase::SkrGenesis => identity == ProvenIdentity::Skr,
        AccessPhase::SkrSnsEarly => {
            identity == ProvenIdentity::Skr || identity == ProvenIdentity::Sns
        }
        AccessPhase::Public => true,
    }
}

/// Everything the program established about a purchase before running it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PurchaseContext {
    pub launch_t0: i64,
    pub now: i64,
    /// Signer is the pinned founder wallet.
    pub is_founder: bool,
    /// Signer proved, in this instruction, that it owns the founder name.
    pub owns_founder_name: bool,
    /// `reward_rate > 0 && now < reward_period_finish`.
    pub schedule_active: bool,
    pub identity: ProvenIdentity,
}

/// What a purchase is allowed to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurchaseGate {
    /// The founder activation: on success, T0 := now.
    Activation,
    /// An ordinary purchase in an open phase.
    Open,
}

/// Decide a purchase. Activation is possible exactly while T0 is unset.
pub fn gate_purchase(ctx: &PurchaseContext) -> EngineResult<PurchaseGate> {
    if ctx.launch_t0 <= NOT_ACTIVATED {
        if !(ctx.is_founder && ctx.owns_founder_name) {
            return Err(EngineError::AccessDenied);
        }
        if !ctx.schedule_active {
            return Err(EngineError::ActivationUnfunded);
        }
        return Ok(PurchaseGate::Activation);
    }
    if purchase_allowed(access_phase(ctx.launch_t0, ctx.now), ctx.identity) {
        Ok(PurchaseGate::Open)
    } else {
        Err(EngineError::AccessDenied)
    }
}

/// The T0 a successful purchase leaves behind. Writes `now` only on
/// activation; otherwise returns the existing T0 unchanged — no path resets,
/// replaces or moves it.
pub fn t0_after_success(gate: PurchaseGate, launch_t0: i64, now: i64) -> i64 {
    match gate {
        PurchaseGate::Activation if launch_t0 <= NOT_ACTIVATED => now,
        _ => launch_t0,
    }
}
