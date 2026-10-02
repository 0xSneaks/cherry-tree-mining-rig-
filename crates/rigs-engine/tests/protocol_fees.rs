//! Protocol fee architecture, engine side (architecture §7a; tests 78–84,
//! 71 and the fee half of test 6).

use proptest::prelude::*;
use rigs_engine::*;

const P: u64 = 10_000_000;
const T0: i64 = 1_800_000_000;

// ------------------------------------------------------------ 78 / 80

#[test]
fn production_purchase_fee_is_exactly_ten_percent_and_capped() {
    for n in [1u64, 7, 100, 250] {
        let q = quote_purchase(n, P, 1_000).unwrap();
        assert_eq!(q.fee_lamports, n * P * 1_000 / 10_000);
        assert_eq!(q.fee_lamports * 10, q.capital_lamports);
    }
    assert_eq!(MAX_PURCHASE_FEE_BPS, 1_000);
    assert_eq!(validate_pricing(P, 1_001), Err(EngineError::InvalidConfig));
    assert_eq!(validate_fee_pair(1_001, 0), Err(EngineError::InvalidConfig));
    let mut s = FeeSchedule {
        purchase_fee_bps: 1_000,
        compound_fee_bps: 1_000,
        ..FeeSchedule::default()
    };
    assert_eq!(
        s.schedule(1_001, 1_000, T0),
        Err(EngineError::InvalidConfig)
    );
    assert_eq!(s.schedule(u16::MAX, 0, T0), Err(EngineError::InvalidConfig));
    assert!(!s.has_pending());
}

#[test]
fn compound_fee_above_purchase_fee_is_rejected() {
    assert_eq!(validate_fee_pair(1_000, 1_000), Ok(()));
    assert_eq!(validate_fee_pair(1_000, 800), Ok(()));
    assert_eq!(validate_fee_pair(0, 0), Ok(()));
    assert_eq!(
        validate_fee_pair(800, 801),
        Err(EngineError::CompoundFeeAbovePurchaseFee)
    );
    // A purchase-fee decrease below the current compound fee without a
    // matching compound decrease.
    let mut s = FeeSchedule {
        purchase_fee_bps: 1_000,
        compound_fee_bps: 1_000,
        ..FeeSchedule::default()
    };
    assert_eq!(
        s.schedule(900, 1_000, T0),
        Err(EngineError::CompoundFeeAbovePurchaseFee)
    );
    assert!(!s.has_pending());
    assert_eq!(s.schedule(900, 900, T0), Ok(T0 + 259_200));
}

// ------------------------------------------------------------------ 82

#[test]
fn timelock_boundaries_reschedule_and_cancel() {
    let mut s = FeeSchedule {
        purchase_fee_bps: 1_000,
        compound_fee_bps: 1_000,
        ..FeeSchedule::default()
    };
    assert_eq!(FEE_CHANGE_TIMELOCK_SECONDS, 259_200);
    assert_eq!(s.apply(T0), Err(EngineError::NoPendingFeeChange));
    assert_eq!(s.cancel(), Err(EngineError::NoPendingFeeChange));

    let eff = s.schedule(800, 500, T0).unwrap();
    assert_eq!(eff, T0 + 259_200);
    assert_eq!(
        (
            s.pending_purchase_fee_bps,
            s.pending_compound_fee_bps,
            s.effective_at
        ),
        (800, 500, eff)
    );
    // Current fees unchanged until applied.
    assert_eq!((s.purchase_fee_bps, s.compound_fee_bps), (1_000, 1_000));
    assert_eq!(s.apply(eff - 1), Err(EngineError::FeeTimelockActive));
    assert_eq!((s.purchase_fee_bps, s.compound_fee_bps), (1_000, 1_000));

    // Reschedule one second before it would apply: the full delay restarts.
    let eff2 = s.schedule(700, 700, eff - 1).unwrap();
    assert_eq!(eff2, eff - 1 + 259_200);
    assert_eq!(s.apply(eff), Err(EngineError::FeeTimelockActive));
    assert_eq!(s.apply(eff2 - 1), Err(EngineError::FeeTimelockActive));

    // Cancel clears it; nothing can apply afterwards.
    s.cancel().unwrap();
    assert_eq!(
        s.apply(eff2 + 10 * 86_400),
        Err(EngineError::NoPendingFeeChange)
    );
    assert_eq!((s.purchase_fee_bps, s.compound_fee_bps), (1_000, 1_000));

    // Apply exactly at effective_at writes both values together, once.
    let eff3 = s.schedule(900, 600, T0 + 1).unwrap();
    assert_eq!(s.apply(eff3), Ok((900, 600)));
    assert_eq!((s.purchase_fee_bps, s.compound_fee_bps), (900, 600));
    assert!(!s.has_pending());
    assert_eq!(s.apply(eff3), Err(EngineError::NoPendingFeeChange));
    // Increases are timelocked too.
    let eff4 = s.schedule(1_000, 1_000, eff3).unwrap();
    assert_eq!(s.apply(eff4 - 1), Err(EngineError::FeeTimelockActive));
    assert_eq!(s.apply(eff4 + 1), Ok((1_000, 1_000)));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Any interleaving of schedule / apply / cancel: the pair stays valid at
    /// every step, and a value lands only after a full 72h since the most
    /// recent schedule.
    #[test]
    fn no_sequence_bypasses_the_timelock(
        ops in prop::collection::vec((0u8..3, 0u16..1_200, 0u16..1_200, 0i64..400_000), 1..60),
    ) {
        let mut s = FeeSchedule { purchase_fee_bps: 1_000, compound_fee_bps: 1_000, ..FeeSchedule::default() };
        let mut now = T0;
        let mut last_schedule: Option<(i64, u16, u16)> = None;
        for (op, p, c, dt) in ops {
            now += dt;
            let before = (s.purchase_fee_bps, s.compound_fee_bps);
            match op {
                0 => if s.schedule(p, c, now).is_ok() { last_schedule = Some((now, p, c)); },
                1 => if s.apply(now).is_ok() {
                    let (at, lp, lc) = last_schedule.take().unwrap();
                    prop_assert!(now >= at + FEE_CHANGE_TIMELOCK_SECONDS);
                    prop_assert_eq!((s.purchase_fee_bps, s.compound_fee_bps), (lp, lc));
                } else {
                    prop_assert_eq!((s.purchase_fee_bps, s.compound_fee_bps), before);
                },
                _ => { if s.cancel().is_ok() { last_schedule = None; } },
            }
            prop_assert!(s.compound_fee_bps <= s.purchase_fee_bps);
            prop_assert!(s.purchase_fee_bps <= MAX_PURCHASE_FEE_BPS);
            prop_assert_eq!(s.has_pending(), last_schedule.is_some());
        }
    }
}

// ------------------------------------------------------ 83 / 84 / 71 / 6

#[test]
fn commission_is_twenty_percent_at_ten_percent_fee() {
    for n in [1u64, 3, 10, 250] {
        let q = quote_purchase(n, P, 1_000).unwrap();
        let c = referral_commission(q.fee_lamports, q.capital_lamports).unwrap();
        assert_eq!(c, q.fee_lamports / 5);
        assert_eq!(q.fee_lamports - c, q.capital_lamports * 8 / 100);
    }
}

#[test]
fn clamp_at_nine_eight_and_below() {
    let v = 1_000_000_000u64; // 1 SOL principal
                              // 9%: floor(0.2 f) = 0.018 SOL but only f - 0.08 v = 0.01 SOL is free.
    assert_eq!(referral_commission(90_000_000, v).unwrap(), 10_000_000);
    // 8%: the whole fee is margin.
    assert_eq!(referral_commission(80_000_000, v).unwrap(), 0);
    for f in [0u64, 1, 50_000_000, 79_999_999] {
        assert_eq!(referral_commission(f, v).unwrap(), 0, "fee {f}");
    }
    // ceil on the margin: v = 13 lamports, m v = 1.04 → 2 retained.
    assert_eq!(referral_commission(10, 13).unwrap(), 2);
    assert_eq!(referral_commission(2, 13).unwrap(), 0);
    assert_eq!(referral_commission(3, 13).unwrap(), 0); // floor(0.6) = 0
}

#[test]
fn timelocked_fee_reduction_shrinks_commission_first() {
    // Test 6, fee half: at 10% the referrer gets 2% of volume; after a
    // (timelocked) cut to 9% the treasury still keeps exactly 8%.
    let q10 = quote_purchase(100, P, 1_000).unwrap();
    let q9 = quote_purchase(100, P, 900).unwrap();
    let c10 = referral_commission(q10.fee_lamports, q10.capital_lamports).unwrap();
    let c9 = referral_commission(q9.fee_lamports, q9.capital_lamports).unwrap();
    assert!(c9 < c10);
    assert_eq!(q9.fee_lamports - c9, q9.capital_lamports * 8 / 100);
    let q8 = quote_purchase(100, P, 800).unwrap();
    assert_eq!(
        referral_commission(q8.fee_lamports, q8.capital_lamports).unwrap(),
        0
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4_000))]

    /// Test 71 / 84: rounding keeps the per-event margin; commission never
    /// exceeds floor(0.2 f); zero whenever m v >= f.
    #[test]
    fn commission_never_cuts_the_margin(
        count in 1u64..=250,
        price in 1u64..=100_000_000,
        bps in 0u16..=1_000,
    ) {
        let q = quote_purchase(count, price, bps).unwrap();
        let (f, v) = (q.fee_lamports as u128, q.capital_lamports as u128);
        let c = referral_commission(q.fee_lamports, q.capital_lamports).unwrap() as u128;
        let margin = (v * 800).div_ceil(10_000);
        prop_assert!(c <= f * 2_000 / 10_000);
        prop_assert!(f - c >= f.min(margin));
        if margin >= f { prop_assert_eq!(c, 0); }
        prop_assert_eq!(c, (f * 2_000 / 10_000).min(f.saturating_sub(margin)));
    }

    #[test]
    fn commission_arbitrary_inputs(f in 0u64..=u64::MAX, v in 0u64..=u64::MAX) {
        let c = referral_commission(f, v).unwrap() as u128;
        let margin = (v as u128 * 800).div_ceil(10_000);
        prop_assert!(c <= f as u128 / 5);
        prop_assert!(f as u128 - c >= (f as u128).min(margin));
    }
}
