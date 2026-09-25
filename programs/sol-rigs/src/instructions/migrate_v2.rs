//! V1 → V2 Config migration: 321 → 329 bytes, in place, one-shot.
//!
//! Anchor 0.31.2 deserializes typed accounts *before* the handler runs, and a
//! 321-byte V1 Config is too short for `Account<Config>` once V2 appends
//! `genesis_cap_rigs` — so the legacy account must enter as a raw
//! `UncheckedAccount` and be proven field by field here.
//!
//! Every check happens before a single byte or lamport moves. Any failure
//! aborts the whole transaction atomically: the account is left byte- and
//! lamport-identical. The only failure mode after the checks begin mutating
//! is the transaction failing, which rolls back to the same guarantee.
//!
//! The migration window is deliberately inert: between the V2 upgrade and a
//! successful `migrate_v2`, every Config-touching instruction fails
//! deserialization cleanly (nothing executes against half-migrated state),
//! and once migrated the 321-byte size check makes a second run impossible.

use anchor_lang::prelude::*;
use anchor_lang::{AccountSerialize, Discriminator};
use borsh::BorshDeserialize;
use rigs_engine::validate_genesis_cap;

use crate::constants::{CONFIG_SEED, STATE_VERSION};
use crate::error::{IntoAnchor, RigsError};
use crate::events::GenesisCapConfigured;
use crate::state::Config;
use crate::vault::deposit_from_wallet;

/// Exact V1 Config account size: 8-byte discriminator + 313 field bytes.
pub const V1_CONFIG_SIZE: usize = 321;
/// V2 Config account size: V1 plus the appended `genesis_cap_rigs` u64.
pub const V2_CONFIG_SIZE: usize = 329;

/// The frozen V1 Config payload, field for field in declaration order.
///
/// A deliberate mirror, never used for storage: even if the live `Config`
/// drifts in a future version, this layout stays exactly what V1 wrote, and
/// `payload.is_empty()` after deserializing proves no unrecognized trailing
/// bytes were smuggled in.
#[derive(BorshDeserialize, Debug)]
struct ConfigV1 {
    authority: Pubkey,
    pending_authority: Pubkey,
    reward_funder: Pubkey,
    fee_recipient: Pubkey,
    paused: bool,
    rig_price_lamports: u64,
    purchase_fee_bps: u16,
    total_rigs: u64,
    reward_rate: u64,
    reward_period_finish: i64,
    reward_last_update: i64,
    reward_per_rig_stored: u128,
    undistributed_dust: u64,
    total_reward_funded: u64,
    total_reward_claimed: u64,
    total_reward_compounded: u64,
    total_capital_received: u64,
    total_protocol_fees: u64,
    total_fees_withdrawn: u64,
    miner_count: u64,
    capital_vault_bump: u8,
    reward_vault_bump: u8,
    fee_vault_bump: u8,
    version: u16,
    bump: u8,
    founder: Pubkey,
    launch_t0: i64,
    total_shares: u64,
    total_claim_penalties: u64,
}

#[derive(Accounts)]
pub struct MigrateV2<'info> {
    /// The V1 authority pinned in the legacy Config. Pays the exact rent
    /// shortfall for the larger account.
    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: the raw legacy V1 Config PDA. It cannot be typed
    /// `Account<Config>` — Anchor would deserialize the 321-byte V1 layout
    /// against the 329-byte V2 struct and fail before the handler ran — so
    /// every property Anchor normally proves (PDA, owner, discriminator,
    /// size, layout, version, bump, signer) is verified explicitly in
    /// `handle_migrate_v2` before any mutation.
    #[account(mut)]
    pub config: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handle_migrate_v2(ctx: Context<MigrateV2>, genesis_cap_rigs: u64) -> Result<()> {
    // ---- check phase: everything, before anything ----
    let v1 = {
        let config_info = ctx.accounts.config.to_account_info();
        let data = config_info.try_borrow_data()?;

        let (canonical, canonical_bump) =
            Pubkey::find_program_address(&[CONFIG_SEED], ctx.program_id);
        require_keys_eq!(config_info.key(), canonical, RigsError::MigrationRejected);
        require_keys_eq!(
            *config_info.owner,
            *ctx.program_id,
            RigsError::MigrationRejected
        );
        // One-shot, both ways: an already-migrated 329-byte account fails
        // here exactly like any other wrong size.
        require_eq!(data.len(), V1_CONFIG_SIZE, RigsError::MigrationRejected);
        require!(
            data[..8] == Config::DISCRIMINATOR[..],
            RigsError::MigrationRejected
        );

        let mut payload: &[u8] = &data[8..];
        let v1 = ConfigV1::deserialize(&mut payload)
            .map_err(|_| error!(RigsError::MigrationRejected))?;
        // Every legacy byte must belong to a declared V1 field.
        require!(payload.is_empty(), RigsError::MigrationRejected);
        require_eq!(v1.version, 1, RigsError::MigrationRejected);
        require_eq!(v1.bump, canonical_bump, RigsError::MigrationRejected);
        require_keys_eq!(
            ctx.accounts.authority.key(),
            v1.authority,
            RigsError::MigrationRejected
        );
        v1
    };

    validate_genesis_cap(genesis_cap_rigs).anchor()?;

    // ---- mutation phase: rent → realloc → full V2 write → event ----
    let config_info = ctx.accounts.config.to_account_info();

    // Fund the rent shortfall BEFORE growing, so the account is never even
    // transiently under-exempt; exactly the shortfall, never more.
    let shortfall = Rent::get()?
        .minimum_balance(V2_CONFIG_SIZE)
        .saturating_sub(config_info.lamports());
    deposit_from_wallet(
        &ctx.accounts.system_program,
        &ctx.accounts.authority,
        &config_info,
        shortfall,
    )?;

    // Grow with zero-initialised extension bytes; the serialize below then
    // writes all 329 bytes deterministically.
    config_info.realloc(V2_CONFIG_SIZE, true)?;

    let v2 = Config {
        authority: v1.authority,
        pending_authority: v1.pending_authority,
        reward_funder: v1.reward_funder,
        fee_recipient: v1.fee_recipient,
        paused: v1.paused,

        rig_price_lamports: v1.rig_price_lamports,
        purchase_fee_bps: v1.purchase_fee_bps,

        total_rigs: v1.total_rigs,
        reward_rate: v1.reward_rate,
        reward_period_finish: v1.reward_period_finish,
        reward_last_update: v1.reward_last_update,
        reward_per_rig_stored: v1.reward_per_rig_stored,
        undistributed_dust: v1.undistributed_dust,

        total_reward_funded: v1.total_reward_funded,
        total_reward_claimed: v1.total_reward_claimed,
        total_reward_compounded: v1.total_reward_compounded,
        total_capital_received: v1.total_capital_received,
        total_protocol_fees: v1.total_protocol_fees,
        total_fees_withdrawn: v1.total_fees_withdrawn,
        miner_count: v1.miner_count,

        capital_vault_bump: v1.capital_vault_bump,
        reward_vault_bump: v1.reward_vault_bump,
        fee_vault_bump: v1.fee_vault_bump,
        version: STATE_VERSION,
        bump: v1.bump,

        founder: v1.founder,
        launch_t0: v1.launch_t0,
        total_shares: v1.total_shares,
        total_claim_penalties: v1.total_claim_penalties,

        genesis_cap_rigs,
    };
    {
        let mut data = config_info.try_borrow_mut_data()?;
        // Same struct name, same discriminator: bytes 0..321 come back
        // identical except the version field (1 → 2), and 321..329 is the cap.
        v2.try_serialize(&mut &mut data[..])?;
    }

    emit!(GenesisCapConfigured { genesis_cap_rigs });
    Ok(())
}
