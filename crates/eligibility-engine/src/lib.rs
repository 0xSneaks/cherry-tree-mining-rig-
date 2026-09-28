//! # eligibility-engine
//!
//! Ownership, encumbrance, debt, reference pricing and Safe Mode for THE
//! CHERRY TREE Fruit model (architecture sections 5 and 9, canonical tests
//! 16–39). Off-chain logic for the indexer / settlement path; it has **no
//! Solana dependencies** and is not linked into the on-chain program.
//!
//! ```text
//! X_i(t) = max(0, sum_j A_ij(t) G_ij(t) - D_i(t))
//! ```
//!
//! Three outcomes per wallet and interval: VERIFIED (with X), INELIGIBLE
//! (known weight 0) and PENDING (unknown: later settlement must reserve).
//!
//! Pass #5 adds:
//! * reference-price commitment with a confidence/TWAP validation interface
//!   that rejects single-block manipulations ([`pricing`], test 31);
//! * FIFO lot valuation where post-reference additions and late
//!   registrations price at `min(reference, verified event price)`
//!   ([`lots`], test 30);
//! * WARM/HOT risk consent bound to wallet, asset, action/scope, chain,
//!   epoch, risk version, nonce and expiry ([`consent`], test 34);
//! * Safe Mode zones, CR = 1 leverage singularity, route availability
//!   separate from DEX quote success, and dwell-hysteresis for the EXTREME
//!   reset/release ([`safe_mode`], tests 35–39);
//! * timeline events for verified NAV resets, route halts and recoveries,
//! * prospective only — settled R and Q never change ([`replay`], tests
//!   33/36).
//!
//! Scope limits, stated honestly:
//! * Launch assets: SKR and xSOL only; other adapters are refused.
//! * No production addresses, registry entries, prices, thresholds or dwell
//!   durations are embedded (gate 14.2-D); every constant in tests is a
//!   labelled fixture.
//! * Debt outside the fixed registry is not detected.
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

pub mod adapter;
pub mod consent;
pub mod debt;
pub mod error;
pub mod ledger;
pub mod lots;
pub mod pricing;
pub mod rational;
pub mod replay;
pub mod safe_mode;
pub mod types;
pub mod valuation;

pub use adapter::{
    AdapterConfig, AdapterMint, AssetKind, MintRole, ProgramPin, RiskExtension, StakeSupport,
    TokenProgram,
};
pub use consent::{
    AcceptAllVerifier, ChainId, ConsentAction, ConsentError, ConsentLedger, ConsentRequest,
    ConsentVerifier, RiskConsent,
};
pub use debt::{DebtBook, DebtPending, DebtReading, DebtRegistry, RegistryEntry, WalletDebt};
pub use error::{EligibilityError, EligibilityResult};
pub use ledger::{Inconsistent, Ledger, StakePosition, TokenAccount};
pub use lots::{Lot, LotBasis, LotLedger};
pub use pricing::{
    commit_reference, min_price, EpochReferences, PriceSample, ReferenceCommit, ReferenceValidation,
};
pub use rational::Ratio;
pub use replay::{
    order_events, replay, Eligibility, EligibilityConfig, IneligibleReason, IntervalStatus,
    PendingReason, PoolInterval, ReplayReport, Segment, WalletAccrual,
};
pub use safe_mode::{
    authorize_entry, leverage_for_cr, zone_for_cr, EntryDecision, EntryDenial, ExtremeCondition,
    ExtremeTracker, QuoteStatus, RiskZone, RouteCheck, SafeModeBoundaries,
};
pub use types::{
    AccountSnapshot, Addr, Authority, DebtAmount, EncumbranceKind, Event, EventKey, EventKind,
    StakeSnapshot, SupplySnapshot,
};
pub use valuation::{PriceBasis, PriceKey, UnitPrice};

/// Product disclosure for the debt limitation (section 5, test 28). The app
/// mirrors this text verbatim (`app/src/lib/eligibility.ts`).
pub const NON_REGISTRY_DEBT_DISCLOSURE: &str = "Borrowed-funds check: Cherry nets only debt it can read in a fixed list of approved lending programs. It cannot detect borrowing in other programs, loans from other wallets, exchange credit or off-chain loans, so it does not prove that an asset was not bought with borrowed money.";

/// Product disclosure for Safe Mode risk consent (section 9). The app
/// mirrors this text verbatim (`app/src/lib/safeMode.ts`).
pub const SAFE_MODE_CONSENT_DISCLOSURE: &str = "Risk zones: entering or increasing exposure in WARM or HOT collateral states requires a signed risk acknowledgement covering your wallet, the asset, the action and amount, the chain, the epoch and the risk-disclosure version, single-use and expiring. A signature is consent to risk, not permission to move funds, and it can never unlock an operation whose required protocol route is unavailable.";
