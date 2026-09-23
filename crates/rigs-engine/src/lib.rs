//! # rigs-engine
//!
//! The money logic of THE CHERRY TREE, with **zero Solana dependencies**.
//!
//! Everything that decides how many lamports a user is owed lives here so it
//! can be exercised by an ordinary `cargo test` on the host target. The Anchor
//! program in `programs/sol-rigs` is a thin shell: it validates accounts, calls
//! into this engine, and moves lamports according to what the engine returns.
//!
//! ## Rules this crate enforces
//!
//! * Integer lamports only. There is no `f64` in this crate, by construction.
//! * Rounding always truncates in the vault's favour, never the user's.
//! * A rig never earns emission that predates it — see
//!   [`MinerPosition::settle`].
//! * Purchase lamports and reward lamports never mix: `buy_rigs` returns a
//!   quote paid by the *user*, and only [`RewardGlobals::fund`] increases
//!   reward backing.
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod accumulator;
pub mod error;
pub mod launch;
pub mod position;
pub mod pricing;
pub mod referral;
pub mod yield_curve;

pub use accumulator::{
    assert_fully_funded, RewardGlobals, RewardLedger, MAX_REWARD_DURATION_SECONDS,
    MAX_REWARD_RATE_LAMPORTS_PER_SECOND, PRECISION, WEIGHT_BASE, WEIGHT_REFERRED, ZERO_RIG_POLICY,
};
pub use error::{EngineError, EngineResult};
pub use launch::{
    access_phase, gate_purchase, purchase_allowed, seconds_to_next_phase, t0_after_success,
    AccessPhase, ProvenIdentity, PurchaseContext, PurchaseGate, ACCESS_WINDOW_SECONDS,
    NOT_ACTIVATED,
};
pub use position::{
    buy_rigs, claim, compound, estimated_output_per_second, CompoundOutcome, MinerPosition,
};
pub use pricing::{
    quote_purchase, rigs_affordable, validate_pricing, PurchaseQuote, BPS_DENOMINATOR,
    MAX_PURCHASE_FEE_BPS, MAX_RIGS_PER_PURCHASE, MAX_TOTAL_RIGS,
};
pub use referral::{referral_weight, ReferrerCheck};

pub use yield_curve::{
    apr_bps, apr_bps_weighted, current_phase, effective_rate_bps, next_threshold,
    project_compounding, required_annual_emission, NextThreshold, Projection, YieldInputs,
    YieldPhase, MAX_PROJECTION_STEPS, SECONDS_PER_YEAR,
};

/// Lamports per SOL. Display helper only.
pub const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
