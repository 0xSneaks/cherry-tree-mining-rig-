//! Day-1 `.skr` binding rule — engine level (architecture §4, tests 12, 13, 15).
//!
//! The program reads the binding PDA and current ownership, then asks
//! `skr_day1_binding` what to do. Only consulted during `[T0, T0 + 24h)`.

use rigs_engine::{skr_day1_binding, EngineError, SkrBindAction};

const A: [u8; 32] = [1; 32];
const B: [u8; 32] = [2; 32];

#[test]
fn first_day1_use_by_the_owner_creates_the_binding() {
    assert_eq!(skr_day1_binding(None, A, true), Ok(SkrBindAction::Create));
}

#[test]
fn unbound_domain_without_ownership_opens_nothing() {
    assert_eq!(
        skr_day1_binding(None, A, false),
        Err(EngineError::AccessDenied)
    );
}

/// Test 12: D bound to A; B proving D is rejected.
#[test]
fn another_wallet_proving_a_bound_domain_is_rejected() {
    assert_eq!(
        skr_day1_binding(Some(A), B, false),
        Err(EngineError::SkrDomainBound)
    );
}

/// Test 13: D transferred A→B during Day 1. B now owns D and is still
/// rejected; A keeps its entitlement without owning D.
#[test]
fn transfer_during_day1_neither_moves_nor_revokes_the_binding() {
    assert_eq!(
        skr_day1_binding(Some(A), B, true),
        Err(EngineError::SkrDomainBound)
    );
    assert_eq!(
        skr_day1_binding(Some(A), A, false),
        Ok(SkrBindAction::Reuse)
    );
}

/// Test 15 (engine part): no input reassigns a binding — the only outcomes
/// for a bound domain are Reuse by its wallet or rejection.
#[test]
fn no_input_reassigns_a_binding() {
    for owns in [false, true] {
        assert_eq!(skr_day1_binding(Some(A), A, owns), Ok(SkrBindAction::Reuse));
        assert_eq!(
            skr_day1_binding(Some(A), B, owns),
            Err(EngineError::SkrDomainBound)
        );
    }
}
