//! The Genesis fresh-purchase cap — through real instructions on a real
//! runtime. The gated period is [T0, T0 + 24h); from T0 + 24h (Public) the
//! cap no longer applies. The cap counts a wallet's cumulative *fresh*
//! purchases (`Miner.lifetime_rigs_bought`): the founder activation counts,
//! compounding never does.
//!
//! `initialize` accepts only a cap of 250 (`GENESIS_CAP`), equal to the
//! permanent wallet cap, so these tests run at the production value. The
//! permanent cap itself is covered in `wallet_cap.rs`.

mod common;

use common::*;
use sol_rigs::error::RigsError;
use solana_sdk::{signature::Signer, transaction::Transaction};

const DAY: i64 = 86_400;

const C: u64 = GENESIS_CAP;

async fn env_with_cap(cap: u64) -> Env {
    assert_eq!(
        cap, GENESIS_CAP,
        "initialize accepts only the production cap"
    );
    let mut env = Env::new().await;
    env.initialize_with_cap(cap).await.unwrap();
    env.fund_rewards(1_000 * SOL, DAY as u64).await.unwrap();
    env
}

// ------------------------------------------------------------ basic shape

#[tokio::test]
async fn first_buy_below_cap_and_exact_cap_are_allowed() {
    let mut env = env_with_cap(C).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, C - 2).await.unwrap(); // activation below cap
    let t0 = env.config().await.launch_t0;
    assert!(t0 > 0);

    // Repeat buys reaching the cap exactly: (C - 2) + 1 + 1 = C.
    env.buy(&f, 1).await.unwrap();
    env.buy(&f, 1).await.unwrap();
    let m = env.miner(&f.pubkey()).await;
    assert_eq!((m.rigs, m.lifetime_rigs_bought), (C, C));
}

#[tokio::test]
async fn one_rig_over_the_cap_is_rejected_before_anything_moves() {
    let mut env = env_with_cap(C).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, C).await.unwrap(); // activation at the exact cap
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
        (C, before.total_capital_received)
    );
    let m = env.miner(&f.pubkey()).await;
    assert_eq!(
        m.lifetime_rigs_bought, C,
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
    let mut env = env_with_cap(C).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, C).await.unwrap();
    for k in 1..=4i64 {
        env.warp(k * 60).await; // still gated (minutes in)
        let err = env.buy(&f, k as u64).await.unwrap_err();
        assert_rigs_error(err, RigsError::GenesisCapExceeded);
    }
    assert_eq!(env.miner(&f.pubkey()).await.rigs, C);
}

#[tokio::test]
async fn other_wallets_have_their_own_cap() {
    let mut env = env_with_cap(C).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, C).await.unwrap(); // founder exhausts his cap
    let t = env.config().await.launch_t0;
    env.set_time(t + 60).await;
    // A .skr holder has the full cap of his own.
    let carol = env.carol.insecure_clone();
    env.buy(&carol, C).await.unwrap();
    assert_rigs_error(
        env.buy(&carol, 1).await.unwrap_err(),
        RigsError::GenesisCapExceeded,
    );
}

// ---------------------------------------------------- founder activation

#[tokio::test]
async fn founder_activation_below_and_at_cap_activates() {
    let mut env = env_with_cap(C).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, C - 4).await.unwrap();
    assert!(env.config().await.launch_t0 > 0, "below cap activates");

    let mut env = env_with_cap(C).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, C).await.unwrap();
    assert!(env.config().await.launch_t0 > 0, "exact cap activates");
}

#[tokio::test]
async fn founder_activation_above_cap_is_refused_and_t0_stays_unset() {
    let mut env = env_with_cap(C).await;
    let f = env.founder.insecure_clone();
    assert_rigs_error(
        env.buy(&f, C + 1).await.unwrap_err(),
        RigsError::GenesisCapExceeded,
    );
    let cfg = env.config().await;
    assert_eq!(cfg.launch_t0, 0, "a refused activation leaves no T0 behind");
    assert_eq!(cfg.total_rigs, 0, "and no state moved");
    // The same wallet can come back within the cap.
    env.buy(&f, C).await.unwrap();
    assert!(env.config().await.launch_t0 > 0);
}

// --------------------------------------------------------- phase coupling

#[tokio::test]
async fn cap_applies_through_the_last_gated_second() {
    let mut env = env_with_cap(C).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, C).await.unwrap();
    let t = env.config().await.launch_t0;
    env.set_time(t + DAY - 1).await;
    assert_rigs_error(
        env.buy(&f, 1).await.unwrap_err(),
        RigsError::GenesisCapExceeded,
    );
}

#[tokio::test]
async fn exact_24h_removes_the_cap_public_forever() {
    let mut env = env_with_cap(C).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, C - 50).await.unwrap();
    let t = env.config().await.launch_t0;

    env.set_time(t + DAY).await; // exact boundary
    env.buy(&f, 40).await.unwrap();
    env.set_time(t + 2 * DAY).await; // well past it
    env.buy(&f, 10).await.unwrap();
    let m = env.miner(&f.pubkey()).await;
    assert_eq!(m.rigs, C);
    // The Genesis check is gone, but the permanent wallet cap is not: a
    // further buy now fails on the wallet cap, not the Genesis cap.
    assert_rigs_error(
        env.buy(&f, 1).await.unwrap_err(),
        RigsError::WalletCapExceeded,
    );
    // A no-domain wallet is public too, up to the wallet cap.
    let erin = env.erin.insecure_clone();
    env.buy(&erin, C).await.unwrap();
}

// ---------------------------------------------------------- compounding

/// Compounding during the gated day mints rigs without consuming Genesis
/// allowance — but it does consume the permanent wallet headroom.
#[tokio::test]
async fn compound_works_with_the_fresh_buy_cap_exhausted_and_consumes_no_allowance() {
    let mut env = env_with_cap(C).await;
    let f = env.founder.insecure_clone();
    env.buy(&f, C - 10).await.unwrap(); // 10 fresh seeds of allowance left
    let t = env.config().await.launch_t0;
    env.warp(12 * 3_600).await; // still gated; accrue a fat ore bin
    assert!(t + 12 * 3_600 < t + DAY);

    let before = env.miner(&f.pubkey()).await;
    env.compound(&f).await.unwrap();
    let after = env.miner(&f.pubkey()).await;
    assert_eq!(after.rigs, C, "compound fills exactly the wallet headroom");
    assert_eq!(
        after.lifetime_rigs_bought, before.lifetime_rigs_bought,
        "compounded rigs never consume cap allowance"
    );
    assert_eq!(
        after.lifetime_rigs_compounded,
        before.lifetime_rigs_compounded + 10
    );

    // Genesis allowance (10) is untouched, but the wallet is full: a fresh
    // buy passes the Genesis check and fails on the permanent cap.
    assert_rigs_error(
        env.buy(&f, 1).await.unwrap_err(),
        RigsError::WalletCapExceeded,
    );
}

// --------------------------------------------------------- rapid-fire buys

/// Two same-slot purchases from one wallet serialize on the Miner PDA: the
/// first lands, the over-cap sibling is rejected, nothing exceeds the cap.
#[tokio::test]
async fn rapid_same_wallet_purchases_serialize_under_the_cap() {
    let mut env = env_with_cap(C).await;
    let f = env.founder.insecure_clone();
    let payer = env.ctx.payer.insecure_clone();
    let bh = env.ctx.banks_client.get_latest_blockhash().await.unwrap();
    let a = Transaction::new_signed_with_payer(
        &[env.buy_ix(&f.pubkey(), C - 1)],
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
    // The sibling would push (C - 1) + 2 past the cap of C.
    let err = env
        .ctx
        .banks_client
        .process_transaction(b)
        .await
        .expect_err("(C - 1) + 2 > C must fail");
    let s = format!("{err:?}");
    assert!(
        s.contains(&format!(
            "Custom({})",
            anchor_lang::error::ERROR_CODE_OFFSET + RigsError::GenesisCapExceeded as u32
        )),
        "expected GenesisCapExceeded, got: {s}"
    );
    let m = env.miner(&f.pubkey()).await;
    assert_eq!((m.rigs, m.lifetime_rigs_bought), (C - 1, C - 1));
}
