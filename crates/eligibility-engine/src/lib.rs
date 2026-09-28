//! # eligibility-engine
//!
//! Ownership, encumbrance and debt eligibility for THE CHERRY TREE Fruit
//! model (architecture section 5, canonical tests 16–29). Off-chain logic
//! for the indexer / settlement path; it has **no Solana dependencies** and
//! is not linked into the on-chain program.
//!
//! ```text
//! X_i(t) = max(0, sum_j A_ij(t) G_ij(t) - D_i(t))
//! ```
//!
//! Three outcomes per wallet and interval: VERIFIED (with X), INELIGIBLE
//! (known weight 0) and PENDING (unknown: later settlement must reserve).
//!
//! Scope limits, stated honestly:
//! * Launch assets: SKR and xSOL only; other adapters are refused.
//! * No production addresses, registry entries, prices or thresholds are
//!   embedded (gate 14.2-D); `Addr::fixture` values are test fixtures.
//! * Prices are an input interface; reference-price commitment is Pass #5.
//! * Debt outside the fixed registry is not detected.
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod adapter;
pub mod debt;
pub mod error;
pub mod ledger;
pub mod replay;
pub mod types;
pub mod valuation;

pub use adapter::{
    AdapterConfig, AdapterMint, AssetKind, MintRole, ProgramPin, RiskExtension, StakeSupport,
    TokenProgram,
};
pub use debt::{DebtBook, DebtPending, DebtReading, DebtRegistry, RegistryEntry, WalletDebt};
pub use error::{EligibilityError, EligibilityResult};
pub use ledger::{Inconsistent, Ledger, StakePosition, TokenAccount};
pub use replay::{
    order_events, replay, Eligibility, EligibilityConfig, IneligibleReason, IntervalStatus,
    PendingReason, PoolInterval, ReplayReport, Segment, WalletAccrual,
};
pub use types::{
    AccountSnapshot, Addr, Authority, DebtAmount, EncumbranceKind, Event, EventKey, EventKind,
    StakeSnapshot, SupplySnapshot,
};
pub use valuation::{PriceBasis, PriceKey, PriceTable, UnitPrice};

/// Product disclosure for the debt limitation (section 5, test 28). The app
/// mirrors this text verbatim (`app/src/lib/eligibility.ts`).
pub const NON_REGISTRY_DEBT_DISCLOSURE: &str = "Borrowed-funds check: Cherry nets only debt it can read in a fixed list of approved lending programs. It cannot detect borrowing in other programs, loans from other wallets, exchange credit or off-chain loans, so it does not prove that an asset was not bought with borrowed money.";
