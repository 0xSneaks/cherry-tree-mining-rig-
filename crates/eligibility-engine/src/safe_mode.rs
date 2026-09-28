//! Safe Mode (architecture section 9, tests 34–39).
//!
//! * xSOL risk maps to its verified pool collateral ratio. `CR` is a ratio;
//!   `leverage = CR/(CR-1)` exists only for CR > 1. CR = 1 is singular and
//!   CR < 1 has none: the interface returns `None`, and the product must
//!   never display a finite value there (test 38).
//! * Zone boundaries are **configuration inputs** ("use official zone
//!   boundaries after verifying the deployed version"); the values used in
//!   tests mirror the pass sheet (1.35 / 1.20 / 1.00) as labelled fixtures,
//!   not production constants.
//! * Risk severity and technical availability are separate concepts: a
//!   label alone does not block HOT entry; an unavailable operation cannot
//!   be bypassed with a signature (tests 34–35). DEX quote failure or thin
//!   pool depth alone is NOT route unavailability (test 39).
//! * Stale or missing data is UNKNOWN — never EXTREME and never zero NAV:
//!   new recognition pauses and existing unsettled capacity is reserved.
//! * Hysteresis: leaving EXTREME and re-entering it requires the condition
//!   to persist for a minimum dwell period; the dwell is a calibrated
//!   parameter, currently unset, and the EXTREME-triggered safety release
//!   cannot be enabled until it is set (test 37).

use crate::consent::{ConsentError, ConsentLedger, ConsentRequest, ConsentVerifier, RiskConsent};
use crate::error::EligibilityResult;
use crate::rational::Ratio;

/// Safe Mode zones. `Unknown` means missing or stale risk data — not
/// EXTREME, and not zero NAV.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskZone {
    Normal,
    Warm,
    Hot,
    Extreme,
    Unknown,
}

/// Zone boundary configuration (per deployed pool version). The pass sheet's
/// xSOL values — NORMAL `CR >= 1.35`, WARM `1.20 <= CR < 1.35`, HOT
/// `1.00 <= CR < 1.20`, EXTREME `CR < 1.00` or verified severe
/// protocol/route failure — appear here only as the fixture constructor the
/// tests use; production uses verified deployed boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SafeModeBoundaries {
    /// `CR >= warm_below` is NORMAL.
    pub warm_below: Ratio,
    /// `CR >= hot_below` (and `< warm_below`) is WARM.
    pub hot_below: Ratio,
}

impl SafeModeBoundaries {
    /// Fixture boundaries equal to the pass sheet's xSOL table. NOT
    /// production constants (production reads the deployed pool version).
    pub const fn fixture_xsol() -> Self {
        Self {
            warm_below: Ratio::new(135, 100),
            hot_below: Ratio::new(120, 100),
        }
    }

    pub fn validate(&self) -> EligibilityResult<()> {
        if self.hot_below.den == 0
            || self.warm_below.den == 0
            || !self.hot_below.lt(&self.warm_below)?
            || !self.hot_below.ge(&Ratio::ONE)?
        {
            return Err(crate::error::EligibilityError::InvalidAdapter);
        }
        Ok(())
    }
}

/// Classify one verified CR observation. `None` (missing/stale feed) maps to
/// UNKNOWN, never EXTREME.
pub fn zone_for_cr(
    cr: Option<Ratio>,
    boundaries: &SafeModeBoundaries,
) -> EligibilityResult<RiskZone> {
    let Some(cr) = cr else {
        return Ok(RiskZone::Unknown);
    };
    if !cr.is_valid() {
        return Ok(RiskZone::Unknown);
    }
    if cr.ge(&boundaries.warm_below)? {
        Ok(RiskZone::Normal)
    } else if cr.ge(&boundaries.hot_below)? {
        Ok(RiskZone::Warm)
    } else if cr.ge(&Ratio::ONE)? {
        Ok(RiskZone::Hot)
    } else {
        Ok(RiskZone::Extreme)
    }
}

/// `leverage = CR / (CR - 1)`, defined only for CR > 1. CR = 1 is singular;
/// CR < 1 is invalid: both return `None` and the product renders nothing
/// finite (test 38).
pub fn leverage_for_cr(cr: Ratio) -> EligibilityResult<Option<Ratio>> {
    cr.leverage()
}

// ------------------------------------------------------------------
// Route availability (tests 35, 39)
// ------------------------------------------------------------------

/// State of a DEX quote attempt for the requested route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteStatus {
    Ok,
    /// Quote failed or pool depth looked thin. On its own this is NOT route
    /// unavailability (section 9, test 39).
    Failed,
}

/// Route availability means the asset's documented protocol
/// mint/redeem/unstake/withdraw path is operational per that protocol's
/// on-chain state. Severity (zone) and availability are separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteCheck {
    /// The documented protocol path is halted per on-chain state.
    pub protocol_route_halted: bool,
    pub dex_quote: QuoteStatus,
}

impl RouteCheck {
    pub fn operational(quote: QuoteStatus) -> Self {
        Self {
            protocol_route_halted: false,
            dex_quote: quote,
        }
    }

    pub fn halted() -> Self {
        Self {
            protocol_route_halted: true,
            dex_quote: QuoteStatus::Ok,
        }
    }

    /// Only the protocol path decides availability. A DEX quote failure or
    /// thin depth alone never does (test 39).
    pub fn available(&self) -> bool {
        !self.protocol_route_halted
    }
}

// ------------------------------------------------------------------
// Entry decision (tests 34, 35)
// ------------------------------------------------------------------

/// Why an entry/increase was refused at the risk layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryDenial {
    /// Stale/missing risk data: pause new recognition, reserve unsettled.
    UnknownRiskData,
    /// Risk consent was missing or invalid (details in the inner error).
    Consent(ConsentError),
    /// Valid consent cannot bypass an unavailable requested route.
    RouteUnavailable,
}

/// Risk-layer entry decision. Ordinary eligibility, pricing and ownership
/// checks apply separately and are not weakened by a Permitted here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryDecision {
    /// NORMAL: permitted when admission checks pass; no consent needed.
    Permitted,
    /// WARM/HOT (and EXTREME where the operation genuinely remains
    /// available): permitted with valid risk consent and, for HOT and
    /// EXTREME, an operational requested route.
    PermittedWithConsent,
    Denied(EntryDenial),
}

/// Authorize a zone entry/increase. HOT is not automatically blocked: it
/// needs valid consent *and* an operational requested route (tests 34–35).
/// EXTREME follows the section 9 table — acknowledge risk if an operation
/// remains genuinely available; never submit an unavailable operation.
#[allow(clippy::too_many_arguments)]
pub fn authorize_entry(
    zone: RiskZone,
    request: &ConsentRequest,
    consent: Option<&RiskConsent>,
    route: &RouteCheck,
    boundaries: &SafeModeBoundaries,
    ledger: &mut ConsentLedger,
    verifier: &dyn ConsentVerifier,
) -> EligibilityResult<EntryDecision> {
    boundaries.validate()?;
    Ok(match zone {
        RiskZone::Normal => EntryDecision::Permitted,
        RiskZone::Unknown => EntryDecision::Denied(EntryDenial::UnknownRiskData),
        RiskZone::Warm => match ledger.validate(request, consent, verifier) {
            Ok(()) => EntryDecision::PermittedWithConsent,
            Err(e) => EntryDecision::Denied(EntryDenial::Consent(e)),
        },
        RiskZone::Hot | RiskZone::Extreme => {
            if !route.available() {
                // A signature cannot bypass an unavailable route.
                EntryDecision::Denied(EntryDenial::RouteUnavailable)
            } else {
                match ledger.validate(request, consent, verifier) {
                    Ok(()) => EntryDecision::PermittedWithConsent,
                    Err(e) => EntryDecision::Denied(EntryDenial::Consent(e)),
                }
            }
        }
    })
}

// ------------------------------------------------------------------
// EXTREME hysteresis (test 37)
// ------------------------------------------------------------------

/// What an observation says about the EXTREME condition. `Extreme` is CR
/// below 1 (rational, exact) **or** a verified severe protocol / required
/// route failure; `Healthy` is a verified healthy reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtremeCondition {
    Healthy,
    Extreme,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrackerState {
    /// Never entered EXTREME (or fully recovered with dwell confirmed).
    Healthy { has_been_extreme: bool },
    /// EXTREME since `since`, with an unconfirmed recovery candidate.
    Extreme {
        since: i64,
        recovery_candidate_since: Option<i64>,
    },
    /// EXTREME condition observed while Healthy after a recovery; entering
    /// again requires it to persist for the dwell period.
    ReentryCandidate { since: i64 },
}

/// Dwell-hysteresis tracker for the EXTREME reset/release.
///
/// * First entry into EXTREME is immediate: the reset is effective from the
///   first finalized slot at which the trigger is observable (section 9,
///   test 36) — dwell never delays it.
/// * *Leaving* EXTREME (recovery) and *re-entering* it both require the
///   condition to persist for the dwell period, so rapid CR oscillation
///   across 1.00 cannot produce repeated resets and releases (test 37).
/// * The dwell parameter is a calibrated input, currently unset. While it
///   is unset the tracker still enters EXTREME immediately but can never
///   auto-recover and the EXTREME-triggered lock safety release stays
///   disabled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtremeTracker {
    /// Dwell in seconds; `None` = unset (uncalibrated).
    dwell: Option<u64>,
    state: TrackerState,
    resets: u64,
    releases: u64,
}

impl ExtremeTracker {
    pub fn new(dwell: Option<u64>) -> Self {
        Self {
            dwell,
            state: TrackerState::Healthy {
                has_been_extreme: false,
            },
            resets: 0,
            releases: 0,
        }
    }

    /// The EXTREME-triggered lock safety release cannot be enabled while
    /// the dwell parameter is unset (section 9, test 37).
    pub fn safety_release_enabled(&self) -> bool {
        self.dwell.is_some()
    }

    pub fn resets(&self) -> u64 {
        self.resets
    }

    pub fn releases(&self) -> u64 {
        self.releases
    }

    pub fn is_extreme(&self) -> bool {
        matches!(self.state, TrackerState::Extreme { .. })
    }

    fn dwell_ok(&self, since: i64, now: i64) -> bool {
        match self.dwell {
            None => false,
            Some(d) => u64::try_from(now.saturating_sub(since))
                .map(|x| x >= d)
                .unwrap_or(false),
        }
    }

    /// Apply one verified observation at finalized time `now`.
    pub fn observe(&mut self, condition: ExtremeCondition, now: i64) {
        let dwell = self.dwell;
        let state = self.state;
        self.state = match (state, condition) {
            (
                TrackerState::Healthy {
                    has_been_extreme: false,
                },
                ExtremeCondition::Extreme,
            ) => {
                // First entry: immediate, effective from this slot.
                self.resets += 1;
                TrackerState::Extreme {
                    since: now,
                    recovery_candidate_since: None,
                }
            }
            (
                TrackerState::Healthy {
                    has_been_extreme: true,
                },
                ExtremeCondition::Extreme,
            ) => match dwell {
                // Re-entry requires the condition to persist for the dwell.
                Some(_) => TrackerState::ReentryCandidate { since: now },
                // Dwell unset: re-entry after an explicit recovery follows
                // the same persist rule, but with no dwell configured the
                // condition can never persist "long enough" — unless the
                // caller configures one, recovery is an explicit process,
                // so this arm is unreachable while dwell is None (recovery
                // below never leaves Extreme when dwell is unset).
                None => TrackerState::ReentryCandidate { since: now },
            },
            (TrackerState::Healthy { .. }, ExtremeCondition::Healthy) => state,
            (TrackerState::ReentryCandidate { since }, ExtremeCondition::Extreme) => {
                if self.dwell_ok(since, now) {
                    self.resets += 1;
                    TrackerState::Extreme {
                        since,
                        recovery_candidate_since: None,
                    }
                } else {
                    state
                }
            }
            (TrackerState::ReentryCandidate { .. }, ExtremeCondition::Healthy) => {
                TrackerState::Healthy {
                    has_been_extreme: true,
                }
            }
            (
                TrackerState::Extreme {
                    recovery_candidate_since,
                    ..
                },
                ExtremeCondition::Healthy,
            ) => {
                let candidate = recovery_candidate_since.unwrap_or(now);
                if self.dwell_ok(candidate, now) {
                    // Dwell-confirmed recovery: the safety release may run
                    // (it is only ever enabled with a dwell configured).
                    self.releases += 1;
                    TrackerState::Healthy {
                        has_been_extreme: true,
                    }
                } else {
                    TrackerState::Extreme {
                        since: match state {
                            TrackerState::Extreme { since, .. } => since,
                            _ => now,
                        },
                        recovery_candidate_since: Some(candidate),
                    }
                }
            }
            (
                TrackerState::Extreme {
                    since,
                    recovery_candidate_since: Some(_),
                },
                ExtremeCondition::Extreme,
            ) => {
                // Oscillation inside the dwell window: the recovery
                // candidate is cancelled and NO new reset is issued.
                TrackerState::Extreme {
                    since,
                    recovery_candidate_since: None,
                }
            }
            (TrackerState::Extreme { since, .. }, ExtremeCondition::Extreme) => {
                TrackerState::Extreme {
                    since,
                    recovery_candidate_since: None,
                }
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b() -> SafeModeBoundaries {
        SafeModeBoundaries::fixture_xsol()
    }

    #[test]
    fn zone_boundaries_are_exact() {
        let f = |n: u128, d: u128| Some(Ratio::new(n, d));
        assert_eq!(zone_for_cr(f(135, 100), &b()).unwrap(), RiskZone::Normal);
        assert_eq!(zone_for_cr(f(27, 20), &b()).unwrap(), RiskZone::Normal); // 1.35 unnormalized
        assert_eq!(zone_for_cr(f(136, 100), &b()).unwrap(), RiskZone::Normal);
        assert_eq!(zone_for_cr(f(134, 100), &b()).unwrap(), RiskZone::Warm);
        assert_eq!(zone_for_cr(f(120, 100), &b()).unwrap(), RiskZone::Warm);
        assert_eq!(zone_for_cr(f(119, 100), &b()).unwrap(), RiskZone::Hot);
        assert_eq!(zone_for_cr(f(100, 100), &b()).unwrap(), RiskZone::Hot);
        assert_eq!(zone_for_cr(f(99, 100), &b()).unwrap(), RiskZone::Extreme);
        assert_eq!(zone_for_cr(None, &b()).unwrap(), RiskZone::Unknown);
        assert_eq!(
            zone_for_cr(Some(Ratio::new(5, 0)), &b()).unwrap(),
            RiskZone::Unknown,
            "den 0 is not a number: UNKNOWN"
        );
    }

    #[test]
    fn cr_one_never_has_finite_leverage() {
        assert!(leverage_for_cr(Ratio::ONE).unwrap().is_none());
        assert!(leverage_for_cr(Ratio::new(999, 1000)).unwrap().is_none());
        let l = leverage_for_cr(Ratio::new(120, 100)).unwrap().unwrap();
        let six = Ratio::new(6, 1);
        assert!(l.le(&six).unwrap() && six.le(&l).unwrap());
        let l = leverage_for_cr(Ratio::new(135, 100)).unwrap().unwrap();
        let exact = Ratio::new(27, 7);
        assert!(l.le(&exact).unwrap() && exact.le(&l).unwrap());
    }

    #[test]
    fn dex_quote_failure_alone_is_not_unavailability() {
        let quote_failed = RouteCheck::operational(QuoteStatus::Failed);
        assert!(quote_failed.available());
        assert!(!RouteCheck::halted().available());
    }

    #[test]
    fn oscillation_across_one_produces_one_reset_and_no_release_without_dwell() {
        let mut t = ExtremeTracker::new(None);
        assert!(!t.safety_release_enabled());
        for i in 0..50 {
            let now = i * 10;
            t.observe(ExtremeCondition::Extreme, now);
            t.observe(ExtremeCondition::Healthy, now + 5);
        }
        assert_eq!(t.resets(), 1, "only the first entry resets");
        assert!(t.is_extreme(), "no auto-recovery while dwell is unset");
        assert_eq!(t.releases(), 0);
        assert!(!t.safety_release_enabled());
    }

    #[test]
    fn dwell_confirms_recovery_and_gates_reentry() {
        let mut t = ExtremeTracker::new(Some(300));
        assert!(t.safety_release_enabled());
        t.observe(ExtremeCondition::Extreme, 0);
        assert_eq!(t.resets(), 1);
        // Rapid oscillation: recoveries never last 300s, dips never last.
        for i in 1..20 {
            let now = i * 20;
            t.observe(ExtremeCondition::Healthy, now);
            t.observe(ExtremeCondition::Extreme, now + 10);
        }
        assert!(t.is_extreme());
        assert_eq!(t.resets(), 1);
        assert_eq!(t.releases(), 0);
        // A dwell-long healthy stretch confirms recovery and runs the release.
        t.observe(ExtremeCondition::Healthy, 10_000);
        t.observe(ExtremeCondition::Healthy, 10_301);
        assert!(!t.is_extreme());
        assert_eq!(t.releases(), 1);
        // A short dip does not re-enter; a dwell-long dip does.
        t.observe(ExtremeCondition::Extreme, 11_000);
        t.observe(ExtremeCondition::Healthy, 11_100);
        assert!(!t.is_extreme());
        assert_eq!(t.resets(), 1);
        t.observe(ExtremeCondition::Extreme, 12_000);
        t.observe(ExtremeCondition::Extreme, 12_301);
        assert!(t.is_extreme());
        assert_eq!(t.resets(), 2);
        assert_eq!(t.releases(), 1);
    }
}
