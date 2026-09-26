use anchor_lang::prelude::*;
use rigs_engine::assert_fully_funded;

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::Compound;
use crate::referral::{self, FeeEvent};
use crate::state::{Config, Miner, Vault};
use crate::vault::{assert_commissions_backed, distributable, move_lamports};

#[derive(Accounts)]
pub struct CompoundRewards<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = !config.paused @ RigsError::ProgramPaused,
    )]
    pub config: Account<'info, Config>,

    #[account(
        mut,
        seeds = [MINER_SEED, owner.key().as_ref()],
        bump = miner.bump,
        has_one = owner @ RigsError::MinerOwnerMismatch,
    )]
    pub miner: Account<'info, Miner>,

    #[account(
        mut,
        seeds = [REWARD_VAULT_SEED],
        bump = config.reward_vault_bump,
        constraint = reward_vault.is(VaultKind::Reward) @ RigsError::VaultKindMismatch,
    )]
    pub reward_vault: Account<'info, Vault>,

    #[account(
        mut,
        seeds = [CAPITAL_VAULT_SEED],
        bump = config.capital_vault_bump,
        constraint = capital_vault.is(VaultKind::Capital) @ RigsError::VaultKindMismatch,
    )]
    pub capital_vault: Account<'info, Vault>,

    #[account(
        mut,
        seeds = [FEE_VAULT_SEED],
        bump = config.fee_vault_bump,
        constraint = fee_vault.is(VaultKind::Fee) @ RigsError::VaultKindMismatch,
    )]
    pub fee_vault: Account<'info, Vault>,

    /// Pays rent if this compound creates the referrer's referral ledger.
    pub system_program: Program<'info, System>,
}

/// `compound`.
///
/// Spends the ore bin down into whole rigs. The lamports genuinely move:
/// reward vault -> capital vault and reward vault -> fee vault, by exactly the
/// quoted amount. The remainder stays claimable and is never discarded.
///
/// Priced at `compound_fee_bps` (§7a item 2). For a referred position the
/// referrer's commission on the compound fee is reserved in FEE here, in the
/// instruction that receives the fee — never deducted a second time from
/// the owner's rewards.
///
/// Remaining accounts: `[referral_ledger]` iff the position has a referrer.
pub fn handle_compound<'info>(
    ctx: Context<'_, '_, 'info, 'info, CompoundRewards<'info>>,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let referrer = ctx.accounts.miner.referrer;
    let referral_ledger = if referrer != Pubkey::default() {
        let ledger = ctx
            .remaining_accounts
            .last()
            .ok_or(error!(RigsError::MissingReferralLedger))?;
        referral::require_address(ledger, &referrer, ctx.program_id)?;
        Some(ledger)
    } else {
        None
    };

    let config = &mut ctx.accounts.config;
    let miner = &mut ctx.accounts.miner;

    let mut globals = config.globals();
    let mut ledger = config.ledger();
    let mut position = miner.position();

    let outcome = rigs_engine::compound(
        &mut globals,
        &mut position,
        &mut ledger,
        now,
        config.rig_price_lamports,
        config.compound_fee_bps,
    )
    .anchor()?;

    let reward_vault_ai = ctx.accounts.reward_vault.to_account_info();
    move_lamports(
        &reward_vault_ai,
        &ctx.accounts.capital_vault.to_account_info(),
        outcome.capital_lamports,
    )?;
    let fee_vault_ai = ctx.accounts.fee_vault.to_account_info();
    move_lamports(&reward_vault_ai, &fee_vault_ai, outcome.fee_lamports)?;

    config.total_capital_received = config
        .total_capital_received
        .checked_add(outcome.capital_lamports)
        .ok_or(error!(RigsError::MathOverflow))?;
    config.total_protocol_fees = config
        .total_protocol_fees
        .checked_add(outcome.fee_lamports)
        .ok_or(error!(RigsError::MathOverflow))?;

    if let Some(ledger_ai) = referral_ledger {
        referral::reserve(
            config,
            FeeEvent {
                ledger: ledger_ai,
                payer: &ctx.accounts.owner.to_account_info(),
                system_program: &ctx.accounts.system_program.to_account_info(),
                referrer,
                referred: ctx.accounts.owner.key(),
                fee_lamports: outcome.fee_lamports,
                volume_lamports: outcome.capital_lamports,
                compound: true,
            },
            ctx.program_id,
        )?;
    }

    assert_fully_funded(distributable(&reward_vault_ai)?, &ledger).anchor()?;
    assert_commissions_backed(&fee_vault_ai, config.reserved_commissions)?;

    config.write_globals(&globals);
    config.write_ledger(&ledger);
    miner.write_position(&position);
    miner.last_action_at = now;

    emit!(Compound {
        owner: ctx.accounts.owner.key(),
        rigs_minted: outcome.rigs_minted,
        capital_lamports: outcome.capital_lamports,
        fee_lamports: outcome.fee_lamports,
        total_lamports: outcome.total_lamports,
        remainder_lamports: outcome.remainder_lamports,
        user_rigs: miner.rigs,
        total_rigs: globals.total_rigs,
    });
    Ok(())
}
