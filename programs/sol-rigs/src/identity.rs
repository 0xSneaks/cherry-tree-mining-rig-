//! On-chain proof of `.skr` and SNS name ownership.
//!
//! A wallet proves it owns a name by passing the name's registry account. The
//! program trusts nothing about that account except what it can check here:
//!
//! * the account is owned by the official registry program (so its data was
//!   written by that program, and a child of a TLD could only be created
//!   with that TLD's authority), and
//! * its header names the expected parent (the `.skr` TLD, or the SNS root)
//!   and names the SIGNER as owner.
//!
//! Display names, query strings, API payloads and client flags never reach
//! this code, so none of them can prove anything.
//!
//! Not accepted (fail closed, documented in LAUNCH.md): NFT-wrapped /
//! tokenized names — their header owner is an escrow, not the wallet — and
//! the new SRS-based `.sol` names, which no official SDK can resolve yet.

use anchor_lang::prelude::*;

use crate::constants::*;

/// ANS header: discriminator(8) parent(32) owner(32) class(32) expires_at(8)
/// created_at(8) non_transferable(1) padding(79). `@onsol/tldparser`
/// `NameRecordHeader.schema`.
const ANS_PARENT: usize = 8;
const ANS_OWNER: usize = 40;
const ANS_EXPIRES: usize = 104;
const ANS_HEADER_LEN: usize = 200;

/// SNS header: parent(32) owner(32) class(32). `NameRegistryState.HEADER_LEN`.
const SNS_PARENT: usize = 0;
const SNS_OWNER: usize = 32;
const SNS_HEADER_LEN: usize = 96;

fn key_at(data: &[u8], offset: usize) -> Option<Pubkey> {
    let bytes: [u8; 32] = data.get(offset..offset + 32)?.try_into().ok()?;
    Some(Pubkey::new_from_array(bytes))
}

/// `account` is an AllDomains name record under `parent`, owned (unwrapped)
/// by `wallet`, and not expired at `now`.
pub fn owns_ans_name(account: &AccountInfo, parent: &Pubkey, wallet: &Pubkey, now: i64) -> bool {
    if account.owner != &ANS_PROGRAM_ID {
        return false;
    }
    let Ok(data) = account.try_borrow_data() else {
        return false;
    };
    if data.len() < ANS_HEADER_LEN || data[..8] != ANS_NAME_RECORD_DISCRIMINATOR {
        return false;
    }
    let expires = match data.get(ANS_EXPIRES..ANS_EXPIRES + 8) {
        Some(b) => u64::from_le_bytes(b.try_into().unwrap_or([0xff; 8])),
        None => return false,
    };
    // 0 = does not expire. Otherwise it must still be live: no grace period
    // is honoured on-chain, which is the strict direction.
    let live = expires == 0 || (now >= 0 && (now as u64) < expires);
    live && key_at(&data, ANS_PARENT) == Some(*parent) && key_at(&data, ANS_OWNER) == Some(*wallet)
}

/// `account` is a top-level SNS name record owned directly by `wallet`.
pub fn owns_sns_name(account: &AccountInfo, wallet: &Pubkey) -> bool {
    if account.owner != &SNS_NAME_PROGRAM_ID {
        return false;
    }
    let Ok(data) = account.try_borrow_data() else {
        return false;
    };
    data.len() >= SNS_HEADER_LEN
        && key_at(&data, SNS_PARENT) == Some(SNS_ROOT)
        && key_at(&data, SNS_OWNER) == Some(*wallet)
}

/// Proof that `wallet` owns `sneaks.skr` right now: exactly the pinned name
/// account, with the signer as its owner.
pub fn owns_founder_name(account: &AccountInfo, wallet: &Pubkey, now: i64) -> bool {
    account.key == &FOUNDER_NAME_ACCOUNT && owns_ans_name(account, &SKR_TLD_PARENT, wallet, now)
}

/// The strongest identity `account` proves for `wallet`.
pub fn prove(
    account: Option<&AccountInfo>,
    wallet: &Pubkey,
    now: i64,
) -> rigs_engine::ProvenIdentity {
    use rigs_engine::ProvenIdentity;
    match account {
        Some(a) if owns_ans_name(a, &SKR_TLD_PARENT, wallet, now) => ProvenIdentity::Skr,
        Some(a) if owns_sns_name(a, wallet) => ProvenIdentity::Sns,
        _ => ProvenIdentity::None,
    }
}
