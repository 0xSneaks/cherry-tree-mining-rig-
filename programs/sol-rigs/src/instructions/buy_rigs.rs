use anchor_lang::prelude::*;
use rigs_engine::assert_fully_funded;

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::RigsPurchased;
use crate::state::{Config, Miner, Vault};
use crate::vault::{deposit_from_wallet, distributable};

#[derive(Accounts)]
pub struct BuyRigs<'info> {
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
        init_if_needed,
        payer = owner,
        space = 8 + Miner::INIT_SPACE,
        seeds = [MINER_SEED, owner.key().as_ref()],
        bump,
    )]
    pub miner: Account<'info, Miner>,

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

    /// Read-only. Present so the fully-funded invariant can be asserted on an
    /// instruction that advances the reward accumulator.
    #[account(
        seeds = [REWARD_VAULT_SEED],
        bump = config.reward_vault_bump,
        constraint = reward_vault.is(VaultKind::Reward) @ RigsError::VaultKindMismatch,
    )]
    pub reward_vault: Account<'info, Vault>,

    pub system_program: Program<'info, System>,
}

/// `buy_rigs(count)`.
///
/// Purchase lamports split between the CAPITAL and FEE vaults. Not one lamport
/// reaches the REWARD vault — that is the core invariant, and it is structural
/// here: the reward vault is passed read-only.
pub fn handle_buy_rigs(ctx: Context<BuyRigs>, count: u64) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let owner_key = ctx.accounts.owner.key();

    // First purchase initialises the position.
    let miner = &mut ctx.accounts.miner;
    let is_new = miner.owner == Pubkey::default();
    if is_new {
        miner.owner = owner_key;
        miner.created_at = now;
        miner.bump = ctx.bumps.miner;
    } else {
        require_keys_eq!(miner.owner, owner_key, RigsError::MinerOwnerMismatch);
    }

    let config = &mut ctx.accounts.config;
    let mut globals = config.globals();
    let mut position = miner.position();

    let quote = rigs_engine::buy_rigs(
        &mut globals,
        &mut position,
        now,
        count,
        config.rig_price_lamports,
        config.purchase_fee_bps,
    )
    .anchor()?;

    // Capital and fee leave the USER's wallet, never the reward vault.
    deposit_from_wallet(
        &ctx.accounts.system_program,
        &ctx.accounts.owner,
        &ctx.accounts.capital_vault.to_account_info(),
        quote.capital_lamports,
    )?;
    deposit_from_wallet(
        &ctx.accounts.system_program,
        &ctx.accounts.owner,
        &ctx.accounts.fee_vault.to_account_info(),
        quote.fee_lamports,
    )?;

    config.total_capital_received = config
        .total_capital_received
        .checked_add(quote.capital_lamports)
        .ok_or(error!(RigsError::MathOverflow))?;
    config.total_protocol_fees = config
        .total_protocol_fees
        .checked_add(quote.fee_lamports)
        .ok_or(error!(RigsError::MathOverflow))?;
    if is_new {
        config.miner_count = config
            .miner_count
            .checked_add(1)
            .ok_or(error!(RigsError::MathOverflow))?;
    }

    let ledger = config.ledger();
    assert_fully_funded(
        distributable(&ctx.accounts.reward_vault.to_account_info())?,
        &ledger,
    )
    .anchor()?;

    config.write_globals(&globals);
    miner.write_position(&position);
    miner.last_action_at = now;

    emit!(RigsPurchased {
        owner: owner_key,
        count,
        capital_lamports: quote.capital_lamports,
        fee_lamports: quote.fee_lamports,
        total_lamports: quote.total_lamports,
        user_rigs: miner.rigs,
        total_rigs: globals.total_rigs,
    });
    Ok(())
}
