use anchor_lang::prelude::*;
use rigs_engine::assert_fully_funded;

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::{Claim, EarlyClaimFeeSplit};
use crate::state::{Config, Miner, Vault};
use crate::vault::{assert_commissions_backed, distributable, move_lamports};

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
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

    /// Receives the early-claim fee's 60% treasury share (§7a item 5).
    #[account(
        mut,
        seeds = [FEE_VAULT_SEED],
        bump = config.fee_vault_bump,
        constraint = fee_vault.is(VaultKind::Fee) @ RigsError::VaultKindMismatch,
    )]
    pub fee_vault: Account<'info, Vault>,
}

/// `claim`.
///
/// `pending_rewards` is zeroed by the engine before a single lamport moves, so
/// a replayed or duplicated instruction in the same slot finds an empty ore
/// bin and pays nothing. Rig count is untouched.
pub fn handle_claim(ctx: Context<ClaimRewards>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    let config = &mut ctx.accounts.config;
    let miner = &mut ctx.accounts.miner;

    let mut globals = config.globals();
    let mut ledger = config.ledger();
    let mut position = miner.position();

    // `net` leaves the reward vault to the owner and the early-claim fee's
    // treasury share leaves it to FEE, settling the liability it came from
    // (§7a item 5). The grove share stays, already re-credited to the other
    // planters by the engine. The capital vault is not an account of this
    // instruction, and the treasury share is never referral-eligible.
    let outcome = rigs_engine::claim(&mut globals, &mut position, &mut ledger, now).anchor()?;
    let amount = outcome.net;

    let reward_vault_ai = ctx.accounts.reward_vault.to_account_info();
    let fee_vault_ai = ctx.accounts.fee_vault.to_account_info();
    move_lamports(
        &reward_vault_ai,
        &ctx.accounts.owner.to_account_info(),
        amount,
    )?;
    move_lamports(&reward_vault_ai, &fee_vault_ai, outcome.penalty_treasury)?;
    config.total_claim_penalties = config
        .total_claim_penalties
        .checked_add(outcome.penalty)
        .ok_or(error!(RigsError::MathOverflow))?;
    config.total_protocol_fees = config
        .total_protocol_fees
        .checked_add(outcome.penalty_treasury)
        .ok_or(error!(RigsError::MathOverflow))?;

    assert_fully_funded(distributable(&reward_vault_ai)?, &ledger).anchor()?;
    assert_commissions_backed(&fee_vault_ai, config.reserved_commissions)?;

    config.write_globals(&globals);
    config.write_ledger(&ledger);
    miner.write_position(&position);
    miner.last_action_at = now;

    emit!(Claim {
        owner: ctx.accounts.owner.key(),
        amount,
        gross: outcome.gross,
        penalty: outcome.penalty,
        penalty_bps: outcome.penalty_bps as u16,
        user_rigs: miner.rigs,
        total_reward_claimed: ledger.total_reward_claimed,
    });
    if outcome.penalty > 0 {
        emit!(EarlyClaimFeeSplit {
            owner: ctx.accounts.owner.key(),
            fee: outcome.penalty,
            treasury: outcome.penalty_treasury,
            grove: outcome.penalty_grove,
        });
    }
    Ok(())
}
