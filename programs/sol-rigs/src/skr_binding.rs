//! Day-1 `.skr` binding: one domain opens Day-1 access for one wallet.
//!
//! `identity::prove` checks *current* ownership only, so on its own a domain
//! could be transferred and reused by a second wallet inside the Day-1
//! window. The binding PDA `["skr_bind", name_account]` closes that: the
//! first Day-1 purchase proving a domain records the wallet, and the rule in
//! `rigs_engine::skr_day1_binding` decides every later Day-1 use.
//!
//! The binding account travels in the remaining accounts, directly after the
//! identity accounts: `[name, binding]` (direct) or
//! `[name, nft_record, token_account, binding]` (wrapped). It is only read or
//! created during `[T0, T0 + 24h)`; in every other phase it is ignored.
//!
//! The account is program-owned and no instruction writes, reassigns or
//! closes it after creation.

use anchor_lang::prelude::*;
use anchor_lang::system_program::{allocate, assign, transfer, Allocate, Assign, Transfer};
use anchor_lang::{AccountDeserialize, AccountSerialize};

use crate::constants::SKR_BIND_SEED;
use crate::error::RigsError;
use crate::state::SkrBinding;

/// Split identity-plus-binding remaining accounts into the identity slice and
/// the binding account, if present. Identity is 1 account (direct) or 3
/// (wrapped / tokenized), so a binding makes the slice 2 or 4 long.
pub fn split<'a, 'info>(
    accounts: &'a [AccountInfo<'info>],
) -> (&'a [AccountInfo<'info>], Option<&'a AccountInfo<'info>>) {
    match accounts.len() {
        2 | 4 => {
            let (identity, binding) = accounts.split_at(accounts.len() - 1);
            (identity, binding.first())
        }
        _ => (accounts, None),
    }
}

/// The binding PDA and bump for `name_account`.
pub fn address(name_account: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[SKR_BIND_SEED, name_account.as_ref()], program_id)
}

/// Read the binding at `binding` for `name_account`: `Some(wallet)` if it
/// exists, `None` if it has not been created yet. Anything that is not the
/// canonical PDA, or holds data that is not a binding for this domain, is
/// [`RigsError::SkrBindingInvalid`].
pub fn read(
    binding: &AccountInfo,
    name_account: &Pubkey,
    program_id: &Pubkey,
) -> Result<Option<Pubkey>> {
    let (expected, _) = address(name_account, program_id);
    require_keys_eq!(*binding.key, expected, RigsError::SkrBindingInvalid);

    if binding.owner == program_id {
        let data = binding.try_borrow_data()?;
        let record = SkrBinding::try_deserialize(&mut &data[..])
            .map_err(|_| error!(RigsError::SkrBindingInvalid))?;
        require_keys_eq!(
            record.name_account,
            *name_account,
            RigsError::SkrBindingInvalid
        );
        return Ok(Some(record.wallet));
    }
    // Not yet created. Lamports alone (someone pre-funding the address) do
    // not make it a binding and must not block creation.
    if binding.owner == &System::id() && binding.data_is_empty() {
        return Ok(None);
    }
    err!(RigsError::SkrBindingInvalid)
}

/// Create the binding `name_account → wallet`, paid by `payer`.
///
/// Uses transfer-shortfall + allocate + assign rather than `create_account`,
/// so lamports pre-sent to the PDA cannot grief creation. Only this program
/// can sign for the PDA, so nobody else can allocate or assign it.
pub fn create<'info>(
    binding: &AccountInfo<'info>,
    payer: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    name_account: &Pubkey,
    wallet: &Pubkey,
    now: i64,
    program_id: &Pubkey,
) -> Result<()> {
    let (expected, bump) = address(name_account, program_id);
    require_keys_eq!(*binding.key, expected, RigsError::SkrBindingInvalid);

    let space = 8 + SkrBinding::INIT_SPACE;
    let shortfall = Rent::get()?
        .minimum_balance(space)
        .saturating_sub(binding.lamports());
    if shortfall > 0 {
        transfer(
            CpiContext::new(
                system_program.clone(),
                Transfer {
                    from: payer.clone(),
                    to: binding.clone(),
                },
            ),
            shortfall,
        )?;
    }

    let seeds: &[&[u8]] = &[SKR_BIND_SEED, name_account.as_ref(), &[bump]];
    let signer = &[seeds];
    allocate(
        CpiContext::new_with_signer(
            system_program.clone(),
            Allocate {
                account_to_allocate: binding.clone(),
            },
            signer,
        ),
        space as u64,
    )?;
    assign(
        CpiContext::new_with_signer(
            system_program.clone(),
            Assign {
                account_to_assign: binding.clone(),
            },
            signer,
        ),
        program_id,
    )?;

    let record = SkrBinding {
        name_account: *name_account,
        wallet: *wallet,
        bound_at: now,
        bump,
    };
    let mut data = binding.try_borrow_mut_data()?;
    record.try_serialize(&mut &mut data[..])?;
    Ok(())
}
