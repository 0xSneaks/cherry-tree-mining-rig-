//! Fixed debt-protocol registry (section 5, debt admission rule).
//!
//! * Debt is netted once, at wallet level, across the registry.
//! * Readings join the finalized event timeline; an increase reduces X from
//!   its event, never retroactively.
//! * A registered wallet whose state in any active registry entry is
//!   unread, unreadable, or whose entry program changed version is PENDING —
//!   never zero, never admitted.
//! * Debt in programs outside the registry is NOT detected. This crate
//!   never claims otherwise: see [`crate::NON_REGISTRY_DEBT_DISCLOSURE`].
//!
//! Production registry contents are activation configuration (gate
//! 14.2-D). Nothing here names a real lending program.

use std::collections::{BTreeMap, BTreeSet};

use crate::adapter::ProgramPin;
use crate::error::{EligibilityError, EligibilityResult};
use crate::types::{Addr, DebtAmount};

/// One approved debt program, pinned to a verified deployed version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegistryEntry {
    pub pin: ProgramPin,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DebtRegistry {
    entries: BTreeMap<Addr, RegistryEntry>,
}

impl DebtRegistry {
    pub fn new(entries: Vec<RegistryEntry>) -> EligibilityResult<Self> {
        let mut map = BTreeMap::new();
        for e in entries {
            if map.insert(e.pin.program, e).is_some() {
                return Err(EligibilityError::DuplicateRegistryEntry);
            }
        }
        Ok(Self { entries: map })
    }

    pub fn contains(&self, program: &Addr) -> bool {
        self.entries.contains_key(program)
    }

    pub fn programs(&self) -> impl Iterator<Item = &Addr> {
        self.entries.keys()
    }

    pub fn entry(&self, program: &Addr) -> Option<&RegistryEntry> {
        self.entries.get(program)
    }
}

/// One wallet's reading in one registry entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DebtReading {
    Readable(Vec<DebtAmount>),
    Unreadable,
}

/// Debt readings for every (wallet, registry program) seen so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DebtBook {
    readings: BTreeMap<(Addr, Addr), DebtReading>,
    /// Registry programs whose observed version differs from the pin.
    unknown_programs: BTreeSet<Addr>,
}

impl DebtBook {
    pub fn record(&mut self, wallet: Addr, program: Addr, reading: DebtReading) {
        self.readings.insert((wallet, program), reading);
    }

    pub fn set_program_unknown(&mut self, program: Addr, unknown: bool) {
        if unknown {
            self.unknown_programs.insert(program);
        } else {
            self.unknown_programs.remove(&program);
        }
    }

    /// The wallet's debt state across the whole registry.
    pub fn wallet_state<'a>(&'a self, registry: &'a DebtRegistry, wallet: &Addr) -> WalletDebt<'a> {
        let mut debts: Vec<&'a [DebtAmount]> = Vec::new();
        for program in registry.programs() {
            if self.unknown_programs.contains(program) {
                return WalletDebt::Pending(DebtPending::RegistryProgramUnknown);
            }
            match self.readings.get(&(*wallet, *program)) {
                None => return WalletDebt::Pending(DebtPending::NotYetRead),
                Some(DebtReading::Unreadable) => {
                    return WalletDebt::Pending(DebtPending::Unreadable)
                }
                Some(DebtReading::Readable(v)) => debts.push(v.as_slice()),
            }
        }
        WalletDebt::Known(debts)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebtPending {
    NotYetRead,
    Unreadable,
    RegistryProgramUnknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WalletDebt<'a> {
    /// Every registry entry read; the listed amounts, entry by entry.
    Known(Vec<&'a [DebtAmount]>),
    Pending(DebtPending),
}
