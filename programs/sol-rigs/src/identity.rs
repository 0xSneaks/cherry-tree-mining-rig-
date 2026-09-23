//! On-chain proof of `.skr` and `.sns` domain ownership.
//!
//! A buyer proves a domain by passing its registry accounts. The program
//! trusts nothing about them except what it checks here — owner programs,
//! PDAs, parents, owner fields, token balances — so a display name, query
//! string, API payload or client flag can never prove anything.
//!
//! Two authoritative ownership forms per suffix, exactly as each registry's
//! official SDK resolves them:
//!
//! | suffix | direct (1 account)                   | wrapped / tokenized (3 accounts)          |
//! |--------|--------------------------------------|-------------------------------------------|
//! | `.skr` | ANS record, parent `.skr`, owner = wallet | ANS record owned by its Name House NFT record; active record; wallet holds the NFT |
//! | `.sns` | SNS record, parent `.sns`, owner = wallet | SNS record owned by its tokenizer NFT record; active record; wallet holds the NFT |
//!
//! Subdomains are not top-level names and are rejected (wrong parent).

use anchor_lang::prelude::*;
use rigs_engine::ProvenIdentity;

use crate::constants::*;

// ANS NameRecordHeader: disc(8) parent(32) owner(32) class(32)
// expires_at(8) created_at(8) non_transferable(1) padding(79).
const ANS_PARENT: usize = 8;
const ANS_OWNER: usize = 40;
const ANS_EXPIRES: usize = 104;
const ANS_HEADER_LEN: usize = 200;

// AllDomains NftRecord: disc(8) tag(1) bump(1) name_account(32) owner(32)
// nft_mint(32) tld_house(32). Tag 1 = ActiveRecord.
const ANS_NFT_TAG: usize = 8;
const ANS_NFT_NAME: usize = 10;
const ANS_NFT_MINT: usize = 74;
const ANS_NFT_ACTIVE: u8 = 1;

// SNS NameRegistryState: parent(32) owner(32) class(32).
const SNS_PARENT: usize = 0;
const SNS_OWNER: usize = 32;
const SNS_HEADER_LEN: usize = 96;

// SNS tokenizer NftRecord: tag(1) nonce(1) name_account(32) owner(32)
// nft_mint(32). Tag 2 = ActiveRecord.
const SNS_NFT_TAG: usize = 0;
const SNS_NFT_NAME: usize = 2;
const SNS_NFT_MINT: usize = 66;
const SNS_NFT_ACTIVE: u8 = 2;

// SPL token account: mint(32) owner(32) amount(8) … state @108.
const TOKEN_MINT: usize = 0;
const TOKEN_OWNER: usize = 32;
const TOKEN_AMOUNT: usize = 64;
const TOKEN_STATE: usize = 108;

fn key_at(data: &[u8], offset: usize) -> Option<Pubkey> {
    let bytes: [u8; 32] = data.get(offset..offset + 32)?.try_into().ok()?;
    Some(Pubkey::new_from_array(bytes))
}

fn u64_at(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        data.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

/// ANS header checks: owner program, discriminator, `.skr` parent, unexpired.
/// Returns the header's owner field.
fn skr_record_owner(account: &AccountInfo, now: i64) -> Option<Pubkey> {
    if account.owner != &ANS_PROGRAM_ID {
        return None;
    }
    let data = account.try_borrow_data().ok()?;
    if data.len() < ANS_HEADER_LEN || data[..8] != ANS_NAME_RECORD_DISCRIMINATOR {
        return None;
    }
    let expires = u64_at(&data, ANS_EXPIRES)?;
    // 0 = does not expire; otherwise strictly live (no grace period).
    let live = expires == 0 || (now >= 0 && (now as u64) < expires);
    if !live || key_at(&data, ANS_PARENT)? != SKR_TLD_PARENT {
        return None;
    }
    key_at(&data, ANS_OWNER)
}

/// SNS header checks: owner program, `.sns` TLD root parent.
fn sns_record_owner(account: &AccountInfo) -> Option<Pubkey> {
    if account.owner != &SNS_NAME_PROGRAM_ID {
        return None;
    }
    let data = account.try_borrow_data().ok()?;
    if data.len() < SNS_HEADER_LEN || key_at(&data, SNS_PARENT)? != SNS_TLD_ROOT {
        return None;
    }
    key_at(&data, SNS_OWNER)
}

/// `token` is an SPL token account holding exactly one of `mint` for `wallet`.
fn holds_nft(token: &AccountInfo, mint: &Pubkey, wallet: &Pubkey) -> bool {
    if token.owner != &SPL_TOKEN_PROGRAM_ID {
        return false;
    }
    let Ok(d) = token.try_borrow_data() else {
        return false;
    };
    // state: 1 = initialized, 2 = frozen (still held). 0 = uninitialized.
    d.len() >= 165
        && key_at(&d, TOKEN_MINT) == Some(*mint)
        && key_at(&d, TOKEN_OWNER) == Some(*wallet)
        && u64_at(&d, TOKEN_AMOUNT) == Some(1)
        && matches!(d.get(TOKEN_STATE), Some(1) | Some(2))
}

/// `.skr`, direct: the header owner is the wallet.
pub fn owns_skr_direct(name: &AccountInfo, wallet: &Pubkey, now: i64) -> bool {
    skr_record_owner(name, now) == Some(*wallet)
}

/// `.skr`, wrapped as an AllDomains NFT.
pub fn owns_skr_wrapped(
    name: &AccountInfo,
    record: &AccountInfo,
    token: &AccountInfo,
    wallet: &Pubkey,
    now: i64,
) -> bool {
    let Some(header_owner) = skr_record_owner(name, now) else {
        return false;
    };
    let (expected, _) = Pubkey::find_program_address(
        &[b"nft_record", SKR_NAME_HOUSE.as_ref(), name.key.as_ref()],
        &ANS_NAME_HOUSE_PROGRAM_ID,
    );
    if record.key != &expected
        || header_owner != expected
        || record.owner != &ANS_NAME_HOUSE_PROGRAM_ID
    {
        return false;
    }
    let Ok(d) = record.try_borrow_data() else {
        return false;
    };
    if d.len() < ANS_NFT_MINT + 32
        || d[..8] != ANS_NFT_RECORD_DISCRIMINATOR
        || d[ANS_NFT_TAG] != ANS_NFT_ACTIVE
        || key_at(&d, ANS_NFT_NAME) != Some(*name.key)
    {
        return false;
    }
    match key_at(&d, ANS_NFT_MINT) {
        Some(mint) => holds_nft(token, &mint, wallet),
        None => false,
    }
}

/// `.sns`, direct: the header owner is the wallet.
pub fn owns_sns_direct(name: &AccountInfo, wallet: &Pubkey) -> bool {
    sns_record_owner(name) == Some(*wallet)
}

/// `.sns`, tokenized by the SNS name tokenizer.
pub fn owns_sns_tokenized(
    name: &AccountInfo,
    record: &AccountInfo,
    token: &AccountInfo,
    wallet: &Pubkey,
) -> bool {
    let Some(header_owner) = sns_record_owner(name) else {
        return false;
    };
    let (expected, _) =
        Pubkey::find_program_address(&[b"nft_record", name.key.as_ref()], &SNS_NAME_TOKENIZER_ID);
    if record.key != &expected || header_owner != expected || record.owner != &SNS_NAME_TOKENIZER_ID
    {
        return false;
    }
    let Ok(d) = record.try_borrow_data() else {
        return false;
    };
    if d.len() < SNS_NFT_MINT + 32
        || d[SNS_NFT_TAG] != SNS_NFT_ACTIVE
        || key_at(&d, SNS_NFT_NAME) != Some(*name.key)
    {
        return false;
    }
    match key_at(&d, SNS_NFT_MINT) {
        Some(mint) => holds_nft(token, &mint, wallet),
        None => false,
    }
}

/// The identity `accounts` prove for `wallet`: `[name]` for a direct name,
/// `[name, nft_record, token_account]` for a wrapped / tokenized one.
/// `.skr` is checked first because it opens every phase `.sns` does.
pub fn prove(accounts: &[AccountInfo], wallet: &Pubkey, now: i64) -> ProvenIdentity {
    match accounts {
        [name] => {
            if owns_skr_direct(name, wallet, now) {
                ProvenIdentity::Skr
            } else if owns_sns_direct(name, wallet) {
                ProvenIdentity::Sns
            } else {
                ProvenIdentity::None
            }
        }
        [name, record, token] => {
            if owns_skr_wrapped(name, record, token, wallet, now) {
                ProvenIdentity::Skr
            } else if owns_sns_tokenized(name, record, token, wallet) {
                ProvenIdentity::Sns
            } else {
                ProvenIdentity::None
            }
        }
        _ => ProvenIdentity::None,
    }
}
