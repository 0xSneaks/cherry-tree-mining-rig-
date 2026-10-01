//! Fruit ledger accounting — architecture §10 / §10a.
//!
//! Pure integer state machine for the three independently funded Fruit
//! ledgers (core, loyalty, lock). Allocation itself is computed off-chain by
//! replay and posted as cumulative Merkle roots; this module is the part the
//! program enforces: every budget, the challenge window, monotone finality,
//! cumulative replay-safe claims and reserve-until-reconciliation.
//!
//! Per ledger `k`, with `V` the vault's spendable lamports (balance minus its
//! rent-exempt minimum):
//!
//! ```text
//! V_k >= L_k + U_k + Z_k
//! L = established accrued/vested unpaid liabilities
//! U = committed, not yet established capacity (reserved until close)
//! Z = uncommitted available reserve
//! ```
//!
//! Every transition runs on a copy of the state and is written back only on
//! success, so a failed transition changes nothing. No transition here can
//! reduce `L` except [`FruitLedger::pay`], and no transition returns lamports
//! to anything but a claimant (against `L`) or a reserve withdrawal (against
//! `Z`). Nothing in this module can mint a seed or reach a base vault.

use crate::error::{EngineError, EngineResult};
use crate::fruit_lock::{lock_final_allocation, LockState};

/// Frozen Fruit seed-price reference, `P_ref` (lamports per seed). Never read
/// from the base program's configurable price.
pub const P_REF_LAMPORTS: u64 = 10_000_000;
/// Frozen Fruit benchmark `M` (seeds).
pub const FRUIT_M: u64 = 250;
/// `qmax = QMAX_NUM / QMAX_DEN = 1/5`: the per-wallet share of any one pool.
pub const QMAX_NUM: u64 = 1;
pub const QMAX_DEN: u64 = 5;

/// Protocol Treasury share of released lock-forfeit capacity: 80%
/// (architecture §7a item 8). The rest stays in the lock ledger's `Z`.
pub const LOCK_FORFEIT_TREASURY_BPS: u64 = 8_000;

/// Treasury release of `forfeited` at lock-pool close:
/// `floor(0.80 forfeited)`. `forfeited - release` stays in `Z`.
pub fn lock_forfeit_treasury_release(forfeited: u64) -> u64 {
    ((forfeited as u128) * LOCK_FORFEIT_TREASURY_BPS as u128 / 10_000) as u64
}

/// Per-wallet ceiling of a pool with budget `budget`: `floor(E * qmax)`.
pub const fn wallet_cap(budget: u64) -> u64 {
    budget / QMAX_DEN
}
const _: () = assert!(QMAX_NUM == 1, "wallet_cap assumes qmax = 1/QMAX_DEN");

/// The three independent Fruit ledgers.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LedgerKind {
    #[default]
    Core = 0,
    Loyalty = 1,
    Lock = 2,
}

impl LedgerKind {
    pub const ALL: [LedgerKind; 3] = [LedgerKind::Core, LedgerKind::Loyalty, LedgerKind::Lock];

    pub fn from_u8(v: u8) -> EngineResult<Self> {
        match v {
            0 => Ok(LedgerKind::Core),
            1 => Ok(LedgerKind::Loyalty),
            2 => Ok(LedgerKind::Lock),
            _ => Err(EngineError::InvalidLedgerKind),
        }
    }
}

/// `L`, `U`, `Z` and cumulative payouts of one Fruit ledger.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FruitLedger {
    /// `L`: established, unpaid liabilities.
    pub liabilities: u64,
    /// `U`: committed capacity not yet established as liability.
    pub committed: u64,
    /// `Z`: uncommitted available reserve.
    pub reserve: u64,
    /// Lamports ever paid to claimants from this ledger.
    pub paid_cum: u64,
}

impl FruitLedger {
    /// `L + U + Z`.
    pub fn obligations(&self) -> EngineResult<u64> {
        self.liabilities
            .checked_add(self.committed)
            .and_then(|x| x.checked_add(self.reserve))
            .ok_or(EngineError::MathOverflow)
    }

    /// The solvency invariant `V >= L + U + Z`.
    pub fn check_solvent(&self, vault_available: u64) -> EngineResult<()> {
        if vault_available < self.obligations()? {
            return Err(EngineError::FruitInsolvent);
        }
        Ok(())
    }

    /// FUND: `V += a; Z += a` (the caller moves the lamports).
    pub fn fund(&mut self, amount: u64) -> EngineResult<()> {
        if amount == 0 {
            return Err(EngineError::InvalidFundingAmount);
        }
        self.reserve = self
            .reserve
            .checked_add(amount)
            .ok_or(EngineError::MathOverflow)?;
        Ok(())
    }

    /// COMMIT: requires `Z >= E`; `Z -= E; U += E`.
    pub fn commit(&mut self, budget: u64) -> EngineResult<()> {
        if self.reserve < budget {
            return Err(EngineError::InsufficientFruitReserve);
        }
        self.reserve -= budget;
        self.committed = self
            .committed
            .checked_add(budget)
            .ok_or(EngineError::MathOverflow)?;
        Ok(())
    }

    /// FINALIZE LIABILITY: `U -= delta; L += delta`.
    pub fn establish(&mut self, delta: u64) -> EngineResult<()> {
        self.committed = self
            .committed
            .checked_sub(delta)
            .ok_or(EngineError::FruitCommitmentUnderflow)?;
        self.liabilities = self
            .liabilities
            .checked_add(delta)
            .ok_or(EngineError::MathOverflow)?;
        Ok(())
    }

    /// PAY CLAIM: `V -= p; L -= p` (the caller moves the lamports). The only
    /// transition that lowers `L`.
    pub fn pay(&mut self, payment: u64) -> EngineResult<()> {
        self.liabilities = self
            .liabilities
            .checked_sub(payment)
            .ok_or(EngineError::FruitLiabilityUnderflow)?;
        self.paid_cum = self
            .paid_cum
            .checked_add(payment)
            .ok_or(EngineError::MathOverflow)?;
        Ok(())
    }

    /// RELEASE VERIFIED UNUSED CAPACITY: `U -= z; Z += z`.
    pub fn release(&mut self, amount: u64) -> EngineResult<()> {
        self.committed = self
            .committed
            .checked_sub(amount)
            .ok_or(EngineError::FruitCommitmentUnderflow)?;
        self.reserve = self
            .reserve
            .checked_add(amount)
            .ok_or(EngineError::MathOverflow)?;
        Ok(())
    }

    /// RESERVE WITHDRAWAL: `V -= z; Z -= z`. Draws on `Z` only — never `L`,
    /// never `U` — however many lamports the vault holds.
    pub fn withdraw_reserve(&mut self, amount: u64) -> EngineResult<()> {
        if amount == 0 {
            return Err(EngineError::InvalidFundingAmount);
        }
        if amount > self.reserve {
            return Err(EngineError::ReserveWithdrawalExceedsReserve);
        }
        self.reserve -= amount;
        Ok(())
    }
}

/// Pool lifecycle (§10a). `Committed`, `Active` and `Ended` follow the clock;
/// `Reconciling` means a root marked complete is pending or final; `Closed`
/// is terminal and only [`close_pool`] reaches it.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PoolStatus {
    #[default]
    Committed = 0,
    Active = 1,
    Ended = 2,
    Reconciling = 3,
    Closed = 4,
}

impl PoolStatus {
    pub fn from_u8(v: u8) -> EngineResult<Self> {
        match v {
            0 => Ok(PoolStatus::Committed),
            1 => Ok(PoolStatus::Active),
            2 => Ok(PoolStatus::Ended),
            3 => Ok(PoolStatus::Reconciling),
            4 => Ok(PoolStatus::Closed),
            _ => Err(EngineError::InvalidFruitState),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RootStatus {
    #[default]
    Pending = 0,
    Vetoed = 1,
    Final = 2,
}

impl RootStatus {
    pub fn from_u8(v: u8) -> EngineResult<Self> {
        match v {
            0 => Ok(RootStatus::Pending),
            1 => Ok(RootStatus::Vetoed),
            2 => Ok(RootStatus::Final),
            _ => Err(EngineError::InvalidFruitState),
        }
    }
}

/// Accounting state of one pool (one ledger × one epoch/Season).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FruitPool {
    /// `E`: committed budget.
    pub budget: u64,
    pub t_start: i64,
    pub t_end: i64,
    /// `floor(E/5)`, fixed at commit.
    pub wallet_cap: u64,
    /// Total of the latest Final root: established liabilities of this pool.
    pub final_total: u64,
    /// Lamports paid from this pool. Always `<= final_total`.
    pub paid_cum: u64,
    /// Highest root version ever posted (Pending, Vetoed or Final).
    pub latest_version: u64,
    /// Version of the Pending root, `0` when none.
    pub pending_version: u64,
    /// Whether the Pending root is marked complete.
    pub pending_complete: bool,
    /// Version of the latest Final root, `0` when none.
    pub final_version: u64,
    /// Whether the latest Final root is marked complete (reconciled).
    pub final_complete: bool,
    pub status: PoolStatus,
    /// `forfeited` carried by the Pending root (lock ledger only, else 0).
    pub pending_forfeited: u64,
    /// `forfeited` carried by the latest Final root; 0 unless it is complete.
    pub final_forfeited: u64,
    /// Lamports released from `Z` to the Protocol Treasury at close
    /// (`floor(0.80 final_forfeited)`), reported separately from fee revenue.
    pub forfeit_treasury_released: u64,
    /// Ledger this pool belongs to. [`commit_pool`] makes core/loyalty
    /// pools; only [`crate::fruit_lock::commit_lock_pool`] makes a
    /// [`LedgerKind::Lock`] pool, and the generic [`post_root`] and
    /// [`terminal_resolution`] refuse it.
    pub kind: LedgerKind,
    /// Lock pools only: the pool-owned allocation lifecycle (Pass #7
    /// P7-L1 / F7 / P7-B). Default (empty, `Registering`) on every other pool.
    pub lock: LockState,
}

/// Accounting state of one posted root.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FruitRoot {
    pub version: u64,
    pub checkpoint_time: i64,
    pub total: u64,
    /// Poster's reconciliation-complete mark; only valid at or after `t_end`.
    pub complete: bool,
    pub posted_at: i64,
    pub final_after: i64,
    pub status: RootStatus,
    /// Distinct veto signatures recorded so far.
    pub veto_count: u8,
    /// Voluntary lock-forfeit capacity carried by a complete lock root
    /// (architecture §8 / §7a item 8). Always 0 for core and loyalty roots.
    pub forfeited: u64,
}

/// Scheduled capacity at `checkpoint_time`:
/// `floor(E * (min(t, t_end) - t_start) / (t_end - t_start))`, 0 before
/// `t_start`, exactly `E` from `t_end`. Monotone in `t`, so successive
/// differences telescope: checkpoint frequency cannot change the total.
pub fn scheduled_capacity(
    budget: u64,
    t_start: i64,
    t_end: i64,
    checkpoint_time: i64,
) -> EngineResult<u64> {
    if t_end <= t_start {
        return Err(EngineError::InvalidPoolWindow);
    }
    let duration = (t_end as i128) - (t_start as i128);
    let elapsed = ((checkpoint_time.min(t_end) as i128) - (t_start as i128)).max(0);
    let cap = (budget as i128) * elapsed / duration;
    u64::try_from(cap).map_err(|_| EngineError::MathOverflow)
}

impl FruitPool {
    /// Re-derive the clock-driven status. `Closed` is sticky.
    pub fn refresh_status(&mut self, now: i64) {
        if self.status == PoolStatus::Closed {
            return;
        }
        self.status = if now < self.t_start {
            PoolStatus::Committed
        } else if now < self.t_end {
            PoolStatus::Active
        } else if self.final_complete || (self.pending_version != 0 && self.pending_complete) {
            PoolStatus::Reconciling
        } else {
            PoolStatus::Ended
        };
    }

    fn require_open(&self) -> EngineResult<()> {
        if self.status == PoolStatus::Closed {
            return Err(EngineError::FruitPoolClosed);
        }
        Ok(())
    }
}

/// COMMIT POOL. Moves `E` from `Z` to `U` exactly once; the pool must not
/// have started.
pub fn commit_pool(
    ledger: &mut FruitLedger,
    budget: u64,
    t_start: i64,
    t_end: i64,
    now: i64,
) -> EngineResult<FruitPool> {
    if budget == 0 {
        return Err(EngineError::InvalidFundingAmount);
    }
    if t_end <= t_start {
        return Err(EngineError::InvalidPoolWindow);
    }
    if now >= t_start {
        return Err(EngineError::FruitPoolStarted);
    }
    let mut next = *ledger;
    next.commit(budget)?;
    *ledger = next;
    let mut pool = FruitPool {
        budget,
        t_start,
        t_end,
        wallet_cap: wallet_cap(budget),
        ..FruitPool::default()
    };
    pool.refresh_status(now);
    Ok(pool)
}

/// Largest `forfeited` a complete root with `total` may carry:
/// `assigned_total - total` (architecture §8 test 89, Pass #7 F7), `0` on a
/// non-lock pool, on a lock pool without a Final allocation, or when `total`
/// already exceeds it. Rounding dust (`gross_allocatable - assigned_total`)
/// and unallocated capacity are outside the bound, so they can never be
/// labelled forfeiture.
fn forfeit_bound(pool: &FruitPool, total: u64) -> EngineResult<u64> {
    if pool.kind != LedgerKind::Lock
        || pool.lock.status != crate::fruit_lock::LockAllocationStatus::Final
    {
        return Ok(0);
    }
    let alloc = lock_final_allocation(pool).map_err(|_| EngineError::InvalidForfeited)?;
    Ok(alloc.assigned_total.saturating_sub(total))
}

/// POST ROOT (Pending). All §10a invariant-3 and invariant-4 checks.
///
/// `forfeited` (lock ledgers only; the caller rejects it elsewhere) may be
/// non-zero only on a root marked complete, and never above
/// `sum A_i - total` (architecture §8, test 89), where `sum A_i` is the
/// Final lock allocation's `assigned_total`. With no Final allocation (or on
/// a non-lock pool) any non-zero `forfeited` is rejected. Since
/// `assigned_total <= E`, this also keeps the release inside `E - total`, so
/// the treasury release can never touch `L` or `U`.
///
/// Refuses [`LedgerKind::Lock`] pools: their capacity is the lock cliff, not
/// the linear schedule, so they post only through
/// [`crate::fruit_lock::post_lock_root`].
#[allow(clippy::too_many_arguments)]
pub fn post_root(
    pool: &mut FruitPool,
    version: u64,
    checkpoint_time: i64,
    total: u64,
    complete: bool,
    forfeited: u64,
    now: i64,
    challenge_window: i64,
) -> EngineResult<FruitRoot> {
    if pool.kind == LedgerKind::Lock {
        return Err(EngineError::InvalidLedgerKind);
    }
    post_root_unchecked(
        pool,
        version,
        checkpoint_time,
        total,
        complete,
        forfeited,
        now,
        challenge_window,
    )
}

/// [`post_root`] without the ledger-kind gate: the lock path calls it only
/// after checking the lock cliff capacity.
#[allow(clippy::too_many_arguments)]
pub(crate) fn post_root_unchecked(
    pool: &mut FruitPool,
    version: u64,
    checkpoint_time: i64,
    total: u64,
    complete: bool,
    forfeited: u64,
    now: i64,
    challenge_window: i64,
) -> EngineResult<FruitRoot> {
    pool.require_open()?;
    if checkpoint_time > now {
        return Err(EngineError::FutureCheckpoint);
    }
    if version <= pool.latest_version {
        return Err(EngineError::StaleRootVersion);
    }
    if pool.pending_version != 0 {
        return Err(EngineError::RootAlreadyPending);
    }
    if total > scheduled_capacity(pool.budget, pool.t_start, pool.t_end, checkpoint_time)? {
        return Err(EngineError::RootExceedsCapacity);
    }
    if total < pool.final_total {
        return Err(EngineError::RootBelowFinalTotal);
    }
    if complete && checkpoint_time < pool.t_end {
        return Err(EngineError::IncompleteCheckpoint);
    }
    if forfeited > 0 && (!complete || forfeited > forfeit_bound(pool, total)?) {
        return Err(EngineError::InvalidForfeited);
    }
    if challenge_window <= 0 {
        return Err(EngineError::InvalidFruitGovernance);
    }
    let final_after = now
        .checked_add(challenge_window)
        .ok_or(EngineError::MathOverflow)?;

    let mut next = *pool;
    next.latest_version = version;
    next.pending_version = version;
    next.pending_complete = complete;
    next.pending_forfeited = forfeited;
    next.refresh_status(now);
    *pool = next;
    Ok(FruitRoot {
        version,
        checkpoint_time,
        total,
        complete,
        posted_at: now,
        final_after,
        status: RootStatus::Pending,
        veto_count: 0,
        forfeited,
    })
}

/// Record one distinct veto signature (the caller guarantees distinctness).
/// Returns `true` when this signature reached `threshold` and vetoed the root.
/// Only possible while the root is Pending and inside its challenge window.
pub fn veto_vote(
    pool: &mut FruitPool,
    root: &mut FruitRoot,
    threshold: u8,
    now: i64,
) -> EngineResult<bool> {
    if root.status != RootStatus::Pending || pool.pending_version != root.version {
        return Err(EngineError::RootNotPending);
    }
    if now >= root.final_after {
        return Err(EngineError::ChallengeWindowClosed);
    }
    if threshold == 0 {
        return Err(EngineError::InvalidFruitGovernance);
    }
    let mut r = *root;
    let mut p = *pool;
    r.veto_count = r
        .veto_count
        .checked_add(1)
        .ok_or(EngineError::MathOverflow)?;
    let vetoed = r.veto_count >= threshold;
    if vetoed {
        r.status = RootStatus::Vetoed;
        p.pending_version = 0;
        p.pending_complete = false;
        p.pending_forfeited = 0;
        p.refresh_status(now);
    }
    *root = r;
    *pool = p;
    Ok(vetoed)
}

/// FINALIZE a Pending root after its window: `U -= Δ; L += Δ` with
/// `Δ = total - final_total`. Returns `Δ`.
pub fn finalize_root(
    ledger: &mut FruitLedger,
    pool: &mut FruitPool,
    root: &mut FruitRoot,
    now: i64,
) -> EngineResult<u64> {
    if root.status != RootStatus::Pending || pool.pending_version != root.version {
        return Err(EngineError::RootNotPending);
    }
    if now < root.final_after {
        return Err(EngineError::ChallengeWindowOpen);
    }
    // A lock root can never establish more than the Final allocation's
    // assigned awards, whatever root object the caller hands in.
    if pool.kind == LedgerKind::Lock && root.total > lock_final_allocation(pool)?.assigned_total {
        return Err(EngineError::RootExceedsCapacity);
    }
    let delta = root
        .total
        .checked_sub(pool.final_total)
        .ok_or(EngineError::RootBelowFinalTotal)?;
    let mut l = *ledger;
    let mut p = *pool;
    let mut r = *root;
    l.establish(delta)?;
    p.final_total = r.total;
    p.final_version = r.version;
    p.final_complete = r.complete;
    p.final_forfeited = if r.complete { r.forfeited } else { 0 };
    p.pending_version = 0;
    p.pending_complete = false;
    p.pending_forfeited = 0;
    p.refresh_status(now);
    r.status = RootStatus::Final;
    *ledger = l;
    *pool = p;
    *root = r;
    Ok(delta)
}

/// TERMINAL RESOLUTION (§10): a quorum-signed final settlement after `t_end`
/// when history cannot be recovered. Final immediately and complete; subject
/// to the same capacity, version and monotone-finality rules as any root, so
/// it can only add liabilities (and let close release excess to `Z`) — it can
/// never reduce `L`. The caller verifies the quorum.
///
/// Refuses [`LedgerKind::Lock`] pools (whose generic cap would be `E`, above
/// the assigned awards); they resolve only through
/// [`crate::fruit_lock::terminal_lock_resolution`].
pub fn terminal_resolution(
    ledger: &mut FruitLedger,
    pool: &mut FruitPool,
    version: u64,
    checkpoint_time: i64,
    total: u64,
    now: i64,
) -> EngineResult<(FruitRoot, u64)> {
    if pool.kind == LedgerKind::Lock {
        return Err(EngineError::InvalidLedgerKind);
    }
    terminal_resolution_unchecked(ledger, pool, version, checkpoint_time, total, now)
}

/// [`terminal_resolution`] without the ledger-kind gate (lock path only,
/// after its capacity check).
pub(crate) fn terminal_resolution_unchecked(
    ledger: &mut FruitLedger,
    pool: &mut FruitPool,
    version: u64,
    checkpoint_time: i64,
    total: u64,
    now: i64,
) -> EngineResult<(FruitRoot, u64)> {
    pool.require_open()?;
    if now < pool.t_end {
        return Err(EngineError::FruitPoolNotEnded);
    }
    if checkpoint_time < pool.t_end {
        return Err(EngineError::IncompleteCheckpoint);
    }
    let mut p = *pool;
    let mut l = *ledger;
    // Unknown history is never treated as voluntary exit (§8): a terminal
    // resolution carries no forfeiture, so nothing it releases leaves `Z`.
    let mut root = post_root_unchecked(&mut p, version, checkpoint_time, total, true, 0, now, 1)?;
    root.final_after = now;
    let delta = finalize_root(&mut l, &mut p, &mut root, now)?;
    *pool = p;
    *ledger = l;
    Ok((root, delta))
}

/// PAY CLAIM against the pool's latest Final root. `cumulative` is the
/// wallet's verified leaf amount; the caller has verified the proof.
///
/// Pays `max(0, cumulative - claimed_cum)`, then
/// `claimed_cum = max(claimed_cum, cumulative)`. A duplicate or lower leaf
/// pays 0 and claws nothing back. Returns the payment.
///
/// Refuses [`LedgerKind::Lock`] pools: a lock claim is also bounded by the
/// wallet's own award, so it goes only through
/// [`crate::fruit_lock::claim_lock`].
pub fn claim(
    ledger: &mut FruitLedger,
    pool: &mut FruitPool,
    claimed_cum: &mut u64,
    cumulative: u64,
) -> EngineResult<u64> {
    if pool.kind == LedgerKind::Lock {
        return Err(EngineError::InvalidLedgerKind);
    }
    claim_unchecked(ledger, pool, claimed_cum, cumulative)
}

/// [`claim`] without the ledger-kind gate (lock path only, after its
/// per-wallet award bound).
pub(crate) fn claim_unchecked(
    ledger: &mut FruitLedger,
    pool: &mut FruitPool,
    claimed_cum: &mut u64,
    cumulative: u64,
) -> EngineResult<u64> {
    if pool.final_version == 0 {
        return Err(EngineError::NoFinalRoot);
    }
    if cumulative > pool.wallet_cap {
        return Err(EngineError::LeafAboveWalletCap);
    }
    let payment = cumulative.saturating_sub(*claimed_cum);
    let paid = pool
        .paid_cum
        .checked_add(payment)
        .ok_or(EngineError::MathOverflow)?;
    if paid > pool.final_total {
        return Err(EngineError::PaidExceedsFinalTotal);
    }
    let mut l = *ledger;
    l.pay(payment)?;
    *ledger = l;
    pool.paid_cum = paid;
    *claimed_cum = (*claimed_cum).max(cumulative);
    Ok(payment)
}

/// What a [`close_pool`] released.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloseOutcome {
    /// `E - final_total`, moved `U -> Z`.
    pub released: u64,
    /// `floor(0.80 forfeited)`, then moved out of `Z` to the Protocol
    /// Treasury in the same close (`V -= t; Z -= t`). 0 when nothing was
    /// forfeited. The caller moves exactly these lamports.
    pub treasury_release: u64,
}

/// CLOSE: after `t_end`, with no Pending root and a Final root marked
/// complete, release `E - final_total` from `U` to `Z`, exactly once; then
/// release `floor(0.80 forfeited)` of it from `Z` to the Protocol Treasury
/// (lock ledger, architecture §8 / §7a item 8). `forfeited - release`,
/// unallocated and safety-cancelled capacity all stay in `Z`. Elapsed time
/// alone never satisfies it. `L` and `U` are never touched by the treasury
/// release.
pub fn close_pool(
    ledger: &mut FruitLedger,
    pool: &mut FruitPool,
    now: i64,
) -> EngineResult<CloseOutcome> {
    pool.require_open()?;
    if now < pool.t_end {
        return Err(EngineError::FruitPoolNotEnded);
    }
    if pool.pending_version != 0 {
        return Err(EngineError::RootAlreadyPending);
    }
    if pool.final_version == 0 || !pool.final_complete {
        return Err(EngineError::FruitPoolNotReconciled);
    }
    // A lock pool closes only against its pool-owned Final allocation.
    if pool.kind == LedgerKind::Lock {
        lock_final_allocation(pool)?;
    }
    let release = pool
        .budget
        .checked_sub(pool.final_total)
        .ok_or(EngineError::MathOverflow)?;
    // Enforced at post time; re-checked against `assigned_total - final_total` (and
    // so `E - final_total`) so the release can never exceed the capacity
    // this close returns to `Z`, even from corrupted state.
    if pool.final_forfeited > 0
        && (pool.final_forfeited > forfeit_bound(pool, pool.final_total)?
            || pool.final_forfeited > release)
    {
        return Err(EngineError::InvalidForfeited);
    }
    let treasury_release = lock_forfeit_treasury_release(pool.final_forfeited);
    let mut l = *ledger;
    l.release(release)?;
    if treasury_release > 0 {
        l.withdraw_reserve(treasury_release)?;
    }
    *ledger = l;
    pool.forfeit_treasury_released = treasury_release;
    pool.status = PoolStatus::Closed;
    Ok(CloseOutcome {
        released: release,
        treasury_release,
    })
}
