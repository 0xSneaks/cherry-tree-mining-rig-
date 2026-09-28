//! Replayed token state: accounts, stake beneficiary positions and mint
//! supplies, updated by finalized events exactly as the token programs do.
//!
//! `apply` either applies an event completely or not at all. An event the
//! token program would have refused (insufficient balance, frozen account,
//! unknown account, delegate over-spend, ...) means the replayed history is
//! not the chain's history: it returns [`Inconsistent`] and the caller marks
//! the reconciliation window unresolved.

use std::collections::BTreeMap;

use crate::error::{EligibilityError, EligibilityResult};
use crate::types::{
    AccountSnapshot, Addr, Authority, EncumbranceKind, EventKind, StakeSnapshot, SupplySnapshot,
};

/// Chain-observable fields of a token account:
/// `(mint, owner, amount, delegate, delegated_amount, frozen)`.
type ChainAccount = (Addr, Addr, u64, Option<Addr>, u64, bool);

/// Replay diverged from what the chain could have done.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inconsistent(pub &'static str);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenAccount {
    pub mint: Addr,
    pub owner: Addr,
    pub amount: u64,
    pub delegate: Option<Addr>,
    pub delegated_amount: u64,
    pub frozen: bool,
    pub encumbered: BTreeMap<EncumbranceKind, u64>,
}

impl TokenAccount {
    fn new(mint: Addr, owner: Addr) -> Self {
        Self {
            mint,
            owner,
            amount: 0,
            delegate: None,
            delegated_amount: 0,
            frozen: false,
            encumbered: BTreeMap::new(),
        }
    }

    /// Units recognized for the owner (section 5): 0 when frozen, else
    /// `amount - delegated_amount - encumbered`, never below 0.
    pub fn recognized(&self) -> u64 {
        if self.frozen {
            return 0;
        }
        let enc: u64 = self
            .encumbered
            .values()
            .fold(0u64, |a, b| a.saturating_add(*b));
        self.amount
            .saturating_sub(self.delegated_amount)
            .saturating_sub(enc)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StakePosition {
    pub active: u64,
    pub pending_unstake: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    pub accounts: BTreeMap<Addr, TokenAccount>,
    /// `(underlying mint, beneficiary)` → position.
    pub stakes: BTreeMap<(Addr, Addr), StakePosition>,
    pub supplies: BTreeMap<Addr, u64>,
}

fn add(a: u64, b: u64) -> Result<u64, Inconsistent> {
    a.checked_add(b).ok_or(Inconsistent("overflow"))
}

fn sub(a: u64, b: u64, why: &'static str) -> Result<u64, Inconsistent> {
    a.checked_sub(b).ok_or(Inconsistent(why))
}

impl Ledger {
    fn account_mut(&mut self, a: &Addr) -> Result<&mut TokenAccount, Inconsistent> {
        self.accounts
            .get_mut(a)
            .ok_or(Inconsistent("unknown account"))
    }

    fn debit(&mut self, a: &Addr, amount: u64, authority: Authority) -> Result<Addr, Inconsistent> {
        let acc = self.account_mut(a)?;
        if acc.frozen {
            return Err(Inconsistent("frozen account debited"));
        }
        acc.amount = sub(acc.amount, amount, "insufficient balance")?;
        if authority == Authority::Delegate {
            if acc.delegate.is_none() {
                return Err(Inconsistent("delegate debit without delegate"));
            }
            acc.delegated_amount = sub(acc.delegated_amount, amount, "delegate over-spend")?;
            if acc.delegated_amount == 0 {
                acc.delegate = None;
            }
        }
        Ok(acc.mint)
    }

    fn credit(&mut self, a: &Addr, mint: Addr, amount: u64) -> Result<(), Inconsistent> {
        let acc = self.account_mut(a)?;
        if acc.frozen {
            return Err(Inconsistent("frozen account credited"));
        }
        if acc.mint != mint {
            return Err(Inconsistent("mint mismatch"));
        }
        acc.amount = add(acc.amount, amount)?;
        Ok(())
    }

    fn transfer(
        &mut self,
        src: &Addr,
        dst: &Addr,
        amount: u64,
        authority: Authority,
    ) -> Result<Addr, Inconsistent> {
        let mint = self.debit(src, amount, authority)?;
        self.credit(dst, mint, amount)?;
        Ok(mint)
    }

    /// Apply one token/stake event atomically. Non-token events are no-ops.
    pub fn apply(&mut self, kind: &EventKind) -> Result<(), Inconsistent> {
        let mut next = self.clone();
        next.apply_in_place(kind)?;
        *self = next;
        Ok(())
    }

    fn apply_in_place(&mut self, kind: &EventKind) -> Result<(), Inconsistent> {
        match kind {
            EventKind::InitializeAccount {
                account,
                mint,
                owner,
            } => {
                if self.accounts.contains_key(account) {
                    return Err(Inconsistent("account already initialized"));
                }
                self.accounts
                    .insert(*account, TokenAccount::new(*mint, *owner));
                self.supplies.entry(*mint).or_insert(0);
            }
            EventKind::CloseAccount { account } => {
                let acc = self
                    .accounts
                    .get(account)
                    .ok_or(Inconsistent("unknown account"))?;
                if acc.amount != 0 {
                    return Err(Inconsistent("closing non-empty account"));
                }
                self.accounts.remove(account);
            }
            EventKind::MintTo { account, amount } => {
                let mint = self
                    .accounts
                    .get(account)
                    .ok_or(Inconsistent("unknown account"))?
                    .mint;
                self.credit(account, mint, *amount)?;
                let s = self.supplies.entry(mint).or_insert(0);
                *s = add(*s, *amount)?;
            }
            EventKind::Burn {
                account,
                amount,
                authority,
            } => {
                let mint = self.debit(account, *amount, *authority)?;
                let s = self.supplies.entry(mint).or_insert(0);
                *s = sub(*s, *amount, "burn above supply")?;
            }
            EventKind::Transfer {
                source,
                destination,
                amount,
                authority,
            } => {
                self.transfer(source, destination, *amount, *authority)?;
            }
            EventKind::SetAccountOwner { account, new_owner } => {
                let acc = self.account_mut(account)?;
                if acc.frozen {
                    return Err(Inconsistent("owner change on frozen account"));
                }
                acc.owner = *new_owner;
                acc.delegate = None;
                acc.delegated_amount = 0;
            }
            EventKind::Approve {
                account,
                delegate,
                amount,
            } => {
                let acc = self.account_mut(account)?;
                if acc.frozen {
                    return Err(Inconsistent("approve on frozen account"));
                }
                acc.delegate = Some(*delegate);
                acc.delegated_amount = *amount;
            }
            EventKind::Revoke { account } => {
                let acc = self.account_mut(account)?;
                if acc.frozen {
                    return Err(Inconsistent("revoke on frozen account"));
                }
                acc.delegate = None;
                acc.delegated_amount = 0;
            }
            EventKind::Freeze { account } => {
                let acc = self.account_mut(account)?;
                if acc.frozen {
                    return Err(Inconsistent("already frozen"));
                }
                acc.frozen = true;
            }
            EventKind::Thaw { account } => {
                let acc = self.account_mut(account)?;
                if !acc.frozen {
                    return Err(Inconsistent("thaw of unfrozen account"));
                }
                acc.frozen = false;
            }
            EventKind::Encumber {
                account,
                kind,
                amount,
            } => {
                let acc = self.account_mut(account)?;
                let e = acc.encumbered.entry(*kind).or_insert(0);
                *e = add(*e, *amount)?;
            }
            EventKind::Release {
                account,
                kind,
                amount,
            } => {
                let acc = self.account_mut(account)?;
                let e = acc.encumbered.entry(*kind).or_insert(0);
                *e = sub(*e, *amount, "release above encumbrance")?;
                if *e == 0 {
                    acc.encumbered.remove(kind);
                }
            }
            EventKind::Stake {
                source,
                custody,
                beneficiary,
                amount,
            } => {
                let mint = self.transfer(source, custody, *amount, Authority::Owner)?;
                let p = self.stakes.entry((mint, *beneficiary)).or_default();
                p.active = add(p.active, *amount)?;
            }
            EventKind::RequestUnstake {
                mint,
                beneficiary,
                amount,
            } => {
                let p = self
                    .stakes
                    .get_mut(&(*mint, *beneficiary))
                    .ok_or(Inconsistent("unknown stake"))?;
                p.active = sub(p.active, *amount, "unstake above active")?;
                p.pending_unstake = add(p.pending_unstake, *amount)?;
            }
            EventKind::CancelUnstake {
                mint,
                beneficiary,
                amount,
            } => {
                let p = self
                    .stakes
                    .get_mut(&(*mint, *beneficiary))
                    .ok_or(Inconsistent("unknown stake"))?;
                p.pending_unstake = sub(p.pending_unstake, *amount, "cancel above pending")?;
                p.active = add(p.active, *amount)?;
            }
            EventKind::WithdrawUnstaked {
                custody,
                destination,
                beneficiary,
                amount,
            } => {
                let mint = self.transfer(custody, destination, *amount, Authority::Owner)?;
                let p = self
                    .stakes
                    .get_mut(&(mint, *beneficiary))
                    .ok_or(Inconsistent("unknown stake"))?;
                p.pending_unstake = sub(p.pending_unstake, *amount, "withdraw above pending")?;
            }
            _ => {}
        }
        Ok(())
    }

    /// State exactly as observed at a checkpoint. Adapter-reported
    /// encumbrances are carried over for accounts that still exist.
    pub fn from_snapshot(
        &self,
        accounts: &[AccountSnapshot],
        stakes: &[StakeSnapshot],
        supplies: &[SupplySnapshot],
    ) -> Ledger {
        let mut l = Ledger::default();
        for s in supplies {
            l.supplies.insert(s.mint, s.supply);
        }
        for a in accounts {
            let encumbered = self
                .accounts
                .get(&a.account)
                .map(|x| x.encumbered.clone())
                .unwrap_or_default();
            l.supplies.entry(a.mint).or_insert(0);
            l.accounts.insert(
                a.account,
                TokenAccount {
                    mint: a.mint,
                    owner: a.owner,
                    amount: a.amount,
                    delegate: a.delegate,
                    delegated_amount: a.delegated_amount,
                    frozen: a.frozen,
                    encumbered,
                },
            );
        }
        for s in stakes {
            if s.active != 0 || s.pending_unstake != 0 {
                l.stakes.insert(
                    (s.mint, s.beneficiary),
                    StakePosition {
                        active: s.active,
                        pending_unstake: s.pending_unstake,
                    },
                );
            }
        }
        l
    }

    /// Chain-observable equality (encumbrances are adapter-reported, not
    /// part of token state; empty stake positions are equivalent to none).
    pub fn same_chain_state(&self, other: &Ledger) -> bool {
        let strip = |l: &Ledger| {
            let accounts: BTreeMap<Addr, ChainAccount> = l
                .accounts
                .iter()
                .map(|(k, a)| {
                    (
                        *k,
                        (
                            a.mint,
                            a.owner,
                            a.amount,
                            a.delegate,
                            a.delegated_amount,
                            a.frozen,
                        ),
                    )
                })
                .collect();
            let stakes: BTreeMap<(Addr, Addr), StakePosition> = l
                .stakes
                .iter()
                .filter(|(_, p)| p.active != 0 || p.pending_unstake != 0)
                .map(|(k, p)| (*k, *p))
                .collect();
            let supplies: BTreeMap<Addr, u64> = l
                .supplies
                .iter()
                .filter(|(_, s)| **s != 0)
                .map(|(k, s)| (*k, *s))
                .collect();
            (accounts, stakes, supplies)
        };
        strip(self) == strip(other)
    }

    /// Token conservation: every mint's account balances sum to its supply.
    pub fn conserved(&self) -> bool {
        let mut sums: BTreeMap<Addr, u128> = BTreeMap::new();
        for a in self.accounts.values() {
            *sums.entry(a.mint).or_insert(0) += a.amount as u128;
        }
        for (mint, sum) in &sums {
            if *sum != *self.supplies.get(mint).unwrap_or(&0) as u128 {
                return false;
            }
        }
        self.supplies
            .iter()
            .all(|(m, s)| *s == 0 || sums.contains_key(m))
    }

    /// Engine-level invariants that hold by construction of `apply`.
    pub fn check_invariants(&self) -> EligibilityResult<()> {
        if !self.conserved() {
            return Err(EligibilityError::InvariantViolation("token conservation"));
        }
        for a in self.accounts.values() {
            if a.recognized() > a.amount {
                return Err(EligibilityError::InvariantViolation(
                    "recognized above owned",
                ));
            }
        }
        Ok(())
    }
}
