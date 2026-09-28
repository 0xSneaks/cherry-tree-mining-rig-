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
//! Lots are FIFO. Consumption order cannot create value: under any order the
//! total is the sum of the lots, and every lot's price is already capped by
//! the min rule at its acquisition. `price == None` marks a unit whose
//! addition price could not be verified (no usable observation): its holder
//! is PENDING (reserved), never silently reference-priced.
//!
//! Pending unstake units sit in a parallel bucket per beneficiary and are
//! excluded from valuation exactly like the ledger excludes them from
//! recognition.

use std::collections::BTreeMap;

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
}

impl LotLedger {
    fn vec_mut(&mut self, key: (Addr, Addr)) -> &mut Vec<Lot> {
        self.lots.entry(key).or_default()
    }

    /// Append a lot, coalescing with the tail when identical.
    pub fn credit(
        &mut self,
        mint: Addr,
        holder: Addr,
        units: u64,
        price: Option<UnitPrice>,
        basis: LotBasis,
    ) -> EligibilityResult<()> {
        let key = (mint, holder);
        let v = self.vec_mut(key);
        if let Some(last) = v.last_mut() {
            if last.basis == basis && last.price == price {
                last.units = last
                    .units
                    .checked_add(units)
                    .ok_or(EligibilityError::MathOverflow)?;
                return Ok(());
            }
        }
        v.push(Lot {
            units,
            price,
            basis,
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

    /// Registration after the reference timestamp (section 5): reference
    /// lots of these holders drop to `price`; addition lots keep their own
    /// event price; unpriced lots stay unpriced. `price == None` (no usable
    /// observation at registration) taints reference lots too.
    pub fn rebase_registered(
        &mut self,
        mint: Addr,
        holders: &[Addr],
        price: Option<UnitPrice>,
    ) -> EligibilityResult<()> {
        for h in holders {
            let key = (mint, *h);
            if let Some(v) = self.lots.get_mut(&key) {
                for lot in v.iter_mut() {
                    if lot.basis == LotBasis::Reference {
                        lot.price = price;
                    }
                }
                coalesce(v);
            }
        }
        Ok(())
    }

    /// Verified emergency NAV / published recovery reference: every lot of
    /// the mint re-bases to `price` (settled intervals are untouched because
    /// the replay applies this only from its event).
    pub fn rebase_mint(&mut self, mint: Addr, price: UnitPrice) -> EligibilityResult<()> {
        let rebase = |v: &mut Vec<Lot>| -> EligibilityResult<()> {
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
            }];
            Ok(())
        };
        for ((m, _), v) in self.lots.iter_mut() {
            if *m == mint {
                rebase(v)?;
            }
        }
        for ((m, _), v) in self.pending.iter_mut() {
            if *m == mint {
                rebase(v)?;
            }
        }
        Ok(())
    }

    /// Stake: units move from the wallet account into the beneficiary's
    /// active bucket, carrying their lots.
    pub fn stake(
        &mut self,
        mint: Addr,
        from_account: Addr,
        beneficiary: Addr,
        units: u64,
    ) -> EligibilityResult<()> {
        let moved = self.split_off(mint, from_account, units)?;
        for lot in moved {
            let v = self.vec_mut((mint, beneficiary));
            v.push(lot);
        }
        Ok(())
    }

    /// RequestUnstake: active → pending bucket (excluded from valuation).
    pub fn request_unstake(
        &mut self,
        mint: Addr,
        beneficiary: Addr,
        units: u64,
    ) -> EligibilityResult<()> {
        let moved = self.split_off(mint, beneficiary, units)?;
        self.pending
            .entry((mint, beneficiary))
            .or_default()
            .extend(moved);
        Ok(())
    }

    /// CancelUnstake: pending → active (lots return).
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
        let idx = 0;
        while left > 0 && idx < v.len() {
            if v[idx].units <= left {
                left -= v[idx].units;
                taken.push(v.swap_remove(idx)); // order within pending is irrelevant
            } else {
                v[idx].units -= left;
                taken.push(Lot {
                    units: left,
                    ..v[idx]
                });
                left = 0;
            }
        }
        if v.is_empty() {
            self.pending.remove(&key);
        }
        if left > 0 {
            return Err(EligibilityError::InvariantViolation("cancel above pending"));
        }
        for lot in taken {
            self.vec_mut(key).push(lot);
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
    /// adoption of pending balances the replay did not observe).
    pub fn credit_pending(
        &mut self,
        mint: Addr,
        holder: Addr,
        units: u64,
        price: Option<UnitPrice>,
    ) -> EligibilityResult<()> {
        let v = self.pending.entry((mint, holder)).or_default();
        if let Some(last) = v.last_mut() {
            if last.price == price {
                last.units = last
                    .units
                    .checked_add(units)
                    .ok_or(EligibilityError::MathOverflow)?;
                return Ok(());
            }
        }
        v.push(Lot {
            units,
            price,
            basis: LotBasis::Addition,
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

/// Merge adjacent identical lots so vectors stay small.
fn coalesce(v: &mut Vec<Lot>) {
    let mut out: Vec<Lot> = Vec::with_capacity(v.len());
    for lot in v.iter() {
        if let Some(last) = out.last_mut() {
            if last.basis == lot.basis && last.price == lot.price {
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
        l.rebase_registered(MINT, &[W], None).unwrap();
        // Reference lot becomes unpriced; the addition lot keeps its price.
        let h = l.head_value(MINT, W, 200).unwrap();
        assert!(h.unpriced);
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
}
