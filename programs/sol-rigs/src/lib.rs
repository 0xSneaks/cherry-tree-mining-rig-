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
//! | REWARD  | realized revenue via `fund_rewards` | user claims and compounds, and  |
//! |         |                                     | the early-claim fee's 60% → FEE |
//! | FEE     | purchase and compound fees, the     | referral commissions (reserved) |
//! |         | early-claim fee's treasury share    | and the unreserved remainder to |
//! |         |                                     | the Protocol Treasury           |
//!
//! `buy_rigs` takes the reward vault as a **read-only** account, so there is
//! no code path by which a purchase lamport can reach a claim.
//!
//! All accounting lives in the dependency-free [`rigs_engine`] crate; the
//! handlers here validate accounts and move lamports.
//!
//! ## Fruit ledgers (architecture §10 / §10a)
//!
//! Fruit core, loyalty and lock SOL live in three further program-owned
//! FRUIT VAULTs, one per ledger, each with its own `L`/`U`/`Z` counters and
//! `V >= L + U + Z` checked against the vault's real balance after every
//! Fruit instruction. No Fruit instruction touches a base vault, a miner
//! position or a seed count, and no base instruction touches a Fruit
//! account. Fruit lamports leave a Fruit vault only as a claim against `L`
//! (cumulative Merkle leaf under a Final root) or a reserve withdrawal
//! against `Z`.

use anchor_lang::prelude::*;

pub mod constants;
pub mod error;
pub mod events;
pub mod fruit_merkle;
pub mod fruit_state;
pub mod identity;
pub mod instructions;
pub mod referral;
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
        compound_fee_bps: u16,
    ) -> Result<()> {
        instructions::initialize::handle_initialize(
            ctx,
            rig_price_lamports,
            purchase_fee_bps,
            reward_funder,
            fee_recipient,
            founder,
            genesis_cap_rigs,
            compound_fee_bps,
        )
    }

    /// Migrate a live V1 Config (321 bytes) in place to the current layout
    /// (375 bytes), pinning the Genesis cap. Authority-only, one-shot, and
    /// validated against the raw legacy account before a single byte moves.
    pub fn migrate_v2(ctx: Context<MigrateV2>, genesis_cap_rigs: u64) -> Result<()> {
        instructions::migrate_v2::handle_migrate_v2(ctx, genesis_cap_rigs)
    }

    /// Migrate a live V2 Config (329 bytes) in place to the current layout
    /// (375 bytes): compound fee = legacy purchase fee, nothing pending,
    /// commission and treasury counters zero. Same guarantees as
    /// `migrate_v2`.
    pub fn migrate_v3(ctx: Context<MigrateV3>) -> Result<()> {
        instructions::migrate_v2::handle_migrate_v3(ctx)
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

    /// Turn the ore bin into more rigs at `compound_fee_bps`, preserving the
    /// remainder. Remaining accounts: `[referral_ledger]` iff referred.
    pub fn compound<'info>(
        ctx: Context<'_, '_, 'info, 'info, CompoundRewards<'info>>,
    ) -> Result<()> {
        instructions::compound::handle_compound(ctx)
    }

    /// A referrer claims its whole unpaid commission from the fee vault.
    pub fn claim_commission(ctx: Context<ClaimCommission>) -> Result<()> {
        instructions::admin::claim_commission(ctx)
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

    /// Schedule a fee change, effective 72 hours from now. Authority only.
    /// Replaces any pending change and restarts the delay.
    pub fn schedule_fee_change(
        ctx: Context<AdminOnly>,
        purchase_fee_bps: u16,
        compound_fee_bps: u16,
    ) -> Result<()> {
        instructions::admin::schedule_fee_change(ctx, purchase_fee_bps, compound_fee_bps)
    }

    /// Apply the pending fee change once its timelock has elapsed.
    /// Permissionless.
    pub fn apply_fee_change(ctx: Context<ApplyFeeChange>) -> Result<()> {
        instructions::admin::apply_fee_change(ctx)
    }

    /// Cancel the pending fee change. Authority only.
    pub fn cancel_fee_change(ctx: Context<AdminOnly>) -> Result<()> {
        instructions::admin::cancel_fee_change(ctx)
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

    /// Withdraw disclosed protocol fees. FEE VAULT ONLY, only its unreserved
    /// balance, and only to `config.fee_recipient`.
    pub fn withdraw_fees(ctx: Context<WithdrawFees>, amount: u64) -> Result<()> {
        instructions::admin::withdraw_fees(ctx, amount)
    }

    // --- Fruit ledger (§10 / §10a) ---

    /// Create the Fruit config. Base authority only; it becomes the Fruit
    /// admin. Governance values are inputs, validated, never defaulted.
    pub fn init_fruit_config(
        ctx: Context<InitFruitConfig>,
        root_poster: Pubkey,
        veto_signers: Vec<Pubkey>,
        veto_threshold: u8,
        challenge_window_secs: i64,
        reserve_recipient: Pubkey,
    ) -> Result<()> {
        instructions::fruit_ledger::init_fruit_config(
            ctx,
            root_poster,
            veto_signers,
            veto_threshold,
            challenge_window_secs,
            reserve_recipient,
        )
    }

    /// Create one Fruit ledger and its own vault. Fruit admin only.
    pub fn init_fruit_ledger(ctx: Context<InitFruitLedger>, kind: u8) -> Result<()> {
        instructions::fruit_ledger::init_fruit_ledger(ctx, kind)
    }

    /// FUND: `V += a; Z += a`.
    pub fn fund_fruit(ctx: Context<FundFruit>, kind: u8, amount: u64) -> Result<()> {
        instructions::fruit_ledger::fund_fruit(ctx, kind, amount)
    }

    /// COMMIT POOL: `Z -= E; U += E`, before `t_start`, once per pool id.
    pub fn commit_fruit_pool(
        ctx: Context<CommitFruitPool>,
        kind: u8,
        pool_id: u64,
        budget: u64,
        t_start: i64,
        t_end: i64,
        reference_hash: [u8; 32],
    ) -> Result<()> {
        instructions::fruit_ledger::commit_fruit_pool(
            ctx,
            kind,
            pool_id,
            budget,
            t_start,
            t_end,
            reference_hash,
        )
    }

    /// Post a Pending cumulative root. Root poster only.
    pub fn post_fruit_root(
        ctx: Context<PostFruitRoot>,
        version: u64,
        merkle_root: [u8; 32],
        checkpoint_time: i64,
        total: u64,
        complete: bool,
        forfeited: u64,
    ) -> Result<()> {
        instructions::fruit_roots::post_fruit_root(
            ctx,
            version,
            merkle_root,
            checkpoint_time,
            total,
            complete,
            forfeited,
        )
    }

    /// One veto signature against a Pending root, inside its window.
    pub fn veto_fruit_root(ctx: Context<VetoFruitRoot>) -> Result<()> {
        instructions::fruit_roots::veto_fruit_root(ctx)
    }

    /// Finalize an unvetoed root after its window: `U -= Δ; L += Δ`.
    pub fn finalize_fruit_root(ctx: Context<FinalizeFruitRoot>) -> Result<()> {
        instructions::fruit_roots::finalize_fruit_root(ctx)
    }

    /// Veto-quorum terminal settlement after `t_end`. Never reduces `L`.
    /// Remaining accounts: the signing veto signers.
    pub fn resolve_fruit_pool_terminal<'info>(
        ctx: Context<'_, '_, 'info, 'info, ResolveFruitPoolTerminal<'info>>,
        version: u64,
        merkle_root: [u8; 32],
        checkpoint_time: i64,
        total: u64,
    ) -> Result<()> {
        instructions::fruit_roots::resolve_fruit_pool_terminal(
            ctx,
            version,
            merkle_root,
            checkpoint_time,
            total,
        )
    }

    /// Cumulative claim against the pool's latest Final root.
    pub fn claim_fruit(
        ctx: Context<ClaimFruit>,
        cumulative_amount: u64,
        proof: Vec<[u8; 32]>,
    ) -> Result<()> {
        instructions::fruit_claim::claim_fruit(ctx, cumulative_amount, proof)
    }

    /// Reconciled close: `U -= E - final_total; Z += E - final_total`; a lock
    /// pool then pays `floor(0.80 forfeited)` from `Z` to the treasury. No
    /// forfeiture can be carried in this version (it needs `sum A_i` from lock
    /// registration), so that release is 0 until the lock engine exists.
    pub fn close_fruit_pool(ctx: Context<CloseFruitPool>) -> Result<()> {
        instructions::fruit_roots::close_fruit_pool(ctx)
    }

    /// RESERVE WITHDRAWAL: `V -= z; Z -= z`, to the reserve recipient only.
    pub fn withdraw_fruit_reserve(
        ctx: Context<WithdrawFruitReserve>,
        kind: u8,
        amount: u64,
    ) -> Result<()> {
        instructions::fruit_ledger::withdraw_fruit_reserve(ctx, kind, amount)
    }

    // --- Lock Season (§8; Pass #7 P7-B) ---

    /// COMMIT the Season's lock pool (window derived from `launch_t0`) and
    /// its `LockSeason`. Fruit admin only; before `t_s`.
    pub fn commit_lock_pool(
        ctx: Context<CommitLockPool>,
        pool_id: u64,
        budget: u64,
        reference_hash: [u8; 32],
    ) -> Result<()> {
        instructions::fruit_lock::commit_lock_pool(ctx, pool_id, budget, reference_hash)
    }

    /// Register lock intent inside `[FRUIT_T0, t_s)`. One per wallet.
    pub fn register_lock(ctx: Context<RegisterLock>, requested: u64) -> Result<()> {
        instructions::fruit_lock::register_lock(ctx, requested)
    }

    /// Cancel a registration inside `[FRUIT_T0, t_s)`, once. No re-registration.
    pub fn cancel_lock(ctx: Context<CancelLock>) -> Result<()> {
        instructions::fruit_lock::cancel_lock_registration(ctx)
    }

    /// Post `ell_i` for a batch of registrations (remaining accounts). Root
    /// poster only; at or after `t_s`.
    pub fn post_lock_weights<'info>(
        ctx: Context<'_, '_, 'info, 'info, PostLockWeights<'info>>,
        weights: Vec<u64>,
    ) -> Result<()> {
        instructions::fruit_lock::post_lock_weights(ctx, weights)
    }

    /// Seal the weights once every active registration is weighed.
    pub fn seal_lock_weights(ctx: Context<LockTallyStep>) -> Result<()> {
        instructions::fruit_lock::seal_lock_weights(ctx)
    }

    /// Compute and add awards for a batch of registrations. Permissionless.
    pub fn award_lock_registrations<'info>(
        ctx: Context<'_, '_, 'info, 'info, LockTallyStep<'info>>,
    ) -> Result<()> {
        instructions::fruit_lock::award_lock_registrations(ctx)
    }

    /// Seal the awards; opens the allocation's challenge window.
    pub fn seal_lock_awards(ctx: Context<LockTallyStep>) -> Result<()> {
        instructions::fruit_lock::seal_lock_awards(ctx)
    }

    /// One veto signature against the Pending lock allocation.
    pub fn veto_lock_allocation(ctx: Context<VetoLockAllocation>) -> Result<()> {
        instructions::fruit_lock::veto_lock_allocation(ctx)
    }

    /// Finalize the lock allocation after its window. Permissionless.
    pub fn finalize_lock_allocation(ctx: Context<LockTallyStep>) -> Result<()> {
        instructions::fruit_lock::finalize_lock_allocation(ctx)
    }

    /// Post a lock root against the Final allocation (cliff capacity).
    pub fn post_lock_root(
        ctx: Context<PostLockRoot>,
        version: u64,
        merkle_root: [u8; 32],
        checkpoint_time: i64,
        total: u64,
        complete: bool,
        forfeited: u64,
    ) -> Result<()> {
        instructions::fruit_lock::post_lock_root(
            ctx,
            version,
            merkle_root,
            checkpoint_time,
            total,
            complete,
            forfeited,
        )
    }

    /// Finalize a lock root after its window. Permissionless.
    pub fn finalize_lock_root(ctx: Context<FinalizeLockRoot>) -> Result<()> {
        instructions::fruit_lock::finalize_lock_root(ctx)
    }

    /// Veto-quorum terminal settlement of a lock pool after `t_end`.
    pub fn resolve_lock_pool_terminal<'info>(
        ctx: Context<'_, '_, 'info, 'info, ResolveLockPoolTerminal<'info>>,
        version: u64,
        merkle_root: [u8; 32],
        checkpoint_time: i64,
        total: u64,
    ) -> Result<()> {
        instructions::fruit_lock::resolve_lock_pool_terminal(
            ctx,
            version,
            merkle_root,
            checkpoint_time,
            total,
        )
    }

    /// Cumulative lock claim, bounded by the wallet's own award.
    pub fn claim_lock(
        ctx: Context<ClaimLock>,
        cumulative_amount: u64,
        proof: Vec<[u8; 32]>,
    ) -> Result<()> {
        instructions::fruit_lock::claim_lock(ctx, cumulative_amount, proof)
    }

    /// Reconciled lock-pool close; 80% of forfeiture to the treasury.
    pub fn close_lock_pool(ctx: Context<CloseLockPool>) -> Result<()> {
        instructions::fruit_lock::close_lock_pool(ctx)
    }

    /// Pause or resume one Fruit ledger (pools, roots, finalize, close).
    pub fn set_fruit_paused(ctx: Context<FruitAdminOnly>, kind: u8, paused: bool) -> Result<()> {
        instructions::fruit_ledger::set_fruit_paused(ctx, kind, paused)
    }
}
