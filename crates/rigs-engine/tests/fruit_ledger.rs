//! Fruit ledger accounting — engine level (architecture §10 / §10a).
//!
//! Frozen-architecture tests 55–67 and 73–76 against the pure state machine,
//! plus a property test that drives random transition sequences and checks
//! after every step:
//!
//! * `V >= L + U + Z` per ledger and in aggregate;
//! * per pool, established allocations `final_total <= E`;
//! * every wallet's paid entitlement `<= floor(E/5)`;
//! * `paid_cum <= final_total`;
//! * `L` never decreases except by a payment.

use proptest::prelude::*;
use rigs_engine::fruit::claim;
use rigs_engine::*;

const SOL: u64 = 1_000_000_000;
const WINDOW: i64 = 3_600;

/// One ledger with a simulated vault (spendable lamports) and its pools.
#[derive(Clone, Debug, Default)]
struct Sim {
    vault: u64,
    ledger: FruitLedger,
    pools: Vec<(FruitPool, Vec<FruitRoot>, Vec<u64>)>, // pool, roots, claimed_cum per wallet
    wallets: usize,
}

impl Sim {
    fn new(wallets: usize) -> Self {
        Sim {
            wallets,
            ..Sim::default()
        }
    }

    fn check(&self) {
        assert!(
            self.vault >= self.ledger.obligations().unwrap(),
            "V {} < L+U+Z {:?}",
            self.vault,
            self.ledger
        );
        let mut established = 0u64;
        let mut paid = 0u64;
        for (p, _, claimed) in &self.pools {
            assert!(p.final_total <= p.budget, "sum allocations > E");
            assert!(p.paid_cum <= p.final_total, "paid_cum > final_total");
            for c in claimed {
                assert!(*c <= p.wallet_cap, "wallet entitlement > E/5");
            }
            assert_eq!(claimed.iter().sum::<u64>(), p.paid_cum);
            established += p.final_total;
            paid += p.paid_cum;
        }
        // L is exactly what finalized roots established minus what was paid.
        assert_eq!(self.ledger.liabilities, established - paid);
    }

    fn fund(&mut self, a: u64) -> EngineResult<()> {
        let mut l = self.ledger;
        l.fund(a)?;
        self.ledger = l;
        self.vault += a;
        self.check();
        Ok(())
    }

    fn withdraw(&mut self, a: u64) -> EngineResult<()> {
        let mut l = self.ledger;
        l.withdraw_reserve(a)?;
        self.ledger = l;
        self.vault -= a;
        self.check();
        Ok(())
    }

    fn commit(&mut self, e: u64, t0: i64, t1: i64, now: i64) -> EngineResult<usize> {
        let p = commit_pool(&mut self.ledger, e, t0, t1, now)?;
        self.pools.push((p, Vec::new(), vec![0; self.wallets]));
        self.check();
        Ok(self.pools.len() - 1)
    }

    fn post(
        &mut self,
        i: usize,
        v: u64,
        cp: i64,
        total: u64,
        complete: bool,
        now: i64,
    ) -> EngineResult<()> {
        self.post_forfeit(i, v, cp, total, complete, 0, now)
    }

    #[allow(clippy::too_many_arguments)]
    fn post_forfeit(
        &mut self,
        i: usize,
        v: u64,
        cp: i64,
        total: u64,
        complete: bool,
        forfeited: u64,
        now: i64,
    ) -> EngineResult<()> {
        let (p, roots, _) = &mut self.pools[i];
        let r = post_root(p, v, cp, total, complete, forfeited, now, WINDOW)?;
        roots.push(r);
        self.check();
        Ok(())
    }

    fn pending_index(&self, i: usize) -> Option<usize> {
        let (p, roots, _) = &self.pools[i];
        roots
            .iter()
            .position(|r| r.version == p.pending_version && r.status == RootStatus::Pending)
    }

    fn veto(&mut self, i: usize, threshold: u8, now: i64) -> EngineResult<bool> {
        let j = self.pending_index(i).ok_or(EngineError::RootNotPending)?;
        let (p, roots, _) = &mut self.pools[i];
        let out = veto_vote(p, &mut roots[j], threshold, now)?;
        self.check();
        Ok(out)
    }

    fn finalize(&mut self, i: usize, now: i64) -> EngineResult<u64> {
        let j = self.pending_index(i).ok_or(EngineError::RootNotPending)?;
        let l_before = self.ledger.liabilities;
        let (p, roots, _) = &mut self.pools[i];
        let d = finalize_root(&mut self.ledger, p, &mut roots[j], now)?;
        assert!(self.ledger.liabilities >= l_before, "finalize reduced L");
        self.check();
        Ok(d)
    }

    fn claim(&mut self, i: usize, w: usize, cumulative: u64) -> EngineResult<u64> {
        let l_before = self.ledger.liabilities;
        let (p, _, claimed) = &mut self.pools[i];
        let pay = claim(&mut self.ledger, p, &mut claimed[w], cumulative)?;
        self.vault -= pay;
        assert_eq!(
            l_before - self.ledger.liabilities,
            pay,
            "L moved by other than payment"
        );
        self.check();
        Ok(pay)
    }

    fn close(&mut self, i: usize, now: i64) -> EngineResult<u64> {
        Ok(self.close_full(i, now)?.released)
    }

    /// Close, moving the treasury release out of the vault as the program
    /// does (`V -= t; Z -= t`).
    fn close_full(&mut self, i: usize, now: i64) -> EngineResult<CloseOutcome> {
        let l_before = self.ledger.liabilities;
        let u_before = self.ledger.committed;
        let (p, _, _) = &mut self.pools[i];
        let budget_left = p.budget - p.final_total;
        let r = close_pool(&mut self.ledger, p, now)?;
        assert_eq!(self.ledger.liabilities, l_before, "close touched L");
        assert_eq!(
            u_before - self.ledger.committed,
            budget_left,
            "close moved U by other than E - final_total"
        );
        self.vault -= r.treasury_release;
        self.check();
        Ok(r)
    }

    fn terminal(&mut self, i: usize, v: u64, cp: i64, total: u64, now: i64) -> EngineResult<u64> {
        let l_before = self.ledger.liabilities;
        let (p, roots, _) = &mut self.pools[i];
        let (r, d) = terminal_resolution(&mut self.ledger, p, v, cp, total, now)?;
        roots.push(r);
        assert!(
            self.ledger.liabilities >= l_before,
            "terminal resolution reduced L"
        );
        self.check();
        Ok(d)
    }

    fn pool(&self, i: usize) -> &FruitPool {
        &self.pools[i].0
    }
}

/// Funded ledger with one 10 SOL pool over [1_000, 2_000), committed at 0.
fn one_pool(wallets: usize) -> Sim {
    let mut s = Sim::new(wallets);
    s.fund(10 * SOL).unwrap();
    s.commit(10 * SOL, 1_000, 2_000, 0).unwrap();
    s
}

/// Post and finalize a root on pool 0.
fn finalize_at(s: &mut Sim, v: u64, cp: i64, total: u64, complete: bool, now: i64) {
    s.post(0, v, cp, total, complete, now).unwrap();
    s.finalize(0, now + WINDOW).unwrap();
}

// ---------------------------------------------------------------- constants

#[test]
fn frozen_constants() {
    assert_eq!(P_REF_LAMPORTS, 10_000_000);
    assert_eq!(FRUIT_M, 250);
    assert_eq!(FRUIT_M, MAX_WALLET_RIGS);
    assert_eq!((QMAX_NUM, QMAX_DEN), (1, 5));
    assert_eq!(wallet_cap(10 * SOL), 2 * SOL);
    assert_eq!(wallet_cap(14), 2); // floor(E/5)
    assert_eq!(wallet_cap(4), 0);
    for k in LedgerKind::ALL {
        assert_eq!(LedgerKind::from_u8(k as u8).unwrap(), k);
    }
    assert_eq!(LedgerKind::from_u8(3), Err(EngineError::InvalidLedgerKind));
}

// ---------------------------------------------------------------- 55–57 roots

/// 55: root total above scheduled capacity rejected.
#[test]
fn t55_root_above_scheduled_capacity_rejected() {
    let mut s = one_pool(1);
    // Halfway: capacity is exactly 5 SOL.
    assert_eq!(
        scheduled_capacity(10 * SOL, 1_000, 2_000, 1_500).unwrap(),
        5 * SOL
    );
    assert_eq!(
        s.post(0, 1, 1_500, 5 * SOL + 1, false, 1_500),
        Err(EngineError::RootExceedsCapacity)
    );
    // Before t_start capacity is 0; after t_end it is exactly E.
    assert_eq!(
        s.post(0, 1, 900, 1, false, 1_500),
        Err(EngineError::RootExceedsCapacity)
    );
    assert_eq!(
        s.post(0, 1, 5_000, 10 * SOL + 1, false, 5_000),
        Err(EngineError::RootExceedsCapacity)
    );
    s.post(0, 1, 1_500, 5 * SOL, false, 1_500).unwrap();
}

/// 56: future checkpoint, non-increasing version, second Pending root rejected.
#[test]
fn t56_future_checkpoint_bad_version_second_pending_rejected() {
    let mut s = one_pool(1);
    assert_eq!(
        s.post(0, 1, 1_501, 1, false, 1_500),
        Err(EngineError::FutureCheckpoint)
    );
    s.post(0, 3, 1_500, 1, false, 1_500).unwrap();
    // Second Pending root, even with a higher version.
    assert_eq!(
        s.post(0, 4, 1_500, 1, false, 1_500),
        Err(EngineError::RootAlreadyPending)
    );
    s.finalize(0, 1_500 + WINDOW).unwrap();
    // Same and lower versions rejected after finalization.
    assert_eq!(
        s.post(0, 3, 1_600, 2, false, 6_000),
        Err(EngineError::StaleRootVersion)
    );
    assert_eq!(
        s.post(0, 2, 1_600, 2, false, 6_000),
        Err(EngineError::StaleRootVersion)
    );
    // A vetoed version is also consumed.
    s.post(0, 4, 1_600, 2, false, 6_000).unwrap();
    assert!(s.veto(0, 1, 6_001).unwrap());
    assert_eq!(
        s.post(0, 4, 1_600, 2, false, 6_002),
        Err(EngineError::StaleRootVersion)
    );
    s.post(0, 5, 1_600, 2, false, 6_002).unwrap();
}

/// 57: total below final_total rejected.
#[test]
fn t57_total_below_final_total_rejected() {
    let mut s = one_pool(1);
    finalize_at(&mut s, 1, 1_500, 4 * SOL, false, 1_500);
    assert_eq!(
        s.post(0, 2, 1_900, 4 * SOL - 1, false, 9_000),
        Err(EngineError::RootBelowFinalTotal)
    );
    s.post(0, 2, 1_900, 4 * SOL, false, 9_000).unwrap();
}

// ---------------------------------------------------------------- 58, 60–62 claims

/// 58: a leaf above the wallet cap is rejected.
#[test]
fn t58_leaf_above_wallet_cap_rejected() {
    let mut s = one_pool(1);
    finalize_at(&mut s, 1, 2_000, 10 * SOL, true, 2_000);
    assert_eq!(s.pool(0).wallet_cap, 2 * SOL);
    assert_eq!(
        s.claim(0, 0, 2 * SOL + 1),
        Err(EngineError::LeafAboveWalletCap)
    );
    assert_eq!(s.claim(0, 0, 2 * SOL).unwrap(), 2 * SOL);
}

/// 60 (engine part): no claim before a root is Final; a vetoed root never
/// finalizes; veto only inside the window, finalize only after it.
#[test]
fn t60_pending_and_vetoed_roots_cannot_pay() {
    let mut s = one_pool(1);
    s.post(0, 1, 1_500, SOL, false, 1_500).unwrap();
    assert_eq!(s.claim(0, 0, SOL), Err(EngineError::NoFinalRoot));
    assert_eq!(
        s.finalize(0, 1_500 + WINDOW - 1),
        Err(EngineError::ChallengeWindowOpen)
    );

    // Threshold 2: one vote does not veto, the second does.
    assert!(!s.veto(0, 2, 1_600).unwrap());
    assert!(s.veto(0, 2, 1_700).unwrap());
    assert_eq!(s.pools[0].1[0].status, RootStatus::Vetoed);
    assert_eq!(
        s.finalize(0, 1_500 + WINDOW),
        Err(EngineError::RootNotPending)
    );
    let (mut p, mut roots, _) = s.pools[0].clone();
    assert_eq!(
        finalize_root(&mut s.ledger.clone(), &mut p, &mut roots[0], 99_999),
        Err(EngineError::RootNotPending)
    );
    assert_eq!(s.claim(0, 0, SOL), Err(EngineError::NoFinalRoot));
    assert_eq!(s.ledger.liabilities, 0);

    // Veto after the window has closed is refused.
    s.post(0, 2, 1_500, SOL, false, 1_800).unwrap();
    assert_eq!(
        s.veto(0, 1, 1_800 + WINDOW),
        Err(EngineError::ChallengeWindowClosed)
    );
    s.finalize(0, 1_800 + WINDOW).unwrap();
    assert_eq!(s.claim(0, 0, SOL).unwrap(), SOL);
}

/// 61: a corrected later root lowering an already-claimed wallet pays 0 and
/// claws nothing back; others are unaffected; paid_cum <= final_total.
#[test]
fn t61_lower_correction_never_claws_back() {
    let mut s = one_pool(2);
    finalize_at(&mut s, 1, 1_500, 3 * SOL, false, 1_500);
    // Leaves at v1: w0 = 2 SOL, w1 = 1 SOL.
    assert_eq!(s.claim(0, 0, 2 * SOL).unwrap(), 2 * SOL);
    // v2 corrects w0 down to 1 SOL and w1 up to 2 SOL. The total counts what
    // w0 was already paid (2 + 2 = 4 SOL): a correction needing extra
    // liability must be inside the funded capacity, never spend it twice.
    finalize_at(&mut s, 2, 1_600, 4 * SOL, false, 9_000);
    let vault = s.vault;
    assert_eq!(s.claim(0, 0, SOL).unwrap(), 0);
    assert_eq!(s.pools[0].2[0], 2 * SOL, "claimed_cum never decreases");
    assert_eq!(s.vault, vault);
    assert_eq!(
        s.ledger.liabilities,
        2 * SOL,
        "w0's lower leaf reduced nothing"
    );
    // w1 is unaffected and receives its full corrected leaf.
    assert_eq!(s.claim(0, 1, 2 * SOL).unwrap(), 2 * SOL);
    assert_eq!(s.pool(0).paid_cum, 4 * SOL);
    assert!(s.pool(0).paid_cum <= s.pool(0).final_total);

    // Hard guard: a correction whose total ignores prior payments cannot
    // overpay the pool, whatever its leaves say.
    let mut t = one_pool(2);
    finalize_at(&mut t, 1, 1_500, 3 * SOL, false, 1_500);
    t.claim(0, 0, 2 * SOL).unwrap();
    finalize_at(&mut t, 2, 1_600, 3 * SOL, false, 9_000);
    t.claim(0, 1, SOL).unwrap();
    assert_eq!(
        t.claim(0, 1, 2 * SOL),
        Err(EngineError::PaidExceedsFinalTotal)
    );
    assert_eq!(t.pool(0).paid_cum, t.pool(0).final_total);
}

/// 62: duplicate claims pay once; many claimants never take V below L.
#[test]
fn t62_duplicate_claim_pays_once_and_v_stays_above_l() {
    let mut s = one_pool(5);
    finalize_at(&mut s, 1, 2_000, 10 * SOL, true, 2_000);
    assert_eq!(s.claim(0, 0, 2 * SOL).unwrap(), 2 * SOL);
    assert_eq!(s.claim(0, 0, 2 * SOL).unwrap(), 0);
    assert_eq!(s.claim(0, 0, SOL).unwrap(), 0);
    for w in 1..5 {
        assert_eq!(s.claim(0, w, 2 * SOL).unwrap(), 2 * SOL);
        assert_eq!(s.claim(0, w, 2 * SOL).unwrap(), 0);
        assert!(s.vault >= s.ledger.liabilities);
    }
    assert_eq!(s.pool(0).paid_cum, 10 * SOL);
    assert_eq!(s.ledger.liabilities, 0);
}

// ---------------------------------------------------------------- 63–66

/// 63: 1 versus 86,400 checkpoints give the same scheduled capacity at every
/// shared instant and exactly E at the end (differences telescope; the only
/// rounding is the floor of the cumulative, never a sum of floors).
#[test]
fn t63_checkpoint_frequency_independence() {
    let (e, t0) = (1_234_567_891u64, 10_000i64);
    let t1 = t0 + 86_400;
    let mut prev = 0u64;
    let mut sum_of_increments = 0u64;
    for t in t0..=t1 {
        let c = scheduled_capacity(e, t0, t1, t).unwrap();
        assert!(c >= prev, "capacity not monotone");
        // Per-step rounding stays within one lamport of the exact rational.
        let exact_num = e as u128 * (t - t0) as u128;
        assert!(exact_num - c as u128 * 86_400 < 86_400);
        sum_of_increments += c - prev;
        prev = c;
    }
    assert_eq!(sum_of_increments, e);
    assert_eq!(scheduled_capacity(e, t0, t1, t1).unwrap(), e);

    // A wallet's cumulative entitlement computed from carried rational
    // weight is identical whether settled once or at every second; summing
    // per-checkpoint floors instead would lose up to one lamport per step.
    let (num, den) = (7u128, 13u128); // wallet share of the pool
    let once = (e as u128 * num / den) as u64;
    let mut carried = 0u128;
    let mut summed_floors = 0u64;
    let mut last_cap = 0u64;
    for t in (t0..=t1).step_by(1) {
        let cap = scheduled_capacity(e, t0, t1, t).unwrap();
        carried += (cap - last_cap) as u128 * num;
        summed_floors += ((cap - last_cap) as u128 * num / den) as u64;
        last_cap = cap;
    }
    assert_eq!((carried / den) as u64, once);
    assert!(once - summed_floors <= 86_400, "naive bound");
    assert!(once >= summed_floors);
}

/// 64: the final lamport remainder is released exactly once at close.
#[test]
fn t64_final_remainder_released_exactly_once() {
    let mut s = Sim::new(1);
    s.fund(1_000_000_007).unwrap();
    s.commit(1_000_000_007, 1_000, 1_003, 0).unwrap();
    // Allocation leaves an odd remainder.
    finalize_at(&mut s, 1, 1_003, 999_999_999, true, 1_003);
    let z = s.ledger.reserve;
    assert_eq!(s.close(0, 5_000).unwrap(), 8);
    assert_eq!(s.ledger.reserve, z + 8);
    assert_eq!(s.ledger.committed, 0);
    assert_eq!(s.close(0, 5_001), Err(EngineError::FruitPoolClosed));
    assert_eq!(s.pool(0).status, PoolStatus::Closed);
    // A closed pool accepts no more roots, but established L still pays.
    assert_eq!(
        s.post(0, 2, 1_003, 999_999_999, true, 6_000),
        Err(EngineError::FruitPoolClosed)
    );
    assert_eq!(s.claim(0, 0, 199_999_999).unwrap(), 199_999_999);
}

/// 65: close without reconciliation, with a Pending root, or early, is
/// rejected; elapsed time alone releases nothing.
#[test]
fn t65_close_requires_reconciliation() {
    let mut s = one_pool(1);
    assert_eq!(s.close(0, 1_999), Err(EngineError::FruitPoolNotEnded));
    // Long after t_end with no root at all.
    assert_eq!(
        s.close(0, 10_000_000),
        Err(EngineError::FruitPoolNotReconciled)
    );
    // A Final root that is not marked complete.
    finalize_at(&mut s, 1, 1_900, 9 * SOL, false, 1_900);
    assert_eq!(
        s.close(0, 10_000_000),
        Err(EngineError::FruitPoolNotReconciled)
    );
    // A complete root still Pending.
    s.post(0, 2, 2_000, 9 * SOL, true, 20_000).unwrap();
    assert_eq!(s.pool(0).status, PoolStatus::Reconciling);
    assert_eq!(s.close(0, 20_001), Err(EngineError::RootAlreadyPending));
    assert_eq!(s.ledger.committed, SOL);
    // Complete roots must checkpoint at or after t_end.
    s.finalize(0, 20_000 + WINDOW).unwrap();
    assert_eq!(s.close(0, 30_000).unwrap(), SOL);
    let mut t = one_pool(1);
    assert_eq!(
        t.post(0, 1, 1_999, 0, true, 5_000),
        Err(EngineError::IncompleteCheckpoint)
    );
}

/// 66: terminal resolution cannot reduce L; it can only add liabilities and
/// let close release the excess to Z.
#[test]
fn t66_terminal_resolution_cannot_reduce_l() {
    let mut s = one_pool(1);
    finalize_at(&mut s, 1, 1_600, 6 * SOL, false, 1_600);
    assert_eq!(s.ledger.liabilities, 6 * SOL);
    // Below final_total: rejected.
    assert_eq!(
        s.terminal(0, 2, 2_000, 6 * SOL - 1, 9_000),
        Err(EngineError::RootBelowFinalTotal)
    );
    // Before t_end, or above E: rejected.
    assert_eq!(
        s.terminal(0, 2, 1_900, 7 * SOL, 1_950),
        Err(EngineError::FruitPoolNotEnded)
    );
    assert_eq!(
        s.terminal(0, 2, 2_000, 10 * SOL + 1, 9_000),
        Err(EngineError::RootExceedsCapacity)
    );
    assert_eq!(s.ledger.liabilities, 6 * SOL);
    // Accepted: adds 1 SOL of liability, immediately final and complete.
    assert_eq!(s.terminal(0, 2, 2_000, 7 * SOL, 9_000).unwrap(), SOL);
    assert_eq!(s.ledger.liabilities, 7 * SOL);
    assert!(s.pool(0).final_complete);
    assert_eq!(s.close(0, 9_000).unwrap(), 3 * SOL);
    assert_eq!(s.ledger.liabilities, 7 * SOL);
    // With a Pending root, terminal resolution waits for veto/finalize.
    let mut t = one_pool(1);
    t.post(0, 1, 2_000, SOL, false, 3_000).unwrap();
    assert_eq!(
        t.terminal(0, 2, 2_000, SOL, 3_001),
        Err(EngineError::RootAlreadyPending)
    );
}

// ---------------------------------------------------------------- 73–76 funding

/// 73: Z < E commit rejected; commit at or after t_start rejected.
#[test]
fn t73_underfunded_or_late_commit_rejected() {
    let mut s = Sim::new(1);
    s.fund(SOL).unwrap();
    assert_eq!(
        s.commit(SOL + 1, 100, 200, 0),
        Err(EngineError::InsufficientFruitReserve)
    );
    assert_eq!(
        s.commit(SOL, 100, 200, 100),
        Err(EngineError::FruitPoolStarted)
    );
    assert_eq!(
        s.commit(SOL, 100, 200, 150),
        Err(EngineError::FruitPoolStarted)
    );
    assert_eq!(
        s.commit(SOL, 200, 200, 0),
        Err(EngineError::InvalidPoolWindow)
    );
    assert_eq!(
        s.commit(0, 100, 200, 0),
        Err(EngineError::InvalidFundingAmount)
    );
    assert_eq!(
        s.ledger,
        FruitLedger {
            reserve: SOL,
            ..Default::default()
        }
    );
    s.commit(SOL, 100, 200, 99).unwrap();
}

/// 74: the same reserve cannot fund two commitments.
#[test]
fn t74_same_reserve_cannot_fund_two_commitments() {
    let mut s = Sim::new(1);
    s.fund(5 * SOL).unwrap();
    s.commit(5 * SOL, 100, 200, 0).unwrap();
    assert_eq!(
        s.commit(5 * SOL, 100, 200, 0),
        Err(EngineError::InsufficientFruitReserve)
    );
    assert_eq!(
        s.commit(1, 300, 400, 0),
        Err(EngineError::InsufficientFruitReserve)
    );
    assert_eq!((s.ledger.reserve, s.ledger.committed), (0, 5 * SOL));
    // Another ledger's reserve is a different struct entirely.
    let mut other = Sim::new(1);
    assert_eq!(
        other.commit(1, 300, 400, 0),
        Err(EngineError::InsufficientFruitReserve)
    );
}

/// 75: a withdrawal touching L or U is rejected.
#[test]
fn t75_withdrawal_never_touches_l_or_u() {
    let mut s = one_pool(1);
    s.fund(SOL).unwrap(); // Z = 1 SOL, U = 10 SOL
    finalize_at(&mut s, 1, 1_500, 4 * SOL, false, 1_500); // L = 4, U = 6
    assert_eq!(s.vault, 11 * SOL);
    assert_eq!(
        s.withdraw(SOL + 1),
        Err(EngineError::ReserveWithdrawalExceedsReserve)
    );
    assert_eq!(
        s.withdraw(11 * SOL),
        Err(EngineError::ReserveWithdrawalExceedsReserve)
    );
    assert_eq!(s.withdraw(0), Err(EngineError::InvalidFundingAmount));
    s.withdraw(SOL).unwrap();
    assert_eq!(
        s.withdraw(1),
        Err(EngineError::ReserveWithdrawalExceedsReserve)
    );
    assert_eq!(
        (s.ledger.liabilities, s.ledger.committed),
        (4 * SOL, 6 * SOL)
    );
}

/// 76: exhausted future funding does not invalidate existing commitments.
#[test]
fn t76_exhausted_funding_leaves_commitments_intact() {
    let mut s = Sim::new(2);
    s.fund(4 * SOL).unwrap();
    s.commit(2 * SOL, 100, 200, 0).unwrap();
    s.commit(2 * SOL, 300, 400, 0).unwrap();
    // No further funding: Z = 0; a third tranche cannot commit.
    assert_eq!(s.ledger.reserve, 0);
    assert_eq!(
        s.commit(1, 500, 600, 0),
        Err(EngineError::InsufficientFruitReserve)
    );
    // Both committed tranches run to completion and pay in full.
    for i in 0..2 {
        s.post(i, 1, 400, 2 * SOL, true, 1_000).unwrap();
        s.finalize(i, 1_000 + WINDOW).unwrap();
    }
    assert_eq!(s.claim(0, 0, SOL * 2 / 5).unwrap(), SOL * 2 / 5);
    assert_eq!(s.claim(1, 1, SOL * 2 / 5).unwrap(), SOL * 2 / 5);
}

// ---------------------------------------------------------------- property

#[derive(Debug, Clone)]
enum Op {
    Fund(u64),
    Withdraw(u64),
    Commit(u64, i64, i64),
    Post(usize, u64, i64, u64, bool),
    Veto(usize, u8),
    Finalize(usize),
    Claim(usize, usize, u64),
    Close(usize),
    Terminal(usize, u64, u64),
    Warp(i64),
}

fn op() -> impl Strategy<Value = Op> {
    let amt = 0u64..(20 * SOL);
    prop_oneof![
        2 => amt.clone().prop_map(Op::Fund),
        1 => amt.clone().prop_map(Op::Withdraw),
        2 => (amt.clone(), 0i64..5_000, 1i64..5_000).prop_map(|(e, s, d)| Op::Commit(e, s, d)),
        4 => (0usize..4, 0u64..12, 0i64..12_000, amt.clone(), any::<bool>())
            .prop_map(|(i, v, c, t, done)| Op::Post(i, v, c, t, done)),
        1 => (0usize..4, 1u8..3).prop_map(|(i, th)| Op::Veto(i, th)),
        3 => (0usize..4).prop_map(Op::Finalize),
        4 => (0usize..4, 0usize..4, amt.clone()).prop_map(|(i, w, c)| Op::Claim(i, w, c)),
        1 => (0usize..4).prop_map(Op::Close),
        1 => (0usize..4, 0u64..12, amt).prop_map(|(i, v, t)| Op::Terminal(i, v, t)),
        3 => (0i64..3_000).prop_map(Op::Warp),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Random transition sequences never break the §10a invariants; every
    /// rejected transition leaves the state bit-identical.
    #[test]
    fn invariants_hold_after_every_transition(ops in prop::collection::vec(op(), 1..120)) {
        let mut s = Sim::new(4);
        let mut now = 0i64;
        for o in ops {
            let before = (s.vault, s.ledger, s.pools.clone());
            let ok = match o {
                Op::Fund(a) => s.fund(a).is_ok(),
                Op::Withdraw(a) => s.withdraw(a).is_ok(),
                Op::Commit(e, st, d) => s.commit(e, now + st, now + st + d, now).is_ok(),
                Op::Post(i, v, c, t, done) if i < s.pools.len() => {
                    // Keep totals in a reachable range so roots often succeed.
                    let t = t % (s.pool(i).budget + 1);
                    s.post(i, v, c.min(now), t, done, now).is_ok()
                }
                Op::Veto(i, th) if i < s.pools.len() => s.veto(i, th, now).is_ok(),
                Op::Finalize(i) if i < s.pools.len() => s.finalize(i, now).is_ok(),
                Op::Claim(i, w, c) if i < s.pools.len() => {
                    let c = c % (s.pool(i).wallet_cap + 2);
                    s.claim(i, w, c).is_ok()
                }
                Op::Close(i) if i < s.pools.len() => s.close(i, now).is_ok(),
                Op::Terminal(i, v, t) if i < s.pools.len() => {
                    let t = t % (s.pool(i).budget + 1);
                    let cp = now.max(s.pool(i).t_end);
                    s.terminal(i, v, cp.min(now), t, now).is_ok()
                }
                Op::Warp(d) => { now += d; true }
                _ => true,
            };
            if !ok {
                prop_assert_eq!(s.vault, before.0);
                prop_assert_eq!(s.ledger, before.1);
                prop_assert_eq!(&s.pools, &before.2);
            }
            s.check();
        }
    }
}

// ---------------------------------------------------- lock forfeit (89)

/// Test 89: at lock-pool close `floor(0.80 forfeited)` leaves `Z` to the
/// Protocol Treasury and the remainder stays in `Z`; unallocated and
/// safety-cancelled capacity go 100% to `Z`; `L` and `U` are untouched by
/// the release; a root claiming more forfeiture than it releases is
/// rejected.
#[test]
fn lock_forfeit_releases_eighty_percent_to_treasury() {
    let (t0, t1) = (1_000, 1_000 + 5 * 86_400);
    let mut sim = Sim::new(3);
    sim.fund(10 * SOL).unwrap();
    let i = sim.commit(10 * SOL, t0, t1, 0).unwrap();
    let e = 10 * SOL;
    let total = 6 * SOL; // established, vested
    let released = e - total; // 4 SOL: unallocated + forfeited + other

    // forfeited above the released capacity: rejected at post.
    assert_eq!(
        sim.post_forfeit(i, 1, t1, total, true, released + 1, t1),
        Err(EngineError::InvalidForfeited)
    );
    // forfeited on an incomplete root: rejected.
    assert_eq!(
        sim.post_forfeit(i, 1, t1 - 1, total, false, 1, t1),
        Err(EngineError::InvalidForfeited)
    );

    let forfeited = SOL + 7; // odd lamports exercise the floor
    sim.post_forfeit(i, 1, t1, total, true, forfeited, t1)
        .unwrap();
    sim.finalize(i, t1 + WINDOW).unwrap();
    let before = sim.ledger;
    let vault_before = sim.vault;
    let out = sim.close_full(i, t1 + WINDOW).unwrap();
    let treasury = forfeited * 8 / 10;
    assert_eq!(out.released, released);
    assert_eq!(out.treasury_release, treasury);
    assert_eq!(lock_forfeit_treasury_release(forfeited), treasury);
    assert_eq!(sim.ledger.liabilities, before.liabilities, "L untouched");
    assert_eq!(sim.ledger.committed, before.committed - released);
    assert_eq!(
        sim.ledger.reserve,
        before.reserve + released - treasury,
        "20% of forfeited, and all unallocated/other capacity, stays in Z"
    );
    assert_eq!(sim.vault, vault_before - treasury);
    assert_eq!(sim.pools[i].0.forfeit_treasury_released, treasury);
    // Exactly once.
    assert_eq!(sim.close(i, t1 + WINDOW), Err(EngineError::FruitPoolClosed));
}

#[test]
fn lock_forfeit_edge_cases() {
    let (t0, t1) = (1_000, 1_000 + 5 * 86_400);
    // forfeited == released: 80% of all released capacity leaves.
    let mut sim = Sim::new(1);
    sim.fund(5 * SOL).unwrap();
    let i = sim.commit(5 * SOL, t0, t1, 0).unwrap();
    sim.post_forfeit(i, 1, t1, 0, true, 5 * SOL, t1).unwrap();
    sim.finalize(i, t1 + WINDOW).unwrap();
    let out = sim.close_full(i, t1 + WINDOW).unwrap();
    assert_eq!(out.treasury_release, 4 * SOL);
    assert_eq!(sim.ledger.reserve, SOL);

    // Nobody forfeits: 100% of released capacity stays in Z.
    let mut sim = Sim::new(1);
    sim.fund(5 * SOL).unwrap();
    let i = sim.commit(5 * SOL, t0, t1, 0).unwrap();
    sim.post(i, 1, t1, SOL, true, t1).unwrap();
    sim.finalize(i, t1 + WINDOW).unwrap();
    let out = sim.close_full(i, t1 + WINDOW).unwrap();
    assert_eq!(out.treasury_release, 0);
    assert_eq!(sim.ledger.reserve, 4 * SOL);

    // A vetoed forfeit root leaves nothing behind; a later complete root
    // without forfeiture releases nothing to the treasury.
    let mut sim = Sim::new(1);
    sim.fund(5 * SOL).unwrap();
    let i = sim.commit(5 * SOL, t0, t1, 0).unwrap();
    sim.post_forfeit(i, 1, t1, SOL, true, 2 * SOL, t1).unwrap();
    assert!(sim.veto(i, 1, t1).unwrap());
    assert_eq!(sim.pools[i].0.pending_forfeited, 0);
    sim.post(i, 2, t1, SOL, true, t1).unwrap();
    sim.finalize(i, t1 + WINDOW).unwrap();
    assert_eq!(sim.close_full(i, t1 + WINDOW).unwrap().treasury_release, 0);

    // Terminal resolution never carries forfeiture.
    let mut sim = Sim::new(1);
    sim.fund(5 * SOL).unwrap();
    let i = sim.commit(5 * SOL, t0, t1, 0).unwrap();
    sim.terminal(i, 1, t1, SOL, t1).unwrap();
    assert_eq!(sim.close_full(i, t1).unwrap().treasury_release, 0);

    // A superseding complete root replaces the carried forfeiture.
    let mut sim = Sim::new(1);
    sim.fund(5 * SOL).unwrap();
    let i = sim.commit(5 * SOL, t0, t1, 0).unwrap();
    sim.post_forfeit(i, 1, t1, SOL, true, 3 * SOL, t1).unwrap();
    sim.finalize(i, t1 + WINDOW).unwrap();
    sim.post_forfeit(i, 2, t1, 2 * SOL, true, SOL, t1 + WINDOW)
        .unwrap();
    sim.finalize(i, t1 + 2 * WINDOW).unwrap();
    let out = sim.close_full(i, t1 + 2 * WINDOW).unwrap();
    assert_eq!(out.released, 3 * SOL);
    assert_eq!(out.treasury_release, SOL * 8 / 10);
}
