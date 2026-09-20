use anchor_lang::prelude::*;
use rigs_engine::validate_pricing;

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::Initialized;
use crate::state::{Config, Vault};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = 8 + Config::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump,
    )]
    pub config: Account<'info, Config>,

    #[account(
        init,
        payer = authority,
        space = 8 + Vault::INIT_SPACE,
        seeds = [CAPITAL_VAULT_SEED],
        bump,
    )]
    pub capital_vault: Account<'info, Vault>,

    #[account(
        init,
        payer = authority,
        space = 8 + Vault::INIT_SPACE,
        seeds = [REWARD_VAULT_SEED],
        bump,
    )]
    pub reward_vault: Account<'info, Vault>,

    #[account(
        init,
        payer = authority,
        space = 8 + Vault::INIT_SPACE,
        seeds = [FEE_VAULT_SEED],
        bump,
    )]
    pub fee_vault: Account<'info, Vault>,

    pub system_program: Program<'info, System>,
}

pub fn handle_initialize(
    ctx: Context<Initialize>,
    rig_price_lamports: u64,
    purchase_fee_bps: u16,
    reward_funder: Pubkey,
) -> Result<()> {
    validate_pricing(rig_price_lamports, purchase_fee_bps).anchor()?;
    require_keys_neq!(reward_funder, Pubkey::default(), RigsError::InvalidConfig);

    let now = Clock::get()?.unix_timestamp;

    ctx.accounts.capital_vault.set_inner(Vault {
        kind: VaultKind::Capital as u8,
        bump: ctx.bumps.capital_vault,
    });
    ctx.accounts.reward_vault.set_inner(Vault {
        kind: VaultKind::Reward as u8,
        bump: ctx.bumps.reward_vault,
    });
    ctx.accounts.fee_vault.set_inner(Vault {
        kind: VaultKind::Fee as u8,
        bump: ctx.bumps.fee_vault,
    });

    ctx.accounts.config.set_inner(Config {
        authority: ctx.accounts.authority.key(),
        pending_authority: Pubkey::default(),
        reward_funder,
        paused: false,

        rig_price_lamports,
        purchase_fee_bps,

        total_rigs: 0,
        reward_rate: 0,
        // No funded window yet: both checkpoints sit at `now`, so the first
        // `fund_rewards` starts a clean schedule with no phantom elapsed time.
        reward_period_finish: now,
        reward_last_update: now,
        reward_per_rig_stored: 0,
        undistributed_dust: 0,

        total_reward_funded: 0,
        total_reward_claimed: 0,
        total_reward_compounded: 0,
        total_capital_received: 0,
        total_protocol_fees: 0,
        total_fees_withdrawn: 0,
        miner_count: 0,

        capital_vault_bump: ctx.bumps.capital_vault,
        reward_vault_bump: ctx.bumps.reward_vault,
        fee_vault_bump: ctx.bumps.fee_vault,
        version: STATE_VERSION,
        bump: ctx.bumps.config,
    });

    emit!(Initialized {
        authority: ctx.accounts.authority.key(),
        reward_funder,
        rig_price_lamports,
        purchase_fee_bps,
    });
    Ok(())
}
