//! Frozen-architecture test 11: only base rewards compound.
//!
//! No Fruit, loyalty, lock, bonus or referral-commission path exists in this
//! program, and the only ways a seed count changes are the two capped engine
//! transitions (`buy_rigs`, `compound`). These are source-level guards: a
//! new instruction, or a new write to a seed count, fails here and has to be
//! reviewed against the cap and the no-new-compound-source rule before this
//! test is updated.

const PROGRAM_LIB: &str = include_str!("../src/lib.rs");
const ENGINE_POSITION: &str = include_str!("../../../crates/rigs-engine/src/position.rs");
const ENGINE_ACCUMULATOR: &str = include_str!("../../../crates/rigs-engine/src/accumulator.rs");
const PROGRAM_SOURCES: &[(&str, &str)] = &[
    ("state.rs", include_str!("../src/state.rs")),
    ("skr_binding.rs", include_str!("../src/skr_binding.rs")),
    ("vault.rs", include_str!("../src/vault.rs")),
    ("identity.rs", include_str!("../src/identity.rs")),
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

#[test]
fn the_instruction_set_has_no_fruit_bonus_lock_or_commission_path() {
    let mut ixs = program_instructions();
    ixs.sort();
    let mut expected = vec![
        "accept_authority",
        "buy_rigs",
        "claim",
        "compound",
        "fund_rewards",
        "initialize",
        "migrate_v2",
        "set_fee_recipient",
        "set_paused",
        "set_pricing",
        "set_reward_funder",
        "transfer_authority",
        "withdraw_fees",
    ];
    expected.sort();
    assert_eq!(ixs, expected);
    for banned in ["fruit", "loyalty", "lock", "bonus", "commission"] {
        assert!(
            !ixs.iter().any(|i| i.contains(banned)),
            "a `{banned}` instruction exists; it must never create seeds"
        );
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
}
