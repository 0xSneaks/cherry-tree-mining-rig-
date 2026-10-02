//! Authority-only instructions.
//!
//! Note what is absent: there is no path from the reward vault or the capital
//! vault to the authority. The only admin money path is the fee vault, and
//! only its unreserved balance (`distributable - reserved_commissions`).

use anchor_lang::prelude::*;
use rigs_engine::validate_pricing;

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::*;
use crate::state::{Config, ReferralLedger, Vault};
use crate::vault::{
    assert_commissions_backed, distributable, move_lamports, require_external_destination,
};

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority,
    )]
    pub config: Account<'info, Config>,
}

pub fn set_paused(ctx: Context<AdminOnly>, paused: bool) -> Result<()> {
    ctx.accounts.config.paused = paused;
    emit!(PausedSet { paused });
    Ok(())
}

/// Seed price is configurable. It affects future purchases only — it cannot
/// touch the accumulator, any accrued balance, or any ore bin.
///
/// Fee rates are NOT changed here (architecture §7a item 3): the
/// `purchase_fee_bps` argument is kept for ABI stability and must equal the
/// current purchase fee; any other value is refused, so no path bypasses the
/// 72-hour fee timelock.
pub fn set_pricing(
    ctx: Context<AdminOnly>,
    rig_price_lamports: u64,
    purchase_fee_bps: u16,
) -> Result<()> {
    validate_pricing(rig_price_lamports, purchase_fee_bps).anchor()?;
    let config = &mut ctx.accounts.config;
    require_eq!(
        purchase_fee_bps,
        config.purchase_fee_bps,
        RigsError::FeeChangeRequiresTimelock
    );
    config.rig_price_lamports = rig_price_lamports;
    emit!(PricingUpdated {
        rig_price_lamports,
        purchase_fee_bps,
    });
    Ok(())
}

/// `schedule_fee_change` (authority): validate `compound <= purchase <=
/// 1,000` bps, store as the pending change with `effective_at = now + 72h`,
/// emit. Replaces any pending change and restarts the full delay.
pub fn schedule_fee_change(
    ctx: Context<AdminOnly>,
    purchase_fee_bps: u16,
    compound_fee_bps: u16,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let config = &mut ctx.accounts.config;
    let mut fees = config.fee_schedule();
    let replaced = fees.has_pending();
    let effective_at = fees
        .schedule(purchase_fee_bps, compound_fee_bps, now)
        .anchor()?;
    config.write_fee_schedule(&fees);
    emit!(FeeChangeScheduled {
        purchase_fee_bps,
        compound_fee_bps,
        effective_at,
        replaced,
    });
    Ok(())
}

/// `cancel_fee_change` (authority).
pub fn cancel_fee_change(ctx: Context<AdminOnly>) -> Result<()> {
    let config = &mut ctx.accounts.config;
    let mut fees = config.fee_schedule();
    let (p, c, at) = (
        fees.pending_purchase_fee_bps,
        fees.pending_compound_fee_bps,
        fees.effective_at,
    );
    fees.cancel().anchor()?;
    config.write_fee_schedule(&fees);
    emit!(FeeChangeCancelled {
        purchase_fee_bps: p,
        compound_fee_bps: c,
        effective_at: at,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct ApplyFeeChange<'info> {
    /// Anyone: applying is permissionless once the timelock has elapsed.
    pub caller: Signer<'info>,

    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,
}

/// `apply_fee_change` (permissionless): only when `now >= effective_at`;
/// writes both values together.
pub fn apply_fee_change(ctx: Context<ApplyFeeChange>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let config = &mut ctx.accounts.config;
    let mut fees = config.fee_schedule();
    let (purchase_fee_bps, compound_fee_bps) = fees.apply(now).anchor()?;
    config.write_fee_schedule(&fees);
    emit!(FeeChangeApplied {
        purchase_fee_bps,
        compound_fee_bps,
        applied_by: ctx.accounts.caller.key(),
    });
    Ok(())
}

pub fn set_reward_funder(ctx: Context<AdminOnly>, reward_funder: Pubkey) -> Result<()> {
    require_keys_neq!(reward_funder, Pubkey::default(), RigsError::InvalidConfig);
    ctx.accounts.config.reward_funder = reward_funder;
    emit!(RewardFunderUpdated { reward_funder });
    Ok(())
}

/// Move where protocol fees are paid.
///
/// Authority-only, like every other config change. The incoming recipient does
/// NOT sign: this sets a payout destination, not a permission, and requiring a
/// signature would mean the recipient's key had to be online to be named.
pub fn set_fee_recipient(ctx: Context<AdminOnly>, fee_recipient: Pubkey) -> Result<()> {
    require_keys_neq!(fee_recipient, Pubkey::default(), RigsError::InvalidConfig);
    let config = &mut ctx.accounts.config;
    let previous = config.fee_recipient;
    config.fee_recipient = fee_recipient;
    emit!(FeeRecipientUpdated {
        previous,
        current: fee_recipient,
    });
    Ok(())
}

/// Step one of a two-step handover. A typo cannot brick the program.
pub fn transfer_authority(ctx: Context<AdminOnly>, new_authority: Pubkey) -> Result<()> {
    ctx.accounts.config.pending_authority = new_authority;
    Ok(())
}

#[derive(Accounts)]
pub struct AcceptAuthority<'info> {
    pub pending_authority: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = config.pending_authority == pending_authority.key()
            @ RigsError::UnauthorizedPendingAuthority,
    )]
    pub config: Account<'info, Config>,
}

pub fn accept_authority(ctx: Context<AcceptAuthority>) -> Result<()> {
    let config = &mut ctx.accounts.config;
    let previous = config.authority;
    config.authority = ctx.accounts.pending_authority.key();
    config.pending_authority = Pubkey::default();
    emit!(AuthorityTransferred {
        previous,
        current: config.authority,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct WithdrawFees<'info> {
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority,
    )]
    pub config: Account<'info, Config>,

    #[account(
        mut,
        seeds = [FEE_VAULT_SEED],
        bump = config.fee_vault_bump,
        constraint = fee_vault.is(VaultKind::Fee) @ RigsError::VaultKindMismatch,
    )]
    pub fee_vault: Account<'info, Vault>,

    /// CHECK: only its lamport balance is touched, and it is pinned to
    /// `config.fee_recipient` by the constraint — the authority chooses
    /// *whether* fees move, never *where* they go.
    #[account(
        mut,
        constraint = destination.key() == config.fee_recipient
            @ RigsError::FeeRecipientMismatch,
    )]
    pub destination: UncheckedAccount<'info>,
}

/// The one auditable admin money path: FEE VAULT -> `config.fee_recipient`
/// (the Protocol Treasury).
///
/// Two separate identities are involved and neither can do the other's job:
/// the authority signs but cannot redirect, and the recipient receives but
/// never signs. Collecting fees therefore never requires the recipient's
/// private key.
///
/// At most `distributable(FEE) - reserved_commissions` may move (§7a item
/// 11): reserved referral commissions are never withdrawable. The recipient
/// may not be an account this program owns, so fees can never be routed back
/// into a vault.
pub fn withdraw_fees(ctx: Context<WithdrawFees>, amount: u64) -> Result<()> {
    require!(amount > 0, RigsError::ZeroAmount);
    require_external_destination(&ctx.accounts.destination.to_account_info(), ctx.program_id)?;

    let fee_vault_ai = ctx.accounts.fee_vault.to_account_info();
    let reserved = ctx.accounts.config.reserved_commissions;
    let withdrawable = distributable(&fee_vault_ai)?
        .checked_sub(reserved)
        .ok_or(error!(RigsError::CommissionReservationBroken))?;
    require!(withdrawable >= amount, RigsError::InsufficientFeeBalance);

    move_lamports(
        &fee_vault_ai,
        &ctx.accounts.destination.to_account_info(),
        amount,
    )?;
    assert_commissions_backed(&fee_vault_ai, reserved)?;

    let config = &mut ctx.accounts.config;
    config.total_fees_withdrawn = config
        .total_fees_withdrawn
        .checked_add(amount)
        .ok_or(error!(RigsError::MathOverflow))?;

    emit!(FeesWithdrawn {
        authority: ctx.accounts.authority.key(),
        destination: ctx.accounts.destination.key(),
        amount,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct ClaimCommission<'info> {
    /// The referrer; receives the commission.
    #[account(mut)]
    pub referrer: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = !config.paused @ RigsError::ProgramPaused,
    )]
    pub config: Account<'info, Config>,

    #[account(
        mut,
        seeds = [REFERRAL_SEED, referrer.key().as_ref()],
        bump = referral_ledger.bump,
        has_one = referrer @ RigsError::ReferralLedgerInvalid,
    )]
    pub referral_ledger: Account<'info, ReferralLedger>,

    #[account(
        mut,
        seeds = [FEE_VAULT_SEED],
        bump = config.fee_vault_bump,
        constraint = fee_vault.is(VaultKind::Fee) @ RigsError::VaultKindMismatch,
    )]
    pub fee_vault: Account<'info, Vault>,
}

/// `claim_commission`: pay the referrer's whole unpaid commission from FEE
/// and release its reservation. `claimed_cum` is raised before a lamport
/// moves, so a replay in the same slot finds nothing unpaid.
pub fn claim_commission(ctx: Context<ClaimCommission>) -> Result<()> {
    let ledger = &mut ctx.accounts.referral_ledger;
    let amount = ledger.unpaid();
    require!(amount > 0, RigsError::NothingToClaimCommission);
    ledger.claimed_cum = ledger
        .claimed_cum
        .checked_add(amount)
        .ok_or(error!(RigsError::MathOverflow))?;

    let config = &mut ctx.accounts.config;
    config.reserved_commissions = config
        .reserved_commissions
        .checked_sub(amount)
        .ok_or(error!(RigsError::CommissionReservationBroken))?;
    config.total_commissions_paid = config
        .total_commissions_paid
        .checked_add(amount)
        .ok_or(error!(RigsError::MathOverflow))?;

    let fee_vault_ai = ctx.accounts.fee_vault.to_account_info();
    move_lamports(
        &fee_vault_ai,
        &ctx.accounts.referrer.to_account_info(),
        amount,
    )?;
    assert_commissions_backed(&fee_vault_ai, config.reserved_commissions)?;

    emit!(CommissionClaimed {
        referrer: ctx.accounts.referrer.key(),
        amount,
        reserved_commissions: config.reserved_commissions,
    });
    Ok(())
}
