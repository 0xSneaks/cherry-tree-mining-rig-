//! Lamport movement helpers for program-owned SOL vaults.
//!
//! Vault PDAs are owned by this program and carry a two-byte [`crate::state::Vault`]
//! record, so lamports are moved by direct balance mutation rather than a
//! system CPI. Every debit is guarded so a vault can never be drained below
//! its rent-exempt reserve and disappear.

use anchor_lang::prelude::*;

use crate::error::RigsError;

/// Lamports in `vault` that are actually spendable: balance minus the
/// rent-exempt minimum for its data length.
pub fn distributable(vault: &AccountInfo) -> Result<u64> {
    let reserve = Rent::get()?.minimum_balance(vault.data_len());
    Ok(vault.lamports().saturating_sub(reserve))
}

/// Move `amount` lamports out of a program-owned `from` account into `to`.
///
/// `from` must be owned by this program — the runtime rejects the debit
/// otherwise, which is the backstop behind the seed checks on every caller.
pub fn move_lamports(from: &AccountInfo, to: &AccountInfo, amount: u64) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    require!(distributable(from)? >= amount, RigsError::VaultRentReserve);

    let from_next = from
        .lamports()
        .checked_sub(amount)
        .ok_or(error!(RigsError::MathOverflow))?;
    let to_next = to
        .lamports()
        .checked_add(amount)
        .ok_or(error!(RigsError::MathOverflow))?;

    **from.try_borrow_mut_lamports()? = from_next;
    **to.try_borrow_mut_lamports()? = to_next;
    Ok(())
}

/// Move `amount` lamports from a user's system-owned wallet into a vault via a
/// system-program CPI. Used for purchase capital, fees, and reward funding.
pub fn deposit_from_wallet<'info>(
    system_program: &Program<'info, System>,
    from: &Signer<'info>,
    to: &AccountInfo<'info>,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    anchor_lang::system_program::transfer(
        CpiContext::new(
            system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: from.to_account_info(),
                to: to.clone(),
            },
        ),
        amount,
    )
}
