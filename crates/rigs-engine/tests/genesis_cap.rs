//! The cumulative per-wallet Genesis fresh-purchase cap — engine level.
//!
//! `genesis_cap_check` is the pure rule the program enforces in `buy_rigs`
//! before any state or money moves. The gated period is [T0, T0 + 24h);
//! from T0 + 24h the protocol is Public forever. Its companion facts live in the program's
//! own tests: `compound` never routes through this check (`programs/sol-rigs/
//! tests/genesis_cap.rs`), so compounding cannot consume allowance, and a new
//! Miner starts from a zero lifetime count.

use proptest::prelude::*;

use rigs_engine::{
    genesis_cap_check, validate_genesis_cap, AccessPhase, EngineError, ACCESS_WINDOW_SECONDS,
    GENESIS_CAP_RIGS, MAX_TOTAL_RIGS, MAX_WALLET_RIGS, NOT_ACTIVATED,
};

const T0: i64 = 1_790_000_000;
const DAY: i64 = 86_400;

fn check(
    launch_t0: i64,
    now: i64,
    lifetime: u64,
    requested: u64,
    cap: u64,
) -> Result<(), EngineError> {
    genesis_cap_check(launch_t0, now, lifetime, requested, cap)
}

// ------------------------------------------------------------- happy paths

#[test]
fn first_buy_below_cap_is_allowed() {
    assert_eq!(check(T0, T0, 0, 3, 5), Ok(()));
}

#[test]
fn equality_with_cap_is_allowed() {
    assert_eq!(check(T0, T0, 0, 5, 5), Ok(()));
    assert_eq!(check(T0, T0, 3, 2, 5), Ok(()));
}

#[test]
fn multiple_buys_may_reach_but_not_exceed_the_cap() {
    // 2 + 2 = 4 ≤ 5, then 4 + 1 = 5 ≤ 5, then 5 + 1 > 5.
    assert_eq!(check(T0, T0, 2, 2, 5), Ok(()));
    assert_eq!(check(T0, T0 + 60, 4, 1, 5), Ok(()));
    assert_eq!(
        check(T0, T0 + 120, 5, 1, 5),
        Err(EngineError::GenesisCapExceeded)
    );
}

// --------------------------------------------------------------- rejections

#[test]
fn one_rig_over_the_cap_is_rejected() {
    assert_eq!(check(T0, T0, 0, 6, 5), Err(EngineError::GenesisCapExceeded));
    assert_eq!(check(T0, T0, 4, 2, 5), Err(EngineError::GenesisCapExceeded));
}

#[test]
fn repeated_over_cap_buys_are_rejected_every_time() {
    for k in 1..=5 {
        assert_eq!(
            check(T0, T0 + k * 60, 5, k as u64, 5),
            Err(EngineError::GenesisCapExceeded),
            "attempt {k}"
        );
    }
}

// -------------------------------------------------------- founder activation

#[test]
fn the_founder_activation_counts_toward_the_cap() {
    // Pre-launch (T0 unset): this is exactly the activation purchase.
    assert_eq!(check(NOT_ACTIVATED, T0, 0, 5, 5), Ok(()), "at cap");
    assert_eq!(
        check(NOT_ACTIVATED, T0, 0, 6, 5),
        Err(EngineError::GenesisCapExceeded),
        "above cap"
    );
    assert_eq!(check(NOT_ACTIVATED, T0, 0, 4, 5), Ok(()), "below cap");
}

// ------------------------------------------------------------- phase edges

#[test]
fn cap_applies_through_the_whole_gated_period() {
    // Genesis window, last second — and every earlier second.
    assert_eq!(
        check(T0, T0 + DAY - 1, 5, 1, 5),
        Err(EngineError::GenesisCapExceeded)
    );
    // The last gated second, one second short of Public.
    assert_eq!(
        check(T0, T0 + ACCESS_WINDOW_SECONDS - 1, 5, 1, 5),
        Err(EngineError::GenesisCapExceeded)
    );
}

#[test]
fn public_removes_the_cap_at_exactly_24h() {
    let t_public = T0 + ACCESS_WINDOW_SECONDS;
    assert_eq!(AccessPhase::Public, rigs_engine::access_phase(T0, t_public));
    assert_eq!(check(T0, t_public, u64::MAX / 2, u64::MAX / 2, 1), Ok(()));
    assert_eq!(check(T0, t_public + 10 * 365 * DAY, 9, 99, 1), Ok(()));
}

// -------------------------------------------------------------- arithmetic

#[test]
fn overflow_is_an_error_never_a_wrap() {
    assert_eq!(
        check(T0, T0, u64::MAX, 1, u64::MAX),
        Err(EngineError::MathOverflow)
    );
}

// --------------------------------------------------------- cap validation

#[test]
fn only_the_production_cap_of_250_is_accepted() {
    assert_eq!(GENESIS_CAP_RIGS, 250);
    assert_eq!(GENESIS_CAP_RIGS, MAX_WALLET_RIGS);
    assert_eq!(validate_genesis_cap(250), Ok(()));
    for bad in [
        0,
        1,
        249,
        251,
        10_000,
        MAX_TOTAL_RIGS,
        MAX_TOTAL_RIGS + 1,
        u64::MAX,
    ] {
        assert_eq!(
            validate_genesis_cap(bad),
            Err(EngineError::InvalidConfig),
            "cap {bad} must be refused"
        );
    }
}

// -------------------------------------------------------------- properties

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    /// Before Public, the check passes exactly when the cumulative sum fits
    /// the cap (and never overflows), independent of where in the gated
    /// period the buy happens.
    #[test]
    fn gated_passes_iff_cumulative_fits(
        offset in 0i64..ACCESS_WINDOW_SECONDS,
        lifetime in 0u64..1_000,
        requested in 0u64..1_000,
        cap in 1u64..1_000,
    ) {
        let now = T0 + offset;
        let want = lifetime + requested <= cap;
        assert_eq!(
            check(T0, now, lifetime, requested, cap).is_ok(),
            want,
            "lifetime {lifetime} + requested {requested} vs cap {cap} at +{offset}s"
        );
    }

    /// At or after Public, no input is ever rejected.
    #[test]
    fn public_never_rejects(
        excess in 0i64..(365 * DAY),
        lifetime in 0u64..1_000,
        requested in 0u64..1_000,
        cap in 0u64..1_000,
    ) {
        prop_assert!(check(T0, T0 + ACCESS_WINDOW_SECONDS + excess, lifetime, requested, cap).is_ok());
    }
}
