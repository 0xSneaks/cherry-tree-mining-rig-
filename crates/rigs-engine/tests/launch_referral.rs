//! Launch activation, access phases, and referral weight — engine level.
//!
//! The program enforces these through the same functions; its own tests
//! (`programs/sol-rigs/tests/launch.rs`) drive them through real
//! instructions. Here every rule is checked exhaustively on the pure logic.

mod common;

use common::{World, SOL};
use proptest::prelude::*;
use rigs_engine::*;

const PRICE: u64 = 10_000_000;
const FEE: u16 = 300;
const DAY: i64 = 86_400;
const T0: i64 = 1_790_000_000;

fn ctx(launch_t0: i64, now: i64) -> PurchaseContext {
    PurchaseContext {
        launch_t0,
        now,
        is_founder: false,
        schedule_active: true,
        identity: ProvenIdentity::None,
    }
}

// ------------------------------------------------------------ phase clock

#[test]
fn phase_boundaries_are_exact() {
    use AccessPhase::*;
    let h = ACCESS_WINDOW_SECONDS;
    for (now, want) in [
        (T0 - 1, PreLaunch),
        (T0, SkrGenesis),
        (T0 + h - 1, SkrGenesis),
        (T0 + h, Public),
        (T0 + h + 1, Public),
        (T0 + 10 * 365 * DAY, Public),
        (i64::MAX, Public),
    ] {
        assert_eq!(access_phase(T0, now), want, "now = T0 {:+}", now - T0);
    }
    assert_eq!(ACCESS_WINDOW_SECONDS, 86_400, "the only gate is 24h");
}

/// Production V2 has no `.sns` phase: the variant survives for `code()`
/// stability alone, and nothing may ever map to it.
#[test]
fn sns_early_phase_is_unreachable_under_v2() {
    for offset in -10 * DAY..=10 * DAY {
        assert_ne!(access_phase(T0, T0 + offset), AccessPhase::SkrSnsEarly);
    }
    assert_ne!(access_phase(NOT_ACTIVATED, T0), AccessPhase::SkrSnsEarly);
    assert_ne!(access_phase(NOT_ACTIVATED, 0), AccessPhase::SkrSnsEarly);
}

#[test]
fn unactivated_is_always_prelaunch() {
    for now in [0, 1, T0, i64::MAX] {
        assert_eq!(access_phase(NOT_ACTIVATED, now), AccessPhase::PreLaunch);
    }
}

#[test]
fn countdown_reaches_zero_exactly_at_each_boundary() {
    assert_eq!(seconds_to_next_phase(T0, T0), Some(DAY));
    assert_eq!(seconds_to_next_phase(T0, T0 + DAY - 1), Some(1));
    assert_eq!(seconds_to_next_phase(T0, T0 + DAY), None);
    assert_eq!(seconds_to_next_phase(NOT_ACTIVATED, T0), None);
}

// ------------------------------------------------------- phase enforcement

#[test]
fn identity_matrix() {
    use AccessPhase::*;
    use ProvenIdentity::*;
    let table = [
        (PreLaunch, [false, false, false]),
        (SkrGenesis, [true, false, false]),
        (SkrSnsEarly, [true, true, false]),
        (Public, [true, true, true]),
    ];
    for (phase, [skr, sns, none]) in table {
        assert_eq!(purchase_allowed(phase, Skr), skr, "{phase:?} .skr");
        assert_eq!(purchase_allowed(phase, Sns), sns, "{phase:?} SNS");
        assert_eq!(purchase_allowed(phase, None), none, "{phase:?} ordinary");
    }
}

// --------------------------------------------------------------- activation

#[test]
fn only_the_founder_wallet_with_a_funded_schedule_activates() {
    // Any other wallet — even one proving a .skr or .sns domain.
    for identity in [
        ProvenIdentity::None,
        ProvenIdentity::Skr,
        ProvenIdentity::Sns,
    ] {
        let mut c = ctx(NOT_ACTIVATED, T0);
        c.identity = identity;
        assert_eq!(
            gate_purchase(&c),
            Err(EngineError::AccessDenied),
            "{identity:?}"
        );
    }

    // The founder wallet, but the reward schedule is not live.
    let mut c = ctx(NOT_ACTIVATED, T0);
    c.is_founder = true;
    c.schedule_active = false;
    assert_eq!(gate_purchase(&c), Err(EngineError::ActivationUnfunded));

    // The founder wallet with a funded schedule: this is the activation. It
    // needs no domain proof — the wallet is the authority.
    c.schedule_active = true;
    assert_eq!(gate_purchase(&c), Ok(PurchaseGate::Activation));
    assert_eq!(
        t0_after_success(PurchaseGate::Activation, NOT_ACTIVATED, T0),
        T0
    );
}

#[test]
fn t0_can_never_move_once_written() {
    // A second founder purchase is an ordinary purchase in an open phase.
    let mut c = ctx(T0, T0 + 5);
    c.is_founder = true;
    c.identity = ProvenIdentity::Skr;
    let gate = gate_purchase(&c).unwrap();
    assert_eq!(gate, PurchaseGate::Open);
    assert_eq!(t0_after_success(gate, T0, T0 + 5), T0);
    // Even a forged Activation gate cannot overwrite an existing T0.
    assert_eq!(t0_after_success(PurchaseGate::Activation, T0, T0 + 999), T0);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    /// Whatever sequence of purchases happens after T0, T0 is unchanged and
    /// the phase is a pure, monotone function of the clock.
    #[test]
    fn t0_is_immutable_and_phases_are_monotone(
        founder in any::<bool>(),
        id in 0u8..3,
        a in 0i64..(10 * DAY),
        b in 0i64..(10 * DAY),
    ) {
        let identity = [ProvenIdentity::None, ProvenIdentity::Skr, ProvenIdentity::Sns][id as usize];
        let now = T0 + a;
        let c = PurchaseContext { launch_t0: T0, now, is_founder: founder,
            schedule_active: true, identity };
        if let Ok(g) = gate_purchase(&c) {
            prop_assert_eq!(g, PurchaseGate::Open);
            prop_assert_eq!(t0_after_success(g, T0, now), T0);
        }
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        prop_assert!(access_phase(T0, T0 + lo) <= access_phase(T0, T0 + hi));
    }
}

// ----------------------------------------------------------------- referral

const ALICE: [u8; 32] = [1; 32];
const BOB: [u8; 32] = [2; 32];

fn planter(k: [u8; 32]) -> Option<ReferrerCheck> {
    Some(ReferrerCheck {
        referrer: k,
        referrer_is_planter: true,
    })
}

#[test]
fn referral_rules() {
    assert_eq!(referral_weight(BOB, true, None), Ok(WEIGHT_BASE));
    assert_eq!(
        referral_weight(BOB, true, planter(ALICE)),
        Ok(WEIGHT_REFERRED)
    );
    assert_eq!(
        referral_weight(BOB, true, planter(BOB)),
        Err(EngineError::SelfReferral)
    );
    assert_eq!(
        referral_weight(BOB, false, planter(ALICE)),
        Err(EngineError::ReferrerAlreadyBound),
        "no binding, replacement or stacking after the first purchase"
    );
    assert_eq!(
        referral_weight(
            BOB,
            true,
            Some(ReferrerCheck {
                referrer: ALICE,
                referrer_is_planter: false
            })
        ),
        Err(EngineError::InvalidReferrer)
    );
    assert_eq!(
        referral_weight(BOB, true, planter([0; 32])),
        Err(EngineError::InvalidReferrer)
    );
    assert_eq!(WEIGHT_REFERRED * 100 / WEIGHT_BASE, 105, "exactly 1.05×");
}

/// A referred rig earns exactly 1.05× a base rig over the same window, and
/// the two together never receive more than was emitted.
#[test]
fn referred_weight_redistributes_the_same_emission() {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(10 * SOL, 10 * DAY as u64).unwrap();
    w.miners[1].weight = WEIGHT_REFERRED;
    w.buy(0, 1_000).unwrap();
    w.buy(1, 1_000).unwrap();
    w.warp(5 * DAY);
    let (base, referred) = (w.earned(0) as u128, w.earned(1) as u128);
    // 1000 : 1050 of 5 SOL.
    assert!(referred * 100 / base == 105 || referred * 100 / base == 104);
    let emitted = w.globals.reward_rate as u128 * 5 * DAY as u128;
    assert!(base + referred <= emitted);
    assert!(
        emitted - (base + referred) < 10,
        "only flooring dust is unpaid"
    );
}

#[test]
fn displayed_referred_apr_is_what_the_referred_rig_accrues() {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(30 * SOL, 30 * DAY as u64).unwrap();
    w.miners[1].weight = WEIGHT_REFERRED;
    w.buy(0, 499).unwrap();
    w.buy(1, 1).unwrap();
    let i = YieldInputs::from_globals(&w.globals, PRICE, FEE, w.now);
    let base = apr_bps_weighted(&i, 0, WEIGHT_BASE).unwrap();
    let referred = apr_bps_weighted(&i, 0, WEIGHT_REFERRED).unwrap();
    assert!(referred >= base * 105 / 100 && referred <= base * 105 / 100 + 1);

    let before = w.earned(1);
    w.warp(10 * DAY);
    let accrued = (w.earned(1) - before) as u128;
    let cost = PRICE as u128 * (10_000 + FEE as u128) / 10_000;
    let predicted =
        referred as u128 * cost * (10 * DAY) as u128 / (SECONDS_PER_YEAR as u128 * 10_000);
    let slack = cost * (10 * DAY) as u128 / (SECONDS_PER_YEAR as u128 * 10_000) + 2;
    assert!(predicted <= accrued + 1 && accrued - predicted.min(accrued) <= slack);
}

/// Referral-heavy solvency: 500 planters, 90% referred, compounding and
/// claiming. The World harness asserts paid + owed ≤ funded after every
/// single action.
#[test]
fn referral_heavy_population_stays_solvent() {
    let n = 500;
    let mut w = World::new(n, PRICE, FEE);
    w.fund(50 * SOL, 30 * DAY as u64).unwrap();
    for i in 0..n {
        if i % 10 != 0 {
            w.miners[i].weight = WEIGHT_REFERRED;
        }
        w.buy(i, 1 + (i as u64 % 13)).unwrap();
    }
    for day in 0..30 {
        w.warp(DAY);
        for i in (0..n).step_by(7) {
            if day % 2 == 0 {
                let _ = w.compound(i);
            } else {
                let _ = w.claim(i);
            }
        }
    }
    let owed: u128 = (0..n).map(|i| w.earned(i) as u128).sum();
    let paid = w.ledger.total_reward_claimed as u128 + w.ledger.total_reward_compounded as u128;
    assert!(paid + owed <= w.ledger.total_reward_funded as u128);
    assert!(w.reward_vault as u128 >= owed);
}
