//! Legacy Config migrations, in place, one-shot:
//!
//! * `migrate_v2`: V1 (321 bytes) → current (375), pinning the Genesis cap.
//! * `migrate_v3`: V2 (329 bytes) → current (375).
//!
//! The current layout (V3) appends the protocol-fee fields of architecture
//! §7a. Migration never invents a fee: `compound_fee_bps` is set equal to
//! the legacy `purchase_fee_bps` (exactly what compound charged before), no
//! fee change is pending, and every commission / treasury counter starts at
//! zero — there are no retroactive commissions on fees already received.
//!
//! Anchor 0.31.2 deserializes typed accounts *before* the handler runs, and a
//! legacy Config is too short for `Account<Config>` — so the legacy account
//! must enter as a raw `UncheckedAccount` and be proven field by field here.
//!
//! Every check happens before a single byte or lamport moves. Any failure
//! aborts the whole transaction atomically: the account is left byte- and
//! lamport-identical. The only failure mode after the checks begin mutating
//! is the transaction failing, which rolls back to the same guarantee.
//!
//! The migration window is deliberately inert: between the upgrade and a
//! successful migration, every Config-touching instruction fails
//! deserialization cleanly (nothing executes against half-migrated state),
//! and once migrated the exact legacy-size check makes a second run
//! impossible.

use anchor_lang::prelude::*;
use anchor_lang::{AccountSerialize, Discriminator};
use borsh::BorshDeserialize;
use rigs_engine::{validate_fee_pair, validate_genesis_cap, validate_pricing};

use crate::constants::{CONFIG_SEED, STATE_VERSION};
use crate::error::{IntoAnchor, RigsError};
use crate::events::{CompoundFeeConfigured, GenesisCapConfigured};
use crate::state::Config;
use crate::vault::deposit_from_wallet;

/// Exact V1 Config account size: 8-byte discriminator + 313 field bytes.
pub const V1_CONFIG_SIZE: usize = 321;
/// V2 Config account size: V1 plus the appended `genesis_cap_rigs` u64.
pub const V2_CONFIG_SIZE: usize = 329;
/// V3 (current) Config account size: V2 plus the §7a protocol-fee fields
/// (3 × u16, 1 × i64, 4 × u64 = 46 bytes).
pub const V3_CONFIG_SIZE: usize = 375;
const _: () = assert!(8 + <Config as anchor_lang::Space>::INIT_SPACE == V3_CONFIG_SIZE);

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

/// The frozen V2 Config payload: V1 plus `genesis_cap_rigs`.
#[derive(BorshDeserialize, Debug)]
struct ConfigV2 {
    v1: ConfigV1,
    genesis_cap_rigs: u64,
}

#[derive(Accounts)]
pub struct MigrateV2<'info> {
    /// The V1 authority pinned in the legacy Config. Pays the exact rent
    /// shortfall for the larger account.
    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: the raw legacy V1 Config PDA. It cannot be typed
    /// `Account<Config>` — Anchor would deserialize the 321-byte V1 layout
    /// against the 375-byte current struct and fail before the handler ran — so
    /// every property Anchor normally proves (PDA, owner, discriminator,
    /// size, layout, version, bump, signer) is verified explicitly in
    /// `handle_migrate_v2` before any mutation.
    #[account(mut)]
    pub config: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

/// Check phase shared by both migrations: canonical PDA, program-owned,
/// exact legacy size, Config discriminator, full layout with no trailing
/// bytes, legacy version, canonical bump, and the legacy authority as signer.
/// Nothing is mutated.
fn read_legacy<T: BorshDeserialize>(
    config: &AccountInfo,
    authority: &Pubkey,
    program_id: &Pubkey,
    size: usize,
    authority_of: impl Fn(&T) -> (Pubkey, u16, u8),
    version: u16,
) -> Result<T> {
    let data = config.try_borrow_data()?;
    let (canonical, canonical_bump) = Pubkey::find_program_address(&[CONFIG_SEED], program_id);
    require_keys_eq!(config.key(), canonical, RigsError::MigrationRejected);
    require_keys_eq!(*config.owner, *program_id, RigsError::MigrationRejected);
    // One-shot, both ways: an already-migrated account fails here exactly
    // like any other wrong size.
    require_eq!(data.len(), size, RigsError::MigrationRejected);
    require!(
        data[..8] == Config::DISCRIMINATOR[..],
        RigsError::MigrationRejected
    );
    let mut payload: &[u8] = &data[8..];
    let legacy = T::deserialize(&mut payload).map_err(|_| error!(RigsError::MigrationRejected))?;
    // Every legacy byte must belong to a declared legacy field.
    require!(payload.is_empty(), RigsError::MigrationRejected);
    let (legacy_authority, legacy_version, legacy_bump) = authority_of(&legacy);
    require_eq!(legacy_version, version, RigsError::MigrationRejected);
    require_eq!(legacy_bump, canonical_bump, RigsError::MigrationRejected);
    require_keys_eq!(*authority, legacy_authority, RigsError::MigrationRejected);
    Ok(legacy)
}

/// Semantic check shared by both migrations, before any mutation: the
/// legacy pricing must satisfy every invariant the current Config relies on
/// — a nonzero valid rig price, `purchase_fee_bps <= MAX_PURCHASE_FEE_BPS`,
/// and the migrated pair (`compound_fee_bps = purchase_fee_bps`) passing
/// `compound <= purchase`. The Genesis cap is validated by each handler.
/// A malformed legacy account is refused, never carried forward.
fn validate_legacy_pricing(v1: &ConfigV1) -> Result<()> {
    validate_pricing(v1.rig_price_lamports, v1.purchase_fee_bps)
        .map_err(|_| error!(RigsError::MigrationRejected))?;
    validate_fee_pair(v1.purchase_fee_bps, v1.purchase_fee_bps)
        .map_err(|_| error!(RigsError::MigrationRejected))?;
    Ok(())
}

/// Mutation phase shared by both migrations: rent shortfall → realloc →
/// full current-layout write. Only called after every check has passed.
fn write_current<'info>(
    config_info: &AccountInfo<'info>,
    authority: &Signer<'info>,
    system_program: &Program<'info, System>,
    current: &Config,
) -> Result<()> {
    // Fund the rent shortfall BEFORE growing, so the account is never even
    // transiently under-exempt; exactly the shortfall, never more.
    let shortfall = Rent::get()?
        .minimum_balance(V3_CONFIG_SIZE)
        .saturating_sub(config_info.lamports());
    deposit_from_wallet(system_program, authority, config_info, shortfall)?;
    // Grow with zero-initialised extension bytes; the serialize below then
    // writes every byte deterministically.
    config_info.realloc(V3_CONFIG_SIZE, true)?;
    let mut data = config_info.try_borrow_mut_data()?;
    // Same struct name, same discriminator: the legacy prefix comes back
    // identical except the version field.
    current.try_serialize(&mut &mut data[..])?;
    Ok(())
}

/// The current Config for a legacy V1 payload plus the V2 cap. Fee fields
/// per the module docs: compound fee = legacy purchase fee, nothing pending,
/// every commission / treasury counter zero.
fn upgrade(v1: ConfigV1, genesis_cap_rigs: u64) -> Config {
    Config {
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

        compound_fee_bps: v1.purchase_fee_bps,
        pending_purchase_fee_bps: 0,
        pending_compound_fee_bps: 0,
        fee_change_effective_at: 0,
        reserved_commissions: 0,
        total_commissions_reserved: 0,
        total_commissions_paid: 0,
        total_reward_to_fees: 0,
    }
}

pub fn handle_migrate_v2(ctx: Context<MigrateV2>, genesis_cap_rigs: u64) -> Result<()> {
    // ---- check phase: everything, before anything ----
    let v1: ConfigV1 = read_legacy(
        &ctx.accounts.config.to_account_info(),
        &ctx.accounts.authority.key(),
        ctx.program_id,
        V1_CONFIG_SIZE,
        |c: &ConfigV1| (c.authority, c.version, c.bump),
        1,
    )?;
    validate_legacy_pricing(&v1)?;
    validate_genesis_cap(genesis_cap_rigs).anchor()?;
    let compound_fee_bps = v1.purchase_fee_bps;
    let current = upgrade(v1, genesis_cap_rigs);
    debug_assert_eq!(current.compound_fee_bps, current.purchase_fee_bps);

    // ---- mutation phase ----
    write_current(
        &ctx.accounts.config.to_account_info(),
        &ctx.accounts.authority,
        &ctx.accounts.system_program,
        &current,
    )?;

    emit!(GenesisCapConfigured { genesis_cap_rigs });
    emit!(CompoundFeeConfigured { compound_fee_bps });
    Ok(())
}

#[derive(Accounts)]
pub struct MigrateV3<'info> {
    /// The V2 authority pinned in the legacy Config. Pays the exact rent
    /// shortfall for the larger account.
    #[account(mut)]
    pub authority: Signer<'info>,

    /// CHECK: the raw legacy V2 Config PDA (329 bytes), verified field by
    /// field in `handle_migrate_v3` before any mutation, exactly like
    /// `migrate_v2`.
    #[account(mut)]
    pub config: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

/// V2 → V3: append the §7a fee fields. The pinned Genesis cap is carried
/// over unchanged (and re-validated: a V2 account with any other cap fails).
pub fn handle_migrate_v3(ctx: Context<MigrateV3>) -> Result<()> {
    let v2: ConfigV2 = read_legacy(
        &ctx.accounts.config.to_account_info(),
        &ctx.accounts.authority.key(),
        ctx.program_id,
        V2_CONFIG_SIZE,
        |c: &ConfigV2| (c.v1.authority, c.v1.version, c.v1.bump),
        2,
    )?;
    validate_legacy_pricing(&v2.v1)?;
    validate_genesis_cap(v2.genesis_cap_rigs).anchor()?;
    let compound_fee_bps = v2.v1.purchase_fee_bps;
    let current = upgrade(v2.v1, v2.genesis_cap_rigs);
    debug_assert_eq!(current.compound_fee_bps, current.purchase_fee_bps);

    write_current(
        &ctx.accounts.config.to_account_info(),
        &ctx.accounts.authority,
        &ctx.accounts.system_program,
        &current,
    )?;

    emit!(CompoundFeeConfigured { compound_fee_bps });
    Ok(())
}
