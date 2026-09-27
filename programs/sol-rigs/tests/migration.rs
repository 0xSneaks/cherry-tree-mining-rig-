//! Legacy Config migrations — through the real instructions on a real runtime.
//!
//! A V1 Config account (321 bytes) is fabricated exactly as the V1 binary
//! would have written it, planted at the canonical PDA, and migrated in place
//! by `migrate_v2` to the current 375-byte layout (V2's cap plus the §7a fee
//! fields). A V2 account (329 bytes) is migrated by `migrate_v3`. Every
//! failure case proves the account comes back byte- and lamport-identical:
//! the migrations are check-then-mutate, atomic, one-shot (test 7).
//!
//! Compatibility facts pinned here:
//! * a V2 `Account<Config>` cannot deserialize the 321-byte V1 layout — the
//!   "dead window" between upgrade and migration is cleanly inert;
//! * the frozen V1 layout still deserializes a migrated 329-byte account,
//!   ignoring the trailing cap — binary readability, NOT rollback safety
//!   (V1 enforces no cap; rollback to V1 is prohibited in DEPLOY.md).

mod common;

use anchor_lang::{AccountDeserialize, Discriminator, InstructionData, ToAccountMetas};
use borsh::{BorshDeserialize, BorshSerialize};
use common::*;
use rigs_engine::{gate_purchase, genesis_cap_check, PurchaseContext};
use sol_rigs::constants::CONFIG_SEED;
use sol_rigs::error::RigsError;
use sol_rigs::instructions::{V1_CONFIG_SIZE, V2_CONFIG_SIZE, V3_CONFIG_SIZE};
use sol_rigs::state::Config;
use solana_sdk::{
    account::Account,
    instruction::Instruction,
    pubkey::Pubkey,
    rent::Rent,
    signature::{Keypair, Signer},
};
use std::str::FromStr;

const CAP: u64 = GENESIS_CAP; // the only cap migrate_v2 accepts (250)

/// The exact V1 Config payload, field for field. Test-only mirror of what
/// the V1 binary wrote; the program carries its own frozen copy for the
/// migration itself.
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone)]
struct V1Mirror {
    authority: Pubkey,
    pending_authority: Pubkey,
    reward_funder: Pubkey,
    fee_recipient: Pubkey,
    paused: bool,
    rig_price_lamports: u64,
    purchase_fee_bps: u16,
    total_rigs: u64,
    reward_rate: u64,
    reward_period_finish: i64,
    reward_last_update: i64,
    reward_per_rig_stored: u128,
    undistributed_dust: u64,
    total_reward_funded: u64,
    total_reward_claimed: u64,
    total_reward_compounded: u64,
    total_capital_received: u64,
    total_protocol_fees: u64,
    total_fees_withdrawn: u64,
    miner_count: u64,
    capital_vault_bump: u8,
    reward_vault_bump: u8,
    fee_vault_bump: u8,
    version: u16,
    bump: u8,
    founder: Pubkey,
    launch_t0: i64,
    total_shares: u64,
    total_claim_penalties: u64,
}

/// A realistic, live-shaped V1: activated (T0 > 0), populated counters.
fn v1_fixture(authority: &Pubkey) -> V1Mirror {
    V1Mirror {
        authority: *authority,
        pending_authority: Pubkey::default(),
        reward_funder: Pubkey::new_unique(),
        fee_recipient: fee_recipient(),
        paused: false,
        rig_price_lamports: RIG_PRICE,
        purchase_fee_bps: FEE_BPS,
        total_rigs: 13,
        reward_rate: 25_462_962,
        reward_period_finish: 1_790_000_000,
        reward_last_update: 1_780_000_000,
        reward_per_rig_stored: 123_456_789_012_345_678_901_234_567_890,
        undistributed_dust: 4_242,
        total_reward_funded: 14_823_000_000,
        total_reward_claimed: 91_000_000,
        total_reward_compounded: 77_000_000,
        total_capital_received: 512_000_000_000,
        total_protocol_fees: 15_360_000_000,
        total_fees_withdrawn: 1_000_000,
        miner_count: 7,
        capital_vault_bump: 250,
        reward_vault_bump: 251,
        fee_vault_bump: 252,
        version: 1,
        bump: Pubkey::find_program_address(&[CONFIG_SEED], &sol_rigs::ID).1,
        // The founder is a separate wallet from the fee recipient.
        founder: Pubkey::from_str("FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp").unwrap(),
        launch_t0: 1_790_293_911,
        total_shares: 1_300,
        total_claim_penalties: 245_048,
    }
}

fn v1_bytes(m: &V1Mirror) -> Vec<u8> {
    let mut b = Config::DISCRIMINATOR.to_vec();
    m.serialize(&mut b).unwrap();
    assert_eq!(
        b.len(),
        V1_CONFIG_SIZE,
        "the V1 fixture is exactly 321 bytes"
    );
    b
}

fn rent_for(len: usize) -> u64 {
    Rent::default().minimum_balance(len)
}

fn migrate_ix(authority: &Pubkey, config: Pubkey, cap: u64) -> Instruction {
    Instruction {
        program_id: sol_rigs::ID,
        accounts: sol_rigs::accounts::MigrateV2 {
            authority: *authority,
            config,
            system_program: Pubkey::default(),
        }
        .to_account_metas(None),
        data: sol_rigs::instruction::MigrateV2 {
            genesis_cap_rigs: cap,
        }
        .data(),
    }
}

async fn raw(env: &mut Env, key: Pubkey) -> (Vec<u8>, u64) {
    let acc = env.account_raw(key).await.unwrap();
    (acc.data, acc.lamports)
}

// ------------------------------------------------------------- happy path

#[tokio::test]
async fn migrates_321_to_375_preserving_every_v1_byte() {
    let fixture = v1_fixture(&Pubkey::new_unique());
    let mut m = fixture.clone();
    // Plant at exact V1 rent: adjust the helper's funding first.
    let authority = Keypair::new();
    m.authority = authority.pubkey();
    let data = v1_bytes(&m);
    let mut env = Env::with(|pt| {
        pt.add_account(
            authority.pubkey(),
            Account {
                lamports: 1_000 * SOL,
                ..Account::default()
            },
        );
        pt.add_account(
            config_pda(),
            Account {
                lamports: rent_for(V1_CONFIG_SIZE),
                data: data.clone(),
                owner: sol_rigs::ID,
                ..Account::default()
            },
        );
    })
    .await;

    let before = raw(&mut env, config_pda()).await;
    assert_eq!(before.0.len(), 321);
    let auth_before = env.lamports(authority.pubkey()).await;

    let ix = migrate_ix(&authority.pubkey(), config_pda(), CAP);
    env.send(ix, &[&authority]).await.unwrap();

    let after = raw(&mut env, config_pda()).await;
    assert_eq!(after.0.len(), V3_CONFIG_SIZE, "321 -> 375");
    assert_eq!(
        &after.0[..8],
        Config::DISCRIMINATOR,
        "discriminator unchanged"
    );
    assert_eq!(&after.0[..8], &before.0[..8]);
    // Bytes 0..321 identical except the two-byte version field (1 -> 3).
    assert_eq!(&after.0[..262], &before.0[..262]);
    assert_eq!(&after.0[264..321], &before.0[264..321]);
    assert_eq!(&after.0[262..264], &3u16.to_le_bytes());
    // The appended cap, little-endian, in the fresh bytes.
    assert_eq!(&after.0[321..329], &CAP.to_le_bytes());
    // §7a fee fields: compound fee = legacy purchase fee, nothing else.
    assert_eq!(&after.0[329..331], &fixture.purchase_fee_bps.to_le_bytes());
    assert!(after.0[331..375].iter().all(|b| *b == 0));
    // Owner unchanged; lamports are exactly current rent.
    let acc = env.account_raw(config_pda()).await.unwrap();
    assert_eq!(acc.owner, sol_rigs::ID);
    assert_eq!(acc.lamports, rent_for(375), "exactly rent-exempt for 375");
    assert_eq!(
        auth_before - env.lamports(authority.pubkey()).await,
        rent_for(375) - rent_for(321),
        "the authority paid exactly the shortfall"
    );

    // Typed deserialization works on the migrated account, and every V1
    // field survived.
    let cfg = env.config().await;
    assert_eq!((cfg.version, cfg.genesis_cap_rigs), (3, CAP));
    assert_eq!(cfg.compound_fee_bps, fixture.purchase_fee_bps);
    assert_eq!(cfg.fee_change_effective_at, 0);
    assert_eq!(
        (cfg.reserved_commissions, cfg.total_commissions_reserved),
        (0, 0),
        "no retroactive commissions"
    );
    assert_eq!(cfg.authority, authority.pubkey());
    assert_eq!(cfg.fee_recipient, fixture.fee_recipient);
    assert_eq!(cfg.launch_t0, fixture.launch_t0);
    assert_eq!(cfg.total_rigs, fixture.total_rigs);
    assert_eq!(cfg.total_shares, fixture.total_shares);
    assert_eq!(cfg.total_claim_penalties, fixture.total_claim_penalties);
    assert_eq!(cfg.miner_count, fixture.miner_count);
    assert_eq!(cfg.rig_price_lamports, fixture.rig_price_lamports);
    assert_eq!(cfg.purchase_fee_bps, fixture.purchase_fee_bps);
    assert_eq!(cfg.reward_per_rig_stored, fixture.reward_per_rig_stored);
}

/// After migration the account is an ordinary V2 Config: a typed instruction
/// (set_paused, authority-gated) reads and writes it.
#[tokio::test]
async fn migrated_config_serves_typed_v2_instructions() {
    let authority = Keypair::new();
    let m = v1_fixture(&authority.pubkey());
    let data = v1_bytes(&m);
    let mut env = Env::with(|pt| {
        pt.add_account(
            authority.pubkey(),
            Account {
                lamports: 1_000 * SOL,
                ..Account::default()
            },
        );
        pt.add_account(
            config_pda(),
            Account {
                lamports: rent_for(V1_CONFIG_SIZE),
                data: data.clone(),
                owner: sol_rigs::ID,
                ..Account::default()
            },
        );
    })
    .await;
    let ix = migrate_ix(&authority.pubkey(), config_pda(), CAP);
    env.send(ix, &[&authority]).await.unwrap();

    let pause = Instruction {
        program_id: sol_rigs::ID,
        accounts: sol_rigs::accounts::AdminOnly {
            authority: authority.pubkey(),
            config: config_pda(),
        }
        .to_account_metas(None),
        data: sol_rigs::instruction::SetPaused { paused: true }.data(),
    };
    env.send(pause, &[&authority]).await.unwrap();
    assert!(env.config().await.paused);
}

// ------------------------------------------------------- the dead window

/// Before migration, a typed V2 instruction cannot open the 321-byte account:
/// the window between upgrade and migration is cleanly inert, never
/// mixed-mode.
#[tokio::test]
async fn v2_typed_instructions_fail_cleanly_on_unmigrated_v1() {
    let authority = Keypair::new();
    let m = v1_fixture(&authority.pubkey());
    let data = v1_bytes(&m);
    let mut env = Env::with(|pt| {
        pt.add_account(
            authority.pubkey(),
            Account {
                lamports: 1_000 * SOL,
                ..Account::default()
            },
        );
        pt.add_account(
            config_pda(),
            Account {
                lamports: rent_for(V1_CONFIG_SIZE),
                data: data.clone(),
                owner: sol_rigs::ID,
                ..Account::default()
            },
        );
    })
    .await;
    let pause = Instruction {
        program_id: sol_rigs::ID,
        accounts: sol_rigs::accounts::AdminOnly {
            authority: authority.pubkey(),
            config: config_pda(),
        }
        .to_account_metas(None),
        data: sol_rigs::instruction::SetPaused { paused: true }.data(),
    };
    assert!(
        env.send(pause, &[&authority]).await.is_err(),
        "Account<Config> cannot deserialize 321 bytes"
    );
    let now = raw(&mut env, config_pda()).await;
    assert_eq!(now.0, data, "nothing changed");
    assert_eq!(now.1, rent_for(V1_CONFIG_SIZE));
}

// ------------------------------------------------------------- one-shot

#[tokio::test]
async fn second_migration_is_refused_with_nothing_changed() {
    let authority = Keypair::new();
    let m = v1_fixture(&authority.pubkey());
    let data = v1_bytes(&m);
    let mut env = Env::with(|pt| {
        pt.add_account(
            authority.pubkey(),
            Account {
                lamports: 1_000 * SOL,
                ..Account::default()
            },
        );
        pt.add_account(
            config_pda(),
            Account {
                lamports: rent_for(V1_CONFIG_SIZE),
                data: data.clone(),
                owner: sol_rigs::ID,
                ..Account::default()
            },
        );
    })
    .await;
    env.send(
        migrate_ix(&authority.pubkey(), config_pda(), CAP),
        &[&authority],
    )
    .await
    .unwrap();
    let once = raw(&mut env, config_pda()).await;

    let err = env
        // Use a different valid cap so this is a distinct signed transaction,
        // not a duplicate signature that the test bank may accept as already
        // processed. The migrated V2 size/version must reject either value.
        .send(
            migrate_ix(&authority.pubkey(), config_pda(), CAP + 1),
            &[&authority],
        )
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::MigrationRejected);
    let twice = raw(&mut env, config_pda()).await;
    assert_eq!(twice, once, "the refused re-run changed nothing");
}

// ---------------------------------------------------------- failure cases

/// Common shape: plant a (usually corrupted) V1-shaped account, attempt the
/// migration, require `MigrationRejected` and byte/lamport preservation.
async fn rejection_case(
    mut data: Vec<u8>,
    lamports: u64,
    owner: Pubkey,
    config_key: Pubkey,
    cap: u64,
    expected: RigsError,
) {
    let authority = Keypair::new();
    // Keep the signer valid unless the test specifically targets authority.
    // Otherwise a mismatched fixture authority would mask the guard under test
    // (in particular cap validation, which deliberately runs last).
    data[8..40].copy_from_slice(authority.pubkey().as_ref());
    let mut env = Env::with(|pt| {
        pt.add_account(
            authority.pubkey(),
            Account {
                lamports: 1_000 * SOL,
                ..Account::default()
            },
        );
        pt.add_account(
            config_key,
            Account {
                lamports,
                data: data.clone(),
                owner,
                ..Account::default()
            },
        );
    })
    .await;
    let before = raw(&mut env, config_key).await;
    let err = env
        .send(
            migrate_ix(&authority.pubkey(), config_key, cap),
            &[&authority],
        )
        .await
        .unwrap_err();
    assert_rigs_error(err, expected);
    let after = raw(&mut env, config_key).await;
    assert_eq!(after.0, before.0, "data unchanged");
    assert_eq!(after.1, before.1, "lamports unchanged");
}

#[tokio::test]
async fn rejects_unauthorized_signer() {
    let authority = Keypair::new();
    let m = v1_fixture(&authority.pubkey());
    let data = v1_bytes(&m);
    let other = Keypair::new();
    let mut env = Env::with(|pt| {
        pt.add_account(
            authority.pubkey(),
            Account {
                lamports: SOL,
                ..Account::default()
            },
        );
        pt.add_account(
            other.pubkey(),
            Account {
                lamports: 1_000 * SOL,
                ..Account::default()
            },
        );
        pt.add_account(
            config_pda(),
            Account {
                lamports: rent_for(V1_CONFIG_SIZE),
                data,
                owner: sol_rigs::ID,
                ..Account::default()
            },
        );
    })
    .await;
    let before = raw(&mut env, config_pda()).await;
    let err = env
        .send(migrate_ix(&other.pubkey(), config_pda(), CAP), &[&other])
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::MigrationRejected);
    let after = raw(&mut env, config_pda()).await;
    assert_eq!((after.0, after.1), before);
}

#[tokio::test]
async fn rejects_wrong_pda() {
    let authority = Keypair::new();
    let data = v1_bytes(&v1_fixture(&authority.pubkey()));
    let decoy = Pubkey::new_unique();
    rejection_case(
        data,
        rent_for(V1_CONFIG_SIZE),
        sol_rigs::ID,
        decoy,
        CAP,
        RigsError::MigrationRejected,
    )
    .await;
}

#[tokio::test]
async fn rejects_wrong_owner() {
    let authority = Keypair::new();
    let data = v1_bytes(&v1_fixture(&authority.pubkey()));
    rejection_case(
        data,
        rent_for(V1_CONFIG_SIZE),
        Pubkey::default(),
        config_pda(),
        CAP,
        RigsError::MigrationRejected,
    )
    .await;
}

#[tokio::test]
async fn rejects_wrong_discriminator() {
    let authority = Keypair::new();
    let mut data = v1_bytes(&v1_fixture(&authority.pubkey()));
    data[0] ^= 0xff;
    rejection_case(
        data,
        rent_for(V1_CONFIG_SIZE),
        sol_rigs::ID,
        config_pda(),
        CAP,
        RigsError::MigrationRejected,
    )
    .await;
}

#[tokio::test]
async fn rejects_wrong_sizes() {
    let authority = Keypair::new();
    let base = v1_bytes(&v1_fixture(&authority.pubkey()));
    for len in [320usize, 322, 200] {
        let mut data = base.clone();
        data.resize(len, 0);
        rejection_case(
            data,
            rent_for(len),
            sol_rigs::ID,
            config_pda(),
            CAP,
            RigsError::MigrationRejected,
        )
        .await;
    }
}

#[tokio::test]
async fn rejects_wrong_stored_bump() {
    let authority = Keypair::new();
    let mut m = v1_fixture(&authority.pubkey());
    m.bump = if m.bump == 255 { 254 } else { m.bump + 1 };
    rejection_case(
        v1_bytes(&m),
        rent_for(V1_CONFIG_SIZE),
        sol_rigs::ID,
        config_pda(),
        CAP,
        RigsError::MigrationRejected,
    )
    .await;
}

#[tokio::test]
async fn rejects_wrong_versions() {
    let authority = Keypair::new();
    for version in [0u16, 2] {
        let mut m = v1_fixture(&authority.pubkey());
        m.version = version;
        rejection_case(
            v1_bytes(&m),
            rent_for(V1_CONFIG_SIZE),
            sol_rigs::ID,
            config_pda(),
            CAP,
            RigsError::MigrationRejected,
        )
        .await;
    }
}

#[tokio::test]
async fn rejects_invalid_caps() {
    let authority = Keypair::new();
    let data = v1_bytes(&v1_fixture(&authority.pubkey()));
    for cap in [0u64, rigs_engine::MAX_TOTAL_RIGS + 1, u64::MAX] {
        rejection_case(
            data.clone(),
            rent_for(V1_CONFIG_SIZE),
            sol_rigs::ID,
            config_pda(),
            cap,
            RigsError::InvalidConfig,
        )
        .await;
    }
}

// --------------------------------------------------------- compatibility

#[test]
fn v2_config_cannot_deserialize_a_v1_account() {
    let data = v1_bytes(&v1_fixture(&Pubkey::new_unique()));
    assert!(
        Config::try_deserialize(&mut &data[..]).is_err(),
        "the current layout needs 375 bytes; 321 must fail"
    );
    let mut v2 = data.clone();
    v2.extend_from_slice(&CAP.to_le_bytes());
    assert!(
        Config::try_deserialize(&mut &v2[..]).is_err(),
        "the current layout needs 375 bytes; 329 must fail"
    );
}

#[test]
fn v1_layout_reads_a_migrated_account_and_ignores_the_cap() {
    let authority = Pubkey::new_unique();
    let m = v1_fixture(&authority);
    let mut v2 = v1_bytes(&m);
    v2.extend_from_slice(&CAP.to_le_bytes());
    v2[262..264].copy_from_slice(&2u16.to_le_bytes());
    assert_eq!(v2.len(), 329);

    let mut payload: &[u8] = &v2[8..];
    let read = V1Mirror::deserialize(&mut payload).unwrap();
    assert_eq!(read.total_rigs, m.total_rigs);
    assert_eq!(read.launch_t0, m.launch_t0);
    assert_eq!(
        payload.len(),
        8,
        "the trailing cap is ignored, not consumed"
    );
}

/// Semantic proof that a V1 binary would NOT enforce the cap: the V1 gate
/// passes exactly where the V2 cap refuses. Binary readability is not
/// governability — the reason rollback to V1 is prohibited (DEPLOY.md).
#[test]
fn v1_gate_alone_would_ignore_the_cap() {
    let t0 = 1_790_293_911i64;
    let now = t0 + 60;
    let ctx = PurchaseContext {
        launch_t0: t0,
        now,
        is_founder: false,
        schedule_active: true,
        identity: rigs_engine::ProvenIdentity::Skr,
    };
    assert!(gate_purchase(&ctx).is_ok(), "the V1 gate is satisfied");
    assert!(
        genesis_cap_check(t0, now, 9_999, 1, 5).is_err(),
        "the V2 cap is not"
    );
}

// ------------------------------------------------------ V2 → V3 (§7a)

/// A V2 Config (329 bytes): the V1 layout, version 2, plus the cap.
fn v2_bytes(m: &V1Mirror, cap: u64) -> Vec<u8> {
    let mut m = m.clone();
    m.version = 2;
    let mut d = v1_bytes(&m);
    d.extend_from_slice(&cap.to_le_bytes());
    assert_eq!(d.len(), V2_CONFIG_SIZE);
    d
}

fn migrate_v3_ix(authority: &Pubkey, config: Pubkey) -> Instruction {
    Instruction {
        program_id: sol_rigs::ID,
        accounts: sol_rigs::accounts::MigrateV3 {
            authority: *authority,
            config,
            system_program: solana_sdk::system_program::ID,
        }
        .to_account_metas(None),
        data: sol_rigs::instruction::MigrateV3 {}.data(),
    }
}

async fn v2_env(authority: &Keypair, data: Vec<u8>, lamports: u64) -> Env {
    let key = authority.pubkey();
    Env::with(move |pt| {
        pt.add_account(
            key,
            Account {
                lamports: 1_000 * SOL,
                ..Account::default()
            },
        );
        pt.add_account(
            config_pda(),
            Account {
                lamports,
                data,
                owner: sol_rigs::ID,
                ..Account::default()
            },
        );
    })
    .await
}

/// Test 7 (V3): 329 -> 375 in place, every V2 byte preserved except the
/// version, compound fee = legacy purchase fee, nothing pending, counters
/// zero, exact rent; the migrated Config serves typed instructions and
/// charges compound at the (unchanged) legacy rate.
#[tokio::test]
async fn migrate_v3_appends_fee_fields_preserving_every_v2_byte() {
    let authority = Keypair::new();
    let m = v1_fixture(&authority.pubkey());
    let data = v2_bytes(&m, CAP);
    let mut env = v2_env(&authority, data.clone(), rent_for(V2_CONFIG_SIZE)).await;
    let auth_before = env.lamports(authority.pubkey()).await;

    // The inert window: a typed instruction cannot open 329 bytes.
    let pause = Instruction {
        program_id: sol_rigs::ID,
        accounts: sol_rigs::accounts::AdminOnly {
            authority: authority.pubkey(),
            config: config_pda(),
        }
        .to_account_metas(None),
        data: sol_rigs::instruction::SetPaused { paused: true }.data(),
    };
    assert!(env.send(pause.clone(), &[&authority]).await.is_err());
    assert_eq!(raw(&mut env, config_pda()).await.0, data);
    // migrate_v2 refuses a V2 account (wrong size).
    assert_rigs_error(
        env.send(
            migrate_ix(&authority.pubkey(), config_pda(), CAP),
            &[&authority],
        )
        .await
        .unwrap_err(),
        RigsError::MigrationRejected,
    );

    env.send(
        migrate_v3_ix(&authority.pubkey(), config_pda()),
        &[&authority],
    )
    .await
    .unwrap();
    let after = raw(&mut env, config_pda()).await;
    assert_eq!(after.0.len(), V3_CONFIG_SIZE);
    assert_eq!(&after.0[..262], &data[..262]);
    assert_eq!(&after.0[262..264], &3u16.to_le_bytes());
    assert_eq!(&after.0[264..329], &data[264..329]);
    assert_eq!(&after.0[329..331], &m.purchase_fee_bps.to_le_bytes());
    assert!(after.0[331..375].iter().all(|b| *b == 0));
    assert_eq!(after.1, rent_for(V3_CONFIG_SIZE));
    assert_eq!(
        auth_before - env.lamports(authority.pubkey()).await,
        rent_for(V3_CONFIG_SIZE) - rent_for(V2_CONFIG_SIZE)
    );
    let cfg = env.config().await;
    assert_eq!(cfg.version, 3);
    assert_eq!(cfg.genesis_cap_rigs, CAP);
    assert_eq!(cfg.compound_fee_bps, cfg.purchase_fee_bps);
    assert_eq!(cfg.total_protocol_fees, m.total_protocol_fees);
    assert_eq!(cfg.total_fees_withdrawn, m.total_fees_withdrawn);

    // One-shot. (Warp first: a fresh blockhash makes the re-run a distinct
    // transaction rather than a deduplicated replay of the first one.)
    env.warp(1).await;
    let once = raw(&mut env, config_pda()).await;
    assert_rigs_error(
        env.send(
            migrate_v3_ix(&authority.pubkey(), config_pda()),
            &[&authority],
        )
        .await
        .unwrap_err(),
        RigsError::MigrationRejected,
    );
    assert_eq!(raw(&mut env, config_pda()).await, once);

    // Typed instructions now work.
    env.send(pause, &[&authority]).await.unwrap();
    assert!(env.config().await.paused);
}

#[tokio::test]
async fn migrate_v3_rejections_change_nothing() {
    // Wrong signer, wrong version, V1 size, invalid carried cap.
    let authority = Keypair::new();
    let m = v1_fixture(&authority.pubkey());
    let cases: Vec<(Vec<u8>, bool, RigsError)> = vec![
        (v2_bytes(&m, CAP), true, RigsError::MigrationRejected),
        (
            {
                let mut d = v2_bytes(&m, CAP);
                d[262..264].copy_from_slice(&1u16.to_le_bytes());
                d
            },
            false,
            RigsError::MigrationRejected,
        ),
        (
            {
                let mut d = v2_bytes(&m, CAP);
                d[262..264].copy_from_slice(&3u16.to_le_bytes());
                d
            },
            false,
            RigsError::MigrationRejected,
        ),
        (v1_bytes(&m), false, RigsError::MigrationRejected),
        (v2_bytes(&m, CAP + 1), false, RigsError::InvalidConfig),
    ];
    for (data, wrong_signer, expected) in cases {
        let lamports = rent_for(data.len());
        let mut env = v2_env(&authority, data.clone(), lamports).await;
        let signer = if wrong_signer {
            env.bob.insecure_clone()
        } else {
            authority.insecure_clone()
        };
        let err = env
            .send(migrate_v3_ix(&signer.pubkey(), config_pda()), &[&signer])
            .await
            .unwrap_err();
        assert_rigs_error(err, expected);
        let after = raw(&mut env, config_pda()).await;
        assert_eq!((after.0, after.1), (data, lamports), "nothing changed");
    }
}

// ------------------------------------- legacy pricing semantics (V1, V2)

/// Legacy Configs whose pricing violates a current invariant: zero rig
/// price, purchase fee above the 1,000 bps ceiling (so the migrated
/// `compound_fee_bps = purchase_fee_bps` would also be out of range).
fn malformed_pricing(m: &V1Mirror) -> Vec<V1Mirror> {
    let mut out = Vec::new();
    for (price, fee) in [
        (0u64, FEE_BPS),
        (RIG_PRICE, rigs_engine::MAX_PURCHASE_FEE_BPS + 1),
        (RIG_PRICE, u16::MAX),
        (0, u16::MAX),
    ] {
        let mut bad = m.clone();
        bad.rig_price_lamports = price;
        bad.purchase_fee_bps = fee;
        out.push(bad);
    }
    out
}

#[tokio::test]
async fn migrate_v2_rejects_malformed_legacy_pricing_unchanged() {
    let authority = Keypair::new();
    for bad in malformed_pricing(&v1_fixture(&authority.pubkey())) {
        rejection_case(
            v1_bytes(&bad),
            rent_for(V1_CONFIG_SIZE),
            sol_rigs::ID,
            config_pda(),
            CAP,
            RigsError::MigrationRejected,
        )
        .await;
    }
}

#[tokio::test]
async fn migrate_v3_rejects_malformed_legacy_pricing_unchanged() {
    let authority = Keypair::new();
    for bad in malformed_pricing(&v1_fixture(&authority.pubkey())) {
        let data = v2_bytes(&bad, CAP);
        let lamports = rent_for(V2_CONFIG_SIZE);
        let mut env = v2_env(&authority, data.clone(), lamports).await;
        let auth_before = env.lamports(authority.pubkey()).await;
        let err = env
            .send(
                migrate_v3_ix(&authority.pubkey(), config_pda()),
                &[&authority],
            )
            .await
            .unwrap_err();
        assert_rigs_error(err, RigsError::MigrationRejected);
        let after = raw(&mut env, config_pda()).await;
        assert_eq!((after.0, after.1), (data, lamports), "nothing changed");
        // Only the fee for the failed transaction left the authority.
        assert!(auth_before - env.lamports(authority.pubkey()).await < SOL / 100);
    }
}

/// The ceiling itself is valid legacy pricing: a V1 and a V2 Config at
/// exactly 1,000 bps migrate, with compound = purchase = 1,000.
#[tokio::test]
async fn legacy_pricing_at_the_ceiling_migrates() {
    let authority = Keypair::new();
    let mut m = v1_fixture(&authority.pubkey());
    m.purchase_fee_bps = rigs_engine::MAX_PURCHASE_FEE_BPS;

    let data = v1_bytes(&m);
    let mut env = v2_env(&authority, data, rent_for(V1_CONFIG_SIZE)).await;
    env.send(
        migrate_ix(&authority.pubkey(), config_pda(), CAP),
        &[&authority],
    )
    .await
    .unwrap();
    let cfg = env.config().await;
    assert_eq!((cfg.purchase_fee_bps, cfg.compound_fee_bps), (1_000, 1_000));

    let data = v2_bytes(&m, CAP);
    let mut env = v2_env(&authority, data, rent_for(V2_CONFIG_SIZE)).await;
    env.send(
        migrate_v3_ix(&authority.pubkey(), config_pda()),
        &[&authority],
    )
    .await
    .unwrap();
    let cfg = env.config().await;
    assert_eq!((cfg.purchase_fee_bps, cfg.compound_fee_bps), (1_000, 1_000));
    assert_eq!(cfg.fee_recipient, fee_recipient());
    assert_ne!(cfg.founder, cfg.fee_recipient);
}
