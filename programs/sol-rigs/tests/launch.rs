//! Launch activation, access phases and referral — through real instructions
//! on a real runtime. Alice is the founder wallet pinned in config (and holds
//! a direct `.skr`); bob holds a wrapped `.skr`, carol a direct `.skr`; dave a
//! tokenized `.sns`, frank a direct `.sns`; erin holds no domain.

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

// ------------------------------------------------ the pinned registry accounts

/// Re-derive the pinned `.skr` accounts from first principles, exactly as
/// `@onsol/tldparser` does.
#[test]
fn pinned_skr_accounts_derive_from_the_official_scheme() {
    use solana_sdk::hash::hashv;
    let hashed = |name: &str| hashv(&[b"ALT Name Service", name.as_bytes()]).to_bytes();
    let origin =
        APubkey::find_program_address(&[&hashed("ANS"), &[0u8; 32], &[0u8; 32]], &ANS_PROGRAM_ID).0;
    assert_eq!(origin, ANS_ORIGIN);
    let skr = APubkey::find_program_address(
        &[&hashed(".skr"), &[0u8; 32], ANS_ORIGIN.as_ref()],
        &ANS_PROGRAM_ID,
    )
    .0;
    assert_eq!(skr, SKR_TLD_PARENT);
    let tld_house =
        APubkey::find_program_address(&[b"tld_house", b".skr"], &ANS_TLD_HOUSE_PROGRAM_ID).0;
    let name_house = APubkey::find_program_address(
        &[b"name_house", tld_house.as_ref()],
        &ANS_NAME_HOUSE_PROGRAM_ID,
    )
    .0;
    assert_eq!(name_house, SKR_NAME_HOUSE);
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
async fn only_the_founder_wallet_activates_domain_holders_do_not() {
    let mut env = funded_env().await;
    for who in [
        env.bob.insecure_clone(),
        env.carol.insecure_clone(),
        env.dave.insecure_clone(),
        env.frank.insecure_clone(),
        env.erin.insecure_clone(),
    ] {
        let err = env.buy(&who, 10).await.unwrap_err();
        assert_rigs_error(err, RigsError::AccessDenied);
    }
    assert_eq!(t0(&mut env).await, 0);
}

/// The founder wallet is the authority on its own: no domain proof needed.
#[tokio::test]
async fn founder_wallet_activates_without_any_domain_proof() {
    let mut env = funded_env().await;
    let f = env.founder.insecure_clone();
    let ix = env.buy_ix_with(&f.pubkey(), 10, vec![], None);
    env.send(ix, &[&f]).await.unwrap();
    assert!(t0(&mut env).await > 0);
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
async fn founder_wallet_purchase_activates_exactly_once() {
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

// ------------------------------------------------------------ access phases

#[tokio::test]
async fn access_phases_enforced_on_chain_at_exact_boundaries() {
    let mut env = funded_env().await;
    let t = activate(&mut env).await;
    let (bob, carol, dave, frank, erin) = (
        env.bob.insecure_clone(),
        env.carol.insecure_clone(),
        env.dave.insecure_clone(),
        env.frank.insecure_clone(),
        env.erin.insecure_clone(),
    );

    // Day 1, first second: .skr (direct and wrapped) yes; .sns no; none no.
    env.buy(&carol, 1).await.unwrap();
    env.buy(&bob, 1).await.unwrap();
    for who in [&dave, &frank, &erin] {
        assert_rigs_error(env.buy(who, 1).await.unwrap_err(), RigsError::AccessDenied);
    }

    // T0 + 24h − 1: still day 1.
    env.set_time(t + DAY - 1).await;
    assert_rigs_error(
        env.buy(&frank, 1).await.unwrap_err(),
        RigsError::AccessDenied,
    );

    // T0 + 24h: day 2 — .sns (direct and tokenized) join; .skr keeps access.
    env.set_time(t + DAY).await;
    env.buy(&frank, 1).await.unwrap();
    env.buy(&dave, 1).await.unwrap();
    env.buy(&carol, 1).await.unwrap();
    assert_rigs_error(
        env.buy(&erin, 1).await.unwrap_err(),
        RigsError::AccessDenied,
    );

    // T0 + 48h − 1: still day 2.
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
async fn someone_elses_domain_proves_nothing() {
    let mut env = funded_env().await;
    let t = activate(&mut env).await;
    env.set_time(t + DAY).await;
    let erin = env.erin.insecure_clone();
    for holder in [
        env.bob.pubkey(),
        env.carol.pubkey(),
        env.dave.pubkey(),
        env.frank.pubkey(),
    ] {
        let accounts = env.identities[&holder].clone();
        let ix = env.buy_ix_with(&erin.pubkey(), 1, accounts, None);
        assert_rigs_error(
            env.send(ix, &[&erin]).await.unwrap_err(),
            RigsError::AccessDenied,
        );
    }
}

type Forged = Vec<(APubkey, solana_sdk::account::Account)>;
/// (label, is `.sns`, builder of the forged accounts for a wallet)
type ForgeCase = (&'static str, bool, Box<dyn Fn(&APubkey) -> Forged>);

fn wrapped_skr(
    w: &APubkey,
    tag: u8,
    holder: Option<APubkey>,
    token_prog: APubkey,
    pda: bool,
) -> Forged {
    let name = APubkey::new_unique();
    let record = if pda {
        skr_wrapped_record_key(&name)
    } else {
        APubkey::new_unique()
    };
    let mint = APubkey::new_unique();
    vec![
        (
            name,
            program_owned(ANS_PROGRAM_ID, ans_record(&SKR_TLD_PARENT, &record, 0)),
        ),
        (
            record,
            program_owned(ANS_NAME_HOUSE_PROGRAM_ID, ans_nft_record(tag, &name, &mint)),
        ),
        (
            APubkey::new_unique(),
            program_owned(
                token_prog,
                token_account(&mint, &holder.unwrap_or(*w), 1, 1),
            ),
        ),
    ]
}

fn tokenized_sns(
    w: &APubkey,
    tag: u8,
    amount: u64,
    record_prog: APubkey,
    name_owned_by_record: bool,
) -> Forged {
    let name = APubkey::new_unique();
    let record = sns_tokenized_record_key(&name);
    let mint = APubkey::new_unique();
    let header_owner = if name_owned_by_record {
        record
    } else {
        APubkey::new_unique()
    };
    vec![
        (
            name,
            program_owned(
                SNS_NAME_PROGRAM_ID,
                sns_record(&SNS_TLD_ROOT, &header_owner),
            ),
        ),
        (
            record,
            program_owned(record_prog, sns_nft_record(tag, &name, &mint)),
        ),
        (
            APubkey::new_unique(),
            program_owned(SPL_TOKEN_PROGRAM_ID, token_account(&mint, w, amount, 1)),
        ),
    ]
}

/// Every forgery of a `.skr` / `.sns` proof fails closed — `.skr` shapes on
/// day 1, `.sns` shapes on day 2 (when a genuine one would pass).
#[tokio::test]
async fn forged_domain_proofs_fail_closed() {
    let sys = solana_sdk::system_program::ID;
    let cases: Vec<ForgeCase> = vec![
        (
            "skr: not owned by ANS",
            false,
            Box::new(move |w| {
                vec![(
                    APubkey::new_unique(),
                    program_owned(sys, ans_record(&SKR_TLD_PARENT, w, 0)),
                )]
            }),
        ),
        (
            "skr: another TLD",
            false,
            Box::new(|w| {
                vec![(
                    APubkey::new_unique(),
                    program_owned(ANS_PROGRAM_ID, ans_record(&APubkey::new_unique(), w, 0)),
                )]
            }),
        ),
        (
            "skr: expired",
            false,
            Box::new(|w| {
                vec![(
                    APubkey::new_unique(),
                    program_owned(ANS_PROGRAM_ID, ans_record(&SKR_TLD_PARENT, w, 1_000)),
                )]
            }),
        ),
        (
            "skr: bad discriminator",
            false,
            Box::new(|w| {
                let mut d = ans_record(&SKR_TLD_PARENT, w, 0);
                d[0] ^= 0xff;
                vec![(APubkey::new_unique(), program_owned(ANS_PROGRAM_ID, d))]
            }),
        ),
        (
            "skr wrapped: record not the name-house PDA",
            false,
            Box::new(|w| wrapped_skr(w, 1, None, SPL_TOKEN_PROGRAM_ID, false)),
        ),
        (
            "skr wrapped: inactive record",
            false,
            Box::new(|w| wrapped_skr(w, 2, None, SPL_TOKEN_PROGRAM_ID, true)),
        ),
        (
            "skr wrapped: NFT held by someone else",
            false,
            Box::new(|w| {
                wrapped_skr(
                    w,
                    1,
                    Some(APubkey::new_unique()),
                    SPL_TOKEN_PROGRAM_ID,
                    true,
                )
            }),
        ),
        (
            "skr wrapped: token account not SPL-owned",
            false,
            Box::new(move |w| wrapped_skr(w, 1, None, sys, true)),
        ),
        (
            "sns: not owned by the name program",
            true,
            Box::new(move |w| {
                vec![(
                    APubkey::new_unique(),
                    program_owned(sys, sns_record(&SNS_TLD_ROOT, w)),
                )]
            }),
        ),
        (
            "sns: subdomain (parent is not the .sns root)",
            true,
            Box::new(|w| {
                vec![(
                    APubkey::new_unique(),
                    program_owned(SNS_NAME_PROGRAM_ID, sns_record(&APubkey::new_unique(), w)),
                )]
            }),
        ),
        (
            "sns tokenized: zero balance",
            true,
            Box::new(|w| tokenized_sns(w, 2, 0, SNS_NAME_TOKENIZER_ID, true)),
        ),
        (
            "sns tokenized: inactive record",
            true,
            Box::new(|w| tokenized_sns(w, 3, 1, SNS_NAME_TOKENIZER_ID, true)),
        ),
        (
            "sns tokenized: record owned by the wrong program",
            true,
            Box::new(|w| tokenized_sns(w, 2, 1, ANS_NAME_HOUSE_PROGRAM_ID, true)),
        ),
        (
            "sns tokenized: name not owned by its record",
            true,
            Box::new(|w| tokenized_sns(w, 2, 1, SNS_NAME_TOKENIZER_ID, false)),
        ),
    ];

    for (label, is_sns, make) in cases {
        let forger = solana_sdk::signature::Keypair::new();
        let fk = forger.pubkey();
        let accounts = make(&fk);
        let keys: Vec<APubkey> = accounts.iter().map(|(k, _)| *k).collect();
        let mut env = Env::with(move |pt| {
            pt.add_account(
                fk,
                solana_sdk::account::Account {
                    lamports: 100 * SOL,
                    ..Default::default()
                },
            );
            for (k, a) in accounts {
                pt.add_account(k, a);
            }
        })
        .await;
        env.initialize().await.unwrap();
        env.fund_rewards(10 * SOL, 30 * DAY as u64).await.unwrap();
        let t = activate(&mut env).await;
        env.set_time(if is_sns { t + DAY } else { t }).await;
        let ix = env.buy_ix_with(&fk, 1, keys, None);
        let err = env.send(ix, &[&forger]).await.expect_err(label);
        assert_rigs_error(err, RigsError::AccessDenied);
    }
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
    let bob_id = env.identities[&bob.pubkey()].clone();

    // Self-referral.
    let ix = env.buy_ix_with(
        &bob.pubkey(),
        1,
        bob_id.clone(),
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
        bob_id.clone(),
        Some((carol.pubkey(), miner_pda(&carol.pubkey()))),
    );
    assert_rigs_error(
        env.send(ix, &[&bob]).await.unwrap_err(),
        RigsError::InvalidReferrer,
    );

    // (A referrer named without its account is checked below, in the public
    // phase, where no identity accounts sit in front of it.)

    // A forged referrer account (not the referrer's miner PDA).
    let ix = env.buy_ix_with(
        &bob.pubkey(),
        1,
        bob_id.clone(),
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
        bob_id.clone(),
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
        bob_id.clone(),
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

    // Referrer named but its account missing: rejected, nothing bound.
    let t = env.config().await.launch_t0;
    env.set_time(t + 2 * DAY).await;
    let erin = env.erin.insecure_clone();
    let mut ix = env.buy_ix_with(&erin.pubkey(), 1, vec![], None);
    ix.data = anchor_lang::InstructionData::data(&sol_rigs::instruction::BuyRigs {
        count: 1,
        referrer: Some(alice.pubkey()),
    });
    assert_rigs_error(
        env.send(ix, &[&erin]).await.unwrap_err(),
        RigsError::MissingReferrerAccount,
    );
}

/// On-chain solvency with a referred planter: paid + owed ≤ funded, and the
/// referred rig earns 1.05× per rig.
#[tokio::test]
async fn referral_weight_redistributes_without_new_emission() {
    let mut env = funded_env().await;
    activate(&mut env).await; // alice: 10 base rigs
    let (alice, bob) = (env.alice.insecure_clone(), env.bob.insecure_clone());
    let bob_id = env.identities[&bob.pubkey()].clone();
    let ix = env.buy_ix_with(
        &bob.pubkey(),
        10,
        bob_id.clone(),
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
