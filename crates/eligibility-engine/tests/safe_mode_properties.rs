//! Adversarial and property coverage for Pass #5 (pricing + Safe Mode):
//! exact CR boundaries, consent field-mutation rejection, reference-window
//! manipulation rejection, the min-rule valuation bound under random
//! timelines, hysteresis bounds, overflow fail-closed behavior, same-slot
//! observation ordering, and stale/missing/wrong-basis data paths.
//! Fixture addresses, prices, thresholds and dwell values only.

mod common;

use common::*;
use eligibility_engine::*;
use proptest::prelude::*;

// ---------------------------------------------------------------- zones

#[test]
fn zone_boundary_table_is_exact_on_both_sides() {
    let b = SafeModeBoundaries::fixture_xsol();
    let cases: &[(u128, u128, RiskZone)] = &[
        (135, 100, RiskZone::Normal), // exact boundary
        (136, 100, RiskZone::Normal),
        (u128::MAX / 1_000, 1, RiskZone::Normal), // very large CR
        (270, 200, RiskZone::Normal),             // 1.35 unnormalized
        (134_999, 100_000, RiskZone::Warm),       // just below 1.35
        (120, 100, RiskZone::Warm),               // exact boundary
        (27, 20 + 1, RiskZone::Warm),             // 1.35 - 1/20
        (119, 100, RiskZone::Hot),
        (101, 100, RiskZone::Hot),
        (100, 100, RiskZone::Hot), // CR = 1 exactly: HOT, leverage singular
        (99, 100, RiskZone::Extreme),
        (1, 1_000_000_000_000, RiskZone::Extreme),
    ];
    for (num, den, want) in cases {
        assert_eq!(
            zone_for_cr(Some(Ratio::new(*num, *den)), &b).unwrap(),
            *want,
            "{num}/{den}"
        );
    }
    // Stale/missing feed is UNKNOWN, never EXTREME.
    assert_eq!(zone_for_cr(None, &b).unwrap(), RiskZone::Unknown);
    assert_eq!(
        zone_for_cr(Some(Ratio::new(1, 0)), &b).unwrap(),
        RiskZone::Unknown,
        "den 0 is not a number"
    );
    // Leverage exists exactly when CR > 1, at every boundary.
    for (num, den, has) in [
        (135u128, 100u128, true),
        (120, 100, true),
        (100, 100, false),
        (99, 100, false),
    ] {
        assert_eq!(
            leverage_for_cr(Ratio::new(num, den)).unwrap().is_some(),
            has,
            "{num}/{den}"
        );
    }
}

// -------------------------------------------------------------- consent

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Mutating exactly one bound field of an otherwise valid consent always
    /// yields the matching rejection — never acceptance.
    #[test]
    fn any_single_consent_field_mutation_is_rejected(
        nonce in 1..u64::MAX,
        epoch in 0..u64::MAX,
        version in 0..u64::MAX,
        amount in 1..1_000_000u128,
        mutated_field in 0..9usize,
    ) {
        let request = ConsentRequest {
            wallet: A,
            asset: AssetKind::XSol,
            action: ConsentAction::Enter,
            amount_lamports: amount,
            chain: ChainId::FIXTURE,
            epoch,
            risk_version: version,
            now: 5_000,
        };
        let mut consent = consent::fixture_consent_for(&request, nonce, 6_000);
        consent.signature = [7u8; 64];
        match mutated_field {
            0 => consent.wallet = B,
            1 => consent.asset = AssetKind::Skr,
            2 => consent.action = ConsentAction::Increase,
            3 => consent.amount_lamports = amount - 1,
            4 => consent.chain = ChainId(ChainId::FIXTURE.0 + 1),
            5 => consent.epoch = epoch.wrapping_add(1),
            6 => consent.risk_version = version.wrapping_add(1),
            7 => consent.nonce = nonce.wrapping_add(1), // different nonce is a *different* consent: accepted
            _ => consent.expires_at = 5_000,            // expired at now
        }
        let mut ledger = ConsentLedger::new();
        let ok = ledger.validate(&request, Some(&consent), &AcceptAllVerifier).is_ok();
        if mutated_field == 7 {
            // A fresh nonce with every binding intact is legitimately valid.
            prop_assert!(ok);
        } else {
            prop_assert!(!ok, "mutation {mutated_field} must be rejected");
        }
    }
}

#[test]
fn wrong_direction_scope_and_expiry_edges() {
    let request = ConsentRequest {
        wallet: A,
        asset: AssetKind::XSol,
        action: ConsentAction::Enter,
        amount_lamports: 1_000,
        chain: ChainId::FIXTURE,
        epoch: 7,
        risk_version: 3,
        now: 1_000,
    };
    // Wider scope than requested: fine.
    let mut wide = consent::fixture_consent_for(&request, 1, 1_100);
    wide.amount_lamports = 5_000;
    wide.signature = [7u8; 64];
    assert!(ConsentLedger::new()
        .validate(&request, Some(&wide), &AcceptAllVerifier)
        .is_ok());
    // Expiry is strict: valid while now < expiry.
    let mut edge = consent::fixture_consent_for(&request, 2, 1_001);
    edge.signature = [7u8; 64];
    assert!(ConsentLedger::new()
        .validate(&request, Some(&edge), &AcceptAllVerifier)
        .is_ok());
    let mut edge = consent::fixture_consent_for(&request, 3, 1_000);
    edge.signature = [7u8; 64];
    assert_eq!(
        ConsentLedger::new().validate(&request, Some(&edge), &AcceptAllVerifier),
        Err(ConsentError::Expired)
    );
}

// ------------------------------------------------- reference windows

fn any_validation(min_slots: u64, spread_den: u128) -> ReferenceValidation {
    ReferenceValidation {
        min_distinct_slots: min_slots,
        min_window_seconds: 0,
        max_spread: Ratio::new(1, spread_den),
        max_confidence: Ratio::new(1, 1),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Any window living inside one block is rejected for any calibration
    /// that requires at least two distinct slots — no matter the prices.
    #[test]
    fn single_block_windows_never_commit(
        slot in 1..u64::MAX,
        prices in prop::collection::vec((1u128..1_000_000, 1u128..10), 1..8),
        spread_den in 1..1_000u128,
    ) {
        let samples: Vec<PriceSample> = prices
            .iter()
            .enumerate()
            .map(|(i, (num, den))| PriceSample {
                slot,
                unix_timestamp: 100 + i as i64,
                lamports_num: *num,
                units_den: *den,
                confidence: None,
            })
            .collect();
        let v = any_validation(2, spread_den);
        prop_assert!(commit_reference(&samples, 10_000, &v, PriceBasis::Reference).is_err());
    }

    /// A window whose spread exceeds the calibrated bound is rejected,
    /// however many blocks it spans.
    #[test]
    fn wide_spread_windows_never_commit(
        lo in 1..10_000u128,
        gap in 100..10_000u128,
    ) {
        let hi = lo + gap;
        let samples = [
            PriceSample { slot: 1, unix_timestamp: 0, lamports_num: lo, units_den: 1, confidence: None },
            PriceSample { slot: 2, unix_timestamp: 60, lamports_num: hi, units_den: 1, confidence: None },
            PriceSample { slot: 3, unix_timestamp: 120, lamports_num: lo, units_den: 1, confidence: None },
        ];
        // spread = gap/lo; require it above 1/10 with a bound of 1/1000.
        prop_assume!(gap * 1_000 > lo * 10);
        let v = any_validation(3, 1_000);
        prop_assert!(commit_reference(&samples, 180, &v, PriceBasis::Reference).is_err());
    }
}

// ------------------------------------------------ min-rule timelines

#[derive(Clone, Debug)]
enum PriceOp {
    Mint {
        to: usize,
        amount: u64,
    },
    Send {
        from: usize,
        to: usize,
        amount: u64,
    },
    /// Verified observation, 1..=3 lamports per unit.
    Observe {
        num: u128,
    },
}

fn price_op() -> impl Strategy<Value = PriceOp> {
    prop_oneof![
        (0..3usize, 1..5_000u64).prop_map(|(to, amount)| PriceOp::Mint { to, amount }),
        (0..3usize, 0..3usize, 1..5_000u64).prop_map(|(from, to, amount)| PriceOp::Send {
            from,
            to,
            amount
        }),
        (1u128..=3u128).prop_map(|num| PriceOp::Observe { num }),
    ]
}

// Random in-epoch timelines (epoch starts at 100, reference price 2). The
// min rule caps every addition at `min(2, observation)`, so a verified
// wallet's gross never exceeds `units * reference`, and no unit is ever
// counted for two owners.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn gross_never_exceeds_units_times_reference(
        ops in prop::collection::vec((price_op(), 1..200i64), 1..40),
    ) {
        const SUPPLY_SEED: u64 = 10_000;
        const EPOCH_START: i64 = 100;
        let cfg = config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A]);
        let refs = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), EPOCH_START));
        let wallets = [A, B, C];

        let mut t = Tl::new();
        t.slot(1, 50)
            .init(A, SKR_MINT)
            .init(B, SKR_MINT)
            .init(C, SKR_MINT)
            .mint(A, SKR_MINT, SUPPLY_SEED)
            .pinned(&[LENDER_A]);
        for w in wallets {
            t.register(w, AssetKind::Skr).no_debt(w, &[LENDER_A]);
        }
        let mut shadow = Ledger::default();
        for e in &t.events {
            shadow.apply(&e.kind).unwrap();
        }
        let (mut slot, mut ts) = (1u64, 50i64);
        for (op, step) in &ops {
            slot += 1;
            ts += step;
            t.slot(slot, ts.max(EPOCH_START + 1));
            match op {
                PriceOp::Observe { num } => {
                    t.observe(SKR_MINT, price(*num, 1));
                }
                PriceOp::Mint { to, amount } => {
                    let k = EventKind::MintTo { account: ta(wallets[*to], SKR_MINT), amount: *amount };
                    if shadow.apply(&k).is_ok() {
                        t.ev(k);
                    }
                }
                PriceOp::Send { from, to, amount } => {
                    let k = EventKind::Transfer {
                        source: ta(wallets[*from], SKR_MINT),
                        destination: ta(wallets[*to], SKR_MINT),
                        amount: *amount,
                        authority: Authority::Owner,
                    };
                    if shadow.apply(&k).is_ok() {
                        t.ev(k);
                    }
                }
            }
        }
        t.slot(slot + 1, ts + 100).close();
        let r = run_with(&cfg, &refs, &t, ts + 100);

        // Truthful history: nothing unresolved.
        prop_assert!(r.pool.iter().all(|p| p.status != IntervalStatus::Unresolved));
        // The generator mints new units, so the conservation bound is the
        // FINAL supply (minting only grows it; segments never see more).
        let final_supply: u128 = shadow.supplies.values().map(|s| *s as u128).sum();
        let mut total_unit_seconds: u128 = 0;
        for s in r.segments.iter().filter(|s| s.status == IntervalStatus::Verified) {
            let mut units_now: u128 = 0;
            for e in s.wallets.values() {
                if let Eligibility::Verified { units, gross_lamports, .. } = *e {
                    // The min rule: never above the reference price.
                    prop_assert!(
                        gross_lamports <= units as u128 * 2,
                        "gross {gross_lamports} > {units} x 2"
                    );
                    units_now += units as u128;
                }
            }
            prop_assert!(units_now <= final_supply);
            total_unit_seconds += units_now * (s.end - s.start) as u128;
        }
        prop_assert!(total_unit_seconds <= final_supply * (ts + 100 - 50) as u128);
    }
}

// ----------------------------------------------------------- hysteresis

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Dwell unset: at most one reset, never a release, no auto-recovery —
    /// under any observation sequence.
    #[test]
    fn unset_dwell_never_releases(
        seq in prop::collection::vec((any::<bool>(), 0..500i64), 1..60),
    ) {
        let mut tr = ExtremeTracker::new(None);
        let mut now = 0i64;
        for (extreme, gap) in &seq {
            now += gap;
            tr.observe(
                if *extreme { ExtremeCondition::Extreme } else { ExtremeCondition::Healthy },
                now,
            );
        }
        prop_assert!(tr.resets() <= 1);
        prop_assert_eq!(tr.releases(), 0);
        prop_assert!(!tr.safety_release_enabled());
    }

    /// Dwell set: releases never exceed resets, and an oscillation whose
    /// gaps are all shorter than the dwell produces no churn.
    #[test]
    fn releases_never_exceed_resets(
        seq in prop::collection::vec((any::<bool>(), 0..300i64), 1..60),
    ) {
        let dwell = 300u64;
        let mut tr = ExtremeTracker::new(Some(dwell));
        let mut now = 0i64;
        for (extreme, gap) in &seq {
            now += gap;
            tr.observe(
                if *extreme { ExtremeCondition::Extreme } else { ExtremeCondition::Healthy },
                now,
            );
            prop_assert!(tr.releases() <= tr.resets());
        }
    }

    #[test]
    fn short_gap_oscillation_produces_no_churn(
        seq in prop::collection::vec(any::<bool>(), 1..60),
    ) {
        let mut tr = ExtremeTracker::new(Some(300));
        let mut now = 0i64;
        for extreme in &seq {
            now += 10;
            tr.observe(
                if *extreme { ExtremeCondition::Extreme } else { ExtremeCondition::Healthy },
                now,
            );
        }
        prop_assert!(tr.resets() <= 1);
        prop_assert_eq!(tr.releases(), 0);
    }
}

// -------------------------------------------------------------- overflow

#[test]
fn overflow_fails_closed_everywhere() {
    // Zone classification with extreme rationals: an error, never a panic
    // or a silent zone.
    let b = SafeModeBoundaries::fixture_xsol();
    let huge = Ratio::new(u128::MAX, 1);
    let other = Ratio::new(u128::MAX - 1, u128::MAX);
    assert_eq!(
        zone_for_cr(Some(huge), &b).and_then(|_| huge.gt(&other)),
        Err(EligibilityError::MathOverflow)
    );
    // Leverage of a huge CR stays finite (and >= 1).
    let l = leverage_for_cr(huge).unwrap().unwrap();
    assert!(l.ge(&Ratio::ONE).unwrap());
    // Valuation overflow is an error, never a wrap.
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, u64::MAX)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::Skr)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 150).close();
    let cfg = config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A]);
    let refs = EpochReferences::new().with(ref_commit_at(
        SKR_MINT,
        UnitPrice {
            lamports_num: u128::MAX,
            units_den: 1,
            basis: PriceBasis::Reference,
        },
        100,
    ));
    assert_eq!(
        replay(&cfg, &refs, t.events, 150).unwrap_err(),
        EligibilityError::MathOverflow
    );
}

// ------------------------------------------- same-slot data ordering

#[test]
fn same_slot_observations_follow_event_order_deterministically() {
    let cfg = config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A]);
    let refs = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 100));
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .init(B, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .pinned(&[LENDER_A])
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    // One slot, three transactions: obs 3, transfer to B, obs 1. B's units
    // are an addition priced with the observation in force *at its event*:
    // min(2, 3) = 2.
    t.slot(2, 200)
        .tx()
        .observe(SKR_MINT, price(3, 1))
        .tx()
        .send(A, B, SKR_MINT, 400)
        .tx()
        .observe(SKR_MINT, price(1, 1));
    t.slot(3, 300).close();
    let r = run_with(&cfg, &refs, &t, 300);
    match x_at(&r, &B, 250) {
        Some(Eligibility::Verified {
            units,
            gross_lamports,
            ..
        }) => {
            assert_eq!((units, gross_lamports), (400, 800));
        }
        o => panic!("{o:?}"),
    }
    // A later addition (after the second observation) uses the newer one.
    let mut t2 = t;
    t2.slot(4, 400).mint(B, SKR_MINT, 100);
    t2.slot(5, 500).close();
    let r2 = run_with(&cfg, &refs, &t2, 500);
    match x_at(&r2, &B, 450) {
        Some(Eligibility::Verified {
            units,
            gross_lamports,
            ..
        }) => {
            assert_eq!((units, gross_lamports), (500, 400 * 2 + 100));
        }
        o => panic!("{o:?}"),
    }
}

// ------------------------------------- stale / missing / wrong basis

#[test]
fn wrong_basis_data_is_unusable_and_fails_closed() {
    let cfg = config(vec![xsol_adapter()], &[LENDER_A]);
    let refs = EpochReferences::new().with(ref_commit_at(XSOL_MINT, nav(3, 1), 100));

    // A wrong-basis observation (reference-style price on an NAV asset, or
    // the leveraged notional) is not usable: additions stay unpriced.
    for basis in [PriceBasis::Reference, PriceBasis::UnderlyingNotional] {
        let mut t = Tl::new();
        t.slot(1, 50)
            .init(A, XSOL_MINT)
            .mint(A, XSOL_MINT, 1_000)
            .pinned(&[XSOL_PROGRAM, LENDER_A])
            .register(A, AssetKind::XSol)
            .no_debt(A, &[LENDER_A]);
        t.slot(2, 150).observe(
            XSOL_MINT,
            UnitPrice {
                lamports_num: 5,
                units_den: 1,
                basis,
            },
        );
        t.slot(3, 200).mint(A, XSOL_MINT, 100);
        t.slot(4, 300).close();
        let r = run_with(&cfg, &refs, &t, 300);
        assert_eq!(
            x_at(&r, &A, 250),
            Some(Eligibility::Pending(PendingReason::AdditionUnpriced)),
            "{basis:?}"
        );
    }

    // A wrong-basis emergency NAV is an unusable datum: recognition pauses
    // (never reprices at a wrong basis).
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, XSOL_MINT)
        .mint(A, XSOL_MINT, 1_000)
        .pinned(&[XSOL_PROGRAM, LENDER_A])
        .register(A, AssetKind::XSol)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 300).close();
    t.slot(3, 400).emergency_nav(
        XSOL_MINT,
        UnitPrice {
            lamports_num: 9,
            units_den: 1,
            basis: PriceBasis::UnderlyingNotional,
        },
    );
    t.slot(4, 600).close();
    let r = run_with(&cfg, &refs, &t, 600);
    assert_eq!(
        x_at(&r, &A, 550),
        Some(Eligibility::Pending(PendingReason::NavUnverified))
    );

    // A wrong-basis recovery segment keeps recognition paused too.
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, XSOL_MINT)
        .mint(A, XSOL_MINT, 1_000)
        .pinned(&[XSOL_PROGRAM, LENDER_A])
        .register(A, AssetKind::XSol)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 300).close();
    t.slot(3, 400).route_halt(XSOL_MINT);
    t.slot(4, 500).route_resumed(
        XSOL_MINT,
        UnitPrice {
            lamports_num: 4,
            units_den: 1,
            basis: PriceBasis::UnderlyingNotional,
        },
    );
    t.slot(5, 600).close();
    let r = run_with(&cfg, &refs, &t, 600);
    assert_eq!(
        x_at(&r, &A, 550),
        Some(Eligibility::Pending(PendingReason::NavUnverified))
    );
    // A correct-basis resume recovers.
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, XSOL_MINT)
        .mint(A, XSOL_MINT, 1_000)
        .pinned(&[XSOL_PROGRAM, LENDER_A])
        .register(A, AssetKind::XSol)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 300).close();
    t.slot(3, 400).route_halt(XSOL_MINT);
    t.slot(4, 500).route_resumed(XSOL_MINT, nav(2, 1));
    t.slot(5, 600).close();
    let r = run_with(&cfg, &refs, &t, 600);
    match x_at(&r, &A, 550) {
        Some(Eligibility::Verified { gross_lamports, .. }) => assert_eq!(gross_lamports, 2_000),
        o => panic!("{o:?}"),
    }
}
