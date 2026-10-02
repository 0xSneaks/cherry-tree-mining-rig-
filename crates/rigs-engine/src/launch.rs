//! Launch activation and access phases.
//!
//! One event flips THE CHERRY TREE from pre-launch to live: the first
//! successful purchase signed by the founder wallet pinned in config. That
//! purchase's chain timestamp is T0. Every access phase is a pure function of T0 and the chain
//! clock — nothing else, and nobody, moves it.
//!
//! Production V2 access:
//!
//! ```text
//! now < T0 (or T0 unset)      PreLaunch    founder activation only
//! T0 ≤ now < T0 + 24h         SkrGenesis   .skr holders
//! T0 + 24h ≤ now              Public       everyone, permanently
//! ```
//!
//! There is no `.sns` gated phase in Production V2: `.sns` identity remains
//! supported infrastructure (see `identity.rs`), but it has no role in launch
//! gating, and no second early-access window exists. [`AccessPhase::SkrSnsEarly`]
//! is retained solely to keep `code()` numbering stable for the TypeScript
//! port and historical V1 fixtures — no input maps to it under V2, which
//! `sns_early_phase_is_unreachable_under_v2` pins down.

use crate::error::{EngineError, EngineResult};
use crate::pricing::MAX_WALLET_RIGS;

/// The only Genesis cap `initialize` / `migrate_v2` accept. Equal to the
/// permanent holding cap: while both are 250 the holding cap dominates, and
/// this Day-1 cumulative check stays as a misconfiguration guard.
pub const GENESIS_CAP_RIGS: u64 = MAX_WALLET_RIGS;

/// Length of the `.skr`-only Genesis window. Public begins the second it ends.
pub const ACCESS_WINDOW_SECONDS: i64 = 24 * 60 * 60;

/// `launch_t0` value meaning "not activated". T0 is a real chain timestamp,
/// so it is always positive once written.
pub const NOT_ACTIVATED: i64 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AccessPhase {
    PreLaunch,
    SkrGenesis,
    /// V1's second gated window. **Unreachable under Production V2**: from
    /// T0 + 24h the protocol is Public forever. The variant survives only so
    /// `code()` numbering (shared with the TypeScript port) stays stable.
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
    /// Owns a `.skr` domain (AllDomains), directly or wrapped.
    Skr,
    /// Owns a `.sns` domain (SNS), directly or tokenized. Provable on-chain,
    /// but in Production V2 it grants no launch access — only Public lets a
    /// `.sns`-only wallet buy.
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
    } else {
        AccessPhase::Public
    }
}

/// Seconds until the next phase, or `None` once public (or pre-launch, where
/// the next phase has no time — it waits on the founder).
pub fn seconds_to_next_phase(launch_t0: i64, now: i64) -> Option<i64> {
    match access_phase(launch_t0, now) {
        AccessPhase::SkrGenesis => Some(launch_t0 + ACCESS_WINDOW_SECONDS - now),
        // `SkrSnsEarly` is unreachable under V2 (see `access_phase`); if it
        // ever were reached there would be no further gate to count down to.
        AccessPhase::SkrSnsEarly | AccessPhase::PreLaunch | AccessPhase::Public => None,
    }
}

/// Whether a post-activation purchase with `identity` may proceed.
pub fn purchase_allowed(phase: AccessPhase, identity: ProvenIdentity) -> bool {
    match phase {
        AccessPhase::PreLaunch => false,
        AccessPhase::SkrGenesis => identity == ProvenIdentity::Skr,
        // Unreachable under V2; the rule is kept for the stable phase table.
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
    /// Signer is the founder wallet pinned in config.
    pub is_founder: bool,
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
        if !ctx.is_founder {
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

/// Validate a configured Genesis cap: exactly [`GENESIS_CAP_RIGS`] (250).
/// Any other value — including one below the holding cap — is refused, so a
/// deployment cannot be initialised (or migrated) with a drifted cap.
pub fn validate_genesis_cap(cap: u64) -> EngineResult<()> {
    if cap != GENESIS_CAP_RIGS {
        return Err(EngineError::InvalidConfig);
    }
    Ok(())
}

/// What a Day-1 `.skr` purchase does with the domain's binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkrBindAction {
    /// First Day-1 use of the domain: bind it to this wallet.
    Create,
    /// The domain is already bound to this wallet: entitlement retained,
    /// whether or not the wallet still holds the domain.
    Reuse,
}

/// Day-1 rule: one `.skr` domain opens Day-1 access for one wallet.
///
/// * `bound_to` — the wallet the domain's binding records, if one exists.
/// * `owns_now` — the wallet proved current on-chain ownership of the domain.
///
/// A binding is permanent for Day 1: once `domain → A`, any other wallet is
/// [`EngineError::SkrDomainBound`] even if it now owns the domain (transfer or
/// wrapper move). `A` keeps its Day-1 entitlement (still subject to the
/// Genesis and holding caps). Without a binding, only a current owner may
/// create one; otherwise [`EngineError::AccessDenied`]. Outside
/// [`AccessPhase::SkrGenesis`] the program never consults this.
pub fn skr_day1_binding(
    bound_to: Option<[u8; 32]>,
    wallet: [u8; 32],
    owns_now: bool,
) -> EngineResult<SkrBindAction> {
    match bound_to {
        Some(w) if w == wallet => Ok(SkrBindAction::Reuse),
        Some(_) => Err(EngineError::SkrDomainBound),
        None if owns_now => Ok(SkrBindAction::Create),
        None => Err(EngineError::AccessDenied),
    }
}

/// The cumulative per-wallet Genesis fresh-purchase cap.
///
/// While the launch is gated — from the founder activation until Public at
/// T0 + 24h — every wallet may buy at most `cap` fresh rigs in total. The
/// check consumes the *launch phase* directly, so:
///
/// * the founder activation itself (PreLaunch) is capped;
/// * a fresh Miner starts from zero lifetime purchases;
/// * compounding never passes through here and never consumes allowance;
/// * from T0 + 24h (Public) the cap no longer applies at all.
///
/// Equality with the cap is allowed; exceeding it is [`EngineError::GenesisCapExceeded`].
pub fn genesis_cap_check(
    launch_t0: i64,
    now: i64,
    lifetime_rigs_bought: u64,
    requested_count: u64,
    cap: u64,
) -> EngineResult<()> {
    if access_phase(launch_t0, now) == AccessPhase::Public {
        return Ok(());
    }
    let cumulative = lifetime_rigs_bought
        .checked_add(requested_count)
        .ok_or(EngineError::MathOverflow)?;
    if cumulative > cap {
        return Err(EngineError::GenesisCapExceeded);
    }
    Ok(())
}
