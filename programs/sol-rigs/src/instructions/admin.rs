//! Authority-only instructions.
//!
//! Note what is absent: there is no path from the reward vault or the capital
//! vault to the authority. The only admin money path in V1 is the fee vault.

use anchor_lang::prelude::*;
use rigs_engine::validate_pricing;

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::*;
use crate::state::{Config, Vault};
use crate::vault::{distributable, move_lamports};

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

/// Pricing is configurable. It affects future purchases only — it cannot touch
/// the accumulator, any accrued balance, or any ore bin.
pub fn set_pricing(
    ctx: Context<AdminOnly>,
    rig_price_lamports: u64,
    purchase_fee_bps: u16,
) -> Result<()> {
    validate_pricing(rig_price_lamports, purchase_fee_bps).anchor()?;
    let config = &mut ctx.accounts.config;
    config.rig_price_lamports = rig_price_lamports;
    config.purchase_fee_bps = purchase_fee_bps;
    emit!(PricingUpdated {
        rig_price_lamports,
        purchase_fee_bps,
    });
    Ok(())
}

pub fn set_reward_funder(ctx: Context<AdminOnly>, reward_funder: Pubkey) -> Result<()> {
    require_keys_neq!(reward_funder, Pubkey::default(), RigsError::InvalidConfig);
    ctx.accounts.config.reward_funder = reward_funder;
    emit!(RewardFunderUpdated { reward_funder });
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

    /// CHECK: destination for the withdrawal; only its lamport balance is
    /// touched, and the authority chooses it.
    #[account(mut)]
    pub destination: UncheckedAccount<'info>,
}

/// The one auditable admin money path in V1: FEE VAULT -> destination.
pub fn withdraw_fees(ctx: Context<WithdrawFees>, amount: u64) -> Result<()> {
    require!(amount > 0, RigsError::ZeroAmount);

    let fee_vault_ai = ctx.accounts.fee_vault.to_account_info();
    require!(
        distributable(&fee_vault_ai)? >= amount,
        RigsError::InsufficientFeeBalance
    );

    move_lamports(
        &fee_vault_ai,
        &ctx.accounts.destination.to_account_info(),
        amount,
    )?;

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
