//! Fruit ledger account layouts (architecture §10a) and the bridge to the
//! pure [`rigs_engine::fruit`] state machine.
//!
//! Like the base accounts these are dumb storage: every accounting decision
//! goes through the engine — read state out, run the transition, write it
//! back. Every account carries `state_version`; handlers refuse any other
//! version.

use anchor_lang::prelude::*;
use rigs_engine::{FruitLedger, FruitPool, FruitRoot, PoolStatus, RootStatus};

use crate::constants::MAX_VETO_SIGNERS;
use crate::error::{IntoAnchor, RigsError};

/// Singleton FRUIT CONFIG PDA — seeds `["fruit_config"]`.
///
/// Governance keys, the veto set, its threshold and the challenge window are
/// initialization inputs; none is chosen by the program. The frozen Fruit
/// constants are written from the engine at creation and have no setter.
#[account]
#[derive(InitSpace, Debug)]
pub struct FruitConfig {
    pub state_version: u16,
    /// Commits pools, withdraws reserve, pauses ledgers. Cannot post roots
    /// and cannot be the poster.
    pub admin: Pubkey,
    /// The only key that may post roots. Never a veto signer.
    pub root_poster: Pubkey,
    /// Independent veto signers; only the first `veto_signer_count` are set.
    pub veto_signers: [Pubkey; MAX_VETO_SIGNERS],
    pub veto_signer_count: u8,
    /// Distinct veto signatures needed to veto a root (also the terminal
    /// resolution quorum).
    pub veto_threshold: u8,
    /// Seconds between posting a root and its earliest finalization.
    pub challenge_window_secs: i64,
    /// Frozen `P_ref` (lamports per seed).
    pub p_ref_lamports: u64,
    /// Frozen benchmark `M`.
    pub m_seeds: u64,
    /// Frozen `qmax = qmax_num / qmax_den`.
    pub qmax_num: u8,
    pub qmax_den: u8,
    /// Per-ledger pause (index = ledger kind). Stops new pools, roots,
    /// finalization and close; never claims, never reduces `L`.
    pub paused: [bool; 3],
    /// Roots currently Pending across all pools. Informational.
    pub pending_roots: u32,
    /// The only account reserve withdrawals may be paid to.
    pub reserve_recipient: Pubkey,
    pub bump: u8,
}

impl FruitConfig {
    pub fn veto_set(&self) -> &[Pubkey] {
        &self.veto_signers[..self.veto_signer_count as usize]
    }

    /// Index of `key` in the veto set.
    pub fn veto_index(&self, key: &Pubkey) -> Option<usize> {
        self.veto_set().iter().position(|k| k == key)
    }

    pub fn is_paused(&self, kind: u8) -> bool {
        self.paused.get(kind as usize).copied().unwrap_or(true)
    }
}

/// FRUIT LEDGER PDA — seeds `["fruit_ledger", kind]`.
#[account]
#[derive(InitSpace, Debug)]
pub struct Ledger {
    pub state_version: u16,
    /// [`rigs_engine::LedgerKind`] discriminant.
    pub kind: u8,
    /// This ledger's FRUIT VAULT PDA.
    pub vault: Pubkey,
    /// `L`
    pub liabilities: u64,
    /// `U`
    pub committed: u64,
    /// `Z`
    pub reserve: u64,
    pub paid_cum: u64,
    /// Lamports ever deposited / withdrawn from reserve (audit trail).
    pub funded_cum: u64,
    pub withdrawn_cum: u64,
    pub vault_bump: u8,
    pub bump: u8,
}

impl Ledger {
    pub fn accounting(&self) -> FruitLedger {
        FruitLedger {
            liabilities: self.liabilities,
            committed: self.committed,
            reserve: self.reserve,
            paid_cum: self.paid_cum,
        }
    }

    pub fn write_accounting(&mut self, l: &FruitLedger) {
        self.liabilities = l.liabilities;
        self.committed = l.committed;
        self.reserve = l.reserve;
        self.paid_cum = l.paid_cum;
    }
}

/// FRUIT VAULT PDA — seeds `["fruit_vault", kind]`. Holds a Fruit ledger's
/// lamports. A distinct account type from the base [`crate::state::Vault`],
/// so neither can be passed as the other.
#[account]
#[derive(InitSpace, Debug)]
pub struct FruitVault {
    pub kind: u8,
    pub bump: u8,
}

/// FRUIT POOL PDA — seeds `["fruit_pool", kind, pool_id_le]`.
#[account]
#[derive(InitSpace, Debug)]
pub struct Pool {
    pub state_version: u16,
    pub ledger_kind: u8,
    pub pool_id: u64,
    /// `E`
    pub budget: u64,
    pub t_start: i64,
    pub t_end: i64,
    /// `floor(E/5)`
    pub wallet_cap: u64,
    /// Hash of the committed per-asset reference prices.
    pub reference_hash: [u8; 32],
    pub final_total: u64,
    pub paid_cum: u64,
    /// [`PoolStatus`] discriminant.
    pub status: u8,
    /// Frozen lock denominator. Reserved: written only by the future lock
    /// implementation; always 0 in this version.
    pub h_lock: u64,
    pub latest_version: u64,
    pub pending_version: u64,
    pub pending_complete: bool,
    pub final_version: u64,
    pub final_complete: bool,
    /// Merkle root of the latest Final root; claims verify against it only.
    pub final_root: [u8; 32],
    pub committed_at: i64,
    pub bump: u8,
    /// `forfeited` of the Pending root (lock ledger only; else 0).
    pub pending_forfeited: u64,
    /// `forfeited` of the latest Final root; 0 unless that root is complete.
    pub final_forfeited: u64,
    /// Released unused lock-bonus allocation paid to the Protocol Treasury at
    /// close: `floor(0.80 final_forfeited)`. Never fee revenue.
    pub forfeit_treasury_released: u64,
}

impl Pool {
    pub fn accounting(&self) -> Result<FruitPool> {
        Ok(FruitPool {
            budget: self.budget,
            t_start: self.t_start,
            t_end: self.t_end,
            wallet_cap: self.wallet_cap,
            final_total: self.final_total,
            paid_cum: self.paid_cum,
            latest_version: self.latest_version,
            pending_version: self.pending_version,
            pending_complete: self.pending_complete,
            final_version: self.final_version,
            final_complete: self.final_complete,
            status: PoolStatus::from_u8(self.status).anchor()?,
            pending_forfeited: self.pending_forfeited,
            final_forfeited: self.final_forfeited,
            forfeit_treasury_released: self.forfeit_treasury_released,
        })
    }

    pub fn write_accounting(&mut self, p: &FruitPool) {
        self.budget = p.budget;
        self.t_start = p.t_start;
        self.t_end = p.t_end;
        self.wallet_cap = p.wallet_cap;
        self.final_total = p.final_total;
        self.paid_cum = p.paid_cum;
        self.latest_version = p.latest_version;
        self.pending_version = p.pending_version;
        self.pending_complete = p.pending_complete;
        self.final_version = p.final_version;
        self.final_complete = p.final_complete;
        self.status = p.status as u8;
        self.pending_forfeited = p.pending_forfeited;
        self.final_forfeited = p.final_forfeited;
        self.forfeit_treasury_released = p.forfeit_treasury_released;
    }
}

/// FRUIT ROOT PDA — seeds `["fruit_root", pool, version_le]`. One account
/// per posted version, never closed: the pool's full root history.
#[account]
#[derive(InitSpace, Debug)]
pub struct Root {
    pub state_version: u16,
    pub pool: Pubkey,
    pub version: u64,
    pub merkle_root: [u8; 32],
    pub checkpoint_time: i64,
    pub total: u64,
    pub complete: bool,
    pub posted_at: i64,
    pub final_after: i64,
    /// [`RootStatus`] discriminant.
    pub status: u8,
    /// Bit `i` set when veto signer `i` has vetoed.
    pub veto_bitmap: u8,
    pub veto_count: u8,
    /// Finalized by a veto-quorum terminal resolution rather than by the
    /// challenge window.
    pub terminal: bool,
    pub bump: u8,
    /// Voluntary lock-forfeit capacity carried by a complete lock root
    /// (architecture §8 / §7a item 8); always 0 on core and loyalty roots.
    pub forfeited: u64,
}

impl Root {
    pub fn accounting(&self) -> Result<FruitRoot> {
        Ok(FruitRoot {
            version: self.version,
            checkpoint_time: self.checkpoint_time,
            total: self.total,
            complete: self.complete,
            posted_at: self.posted_at,
            final_after: self.final_after,
            status: RootStatus::from_u8(self.status).anchor()?,
            veto_count: self.veto_count,
            forfeited: self.forfeited,
        })
    }

    pub fn write_accounting(&mut self, r: &FruitRoot) {
        self.version = r.version;
        self.checkpoint_time = r.checkpoint_time;
        self.total = r.total;
        self.complete = r.complete;
        self.posted_at = r.posted_at;
        self.final_after = r.final_after;
        self.status = r.status as u8;
        self.veto_count = r.veto_count;
        self.forfeited = r.forfeited;
    }
}

/// FRUIT CLAIM RECORD PDA — seeds `["fruit_claim", pool, wallet]`.
#[account]
#[derive(InitSpace, Debug)]
pub struct ClaimRecord {
    pub state_version: u16,
    pub pool: Pubkey,
    pub wallet: Pubkey,
    /// Highest cumulative entitlement ever paid against. Never decreases.
    pub claimed_cum: u64,
    pub bump: u8,
}

/// Fail closed on any Fruit account written by a different layout version.
pub fn require_version(v: u16) -> Result<()> {
    require!(
        v == crate::constants::FRUIT_STATE_VERSION,
        RigsError::FruitVersionMismatch
    );
    Ok(())
}
