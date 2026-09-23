//! Launch activation, access phases and referral — through real instructions
//! on a real runtime. Alice is the pinned founder and owns the planted
//! `sneaks.skr` record; bob and carol own `.skr` records; dave owns only an
//! SNS record; erin owns nothing.

mod common;

use anchor_lang::prelude::Pubkey as APubkey;
use common::*;
use sol_rigs::constants::*;
use sol_rigs::error::RigsError;
use solana_sdk::{signature::Signer, transaction::Transaction};

const DAY: i64 = 86_400;

async fn funded_env() -> Env {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    env.fund_rewards(10 * SOL, 30 * DAY as u64).await.unwrap();
    env
}

async fn t0(env: &mut Env) -> i64 {
    env.config().await.launch_t0
}

async fn activate(env: &mut Env) -> i64 {
    let f = env.founder.insecure_clone();
    env.buy(&f, 10).await.unwrap();
    let t = t0(env).await;
    assert!(t > 0);
    t
}

// ------------------------------------------------ the pinned identity itself

/// The pinned constants are re-derived here from first principles, exactly as
/// `@onsol/tldparser` derives them: PDA([sha256("ALT Name Service" ‖ name),
/// [0;32] class, parent], ANS).
#[test]
fn pinned_skr_accounts_derive_from_the_official_scheme() {
    use solana_sdk::hash::hashv;
    let hashed = |name: &str| hashv(&[b"ALT Name Service", name.as_bytes()]).to_bytes();
    let pda = |name: &str, parent: &APubkey| {
        APubkey::find_program_address(
            &[&hashed(name), &[0u8; 32], parent.as_ref()],
            &ANS_PROGRAM_ID,
        )
        .0
    };
    let origin =
        APubkey::find_program_address(&[&hashed("ANS"), &[0u8; 32], &[0u8; 32]], &ANS_PROGRAM_ID).0;
    assert_eq!(origin, ANS_ORIGIN);
    assert_eq!(pda(".skr", &ANS_ORIGIN), SKR_TLD_PARENT);
    assert_eq!(pda(FOUNDER_NAME, &SKR_TLD_PARENT), FOUNDER_NAME_ACCOUNT);
    assert_eq!(FOUNDER_NAME, "sneaks");
}

// ------------------------------------------------------ what never activates

#[tokio::test]
async fn initialize_fund_and_connect_do_not_activate() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    assert_eq!(t0(&mut env).await, 0, "initialize");
    assert_eq!(env.config().await.founder, env.founder.pubkey());
    env.fund_rewards(10 * SOL, 30 * DAY as u64).await.unwrap();
    assert_eq!(t0(&mut env).await, 0, "funding");
    // "Connecting" is off-chain; the only on-chain effect is none at all.
    assert_eq!(t0(&mut env).await, 0, "connect");
}

#[tokio::test]
async fn simulation_does_not_activate() {
    let mut env = funded_env().await;
    let f = env.founder.insecure_clone();
    let ix = env.buy_ix(&f.pubkey(), 10);
    let payer = env.ctx.payer.insecure_clone();
    let bh = env.ctx.banks_client.get_latest_blockhash().await.unwrap();
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer, &f], bh);
    let sim = env.ctx.banks_client.simulate_transaction(tx).await.unwrap();
    assert!(
        sim.result.unwrap().is_ok(),
        "the activation simulates cleanly"
    );
    assert_eq!(t0(&mut env).await, 0, "a simulation writes nothing");
}

#[tokio::test]
async fn wrong_wallet_does_not_activate_even_with_a_skr_name() {
    let mut env = funded_env().await;
    let bob = env.bob.insecure_clone();
    let err = env.buy(&bob, 10).await.unwrap_err();
    assert_rigs_error(err, RigsError::AccessDenied);
    // Bob passing the real sneaks.skr record (owned by alice) changes nothing.
    let ix = env.buy_ix_with(&bob.pubkey(), 10, Some(FOUNDER_NAME_ACCOUNT), None);
    assert_rigs_error(
        env.send(ix, &[&bob]).await.unwrap_err(),
        RigsError::AccessDenied,
    );
    assert_eq!(t0(&mut env).await, 0);
}

#[tokio::test]
async fn founder_without_the_name_proof_does_not_activate() {
    let mut env = funded_env().await;
    let f = env.founder.insecure_clone();
    // No identity account.
    let ix = env.buy_ix_with(&f.pubkey(), 10, None, None);
    assert_rigs_error(
        env.send(ix, &[&f]).await.unwrap_err(),
        RigsError::AccessDenied,
    );
    // Some other .skr (bob's) is not sneaks.skr.
    let bobs = env.identities[&env.bob.pubkey()];
    let ix = env.buy_ix_with(&f.pubkey(), 10, Some(bobs), None);
    assert_rigs_error(
        env.send(ix, &[&f]).await.unwrap_err(),
        RigsError::AccessDenied,
    );
    assert_eq!(t0(&mut env).await, 0);
}

/// Spoofs of `sneaks.skr`: every one fails closed.
#[tokio::test]
async fn spoofed_founder_names_do_not_activate() {
    type Plant = fn(&APubkey) -> solana_sdk::account::Account;
    let spoofs: [(&str, Plant); 5] = [
        // Right bytes, wrong owner program: anyone can create this.
        ("not owned by ANS", |f| {
            program_owned(
                solana_sdk::system_program::ID,
                ans_record(&SKR_TLD_PARENT, f, 0),
            )
        }),
        // Real ANS record, but the name belongs to someone else.
        ("owned by another wallet", |_| {
            program_owned(
                ANS_PROGRAM_ID,
                ans_record(&SKR_TLD_PARENT, &APubkey::new_unique(), 0),
            )
        }),
        // A name under some other TLD.
        ("wrong parent", |f| {
            program_owned(ANS_PROGRAM_ID, ans_record(&APubkey::new_unique(), f, 0))
        }),
        // Expired.
        ("expired", |f| {
            program_owned(ANS_PROGRAM_ID, ans_record(&SKR_TLD_PARENT, f, 1_000))
        }),
        // Wrong discriminator.
        ("bad discriminator", |f| {
            let mut d = ans_record(&SKR_TLD_PARENT, f, 0);
            d[0] ^= 0xff;
            program_owned(ANS_PROGRAM_ID, d)
        }),
    ];
    for (label, make) in spoofs {
        let mut env = Env::with_names(|pt, alice| {
            pt.add_account(FOUNDER_NAME_ACCOUNT, make(&alice.pubkey()));
            true
        })
        .await;
        env.initialize().await.unwrap();
        env.fund_rewards(10 * SOL, 30 * DAY as u64).await.unwrap();
        let f = env.founder.insecure_clone();
        let err = env.buy(&f, 10).await.unwrap_err();
        assert_rigs_error(err, RigsError::AccessDenied);
        assert_eq!(t0(&mut env).await, 0, "{label}");
    }
}

#[tokio::test]
async fn unfunded_or_failed_founder_purchases_do_not_activate() {
    // Initialised but never funded.
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    let f = env.founder.insecure_clone();
    assert_rigs_error(
        env.buy(&f, 10).await.unwrap_err(),
        RigsError::ActivationUnfunded,
    );
    assert_eq!(t0(&mut env).await, 0);

    // Funded, but the window has already closed.
    env.fund_rewards(SOL, 100).await.unwrap();
    env.warp(200).await;
    assert_rigs_error(
        env.buy(&f, 10).await.unwrap_err(),
        RigsError::ActivationUnfunded,
    );
    assert_eq!(t0(&mut env).await, 0);

    // Funded and live, but the purchase itself fails.
    env.fund_rewards(SOL, 30 * DAY as u64).await.unwrap();
    assert_rigs_error(
        env.buy(&f, 0).await.unwrap_err(),
        RigsError::InvalidRigCount,
    );
    assert_eq!(t0(&mut env).await, 0, "invalid count");
    // More rigs than the founder can pay for: the transfer fails.
    assert!(env.buy(&f, 1_000_000).await.is_err());
    assert_eq!(t0(&mut env).await, 0, "insufficient SOL");
    // Paused program.
    env.set_paused(true).await.unwrap();
    assert!(env.buy(&f, 10).await.is_err());
    assert_eq!(t0(&mut env).await, 0, "paused");
}

// ------------------------------------------------------------ what activates

#[tokio::test]
async fn verified_founder_purchase_activates_exactly_once() {
    let mut env = funded_env().await;
    let now = env.clock().await.unix_timestamp;
    let f = env.founder.insecure_clone();
    env.buy(&f, 10).await.unwrap();
    let cfg = env.config().await;
    assert_eq!(cfg.launch_t0, now, "T0 is the chain time of the purchase");
    let m = env.miner(&f.pubkey()).await;
    assert_eq!(m.rigs, 10);
    assert_eq!(cfg.total_rigs, 10);
    assert_eq!(
        cfg.total_protocol_fees,
        10 * RIG_PRICE * FEE_BPS as u64 / 10_000
    );

    // A second founder purchase later: T0 does not move.
    env.warp(3_600).await;
    env.buy(&f, 5).await.unwrap();
    assert_eq!(t0(&mut env).await, now);
}

#[tokio::test]
async fn replay_and_concurrent_attempts_yield_one_t0() {
    let mut env = funded_env().await;
    let f = env.founder.insecure_clone();
    let payer = env.ctx.payer.insecure_clone();
    let bh = env.ctx.banks_client.get_latest_blockhash().await.unwrap();
    // Two distinct activation transactions in the same slot (different
    // counts so the signatures differ) and a byte-identical replay.
    let a = Transaction::new_signed_with_payer(
        &[env.buy_ix(&f.pubkey(), 10)],
        Some(&payer.pubkey()),
        &[&payer, &f],
        bh,
    );
    let b = Transaction::new_signed_with_payer(
        &[env.buy_ix(&f.pubkey(), 11)],
        Some(&payer.pubkey()),
        &[&payer, &f],
        bh,
    );
    env.ctx
        .banks_client
        .process_transaction(a.clone())
        .await
        .unwrap();
    let first = t0(&mut env).await;
    assert!(first > 0);
    // A byte-identical replay (same signature) is deduplicated by the
    // runtime: whatever the client reports, it executes nothing again.
    let _ = env.ctx.banks_client.process_transaction(a).await;
    assert_eq!(
        env.miner(&f.pubkey()).await.rigs,
        10,
        "replay credited nothing"
    );
    assert_eq!(t0(&mut env).await, first);
    env.warp(10).await;
    // The concurrent sibling lands as an ordinary purchase.
    env.ctx.banks_client.process_transaction(b).await.ok();
    assert_eq!(t0(&mut env).await, first);
}

#[tokio::test]
async fn no_admin_path_touches_t0() {
    let mut env = funded_env().await;
    let t = activate(&mut env).await;
    env.set_paused(true).await.unwrap();
    env.set_paused(false).await.unwrap();
    env.fund_rewards(5 * SOL, 10 * DAY as u64).await.unwrap();
    env.warp(DAY).await;
    let f = env.founder.insecure_clone();
    env.claim(&f).await.unwrap();
    assert_eq!(t0(&mut env).await, t);
    // Re-initialising is impossible, so T0 cannot be reset that way either.
    assert!(env.initialize().await.is_err());
    assert_eq!(t0(&mut env).await, t);
}

// ------------------------------------------------------------ access phases

#[tokio::test]
async fn access_phases_enforced_on_chain_at_exact_boundaries() {
    let mut env = funded_env().await;
    let t = activate(&mut env).await;
    let (carol, dave, erin) = (
        env.carol.insecure_clone(),
        env.dave.insecure_clone(),
        env.erin.insecure_clone(),
    );

    // Phase 1, first second: .skr yes; SNS-only no; ordinary no.
    env.buy(&carol, 1).await.unwrap();
    assert_rigs_error(
        env.buy(&dave, 1).await.unwrap_err(),
        RigsError::AccessDenied,
    );
    assert_rigs_error(
        env.buy(&erin, 1).await.unwrap_err(),
        RigsError::AccessDenied,
    );

    // T0 + 24h − 1: still phase 1.
    env.set_time(t + DAY - 1).await;
    assert_rigs_error(
        env.buy(&dave, 1).await.unwrap_err(),
        RigsError::AccessDenied,
    );

    // T0 + 24h: phase 2 — SNS yes, ordinary still no.
    env.set_time(t + DAY).await;
    env.buy(&dave, 1).await.unwrap();
    assert_rigs_error(
        env.buy(&erin, 1).await.unwrap_err(),
        RigsError::AccessDenied,
    );

    // T0 + 48h − 1: still phase 2.
    env.set_time(t + 2 * DAY - 1).await;
    assert_rigs_error(
        env.buy(&erin, 1).await.unwrap_err(),
        RigsError::AccessDenied,
    );

    // T0 + 48h: public.
    env.set_time(t + 2 * DAY).await;
    env.buy(&erin, 1).await.unwrap();

    // Years later: still public.
    env.set_time(t + 5 * 365 * DAY).await;
    env.buy(&erin, 1).await.unwrap();
    assert_eq!(t0(&mut env).await, t);
}

#[tokio::test]
async fn an_identity_account_for_someone_else_proves_nothing() {
    let mut env = funded_env().await;
    activate(&mut env).await;
    let erin = env.erin.insecure_clone();
    // Erin passes carol's genuine .skr record.
    let carols = env.identities[&env.carol.pubkey()];
    let ix = env.buy_ix_with(&erin.pubkey(), 1, Some(carols), None);
    assert_rigs_error(
        env.send(ix, &[&erin]).await.unwrap_err(),
        RigsError::AccessDenied,
    );
}

// ------------------------------------------------------------------ referral

#[tokio::test]
async fn referral_binds_once_to_an_existing_planter() {
    let mut env = funded_env().await;
    activate(&mut env).await;
    let (alice, bob, carol) = (
        env.alice.insecure_clone(),
        env.bob.insecure_clone(),
        env.carol.insecure_clone(),
    );
    let bob_id = env.identities[&bob.pubkey()];

    // Self-referral.
    let ix = env.buy_ix_with(
        &bob.pubkey(),
        1,
        Some(bob_id),
        Some((bob.pubkey(), miner_pda(&bob.pubkey()))),
    );
    assert_rigs_error(
        env.send(ix, &[&bob]).await.unwrap_err(),
        RigsError::SelfReferral,
    );

    // A referrer with no position.
    let ix = env.buy_ix_with(
        &bob.pubkey(),
        1,
        Some(bob_id),
        Some((carol.pubkey(), miner_pda(&carol.pubkey()))),
    );
    assert_rigs_error(
        env.send(ix, &[&bob]).await.unwrap_err(),
        RigsError::InvalidReferrer,
    );

    // Referrer named but account missing.
    let mut ix = env.buy_ix_with(&bob.pubkey(), 1, Some(bob_id), None);
    ix.data = anchor_lang::InstructionData::data(&sol_rigs::instruction::BuyRigs {
        count: 1,
        referrer: Some(alice.pubkey()),
    });
    assert_rigs_error(
        env.send(ix, &[&bob]).await.unwrap_err(),
        RigsError::MissingReferrerAccount,
    );

    // A forged referrer account (not the referrer's miner PDA).
    let ix = env.buy_ix_with(
        &bob.pubkey(),
        1,
        Some(bob_id),
        Some((alice.pubkey(), config_pda())),
    );
    assert_rigs_error(
        env.send(ix, &[&bob]).await.unwrap_err(),
        RigsError::InvalidReferrer,
    );

    // Valid: alice (a planter) refers bob at bob's first purchase.
    let ix = env.buy_ix_with(
        &bob.pubkey(),
        10,
        Some(bob_id),
        Some((alice.pubkey(), miner_pda(&alice.pubkey()))),
    );
    env.send(ix, &[&bob]).await.unwrap();
    let m = env.miner(&bob.pubkey()).await;
    assert_eq!((m.weight, m.referrer), (105, alice.pubkey()));
    let cfg = env.config().await;
    assert_eq!(cfg.total_shares, 10 * 100 + 10 * 105);

    // Replacement / stacking: any referrer on a later purchase is refused.
    let ix = env.buy_ix_with(
        &bob.pubkey(),
        1,
        Some(bob_id),
        Some((alice.pubkey(), miner_pda(&alice.pubkey()))),
    );
    assert_rigs_error(
        env.send(ix, &[&bob]).await.unwrap_err(),
        RigsError::ReferrerAlreadyBound,
    );
    // A plain later purchase keeps the bound weight.
    env.buy(&bob, 1).await.unwrap();
    let m = env.miner(&bob.pubkey()).await;
    assert_eq!((m.rigs, m.weight, m.referrer), (11, 105, alice.pubkey()));
    assert_eq!(env.config().await.total_shares, 10 * 100 + 11 * 105);
}

/// On-chain solvency with a referred planter: paid + owed ≤ funded, and the
/// referred rig earns 1.05× per rig.
#[tokio::test]
async fn referral_weight_redistributes_without_new_emission() {
    let mut env = funded_env().await;
    activate(&mut env).await; // alice: 10 base rigs
    let (alice, bob) = (env.alice.insecure_clone(), env.bob.insecure_clone());
    let bob_id = env.identities[&bob.pubkey()];
    let ix = env.buy_ix_with(
        &bob.pubkey(),
        10,
        Some(bob_id),
        Some((alice.pubkey(), miner_pda(&alice.pubkey()))),
    );
    env.send(ix, &[&bob]).await.unwrap();
    env.warp(5 * DAY).await;
    let a_before = env.lamports(alice.pubkey()).await;
    let b_before = env.lamports(bob.pubkey()).await;
    env.claim(&alice).await.unwrap();
    env.claim(&bob).await.unwrap();
    let a = env.lamports(alice.pubkey()).await as i128 - a_before as i128;
    let b = env.lamports(bob.pubkey()).await as i128 - b_before as i128;
    // Bob's per-rig take is 1.05× alice's over the shared window (alice also
    // earned alone briefly before bob arrived, so bob ≤ 1.05× alice).
    assert!(b > a, "referred planter earns more per rig");
    assert!(b * 100 <= a * 106);
    let cfg = env.config().await;
    assert!(cfg.total_reward_claimed <= cfg.total_reward_funded);
}
