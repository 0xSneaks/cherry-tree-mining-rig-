//! Permanent 250-seed wallet cap and capped compound — engine level.
//!
//! Architecture §4 check 2: every purchase and compound preserves
//! `rigs <= MAX_WALLET_RIGS`, enforced inside the pure engine so every
//! on-chain path inherits it. Frozen-architecture tests 1, 2, 3, 4 (engine
//! part), 5, 10 and 11 (engine part).

mod common;

use common::{World, SOL};
use proptest::prelude::*;
use rigs_engine::*;

const PRICE: u64 = SOL / 100; // 0.01 SOL — fixture only
const FEE: u16 = 300;

/// A world whose miner 0 holds exactly `rigs` bought seeds, with a live
/// reward schedule.
fn world_at(rigs: u64) -> World {
    let mut w = World::new(2, PRICE, FEE);
    w.fund(10_000 * SOL, 1_000_000).unwrap();
    if rigs > 0 {
        w.buy(0, rigs).unwrap();
    }
    w
}

// ------------------------------------------------------------- fresh buys

#[test]
fn cap_constant_is_250() {
    assert_eq!(MAX_WALLET_RIGS, 250);
    assert_eq!(wallet_headroom(0), 250);
    assert_eq!(wallet_headroom(248), 2);
    assert_eq!(wallet_headroom(250), 0);
    assert_eq!(wallet_headroom(400), 0);
}

/// Test 1: S=248 + 2 succeeds; S=249 + 2 rejects with nothing changed.
#[test]
fn buy_to_exactly_the_cap_succeeds_and_one_over_rejects_atomically() {
    let mut w = world_at(248);
    w.buy(0, 2).unwrap();
    assert_eq!(w.miners[0].rigs, 250);

    let mut w = world_at(249);
    w.warp(3_600);
    let (m, g, wallet, cap, fee) = (
        w.miners[0],
        w.globals,
        w.wallets[0],
        w.capital_vault,
        w.fee_vault,
    );
    assert_eq!(w.buy(0, 2), Err(EngineError::WalletCapExceeded));
    // Not settled, not charged, not counted: bit-identical state.
    assert_eq!(w.miners[0], m);
    assert_eq!(w.globals, g);
    assert_eq!(
        (w.wallets[0], w.capital_vault, w.fee_vault),
        (wallet, cap, fee)
    );
}

#[test]
fn at_the_cap_every_buy_size_rejects() {
    let mut w = world_at(250);
    for n in [1, 2, 100, MAX_RIGS_PER_PURCHASE] {
        assert_eq!(w.buy(0, n), Err(EngineError::WalletCapExceeded));
    }
    assert_eq!(w.miners[0].rigs, 250);
}

#[test]
fn a_single_buy_above_the_cap_from_zero_rejects() {
    let mut w = world_at(0);
    assert_eq!(w.buy(0, 251), Err(EngineError::WalletCapExceeded));
    assert_eq!(w.miners[0], MinerPosition::default());
    w.buy(0, 250).unwrap();
}

#[test]
fn invalid_counts_keep_their_own_error() {
    let mut w = world_at(0);
    assert_eq!(w.buy(0, 0), Err(EngineError::InvalidRigCount));
    assert_eq!(
        w.buy(0, MAX_RIGS_PER_PURCHASE + 1),
        Err(EngineError::InvalidRigCount)
    );
}

// --------------------------------------------------------- capped compound

/// Rewards that would buy exactly `n` whole seeds (plus `extra` lamports).
fn pending_for(n: u64, extra: u64) -> u64 {
    quote_purchase(n, PRICE, FEE).unwrap().total_lamports + extra
}

/// Direct engine fixture: a position with `rigs` and an explicit ore bin, and
/// globals with no emission running (settle is a no-op).
fn fixture(rigs: u64, pending: u64, weight: u64) -> (RewardGlobals, MinerPosition, RewardLedger) {
    let globals = RewardGlobals {
        total_rigs: rigs,
        total_shares: rigs * weight,
        reward_last_update: 1_000,
        reward_period_finish: 1_000,
        ..Default::default()
    };
    let position = MinerPosition {
        rigs,
        pending_rewards: pending,
        weight,
        last_planted_at: 500,
        ..Default::default()
    };
    (globals, position, RewardLedger::default())
}

/// Test 2: S=240 with rewards for 50 mints exactly 10, debits exactly
/// quote(10) and leaves the rest claimable.
#[test]
fn compound_at_240_with_rewards_for_50_mints_exactly_10() {
    let pending = pending_for(50, 7);
    let (mut g, mut p, mut l) = fixture(240, pending, WEIGHT_BASE);
    assert_eq!(rigs_affordable(pending, PRICE, FEE).unwrap(), 50);

    let out = compound(&mut g, &mut p, &mut l, 1_000, PRICE, FEE).unwrap();
    let q10 = quote_purchase(10, PRICE, FEE).unwrap();

    assert_eq!(out.rigs_minted, 10);
    assert_eq!(out.capital_lamports, q10.capital_lamports);
    assert_eq!(out.fee_lamports, q10.fee_lamports);
    assert_eq!(out.total_lamports, q10.total_lamports);
    assert_eq!(out.remainder_lamports, pending - q10.total_lamports);
    assert_eq!(p.pending_rewards, pending - q10.total_lamports);
    assert_eq!(p.rigs, 250);
    assert_eq!(g.total_rigs, 250);
    assert_eq!(l.total_reward_compounded, q10.total_lamports);
    // Compound never moves the claim-penalty anchor.
    assert_eq!(p.last_planted_at, 500);
}

/// Test 3: at S=250, compound creates no seed and charges no fee — the
/// engine refuses before settling, and on-chain the revert leaves every
/// account, pending reward and penalty anchor unchanged.
#[test]
fn compound_at_the_cap_mints_nothing_and_charges_nothing() {
    let (mut g, mut p, mut l) = fixture(250, pending_for(50, 0), WEIGHT_BASE);
    let (g0, p0, l0) = (g, p, l);
    assert_eq!(
        compound(&mut g, &mut p, &mut l, 1_000, PRICE, FEE),
        Err(EngineError::WalletCapExceeded)
    );
    assert_eq!((g, p, l), (g0, p0, l0));
}

#[test]
fn compound_at_the_cap_leaves_rewards_claimable_through_the_world() {
    let mut w = world_at(250);
    w.warp(10_000);
    let reward_before = w.reward_vault;
    let earned = w.earned(0);
    assert!(earned > 0);

    assert_eq!(w.compound(0).unwrap_err(), EngineError::WalletCapExceeded);
    assert_eq!(w.reward_vault, reward_before);
    assert_eq!(w.miners[0].rigs, 250);

    // The earned SOL is still owed and claimable.
    w.warp(PENALTY_WINDOW_SECONDS);
    let gross = w.claim_outcome(0).unwrap().gross;
    assert!(gross >= earned);
}

#[test]
fn compound_below_headroom_is_unchanged() {
    // Headroom 100, rewards for 5: mints 5, exactly as before the cap.
    let pending = pending_for(5, 3);
    let (mut g, mut p, mut l) = fixture(150, pending, WEIGHT_BASE);
    let out = compound(&mut g, &mut p, &mut l, 1_000, PRICE, FEE).unwrap();
    assert_eq!(out.rigs_minted, 5);
    assert_eq!(p.rigs, 155);
}

#[test]
fn compound_with_headroom_but_too_little_reward_is_nothing_to_compound() {
    let (mut g, mut p, mut l) = fixture(200, pending_for(1, 0) - 1, WEIGHT_BASE);
    assert_eq!(
        compound(&mut g, &mut p, &mut l, 1_000, PRICE, FEE),
        Err(EngineError::NothingToCompound)
    );
}

/// Test 5: a referred wallet (weight 105) filled to the cap by buy and
/// compound contributes exactly 250 × 105 shares.
#[test]
fn referred_wallet_at_the_cap_has_exactly_250_x_105_shares() {
    let (mut g, mut p, mut l) = fixture(240, pending_for(50, 0), WEIGHT_REFERRED);
    compound(&mut g, &mut p, &mut l, 1_000, PRICE, FEE).unwrap();
    assert_eq!(p.rigs, 250);
    assert_eq!(p.shares().unwrap(), 250 * 105);
    assert_eq!(g.total_shares, 250 * 105);

    // A further buy is refused and shares do not move.
    assert_eq!(
        buy_rigs(&mut g, &mut p, 1_000, 1, PRICE, FEE),
        Err(EngineError::WalletCapExceeded)
    );
    assert_eq!(g.total_shares, 250 * 105);
}

// --------------------------------------------------------- legacy (S > 250)

/// Test 4 (engine part): a legacy position above the cap is frozen, never
/// clipped. Buy and compound refuse, claims keep working, shares unchanged.
#[test]
fn legacy_position_above_the_cap_is_frozen_not_clipped() {
    let (mut g, mut p, mut l) = fixture(400, pending_for(20, 0), WEIGHT_BASE);
    let shares = g.total_shares;

    assert_eq!(
        buy_rigs(&mut g, &mut p, 1_000, 1, PRICE, FEE),
        Err(EngineError::WalletCapExceeded)
    );
    assert_eq!(
        compound(&mut g, &mut p, &mut l, 1_000, PRICE, FEE),
        Err(EngineError::WalletCapExceeded)
    );
    assert_eq!((p.rigs, g.total_shares), (400, shares));

    let pending = p.pending_rewards;
    let out = claim(&mut g, &mut p, &mut l, 1_000 + PENALTY_WINDOW_SECONDS).unwrap();
    assert_eq!(out.gross, pending);
    assert_eq!(p.rigs, 400);
}

// ---------------------------------------------------------------- property

#[derive(Debug, Clone, Copy)]
enum Action {
    Buy(u64),
    Compound,
    Claim,
    Warp(i64),
}

fn action() -> impl Strategy<Value = Action> {
    prop_oneof![
        4 => (1u64..300).prop_map(Action::Buy),
        4 => Just(Action::Compound),
        1 => Just(Action::Claim),
        4 => (0i64..200_000).prop_map(Action::Warp),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Test 10: no sequence of buy/compound takes a wallet from S <= 250 to
    /// S > 250. `World::check_invariants` also asserts the cap after every
    /// action, for this and every other engine suite.
    #[test]
    fn no_sequence_of_buys_and_compounds_exceeds_the_cap(
        price in 1u64..(SOL / 100),
        fee in 0u16..=1000,
        actions in prop::collection::vec(action(), 1..150),
    ) {
        let mut w = World::new(1, price, fee);
        w.fund(1_000 * SOL, 500_000).unwrap();
        for a in actions {
            let before = w.miners[0].rigs;
            match a {
                Action::Buy(n) => {
                    let r = w.buy(0, n);
                    if before + n > MAX_WALLET_RIGS {
                        prop_assert_eq!(r.unwrap_err(), EngineError::WalletCapExceeded);
                        prop_assert_eq!(w.miners[0].rigs, before);
                    }
                }
                Action::Compound => {
                    if let Ok(out) = w.compound(0) {
                        prop_assert!(out.rigs_minted <= MAX_WALLET_RIGS - before);
                    }
                }
                Action::Claim => { let _ = w.claim(0); }
                Action::Warp(s) => w.warp(s),
            }
            prop_assert!(w.miners[0].rigs <= MAX_WALLET_RIGS);
        }
    }
}
