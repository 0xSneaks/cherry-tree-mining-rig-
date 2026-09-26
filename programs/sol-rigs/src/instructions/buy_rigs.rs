use anchor_lang::prelude::*;
use rigs_engine::{
    access_phase, assert_fully_funded, gate_purchase, genesis_cap_check, referral_weight,
    skr_day1_binding, t0_after_success, AccessPhase, ProvenIdentity, PurchaseContext, PurchaseGate,
    ReferrerCheck, SkrBindAction,
};

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::{LaunchActivated, ReferralBound, RigsPurchased, SkrDomainBound};
use crate::identity;
use crate::skr_binding;
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
/// * `launch_t0 == 0`: only the ACTIVATION is possible — a purchase signed by
///   the founder wallet pinned in config, with an active funded schedule. On success `launch_t0 := Clock` in this same
///   instruction, so a failed purchase can never leave T0 behind.
/// * afterwards: the access phase (from T0 and the clock only) decides which
///   proven identity may buy.
/// * Day 1 (`[T0, T0 + 24h)`): a `.skr` purchase must carry the domain's
///   binding PDA. One domain opens Day-1 access for one wallet — see
///   `rigs_engine::skr_day1_binding`.
///
/// Every purchase also obeys the permanent 250-seed holding cap, enforced in
/// `rigs_engine::buy_rigs`.
pub fn handle_buy_rigs<'info>(
    ctx: Context<'_, '_, 'info, 'info, BuyRigs<'info>>,
    count: u64,
    referrer: Option<Pubkey>,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let owner_key = ctx.accounts.owner.key();
    // Remaining accounts: identity accounts first (1 for a direct domain, 3
    // for a wrapped / tokenized one), then the `.skr` binding PDA if given,
    // then the referrer's miner iff named.
    let remaining = ctx.remaining_accounts;
    let n_ref = usize::from(referrer.is_some());
    if remaining.len() < n_ref {
        return err!(RigsError::MissingReferrerAccount);
    }
    let (identity_and_binding, referrer_accounts) = remaining.split_at(remaining.len() - n_ref);
    let (identity_accounts, binding_account) = skr_binding::split(identity_and_binding);

    let mut identity = identity::prove(identity_accounts, &owner_key, now);

    // --- Day-1 .skr binding: one domain, one wallet ---
    // Only during [T0, T0 + 24h). Decided before the gate so a domain bound
    // to another wallet can never open access, even for its current owner.
    let mut create_binding: Option<Pubkey> = None;
    if access_phase(ctx.accounts.config.launch_t0, now) == AccessPhase::SkrGenesis {
        match (identity_accounts.first(), binding_account) {
            (Some(name), Some(binding)) => {
                let bound_to = skr_binding::read(binding, name.key, ctx.program_id)?;
                let action = skr_day1_binding(
                    bound_to.map(|w| w.to_bytes()),
                    owner_key.to_bytes(),
                    identity == ProvenIdentity::Skr,
                )
                .anchor()?;
                if action == SkrBindAction::Create {
                    create_binding = Some(*name.key);
                }
                // Bound to this wallet (now, or earlier and retained).
                identity = ProvenIdentity::Skr;
            }
            // A proven `.skr` without its binding cannot open Day 1: an
            // unbound use would let the domain be reused after a transfer.
            _ if identity == ProvenIdentity::Skr => {
                return err!(RigsError::SkrBindingRequired);
            }
            // No domain proof at all: the gate below denies it.
            _ => {}
        }
    }

    // --- launch gate ---
    let gate = {
        let config = &ctx.accounts.config;
        gate_purchase(&PurchaseContext {
            launch_t0: config.launch_t0,
            now,
            is_founder: owner_key == config.founder,
            schedule_active: config.reward_rate > 0 && now < config.reward_period_finish,
            identity,
        })
        .anchor()?
    };

    // --- Genesis cap ---
    // Cumulative fresh purchases per wallet, gated period only ([T0, T0+24h),
    // plus the activation itself). Checked before any state or money moves;
    // `compound` never passes through here, so compounding never consumes
    // allowance. A fresh Miner reads zero lifetime purchases.
    genesis_cap_check(
        ctx.accounts.config.launch_t0,
        now,
        ctx.accounts.miner.lifetime_rigs_bought,
        count,
        ctx.accounts.config.genesis_cap_rigs,
    )
    .anchor()?;

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
            let acct = referrer_accounts
                .first()
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

    // First Day-1 use of the domain: bind it to this wallet, paid by the
    // buyer, in the same atomic transaction as the purchase.
    if let (Some(name_key), Some(binding)) = (create_binding, binding_account) {
        skr_binding::create(
            binding,
            &ctx.accounts.owner.to_account_info(),
            &ctx.accounts.system_program.to_account_info(),
            &name_key,
            &owner_key,
            now,
            ctx.program_id,
        )?;
        emit!(SkrDomainBound {
            name_account: name_key,
            wallet: owner_key,
            bound_at: now,
        });
    }

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
