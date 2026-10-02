//! Permanent 250-seed wallet cap and capped compound — through real
//! instructions on a real runtime. Frozen-architecture tests 1, 2, 3, 5, 9,
//! 10 and 11.

mod common;

use common::*;
use sol_rigs::error::RigsError;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signature::Signer};

const DAY: i64 = 86_400;
const C: u64 = WALLET_CAP;

fn quote(n: u64) -> rigs_engine::PurchaseQuote {
    rigs_engine::quote_purchase(n, RIG_PRICE, FEE_BPS).unwrap()
}

/// Initialized, funded, activated by the founder with `activation` rigs, and
/// warped to Public (T0 + 24h) so the Genesis check is out of the way.
async fn public_env(activation: u64) -> Env {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    env.fund_rewards(1_000 * SOL, 30 * DAY as u64)
        .await
        .unwrap();
    let f = env.founder.insecure_clone();
    env.buy(&f, activation).await.unwrap();
    let t0 = env.config().await.launch_t0;
    env.set_time(t0 + DAY).await;
    env
}

/// Every balance and account a rejected purchase or compound must leave
/// untouched.
struct Snapshot {
    wallet: u64,
    capital: u64,
    fee: u64,
    reward: u64,
    config: Vec<u8>,
    miner: Option<Vec<u8>>,
}

async fn snapshot(env: &mut Env, who: &Pubkey) -> Snapshot {
    Snapshot {
        wallet: env.lamports(*who).await,
        capital: env.lamports(capital_vault_pda()).await,
        fee: env.lamports(fee_vault_pda()).await,
        reward: env.lamports(reward_vault_pda()).await,
        config: env.account_raw(config_pda()).await.unwrap().data,
        miner: env.account_raw(miner_pda(who)).await.map(|a| a.data),
    }
}

async fn assert_unchanged(env: &mut Env, who: &Pubkey, s: &Snapshot) {
    let now = snapshot(env, who).await;
    assert_eq!(now.wallet, s.wallet, "wallet lamports moved");
    assert_eq!(now.capital, s.capital, "capital vault moved");
    assert_eq!(now.fee, s.fee, "fee vault moved");
    assert_eq!(now.reward, s.reward, "reward vault moved");
    assert_eq!(now.config, s.config, "config bytes changed");
    assert_eq!(now.miner, s.miner, "miner bytes changed");
}

// ------------------------------------------------------------ fresh buys

/// Test 1: S=248 + 2 succeeds; S=249 + 2 reverts atomically with no lamport
/// movement and no state change.
#[tokio::test]
async fn buy_248_plus_2_succeeds_and_249_plus_2_rejects_atomically() {
    let mut env = public_env(10).await;
    let erin = env.erin.insecure_clone();
    env.buy(&erin, 248).await.unwrap();
    env.buy(&erin, 2).await.unwrap();
    assert_eq!(env.miner(&erin.pubkey()).await.rigs, C);

    let frank = env.frank.insecure_clone();
    env.buy(&frank, 249).await.unwrap();
    env.warp(600).await;
    let s = snapshot(&mut env, &frank.pubkey()).await;
    assert_rigs_error(
        env.buy(&frank, 2).await.unwrap_err(),
        RigsError::WalletCapExceeded,
    );
    assert_unchanged(&mut env, &frank.pubkey(), &s).await;
    // Exactly the headroom still fits.
    env.buy(&frank, 1).await.unwrap();
    assert_eq!(env.miner(&frank.pubkey()).await.rigs, C);
}

#[tokio::test]
async fn at_the_cap_every_buy_rejects_and_a_new_wallet_cannot_open_above_it() {
    let mut env = public_env(10).await;
    let erin = env.erin.insecure_clone();
    env.buy(&erin, C).await.unwrap();
    for n in [1, 2, 100] {
        let s = snapshot(&mut env, &erin.pubkey()).await;
        assert_rigs_error(
            env.buy(&erin, n).await.unwrap_err(),
            RigsError::WalletCapExceeded,
        );
        assert_unchanged(&mut env, &erin.pubkey(), &s).await;
    }

    // First purchase above the cap: no Miner account is left behind.
    let frank = env.frank.insecure_clone();
    let s = snapshot(&mut env, &frank.pubkey()).await;
    assert!(s.miner.is_none());
    assert_rigs_error(
        env.buy(&frank, C + 1).await.unwrap_err(),
        RigsError::WalletCapExceeded,
    );
    assert_unchanged(&mut env, &frank.pubkey(), &s).await;
}

/// Day 1 too: the permanent cap and the Genesis cap both hold.
#[tokio::test]
async fn day1_purchases_obey_the_cap() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    env.fund_rewards(1_000 * SOL, 30 * DAY as u64)
        .await
        .unwrap();
    let f = env.founder.insecure_clone();
    env.buy(&f, 10).await.unwrap();
    let carol = env.carol.insecure_clone();
    env.buy(&carol, 248).await.unwrap();
    env.buy(&carol, 2).await.unwrap();
    let err = env.buy(&carol, 1).await.unwrap_err();
    // Day 1: the Genesis check runs first; both caps refuse it.
    assert_rigs_error(err, RigsError::GenesisCapExceeded);
    assert_eq!(env.miner(&carol.pubkey()).await.rigs, C);
}

// --------------------------------------------------------- capped compound

/// Test 2: S=240 with rewards for at least 50 seeds mints exactly 10, moves
/// exactly quote(10) out of the reward vault, and leaves the rest claimable.
#[tokio::test]
async fn compound_at_240_with_rewards_for_50_mints_exactly_10() {
    let mut env = public_env(10).await;
    let erin = env.erin.insecure_clone();
    env.buy(&erin, 240).await.unwrap();
    env.warp(DAY).await; // ~33 SOL/day emission, erin holds 240/250 of it

    let before = env.miner(&erin.pubkey()).await;
    let (reward0, capital0, fee0) = (
        env.lamports(reward_vault_pda()).await,
        env.lamports(capital_vault_pda()).await,
        env.lamports(fee_vault_pda()).await,
    );
    env.compound(&erin).await.unwrap();
    let after = env.miner(&erin.pubkey()).await;
    let q = quote(10);

    assert_eq!(after.rigs, C);
    assert_eq!(
        after.lifetime_rigs_compounded,
        before.lifetime_rigs_compounded + 10
    );
    assert_eq!(
        env.lamports(reward_vault_pda()).await,
        reward0 - q.total_lamports
    );
    assert_eq!(
        env.lamports(capital_vault_pda()).await,
        capital0 + q.capital_lamports
    );
    assert_eq!(env.lamports(fee_vault_pda()).await, fee0 + q.fee_lamports);
    assert_eq!(after.lifetime_sol_compounded, q.total_lamports);
    // Rewards for at least 50 seeds existed; 40+ seeds' worth stays claimable.
    assert!(after.pending_rewards >= quote(40).total_lamports);
    // Compound never moves the claim-penalty anchor.
    assert_eq!(after.last_planted_at, before.last_planted_at);

    // The remainder is really claimable.
    env.warp(3 * DAY).await;
    let wallet0 = env.lamports(erin.pubkey()).await;
    env.claim(&erin).await.unwrap();
    assert!(env.lamports(erin.pubkey()).await - wallet0 >= after.pending_rewards);
}

/// Test 3: compound at S=250 creates no seed, charges no fee, moves no
/// lamport and changes no account — pending rewards and the penalty anchor
/// included.
#[tokio::test]
async fn compound_at_the_cap_is_a_clean_revert() {
    let mut env = public_env(10).await;
    let erin = env.erin.insecure_clone();
    env.buy(&erin, C).await.unwrap();
    env.warp(DAY).await;

    let s = snapshot(&mut env, &erin.pubkey()).await;
    assert_rigs_error(
        env.compound(&erin).await.unwrap_err(),
        RigsError::WalletCapExceeded,
    );
    assert_unchanged(&mut env, &erin.pubkey(), &s).await;

    // Earned SOL stays owed and claimable.
    env.warp(3 * DAY).await;
    let wallet0 = env.lamports(erin.pubkey()).await;
    env.claim(&erin).await.unwrap();
    assert!(env.lamports(erin.pubkey()).await > wallet0);
    assert_eq!(env.miner(&erin.pubkey()).await.rigs, C);
}

// ------------------------------------------------------------- referrals

/// Test 5: a referred wallet (weight 105) filled to the cap — by buy, then
/// by compound — contributes exactly 250 × 105 to `total_shares`.
#[tokio::test]
async fn referred_wallet_at_the_cap_has_exactly_250_x_105_shares() {
    let mut env = public_env(10).await; // alice (founder): 10 base rigs
    let (alice, erin) = (env.alice.insecure_clone(), env.erin.insecure_clone());
    let ix = env.buy_ix_with(
        &erin.pubkey(),
        240,
        vec![],
        Some((alice.pubkey(), miner_pda(&alice.pubkey()))),
    );
    env.send(ix, &[&erin]).await.unwrap();
    assert_eq!(env.miner(&erin.pubkey()).await.weight, 105);

    env.warp(DAY).await;
    env.compound(&erin).await.unwrap();
    let m = env.miner(&erin.pubkey()).await;
    assert_eq!(m.rigs, C);
    let cfg = env.config().await;
    assert_eq!(cfg.total_shares, 10 * 100 + 250 * 105);

    assert_rigs_error(
        env.buy(&erin, 1).await.unwrap_err(),
        RigsError::WalletCapExceeded,
    );
    assert_rigs_error(
        env.compound(&erin).await.unwrap_err(),
        RigsError::WalletCapExceeded,
    );
    assert_eq!(env.config().await.total_shares, 10 * 100 + 250 * 105);
    // Referring grants the referrer no seeds.
    assert_eq!(env.miner(&alice.pubkey()).await.rigs, 10);
}

// ---------------------------------------------------- founder activation

/// Test 9: the activation purchase counts against both the Genesis cap and
/// the permanent cap.
#[tokio::test]
async fn activation_counts_against_both_caps() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    env.fund_rewards(1_000 * SOL, 30 * DAY as u64)
        .await
        .unwrap();
    let f = env.founder.insecure_clone();

    let s = snapshot(&mut env, &f.pubkey()).await;
    assert_rigs_error(
        env.buy(&f, C + 1).await.unwrap_err(),
        RigsError::GenesisCapExceeded,
    );
    assert_unchanged(&mut env, &f.pubkey(), &s).await;
    assert_eq!(env.config().await.launch_t0, 0);

    env.buy(&f, C).await.unwrap(); // activation at exactly 250
    let t0 = env.config().await.launch_t0;
    assert!(t0 > 0);

    // Day 1: Genesis allowance and wallet both exhausted.
    assert_rigs_error(
        env.buy(&f, 1).await.unwrap_err(),
        RigsError::GenesisCapExceeded,
    );
    // Public: the Genesis cap is gone, the wallet cap still holds — the
    // activation seeds count against it.
    env.set_time(t0 + DAY).await;
    assert_rigs_error(
        env.buy(&f, 1).await.unwrap_err(),
        RigsError::WalletCapExceeded,
    );
    env.warp(3_600).await;
    assert_rigs_error(
        env.compound(&f).await.unwrap_err(),
        RigsError::WalletCapExceeded,
    );
}

// ------------------------------------------------------------- sequences

/// Test 10 (on-chain): an interleaved sequence of buys and compounds across
/// wallets never leaves any Miner above 250, and every refusal is the cap.
#[tokio::test]
async fn sequential_buys_and_compounds_never_exceed_the_cap() {
    let mut env = public_env(10).await;
    let wallets: Vec<Keypair> = vec![
        env.erin.insecure_clone(),
        env.frank.insecure_clone(),
        env.dave.insecure_clone(),
    ];
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..45 {
        let r = next();
        let who = &wallets[(r % 3) as usize];
        let rigs_before = match env.account_raw(miner_pda(&who.pubkey())).await {
            Some(_) => env.miner(&who.pubkey()).await.rigs,
            None => 0,
        };
        if (r >> 8) % 2 == 0 {
            let n = 1 + (r >> 16) % 120;
            match env.buy(who, n).await {
                Ok(()) => assert!(rigs_before + n <= C),
                Err(e) => {
                    assert!(rigs_before + n > C, "an in-cap buy failed: {e:?}");
                    assert_rigs_error(e, RigsError::WalletCapExceeded);
                }
            }
        } else if rigs_before > 0 {
            if let Err(e) = env.compound(who).await {
                let s = format!("{e:?}");
                let cap =
                    anchor_lang::error::ERROR_CODE_OFFSET + RigsError::WalletCapExceeded as u32;
                let none =
                    anchor_lang::error::ERROR_CODE_OFFSET + RigsError::NothingToCompound as u32;
                assert!(
                    s.contains(&format!("Custom({cap})")) || s.contains(&format!("Custom({none})")),
                    "unexpected compound error: {s}"
                );
                if rigs_before >= C {
                    assert!(s.contains(&format!("Custom({cap})")));
                }
            }
        }
        env.warp(((r >> 24) % 20_000) as i64).await;
        for w in &wallets {
            if env.account_raw(miner_pda(&w.pubkey())).await.is_some() {
                assert!(env.miner(&w.pubkey()).await.rigs <= C);
            }
        }
    }
}
