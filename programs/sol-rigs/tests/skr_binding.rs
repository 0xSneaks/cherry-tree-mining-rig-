//! Day-1 `.skr` binding: one domain opens Day-1 access for one wallet.
//! Frozen-architecture tests 12, 13, 14 and 15, through real instructions.
//!
//! Wallets (see `common::Env`): alice = founder (direct `.skr`), bob =
//! wrapped `.skr`, carol = direct `.skr`, dave = tokenized `.sns`, erin = no
//! domain, frank = direct `.sns`.

mod common;

use anchor_lang::AccountDeserialize;
use common::*;
use sol_rigs::error::RigsError;
use sol_rigs::state::SkrBinding;
use solana_sdk::{
    account::{Account, AccountSharedData},
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

const DAY: i64 = 86_400;

/// System-program `Transfer { lamports }` (instruction index 2), built by
/// hand: `from` must sign for it to succeed.
fn system_transfer(from: &Pubkey, to: &Pubkey, lamports: u64, from_signs: bool) -> Instruction {
    let mut data = 2u32.to_le_bytes().to_vec();
    data.extend_from_slice(&lamports.to_le_bytes());
    Instruction {
        program_id: solana_sdk::system_program::ID,
        accounts: vec![
            AccountMeta::new(*from, from_signs),
            AccountMeta::new(*to, false),
        ],
        data,
    }
}

/// Initialized, funded, activated (founder buys 10). Returns T0; the clock
/// sits inside Day 1.
async fn day1_env() -> (Env, i64) {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    env.fund_rewards(100 * SOL, 30 * DAY as u64).await.unwrap();
    let f = env.founder.insecure_clone();
    env.buy(&f, 10).await.unwrap();
    let t0 = env.config().await.launch_t0;
    assert!(t0 > 0);
    env.set_time(t0 + 60).await;
    (env, t0)
}

fn name_of(env: &Env, who: &Keypair) -> Pubkey {
    env.skr_names[&who.pubkey()]
}

async fn binding(env: &mut Env, name: &Pubkey) -> Option<(SkrBinding, Account)> {
    let acc = env.account_raw(skr_binding_pda(name)).await?;
    let b = SkrBinding::try_deserialize(&mut acc.data.as_slice()).unwrap();
    Some((b, acc))
}

/// Move a direct `.skr` domain's ANS owner field to `new_owner`: an on-chain
/// domain transfer, as the name service would record it.
async fn transfer_direct(env: &mut Env, name: &Pubkey, new_owner: &Pubkey) {
    let mut acc = env.account_raw(*name).await.unwrap();
    acc.data[40..72].copy_from_slice(new_owner.as_ref());
    env.ctx.set_account(name, &AccountSharedData::from(acc));
}

/// Move a wrapped `.skr` domain's NFT (its token account owner).
async fn transfer_wrapped(env: &mut Env, token: &Pubkey, new_owner: &Pubkey) {
    let mut acc = env.account_raw(*token).await.unwrap();
    acc.data[32..64].copy_from_slice(new_owner.as_ref());
    env.ctx.set_account(token, &AccountSharedData::from(acc));
}

async fn send_buy(
    env: &mut Env,
    who: &Keypair,
    count: u64,
    identity: Vec<Pubkey>,
    binding: Option<Pubkey>,
) -> Result<(), solana_sdk::transport::TransportError> {
    let ix = match binding {
        Some(b) => env.buy_ix_bound(&who.pubkey(), count, identity, b),
        None => env.buy_ix_unbound(&who.pubkey(), count, identity, None),
    };
    let who = who.insecure_clone();
    env.send(ix, &[&who]).await
}

// ------------------------------------------------------------- creation

#[tokio::test]
async fn first_day1_purchase_creates_an_immutable_program_owned_binding() {
    let (mut env, _) = day1_env().await;
    let carol = env.carol.insecure_clone();
    let d = name_of(&env, &carol);
    assert!(binding(&mut env, &d).await.is_none());

    env.buy(&carol, 5).await.unwrap();
    let (b, acc) = binding(&mut env, &d).await.expect("binding created");
    assert_eq!(acc.owner, sol_rigs::ID);
    assert_eq!((b.name_account, b.wallet), (d, carol.pubkey()));
    assert!(b.bound_at > 0);
    let bytes = acc.data.clone();

    // A's later Day-1 purchases reuse it; nothing about it changes.
    env.warp(600).await;
    env.buy(&carol, 5).await.unwrap();
    assert_eq!(binding(&mut env, &d).await.unwrap().1.data, bytes);
}

#[tokio::test]
async fn activation_creates_no_binding_and_the_founders_first_day1_buy_does() {
    let (mut env, _) = day1_env().await;
    let alice = env.alice.insecure_clone();
    let d = name_of(&env, &alice);
    assert!(
        binding(&mut env, &d).await.is_none(),
        "the pre-launch activation relies on no domain and binds none"
    );
    env.buy(&alice, 1).await.unwrap();
    assert_eq!(
        binding(&mut env, &d).await.unwrap().0.wallet,
        alice.pubkey()
    );
}

#[tokio::test]
async fn day1_skr_purchase_without_its_binding_account_is_rejected() {
    let (mut env, _) = day1_env().await;
    let carol = env.carol.insecure_clone();
    let id = env.identities[&carol.pubkey()].clone();
    assert_rigs_error(
        send_buy(&mut env, &carol, 1, id, None).await.unwrap_err(),
        RigsError::SkrBindingRequired,
    );
}

#[tokio::test]
async fn a_binding_account_for_another_domain_is_rejected() {
    let (mut env, _) = day1_env().await;
    let (carol, alice) = (env.carol.insecure_clone(), env.alice.insecure_clone());
    let id = env.identities[&carol.pubkey()].clone();
    let wrong = skr_binding_pda(&name_of(&env, &alice));
    assert_rigs_error(
        send_buy(&mut env, &carol, 1, id.clone(), Some(wrong))
            .await
            .unwrap_err(),
        RigsError::SkrBindingInvalid,
    );
    assert_rigs_error(
        send_buy(&mut env, &carol, 1, id, Some(Pubkey::new_unique()))
            .await
            .unwrap_err(),
        RigsError::SkrBindingInvalid,
    );
}

/// Lamports sent to the binding address in advance cannot block creation.
#[tokio::test]
async fn prefunding_the_binding_address_does_not_block_it() {
    let (mut env, _) = day1_env().await;
    let (carol, erin) = (env.carol.insecure_clone(), env.erin.insecure_clone());
    let d = name_of(&env, &carol);
    let ix = system_transfer(&erin.pubkey(), &skr_binding_pda(&d), 5_000_000, true);
    env.send(ix, &[&erin]).await.unwrap();

    env.buy(&carol, 3).await.unwrap();
    let (b, acc) = binding(&mut env, &d).await.unwrap();
    assert_eq!((b.wallet, acc.owner), (carol.pubkey(), sol_rigs::ID));
}

// ---------------------------------------------------- one domain, one wallet

/// Test 12: D used by A on Day 1; B presenting D is rejected — with the
/// binding (bound elsewhere) and without it (B does not own D).
#[tokio::test]
async fn another_wallet_cannot_use_a_bound_domain_on_day1() {
    let (mut env, _) = day1_env().await;
    let (carol, erin) = (env.carol.insecure_clone(), env.erin.insecure_clone());
    let d = name_of(&env, &carol);
    env.buy(&carol, 5).await.unwrap();

    assert_rigs_error(
        send_buy(&mut env, &erin, 1, vec![d], Some(skr_binding_pda(&d)))
            .await
            .unwrap_err(),
        RigsError::SkrDomainBound,
    );
    assert_rigs_error(
        send_buy(&mut env, &erin, 1, vec![d], None)
            .await
            .unwrap_err(),
        RigsError::AccessDenied,
    );
    assert!(env.account_raw(miner_pda(&erin.pubkey())).await.is_none());
}

/// Test 13 (direct `.skr`): D transferred A→B during Day 1. B now owns D
/// and is still rejected; A keeps its Day-1 entitlement up to its cap.
#[tokio::test]
async fn transferring_a_bound_domain_during_day1_does_not_move_access() {
    let (mut env, _) = day1_env().await;
    let (carol, erin) = (env.carol.insecure_clone(), env.erin.insecure_clone());
    let d = name_of(&env, &carol);
    env.buy(&carol, 100).await.unwrap();
    let bytes = binding(&mut env, &d).await.unwrap().1.data;

    transfer_direct(&mut env, &d, &erin.pubkey()).await;
    env.warp(60).await;

    // B proves current ownership of D — and is rejected.
    assert_rigs_error(
        send_buy(&mut env, &erin, 1, vec![d], Some(skr_binding_pda(&d)))
            .await
            .unwrap_err(),
        RigsError::SkrDomainBound,
    );
    // Omitting the binding does not help B either.
    assert_rigs_error(
        send_buy(&mut env, &erin, 1, vec![d], None)
            .await
            .unwrap_err(),
        RigsError::SkrBindingRequired,
    );

    // A no longer owns D but keeps the entitlement bound to it, up to the cap.
    send_buy(&mut env, &carol, 150, vec![d], Some(skr_binding_pda(&d)))
        .await
        .unwrap();
    assert_eq!(env.miner(&carol.pubkey()).await.rigs, 250);
    assert_rigs_error(
        send_buy(&mut env, &carol, 1, vec![d], Some(skr_binding_pda(&d)))
            .await
            .unwrap_err(),
        RigsError::GenesisCapExceeded,
    );
    assert_eq!(
        binding(&mut env, &d).await.unwrap().1.data,
        bytes,
        "never reassigned"
    );
}

/// Test 13 (wrapped `.skr`): the binding is keyed by the ANS name account,
/// so moving the wrapper NFT does not open Day 1 for the new holder.
#[tokio::test]
async fn transferring_a_wrapped_domain_nft_during_day1_does_not_move_access() {
    let (mut env, _) = day1_env().await;
    let (bob, erin) = (env.bob.insecure_clone(), env.erin.insecure_clone());
    let id = env.identities[&bob.pubkey()].clone(); // [name, record, token]
    env.buy(&bob, 5).await.unwrap();
    assert_eq!(
        binding(&mut env, &id[0]).await.unwrap().0.wallet,
        bob.pubkey()
    );

    transfer_wrapped(&mut env, &id[2], &erin.pubkey()).await;
    env.warp(60).await;
    assert_rigs_error(
        send_buy(
            &mut env,
            &erin,
            1,
            id.clone(),
            Some(skr_binding_pda(&id[0])),
        )
        .await
        .unwrap_err(),
        RigsError::SkrDomainBound,
    );
    // The original wallet keeps its entitlement.
    send_buy(&mut env, &bob, 5, id.clone(), Some(skr_binding_pda(&id[0])))
        .await
        .unwrap();
}

/// Test 15: a binding cannot be reassigned or closed during Day 1. No
/// instruction path writes it after creation, it is program-owned (only
/// this program can debit or reassign it, and it has no instruction that
/// does), and attempts through buy leave it byte-identical.
#[tokio::test]
async fn a_binding_cannot_be_reassigned_or_closed_during_day1() {
    let (mut env, t0) = day1_env().await;
    let (carol, erin) = (env.carol.insecure_clone(), env.erin.insecure_clone());
    let d = name_of(&env, &carol);
    env.buy(&carol, 5).await.unwrap();
    let (_, acc) = binding(&mut env, &d).await.unwrap();
    let key = skr_binding_pda(&d);

    // Reassignment attempts by another wallet, before and after a transfer.
    for transfer in [false, true] {
        if transfer {
            transfer_direct(&mut env, &d, &erin.pubkey()).await;
        }
        env.warp(60).await;
        assert!(send_buy(&mut env, &erin, 1, vec![d], Some(key))
            .await
            .is_err());
        assert_eq!(env.account_raw(key).await.unwrap(), acc);
    }

    // Closing it: a system transfer out of a program-owned account fails.
    // Nobody holds a key for the PDA, and the system program cannot debit
    // an account it does not own.
    let ix = system_transfer(&key, &erin.pubkey(), acc.lamports, false);
    assert!(env.send(ix, &[]).await.is_err());
    assert_eq!(env.account_raw(key).await.unwrap(), acc);

    // Through the last Day-1 second, still bound to A.
    env.set_time(t0 + DAY - 1).await;
    assert_rigs_error(
        send_buy(&mut env, &erin, 1, vec![d], Some(key))
            .await
            .unwrap_err(),
        RigsError::SkrDomainBound,
    );
    assert_eq!(env.account_raw(key).await.unwrap(), acc);
}

// ------------------------------------------------------------ public phase

/// Test 14: from T0 + 86,400 the binding restricts nothing. The new owner of
/// a bound domain, a no-domain wallet and an `.sns` wallet all buy; the
/// binding is neither consulted nor changed.
#[tokio::test]
async fn binding_has_no_effect_from_t0_plus_86400() {
    let (mut env, t0) = day1_env().await;
    let (carol, erin, frank) = (
        env.carol.insecure_clone(),
        env.erin.insecure_clone(),
        env.frank.insecure_clone(),
    );
    let d = name_of(&env, &carol);
    env.buy(&carol, 5).await.unwrap();
    transfer_direct(&mut env, &d, &erin.pubkey()).await;
    let acc = env.account_raw(skr_binding_pda(&d)).await.unwrap();

    env.set_time(t0 + DAY - 1).await;
    assert_rigs_error(
        send_buy(&mut env, &erin, 1, vec![d], Some(skr_binding_pda(&d)))
            .await
            .unwrap_err(),
        RigsError::SkrDomainBound,
    );

    env.set_time(t0 + DAY).await; // exact boundary: public
    send_buy(&mut env, &erin, 3, vec![d], Some(skr_binding_pda(&d)))
        .await
        .unwrap();
    send_buy(&mut env, &erin, 2, vec![d], None).await.unwrap();
    env.buy(&frank, 4).await.unwrap();
    assert_eq!(env.miner(&erin.pubkey()).await.rigs, 5);
    assert_eq!(env.account_raw(skr_binding_pda(&d)).await.unwrap(), acc);

    // A never-bound domain creates no binding in public.
    let alice = env.alice.insecure_clone();
    let a = name_of(&env, &alice);
    env.buy(&alice, 1).await.unwrap();
    assert!(binding(&mut env, &a).await.is_none());
}

/// Pins the binding PDA derivation shared with the TypeScript client
/// (`app/tests/instructions.test.ts`), for the name account `[7u8; 32]`.
#[test]
fn binding_pda_matches_the_typescript_client() {
    let name = Pubkey::new_from_array([7u8; 32]);
    let pda = skr_binding_pda(&name);
    println!("skr_bind([7;32]) name={name} pda={pda}");
    assert_eq!(pda.to_string(), PINNED_BINDING_PDA);
}

const PINNED_BINDING_PDA: &str = "3X5vjVDFZJNy9Yc9jMrbWNpXN3U6crbTwSzfyHeihrVH";
