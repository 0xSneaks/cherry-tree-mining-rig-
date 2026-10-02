//! Lot bookkeeping for the valuation rule of section 5 (test 30):
//!
//! * units held at the epoch reference timestamp are valued at the committed
//!   reference price;
//! * units **added or registered after it** are valued at
//!   `min(reference, verified price at the addition event)`, so weight can
//!   never be bought at a stale pre-drop price;
//! * an emergency NAV reset (section 9) re-bases *every* lot of the asset to
//!   the verified NAV, prospectively only (the replay applies it from its
//!   event, test 36).
//!
//! Lots are FIFO **by acquisition age**: a monotone stamp on every lot,
//! kept ordered across the active and the pending-unstake buckets — and
//! across every key the units can later travel to (other accounts of the
//! same wallet, stake beneficiaries, checkpoint adoptions). Age is
//! therefore GLOBAL, and merging is allowed only between fragments of
//! ONE acquisition (equal stamps): folding a younger lot into an older
//! same-price lot would re-stamp its units with the older age, and any
//! lot of intermediate age living under a different key would later be
//! consumed behind them (NEW-4). Distinct acquisitions never merge; lot
//! vectors grow one entry per acquisition instead.
//! Consumption order cannot create value: under any order the total is
//! the sum of the lots, and every lot's price is already capped by
//! the min rule at its acquisition; age order additionally fixes WHICH
//! capped price applies when units are partially consumed, so no
//! unstake/cancel sequence can front-run an expensive lot with a cheap
//! one. `price == None` marks a unit whose
//! addition price could not be verified (no usable observation): its holder
//! is PENDING (reserved), never silently reference-priced.
//!
//! Registration pricing applies **exactly once**, at the holder's first
//! registration event (or at a verified reset, whose published price is
//! authoritative from its event). A later registration event never reprices
//! or caps already-established lots — in either direction and never
//! retroactively: each lot's price is fixed by the event that acquired or
//! registered it.
//!
//! Pending unstake units sit in a parallel bucket per beneficiary and are
//! excluded from valuation exactly like the ledger excludes them from
//! recognition.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{EligibilityError, EligibilityResult};
use crate::types::Addr;
use crate::valuation::UnitPrice;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LotBasis {
    /// Held at the reference timestamp (or re-based by a verified reset).
    Reference,
    /// Acquired after the reference timestamp.
    Addition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lot {
    pub units: u64,
    /// `None` = addition price unverified: fail closed (PENDING).
    pub price: Option<UnitPrice>,
    pub basis: LotBasis,
    /// Acquisition-order stamp (monotone across the ledger): FIFO age is
    /// totally ordered across the active and pending buckets, so lots
    /// returning from the pending bucket re-enter at their age position
    /// instead of a bucket end.
    pub seq: u64,
}

/// Valuation of the first `n` units of a holder (FIFO).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadValue {
    pub lamports: u128,
    /// Some unpriced lot lies within the first `n` units.
    pub unpriced: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LotLedger {
    /// `(mint, holder)` → lots; holder is a token account or a stake
    /// beneficiary.
    lots: BTreeMap<(Addr, Addr), Vec<Lot>>,
    /// `(mint, beneficiary)` → pending-unstake lots (never valued).
    pending: BTreeMap<(Addr, Addr), Vec<Lot>>,
    /// `(mint, holder)` pairs whose registration pricing is already
    /// anchored: applied at their FIRST registration event (or at a
    /// verified reset). Later registration events never touch them again.
    /// The anchor belongs to the owning wallet: an account-owner transfer
    /// clears it ([`LotLedger::clear_registered`]) so the recipient's own
    /// first registration applies.
    registered: BTreeSet<(Addr, Addr)>,
    /// Next acquisition-order stamp (see [`Lot::seq`]).
    next_seq: u64,
}

impl LotLedger {
    fn vec_mut(&mut self, key: (Addr, Addr)) -> &mut Vec<Lot> {
        self.lots.entry(key).or_default()
    }

    /// Fresh acquisition-order stamp.
    fn fresh_seq(&mut self) -> u64 {
        let s = self.next_seq;
        self.next_seq += 1;
        s
    }

    /// Append a lot as its own acquisition. Distinct acquisitions never
    /// merge: the fresh stamp is unique, and folding the new units into
    /// an older same-price lot would re-stamp them with that older age —
    /// corrupting the GLOBAL FIFO order once lots later co-locate across
    /// keys and buckets (another account of the same wallet, a stake
    /// beneficiary, a checkpoint adoption; NEW-4). Vectors grow one entry
    /// per acquisition; equal-stamp fragments may still recombine, but
    /// only where stamps provably match (see [`coalesce_fragments`]).
    pub fn credit(
        &mut self,
        mint: Addr,
        holder: Addr,
        units: u64,
        price: Option<UnitPrice>,
        basis: LotBasis,
    ) -> EligibilityResult<()> {
        let seq = self.fresh_seq();
        self.vec_mut((mint, holder)).push(Lot {
            units,
            price,
            basis,
            seq,
        });
        Ok(())
    }

    /// Remove `units` FIFO. Never removes more than present.
    pub fn consume(&mut self, mint: Addr, holder: Addr, units: u64) -> EligibilityResult<()> {
        let key = (mint, holder);
        let mut left = units;
        let v = self.vec_mut(key);
        let mut idx = 0;
        while left > 0 && idx < v.len() {
            if v[idx].units <= left {
                left -= v[idx].units;
                idx += 1;
            } else {
                v[idx].units -= left;
                left = 0;
            }
        }
        v.drain(..idx);
        if left > 0 {
            return Err(EligibilityError::InvariantViolation("consume above lots"));
        }
        if v.is_empty() {
            self.lots.remove(&key);
        }
        Ok(())
    }

    /// Valuation of the first `n` units (FIFO, per-lot floor rounding).
    pub fn head_value(&self, mint: Addr, holder: Addr, n: u64) -> EligibilityResult<HeadValue> {
        let mut left = n;
        let mut lamports: u128 = 0;
        let mut unpriced = false;
        for lot in self.lots.get(&(mint, holder)).into_iter().flatten() {
            if left == 0 {
                break;
            }
            let take = left.min(lot.units);
            left -= take;
            match lot.price {
                None => unpriced = true,
                Some(p) => {
                    lamports = lot_value_add(lamports, p, take)?;
                }
            }
        }
        Ok(HeadValue { lamports, unpriced })
    }

    /// Total units held (any basis, any price state).
    pub fn units(&self, mint: Addr, holder: Addr) -> u64 {
        self.lots
            .get(&(mint, holder))
            .into_iter()
            .flatten()
            .map(|l| l.units)
            .fold(0u64, |a, b| a.saturating_add(b))
    }

    /// Whether the holder's registration pricing is already anchored.
    pub fn is_registered(&self, mint: Addr, holder: Addr) -> bool {
        self.registered.contains(&(mint, holder))
    }

    /// Anchor `holders` of `mint` as registration-priced without rebasing:
    /// used when the first registration lands at or before the reference
    /// timestamp (their reference pricing is legitimate) so that no LATER
    /// registration event can reprice them.
    pub fn mark_registered(&mut self, mint: Addr, holders: &[Addr]) {
        for h in holders {
            self.registered.insert((mint, *h));
        }
    }

    /// Remove a holder's registration anchor. The anchor belongs to the
    /// owning wallet's first registration, so it must not survive an
    /// account-owner transfer: `SetAccountOwner` moves the whole account
    /// to a different wallet, and the recipient re-anchors only if it is
    /// itself already registered (the replay's credit path); otherwise the
    /// recipient's own first registration applies the boundary price.
    pub fn clear_registered(&mut self, mint: Addr, holder: Addr) {
        self.registered.remove(&(mint, holder));
    }

    /// Erase every trace of `account` as a lot-holder key: its
    /// registration anchor and any residual lots, whatever the mint.
    /// The token program closes only EMPTY accounts, so the anchor is the
    /// state that can leak across the boundary: a re-initialized address
    /// must start with no pricing history, or its next owner would
    /// inherit the prior owner's registration anchor (F1-A, close +
    /// re-initialize).
    pub fn erase_account(&mut self, account: Addr) {
        self.registered.retain(|(_, holder)| *holder != account);
        self.lots.retain(|(_, holder), _| *holder != account);
    }

    /// Registration after the reference timestamp (section 5): reference
    /// lots of these holders drop to `price`; addition lots keep their own
    /// event price; unpriced lots stay unpriced. `price == None` (no usable
    /// observation at registration) taints reference lots too.
    ///
    /// Applied **once per holder**: a holder already anchored at an earlier
    /// registration event (or a verified reset) is skipped — a later
    /// registration event never reprices or caps established lots.
    /// Reference lots sitting in the holder's pending-unstake bucket rebase
    /// equally, so units that later return via `CancelUnstake` carry the
    /// registration pricing instead of the pre-registration one.
    pub fn rebase_registered(
        &mut self,
        mint: Addr,
        holders: &[Addr],
        price: Option<UnitPrice>,
    ) -> EligibilityResult<()> {
        for h in holders {
            let key = (mint, *h);
            if !self.registered.insert(key) {
                continue; // priced at its first registration event already
            }
            for bucket in [&mut self.lots, &mut self.pending] {
                if let Some(v) = bucket.get_mut(&key) {
                    for lot in v.iter_mut() {
                        if lot.basis == LotBasis::Reference {
                            lot.price = price;
                        }
                    }
                }
            }
            // Recombine adjacent fragments of ONE acquisition only
            // (equal stamps): repricing can make neighbouring lots
            // identical in basis and price, but merging across DISTINCT
            // stamps would re-stamp the younger units with the older
            // age — and an intermediate-age lot under any other key
            // would later be consumed behind them (F1-C, NEW-4).
            if let Some(v) = self.lots.get_mut(&key) {
                coalesce_fragments(v);
            }
            if let Some(v) = self.pending.get_mut(&key) {
                coalesce_fragments(v);
            }
        }
        Ok(())
    }

    /// Verified emergency NAV / published recovery reference: every lot of
    /// the mint re-bases to `price` (settled intervals are untouched because
    /// the replay applies this only from its event). The reset price is
    /// authoritative from its event, so every touched holder is anchored: a
    /// later registration event never reprices reset-priced lots.
    pub fn rebase_mint(&mut self, mint: Addr, price: UnitPrice) -> EligibilityResult<()> {
        let keys: Vec<(Addr, Addr)> = self
            .lots
            .keys()
            .chain(self.pending.keys())
            .filter(|(m, _)| *m == mint)
            .cloned()
            .collect();
        for key in keys {
            let seq = self.fresh_seq();
            let collapse = |v: &mut Vec<Lot>| -> EligibilityResult<()> {
                let mut total = 0u64;
                for l in v.iter() {
                    total = total
                        .checked_add(l.units)
                        .ok_or(EligibilityError::MathOverflow)?;
                }
                *v = vec![Lot {
                    units: total,
                    price: Some(price),
                    basis: LotBasis::Reference,
                    seq,
                }];
                Ok(())
            };
            self.registered.insert(key);
            if let Some(v) = self.lots.get_mut(&key) {
                collapse(v)?;
            }
            if let Some(v) = self.pending.get_mut(&key) {
                collapse(v)?;
            }
        }
        Ok(())
    }

    /// Stake: units move from the wallet account into the beneficiary's
    /// active bucket, carrying their lots (and their age).
    pub fn stake(
        &mut self,
        mint: Addr,
        from_account: Addr,
        beneficiary: Addr,
        units: u64,
    ) -> EligibilityResult<()> {
        let moved = self.split_off(mint, from_account, units)?;
        for lot in moved {
            insert_seq(self.vec_mut((mint, beneficiary)), lot);
        }
        Ok(())
    }

    /// RequestUnstake: active → pending bucket (excluded from valuation).
    /// The moved lots keep their acquisition age and insert into the
    /// pending bucket at that position, so pending stays age-ordered like
    /// the active bucket.
    pub fn request_unstake(
        &mut self,
        mint: Addr,
        beneficiary: Addr,
        units: u64,
    ) -> EligibilityResult<()> {
        let moved = self.split_off(mint, beneficiary, units)?;
        let v = self.pending.entry((mint, beneficiary)).or_default();
        for lot in moved {
            let idx = v.partition_point(|l| l.seq <= lot.seq);
            v.insert(idx, lot);
        }
        Ok(())
    }

    /// CancelUnstake: pending → active. The pending bucket is consumed
    /// FIFO (oldest lots first, never a reordered swap), and each returned
    /// lot re-enters the active bucket at its acquisition-age position:
    /// canonical FIFO holds across both buckets, so no cancellation
    /// ordering can promote a newer cheap lot in front of an older
    /// expensive one (or the reverse).
    pub fn cancel_unstake(
        &mut self,
        mint: Addr,
        beneficiary: Addr,
        units: u64,
    ) -> EligibilityResult<()> {
        let key = (mint, beneficiary);
        let mut left = units;
        let v = self.pending.entry(key).or_default();
        let mut taken = Vec::new();
        let mut idx = 0;
        while left > 0 && idx < v.len() {
            if v[idx].units <= left {
                left -= v[idx].units;
                taken.push(v[idx]);
                idx += 1;
            } else {
                v[idx].units -= left;
                taken.push(Lot {
                    units: left,
                    ..v[idx]
                });
                left = 0;
            }
        }
        v.drain(..idx);
        if v.is_empty() {
            self.pending.remove(&key);
        }
        if left > 0 {
            return Err(EligibilityError::InvariantViolation("cancel above pending"));
        }
        for lot in taken {
            insert_seq(self.vec_mut(key), lot);
        }
        Ok(())
    }

    /// WithdrawUnstaked: consume from the pending bucket.
    pub fn withdraw_unstaked(
        &mut self,
        mint: Addr,
        beneficiary: Addr,
        units: u64,
    ) -> EligibilityResult<()> {
        let key = (mint, beneficiary);
        let mut left = units;
        let v = self.pending.entry(key).or_default();
        let mut idx = 0;
        while left > 0 && idx < v.len() {
            if v[idx].units <= left {
                left -= v[idx].units;
                idx += 1;
            } else {
                v[idx].units -= left;
                left = 0;
            }
        }
        v.drain(..idx);
        if v.is_empty() {
            self.pending.remove(&key);
        }
        if left > 0 {
            return Err(EligibilityError::InvariantViolation(
                "withdraw above pending",
            ));
        }
        Ok(())
    }

    /// Total units in a beneficiary's pending-unstake bucket.
    pub fn pending_units(&self, mint: Addr, holder: Addr) -> u64 {
        self.pending
            .get(&(mint, holder))
            .into_iter()
            .flatten()
            .map(|l| l.units)
            .fold(0u64, |a, b| a.saturating_add(b))
    }

    /// Credit units straight into the pending-unstake bucket (checkpoint
    /// adoption of pending balances the replay did not observe). The
    /// acquisition basis travels with the lot: a pre-reference adoption is
    /// a `Reference` lot that a late first registration reprices exactly
    /// like an observed one; a post-reference adoption is an `Addition`
    /// at the adoption's min-rule price (possibly unpriced: fail closed).
    /// Like [`LotLedger::credit`], a fresh acquisition never merges: its
    /// stamp is unique, and merging would destroy its age (NEW-4).
    pub fn credit_pending(
        &mut self,
        mint: Addr,
        holder: Addr,
        units: u64,
        price: Option<UnitPrice>,
        basis: LotBasis,
    ) -> EligibilityResult<()> {
        let seq = self.fresh_seq();
        self.pending.entry((mint, holder)).or_default().push(Lot {
            units,
            price,
            basis,
            seq,
        });
        Ok(())
    }

    /// Consume units from the pending-unstake bucket FIFO.
    pub fn consume_pending(
        &mut self,
        mint: Addr,
        holder: Addr,
        units: u64,
    ) -> EligibilityResult<()> {
        let key = (mint, holder);
        let mut left = units;
        let v = self.pending.entry(key).or_default();
        let mut idx = 0;
        while left > 0 && idx < v.len() {
            if v[idx].units <= left {
                left -= v[idx].units;
                idx += 1;
            } else {
                v[idx].units -= left;
                left = 0;
            }
        }
        v.drain(..idx);
        if v.is_empty() {
            self.pending.remove(&key);
        }
        if left > 0 {
            return Err(EligibilityError::InvariantViolation(
                "consume above pending",
            ));
        }
        Ok(())
    }

    /// FIFO-split `units` out of a holder's lots, returning the removed lots
    /// (partial lot split into two).
    fn split_off(&mut self, mint: Addr, holder: Addr, units: u64) -> EligibilityResult<Vec<Lot>> {
        let key = (mint, holder);
        let mut left = units;
        let mut out = Vec::new();
        let v = self.vec_mut(key);
        while left > 0 {
            let Some(front) = v.first_mut() else {
                return Err(EligibilityError::InvariantViolation("split above lots"));
            };
            if front.units <= left {
                left -= front.units;
                out.push(v.remove(0));
            } else {
                front.units -= left;
                out.push(Lot {
                    units: left,
                    ..*front
                });
                left = 0;
            }
        }
        if v.is_empty() {
            self.lots.remove(&key);
        }
        Ok(out)
    }
}

fn lot_value_add(sum: u128, p: UnitPrice, units: u64) -> EligibilityResult<u128> {
    let v = (units as u128)
        .checked_mul(p.lamports_num)
        .ok_or(EligibilityError::MathOverflow)?
        / p.units_den;
    sum.checked_add(v).ok_or(EligibilityError::MathOverflow)
}

/// Insert `lot` into `v` keeping the vector ordered by acquisition `seq`
/// (stable: equal stamps append after their equals). Both buckets stay
/// age-ordered, so FIFO consumption and valuation are canonical across
/// pending-unstake round trips.
fn insert_seq(v: &mut Vec<Lot>, lot: Lot) {
    let idx = v.partition_point(|l| l.seq <= lot.seq);
    v.insert(idx, lot);
}

/// Recombine adjacent fragments of ONE acquisition: entries merge only
/// when their stamps are EQUAL (splits of a single lot, e.g. the two
/// halves of a partial unstake/cancel round trip), so no merge can ever
/// re-stamp units with a different acquisition age — nothing can sit
/// strictly between two equal stamps. Distinct acquisitions (different
/// stamps) never merge, even when identical in basis and price: their
/// age difference is globally FIFO-relevant because lots travel between
/// keys and buckets (NEW-4).
fn coalesce_fragments(v: &mut Vec<Lot>) {
    let mut out: Vec<Lot> = Vec::with_capacity(v.len());
    for lot in v.iter() {
        if let Some(last) = out.last_mut() {
            if last.seq == lot.seq && last.basis == lot.basis && last.price == lot.price {
                last.units = last.units.saturating_add(lot.units);
                continue;
            }
        }
        out.push(*lot);
    }
    *v = out;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::valuation::PriceBasis;

    const MINT: Addr = Addr::fixture(1);
    const W: Addr = Addr::fixture(2);
    const CUSTODY: Addr = Addr::fixture(3);

    fn p(n: u128) -> Option<UnitPrice> {
        Some(UnitPrice {
            lamports_num: n,
            units_den: 1,
            basis: PriceBasis::Reference,
        })
    }

    #[test]
    fn fifo_valuation_and_min_rule_bound() {
        let mut l = LotLedger::default();
        l.credit(MINT, W, 100, p(2), LotBasis::Reference).unwrap();
        l.credit(MINT, W, 100, p(1), LotBasis::Addition).unwrap();
        // First 100 units at reference 2, next 100 at the addition price 1.
        let h = l.head_value(MINT, W, 100).unwrap();
        assert_eq!(h.lamports, 200);
        let h = l.head_value(MINT, W, 200).unwrap();
        assert_eq!(h.lamports, 300);
        // Consuming removes the reference lot first (FIFO).
        l.consume(MINT, W, 100).unwrap();
        assert_eq!(l.units(MINT, W), 100);
        assert_eq!(l.head_value(MINT, W, 100).unwrap().lamports, 100);
    }

    #[test]
    fn unpriced_lot_fails_closed() {
        let mut l = LotLedger::default();
        l.credit(MINT, W, 50, p(2), LotBasis::Reference).unwrap();
        l.credit(MINT, W, 50, None, LotBasis::Addition).unwrap();
        let h = l.head_value(MINT, W, 100).unwrap();
        assert!(h.unpriced);
        // Within the priced prefix only: still priced.
        assert!(!l.head_value(MINT, W, 50).unwrap().unpriced);
    }

    #[test]
    fn emergency_rebase_rewrites_every_lot_including_pending() {
        let mut l = LotLedger::default();
        l.credit(MINT, W, 70, p(2), LotBasis::Reference).unwrap();
        l.credit(MINT, W, 30, p(1), LotBasis::Addition).unwrap();
        l.request_unstake(MINT, W, 40).unwrap();
        l.rebase_mint(MINT, p(1).unwrap()).unwrap();
        assert_eq!(l.head_value(MINT, W, 60).unwrap().lamports, 60);
        l.cancel_unstake(MINT, W, 40).unwrap();
        assert_eq!(l.head_value(MINT, W, 100).unwrap().lamports, 100);
        assert!(!l.head_value(MINT, W, 100).unwrap().unpriced);
    }

    #[test]
    fn registration_rebase_only_touches_reference_lots() {
        let mut l = LotLedger::default();
        l.credit(MINT, W, 100, p(2), LotBasis::Reference).unwrap();
        l.credit(MINT, W, 100, p(1), LotBasis::Addition).unwrap();
        l.rebase_registered(MINT, &[W], p(1)).unwrap();
        assert_eq!(l.head_value(MINT, W, 200).unwrap().lamports, 200);
        // A second registration event for the SAME holder is a no-op: the
        // lots were priced at their first registration event and are never
        // repriced (up, down or un-tainted) by a later one.
        l.rebase_registered(MINT, &[W], None).unwrap();
        l.rebase_registered(MINT, &[W], p(9)).unwrap();
        let h = l.head_value(MINT, W, 200).unwrap();
        assert!(!h.unpriced);
        assert_eq!(h.lamports, 200);
        // An unmarked holder still gets its own registration pricing.
        l.credit(MINT, CUSTODY, 100, p(2), LotBasis::Reference)
            .unwrap();
        l.rebase_registered(MINT, &[CUSTODY], None).unwrap();
        assert!(l.head_value(MINT, CUSTODY, 100).unwrap().unpriced);
    }

    #[test]
    fn pre_reference_anchor_blocks_later_registration_rebase() {
        let mut l = LotLedger::default();
        l.credit(MINT, W, 100, p(2), LotBasis::Reference).unwrap();
        // Registered at/before the reference timestamp: anchored, reference
        // pricing stands.
        l.mark_registered(MINT, &[W]);
        l.rebase_registered(MINT, &[W], p(1)).unwrap();
        assert_eq!(l.head_value(MINT, W, 100).unwrap().lamports, 200);
    }

    #[test]
    fn reset_price_is_authoritative_and_anchors_holders() {
        let mut l = LotLedger::default();
        l.credit(MINT, W, 70, p(2), LotBasis::Reference).unwrap();
        l.credit(MINT, W, 30, p(1), LotBasis::Addition).unwrap();
        l.rebase_mint(MINT, p(1).unwrap()).unwrap();
        assert_eq!(l.head_value(MINT, W, 100).unwrap().lamports, 100);
        // A later registration event must not reprice reset-priced lots.
        l.rebase_registered(MINT, &[W], p(0)).unwrap();
        l.rebase_registered(MINT, &[W], None).unwrap();
        let h = l.head_value(MINT, W, 100).unwrap();
        assert!(!h.unpriced);
        assert_eq!(h.lamports, 100);
    }

    #[test]
    fn stake_moves_carry_lots_between_holders() {
        let mut l = LotLedger::default();
        l.credit(MINT, CUSTODY, 100, p(2), LotBasis::Reference)
            .unwrap();
        l.stake(MINT, CUSTODY, W, 60).unwrap();
        assert_eq!(l.units(MINT, CUSTODY), 40);
        assert_eq!(l.head_value(MINT, W, 60).unwrap().lamports, 120);
    }

    // ------------------------------------- Pass #5 second remediation

    /// F1-C: partial cancellation of differently priced pending lots
    /// consumes them FIFO (oldest first) and re-seats the returned lots at
    /// their age position in the active bucket.
    #[test]
    fn cancel_unstake_is_fifo_and_age_ordered() {
        let mut l = LotLedger::default();
        l.credit(MINT, W, 10, p(1), LotBasis::Reference).unwrap();
        l.credit(MINT, W, 10, p(2), LotBasis::Addition).unwrap();
        l.credit(MINT, W, 10, p(3), LotBasis::Addition).unwrap();
        l.request_unstake(MINT, W, 30).unwrap();
        assert_eq!(l.units(MINT, W), 0);
        assert_eq!(l.pending_units(MINT, W), 30);
        // Partial cancel of 15: the oldest lot (10@1) plus HALF of the
        // second (5@2) — never a slice of the newest lot.
        l.cancel_unstake(MINT, W, 15).unwrap();
        assert_eq!(l.pending_units(MINT, W), 15);
        assert_eq!(
            l.head_value(MINT, W, 15).unwrap().lamports,
            10 + 5 * 2,
            "partial cancellation returns the oldest pending lots first"
        );
        // Full cancel returns the remainder in age order; consuming from
        // the front still hits the oldest prices first.
        l.cancel_unstake(MINT, W, 15).unwrap();
        assert_eq!(l.pending_units(MINT, W), 0);
        assert_eq!(l.head_value(MINT, W, 30).unwrap().lamports, 10 + 20 + 30);
        l.consume(MINT, W, 15).unwrap();
        assert_eq!(l.head_value(MINT, W, 15).unwrap().lamports, 5 * 2 + 10 * 3);
    }

    /// F1-C adversarial: cancellation ordering must not let a holder burn
    /// newly-acquired cheap lots while old expensive lots stay recognized.
    #[test]
    fn cancel_returns_old_lots_in_front_of_new_acquisitions() {
        let mut l = LotLedger::default();
        l.credit(MINT, W, 100, p(2), LotBasis::Reference).unwrap();
        l.request_unstake(MINT, W, 100).unwrap();
        l.credit(MINT, W, 100, p(1), LotBasis::Addition).unwrap(); // cheap new acquisition
        l.cancel_unstake(MINT, W, 100).unwrap();
        // Consume removes the OLD expensive lot first: the remaining units
        // are the cheap ones, never the other way around.
        l.consume(MINT, W, 100).unwrap();
        assert_eq!(l.head_value(MINT, W, 100).unwrap().lamports, 100);
    }

    /// F1-B: checkpoint-adopted pending lots carry their acquisition
    /// BASIS: a pre-reference adoption is a Reference lot that a late
    /// first registration reprices exactly like an observed one.
    #[test]
    fn adopted_pending_lots_carry_their_basis_into_registration() {
        let mut l = LotLedger::default();
        l.credit_pending(MINT, W, 100, p(2), LotBasis::Reference)
            .unwrap();
        l.rebase_registered(MINT, &[W], p(1)).unwrap();
        l.cancel_unstake(MINT, W, 100).unwrap();
        assert_eq!(
            l.head_value(MINT, W, 100).unwrap().lamports,
            100,
            "the adopted pre-reference pending lot rebased at the late registration"
        );
    }

    // -------------------------------------- Pass #5 third remediation

    /// NEW-1 (F1-A close + re-initialize): erasing an account removes its
    /// registration anchor and residual lots, so a re-initialized address
    /// starts with no pricing history and the next owner's first
    /// registration applies normally.
    #[test]
    fn erase_account_removes_anchor_and_lots() {
        let mut l = LotLedger::default();
        l.credit(MINT, W, 100, p(2), LotBasis::Reference).unwrap();
        l.mark_registered(MINT, &[W]);
        l.erase_account(W);
        assert!(!l.is_registered(MINT, W));
        assert_eq!(l.units(MINT, W), 0);
        // A later first registration applies to the fresh history.
        l.credit(MINT, W, 100, p(2), LotBasis::Reference).unwrap();
        l.rebase_registered(MINT, &[W], p(1)).unwrap();
        assert_eq!(l.head_value(MINT, W, 100).unwrap().lamports, 100);
    }

    /// NEW-3 (F1-C through lot merging): the registration rebase must not
    /// merge same-price lots across an intermediate-age PENDING lot — a
    /// merge keeps the older stamp, so the younger half would jump ahead
    /// of the intermediate lot when the buckets reconcile.
    #[test]
    fn rebase_merge_cannot_erase_cross_bucket_age() {
        let mut l = LotLedger::default();
        l.credit(MINT, W, 100, p(1), LotBasis::Addition).unwrap(); // X oldest
        l.credit(MINT, W, 100, p(3), LotBasis::Addition).unwrap(); // Y middle
        l.credit(MINT, W, 100, p(1), LotBasis::Addition).unwrap(); // Z newest
        l.request_unstake(MINT, W, 200).unwrap(); // X, Y -> pending
        l.cancel_unstake(MINT, W, 100).unwrap(); // X returns: active = [X, Z]
        l.rebase_registered(MINT, &[W], p(1)).unwrap(); // must NOT merge X + Z
        l.cancel_unstake(MINT, W, 100).unwrap(); // Y returns between X and Z
        l.request_unstake(MINT, W, 200).unwrap(); // consumes X and Y
        assert_eq!(
            l.head_value(MINT, W, 100).unwrap().lamports,
            100,
            "the remaining position is the newest lot Z@1, never Y@3"
        );
    }

    /// NEW-3 sibling hazard: `credit` merging into the active tail must
    /// not stamp units with an age older than a PENDING lot (checkpoint
    /// adoption can leave intermediate-age lots in pending while the
    /// active bucket is being credited).
    #[test]
    fn credit_merge_cannot_jump_a_pending_lot() {
        let mut l = LotLedger::default();
        l.credit(MINT, W, 100, p(2), LotBasis::Addition).unwrap(); // active, oldest
        l.credit_pending(MINT, W, 100, p(3), LotBasis::Addition)
            .unwrap(); // pending, middle
        l.credit(MINT, W, 100, p(2), LotBasis::Addition).unwrap(); // must NOT merge into @2
        l.cancel_unstake(MINT, W, 100).unwrap(); // the @3 lot returns at its age
        l.consume(MINT, W, 100).unwrap(); // FIFO: the oldest @2 leaves
        assert_eq!(
            l.head_value(MINT, W, 100).unwrap().lamports,
            100 * 3,
            "the next-oldest units are the returned @3 lot, not a merged @2 tail"
        );
    }

    /// NEW-3 mirrored hazard: `credit_pending` merging into the pending
    /// tail must not stamp units older than an ACTIVE lot of
    /// intermediate age.
    #[test]
    fn credit_pending_merge_cannot_jump_an_active_lot() {
        let mut l = LotLedger::default();
        l.credit(MINT, W, 100, p(2), LotBasis::Addition).unwrap(); // X oldest
        l.request_unstake(MINT, W, 100).unwrap(); // X -> pending
        l.credit(MINT, W, 100, p(3), LotBasis::Addition).unwrap(); // Y active, middle
        l.credit_pending(MINT, W, 100, p(2), LotBasis::Addition)
            .unwrap(); // must NOT merge into X
        l.cancel_unstake(MINT, W, 200).unwrap(); // X and the new lot return at their ages
        l.consume(MINT, W, 100).unwrap(); // FIFO: X leaves first
        assert_eq!(
            l.head_value(MINT, W, 100).unwrap().lamports,
            100 * 3,
            "the next-oldest units are the active @3 lot, never a merged @2 tail"
        );
    }

    // ---------------------------------- Pass #5 fourth remediation (NEW-4)

    /// Deterministic xorshift generator (test-only): every trial is
    /// reproducible from its seed.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    /// NEW-4 reference model: a merge-free lot book. Every acquisition
    /// keeps its own stamp forever and nothing ever coalesces; every other
    /// operation mirrors [`LotLedger`] exactly. Because FIFO age is global
    /// (lots travel between keys: other accounts of the same wallet, stake
    /// beneficiaries), the production ledger preserves the model's
    /// structure bucket-for-bucket iff no operation ever re-stamps units
    /// with a different acquisition age.
    ///
    /// Scope (LotLedger-level; the event-level paths that reduce to these
    /// operations are covered by the `new4*` integration tests): fresh
    /// credits (MintTo / Transfer destination / SetAccountOwner /
    /// WithdrawUnstaked / checkpoint reconcile), pending credits
    /// (checkpoint adoption), FIFO consumption (Burn / transfer source /
    /// reconcile shrink), request/cancel unstake round trips, cross-key
    /// stake moves, and first-registration rebases. `rebase_mint`'s
    /// emergency collapse is out of scope by design: it first reprices
    /// EVERY lot of the mint to one price, which makes age order
    /// value-neutral for those units.
    #[derive(Default)]
    struct Model {
        active: BTreeMap<(Addr, Addr), Vec<Lot>>,
        pending: BTreeMap<(Addr, Addr), Vec<Lot>>,
        registered: BTreeSet<(Addr, Addr)>,
        next_seq: u64,
    }

    impl Model {
        fn fresh_seq(&mut self) -> u64 {
            let s = self.next_seq;
            self.next_seq += 1;
            s
        }

        fn insert_seq(v: &mut Vec<Lot>, lot: Lot) {
            let idx = v.partition_point(|l| l.seq <= lot.seq);
            v.insert(idx, lot);
        }

        fn split_off_active(&mut self, key: (Addr, Addr), units: u64) -> Vec<Lot> {
            let v = self.active.entry(key).or_default();
            let mut left = units;
            let mut out = Vec::new();
            while left > 0 {
                let Some(front) = v.first_mut() else {
                    break;
                };
                if front.units <= left {
                    left -= front.units;
                    out.push(v.remove(0));
                } else {
                    front.units -= left;
                    out.push(Lot {
                        units: left,
                        ..*front
                    });
                    left = 0;
                }
            }
            if v.is_empty() {
                self.active.remove(&key);
            }
            out
        }

        fn credit(
            &mut self,
            key: (Addr, Addr),
            units: u64,
            price: Option<UnitPrice>,
            basis: LotBasis,
        ) {
            let seq = self.fresh_seq();
            self.active.entry(key).or_default().push(Lot {
                units,
                price,
                basis,
                seq,
            });
        }

        fn credit_pending(
            &mut self,
            key: (Addr, Addr),
            units: u64,
            price: Option<UnitPrice>,
            basis: LotBasis,
        ) {
            let seq = self.fresh_seq();
            self.pending.entry(key).or_default().push(Lot {
                units,
                price,
                basis,
                seq,
            });
        }

        fn consume(&mut self, key: (Addr, Addr), units: u64) {
            let mut left = units;
            let v = self.active.entry(key).or_default();
            let mut idx = 0;
            while left > 0 && idx < v.len() {
                if v[idx].units <= left {
                    left -= v[idx].units;
                    idx += 1;
                } else {
                    v[idx].units -= left;
                    left = 0;
                }
            }
            v.drain(..idx);
            if v.is_empty() {
                self.active.remove(&key);
            }
        }

        fn consume_pending(&mut self, key: (Addr, Addr), units: u64) {
            let mut left = units;
            let v = self.pending.entry(key).or_default();
            let mut idx = 0;
            while left > 0 && idx < v.len() {
                if v[idx].units <= left {
                    left -= v[idx].units;
                    idx += 1;
                } else {
                    v[idx].units -= left;
                    left = 0;
                }
            }
            v.drain(..idx);
            if v.is_empty() {
                self.pending.remove(&key);
            }
        }

        fn request(&mut self, key: (Addr, Addr), units: u64) {
            let moved = self.split_off_active(key, units);
            let v = self.pending.entry(key).or_default();
            for lot in moved {
                let idx = v.partition_point(|l| l.seq <= lot.seq);
                v.insert(idx, lot);
            }
        }

        fn cancel(&mut self, key: (Addr, Addr), units: u64) {
            let mut left = units;
            let v = self.pending.entry(key).or_default();
            let mut taken = Vec::new();
            let mut idx = 0;
            while left > 0 && idx < v.len() {
                if v[idx].units <= left {
                    left -= v[idx].units;
                    taken.push(v[idx]);
                    idx += 1;
                } else {
                    v[idx].units -= left;
                    taken.push(Lot {
                        units: left,
                        ..v[idx]
                    });
                    left = 0;
                }
            }
            v.drain(..idx);
            if v.is_empty() {
                self.pending.remove(&key);
            }
            for lot in taken {
                Self::insert_seq(self.active.entry(key).or_default(), lot);
            }
        }

        fn move_lots(&mut self, src: (Addr, Addr), dst: (Addr, Addr), units: u64) {
            let moved = self.split_off_active(src, units);
            for lot in moved {
                Self::insert_seq(self.active.entry(dst).or_default(), lot);
            }
        }

        fn rebase(&mut self, key: (Addr, Addr), price: Option<UnitPrice>) {
            if !self.registered.insert(key) {
                return;
            }
            for bucket in [&mut self.active, &mut self.pending] {
                if let Some(v) = bucket.get_mut(&key) {
                    for lot in v.iter_mut() {
                        if lot.basis == LotBasis::Reference {
                            lot.price = price;
                        }
                    }
                }
            }
        }

        fn units(&self, key: &(Addr, Addr)) -> u64 {
            self.active
                .get(key)
                .into_iter()
                .flatten()
                .map(|l| l.units)
                .fold(0, u64::saturating_add)
        }

        fn pending_units(&self, key: &(Addr, Addr)) -> u64 {
            self.pending
                .get(key)
                .into_iter()
                .flatten()
                .map(|l| l.units)
                .fold(0, u64::saturating_add)
        }
    }

    /// A bucket reduced to its age profile: entries in stamp order, with
    /// only ADJACENT fragments of one acquisition (equal stamp AND equal
    /// price/basis) folded. Equal stamps with different prices CAN
    /// legitimately coexist: a registration rebase is per-key, so
    /// fragments of one lot split across two keys can reprice
    /// differently — they stay distinct entries, because folding them
    /// would destroy that difference.
    fn profile(v: &[Lot]) -> Vec<(u64, u64, Option<UnitPrice>, LotBasis)> {
        let mut out: Vec<(u64, u64, Option<UnitPrice>, LotBasis)> = Vec::new();
        for lot in v {
            if let Some(last) = out.last_mut() {
                if last.0 == lot.seq && last.2 == lot.price && last.3 == lot.basis {
                    last.1 = last.1.saturating_add(lot.units);
                    continue;
                }
            }
            out.push((lot.seq, lot.units, lot.price, lot.basis));
        }
        out
    }

    /// The production ledger must hold EXACTLY the merge-free model's
    /// acquisition-age structure — no re-stamping, anywhere, ever.
    fn assert_same_as_model(prod: &LotLedger, model: &Model, ctx: &str) {
        let mut keys: BTreeSet<(Addr, Addr)> = BTreeSet::new();
        keys.extend(prod.lots.keys().cloned());
        keys.extend(prod.pending.keys().cloned());
        keys.extend(model.active.keys().cloned());
        keys.extend(model.pending.keys().cloned());
        for key in keys {
            let pa = prod.lots.get(&key).map(|v| profile(v)).unwrap_or_default();
            let ma = model
                .active
                .get(&key)
                .map(|v| profile(v))
                .unwrap_or_default();
            assert_eq!(
                pa, ma,
                "{ctx}: active bucket of holder {:?} diverged from the \
                 merge-free model (a merge re-stamped units with a foreign age)",
                key.1
            );
            let pp = prod
                .pending
                .get(&key)
                .map(|v| profile(v))
                .unwrap_or_default();
            let mp = model
                .pending
                .get(&key)
                .map(|v| profile(v))
                .unwrap_or_default();
            assert_eq!(
                pp, mp,
                "{ctx}: pending bucket of holder {:?} diverged from the \
                 merge-free model (a merge re-stamped units with a foreign age)",
                key.1
            );
        }
    }

    /// Drain a production bucket unit-by-unit, recording WHICH priced lot
    /// each consumed unit came from (per-unit lamports; None-marked lots
    /// record the fail-closed flag).
    fn prod_drain(prod: &mut LotLedger, key: Addr) -> Vec<(u128, bool)> {
        let mut tape = Vec::new();
        while prod.units(MINT, key) > 0 {
            let h = prod.head_value(MINT, key, 1).unwrap();
            tape.push((h.lamports, h.unpriced));
            prod.consume(MINT, key, 1).unwrap();
        }
        tape
    }

    /// The same drain on the model (front lot = oldest by stamp).
    fn model_drain(model: &mut Model, key: Addr) -> Vec<(u128, bool)> {
        let mut tape = Vec::new();
        while model.units(&(MINT, key)) > 0 {
            let v = model.active.get_mut(&(MINT, key)).unwrap();
            let front = v[0];
            tape.push(match front.price {
                None => (0, true),
                Some(p) => (p.lamports_num / p.units_den, false),
            });
            if front.units == 1 {
                v.remove(0);
                if v.is_empty() {
                    model.active.remove(&(MINT, key));
                }
            } else {
                v[0].units -= 1;
            }
        }
        tape
    }

    /// NEW-4: under arbitrary interleavings of the lot operations, the
    /// production ledger must preserve EXACTLY the merge-free model's
    /// acquisition-age structure (asserted after EVERY operation), and
    /// FIFO consumption must draw the same economic lots in the same
    /// order (asserted by draining every bucket unit-by-unit and
    /// comparing the per-unit price tapes — not just the totals).
    #[test]
    fn global_fifo_matches_a_merge_free_reference_model() {
        // Two token accounts of one wallet plus a stake-beneficiary-style
        // key: lots travel between them (cross-key moves are NEW-4's core).
        let accounts = [
            Addr::fixture(0x71),
            Addr::fixture(0x72),
            Addr::fixture(0x73),
        ];

        for seed in 1..=64u64 {
            let mut rng = Rng(0x9E3779B97F4A7C15 ^ seed);
            let mut prod = LotLedger::default();
            let mut model = Model::default();
            for step in 0..300 {
                let key = accounts[rng.below(3) as usize];
                let src = accounts[rng.below(3) as usize];
                let dst = accounts[rng.below(3) as usize];
                let ctx = format!("seed {seed}, step {step}");
                let draw_price = |rng: &mut Rng| match rng.below(6) {
                    0 => None, // unpriced acquisition: fail-closed path
                    n => p(u128::from(1 + n % 4)),
                };
                let draw_basis = |rng: &mut Rng| {
                    if rng.below(2) == 0 {
                        LotBasis::Reference
                    } else {
                        LotBasis::Addition
                    }
                };
                match rng.below(8) {
                    0 => {
                        let (pr, b, u) = (
                            draw_price(&mut rng),
                            draw_basis(&mut rng),
                            1 + rng.below(40),
                        );
                        prod.credit(MINT, key, u, pr, b).unwrap();
                        model.credit((MINT, key), u, pr, b);
                    }
                    1 => {
                        let (pr, b, u) = (
                            draw_price(&mut rng),
                            draw_basis(&mut rng),
                            1 + rng.below(40),
                        );
                        prod.credit_pending(MINT, key, u, pr, b).unwrap();
                        model.credit_pending((MINT, key), u, pr, b);
                    }
                    2 => {
                        let avail = prod.units(MINT, key);
                        if avail > 0 {
                            let n = 1 + rng.below(avail);
                            prod.consume(MINT, key, n).unwrap();
                            model.consume((MINT, key), n);
                        }
                    }
                    3 => {
                        let avail = prod.pending_units(MINT, key);
                        if avail > 0 {
                            let n = 1 + rng.below(avail);
                            prod.consume_pending(MINT, key, n).unwrap();
                            model.consume_pending((MINT, key), n);
                        }
                    }
                    4 => {
                        let avail = prod.units(MINT, key);
                        if avail > 0 {
                            let n = 1 + rng.below(avail);
                            prod.request_unstake(MINT, key, n).unwrap();
                            model.request((MINT, key), n);
                        }
                    }
                    5 => {
                        let avail = prod.pending_units(MINT, key);
                        if avail > 0 {
                            let n = 1 + rng.below(avail);
                            prod.cancel_unstake(MINT, key, n).unwrap();
                            model.cancel((MINT, key), n);
                        }
                    }
                    6 => {
                        if src != dst {
                            let avail = prod.units(MINT, src);
                            if avail > 0 {
                                let n = 1 + rng.below(avail);
                                prod.stake(MINT, src, dst, n).unwrap();
                                model.move_lots((MINT, src), (MINT, dst), n);
                            }
                        }
                    }
                    _ => {
                        let pr = draw_price(&mut rng);
                        prod.rebase_registered(MINT, &[key], pr).unwrap();
                        model.rebase((MINT, key), pr);
                    }
                }
                assert_same_as_model(&prod, &model, &ctx);
            }

            // Economic assertion: drain every bucket FIFO, one unit at a
            // time — WHICH lot leaves, in which order, must match the
            // merge-free model exactly.
            for key in accounts {
                assert_eq!(
                    prod_drain(&mut prod, key),
                    model_drain(&mut model, key),
                    "seed {seed}: active-bucket FIFO tape of {key:?}"
                );
                let pend_p = prod.pending_units(MINT, key);
                let pend_m = model.pending_units(&(MINT, key));
                assert_eq!(pend_p, pend_m, "seed {seed}: pending totals of {key:?}");
                if pend_p > 0 {
                    prod.cancel_unstake(MINT, key, pend_p).unwrap();
                    model.cancel((MINT, key), pend_m);
                    assert_eq!(
                        prod_drain(&mut prod, key),
                        model_drain(&mut model, key),
                        "seed {seed}: pending->active FIFO tape of {key:?}"
                    );
                }
            }
        }
    }
}
