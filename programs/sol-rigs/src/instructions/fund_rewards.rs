use anchor_lang::prelude::*;
use rigs_engine::assert_fully_funded;

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::RewardsFunded;
use crate::state::{Config, Vault};
use crate::vault::{deposit_from_wallet, distributable};

#[derive(Accounts)]
pub struct FundRewards<'info> {
    #[account(mut)]
    pub funder: Signer<'info>,

    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = config.reward_funder == funder.key() @ RigsError::UnauthorizedFunder,
    )]
    pub config: Account<'info, Config>,

    #[account(
        mut,
        seeds = [REWARD_VAULT_SEED],
        bump = config.reward_vault_bump,
        constraint = reward_vault.is(VaultKind::Reward) @ RigsError::VaultKindMismatch,
    )]
    pub reward_vault: Account<'info, Vault>,

    pub system_program: Program<'info, System>,
}

/// `fund_rewards(amount, duration)`.
///
/// Backing moves first, schedule is written second. The vault is never
/// promised emission it does not already hold.
pub fn handle_fund_rewards(ctx: Context<FundRewards>, amount: u64, duration: u64) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let reward_vault_ai = ctx.accounts.reward_vault.to_account_info();

    // 1. Backing arrives BEFORE the schedule is written.
    deposit_from_wallet(
        &ctx.accounts.system_program,
        &ctx.accounts.funder,
        &reward_vault_ai,
        amount,
    )?;

    // 2. Roll leftover emission into the new rate and write the schedule.
    let config = &mut ctx.accounts.config;
    let mut globals = config.globals();
    let mut ledger = config.ledger();
    globals.fund(&mut ledger, now, amount, duration).anchor()?;

    // 3. Fully-funded invariant: the vault must cover everything it now owes.
    let available = distributable(&reward_vault_ai)?;
    assert_fully_funded(available, &ledger).anchor()?;

    config.write_globals(&globals);
    config.write_ledger(&ledger);

    emit!(RewardsFunded {
        funder: ctx.accounts.funder.key(),
        amount,
        duration,
        new_reward_rate: globals.reward_rate,
        period_finish: globals.reward_period_finish,
        reward_vault_balance: available,
        outstanding_liabilities: ledger.outstanding().anchor()?,
    });
    Ok(())
}
