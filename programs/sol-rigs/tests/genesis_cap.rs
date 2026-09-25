//! The Genesis fresh-purchase cap — through real instructions on a real
//! runtime. The gated period is [T0, T0 + 24h); from T0 + 24h (Public) the
//! cap no longer applies. The cap counts a wallet's cumulative *fresh*
//! purchases (`Miner.lifetime_rigs_bought`): the founder activation counts,
//! compounding never does.

mod common;

use common::*;
use sol_rigs::error::RigsError;
use solana_sdk::{signature::Signer, transaction::Transaction};

const DAY: i64 = 86_400;

async fn env_with_cap(cap: u64) -> Env {
    let mut env = Env::new().await;
    env.initialize_with_cap(cap).await.unwrap();
    env.fund_rewards(1_000 * SOL, DAY as u64).await.unwrap();
    env
}

// ------------------------------------------------------------ basic shape

#[tokio::test]
async fn first_buy_below_cap_and_exact_cap_are_allowed() {
    let mut env = env_with_cap(5).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, 3).await.unwrap(); // activation below cap
    let t0 = env.config().await.launch_t0;
    assert!(t0 > 0);

    // Repeat buys reaching the cap exactly: 3 + 1 + 1 = 5.
    env.buy(&f, 1).await.unwrap();
    env.buy(&f, 1).await.unwrap();
    let m = env.miner(&f.pubkey()).await;
    assert_eq!((m.rigs, m.lifetime_rigs_bought), (5, 5));
}

#[tokio::test]
async fn one_rig_over_the_cap_is_rejected_before_anything_moves() {
    let mut env = env_with_cap(5).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, 5).await.unwrap(); // activation at the exact cap
    let before = env.config().await;
    let bal_before = env.lamports(f.pubkey()).await;

    assert_rigs_error(
        env.buy(&f, 1).await.unwrap_err(),
        RigsError::GenesisCapExceeded,
    );
    assert_rigs_error(
        env.buy(&f, 100).await.unwrap_err(),
        RigsError::GenesisCapExceeded,
    );

    // Nothing moved: rigs, lifetime count, protocol counters, wallet balance.
    let after = env.config().await;
    assert_eq!(
        (after.total_rigs, after.total_capital_received),
        (5, before.total_capital_received)
    );
    let m = env.miner(&f.pubkey()).await;
    assert_eq!(
        m.lifetime_rigs_bought, 5,
        "the rejected buy consumed no allowance"
    );
    assert_eq!(
        env.lamports(f.pubkey()).await,
        bal_before,
        "no lamports left the wallet"
    );
}

#[tokio::test]
async fn repeated_buys_over_the_cap_are_rejected_every_time() {
    let mut env = env_with_cap(5).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, 5).await.unwrap();
    for k in 1..=4i64 {
        env.warp(k * 60).await; // still gated (minutes in)
        let err = env.buy(&f, k as u64).await.unwrap_err();
        assert_rigs_error(err, RigsError::GenesisCapExceeded);
    }
    assert_eq!(env.miner(&f.pubkey()).await.rigs, 5);
}

#[tokio::test]
async fn other_wallets_have_their_own_cap() {
    let mut env = env_with_cap(5).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, 5).await.unwrap(); // founder exhausts his cap
    let t = env.config().await.launch_t0;
    env.set_time(t + 60).await;
    // A .skr holder has the full 5 of his own.
    let carol = env.carol.insecure_clone();
    env.buy(&carol, 5).await.unwrap();
    assert_rigs_error(
        env.buy(&carol, 1).await.unwrap_err(),
        RigsError::GenesisCapExceeded,
    );
}

// ---------------------------------------------------- founder activation

#[tokio::test]
async fn founder_activation_below_and_at_cap_activates() {
    let mut env = env_with_cap(10).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, 6).await.unwrap();
    assert!(env.config().await.launch_t0 > 0, "below cap activates");

    let mut env = env_with_cap(5).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, 5).await.unwrap();
    assert!(env.config().await.launch_t0 > 0, "exact cap activates");
}

#[tokio::test]
async fn founder_activation_above_cap_is_refused_and_t0_stays_unset() {
    let mut env = env_with_cap(5).await;
    let f = env.founder.insecure_clone();
    assert_rigs_error(
        env.buy(&f, 6).await.unwrap_err(),
        RigsError::GenesisCapExceeded,
    );
    let cfg = env.config().await;
    assert_eq!(cfg.launch_t0, 0, "a refused activation leaves no T0 behind");
    assert_eq!(cfg.total_rigs, 0, "and no state moved");
    // The same wallet can come back within the cap.
    env.buy(&f, 5).await.unwrap();
    assert!(env.config().await.launch_t0 > 0);
}

// --------------------------------------------------------- phase coupling

#[tokio::test]
async fn cap_applies_through_the_last_gated_second() {
    let mut env = env_with_cap(5).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, 5).await.unwrap();
    let t = env.config().await.launch_t0;
    env.set_time(t + DAY - 1).await;
    assert_rigs_error(
        env.buy(&f, 1).await.unwrap_err(),
        RigsError::GenesisCapExceeded,
    );
}

#[tokio::test]
async fn exact_24h_removes_the_cap_public_forever() {
    let mut env = env_with_cap(5).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, 5).await.unwrap();
    let t = env.config().await.launch_t0;

    env.set_time(t + DAY).await; // exact boundary
    env.buy(&f, 50).await.unwrap();
    env.set_time(t + 2 * DAY).await; // well past it
    env.buy(&f, 500).await.unwrap();
    let m = env.miner(&f.pubkey()).await;
    assert_eq!(m.rigs, 555);
    // A no-domain wallet is public too, with no cap.
    let erin = env.erin.insecure_clone();
    env.buy(&erin, 7).await.unwrap();
}

// ---------------------------------------------------------- compounding

/// Compounding while the fresh-buy allowance is exhausted still mints rigs:
/// the cap bounds only fresh purchases.
#[tokio::test]
async fn compound_works_with_the_fresh_buy_cap_exhausted_and_consumes_no_allowance() {
    let mut env = env_with_cap(5).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, 5).await.unwrap(); // cap exhausted
    let t = env.config().await.launch_t0;
    env.warp(12 * 3_600).await; // still gated; accrue a fat ore bin
    assert!(t + 12 * 3_600 < t + DAY);

    let before = env.miner(&f.pubkey()).await;
    env.compound(&f).await.unwrap();
    let after = env.miner(&f.pubkey()).await;
    assert!(after.rigs > before.rigs, "compounding minted rigs");
    assert_eq!(
        after.lifetime_rigs_bought, before.lifetime_rigs_bought,
        "compounded rigs never consume cap allowance"
    );
    assert!(after.lifetime_rigs_compounded > before.lifetime_rigs_compounded);

    // And the exhausted allowance is still exhausted for fresh buys.
    assert_rigs_error(
        env.buy(&f, 1).await.unwrap_err(),
        RigsError::GenesisCapExceeded,
    );
}

// --------------------------------------------------------- rapid-fire buys

/// Two same-slot purchases from one wallet serialize on the Miner PDA: the
/// first lands, the over-cap sibling is rejected, nothing exceeds the cap.
#[tokio::test]
async fn rapid_same_wallet_purchases_serialize_under_the_cap() {
    let mut env = env_with_cap(5).await;
    let f = env.founder.insecure_clone();
    let payer = env.ctx.payer.insecure_clone();
    let bh = env.ctx.banks_client.get_latest_blockhash().await.unwrap();
    let a = Transaction::new_signed_with_payer(
        &[env.buy_ix(&f.pubkey(), 4)],
        Some(&payer.pubkey()),
        &[&payer, &f],
        bh,
    );
    // Distinct count => distinct signature: this is a real concurrent sibling,
    // not a deduplicated byte-identical replay.
    let b = Transaction::new_signed_with_payer(
        &[env.buy_ix(&f.pubkey(), 2)],
        Some(&payer.pubkey()),
        &[&payer, &f],
        bh,
    );
    env.ctx.banks_client.process_transaction(a).await.unwrap();
    let t0 = env.config().await.launch_t0;
    assert!(t0 > 0);
    // The sibling would push 4 + 2 past the cap of 5.
    let err = env
        .ctx
        .banks_client
        .process_transaction(b)
        .await
        .expect_err("4 + 2 > 5 must fail");
    let s = format!("{err:?}");
    assert!(
        s.contains(&format!(
            "Custom({})",
            anchor_lang::error::ERROR_CODE_OFFSET + RigsError::GenesisCapExceeded as u32
        )),
        "expected GenesisCapExceeded, got: {s}"
    );
    let m = env.miner(&f.pubkey()).await;
    assert_eq!((m.rigs, m.lifetime_rigs_bought), (4, 4));
}
