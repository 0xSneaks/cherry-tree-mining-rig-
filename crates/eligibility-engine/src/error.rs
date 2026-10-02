//! Errors. Malformed *configuration* and broken *engine invariants* are
//! errors; inconsistent or missing *chain data* is not an error — it is
//! recorded as an unresolved interval (fail closed, section 10).

use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EligibilityError {
    /// Two events share one `(slot, tx, ix, inner)` key.
    DuplicateEventKey,
    /// Events in one slot carry different `unix_timestamp`s.
    SlotTimestampMismatch,
    /// A later slot carries an earlier `unix_timestamp`.
    NonMonotonicTimestamp,
    /// An asset outside the Pass #4 launch scope (SKR, xSOL) was activated.
    AssetNotEnabled,
    /// Leveraged / Multiply / queued / vault JLP: excluded by the model.
    AssetExcluded,
    /// Two adapters claim the same mint, or one adapter lists a mint twice.
    DuplicateMint,
    /// A registry entry is listed twice.
    DuplicateRegistryEntry,
    /// An adapter was configured with an unusable shape.
    InvalidAdapter,
    /// Checked arithmetic overflowed.
    MathOverflow,
    /// A reference-price sample window failed validation (single-block
    /// manipulation, short window, wide spread or wide oracle confidence).
    InvalidPriceWindow(&'static str),
    /// A reference price was committed at or after the epoch it claims to
    /// price; it is not a reference (the asset is inactive instead).
    LateReferenceCommit,
    /// A risk consent was rejected (details in `consent::ConsentError`).
    ConsentRejected,
    /// An internal accounting invariant failed. Always a bug.
    InvariantViolation(&'static str),
}

impl fmt::Display for EligibilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EligibilityError {}

pub type EligibilityResult<T> = Result<T, EligibilityError>;
