//! # THE CHERRY TREE
//!
//! Product name: THE CHERRY TREE. The crate, program module and PDA seeds
//! keep their original technical names (`sol_rigs`, `rigs-engine`) so the
//! program ID, instruction discriminators and account discriminators are
//! unchanged by the rebrand.
//!
//! A Solana-native mining **simulation** with SOL-denominated rewards.
//!
//! SOL is not proof-of-work mined. Rigs do not participate in Solana
//! consensus and do not cryptographically produce new SOL. The mining
//! metaphor governs the game loop — rigs, hashpower, production, accrual,
//! compounding, claiming — and nothing else. Rig purchases buy game
//! production capacity and are non-redeemable; they are not a deposit.
//!
//! ## The core invariant
//!
//! ```text
//! BUYING RIGS != FUNDING EXISTING MINER REWARDS
//! ```
//!
//! Three separate SOL vaults make this structural rather than documentary:
//!
//! | Vault   | Receives                            | Pays out                        |
//! |---------|-------------------------------------|---------------------------------|
//! | CAPITAL | non-fee portion of rig purchases    | nothing — locked in this version|
//! | REWARD  | realized revenue via `fund_rewards` | user claims and compounds only  |
//! | FEE     | disclosed purchase fee              | authority withdrawal            |
//!
//! `buy_rigs` takes the reward vault as a **read-only** account, so there is
//! no code path by which a purchase lamport can reach a claim.
//!
//! All accounting lives in the dependency-free [`rigs_engine`] crate; the
//! handlers here validate accounts and move lamports.

use anchor_lang::prelude::*;

pub mod constants;
pub mod error;
pub mod events;
pub mod identity;
pub mod instructions;
pub mod skr_binding;
pub mod state;
pub mod vault;

use instructions::*;

declare_id!("C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt");

#[program]
pub mod sol_rigs {
    use super::*;

    /// Create the config PDA and the three vault PDAs, pinning the Genesis
    /// fresh-purchase cap for the gated first 24 hours.
    pub fn initialize(
        ctx: Context<Initialize>,
        rig_price_lamports: u64,
        purchase_fee_bps: u16,
        reward_funder: Pubkey,
        fee_recipient: Pubkey,
        founder: Pubkey,
        genesis_cap_rigs: u64,
    ) -> Result<()> {
        instructions::initialize::handle_initialize(
            ctx,
            rig_price_lamports,
            purchase_fee_bps,
            reward_funder,
            fee_recipient,
            founder,
            genesis_cap_rigs,
        )
    }

    /// Migrate a live V1 Config (321 bytes) in place to V2 (329 bytes),
    /// pinning the Genesis cap. Authority-only, one-shot, and validated
    /// against the raw legacy account before a single byte moves.
    pub fn migrate_v2(ctx: Context<MigrateV2>, genesis_cap_rigs: u64) -> Result<()> {
        instructions::migrate_v2::handle_migrate_v2(ctx, genesis_cap_rigs)
    }

    /// Fund the reward vault and (re)write the emission schedule.
    /// Authorized `reward_funder` only.
    pub fn fund_rewards(ctx: Context<FundRewards>, amount: u64, duration: u64) -> Result<()> {
        instructions::fund_rewards::handle_fund_rewards(ctx, amount, duration)
    }

    /// Buy `count` whole rigs. 1 rig = 1 hashpower.
    ///
    /// Remaining accounts: `[identity…, skr_binding?, referrer_miner?]`.
    /// Identity is the buyer's `.skr` / `.sns` proof — `[name]` for a direct
    /// domain, `[name, nft_record, token_account]` for a wrapped / tokenized
    /// one — or nothing at all. `skr_binding` is the writable
    /// `["skr_bind", name]` PDA, required for a `.skr` purchase during Day 1
    /// and ignored otherwise. `referrer_miner` comes last, iff `referrer` is
    /// `Some`.
    pub fn buy_rigs<'info>(
        ctx: Context<'_, '_, 'info, 'info, BuyRigs<'info>>,
        count: u64,
        referrer: Option<Pubkey>,
    ) -> Result<()> {
        instructions::buy_rigs::handle_buy_rigs(ctx, count, referrer)
    }

    /// Claim the settled ore bin as SOL.
    pub fn claim(ctx: Context<ClaimRewards>) -> Result<()> {
        instructions::claim::handle_claim(ctx)
    }

    /// Turn the ore bin into more rigs, preserving the remainder.
    pub fn compound(ctx: Context<CompoundRewards>) -> Result<()> {
        instructions::compound::handle_compound(ctx)
    }

    // --- authority ---

    pub fn set_paused(ctx: Context<AdminOnly>, paused: bool) -> Result<()> {
        instructions::admin::set_paused(ctx, paused)
    }

    pub fn set_pricing(
        ctx: Context<AdminOnly>,
        rig_price_lamports: u64,
        purchase_fee_bps: u16,
    ) -> Result<()> {
        instructions::admin::set_pricing(ctx, rig_price_lamports, purchase_fee_bps)
    }

    pub fn set_reward_funder(ctx: Context<AdminOnly>, reward_funder: Pubkey) -> Result<()> {
        instructions::admin::set_reward_funder(ctx, reward_funder)
    }

    /// Move where protocol fees are paid. Authority only.
    pub fn set_fee_recipient(ctx: Context<AdminOnly>, fee_recipient: Pubkey) -> Result<()> {
        instructions::admin::set_fee_recipient(ctx, fee_recipient)
    }

    pub fn transfer_authority(ctx: Context<AdminOnly>, new_authority: Pubkey) -> Result<()> {
        instructions::admin::transfer_authority(ctx, new_authority)
    }

    pub fn accept_authority(ctx: Context<AcceptAuthority>) -> Result<()> {
        instructions::admin::accept_authority(ctx)
    }

    /// Withdraw disclosed protocol fees. FEE VAULT ONLY, and only to
    /// `config.fee_recipient`.
    pub fn withdraw_fees(ctx: Context<WithdrawFees>, amount: u64) -> Result<()> {
        instructions::admin::withdraw_fees(ctx, amount)
    }
}
