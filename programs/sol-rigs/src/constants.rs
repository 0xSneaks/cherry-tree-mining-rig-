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

// --- identity: official name registries, pinned (see `identity.rs`) ---

use anchor_lang::prelude::Pubkey;
use anchor_lang::pubkey;

// .skr — AllDomains (Seeker ID). docs.solanamobile.com: ".skr domains are
// built on AllDomains"; resolver SDK `@onsol/tldparser`.

/// AllDomains Alternative Name Service program (`ANS_PROGRAM_ID`).
pub const ANS_PROGRAM_ID: Pubkey = pubkey!("ALTNSZ46uaAUU7XUV6awvdorLGqAsPwa9shm7h4uP2FK");
/// ANS origin name account (`ROOT_ANS_PUBLIC_KEY`).
pub const ANS_ORIGIN: Pubkey = pubkey!("3mX9b4AZaQehNoQGfckVcmgmA6bkBoFcbLj9RMmMyNcU");
/// The `.skr` TLD name account — parent of every `.skr` name record.
/// PDA([sha256("ALT Name Service" ‖ ".skr"), [0;32], ANS_ORIGIN], ANS);
/// re-derived in `tests/launch.rs`.
pub const SKR_TLD_PARENT: Pubkey = pubkey!("F3A8kuikEiu6k2399oSJ1PWfcJYDHqpwoQ2e8psSDNuF");
/// ANS `NameRecordHeader` account discriminator.
pub const ANS_NAME_RECORD_DISCRIMINATOR: [u8; 8] = [68, 72, 88, 44, 15, 167, 103, 243];
/// AllDomains Name House program: holds the record of a name wrapped as an
/// NFT (`NAME_HOUSE_PROGRAM_ID`).
pub const ANS_NAME_HOUSE_PROGRAM_ID: Pubkey =
    pubkey!("NH3uX6FtVE2fNREAioP7hm5RaozotZxeL6khU1EHx51");
/// The `.skr` name house: PDA(["name_house", PDA(["tld_house", ".skr"],
/// TLD House)], Name House); re-derived in `tests/launch.rs`.
pub const SKR_NAME_HOUSE: Pubkey = pubkey!("CPD3hKJE51VuiDE5u5Nosx1u7MXWwKGPff8HFJTtpX4V");
/// AllDomains TLD House program (only used to re-derive SKR_NAME_HOUSE).
pub const ANS_TLD_HOUSE_PROGRAM_ID: Pubkey = pubkey!("TLDHkysf5pCnKsVA4gXpNvmy7psXLPEu4LAdDJthT9S");
/// AllDomains `NftRecord` account discriminator.
pub const ANS_NFT_RECORD_DISCRIMINATOR: [u8; 8] = [174, 190, 114, 100, 177, 14, 90, 254];

// .sns — SNS (sns.id). dev.sns.id: `SNS_ROOT_DOMAIN_ACCOUNT` — "The `.sns`
// TLD"; `getSnsDomainKeysForOwner` — "top-level `.sns` domain accounts owned
// by a wallet"; `getSnsNftsForOwner` — "tokenized `.sns` domains".

/// SPL Name Service program (`NAME_PROGRAM_ID`).
pub const SNS_NAME_PROGRAM_ID: Pubkey = pubkey!("namesLPneVptA9Z5rqUDD9tMTWEJwofgaYwp8cawRkX");
/// The `.sns` TLD root (`SNS_ROOT_DOMAIN_ACCOUNT`).
pub const SNS_TLD_ROOT: Pubkey = pubkey!("58PwtjSDuFHuUkYjH9BYnnQKHfwo9reZhC2zMJv9JPkx");
/// SNS name tokenizer program (`NAME_TOKENIZER_ID`).
pub const SNS_NAME_TOKENIZER_ID: Pubkey = pubkey!("nftD3vbNkNqfj2Sd3HZwbpw4BxxKWr4AjGb9X38JeZk");

/// SPL Token program: both registries mint their name NFTs here.
pub const SPL_TOKEN_PROGRAM_ID: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
