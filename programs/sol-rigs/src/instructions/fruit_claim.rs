//! Fruit claim: cumulative, replay-protected, against the latest Final root.

use anchor_lang::prelude::*;

use crate::constants::*;
use crate::error::{IntoAnchor, RigsError};
use crate::events::FruitClaimed;
use crate::fruit_merkle;
use crate::fruit_state::{require_version, ClaimRecord, FruitVault, Ledger, Pool};
use crate::instructions::fruit_ledger::check_solvent;
use crate::vault::move_lamports;

const LOCK_LEDGER_KIND: u8 = rigs_engine::LedgerKind::Lock as u8;

#[derive(Accounts)]
pub struct ClaimFruit<'info> {
    /// The claimant: signs, pays the claim record's rent, and is the only
    /// account the payment can go to. Its key is bound into the leaf.
    #[account(mut)]
    pub wallet: Signer<'info>,

    #[account(
        mut,
        seeds = [FRUIT_POOL_SEED, &[pool.ledger_kind], &pool.pool_id.to_le_bytes()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        mut,
        seeds = [FRUIT_LEDGER_SEED, &[pool.ledger_kind]],
        bump = ledger.bump,
        has_one = vault @ RigsError::FruitAccountMismatch,
    )]
    pub ledger: Account<'info, Ledger>,

    #[account(mut, seeds = [FRUIT_VAULT_SEED, &[pool.ledger_kind]], bump = ledger.vault_bump)]
    pub vault: Account<'info, FruitVault>,

    #[account(
        init_if_needed,
        payer = wallet,
        space = 8 + ClaimRecord::INIT_SPACE,
        seeds = [FRUIT_CLAIM_SEED, pool.key().as_ref(), wallet.key().as_ref()],
        bump,
    )]
    pub claim_record: Account<'info, ClaimRecord>,

    pub system_program: Program<'info, System>,
}

/// Pay `max(0, cumulative_amount - claimed_cum)` against the pool's latest
/// Final root; then `claimed_cum = max(claimed_cum, cumulative_amount)`,
/// `V -= payment`, `L -= payment`, `paid_cum += payment`, atomically.
///
/// Not gated by pause: pausing never blocks an established liability.
pub fn claim_fruit(
    ctx: Context<ClaimFruit>,
    cumulative_amount: u64,
    proof: Vec<[u8; 32]>,
) -> Result<()> {
    require_version(ctx.accounts.pool.state_version)?;
    require_version(ctx.accounts.ledger.state_version)?;
    require!(
        proof.len() <= MAX_FRUIT_PROOF_LEN,
        RigsError::FruitProofTooLong
    );

    let wallet = ctx.accounts.wallet.key();
    let pool_key = ctx.accounts.pool.key();

    let record = &mut ctx.accounts.claim_record;
    if record.state_version == 0 {
        record.state_version = FRUIT_STATE_VERSION;
        record.pool = pool_key;
        record.wallet = wallet;
        record.claimed_cum = 0;
        record.bump = ctx.bumps.claim_record;
    }
    require_version(record.state_version)?;
    require!(
        record.pool == pool_key && record.wallet == wallet,
        RigsError::FruitAccountMismatch
    );

    let pool = &ctx.accounts.pool;
    // Lock claims are also bounded by the wallet's award: `claim_lock` only.
    require!(
        pool.ledger_kind != LOCK_LEDGER_KIND,
        RigsError::InvalidLedgerKind
    );
    require!(pool.final_version != 0, RigsError::NoFinalRoot);
    let leaf = fruit_merkle::leaf(
        &crate::ID,
        pool.ledger_kind,
        pool.pool_id,
        &wallet,
        cumulative_amount,
    );
    require!(
        fruit_merkle::verify(&proof, &pool.final_root, leaf),
        RigsError::InvalidMerkleProof
    );

    let mut acct = ctx.accounts.ledger.accounting();
    let mut pool_state = pool.accounting()?;
    let mut claimed = record.claimed_cum;
    let payment =
        rigs_engine::fruit::claim(&mut acct, &mut pool_state, &mut claimed, cumulative_amount)
            .anchor()?;

    let vault_ai = ctx.accounts.vault.to_account_info();
    move_lamports(&vault_ai, &ctx.accounts.wallet.to_account_info(), payment)?;
    check_solvent(&vault_ai, &acct)?;

    ctx.accounts.ledger.write_accounting(&acct);
    ctx.accounts.pool.write_accounting(&pool_state);
    ctx.accounts.claim_record.claimed_cum = claimed;

    emit!(FruitClaimed {
        pool: pool_key,
        wallet,
        cumulative_amount,
        payment,
        claimed_cum: claimed,
    });
    Ok(())
}
