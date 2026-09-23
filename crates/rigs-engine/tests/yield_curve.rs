//! Genesis yield: correctness of the APR / phase / threshold / projection
//! maths, that the displayed APR is the rate the accumulator actually pays,
//! and the exploit classes the yield system has to be immune to.

mod common;

use common::{World, SOL};
use proptest::prelude::*;
use rigs_engine::*;

const PRICE: u64 = 10_000_000; // 0.01 SOL
const FEE: u16 = 300; // 3.00%
const DAY: i64 = 86_400;

/// Inputs for `total_rigs` base-weight rigs.
fn inputs(rate: u64, total_rigs: u64, now: i64, finish: i64) -> YieldInputs {
    shares_inputs(rate, total_rigs * WEIGHT_BASE, now, finish)
}

fn shares_inputs(rate: u64, total_shares: u64, now: i64, finish: i64) -> YieldInputs {
    YieldInputs {
        reward_rate: rate,
        reward_period_finish: finish,
        total_shares,
        rig_price_lamports: PRICE,
        purchase_fee_bps: FEE,
        now,
    }
}

/// Fee-inclusive lamports per rig.
fn cost() -> u128 {
    PRICE as u128 * (10_000 + FEE as u128) / 10_000
}

// ------------------------------------------------------------------ maths

#[test]
fn apr_is_annual_emission_over_fee_inclusive_capital() {
    // 1 SOL/day over 1,000 rigs of 0.0103 SOL (10.3 SOL planted):
    // 365 SOL / 10.3 SOL ≈ 3,543%, less the flooring of the per-second rate.
    let rate = SOL / 86_400; // 11_574 lamports/s (floored)
    let i = inputs(rate, 1_000, 0, 180 * DAY);
    let expected = rate as u128 * SECONDS_PER_YEAR as u128 * 10_000 / (1_000 * cost());
    assert_eq!(apr_bps(&i, 0).unwrap() as u128, expected);
    // ≈ 3,543%: Genesis.
    assert!((354_000..354_400).contains(&apr_bps(&i, 0).unwrap()));
    assert_eq!(current_phase(&i).unwrap(), YieldPhase::Genesis);
}

#[test]
fn apr_is_zero_without_funded_emission() {
    assert_eq!(
        apr_bps(&inputs(1_000, 10, 100, 100), 0).unwrap(),
        0,
        "window ended"
    );
    assert_eq!(
        apr_bps(&inputs(1_000, 10, 101, 100), 0).unwrap(),
        0,
        "past the window"
    );
    assert_eq!(apr_bps(&inputs(0, 10, 0, 100), 0).unwrap(), 0, "no rate");
    assert_eq!(
        current_phase(&inputs(0, 10, 0, 100)).unwrap(),
        YieldPhase::Dormant
    );
}

#[test]
fn apr_with_no_rigs_quotes_the_first_rig() {
    let i = inputs(1_000_000, 0, 0, DAY);
    assert_eq!(apr_bps(&i, 0).unwrap(), apr_bps(&i, 1).unwrap());
}

#[test]
fn marginal_apr_includes_your_own_dilution() {
    let i = inputs(1_000_000, 1_000, 0, DAY);
    let now = apr_bps(&i, 0).unwrap();
    let after = apr_bps(&i, 1_000).unwrap();
    // Doubling the rig count halves the APR, to the basis point.
    assert!(after == now / 2 || after + 1 == now / 2 || after == now / 2 + 1);
}

#[test]
fn phase_bands_are_exact() {
    use YieldPhase::*;
    for (bps, phase) in [
        (u64::MAX, Genesis),
        (300_000, Genesis),
        (120_000, Genesis),
        (119_999, EarlyGrove),
        (60_000, EarlyGrove),
        (59_999, Growth),
        (25_000, Growth),
        (24_999, Mature),
        (1, Mature),
        (0, Dormant),
    ] {
        assert_eq!(YieldPhase::from_apr_bps(bps), phase, "{bps} bps");
    }
}

#[test]
fn next_threshold_is_exact_to_the_rig() {
    let rate = SOL / 86_400;
    for total in [1u64, 7, 100, 1_000, 2_847] {
        let i = inputs(rate, total, 0, 180 * DAY);
        let phase = current_phase(&i).unwrap();
        match next_threshold(&i).unwrap() {
            NextThreshold::Shares {
                phase: next,
                at_total_shares,
            } => {
                assert_eq!(next, phase.next());
                assert!(at_total_shares > total * WEIGHT_BASE);
                let at = shares_inputs(rate, at_total_shares, 0, 180 * DAY);
                let before = shares_inputs(rate, at_total_shares - 1, 0, 180 * DAY);
                assert!(
                    apr_bps(&at, 0).unwrap() < phase.floor_bps(),
                    "at the threshold the APR must be below {phase:?}'s floor"
                );
                assert!(
                    apr_bps(&before, 0).unwrap() >= phase.floor_bps(),
                    "one rig before the threshold must still be {phase:?}"
                );
                assert_eq!(current_phase(&at).unwrap(), next);
            }
            NextThreshold::WindowEnds { at } => {
                assert_eq!(phase, YieldPhase::Mature);
                assert_eq!(at, 180 * DAY);
            }
            NextThreshold::None => panic!("emitting state has a threshold"),
        }
    }
}

#[test]
fn required_emission_inverts_apr() {
    // 3,000% on 10 SOL needs 300 SOL a year.
    assert_eq!(
        required_annual_emission(300_000, 10 * SOL),
        300 * SOL as u128
    );
}

// ------------------------------------------- UI rate == accounting rate

/// The core anti-drift property: over any window with no state change, what
/// one rig actually accrues equals what the displayed APR says it accrues,
/// to within flooring (one lamport per rig per step plus the bps floor).
#[test]
fn displayed_apr_is_the_rate_the_accumulator_pays() {
    for (rate, rigs) in [
        (11_574u64, 1_000u64),
        (500_000, 37),
        (2, 5),
        (1_000_000_000, 123_456),
    ] {
        let mut w = World::new(2, PRICE, FEE);
        w.fund(rate * 365 * DAY as u64, 365 * DAY as u64).unwrap();
        w.buy(0, rigs - 1).unwrap();
        w.buy(1, 1).unwrap();
        let apr = apr_bps(&YieldInputs::from_globals(&w.globals, PRICE, FEE, w.now), 0).unwrap();

        let before = w.earned(1);
        let dt = 30 * DAY;
        w.warp(dt);
        let accrued = (w.earned(1) - before) as u128;

        // What the APR predicts one rig earns over dt.
        let predicted = apr as u128 * cost() * dt as u128 / (SECONDS_PER_YEAR as u128 * 10_000);
        // Exact per-rig emission from the schedule.
        let exact = w.globals.reward_rate as u128 * dt as u128 / rigs as u128;
        assert!(
            accrued <= exact && exact - accrued <= 1,
            "accumulator vs schedule"
        );
        // The bps floor loses < 1 bps of cost per year.
        let slack = cost() * dt as u128 / (SECONDS_PER_YEAR as u128 * 10_000) + 2;
        assert!(
            predicted <= accrued + 1 && accrued - predicted.min(accrued) <= slack,
            "APR {apr} bps predicts {predicted}, accumulator paid {accrued} (rate {rate}, rigs {rigs})"
        );
    }
}

// ------------------------------------------------------------ projection

#[test]
fn projection_without_compounding_is_simple_share_of_emission() {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(100 * SOL, 180 * DAY as u64).unwrap();
    w.buy(0, 900).unwrap();
    w.buy(1, 100).unwrap();
    let p = project_compounding(
        &w.globals,
        &w.miners[1],
        w.now,
        PRICE,
        FEE,
        0,
        365 * DAY as u64,
    )
    .unwrap();
    assert_eq!(
        p.horizon_seconds,
        180 * DAY as u64,
        "capped at the funded runway"
    );
    assert_eq!(p.end_rigs, 100);
    // Exactly 10% of what the (floored) schedule emits over the window.
    let emitted = w.globals.reward_rate as u128 * p.horizon_seconds as u128;
    assert_eq!(p.reward_lamports as u128, emitted / 10);
    assert!(p.reward_lamports <= 10 * SOL);
}

#[test]
fn compounding_beats_simple_and_stays_inside_the_emission() {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(100 * SOL, 180 * DAY as u64).unwrap();
    w.buy(0, 900).unwrap();
    w.buy(1, 100).unwrap();
    let simple = project_compounding(
        &w.globals,
        &w.miners[1],
        w.now,
        PRICE,
        FEE,
        0,
        365 * DAY as u64,
    )
    .unwrap();
    let daily = project_compounding(
        &w.globals,
        &w.miners[1],
        w.now,
        PRICE,
        FEE,
        DAY as u64,
        365 * DAY as u64,
    )
    .unwrap();
    assert!(daily.reward_lamports > simple.reward_lamports);
    assert!(daily.end_rigs > simple.end_rigs);
    // The whole window only ever emits 100 SOL.
    assert!(daily.reward_lamports <= 100 * SOL);
}

#[test]
fn projection_is_zero_once_the_window_has_ended() {
    let mut w = World::new(1, PRICE, FEE);
    w.fund(SOL, DAY as u64).unwrap();
    w.buy(0, 10).unwrap();
    w.warp(2 * DAY);
    let p = project_compounding(
        &w.globals,
        &w.miners[0],
        w.now,
        PRICE,
        FEE,
        3_600,
        365 * DAY as u64,
    )
    .unwrap();
    assert_eq!(p.horizon_seconds, 0);
    assert_eq!(p.reward_lamports, 0);
    assert_eq!(p.yield_bps, 0);
}

#[test]
fn projection_refuses_unbounded_work() {
    let mut w = World::new(1, PRICE, FEE);
    w.fund(SOL, 365 * DAY as u64).unwrap();
    w.buy(0, 1).unwrap();
    assert_eq!(
        project_compounding(
            &w.globals,
            &w.miners[0],
            w.now,
            PRICE,
            FEE,
            1,
            365 * DAY as u64
        ),
        Err(EngineError::InvalidDuration)
    );
}

#[test]
fn projection_does_not_touch_real_state() {
    let mut w = World::new(1, PRICE, FEE);
    w.fund(SOL, 30 * DAY as u64).unwrap();
    w.buy(0, 10).unwrap();
    let (g, m) = (w.globals, w.miners[0]);
    project_compounding(
        &w.globals,
        &w.miners[0],
        w.now,
        PRICE,
        FEE,
        DAY as u64,
        30 * DAY as u64,
    )
    .unwrap();
    assert_eq!((w.globals, w.miners[0]), (g, m));
}

#[test]
fn effective_rate_waits_an_hour_then_annualises_what_was_received() {
    let mut pos = MinerPosition {
        lifetime_sol_spent: 10 * SOL,
        ..Default::default()
    };
    assert_eq!(effective_rate_bps(&pos, 0, 0, 3_599), None);
    pos.lifetime_sol_claimed = SOL;
    // 1 SOL received on 10 SOL over half a year = 20% APR.
    let half_year = SECONDS_PER_YEAR as i64 / 2;
    assert_eq!(effective_rate_bps(&pos, 0, 0, half_year), Some(2_000));
    assert_eq!(effective_rate_bps(&pos, SOL, 0, half_year), Some(4_000));
}

// -------------------------------------------------------------- exploits

/// Splitting one purchase into many does not buy early status: it earns
/// exactly what the single purchase earns.
#[test]
fn exploit_deposit_splitting_earns_nothing_extra() {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(50 * SOL, 90 * DAY as u64).unwrap();
    w.buy(0, 100).unwrap();
    for _ in 0..100 {
        w.buy(1, 1).unwrap();
    }
    w.warp(30 * DAY);
    assert_eq!(w.earned(0), w.earned(1));
}

/// Claim-and-rebuy cannot reset a phase: phase is a function of global state
/// alone, and the only global effect of a rebuy is more rigs (a later phase).
#[test]
fn exploit_claim_and_rebuy_cannot_reset_the_phase() {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(100 * SOL, 180 * DAY as u64).unwrap();
    w.buy(0, 50).unwrap();
    w.buy(1, 50).unwrap();
    w.warp(10 * DAY);
    let phase = |w: &World| {
        current_phase(&YieldInputs::from_globals(&w.globals, PRICE, FEE, w.now)).unwrap()
    };
    let apr =
        |w: &World| apr_bps(&YieldInputs::from_globals(&w.globals, PRICE, FEE, w.now), 0).unwrap();
    let (p0, a0) = (phase(&w), apr(&w));
    w.claim(1).unwrap();
    assert_eq!((phase(&w), apr(&w)), (p0, a0), "a claim changes neither");
    w.buy(1, 10).unwrap();
    assert!(apr(&w) < a0, "a rebuy only dilutes");
    assert!(phase(&w) >= p0, "phases only move forward");
}

#[test]
fn exploit_double_claim_pays_once() {
    let mut w = World::new(1, PRICE, FEE);
    w.fund(SOL, DAY as u64).unwrap();
    w.buy(0, 10).unwrap();
    w.warp(3_600);
    assert!(w.claim(0).unwrap() > 0);
    assert_eq!(w.claim(0), Err(EngineError::NothingToClaim));
}

/// Claim and compound in the same second draw from one balance, never two.
#[test]
fn exploit_claim_plus_compound_cannot_duplicate() {
    for order in [0, 1] {
        let mut w = World::new(1, PRICE, FEE);
        w.fund(10 * SOL, DAY as u64).unwrap();
        w.buy(0, 10).unwrap();
        w.warp(3_600);
        let earned = w.earned(0);
        let mut forfeited = 0;
        let out = if order == 0 {
            let c = w.compound(0).unwrap().total_lamports;
            let cl = w
                .claim_outcome(0)
                .map(|o| (o.net, o.penalty))
                .unwrap_or((0, 0));
            forfeited += cl.1;
            c + cl.0
        } else {
            let cl = w.claim_outcome(0).unwrap();
            forfeited += cl.penalty;
            cl.net + w.compound(0).map(|o| o.total_lamports).unwrap_or(0)
        };
        assert_eq!(
            out + forfeited,
            earned,
            "order {order}: paid out + penalty is exactly what was earned"
        );
        assert_eq!(w.earned(0), 0);
    }
}

/// Compounding every second is rate-limited by the emission, not by the
/// planter: a single planter can never receive more than the schedule emits.
#[test]
fn exploit_compound_spam_is_bounded_by_emission() {
    let mut w = World::new(1, PRICE, FEE);
    w.fund(10 * SOL, DAY as u64).unwrap();
    w.buy(0, 1).unwrap();
    let mut out = 0u64;
    for _ in 0..3_600 {
        w.warp(1);
        if let Ok(o) = w.compound(0) {
            out += o.total_lamports;
        }
    }
    let emitted = w.globals.reward_rate * 3_600;
    assert!(out + w.earned(0) <= emitted);
}

/// The clock only comes from the chain; a timestamp earlier than the last
/// sync is a no-op, never negative emission or a rewind.
#[test]
fn exploit_clock_cannot_run_backwards() {
    let mut w = World::new(1, PRICE, FEE);
    w.fund(SOL, DAY as u64).unwrap();
    w.buy(0, 10).unwrap();
    w.warp(1_000);
    w.globals.sync(w.now).unwrap();
    let snap = w.globals;
    w.globals.sync(w.now - 500).unwrap();
    assert_eq!(w.globals, snap);
    // And APR at an earlier clock is still only ever what the schedule funds.
    let i = YieldInputs::from_globals(&w.globals, PRICE, FEE, w.now - 500);
    assert_eq!(
        apr_bps(&i, 0).unwrap(),
        apr_bps(&YieldInputs::from_globals(&w.globals, PRICE, FEE, w.now), 0).unwrap()
    );
}

/// Exhaustion: when the window ends APR is 0, the phase is Dormant, nothing
/// further accrues, everything earned stays claimable, and a rig planted
/// after exhaustion earns nothing until new funding arrives.
#[test]
fn exhaustion_is_defined() {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(5 * SOL, 10 * DAY as u64).unwrap();
    w.buy(0, 100).unwrap();
    w.warp(11 * DAY);
    let i = YieldInputs::from_globals(&w.globals, PRICE, FEE, w.now);
    assert_eq!(apr_bps(&i, 0).unwrap(), 0);
    assert_eq!(current_phase(&i).unwrap(), YieldPhase::Dormant);
    assert_eq!(next_threshold(&i).unwrap(), NextThreshold::None);

    let owed = w.earned(0);
    w.warp(30 * DAY);
    assert_eq!(w.earned(0), owed, "nothing accrues after the window");

    w.buy(1, 100).unwrap();
    w.warp(30 * DAY);
    assert_eq!(w.earned(1), 0, "a post-exhaustion rig earns nothing");

    let paid = w.claim(0).unwrap();
    assert_eq!(paid, owed);
    // What remains is flooring residue only: < 1 lamport/s of the rate, plus
    // < 1 lamport per rig from the per-rig division.
    assert!(
        w.reward_vault < 10 * DAY as u64 + 200,
        "only flooring dust remains: {}",
        w.reward_vault
    );
}

/// Referral V1 lives entirely inside `buy_rigs` (an optional referrer bound
/// once, as a reward weight). There is no referral instruction and no
/// referrer payout: the instruction set is exactly the one below, so a
/// referral can never move lamports on its own. Adding one would have to be
/// done here, deliberately.
#[test]
fn exploit_referral_has_no_payout_surface() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../programs/sol-rigs/src/lib.rs"
    ))
    .unwrap();
    let program = &src[src.find("pub mod sol_rigs").unwrap()..];
    let mut fns: Vec<&str> = program
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub fn "))
        .map(|l| l.split(['(', '<']).next().unwrap())
        .collect();
    fns.sort_unstable();
    assert_eq!(
        fns,
        [
            "accept_authority",
            "buy_rigs",
            "claim",
            "compound",
            "fund_rewards",
            "initialize",
            "set_fee_recipient",
            "set_paused",
            "set_pricing",
            "set_reward_funder",
            "transfer_authority",
            "withdraw_fees",
        ]
    );
    let events = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../programs/sol-rigs/src/events.rs"
    ))
    .unwrap();
    // The only referral event records a binding; none records a payment.
    assert!(events.contains("pub struct ReferralBound"));
    assert!(!events.to_lowercase().contains("referralpaid"));
    assert!(!events.to_lowercase().contains("referrer_reward"));
}

/// Every money handler reads the chain clock itself; none accepts a time.
#[test]
fn exploit_no_user_supplied_timestamp() {
    for f in ["buy_rigs", "claim", "compound", "fund_rewards"] {
        let src = std::fs::read_to_string(format!(
            "{}/../../programs/sol-rigs/src/instructions/{f}.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        assert!(
            src.contains("Clock::get()?.unix_timestamp"),
            "{f} must read the Clock sysvar"
        );
        let sig = &src[src.find("pub fn ").unwrap()..];
        let sig = &sig[..sig.find(')').unwrap()];
        assert!(
            !sig.contains("i64"),
            "{f} must not take a timestamp argument: {sig}"
        );
    }
}

// ------------------------------------------------------------ properties

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// No input can make these functions panic, wrap, or report a rate the
    /// schedule does not fund.
    #[test]
    fn yield_maths_never_panics_or_overstates(
        rate in 0u64..=MAX_REWARD_RATE_LAMPORTS_PER_SECOND,
        rigs in 0u64..=MAX_TOTAL_RIGS,
        extra in 0u64..=MAX_RIGS_PER_PURCHASE,
        price in 1u64..=u64::MAX,
        fee in 0u16..=MAX_PURCHASE_FEE_BPS,
        now in -1_000i64..1_000_000,
        finish in -1_000i64..1_000_000,
    ) {
        let i = YieldInputs { reward_rate: rate, reward_period_finish: finish,
            total_shares: rigs * WEIGHT_BASE, rig_price_lamports: price, purchase_fee_bps: fee, now };
        let apr = apr_bps(&i, extra).unwrap();
        let _ = next_threshold(&i).unwrap();
        if apr > 0 && apr < u64::MAX {
            // Inverse check: the emission this APR implies over the planted
            // capital is never more than the schedule's annual emission.
            let denom_rigs = core::cmp::max(rigs as u128 + extra as u128, 1);
            let capital = denom_rigs * price as u128 * (10_000 + fee as u128) / 10_000;
            let implied = apr as u128 * capital / 10_000;
            prop_assert!(implied <= rate as u128 * SECONDS_PER_YEAR as u128);
        }
    }

    /// Random projections never pay more than the window emits.
    #[test]
    fn projection_never_exceeds_emission(
        fund_sol in 1u64..1_000,
        days in 1u64..365,
        mine in 1u64..10_000,
        others in 0u64..100_000,
        interval_h in 0u64..48,
    ) {
        let mut w = World::new(2, PRICE, FEE);
        w.fund(fund_sol * SOL, days * DAY as u64).unwrap();
        w.buy(0, mine).unwrap();
        if others > 0 { w.buy(1, others).unwrap(); }
        let interval = interval_h * 3_600;
        if interval > 0 && (days * DAY as u64) / interval > MAX_PROJECTION_STEPS { return Ok(()); }
        let p = project_compounding(&w.globals, &w.miners[0], w.now, PRICE, FEE, interval, 365 * DAY as u64).unwrap();
        prop_assert!(p.reward_lamports as u128 <= w.globals.reward_rate as u128 * p.horizon_seconds as u128);
    }
}
