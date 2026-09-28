//! Asset adapters: which mints form a selectable asset, which Token-2022
//! extensions the adapter has explicitly cleared, which accounts are
//! protocol custody (never attributed to a wallet), how stake beneficiary
//! positions are decoded, and which external program versions are pinned.
//!
//! Launch scope for Pass #4 is SKR and xSOL only. The other kinds exist so
//! the framework stays general, but configuring them is refused.

use std::collections::BTreeSet;

use crate::error::{EligibilityError, EligibilityResult};
use crate::types::Addr;
use crate::valuation::PriceBasis;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssetKind {
    Skr,
    XSol,
    // Deferred: framework-supported, not activatable in this pass.
    JitoSol,
    SpotJlp,
    XBtc,
    XHype,
    // Excluded by the model.
    LeveragedJlp,
}

impl AssetKind {
    /// Current launch candidates.
    pub const fn launch_candidate(self) -> bool {
        matches!(self, AssetKind::Skr | AssetKind::XSol)
    }

    /// Price basis the adapter admits for holdings of this asset.
    pub const fn required_basis(self) -> PriceBasis {
        match self {
            AssetKind::XSol => PriceBasis::OwnedTokenNav,
            _ => PriceBasis::Reference,
        }
    }
}

/// Token-2022 mint extensions that change ownership/transfer semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskExtension {
    PermanentDelegate,
    TransferHook,
    ConfidentialTransfer,
}

/// A mint's token program and its risk-relevant extensions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TokenProgram {
    SplToken,
    Token2022 { extensions: BTreeSet<RiskExtension> },
}

/// How a mint relates to the asset's underlying units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MintRole {
    /// Holds the underlying units directly.
    Underlying,
    /// A receipt for units already counted elsewhere (e.g. through a stake
    /// position): never counted, so one claim counts once.
    DuplicateReceipt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterMint {
    pub mint: Addr,
    pub role: MintRole,
    pub program: TokenProgram,
}

/// A pinned external program version (programdata hash or last-deploy slot
/// digest). A different observed version makes the dependent asset or
/// registry entry UNKNOWN from that event until re-verified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramPin {
    pub program: Addr,
    pub version: [u8; 32],
}

/// Stake beneficiary support.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StakeSupport {
    /// The asset has no stake representation.
    None,
    /// A stake representation exists but its live layout / beneficiary rules
    /// are not verified: any stake position makes the wallet PENDING.
    Unverified,
    /// Verified decoding: token accounts owned by `custody_owners` hold the
    /// staked units and are attributed only through beneficiary positions.
    Verified { custody_owners: BTreeSet<Addr> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterConfig {
    pub asset: AssetKind,
    pub mints: Vec<AdapterMint>,
    /// Token-2022 risk extensions this adapter has explicitly cleared.
    pub cleared_extensions: BTreeSet<RiskExtension>,
    pub stake: StakeSupport,
    pub pins: Vec<ProgramPin>,
}

impl AdapterConfig {
    /// SKR: wallet token accounts plus (when verified) staked beneficiary
    /// positions, one selected asset. Addresses come from configuration.
    pub fn skr(
        mint: Addr,
        program: TokenProgram,
        stake: StakeSupport,
        pins: Vec<ProgramPin>,
    ) -> Self {
        Self {
            asset: AssetKind::Skr,
            mints: vec![AdapterMint {
                mint,
                role: MintRole::Underlying,
                program,
            }],
            cleared_extensions: BTreeSet::new(),
            stake,
            pins,
        }
    }

    /// xSOL: owned token equity at owned-token NAV, never leveraged notional.
    pub fn xsol(mint: Addr, program: TokenProgram, pins: Vec<ProgramPin>) -> Self {
        Self {
            asset: AssetKind::XSol,
            mints: vec![AdapterMint {
                mint,
                role: MintRole::Underlying,
                program,
            }],
            cleared_extensions: BTreeSet::new(),
            stake: StakeSupport::None,
            pins,
        }
    }

    pub fn validate(&self) -> EligibilityResult<()> {
        if self.asset == AssetKind::LeveragedJlp {
            return Err(EligibilityError::AssetExcluded);
        }
        if !self.asset.launch_candidate() {
            return Err(EligibilityError::AssetNotEnabled);
        }
        if self.mints.is_empty() || !self.mints.iter().any(|m| m.role == MintRole::Underlying) {
            return Err(EligibilityError::InvalidAdapter);
        }
        let mut seen = BTreeSet::new();
        for m in &self.mints {
            if !seen.insert(m.mint) {
                return Err(EligibilityError::DuplicateMint);
            }
        }
        if let StakeSupport::Verified { custody_owners } = &self.stake {
            if custody_owners.is_empty() {
                return Err(EligibilityError::InvalidAdapter);
            }
        }
        Ok(())
    }

    /// Risk extensions on underlying mints that the adapter has not cleared.
    pub fn uncleared_extension(&self) -> Option<RiskExtension> {
        self.mints
            .iter()
            .filter(|m| m.role == MintRole::Underlying)
            .find_map(|m| match &m.program {
                TokenProgram::SplToken => None,
                TokenProgram::Token2022 { extensions } => extensions
                    .iter()
                    .find(|e| !self.cleared_extensions.contains(e))
                    .copied(),
            })
    }

    pub fn role_of(&self, mint: &Addr) -> Option<MintRole> {
        self.mints.iter().find(|m| &m.mint == mint).map(|m| m.role)
    }

    pub fn underlying_mint(&self) -> Addr {
        self.mints
            .iter()
            .find(|m| m.role == MintRole::Underlying)
            .map(|m| m.mint)
            .unwrap_or_default()
    }

    pub fn is_custody(&self, owner: &Addr) -> bool {
        matches!(&self.stake, StakeSupport::Verified { custody_owners } if custody_owners.contains(owner))
    }
}
