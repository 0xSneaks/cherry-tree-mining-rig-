//! Acceptance tests 1-13 and 19 from THE CHERRY TREE spec §12.
//!
//! Each test is named for the numbered requirement it discharges.

mod common;

use common::{World, SOL};
use rigs_engine::*;

const CENTI: u64 = SOL / 100; // 0.01 SOL — the reference rig price

// ---------------------------------------------------------------- Accounting

/// Test 1 — a new rig earns zero rewards accrued before its purchase block.
#[test]
fn t01_new_rig_earns_nothing_accrued_before_it_existed() {
    let mut w = World::new(2, CENTI, 0);
    w.fund(100 * SOL, 1_000).unwrap();

    w.buy(0, 10).unwrap();
    w.warp(500); // miner 0 alone for 500s

    let alone = w.earned(0);
    assert!(alone > 0);

    // Miner 1 arrives now.
    w.buy(1, 10).unwrap();
    assert_eq!(w.earned(1), 0, "a brand new rig must start at zero");

    // Miner 0's accrual is untouched by the newcomer's arrival.
    assert_eq!(w.earned(0), alone);

    // And after buying more rigs itself, miner 0 keeps its prior accrual and
    // its new rigs start from the current watermark.
    let before = w.earned(0);
    w.buy(0, 10).unwrap();
    assert_eq!(w.earned(0), before);
}

/// Test 2 — two miners with rigs in ratio N:M receive rewards in ratio N:M.
#[test]
fn t02_rewards_split_in_rig_ratio() {
    for (n, m) in [(1u64, 1u64), (1, 3), (7, 5), (42, 1)] {
        let mut w = World::new(2, CENTI, 0);
        w.fund(1_000 * SOL, 100_000).unwrap();
        w.buy(0, n).unwrap();
        w.buy(1, m).unwrap();
        w.warp(50_000);

        let a = w.earned(0) as u128;
        let b = w.earned(1) as u128;

        // a/b == n/m, allowing only integer-truncation dust.
        let lhs = a * m as u128;
        let rhs = b * n as u128;
        let diff = lhs.abs_diff(rhs);
        assert!(
            diff <= (n as u128 + m as u128) * PRECISION,
            "ratio {n}:{m} skewed: {a} vs {b}"
        );
    }
}

/// Test 3 — `claim` transfers exactly `earned`; post-claim `claimable == 0`;
/// rigs unchanged.
#[test]
fn t03_claim_transfers_exactly_earned() {
    let mut w = World::new(1, CENTI, 0);
    w.fund(100 * SOL, 1_000).unwrap();
    w.buy(0, 25).unwrap();
    w.warp(400);
    // Past the claim-penalty window, so the claim pays everything earned.
    w.warp(3 * 86_400);

    let expected = w.earned(0);
    let rigs_before = w.miners[0].rigs;
    let wallet_before = w.wallets[0];
    let vault_before = w.reward_vault;

    let paid = w.claim(0).unwrap();

    assert_eq!(paid, expected, "claim must pay exactly the earned figure");
    assert_eq!(w.wallets[0], wallet_before + expected);
    assert_eq!(w.reward_vault, vault_before - expected);
    assert_eq!(w.earned(0), 0, "claimable must be zero right after a claim");
    assert_eq!(w.miners[0].rigs, rigs_before, "rigs must be unchanged");
    assert_eq!(w.ledger.total_reward_claimed, expected);
}

/// Test 4 — a double-claim in the same slot transfers nothing the second time.
#[test]
fn t04_double_claim_in_same_slot_pays_nothing() {
    let mut w = World::new(1, CENTI, 0);
    w.fund(100 * SOL, 1_000).unwrap();
    w.buy(0, 10).unwrap();
    w.warp(300);

    // Inside the penalty window, as a sole planter: the penalty goes to dust,
    // never back to the claimer, so the second claim still finds nothing.
    let first = w.claim(0).unwrap();
    assert!(first > 0);

    let vault_after_first = w.reward_vault;
    let wallet_after_first = w.wallets[0];

    // Same timestamp — no time has passed.
    let second = w.claim(0);
    assert_eq!(second, Err(EngineError::NothingToClaim));
    assert_eq!(w.reward_vault, vault_after_first, "no lamports may move");
    assert_eq!(w.wallets[0], wallet_after_first);
    assert_eq!(w.ledger.total_reward_claimed, first);
}

/// Test 5 — `compound` buys `floor(pending / cost_per_rig)` rigs and preserves
/// the exact remainder. This is the worked example from spec §6.
#[test]
fn t05_compound_worked_example_from_spec() {
    let mut w = World::new(1, CENTI, 0);
    w.fund(1_000 * SOL, 10_000).unwrap();
    w.buy(0, 1).unwrap();

    // Force the ore bin to exactly 0.128 SOL.
    w.miners[0].pending_rewards = 128 * SOL / 1000;
    let pending = w.miners[0].pending_rewards;
    assert_eq!(pending, 128_000_000);

    let out = w.compound(0).unwrap();

    assert_eq!(out.rigs_minted, 12, "0.128 / 0.01 -> 12 whole rigs");
    assert_eq!(out.total_lamports, 120_000_000, "12 rigs cost 0.120 SOL");
    assert_eq!(out.remainder_lamports, 8_000_000, "0.008 SOL remains");
    assert_eq!(
        w.miners[0].pending_rewards, 8_000_000,
        "the remainder must never be discarded"
    );
    assert_eq!(w.miners[0].rigs, 13);
}

/// Test 5 (fee variant) — the affordability search is exact once a fee is on
/// top, for a wide sweep of budgets and fee rates.
#[test]
fn t05b_compound_affordability_is_exact_with_fees() {
    for fee_bps in [0u16, 1, 37, 300, 999, 1000] {
        for price in [1u64, 7, CENTI, CENTI + 1, 3_333_333] {
            for budget in [
                0u64,
                1,
                price - 1,
                price,
                price + 1,
                price * 12,
                price * 12 + 7,
                999_999_999,
                123_456_789_012,
            ] {
                let n = rigs_affordable(budget, price, fee_bps).unwrap();

                if n > 0 {
                    let q = quote_purchase(n, price, fee_bps).unwrap();
                    assert!(
                        q.total_lamports <= budget,
                        "n={n} over budget: {} > {budget} (price={price}, bps={fee_bps})",
                        q.total_lamports
                    );
                }
                if n < MAX_RIGS_PER_PURCHASE {
                    let q = quote_purchase(n + 1, price, fee_bps).unwrap();
                    assert!(
                        q.total_lamports > budget,
                        "n+1={} should not fit in {budget} (price={price}, bps={fee_bps})",
                        n + 1
                    );
                }
            }
        }
    }
}

/// Test 6 — `compound` moves the corresponding lamports out of the reward
/// vault into capital and fee; the reward vault drops by exactly the amount
/// spent.
#[test]
fn t06_compound_moves_real_lamports_between_vaults() {
    let mut w = World::new(1, CENTI, 300); // 3.00% fee
    w.fund(500 * SOL, 5_000).unwrap();
    w.buy(0, 20).unwrap();
    w.warp(2_500);

    let reward_before = w.reward_vault;
    let capital_before = w.capital_vault;
    let fee_before = w.fee_vault;
    let pending_before = w.earned(0);

    let out = w.compound(0).unwrap();

    assert_eq!(
        w.reward_vault,
        reward_before - out.total_lamports,
        "reward vault must drop by exactly the spent amount"
    );
    assert_eq!(w.capital_vault, capital_before + out.capital_lamports);
    assert_eq!(w.fee_vault, fee_before + out.fee_lamports);
    assert_eq!(out.capital_lamports + out.fee_lamports, out.total_lamports);
    assert_eq!(
        pending_before - out.total_lamports,
        out.remainder_lamports,
        "remainder must be pending minus spent, exactly"
    );
    assert!(out.fee_lamports > 0, "a 3% fee must actually be charged");
}

/// Test 7 / 8 — over a long interleaved run, distributed value never exceeds
/// funded value and the vault always backs its liabilities.
///
/// `World::check_invariants` asserts both after *every* action; this drives a
/// deterministic pseudo-random interleaving through it.
#[test]
fn t07_t08_interleaved_actions_never_break_funding_invariants() {
    let mut w = World::new(4, CENTI, 250);
    w.fund(250 * SOL, 20_000).unwrap();

    let mut seed: u64 = 0x5EED_1234_ABCD_0001;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    for step in 0..4_000u64 {
        let r = next();
        let who = (r % 4) as usize;
        w.warp(((r >> 8) % 37) as i64);

        match (r >> 16) % 10 {
            0..=3 => {
                let _ = w.buy(who, 1 + (r >> 32) % 50);
            }
            4..=6 => {
                let _ = w.claim(who);
            }
            7..=8 => {
                let _ = w.compound(who);
            }
            _ => {
                if step % 400 == 0 {
                    let _ = w.fund(10 * SOL, 5_000);
                }
            }
        }
    }

    // Drain everything (past the claim-penalty window) and confirm the books
    // close.
    w.warp(4 * 86_400);
    for i in 0..4 {
        let _ = w.claim(i);
    }
    assert_eq!(w.total_earned(), 0);
    assert!(w.ledger.outstanding().unwrap() >= w.globals.undistributed_dust);
}

// ------------------------------------------------------------------- Funding

/// Test 9 — `fund_rewards` mid-schedule rolls leftover into the new rate.
#[test]
fn t09_funding_mid_schedule_rolls_leftover() {
    let mut w = World::new(1, CENTI, 0);
    w.buy(0, 1).unwrap();

    w.fund(1_000, 100).unwrap();
    assert_eq!(w.globals.reward_rate, 10);

    // Halfway through: 50s * 10 = 500 lamports of leftover emission.
    w.warp(50);
    w.fund(500, 100).unwrap();

    // (500 leftover + 500 new) / 100s = 10 lamports/s.
    assert_eq!(w.globals.reward_rate, 10);
    assert_eq!(w.globals.reward_period_finish, w.now + 100);
    assert_eq!(w.ledger.total_reward_funded, 1_500);

    // A larger top-up raises the rate, leftover included.
    w.warp(50);
    w.fund(1_000, 100).unwrap();
    assert_eq!(w.globals.reward_rate, (500 + 1_000) / 100);
}

/// Test 10 — the schedule is never written against backing that is not there.
///
/// The engine's `fund` is schedule-only by design; the handler must move
/// lamports first and then assert. This test drives the assertion directly:
/// a schedule written without the matching transfer is rejected.
#[test]
fn t10_funding_without_backing_is_rejected() {
    let mut globals = RewardGlobals::default();
    let mut ledger = RewardLedger::default();

    globals.fund(&mut ledger, 0, 100 * SOL, 1_000).unwrap();

    // Vault holds nothing: the invariant must fail.
    assert_eq!(
        assert_fully_funded(0, &ledger),
        Err(EngineError::InsufficientRewardBacking)
    );
    // One lamport short is still short.
    assert_eq!(
        assert_fully_funded(100 * SOL - 1, &ledger),
        Err(EngineError::InsufficientRewardBacking)
    );
    // Exactly backed is fine.
    assert_eq!(assert_fully_funded(100 * SOL, &ledger), Ok(()));
}

/// Test 11 — after `period_finish`, accrual stops.
#[test]
fn t11_accrual_stops_at_period_finish() {
    let mut w = World::new(1, CENTI, 0);
    w.buy(0, 10).unwrap();
    w.fund(1_000 * SOL, 1_000).unwrap();

    w.warp(1_000); // exactly to the finish line
    let at_finish = w.earned(0);

    w.warp(10_000_000); // long past it
    assert_eq!(
        w.earned(0),
        at_finish,
        "no rewards may accrue past the funded window"
    );

    let claimed = w.claim(0).unwrap();
    assert_eq!(claimed, at_finish);
    assert!(claimed <= 1_000 * SOL);
}

/// Test 12 — a window with `total_rigs == 0` creates no unattributable
/// liabilities. Documented choice: the elapsed slice is credited to dust.
#[test]
fn t12_zero_rig_window_credits_dust_and_attributes_nothing() {
    assert_eq!(ZERO_RIG_POLICY, "credited-to-dust");

    let mut w = World::new(1, CENTI, 0);
    w.fund(1_000, 1_000).unwrap(); // 1 lamport/s
    assert_eq!(w.globals.reward_rate, 1);

    // 400 seconds with no rigs in existence at all.
    w.warp(400);
    w.buy(0, 1).unwrap();

    assert_eq!(w.globals.undistributed_dust, 400);
    assert_eq!(
        w.globals.reward_per_rig_stored, 0,
        "no accumulator movement while nobody could earn"
    );
    assert_eq!(w.earned(0), 0);

    // The remaining 600s accrue to the sole miner and nothing more.
    w.warp(600);
    assert_eq!(w.earned(0), 600);

    w.warp(300_000); // past the 72h claim-penalty window
    assert_eq!(w.earned(0), 600);
    let claimed = w.claim(0).unwrap();
    assert_eq!(claimed, 600);

    // The dust is still sitting in the vault, attributed to nobody, and V1
    // ships no way to remove it.
    assert_eq!(w.reward_vault, 400);
    assert_eq!(w.total_earned(), 0);
}

// ---------------------------------------------------------------- Separation

/// Test 13 — no path moves purchase lamports into the reward vault, proven by
/// balance accounting across a full buy -> accrue -> claim cycle.
#[test]
fn t13_purchase_lamports_never_reach_the_reward_vault() {
    let mut w = World::new(2, CENTI, 300);
    w.fund(10 * SOL, 1_000).unwrap();
    let reward_after_funding = w.reward_vault;

    // A large purchase by user B while user A is accruing.
    w.buy(0, 5).unwrap();
    w.warp(200);
    let q = w.buy(1, 5_000).unwrap();
    assert!(q.total_lamports > 50 * SOL);

    assert_eq!(
        w.reward_vault, reward_after_funding,
        "a purchase must not add one lamport of reward backing"
    );
    assert_eq!(w.capital_vault, 5_005 * CENTI);
    assert_eq!(w.fee_vault, 5_005 * CENTI * 300 / 10_000);

    // User A now claims. Everything they receive traces to funded lamports.
    w.warp(800);
    let claimed = w.claim(0).unwrap();
    assert!(claimed > 0);
    assert!(
        w.ledger.total_reward_claimed <= w.ledger.total_reward_funded,
        "claims can only ever draw on funded value"
    );
    assert_eq!(
        w.funded_in,
        10 * SOL,
        "the funder is the only reward source"
    );
}

// ------------------------------------------------------------- Checked math

/// Test 19 — checked math everywhere; realistic and extreme inputs error out
/// rather than wrapping.
#[test]
fn t19_extreme_inputs_error_rather_than_wrap() {
    // Config validation.
    assert_eq!(validate_pricing(0, 0), Err(EngineError::InvalidConfig));
    assert_eq!(
        validate_pricing(1, MAX_PURCHASE_FEE_BPS + 1),
        Err(EngineError::InvalidConfig)
    );

    // Rig count bounds.
    assert_eq!(
        quote_purchase(0, CENTI, 0),
        Err(EngineError::InvalidRigCount)
    );
    assert_eq!(
        quote_purchase(MAX_RIGS_PER_PURCHASE + 1, CENTI, 0),
        Err(EngineError::InvalidRigCount)
    );

    // Quote at the top of the allowed range still fits in u64.
    let q = quote_purchase(
        MAX_RIGS_PER_PURCHASE,
        u64::MAX / MAX_RIGS_PER_PURCHASE / 2,
        1_000,
    )
    .unwrap();
    assert_eq!(q.total_lamports, q.capital_lamports + q.fee_lamports);

    // Global rig cap.
    let mut g = RewardGlobals::default();
    let mut p = MinerPosition::default();
    g.total_rigs = MAX_TOTAL_RIGS;
    assert_eq!(
        buy_rigs(&mut g, &mut p, 0, 1, CENTI, 0),
        Err(EngineError::RigSupplyExceeded)
    );

    // Funding bounds.
    let mut g = RewardGlobals::default();
    let mut l = RewardLedger::default();
    assert_eq!(
        g.fund(&mut l, 0, 0, 100),
        Err(EngineError::InvalidFundingAmount)
    );
    assert_eq!(g.fund(&mut l, 0, 1, 0), Err(EngineError::InvalidDuration));
    assert_eq!(
        g.fund(&mut l, 0, 1, MAX_REWARD_DURATION_SECONDS + 1),
        Err(EngineError::InvalidDuration)
    );
    // Dust funding that would floor the rate to zero is rejected, not silently
    // accepted as a stalled schedule.
    assert_eq!(g.fund(&mut l, 0, 99, 100), Err(EngineError::RewardRateZero));
    // Absurd rate is rejected.
    assert_eq!(
        g.fund(&mut l, 0, u64::MAX, 1),
        Err(EngineError::MathOverflow)
    );

    // The accumulator survives the pessimistic bound used to pick PRECISION:
    // ~31 years of elapsed time at 1000 SOL/second with a single rig.
    let mut g = RewardGlobals {
        total_rigs: 1,
        total_shares: WEIGHT_BASE,
        reward_rate: MAX_REWARD_RATE_LAMPORTS_PER_SECOND,
        reward_period_finish: i64::MAX,
        reward_last_update: 0,
        reward_per_rig_stored: 0,
        undistributed_dust: 0,
    };
    g.sync(1_000_000_000).unwrap();
    assert!(g.reward_per_rig_stored > 0);

    // A ledger can never report negative outstanding value.
    let bad = RewardLedger {
        total_reward_funded: 10,
        total_reward_claimed: 11,
        total_reward_compounded: 0,
    };
    assert_eq!(bad.outstanding(), Err(EngineError::MathOverflow));
}

/// Test 29 (engine half) — the quoted total is exactly what gets debited.
#[test]
fn t29_quote_matches_debit() {
    let mut w = World::new(1, CENTI, 300);
    let before = w.wallets[0];
    let q = w.buy(0, 10).unwrap();

    assert_eq!(q.capital_lamports, 100_000_000);
    assert_eq!(q.fee_lamports, 3_000_000);
    assert_eq!(q.total_lamports, 103_000_000);
    assert_eq!(before - w.wallets[0], q.total_lamports);
    assert_eq!(w.capital_vault + w.fee_vault, q.total_lamports);
}

/// The engine must never produce value out of rounding. Repeated tiny syncs
/// must never out-earn one large sync over the same window.
#[test]
fn rounding_always_favours_the_vault() {
    let run = |chunk: i64| {
        let mut w = World::new(1, CENTI, 0);
        w.buy(0, 7).unwrap();
        w.fund(1_000_000, 10_000).unwrap();
        let mut t = 0;
        while t < 10_000 {
            w.warp(chunk);
            let mut g = w.globals;
            g.sync(w.now).unwrap();
            w.globals = g;
            t += chunk;
        }
        w.earned(0)
    };

    let one_shot = run(10_000);
    for chunk in [1, 3, 7, 100, 999] {
        let chunked = run(chunk);
        assert!(
            chunked <= one_shot,
            "chunked sync every {chunk}s produced {chunked} > {one_shot}"
        );
    }
}
