//! Adversarial tests for the `initialize` trust boundary.
//!
//! Invariant under test: before the protocol is initialized, only the
//! program's upgrade authority (read from the loader-owned ProgramData PDA)
//! can create Config, become `config.authority`, or choose founder, reward
//! funder and fee recipient. Every rejected attempt must leave Config absent.

mod common;

use anchor_lang::{InstructionData, ToAccountMetas};
use common::*;
use sol_rigs::constants::VaultKind;
use sol_rigs::error::RigsError;
use solana_program_test::{processor, ProgramTest};
use solana_sdk::{
    account::{Account, AccountSharedData},
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transport::TransportError,
};

/// Assert a transaction failed with a specific raw Anchor/custom error code.
fn assert_code(err: TransportError, code: u32) {
    let s = format!("{err:?}");
    assert!(
        s.contains(&format!("Custom({code})")),
        "expected custom {code}, got: {s}"
    );
}

async fn assert_uninitialized(env: &mut Env) {
    for key in [
        config_pda(),
        capital_vault_pda(),
        reward_vault_pda(),
        fee_vault_pda(),
    ] {
        assert!(
            env.account_raw(key).await.is_none(),
            "{key} was created by a rejected initialize"
        );
    }
}

/// Initialize data with every privileged value chosen by `who`.
fn hostile_data(who: &Pubkey) -> Vec<u8> {
    sol_rigs::instruction::Initialize {
        rig_price_lamports: RIG_PRICE,
        purchase_fee_bps: 1_000,
        reward_funder: *who,
        fee_recipient: *who,
        founder: *who,
        genesis_cap_rigs: GENESIS_CAP,
        compound_fee_bps: 1_000,
    }
    .data()
}

// ------------------------------------------------------------------ attacks

/// An arbitrary wallet racing the deployer cannot initialize, even choosing
/// every privileged value itself. The deployer still can afterwards.
#[tokio::test]
async fn attacker_first_initialize_is_rejected() {
    let mut env = Env::new().await;
    let attacker = env.erin.insecure_clone();

    let mut ix = env.initialize_ix(&attacker.pubkey(), program_data_pda());
    ix.data = hostile_data(&attacker.pubkey());
    let err = env.send(ix, &[&attacker]).await.unwrap_err();
    assert_rigs_error(err, RigsError::UnauthorizedInitializer);
    assert_uninitialized(&mut env).await;

    env.initialize().await.unwrap();
    assert_eq!(env.config().await.authority, env.authority.pubkey());
}

/// Naming the real upgrade authority without its signature fails: Anchor's
/// `Signer` check fires before any account is created. (A transaction that
/// marks the key as a signer cannot be built without its private key.)
#[tokio::test]
async fn authority_named_but_not_signing_is_rejected() {
    let mut env = Env::new().await;

    // Submitted and paid by a third party (the harness fee payer) only.
    let mut ix = env.initialize_ix(&env.authority.pubkey(), program_data_pda());
    ix.accounts[0].is_signer = false;
    let err = env.send(ix, &[]).await.unwrap_err();
    assert_code(err, anchor_lang::error::ErrorCode::AccountNotSigner as u32);
    assert_uninitialized(&mut env).await;
}

/// A counterfeit ProgramData — owned by the real loader, naming the attacker
/// as upgrade authority — at any address other than the canonical PDA fails
/// the seeds constraint.
#[tokio::test]
async fn counterfeit_program_data_is_rejected() {
    let fake = Pubkey::new_unique();
    let attacker = Keypair::new();
    let attacker_key = attacker.pubkey();
    let mut env = Env::with(move |pt| {
        pt.add_account(fake, program_data_account(Some(attacker_key)));
        pt.add_account(
            attacker_key,
            Account {
                lamports: 100 * SOL,
                ..Account::default()
            },
        );
    })
    .await;

    let mut ix = env.initialize_ix(&attacker_key, fake);
    ix.data = hostile_data(&attacker_key);
    let err = env.send(ix, &[&attacker]).await.unwrap_err();
    assert_code(err, anchor_lang::error::ErrorCode::ConstraintSeeds as u32);
    assert_uninitialized(&mut env).await;
}

/// A system-owned account posing as ProgramData is refused on ownership.
#[tokio::test]
async fn program_data_with_wrong_owner_is_rejected() {
    let mut env = Env::new().await;
    let attacker = env.erin.insecure_clone();
    // Any non-loader-owned account: the attacker's own wallet.
    let ix = env.initialize_ix(&attacker.pubkey(), attacker.pubkey());
    let err = env.send(ix, &[&attacker]).await.unwrap_err();
    let s = format!("{err:?}");
    assert!(
        s.contains(&format!(
            "Custom({})",
            anchor_lang::error::ErrorCode::AccountOwnedByWrongProgram as u32
        )) || s.contains(&format!(
            "Custom({})",
            anchor_lang::error::ErrorCode::AccountNotInitialized as u32
        )),
        "expected owner rejection, got: {s}"
    );
    assert_uninitialized(&mut env).await;
}

/// Non-canonical Config / vault addresses fail the seeds constraint, even for
/// the legitimate authority.
#[tokio::test]
async fn config_and_vault_substitution_is_rejected() {
    let mut env = Env::new().await;
    let auth = env.authority.insecure_clone();
    for slot in 1..=4 {
        let rogue = Keypair::new();
        let mut ix = env.initialize_ix(&auth.pubkey(), program_data_pda());
        ix.accounts[slot] = AccountMeta::new(rogue.pubkey(), true);
        let err = env.send(ix, &[&auth, &rogue]).await.unwrap_err();
        assert_code(err, anchor_lang::error::ErrorCode::ConstraintSeeds as u32);
        env.warp(1).await;
    }
    assert_uninitialized(&mut env).await;
}

/// Once the upgrade authority is revoked (immutable program), nobody —
/// including the former deployer — can initialize.
#[tokio::test]
async fn immutable_program_cannot_be_initialized() {
    let mut env = Env::new().await;
    env.ctx.set_account(
        &program_data_pda(),
        &AccountSharedData::from(program_data_account(None)),
    );
    let err = env.initialize().await.unwrap_err();
    assert_rigs_error(err, RigsError::UnauthorizedInitializer);
    assert_uninitialized(&mut env).await;
}

/// The binding follows the live upgrade authority: after a rotation the old
/// deployer is refused and the new holder succeeds.
#[tokio::test]
async fn rotated_upgrade_authority_binds_initialize() {
    let mut env = Env::new().await;
    let new_auth = env.frank.insecure_clone();
    env.ctx.set_account(
        &program_data_pda(),
        &AccountSharedData::from(program_data_account(Some(new_auth.pubkey()))),
    );
    let err = env.initialize().await.unwrap_err();
    assert_rigs_error(err, RigsError::UnauthorizedInitializer);
    assert_uninitialized(&mut env).await;

    let ix = env.initialize_ix(&new_auth.pubkey(), program_data_pda());
    env.send(ix, &[&new_auth]).await.unwrap();
    assert_eq!(env.config().await.authority, new_auth.pubkey());
}

// ------------------------------------------------------------ CPI attacker

const ATTACKER_PROGRAM: Pubkey = Pubkey::new_from_array([7u8; 32]);
const ATTACKER_SEED: &[u8] = b"hostile_authority";

fn attacker_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ATTACKER_SEED], &ATTACKER_PROGRAM)
}

/// A foreign program that forwards `data` to `sol_rigs::initialize`, signing
/// as its own PDA. Accounts: `[pda, config, capital, reward, fee, system,
/// program_data, sol_rigs]`.
fn attacker_entry(
    _program_id: &anchor_lang::prelude::Pubkey,
    accounts: &[anchor_lang::prelude::AccountInfo],
    data: &[u8],
) -> anchor_lang::solana_program::entrypoint::ProgramResult {
    let (pda, bump) = attacker_pda();
    let metas = sol_rigs::accounts::Initialize {
        authority: pda,
        config: config_pda(),
        capital_vault: capital_vault_pda(),
        reward_vault: reward_vault_pda(),
        fee_vault: fee_vault_pda(),
        system_program: solana_sdk::system_program::ID,
        program_data: *accounts[6].key,
    }
    .to_account_metas(None);
    let ix = Instruction {
        program_id: sol_rigs::ID,
        accounts: metas,
        data: data.to_vec(),
    };
    anchor_lang::solana_program::program::invoke_signed(&ix, accounts, &[&[ATTACKER_SEED, &[bump]]])
}

/// A foreign program invoking `initialize` through CPI, signing as its own PDA
/// (a signer no user can forge), is refused: the check binds the signer to
/// the upgrade authority, not to the call path.
#[tokio::test]
async fn cpi_from_foreign_program_is_rejected() {
    let (pda, _) = attacker_pda();
    let mut env = Env::with(move |pt: &mut ProgramTest| {
        pt.add_program("hostile_cpi", ATTACKER_PROGRAM, processor!(attacker_entry));
        pt.add_account(
            pda,
            Account {
                lamports: 100 * SOL,
                ..Account::default()
            },
        );
    })
    .await;

    let ix = Instruction {
        program_id: ATTACKER_PROGRAM,
        accounts: vec![
            AccountMeta::new(pda, false),
            AccountMeta::new(config_pda(), false),
            AccountMeta::new(capital_vault_pda(), false),
            AccountMeta::new(reward_vault_pda(), false),
            AccountMeta::new(fee_vault_pda(), false),
            AccountMeta::new_readonly(solana_sdk::system_program::ID, false),
            AccountMeta::new_readonly(program_data_pda(), false),
            AccountMeta::new_readonly(sol_rigs::ID, false),
        ],
        data: hostile_data(&pda),
    };
    let err = env.send(ix, &[]).await.unwrap_err();
    assert_rigs_error(err, RigsError::UnauthorizedInitializer);
    assert_uninitialized(&mut env).await;
}

// ------------------------------------------------------------- legitimate

/// The upgrade authority initializes, and every privileged value and PDA is
/// exactly what it chose.
#[tokio::test]
async fn upgrade_authority_initializes_with_expected_state() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();

    let c = env.config().await;
    assert_eq!(c.authority, env.authority.pubkey());
    assert_eq!(c.pending_authority, Pubkey::default());
    assert_eq!(c.founder, env.founder.pubkey());
    assert_eq!(c.reward_funder, env.funder.pubkey());
    assert_eq!(c.fee_recipient, env.fee_recipient);
    assert_eq!(c.rig_price_lamports, RIG_PRICE);
    assert_eq!(c.purchase_fee_bps, FEE_BPS);
    assert_eq!(c.genesis_cap_rigs, GENESIS_CAP);
    assert_eq!(c.launch_t0, rigs_engine::NOT_ACTIVATED);
    assert!(!c.paused);
    assert_eq!(c.version, sol_rigs::constants::STATE_VERSION);

    let canon = |seed: &[u8]| Pubkey::find_program_address(&[seed], &sol_rigs::ID);
    assert_eq!(c.bump, canon(sol_rigs::constants::CONFIG_SEED).1);
    for (key, kind, stored_bump, seed) in [
        (
            capital_vault_pda(),
            VaultKind::Capital,
            c.capital_vault_bump,
            sol_rigs::constants::CAPITAL_VAULT_SEED,
        ),
        (
            reward_vault_pda(),
            VaultKind::Reward,
            c.reward_vault_bump,
            sol_rigs::constants::REWARD_VAULT_SEED,
        ),
        (
            fee_vault_pda(),
            VaultKind::Fee,
            c.fee_vault_bump,
            sol_rigs::constants::FEE_VAULT_SEED,
        ),
    ] {
        let raw = env.account_raw(key).await.expect("vault missing");
        assert_eq!(raw.owner, sol_rigs::ID);
        let v: sol_rigs::state::Vault = env.anchor_account(key).await;
        assert_eq!(v.kind, kind as u8);
        assert_eq!(v.bump, canon(seed).1);
        assert_eq!(stored_bump, canon(seed).1);
    }
}

/// After a successful initialize, neither the authority nor an attacker can
/// run it again, and Config is left byte-identical.
#[tokio::test]
async fn double_initialize_is_rejected_and_state_is_unchanged() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    let before = env.account_raw(config_pda()).await.unwrap();

    env.warp(1).await;
    assert!(env.initialize().await.is_err());

    let attacker = env.erin.insecure_clone();
    let mut ix = env.initialize_ix(&attacker.pubkey(), program_data_pda());
    ix.data = hostile_data(&attacker.pubkey());
    assert!(env.send(ix, &[&attacker]).await.is_err());

    assert_eq!(
        env.account_raw(config_pda()).await.unwrap().data,
        before.data
    );
}

/// The ProgramData address the clients derive (`app/src/lib/program/pdas.ts`,
/// pinned in `app/tests/ops.test.ts`) is the one the program demands.
#[test]
fn program_data_address_is_pinned() {
    assert_eq!(
        program_data_pda().to_string(),
        "3W8FkxmT9RmES9Duj8jjp2BR9htCNqCZtrvnYLFsETVT"
    );
}
