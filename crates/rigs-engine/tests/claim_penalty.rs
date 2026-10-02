//! Early-claim fee (architecture §7a item 5; tests 85–87 engine side):
//! 30/20/10/0 schedule, floor rounding, the 60/40 treasury/grove split,
//! reset rules, and every bypass attempt. The `World` harness asserts after
//! every action that the reward vault backs what it owes, that `vault =
//! funded − claimed − compounded − treasury share`, that `paid + owed ≤
//! funded`, that FEE holds exactly purchase + compound fees + treasury
//! shares, and that a claim never moves the capital vault.

mod common;

use common::{World, SOL};
use proptest::prelude::*;
use rigs_engine::*;

const PRICE: u64 = 10_000_000;
const FEE: u16 = 300;
const H: i64 = 3_600;
const DAY: i64 = 86_400;

// ------------------------------------------------------------- schedule

#[test]
fn schedule_boundaries_are_exact() {
    let t = 1_000_000;
    // Test 85: exact half-open boundaries.
    for (age, bps) in [
        (0, 3_000),
        (DAY - 1, 3_000),
        (DAY, 2_000),
        (2 * DAY - 1, 2_000),
        (2 * DAY, 1_000),
        (3 * DAY - 1, 1_000),
        (3 * DAY, 0),
        (10 * 365 * DAY, 0),
    ] {
        assert_eq!(claim_penalty_bps(t, t + age), bps, "age {age}");
    }
    // A clock behind the anchor is the strict case, never a discount.
    assert_eq!(claim_penalty_bps(t, t - 1), 3_000);
    assert_eq!(claim_penalty_bps(i64::MAX, i64::MIN), 3_000);
}

/// `x = floor(gross × bps / 10_000)` (§7a item 5).
#[test]
fn early_claim_fee_rounds_down() {
    assert_eq!(penalty_amount(1, 3_000).unwrap(), 0);
    assert_eq!(penalty_amount(3, 3_000).unwrap(), 0);
    assert_eq!(penalty_amount(4, 3_000).unwrap(), 1);
    assert_eq!(penalty_amount(10, 3_000).unwrap(), 3);
    assert_eq!(penalty_amount(9, 1_000).unwrap(), 0);
    assert_eq!(penalty_amount(1_000, 0).unwrap(), 0);
    assert_eq!(
        penalty_amount(u64::MAX, 3_000).unwrap(),
        (u64::MAX as u128 * 3 / 10) as u64
    );
}

/// Test 86: the split parts sum exactly to `x`, treasury = floor(0.60 x).
#[test]
fn split_is_exact_and_favours_the_grove() {
    assert_eq!(early_claim_fee_split(0), (0, 0));
    assert_eq!(early_claim_fee_split(1), (0, 1));
    assert_eq!(early_claim_fee_split(2), (1, 1));
    assert_eq!(early_claim_fee_split(3), (1, 2));
    assert_eq!(early_claim_fee_split(5), (3, 2));
    assert_eq!(early_claim_fee_split(10), (6, 4));
    assert_eq!(early_claim_fee_split(99), (59, 40));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn fee_is_the_exact_floor_and_splits_exactly(
        gross in 0u64..=u64::MAX,
        bps in prop::sample::select(vec![0u64, 1_000, 2_000, 3_000]),
    ) {
        let x = penalty_amount(gross, bps).unwrap() as u128;
        prop_assert_eq!(x, gross as u128 * bps as u128 / 10_000);
        let (t, g) = early_claim_fee_split(x as u64);
        prop_assert_eq!(t as u128 + g as u128, x);
        prop_assert_eq!(t as u128, x * 6 / 10);
        prop_assert!(g >= t / 2); // 40% ≥ ⅔ of 60%, rounded toward the grove
    }
}

// ------------------------------------------------------ where it goes

/// Test 86: 60% of the fee moves REWARD → FEE; the 40% grove share stays in
/// the reward vault and is owed to the OTHER planters — by shares — never
/// the claimer.
#[test]
fn penalty_is_recredited_to_other_planters_only() {
    let mut w = World::new(3, PRICE, FEE);
    w.fund(30 * SOL, 30 * DAY as u64).unwrap();
    w.buy(0, 100).unwrap();
    w.buy(1, 100).unwrap();
    w.buy(2, 200).unwrap();
    w.warp(H);
    let (b1, b2) = (w.earned(1), w.earned(2));
    let (vault, fees) = (w.reward_vault, w.fee_vault);

    let o = w.claim_outcome(0).unwrap();
    assert_eq!(o.penalty_bps, 3_000);
    assert_eq!(o.penalty, o.gross * 3 / 10);
    assert_eq!(o.penalty_treasury, o.penalty * 6 / 10);
    assert_eq!(o.penalty_treasury + o.penalty_grove, o.penalty);
    assert_eq!(o.net, o.gross - o.penalty);
    assert_eq!(
        w.reward_vault,
        vault - o.net - o.penalty_treasury,
        "net to the claimer and the treasury share to FEE left REWARD"
    );
    assert_eq!(w.fee_vault, fees + o.penalty_treasury);
    assert_eq!(w.ledger.total_reward_to_fees, o.penalty_treasury);
    assert_eq!(
        w.earned(0),
        0,
        "the claimer gets none of its own penalty back"
    );
    // 1 : 2 split of the grove share across the other 300 rigs.
    let (g1, g2) = (w.earned(1) - b1, w.earned(2) - b2);
    assert!(g1 + g2 <= o.penalty_grove && o.penalty_grove - (g1 + g2) <= 2);
    assert!(g2 == 2 * g1 || g2 == 2 * g1 + 1);
}

/// A sole planter's penalty cannot come back to it: it is credited to dust
/// and stays in the reward vault, attributed to no one.
#[test]
fn sole_planter_penalty_goes_to_dust_not_back_to_the_claimer() {
    let mut w = World::new(1, PRICE, FEE);
    w.fund(10 * SOL, 10 * DAY as u64).unwrap();
    w.buy(0, 10).unwrap();
    w.warp(H);
    let dust = w.globals.undistributed_dust;
    let o = w.claim_outcome(0).unwrap();
    assert!(o.penalty_grove > 0);
    assert_eq!(w.globals.undistributed_dust, dust + o.penalty_grove);
    assert_eq!(w.earned(0), 0);
    assert_eq!(w.claim(0), Err(EngineError::NothingToClaim));
}

// ---------------------------------------------------------- reset rules

#[test]
fn claims_do_not_reset_the_clock() {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(30 * SOL, 30 * DAY as u64).unwrap();
    w.buy(0, 10).unwrap();
    w.buy(1, 10).unwrap();
    w.warp(DAY + H);
    assert_eq!(w.claim_outcome(0).unwrap().penalty_bps, 2_000);
    assert_eq!(
        w.miners[0].last_planted_at, 0,
        "claim never moves the anchor"
    );
    w.warp(DAY);
    assert_eq!(w.claim_outcome(0).unwrap().penalty_bps, 1_000);
    w.warp(DAY);
    assert_eq!(w.claim_outcome(0).unwrap().penalty_bps, 0);
}

#[test]
fn a_purchase_resets_the_clock_for_the_whole_position() {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(30 * SOL, 30 * DAY as u64).unwrap();
    w.buy(0, 10).unwrap();
    w.buy(1, 10).unwrap();
    w.warp(3 * DAY - H);
    let pending = w.earned(0);
    assert!(pending > 0);
    w.buy(0, 1).unwrap(); // new capital at 71h
    assert_eq!(w.miners[0].last_planted_at, w.now);
    w.warp(2 * H); // 73h after the first purchase, 2h after the second
                   // Test 87 (engine): the whole pending amount, older rewards included,
                   // is back under the 30% tier.
    let o = w.claim_outcome(0).unwrap();
    assert_eq!(o.penalty_bps, 3_000);
    assert!(o.gross >= pending);
    assert_eq!(o.penalty, o.gross * 3 / 10);
}

/// Replant is never penalised and does not move the clock.
#[test]
fn replant_is_free_and_does_not_reset_the_clock() {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(30 * SOL, 3 * DAY as u64).unwrap();
    w.buy(0, 10).unwrap();
    w.buy(1, 10).unwrap();
    w.warp(H);
    let earned = w.earned(0);
    let out = w.compound(0).unwrap();
    assert_eq!(
        out.total_lamports + out.remainder_lamports,
        earned,
        "no penalty on replant"
    );
    let anchor = w.miners[0].last_planted_at;
    w.warp(3 * DAY);
    assert_eq!(w.miners[0].last_planted_at, anchor);
    assert_eq!(w.claim_outcome(0).unwrap().penalty_bps, 0);
}

// ---------------------------------------------------------- bypass attempts

/// Repeated same-slot claims recover nothing.
#[test]
fn repeated_claims_cannot_recover_the_penalty() {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(10 * SOL, 10 * DAY as u64).unwrap();
    w.buy(0, 225).unwrap();
    w.buy(1, 25).unwrap();
    w.warp(H);
    let first = w.claim_outcome(0).unwrap();
    assert!(first.penalty > 0);
    assert_eq!(w.claim(0), Err(EngineError::NothingToClaim));
}

/// Claiming in many small pieces never pays less penalty than one claim.
#[test]
fn split_claims_pay_at_least_the_single_claim_penalty() {
    let run = |steps: i64| {
        let mut w = World::new(2, PRICE, FEE);
        w.fund(10 * SOL, 10 * DAY as u64).unwrap();
        w.buy(0, 10).unwrap();
        w.buy(1, 10).unwrap();
        let mut penalty = 0;
        for _ in 0..steps {
            w.warp(12 * H / steps);
            if let Ok(o) = w.claim_outcome(0) {
                penalty += o.penalty;
            }
        }
        (penalty, w.miners[0].lifetime_sol_claimed)
    };
    let (single, _) = run(1);
    let (many, _) = run(720);
    assert!(many >= single, "split {many} < single {single}");
}

/// Replant-then-claim cannot launder a penalty: replanted seeds are not
/// redeemable, and later rewards are still claimed on the same clock.
#[test]
fn replanting_does_not_launder_the_penalty() {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(10 * SOL, 10 * DAY as u64).unwrap();
    w.buy(0, 10).unwrap();
    w.buy(1, 10).unwrap();
    w.warp(6 * H);
    let _ = w.compound(0).unwrap();
    w.warp(H);
    let o = w.claim_outcome(0).unwrap();
    assert_eq!(
        o.penalty_bps, 3_000,
        "still inside the first 24h of the purchase"
    );
}

/// Referral weight: penalty is a share of the claimer's (weighted) gross,
/// and is re-credited to others by their weighted shares.
#[test]
fn penalty_respects_weighted_shares() {
    let mut w = World::new(3, PRICE, FEE);
    w.fund(30 * SOL, 30 * DAY as u64).unwrap();
    w.miners[2].weight = WEIGHT_REFERRED;
    w.buy(0, 100).unwrap();
    w.buy(1, 100).unwrap();
    w.buy(2, 100).unwrap();
    w.warp(H);
    let (b1, b2) = (w.earned(1), w.earned(2));
    let o = w.claim_outcome(0).unwrap();
    let (g1, g2) = (w.earned(1) - b1, w.earned(2) - b2);
    // 100 : 105 by shares.
    assert!(g2 * 100 >= g1 * 104 && g2 * 100 <= g1 * 106);
    assert!(g1 + g2 <= o.penalty_grove);
}

/// After exhaustion the penalty still applies to fresh positions and still
/// re-credits (from the vault's own balance, never new emission).
#[test]
fn penalty_after_exhaustion_moves_no_new_emission() {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(SOL, DAY as u64).unwrap();
    w.buy(0, 10).unwrap();
    w.buy(1, 10).unwrap();
    w.warp(2 * DAY);
    let funded = w.ledger.total_reward_funded;
    let o = w.claim_outcome(0).unwrap();
    assert!(o.penalty > 0);
    assert_eq!(w.ledger.total_reward_funded, funded);
}

// ------------------------------------------------------------- solvency

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Random buys, claims, replants and warps across the fee windows, with an
    /// independent compound fee:
    /// the harness checks every invariant after every action, and the books
    /// close at the end.
    #[test]
    fn interleavings_with_penalties_stay_solvent(
        ops in prop::collection::vec((0usize..4, 0u8..4, 0i64..(30 * H)), 1..120),
        referred in prop::collection::vec(any::<bool>(), 4),
    ) {
        let mut w = World::new(4, PRICE, FEE);
        w.compound_fee_bps = FEE / 2;
        w.fund(20 * SOL, 10 * DAY as u64).unwrap();
        for (i, r) in referred.iter().enumerate() {
            if *r { w.miners[i].weight = WEIGHT_REFERRED; }
        }
        for (who, op, dt) in ops {
            w.warp(dt);
            match op {
                0 => { let _ = w.buy(who, 1 + (dt as u64 % 40)); }
                1 | 2 => { let _ = w.claim(who); }
                _ => { let _ = w.compound(who); }
            }
        }
        let owed: u128 = (0..4).map(|i| w.earned(i) as u128).sum();
        let paid = w.ledger.total_reward_claimed as u128
            + w.ledger.total_reward_compounded as u128
            + w.ledger.total_reward_to_fees as u128;
        prop_assert!(paid + owed <= w.ledger.total_reward_funded as u128);
        prop_assert!(w.reward_vault as u128 >= owed);
    }
}

// ------------------------------------------------------ compound fee (79, 81)

/// Test 79 (engine): `compound` charges `compound_fee_bps`, `buy_rigs`
/// charges `purchase_fee_bps`; the quotes differ exactly by rate.
#[test]
fn compound_uses_its_own_fee_rate() {
    let mut w = World::new(2, PRICE, 1_000);
    w.compound_fee_bps = 800;
    w.fund(30 * SOL, 3 * DAY as u64).unwrap();
    let q = w.buy(0, 10).unwrap();
    assert_eq!(q.fee_lamports, 10 * PRICE / 10);
    w.buy(1, 10).unwrap();
    w.warp(DAY);
    let earned = w.earned(0);
    let out = w.compound(0).unwrap();
    assert_eq!(out.capital_lamports, out.rigs_minted * PRICE);
    assert_eq!(out.fee_lamports, out.capital_lamports * 800 / 10_000);
    assert_eq!(out.total_lamports + out.remainder_lamports, earned);
    assert_eq!(
        out.rigs_minted,
        rigs_affordable(earned, PRICE, 800).unwrap().min(240)
    );
    let buy_same = quote_purchase(out.rigs_minted, PRICE, 1_000).unwrap();
    assert_eq!(buy_same.capital_lamports, out.capital_lamports);
    assert_eq!(
        buy_same.fee_lamports - out.fee_lamports,
        out.capital_lamports * 200 / 10_000
    );
}

/// Test 81 (engine): a capped or sub-seed compound changes nothing.
#[test]
fn noop_compound_changes_nothing() {
    let mut w = World::new(2, PRICE, 1_000);
    w.fund(30 * SOL, 30 * DAY as u64).unwrap();
    w.buy(0, 250).unwrap();
    w.buy(1, 1).unwrap();
    w.warp(DAY);
    let before = (w.miners[0], w.reward_vault, w.fee_vault, w.capital_vault);
    assert_eq!(w.compound(0), Err(EngineError::WalletCapExceeded));
    assert_eq!(
        (w.miners[0], w.reward_vault, w.fee_vault, w.capital_vault),
        before
    );
    assert!(w.earned(0) > 0, "earned SOL stays claimable");

    // Rewards below one whole seed: nothing to compound, nothing moves.
    let mut w = World::new(2, PRICE, 1_000);
    w.fund(30 * SOL, 30 * DAY as u64).unwrap();
    w.buy(0, 1).unwrap();
    w.warp(1);
    // The engine may settle before it refuses; on-chain the error reverts the
    // whole instruction. Economically nothing may change.
    let snap = |w: &World| {
        (
            w.earned(0),
            w.miners[0].rigs,
            w.miners[0].last_planted_at,
            w.reward_vault,
            w.fee_vault,
            w.capital_vault,
        )
    };
    let before = snap(&w);
    assert!(before.0 > 0 && before.0 < PRICE);
    assert_eq!(w.compound(0), Err(EngineError::NothingToCompound));
    assert_eq!(snap(&w), before);
}
