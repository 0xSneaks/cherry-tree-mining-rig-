//! Referral commission (architecture §7 / §7a item 4).
//!
//! One direct level. In the instruction that receives an eligible fee into
//! FEE (a referred wallet's fresh purchase or completed compound), the
//! commission `min(floor(0.20 f), max(0, f - ceil(0.08 v)))` is reserved:
//! `Config.reserved_commissions` and the referrer's REFERRAL LEDGER PDA
//! `["referral", referrer]` both rise by it. `withdraw_fees` can only touch
//! `distributable(FEE) - reserved_commissions`; `claim_commission` pays the
//! referrer from FEE and lowers both.
//!
//! The ledger travels as the LAST remaining account of `buy_rigs` /
//! `compound` whenever the position is referred, so its presence is decided
//! by on-chain state (the Miner's immutable referrer), never by the client.
//! It is created, paid by the signer, the first time a commission is
//! reserved; the account is never closed.

use anchor_lang::prelude::*;
use anchor_lang::system_program::{allocate, assign, transfer, Allocate, Assign, Transfer};
use anchor_lang::{AccountDeserialize, AccountSerialize};
use rigs_engine::referral_commission;

use crate::constants::REFERRAL_SEED;
use crate::error::RigsError;
use crate::events::CommissionReserved;
use crate::state::{Config, ReferralLedger};

/// The referral ledger PDA and bump for `referrer`.
pub fn address(referrer: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[REFERRAL_SEED, referrer.as_ref()], program_id)
}

/// Check that `ledger` is the canonical ledger PDA for `referrer`.
pub fn require_address(ledger: &AccountInfo, referrer: &Pubkey, program_id: &Pubkey) -> Result<()> {
    let (expected, _) = address(referrer, program_id);
    require_keys_eq!(*ledger.key, expected, RigsError::ReferralLedgerInvalid);
    Ok(())
}

/// Everything an eligible fee event needs to reserve a commission.
pub struct FeeEvent<'a, 'info> {
    pub ledger: &'a AccountInfo<'info>,
    pub payer: &'a AccountInfo<'info>,
    pub system_program: &'a AccountInfo<'info>,
    pub referrer: Pubkey,
    pub referred: Pubkey,
    /// Actual fee received in FEE by this instruction.
    pub fee_lamports: u64,
    /// Fee-bearing principal volume of the event.
    pub volume_lamports: u64,
    pub compound: bool,
}

/// Reserve the commission on one eligible fee event, exactly once (it is
/// called once per instruction, after the fee has moved into FEE). Returns
/// the commission. A zero commission touches no account.
pub fn reserve(config: &mut Config, ev: FeeEvent, program_id: &Pubkey) -> Result<u64> {
    require_address(ev.ledger, &ev.referrer, program_id)?;
    let commission = referral_commission(ev.fee_lamports, ev.volume_lamports)
        .map_err(|_| error!(RigsError::MathOverflow))?;
    if commission == 0 {
        return Ok(0);
    }

    let mut record = if ev.ledger.owner == program_id {
        let data = ev.ledger.try_borrow_data()?;
        let r = ReferralLedger::try_deserialize(&mut &data[..])
            .map_err(|_| error!(RigsError::ReferralLedgerInvalid))?;
        require_keys_eq!(r.referrer, ev.referrer, RigsError::ReferralLedgerInvalid);
        r
    } else if ev.ledger.owner == &System::id() && ev.ledger.data_is_empty() {
        create(
            ev.ledger,
            ev.payer,
            ev.system_program,
            &ev.referrer,
            program_id,
        )?
    } else {
        return err!(RigsError::ReferralLedgerInvalid);
    };

    record.accrued_cum = record
        .accrued_cum
        .checked_add(commission)
        .ok_or(error!(RigsError::MathOverflow))?;
    {
        let mut data = ev.ledger.try_borrow_mut_data()?;
        record.try_serialize(&mut &mut data[..])?;
    }

    config.reserved_commissions = config
        .reserved_commissions
        .checked_add(commission)
        .ok_or(error!(RigsError::MathOverflow))?;
    config.total_commissions_reserved = config
        .total_commissions_reserved
        .checked_add(commission)
        .ok_or(error!(RigsError::MathOverflow))?;

    emit!(CommissionReserved {
        referrer: ev.referrer,
        referred: ev.referred,
        compound: ev.compound,
        fee_lamports: ev.fee_lamports,
        volume_lamports: ev.volume_lamports,
        commission,
        reserved_commissions: config.reserved_commissions,
    });
    Ok(commission)
}

/// Create the ledger for `referrer`, paid by `payer`. Transfer-shortfall +
/// allocate + assign, so lamports pre-sent to the PDA cannot grief creation.
fn create<'info>(
    ledger: &AccountInfo<'info>,
    payer: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    referrer: &Pubkey,
    program_id: &Pubkey,
) -> Result<ReferralLedger> {
    let (expected, bump) = address(referrer, program_id);
    require_keys_eq!(*ledger.key, expected, RigsError::ReferralLedgerInvalid);

    let space = 8 + ReferralLedger::INIT_SPACE;
    let shortfall = Rent::get()?
        .minimum_balance(space)
        .saturating_sub(ledger.lamports());
    if shortfall > 0 {
        transfer(
            CpiContext::new(
                system_program.clone(),
                Transfer {
                    from: payer.clone(),
                    to: ledger.clone(),
                },
            ),
            shortfall,
        )?;
    }
    let seeds: &[&[u8]] = &[REFERRAL_SEED, referrer.as_ref(), &[bump]];
    let signer = &[seeds];
    allocate(
        CpiContext::new_with_signer(
            system_program.clone(),
            Allocate {
                account_to_allocate: ledger.clone(),
            },
            signer,
        ),
        space as u64,
    )?;
    assign(
        CpiContext::new_with_signer(
            system_program.clone(),
            Assign {
                account_to_assign: ledger.clone(),
            },
            signer,
        ),
        program_id,
    )?;
    Ok(ReferralLedger {
        referrer: *referrer,
        accrued_cum: 0,
        claimed_cum: 0,
        bump,
    })
}
