//! PDA seeds and vault discriminants.

/// Seed for the singleton CONFIG PDA.
pub const CONFIG_SEED: &[u8] = b"config";
/// Seed prefix for a MINER POSITION PDA: `["miner", owner]`.
pub const MINER_SEED: &[u8] = b"miner";
/// Seed for the CAPITAL VAULT PDA.
pub const CAPITAL_VAULT_SEED: &[u8] = b"capital_vault";
/// Seed for the REWARD VAULT PDA.
pub const REWARD_VAULT_SEED: &[u8] = b"reward_vault";
/// Seed for the FEE VAULT PDA.
pub const FEE_VAULT_SEED: &[u8] = b"fee_vault";

/// On-chain state layout version.
pub const STATE_VERSION: u16 = 1;

/// Marks which vault a [`crate::state::Vault`] account is, so a mis-wired
/// account cannot pose as a different vault even before seed validation.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VaultKind {
    Capital = 0,
    Reward = 1,
    Fee = 2,
}

// --- identity: official name services, pinned (see `identity.rs`) ---

use anchor_lang::prelude::Pubkey;
use anchor_lang::pubkey;

/// AllDomains Alternative Name Service program — the registry `.skr` names
/// (Seeker ID) live in. Source: `@onsol/tldparser` `ANS_PROGRAM_ID`, and
/// docs.solanamobile.com (".skr domains are built on AllDomains").
pub const ANS_PROGRAM_ID: Pubkey = pubkey!("ALTNSZ46uaAUU7XUV6awvdorLGqAsPwa9shm7h4uP2FK");

/// ANS origin ("ANS") name account. `@onsol/tldparser` `ROOT_ANS_PUBLIC_KEY`.
pub const ANS_ORIGIN: Pubkey = pubkey!("3mX9b4AZaQehNoQGfckVcmgmA6bkBoFcbLj9RMmMyNcU");

/// The `.skr` TLD name account: the parent of every `.skr` name record.
/// PDA([sha256("ALT Name Service" ‖ ".skr"), [0;32], ANS_ORIGIN], ANS) —
/// recomputed from first principles in `tests/identity.rs`.
pub const SKR_TLD_PARENT: Pubkey = pubkey!("F3A8kuikEiu6k2399oSJ1PWfcJYDHqpwoQ2e8psSDNuF");

/// The founder's name, without its TLD: `sneaks.skr`.
pub const FOUNDER_NAME: &str = "sneaks";

/// The `sneaks.skr` name record account.
/// PDA([sha256("ALT Name Service" ‖ "sneaks"), [0;32], SKR_TLD_PARENT], ANS) —
/// recomputed in `tests/identity.rs`. The program only ever accepts THIS
/// account as proof of the founder name; a look-alike cannot stand in.
pub const FOUNDER_NAME_ACCOUNT: Pubkey = pubkey!("H4ZaQQaESb69EPFnMLGFde1stMPbb5s15YBnzyDEHxZH");

/// ANS `NameRecordHeader` account discriminator.
pub const ANS_NAME_RECORD_DISCRIMINATOR: [u8; 8] = [68, 72, 88, 44, 15, 167, 103, 243];

/// SPL Name Service program (SNS). `@bonfida/spl-name-service` `NAME_PROGRAM_ID`.
pub const SNS_NAME_PROGRAM_ID: Pubkey = pubkey!("namesLPneVptA9Z5rqUDD9tMTWEJwofgaYwp8cawRkX");

/// SNS root: the parent of every top-level SNS name (shown as `.sns`, formerly
/// `.sol`, since SNS's 2026 migration). `@bonfida/spl-name-service`
/// `SNS_ROOT_DOMAIN_ACCOUNT`.
pub const SNS_ROOT: Pubkey = pubkey!("58PwtjSDuFHuUkYjH9BYnnQKHfwo9reZhC2zMJv9JPkx");
