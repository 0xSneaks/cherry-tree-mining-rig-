use anchor_lang::prelude::*;
use rigs_engine::{
    assert_fully_funded, gate_purchase, referral_weight, t0_after_success, PurchaseContext,
    PurchaseGate, ReferrerCheck,
};

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::{LaunchActivated, ReferralBound, RigsPurchased};
use crate::identity;
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

/// `buy_rigs(count, referrer)`.
///
/// Purchase lamports split between the CAPITAL and FEE vaults. Not one lamport
/// reaches the REWARD vault — that is the core invariant, and it is structural
/// here: the reward vault is passed read-only.
///
/// Launch gate, decided by `rigs_engine::gate_purchase` before anything moves:
///
/// * `launch_t0 == 0`: only the ACTIVATION is possible — the pinned founder,
///   proving in this instruction that it owns `sneaks.skr`, with an active
///   funded schedule. On success `launch_t0 := Clock` in this same
///   instruction, so a failed purchase can never leave T0 behind.
/// * afterwards: the access phase (from T0 and the clock only) decides which
///   proven identity may buy.
pub fn handle_buy_rigs<'info>(
    ctx: Context<'_, '_, 'info, 'info, BuyRigs<'info>>,
    count: u64,
    referrer: Option<Pubkey>,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let owner_key = ctx.accounts.owner.key();
    let identity_account = ctx.remaining_accounts.first();

    // --- launch gate ---
    let gate = {
        let config = &ctx.accounts.config;
        gate_purchase(&PurchaseContext {
            launch_t0: config.launch_t0,
            now,
            is_founder: owner_key == config.founder,
            owns_founder_name: identity_account
                .map(|a| identity::owns_founder_name(a, &owner_key, now))
                .unwrap_or(false),
            schedule_active: config.reward_rate > 0 && now < config.reward_period_finish,
            identity: identity::prove(identity_account, &owner_key, now),
        })
        .anchor()?
    };

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

    // --- referral: bound once, at creation, to an existing planter ---
    let referrer_check = match referrer {
        None => None,
        Some(r) => {
            let acct = ctx
                .remaining_accounts
                .get(1)
                .ok_or(error!(RigsError::MissingReferrerAccount))?;
            let (expected, _) =
                Pubkey::find_program_address(&[MINER_SEED, r.as_ref()], ctx.program_id);
            let is_planter = acct.key == &expected
                && acct.owner == ctx.program_id
                && Account::<Miner>::try_from(acct)
                    .map(|m| m.owner == r && m.rigs > 0)
                    .unwrap_or(false);
            Some(ReferrerCheck {
                referrer: r.to_bytes(),
                referrer_is_planter: is_planter,
            })
        }
    };
    let weight = referral_weight(owner_key.to_bytes(), is_new, referrer_check).anchor()?;
    if is_new {
        miner.weight = u16::try_from(weight).map_err(|_| error!(RigsError::MathOverflow))?;
        miner.referrer = referrer.unwrap_or_default();
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

    // T0 is written only by the activation, only while unset, and only here —
    // after the purchase and the solvency check have both succeeded. Any
    // earlier failure aborts the whole transaction, T0 included.
    let previous_t0 = config.launch_t0;
    config.launch_t0 = t0_after_success(gate, previous_t0, now);
    if gate == PurchaseGate::Activation && previous_t0 != config.launch_t0 {
        emit!(LaunchActivated {
            founder: owner_key,
            launch_t0: config.launch_t0,
            rigs: count,
            total_lamports: quote.total_lamports,
        });
    }
    if is_new && referrer.is_some() {
        emit!(ReferralBound {
            miner: owner_key,
            referrer: miner.referrer,
            weight: miner.weight,
        });
    }

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
