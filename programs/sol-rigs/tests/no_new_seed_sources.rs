//! Frozen-architecture test 11: only base rewards compound.
//!
//! No loyalty or bonus instruction exists in this program; the lock Season
//! instructions (§8, Pass #7 P7-B) and the single referral-commission payout
//! are exact, reviewed sets. The Fruit ledger instructions (§10a) are an exact, reviewed set
//! whose sources never touch a miner position, a seed count, a base vault or
//! the compound path, and no base instruction touches a Fruit account. The
//! only ways a seed count changes are the two capped engine transitions
//! (`buy_rigs`, `compound`). These are source-level guards: a
//! new instruction, or a new write to a seed count, fails here and has to be
//! reviewed against the cap and the no-new-compound-source rule before this
//! test is updated.

const PROGRAM_LIB: &str = include_str!("../src/lib.rs");
const ENGINE_POSITION: &str = include_str!("../../../crates/rigs-engine/src/position.rs");
const ENGINE_ACCUMULATOR: &str = include_str!("../../../crates/rigs-engine/src/accumulator.rs");
const FRUIT_SOURCES: &[(&str, &str)] = &[
    ("fruit_state.rs", include_str!("../src/fruit_state.rs")),
    ("fruit_merkle.rs", include_str!("../src/fruit_merkle.rs")),
    (
        "instructions/fruit_ledger.rs",
        include_str!("../src/instructions/fruit_ledger.rs"),
    ),
    (
        "instructions/fruit_roots.rs",
        include_str!("../src/instructions/fruit_roots.rs"),
    ),
    (
        "instructions/fruit_claim.rs",
        include_str!("../src/instructions/fruit_claim.rs"),
    ),
    (
        "instructions/fruit_lock.rs",
        include_str!("../src/instructions/fruit_lock.rs"),
    ),
    (
        "engine fruit.rs",
        include_str!("../../../crates/rigs-engine/src/fruit.rs"),
    ),
    (
        "engine fruit_lock.rs",
        include_str!("../../../crates/rigs-engine/src/fruit_lock.rs"),
    ),
];
const PROGRAM_SOURCES: &[(&str, &str)] = &[
    ("state.rs", include_str!("../src/state.rs")),
    ("skr_binding.rs", include_str!("../src/skr_binding.rs")),
    ("vault.rs", include_str!("../src/vault.rs")),
    ("identity.rs", include_str!("../src/identity.rs")),
    ("referral.rs", include_str!("../src/referral.rs")),
    (
        "instructions/admin.rs",
        include_str!("../src/instructions/admin.rs"),
    ),
    (
        "instructions/buy_rigs.rs",
        include_str!("../src/instructions/buy_rigs.rs"),
    ),
    (
        "instructions/claim.rs",
        include_str!("../src/instructions/claim.rs"),
    ),
    (
        "instructions/compound.rs",
        include_str!("../src/instructions/compound.rs"),
    ),
    (
        "instructions/fund_rewards.rs",
        include_str!("../src/instructions/fund_rewards.rs"),
    ),
    (
        "instructions/initialize.rs",
        include_str!("../src/instructions/initialize.rs"),
    ),
    (
        "instructions/migrate_v2.rs",
        include_str!("../src/instructions/migrate_v2.rs"),
    ),
];

/// Names of the `pub fn`s inside the `#[program]` module.
fn program_instructions() -> Vec<String> {
    let body = &PROGRAM_LIB[PROGRAM_LIB.find("#[program]").expect("#[program] module")..];
    body.lines()
        .filter_map(|l| l.trim().strip_prefix("pub fn "))
        .map(|rest| rest.split(['(', '<']).next().unwrap().to_string())
        .collect()
}

/// Names of the `pub fn`s in `src` whose bodies contain `needle`.
fn fns_containing(src: &str, needle: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in src.lines() {
        if let Some(rest) = line.strip_prefix("pub fn ") {
            current = Some(rest.split(['(', '<']).next().unwrap().to_string());
        }
        if line.contains(needle) && !line.trim_start().starts_with("//") {
            if let Some(f) = &current {
                if !out.contains(f) {
                    out.push(f.clone());
                }
            }
        }
    }
    out
}

/// The Fruit ledger instructions: an exact set, each reviewed against the
/// no-seed / no-base-vault rule below.
const FRUIT_INSTRUCTIONS: &[&str] = &[
    "claim_fruit",
    "close_fruit_pool",
    "commit_fruit_pool",
    "finalize_fruit_root",
    "fund_fruit",
    "init_fruit_config",
    "init_fruit_ledger",
    "post_fruit_root",
    "resolve_fruit_pool_terminal",
    "set_fruit_paused",
    "veto_fruit_root",
    "withdraw_fruit_reserve",
];

/// The lock Season instructions (§8, Pass #7 P7-B): an exact set. They live
/// in `instructions/fruit_lock.rs`, which is a Fruit source, so the
/// no-seed / no-base-vault rule below covers them. None is a claim, a
/// compound or a lamport outflow.
const LOCK_INSTRUCTIONS: &[&str] = &[
    "award_lock_registrations",
    "cancel_lock",
    "claim_lock",
    "close_lock_pool",
    "commit_lock_pool",
    "finalize_lock_allocation",
    "finalize_lock_root",
    "post_lock_root",
    "resolve_lock_pool_terminal",
    "post_lock_weights",
    "register_lock",
    "seal_lock_awards",
    "seal_lock_weights",
    "veto_lock_allocation",
];

#[test]
fn the_instruction_set_has_no_bonus_lock_or_commission_path() {
    let mut ixs = program_instructions();
    ixs.sort();
    let mut expected = vec![
        "accept_authority",
        "apply_fee_change",
        "buy_rigs",
        "cancel_fee_change",
        "claim",
        "claim_commission",
        "compound",
        "fund_rewards",
        "initialize",
        "migrate_v2",
        "migrate_v3",
        "schedule_fee_change",
        "set_fee_recipient",
        "set_paused",
        "set_pricing",
        "set_reward_funder",
        "transfer_authority",
        "withdraw_fees",
    ];
    expected.extend_from_slice(FRUIT_INSTRUCTIONS);
    expected.extend_from_slice(LOCK_INSTRUCTIONS);
    expected.sort();
    assert_eq!(ixs, expected);
    // Every instruction naming a lock is one of the reviewed lock set.
    for i in ixs.iter().filter(|i| i.contains("lock")) {
        assert!(
            LOCK_INSTRUCTIONS.contains(&i.as_str()),
            "unreviewed lock instruction {i}"
        );
    }
    for banned in ["loyalty", "bonus"] {
        assert!(
            !ixs.iter().any(|i| i.contains(banned)),
            "a `{banned}` instruction exists; it must never create seeds"
        );
    }
    // Architecture §7a item 4: exactly one commission instruction, the
    // reviewed referrer payout (see `commission_code_never_reaches_seeds`).
    assert_eq!(
        ixs.iter()
            .filter(|i| i.contains("commission"))
            .collect::<Vec<_>>(),
        vec!["claim_commission"]
    );
    // Every instruction naming Fruit is one of the reviewed Fruit set.
    for i in ixs.iter().filter(|i| i.contains("fruit")) {
        assert!(
            FRUIT_INSTRUCTIONS.contains(&i.as_str()),
            "unreviewed Fruit instruction {i}"
        );
    }
}

/// Test 11 (Fruit part): no Fruit source reaches a seed count, a miner
/// position, the compound/purchase path or a base vault; no base source
/// reaches a Fruit account. Fruit lamports can therefore never become seeds
/// or base-vault backing.
#[test]
fn fruit_code_never_reaches_seeds_or_base_vaults() {
    let code = |src: &str| -> String {
        src.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    for (file, src) in FRUIT_SOURCES {
        let src = code(src);
        for needle in [
            "Miner",
            "MinerPosition",
            ".rigs",
            "add_rigs",
            "buy_rigs",
            "compound",
            "RewardGlobals",
            "RewardLedger",
            "CAPITAL_VAULT_SEED",
            "REWARD_VAULT_SEED",
            "FEE_VAULT_SEED",
            "state::Vault",
            "crate::state::{",
            "VaultKind",
        ] {
            assert!(!src.contains(needle), "{file} references `{needle}`");
        }
    }
    for (file, src) in PROGRAM_SOURCES {
        let src = code(src);
        for needle in [
            "FRUIT_",
            "fruit_state",
            "fruit_merkle",
            "FruitVault",
            "fruit::",
        ] {
            assert!(!src.contains(needle), "base {file} references `{needle}`");
        }
    }
}

#[test]
fn seed_counts_change_only_in_the_two_capped_engine_transitions() {
    // Only buy_rigs and compound add rigs to a position or to the globals.
    assert_eq!(
        fns_containing(ENGINE_POSITION, "add_rigs("),
        vec!["buy_rigs", "compound"]
    );
    assert_eq!(
        fns_containing(ENGINE_POSITION, "position.rigs = "),
        vec!["buy_rigs", "compound"]
    );
    // `add_rigs` is crate-private: the program cannot call it directly.
    assert!(ENGINE_ACCUMULATOR.contains("pub(crate) fn add_rigs("));

    // The program writes `Miner.rigs` only by copying an engine position back
    // (`write_position`), and never touches it anywhere else.
    for (file, src) in PROGRAM_SOURCES {
        let writes = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .filter(|l| l.contains(".rigs = ") || l.contains(".rigs += "))
            .count();
        let expected = usize::from(*file == "state.rs");
        assert_eq!(writes, expected, "unexpected seed-count write in {file}");
    }
    for (file, src) in FRUIT_SOURCES {
        assert!(
            !src.contains(".rigs = ") && !src.contains(".rigs += "),
            "seed-count write in {file}"
        );
    }
}

/// Test 11 (commission part): commission SOL is reserved in, and paid out
/// of, the fee vault only. The referral module and the admin instructions
/// (where `claim_commission` lives) never reach a seed count, a miner
/// position, the compound path, the capital or reward vault, or a Fruit
/// account.
#[test]
fn commission_code_never_reaches_seeds() {
    const REFERRAL: &str = include_str!("../src/referral.rs");
    const ADMIN: &str = include_str!("../src/instructions/admin.rs");
    let code = |src: &str| -> String {
        src.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    for (file, src) in [("referral.rs", REFERRAL), ("admin.rs", ADMIN)] {
        let src = code(src);
        for needle in [
            "Miner",
            "MinerPosition",
            ".rigs",
            "add_rigs",
            "buy_rigs",
            "rigs_engine::compound",
            "RewardGlobals",
            "CAPITAL_VAULT_SEED",
            "REWARD_VAULT_SEED",
            "FRUIT_",
            "fruit",
        ] {
            assert!(!src.contains(needle), "{file} references `{needle}`");
        }
    }
    // The referral module moves no lamports out of any vault: it only
    // raises counters (and pays rent for its own ledger PDA).
    let r = code(REFERRAL);
    assert!(!r.contains("move_lamports"));
    assert!(!r.contains("FEE_VAULT_SEED"));
}
