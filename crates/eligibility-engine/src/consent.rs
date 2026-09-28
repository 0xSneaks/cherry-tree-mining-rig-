//! WARM/HOT risk consent (architecture section 9, test 34).
//!
//! The two steps are a specific risk screen and an explicit wallet signature
//! bound to **wallet, asset, action/amount scope, chain, epoch, risk/config
//! version, nonce and expiry**. It is risk consent, not two-factor
//! authentication and not permission to move funds. Reconfirmation is
//! required when the action scope or risk version materially changes.
//!
//! The engine validates the binding of every field, single-use nonces and
//! expiry. The signature itself is verified by an injected verifier over the
//! canonical payload below (the engine never invents a signature scheme);
//! the product signs the same payload bytes with the wallet key.

use std::collections::BTreeSet;

use crate::adapter::AssetKind;
use crate::error::EligibilityError;
use crate::types::Addr;

/// The action a consent authorizes. Deliberately small: entering a risk zone
/// with new units, or increasing exposure there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConsentAction {
    Enter,
    Increase,
}

/// Chain identity the consent is bound to (mainnet-beta genesis hash in
/// production; a fixture u64 here — never a guessed production value).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChainId(pub u64);

impl ChainId {
    /// Fixture chain id used by tests. NOT a production chain identifier.
    pub const FIXTURE: ChainId = ChainId(0xF1);
}

/// A wallet's signed risk acknowledgement. Every field is part of the signed
/// payload; changing any field invalidates the signature *and* the binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiskConsent {
    pub wallet: Addr,
    pub asset: AssetKind,
    pub action: ConsentAction,
    /// Scope ceiling in lamports; a request above it is out of scope.
    pub amount_lamports: u128,
    pub chain: ChainId,
    pub epoch: u64,
    /// Risk/config version the screen showed when signing.
    pub risk_version: u64,
    /// Single-use per wallet.
    pub nonce: u64,
    /// Valid while `now < expiry` (unix seconds).
    pub expires_at: i64,
    /// Wallet signature over [`RiskConsent::payload`].
    pub signature: [u8; 64],
}

impl RiskConsent {
    /// Canonical, domain-separated payload bytes that must be signed. Every
    /// bound field serializes in; `payload` differing on any field is the
    /// binding proof (test 34).
    pub fn payload(&self) -> Vec<u8> {
        let action_code: u16 = match self.action {
            ConsentAction::Enter => 1,
            ConsentAction::Increase => 2,
        };
        let mut out = Vec::with_capacity(200);
        out.extend_from_slice(b"CHERRY.RISK_CONSENT.v1");
        out.extend_from_slice(&self.wallet.0);
        out.extend_from_slice(&(self.asset as u16).to_le_bytes());
        out.extend_from_slice(&action_code.to_le_bytes());
        out.extend_from_slice(&self.amount_lamports.to_le_bytes());
        out.extend_from_slice(&self.chain.0.to_le_bytes());
        out.extend_from_slice(&self.epoch.to_le_bytes());
        out.extend_from_slice(&self.risk_version.to_le_bytes());
        out.extend_from_slice(&self.nonce.to_le_bytes());
        out.extend_from_slice(&self.expires_at.to_le_bytes());
        out
    }
}

/// What the wallet is asking to do right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsentRequest {
    pub wallet: Addr,
    pub asset: AssetKind,
    pub action: ConsentAction,
    pub amount_lamports: u128,
    pub chain: ChainId,
    pub epoch: u64,
    pub risk_version: u64,
    /// Finalized "now" for the expiry check.
    pub now: i64,
}

/// Why a consent was refused. Every canonical rejection of test 34 (and the
/// pass scope list) has its own variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsentError {
    Missing,
    Expired,
    WrongWallet,
    WrongAsset,
    WrongAction,
    WrongScope,
    WrongChain,
    WrongEpoch,
    WrongVersion,
    ReplayedNonce,
    BadSignature,
}

impl ConsentError {
    pub const fn is_malformed(&self) -> bool {
        !matches!(self, ConsentError::Missing | ConsentError::Expired)
    }
}

/// Signature verification is the product's job (wallet SDK); the engine
/// takes it as a trait so tests can exercise wrong-signature rejection.
pub trait ConsentVerifier {
    fn verify(&self, wallet: &Addr, payload: &[u8], signature: &[u8; 64]) -> bool;
}

/// Verifier that accepts everything; used where the signature layer is out
/// of scope (unit seams) and in fixtures alongside explicitly signed consents.
pub struct AcceptAllVerifier;

impl ConsentVerifier for AcceptAllVerifier {
    fn verify(&self, _wallet: &Addr, _payload: &[u8], _signature: &[u8; 64]) -> bool {
        true
    }
}

/// Single-use nonces, per wallet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsentLedger {
    used: BTreeSet<(Addr, u64)>,
}

impl ConsentLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_used(&self, wallet: &Addr, nonce: u64) -> bool {
        self.used.contains(&(*wallet, nonce))
    }

    /// Validate a consent against a request. On success the nonce is burned:
    /// a second presentation is a replay (test 34).
    pub fn validate(
        &mut self,
        request: &ConsentRequest,
        consent: Option<&RiskConsent>,
        verifier: &dyn ConsentVerifier,
    ) -> Result<(), ConsentError> {
        let Some(c) = consent else {
            return Err(ConsentError::Missing);
        };
        if c.wallet != request.wallet {
            return Err(ConsentError::WrongWallet);
        }
        if c.asset != request.asset {
            return Err(ConsentError::WrongAsset);
        }
        if c.action != request.action {
            return Err(ConsentError::WrongAction);
        }
        if c.amount_lamports < request.amount_lamports {
            return Err(ConsentError::WrongScope);
        }
        if c.chain != request.chain {
            return Err(ConsentError::WrongChain);
        }
        if c.epoch != request.epoch {
            return Err(ConsentError::WrongEpoch);
        }
        if c.risk_version != request.risk_version {
            return Err(ConsentError::WrongVersion);
        }
        if request.now >= c.expires_at {
            return Err(ConsentError::Expired);
        }
        if self.is_used(&c.wallet, c.nonce) {
            return Err(ConsentError::ReplayedNonce);
        }
        if !verifier.verify(&c.wallet, &c.payload(), &c.signature) {
            return Err(ConsentError::BadSignature);
        }
        self.used.insert((c.wallet, c.nonce));
        Ok(())
    }
}

pub type ConsentResult<T> = Result<T, ConsentError>;

/// Convenience: a fixture consent matching a request, with a nonce the
/// caller controls. Engine-internal helper for tests; the product builds
/// real signatures.
pub fn fixture_consent_for(request: &ConsentRequest, nonce: u64, expires_at: i64) -> RiskConsent {
    RiskConsent {
        wallet: request.wallet,
        asset: request.asset,
        action: request.action,
        amount_lamports: request.amount_lamports,
        chain: request.chain,
        epoch: request.epoch,
        risk_version: request.risk_version,
        nonce,
        expires_at,
        signature: [0u8; 64],
    }
}

/// Map consent failures into the engine error space for callers that need
/// `EligibilityResult`.
impl From<ConsentError> for EligibilityError {
    fn from(_: ConsentError) -> Self {
        EligibilityError::ConsentRejected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(now: i64) -> ConsentRequest {
        ConsentRequest {
            wallet: Addr::fixture(0x41),
            asset: AssetKind::XSol,
            action: ConsentAction::Enter,
            amount_lamports: 1_000,
            chain: ChainId::FIXTURE,
            epoch: 7,
            risk_version: 3,
            now,
        }
    }

    fn valid(now: i64) -> RiskConsent {
        let mut c = fixture_consent_for(&request(now), 1, now + 600);
        c.signature = [9u8; 64];
        c
    }

    /// Fixture verifier accepting exactly `[9; 64]`.
    struct Nine;

    impl ConsentVerifier for Nine {
        fn verify(&self, _w: &Addr, _p: &[u8], s: &[u8; 64]) -> bool {
            *s == [9u8; 64]
        }
    }

    #[test]
    fn every_field_binds_the_payload() {
        let c = valid(500);
        let base_payload = c.payload();
        let mut variants: Vec<RiskConsent> = Vec::new();
        for k in 0..9 {
            let mut v = c.clone();
            match k {
                0 => v.wallet = Addr::fixture(0x99),
                1 => v.asset = AssetKind::Skr,
                2 => v.action = ConsentAction::Increase,
                3 => v.amount_lamports += 1,
                4 => v.chain = ChainId(0xEE),
                5 => v.epoch += 1,
                6 => v.risk_version += 1,
                7 => v.nonce += 1,
                _ => v.expires_at += 1,
            }
            variants.push(v);
        }
        for v in &variants {
            assert_ne!(v.payload(), base_payload);
        }
        assert!(base_payload.starts_with(b"CHERRY.RISK_CONSENT.v1"));
    }

    #[test]
    fn accept_and_replay() {
        let mut ledger = ConsentLedger::new();
        let req = request(500);
        assert_eq!(ledger.validate(&req, Some(&valid(500)), &Nine), Ok(()));
        assert_eq!(
            ledger.validate(&req, Some(&valid(500)), &Nine),
            Err(ConsentError::ReplayedNonce)
        );
    }

    #[test]
    fn reject_matrix() {
        let mut ledger = ConsentLedger::new();
        let now = 500;
        let req = request(now);
        assert_eq!(
            ledger.validate(&req, None, &Nine),
            Err(ConsentError::Missing)
        );
        let expired = {
            let mut c = valid(now);
            c.expires_at = now;
            c
        };
        assert_eq!(
            ledger.validate(&req, Some(&expired), &Nine),
            Err(ConsentError::Expired)
        );
        for (k, err) in [
            (0, ConsentError::WrongWallet),
            (1, ConsentError::WrongAsset),
            (2, ConsentError::WrongAction),
            (4, ConsentError::WrongChain),
            (5, ConsentError::WrongEpoch),
            (6, ConsentError::WrongVersion),
        ] {
            let mut c = valid(now);
            match k {
                0 => c.wallet = Addr::fixture(0x99),
                1 => c.asset = AssetKind::Skr,
                2 => c.action = ConsentAction::Increase,
                4 => c.chain = ChainId(0xEE),
                5 => c.epoch += 1,
                _ => c.risk_version += 1,
            }
            assert_eq!(ledger.validate(&req, Some(&c), &Nine), Err(err));
        }
        let scope = {
            let mut c = valid(now);
            c.amount_lamports = 999;
            c
        };
        assert_eq!(
            ledger.validate(&req, Some(&scope), &Nine),
            Err(ConsentError::WrongScope)
        );
        let bad_sig = {
            let mut c = valid(now);
            c.signature = [1u8; 64];
            c
        };
        assert_eq!(
            ledger.validate(&req, Some(&bad_sig), &Nine),
            Err(ConsentError::BadSignature)
        );
        // A higher-scope consent than requested is fine (it covers it).
        let wide = {
            let mut c = valid(now);
            c.amount_lamports = 2_000;
            c
        };
        assert_eq!(ledger.validate(&req, Some(&wide), &Nine), Ok(()));
    }
}
