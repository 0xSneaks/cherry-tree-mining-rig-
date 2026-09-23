//! Prints, and pins, the Anchor discriminators the TypeScript client bakes in.
//!
//! `app/src/lib/program/discriminators.ts` hardcodes these bytes so the
//! browser bundle needs no sha256 at runtime. This test fails if the program
//! ever changes an instruction or account name without the client following,
//! which would otherwise show up as an opaque runtime decode failure.

use anchor_lang::{Discriminator, Space};

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn discriminators_match_the_typescript_client() {
    let pairs: Vec<(&str, String)> = vec![
        // instructions
        (
            "initialize",
            hex(sol_rigs::instruction::Initialize::DISCRIMINATOR),
        ),
        (
            "fundRewards",
            hex(sol_rigs::instruction::FundRewards::DISCRIMINATOR),
        ),
        (
            "buyRigs",
            hex(sol_rigs::instruction::BuyRigs::DISCRIMINATOR),
        ),
        ("claim", hex(sol_rigs::instruction::Claim::DISCRIMINATOR)),
        (
            "compound",
            hex(sol_rigs::instruction::Compound::DISCRIMINATOR),
        ),
        (
            "setFeeRecipient",
            hex(sol_rigs::instruction::SetFeeRecipient::DISCRIMINATOR),
        ),
        (
            "withdrawFees",
            hex(sol_rigs::instruction::WithdrawFees::DISCRIMINATOR),
        ),
        // accounts
        ("Config", hex(sol_rigs::state::Config::DISCRIMINATOR)),
        ("Miner", hex(sol_rigs::state::Miner::DISCRIMINATOR)),
        ("Vault", hex(sol_rigs::state::Vault::DISCRIMINATOR)),
    ];

    // Regenerate with: cargo test -p sol-rigs --test discriminators -- --nocapture
    for (name, h) in &pairs {
        println!("{name} = {h}");
    }

    let expected: &[(&str, &str)] = &[
        ("initialize", "afaf6d1f0d989bed"),
        ("fundRewards", "7240a370afa71379"),
        ("buyRigs", "0cfa7948abf4e5a3"),
        ("claim", "3ec6d6c1d59f6cd2"),
        ("compound", "a5d0fb4ef2a08d2f"),
        ("setFeeRecipient", "e312d72aedf69742"),
        ("withdrawFees", "c6d4ab6d90d7ae59"),
        ("Config", "9b0caae01efacc82"),
        ("Miner", "df710f367b7a8c64"),
        ("Vault", "d308e82b02987577"),
    ];

    for ((name, actual), (ename, eexpected)) in pairs.iter().zip(expected) {
        assert_eq!(name, ename);
        assert_eq!(
            actual, eexpected,
            "discriminator for {name} changed — update app/src/lib/program/discriminators.ts"
        );
    }
}

/// Emits a Borsh fixture of a fully-populated `Config` and `Miner`.
///
/// `app/tests/accounts.test.ts` decodes these exact bytes and asserts every
/// field, which is what keeps the hand-written TypeScript decoder honest
/// against the Rust layout.
#[test]
fn emit_account_fixtures_for_the_typescript_decoder() {
    use anchor_lang::prelude::Pubkey;
    use anchor_lang::AccountSerialize;
    use std::str::FromStr;

    let config = sol_rigs::state::Config {
        authority: Pubkey::new_from_array([1u8; 32]),
        pending_authority: Pubkey::new_from_array([2u8; 32]),
        reward_funder: Pubkey::new_from_array([3u8; 32]),
        // The real production fee recipient, so the TypeScript decoder test
        // asserts the exact address rather than a placeholder.
        fee_recipient: Pubkey::from_str("FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp").unwrap(),
        paused: true,
        rig_price_lamports: 10_000_000,
        purchase_fee_bps: 300,
        total_rigs: 14_281,
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
        miner_count: 987,
        capital_vault_bump: 250,
        reward_vault_bump: 251,
        fee_vault_bump: 252,
        version: 1,
        bump: 253,
        // The founder the preflight expects (`sneaks.skr` → this wallet).
        founder: Pubkey::from_str("FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp").unwrap(),
        launch_t0: 1_779_900_000,
        // 14,156 base rigs + 125 referred rigs.
        total_shares: 14_156 * 100 + 125 * 105,
    };
    let miner = sol_rigs::state::Miner {
        owner: Pubkey::new_from_array([9u8; 32]),
        rigs: 42,
        reward_per_rig_paid: 98_765_432_109_876_543_210,
        pending_rewards: 18_291_000,
        lifetime_rigs_bought: 30,
        lifetime_rigs_compounded: 12,
        lifetime_sol_spent: 309_000_000,
        lifetime_sol_compounded: 120_000_000,
        lifetime_sol_claimed: 55_500_000,
        created_at: 1_770_000_000,
        last_action_at: 1_785_000_000,
        bump: 254,
        weight: 105,
        referrer: Pubkey::new_from_array([7u8; 32]),
    };

    let mut cbuf = Vec::new();
    config.try_serialize(&mut cbuf).unwrap();
    let mut mbuf = Vec::new();
    miner.try_serialize(&mut mbuf).unwrap();

    println!("CONFIG_FIXTURE={}", hex(&cbuf));
    println!("MINER_FIXTURE={}", hex(&mbuf));
    println!("CONFIG_SPACE={}", 8 + sol_rigs::state::Config::INIT_SPACE);
    println!("MINER_SPACE={}", 8 + sol_rigs::state::Miner::INIT_SPACE);
}
