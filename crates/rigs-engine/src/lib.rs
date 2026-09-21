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
pub mod position;
pub mod pricing;

pub use accumulator::{
    assert_fully_funded, RewardGlobals, RewardLedger, MAX_REWARD_DURATION_SECONDS,
    MAX_REWARD_RATE_LAMPORTS_PER_SECOND, PRECISION, ZERO_RIG_POLICY,
};
pub use error::{EngineError, EngineResult};
pub use position::{
    buy_rigs, claim, compound, estimated_output_per_second, CompoundOutcome, MinerPosition,
};
pub use pricing::{
    quote_purchase, rigs_affordable, validate_pricing, PurchaseQuote, BPS_DENOMINATOR,
    MAX_PURCHASE_FEE_BPS, MAX_RIGS_PER_PURCHASE, MAX_TOTAL_RIGS,
};

/// Lamports per SOL. Display helper only.
pub const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
