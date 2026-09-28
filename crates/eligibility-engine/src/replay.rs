//! Global ownership timeline replay and equity integration (section 5,
//! tests 16–29).
//!
//! * Events are totally ordered by `(slot, tx, ix, inner)`; each slot has
//!   one `unix_timestamp`. State between two consecutive events holds for
//!   the half-open interval `[t_i, t_{i+1})`. Events sharing a timestamp
//!   (one transaction, or distinct slots on one clock second) therefore give
//!   intermediate states **zero** duration: no overlap, no carousel credit,
//!   no flash-loan weight.
//! * Intervals are VERIFIED only when closed by a checkpoint whose observed
//!   finalized state equals the replay. A mismatch, an event the token
//!   program would have refused, or a declared gap marks the **whole
//!   common-pool window** since the last verified checkpoint UNRESOLVED —
//!   every participant PENDING, not only the wallet whose data is missing.
//!   Intervals after the last checkpoint are PENDING (not yet reconciled).
//! * Per wallet: `X = max(0, sum A*G - D)`, with `G` from recognized units
//!   of the selected asset and `D` netted once across the debt registry.

use std::collections::{BTreeMap, BTreeSet};

use crate::adapter::{AdapterConfig, AssetKind, MintRole, RiskExtension, StakeSupport};
use crate::debt::{DebtBook, DebtPending, DebtReading, DebtRegistry, WalletDebt};
use crate::error::{EligibilityError, EligibilityResult};
use crate::ledger::Ledger;
use crate::types::{Addr, Event, EventKind};
use crate::valuation::{PriceKey, PriceTable};

/// Configuration for one replay: the enabled adapters and the fixed debt
/// registry. Validated on construction.
#[derive(Clone, Debug)]
pub struct EligibilityConfig {
    adapters: BTreeMap<AssetKind, AdapterConfig>,
    registry: DebtRegistry,
}

impl EligibilityConfig {
    pub fn new(adapters: Vec<AdapterConfig>, registry: DebtRegistry) -> EligibilityResult<Self> {
        let mut map = BTreeMap::new();
        let mut mints = BTreeSet::new();
        for a in adapters {
            a.validate()?;
            for m in &a.mints {
                if !mints.insert(m.mint) {
                    return Err(EligibilityError::DuplicateMint);
                }
            }
            if map.insert(a.asset, a).is_some() {
                return Err(EligibilityError::InvalidAdapter);
            }
        }
        Ok(Self {
            adapters: map,
            registry,
        })
    }

    pub fn adapter(&self, asset: AssetKind) -> Option<&AdapterConfig> {
        self.adapters.get(&asset)
    }

    pub fn registry(&self) -> &DebtRegistry {
        &self.registry
    }
}

/// Why a wallet is PENDING (reserved, never zero, never admitted).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingReason {
    /// Missing or inconsistent ownership history in the common pool.
    UnresolvedHistory,
    /// After the last checkpoint: not yet reconciled.
    NotReconciled,
    Debt(DebtPending),
    /// A debt amount has no committed price.
    DebtUnpriced,
    /// A pinned external program of the selected asset changed version.
    AdapterProgramUnknown,
    /// A stake position exists but the stake adapter is not verified.
    StakeUnverified,
}

/// Why a wallet is INELIGIBLE (known before the fact; weight 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IneligibleReason {
    AssetNotConfigured,
    /// Token-2022 risk extension the adapter has not cleared.
    Token2022Extension(RiskExtension),
    /// No committed reference price: inactive, not UNKNOWN.
    InactiveNoReference,
    /// The supplied price basis is not admissible for this asset.
    InvalidPriceBasis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eligibility {
    Verified {
        /// Recognized units of the selected asset (wallet accounts + active
        /// verified stake), before valuation.
        units: u64,
        gross_lamports: u128,
        debt_lamports: u128,
        /// `max(0, gross - debt)`.
        x_lamports: u128,
    },
    Ineligible(IneligibleReason),
    Pending(PendingReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntervalStatus {
    Verified,
    Unresolved,
    NotReconciled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolInterval {
    pub start: i64,
    pub end: i64,
    pub status: IntervalStatus,
}

/// Integrated totals for one wallet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WalletAccrual {
    /// `sum X * dt` over VERIFIED intervals (lamport-seconds).
    pub equity_seconds: u128,
    /// `sum units * dt` over VERIFIED intervals.
    pub unit_seconds: u128,
    pub pending_seconds: u64,
    pub ineligible_seconds: u64,
}

/// One constant-state interval as it was finally classified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub start: i64,
    pub end: i64,
    pub status: IntervalStatus,
    pub wallets: BTreeMap<Addr, Eligibility>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayReport {
    pub wallets: BTreeMap<Addr, WalletAccrual>,
    pub pool: Vec<PoolInterval>,
    pub segments: Vec<Segment>,
}

impl ReplayReport {
    pub fn wallet(&self, w: &Addr) -> WalletAccrual {
        self.wallets.get(w).copied().unwrap_or_default()
    }
}

/// Sort events into the global order and validate the clock.
pub fn order_events(mut events: Vec<Event>) -> EligibilityResult<Vec<Event>> {
    events.sort_by_key(|e| e.key);
    for w in events.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if a.key == b.key {
            return Err(EligibilityError::DuplicateEventKey);
        }
        if a.key.slot == b.key.slot && a.unix_timestamp != b.unix_timestamp {
            return Err(EligibilityError::SlotTimestampMismatch);
        }
        if b.unix_timestamp < a.unix_timestamp {
            return Err(EligibilityError::NonMonotonicTimestamp);
        }
    }
    Ok(events)
}

struct State<'a> {
    config: &'a EligibilityConfig,
    prices: &'a PriceTable,
    ledger: Ledger,
    /// Whether `ledger` is the chain's state (false after divergence until a
    /// consistent checkpoint).
    in_sync: bool,
    registrations: BTreeMap<Addr, AssetKind>,
    debt: DebtBook,
    unknown_assets: BTreeSet<AssetKind>,
    /// Latest observed version of every pinned program.
    observed_versions: BTreeMap<Addr, [u8; 32]>,
}

impl State<'_> {
    fn evaluate(&self, wallet: &Addr, asset: AssetKind) -> EligibilityResult<Eligibility> {
        let Some(adapter) = self.config.adapter(asset) else {
            return Ok(Eligibility::Ineligible(
                IneligibleReason::AssetNotConfigured,
            ));
        };
        if let Some(e) = adapter.uncleared_extension() {
            return Ok(Eligibility::Ineligible(
                IneligibleReason::Token2022Extension(e),
            ));
        }
        let underlying = adapter.underlying_mint();
        let Some(price) = self.prices.get(PriceKey::Mint(underlying)) else {
            return Ok(Eligibility::Ineligible(
                IneligibleReason::InactiveNoReference,
            ));
        };
        if price.basis != asset.required_basis() {
            return Ok(Eligibility::Ineligible(IneligibleReason::InvalidPriceBasis));
        }
        if self.unknown_assets.contains(&asset) {
            return Ok(Eligibility::Pending(PendingReason::AdapterProgramUnknown));
        }

        let mut units: u64 = 0;
        for acc in self.ledger.accounts.values() {
            if &acc.owner != wallet || adapter.is_custody(&acc.owner) {
                continue;
            }
            if adapter.role_of(&acc.mint) == Some(MintRole::Underlying) {
                units = units
                    .checked_add(acc.recognized())
                    .ok_or(EligibilityError::MathOverflow)?;
            }
        }
        if let Some(p) = self.ledger.stakes.get(&(underlying, *wallet)) {
            match &adapter.stake {
                StakeSupport::Verified { .. } => {
                    // Pending unstake is excluded; only active stake counts.
                    units = units
                        .checked_add(p.active)
                        .ok_or(EligibilityError::MathOverflow)?;
                }
                StakeSupport::Unverified | StakeSupport::None => {
                    if p.active != 0 || p.pending_unstake != 0 {
                        return Ok(Eligibility::Pending(PendingReason::StakeUnverified));
                    }
                }
            }
        }
        let gross = price.value(units)?;

        let debt = match self.debt.wallet_state(self.config.registry(), wallet) {
            WalletDebt::Pending(r) => return Ok(Eligibility::Pending(PendingReason::Debt(r))),
            WalletDebt::Known(entries) => {
                let mut d: u128 = 0;
                for entry in entries {
                    for amount in entry {
                        let Some(p) = self.prices.get(amount.price_key) else {
                            return Ok(Eligibility::Pending(PendingReason::DebtUnpriced));
                        };
                        d = d
                            .checked_add(p.value(amount.units)?)
                            .ok_or(EligibilityError::MathOverflow)?;
                    }
                }
                d
            }
        };
        Ok(Eligibility::Verified {
            units,
            gross_lamports: gross,
            debt_lamports: debt,
            x_lamports: gross.saturating_sub(debt),
        })
    }

    fn evaluate_all(&self) -> EligibilityResult<BTreeMap<Addr, Eligibility>> {
        let mut out = BTreeMap::new();
        let mut counted: BTreeMap<AssetKind, u128> = BTreeMap::new();
        for (w, asset) in &self.registrations {
            let e = self.evaluate(w, *asset)?;
            if let Eligibility::Verified {
                units,
                gross_lamports,
                debt_lamports,
                x_lamports,
            } = e
            {
                if x_lamports > gross_lamports
                    || x_lamports + debt_lamports.min(gross_lamports) != gross_lamports
                {
                    return Err(EligibilityError::InvariantViolation("X != max(0, G - D)"));
                }
                *counted.entry(*asset).or_insert(0) += units as u128;
            }
            out.insert(*w, e);
        }
        // No underlying unit is counted for two owners: the units counted
        // across all wallets never exceed the underlying supply.
        if self.in_sync {
            for (asset, total) in counted {
                if let Some(a) = self.config.adapter(asset) {
                    let supply =
                        *self.ledger.supplies.get(&a.underlying_mint()).unwrap_or(&0) as u128;
                    if total > supply {
                        return Err(EligibilityError::InvariantViolation("units counted twice"));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Staked units must be backed by custody balances of the same mint.
    /// Only checkable where the stake adapter is verified (custody known);
    /// unverified stake positions make their wallets PENDING instead.
    fn stakes_backed(&self) -> bool {
        let verified = |mint: &Addr| {
            self.config.adapters.values().any(|a| {
                matches!(a.stake, StakeSupport::Verified { .. })
                    && a.role_of(mint) == Some(MintRole::Underlying)
            })
        };
        let mut positions: BTreeMap<Addr, u128> = BTreeMap::new();
        for ((mint, _), p) in self.ledger.stakes.iter().filter(|((m, _), _)| verified(m)) {
            *positions.entry(*mint).or_insert(0) += p.active as u128 + p.pending_unstake as u128;
        }
        positions.iter().all(|(mint, staked)| {
            let custody: u128 = self
                .ledger
                .accounts
                .values()
                .filter(|a| {
                    &a.mint == mint
                        && self
                            .config
                            .adapters
                            .values()
                            .any(|ad| ad.is_custody(&a.owner))
                })
                .map(|a| a.amount as u128)
                .sum();
            *staked <= custody
        })
    }

    fn apply(&mut self, kind: &EventKind) -> EligibilityResult<()> {
        match kind {
            EventKind::Register { wallet, asset } => {
                self.registrations.insert(*wallet, *asset);
            }
            EventKind::Exit { wallet } => {
                self.registrations.remove(wallet);
            }
            EventKind::DebtObserved {
                wallet,
                program,
                debts,
            } => {
                // Outside the registry: not detected (test 28).
                if self.config.registry().contains(program) {
                    self.debt
                        .record(*wallet, *program, DebtReading::Readable(debts.clone()));
                }
            }
            EventKind::DebtUnreadable { wallet, program } => {
                if self.config.registry().contains(program) {
                    self.debt.record(*wallet, *program, DebtReading::Unreadable);
                }
            }
            EventKind::ProgramVersion { program, version } => {
                self.observed_versions.insert(*program, *version);
                if let Some(e) = self.config.registry().entry(program) {
                    self.debt
                        .set_program_unknown(*program, &e.pin.version != version);
                }
                for a in self.config.adapters.values() {
                    if a.pins.iter().any(|p| &p.program == program) {
                        // Known only while EVERY pin of the adapter matches.
                        let all_match = a
                            .pins
                            .iter()
                            .all(|p| self.observed_versions.get(&p.program) == Some(&p.version));
                        if all_match {
                            self.unknown_assets.remove(&a.asset);
                        } else {
                            self.unknown_assets.insert(a.asset);
                        }
                    }
                }
            }
            EventKind::DeclaredGap => self.in_sync = false,
            EventKind::Checkpoint { .. } => unreachable!("handled by replay"),
            token => {
                if self.in_sync {
                    let before = self.ledger.clone();
                    if self.ledger.apply(token).is_err() || !self.stakes_backed() {
                        self.ledger = before;
                        self.in_sync = false;
                    } else {
                        self.ledger.check_invariants()?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn dt(start: i64, end: i64) -> EligibilityResult<u64> {
    u64::try_from(end.saturating_sub(start).max(0)).map_err(|_| EligibilityError::MathOverflow)
}

/// Replay `events` and integrate eligibility up to `end` (exclusive).
pub fn replay(
    config: &EligibilityConfig,
    prices: &PriceTable,
    events: Vec<Event>,
    end: i64,
) -> EligibilityResult<ReplayReport> {
    let events = order_events(events)?;
    // Fail closed on versions: every pinned external program (adapter or
    // registry) is UNKNOWN until a finalized observation matches its pin.
    let mut debt = DebtBook::default();
    for p in config.registry().programs() {
        debt.set_program_unknown(*p, true);
    }
    let unknown_assets = config
        .adapters
        .values()
        .filter(|a| !a.pins.is_empty())
        .map(|a| a.asset)
        .collect();
    let mut st = State {
        config,
        prices,
        ledger: Ledger::default(),
        in_sync: true,
        registrations: BTreeMap::new(),
        debt,
        unknown_assets,
        observed_versions: BTreeMap::new(),
    };
    let mut report = ReplayReport::default();
    // Segments since the last checkpoint, classified when it closes.
    let mut window: Vec<Segment> = Vec::new();
    let mut current = st.evaluate_all()?;
    let mut prev_ts: Option<i64> = None;

    for ev in &events {
        let ts = ev.unix_timestamp;
        if let Some(p) = prev_ts {
            let e = ts.min(end).max(p.min(end));
            if e > p.min(end) {
                window.push(Segment {
                    start: p.min(end),
                    end: e,
                    status: IntervalStatus::NotReconciled,
                    wallets: current.clone(),
                });
            }
        }
        prev_ts = Some(ts);

        if let EventKind::Checkpoint {
            accounts,
            stakes,
            supplies,
        } = &ev.kind
        {
            let observed = st.ledger.from_snapshot(accounts, stakes, supplies);
            let consistent = st.in_sync && st.ledger.same_chain_state(&observed);
            let status = if consistent {
                IntervalStatus::Verified
            } else {
                IntervalStatus::Unresolved
            };
            commit(&mut report, std::mem::take(&mut window), status)?;
            st.ledger = observed;
            // Observed state is trusted only if it is itself coherent.
            st.in_sync = st.ledger.conserved() && st.stakes_backed();
        } else {
            st.apply(&ev.kind)?;
        }
        current = st.evaluate_all()?;
    }
    if let Some(p) = prev_ts {
        if end > p {
            window.push(Segment {
                start: p,
                end,
                status: IntervalStatus::NotReconciled,
                wallets: current,
            });
        }
    }
    commit(&mut report, window, IntervalStatus::NotReconciled)?;
    Ok(report)
}

fn commit(
    report: &mut ReplayReport,
    window: Vec<Segment>,
    status: IntervalStatus,
) -> EligibilityResult<()> {
    for mut seg in window {
        seg.status = status;
        let d = dt(seg.start, seg.end)?;
        for (w, e) in seg.wallets.iter_mut() {
            let acc = report.wallets.entry(*w).or_default();
            match status {
                IntervalStatus::Unresolved => {
                    *e = Eligibility::Pending(PendingReason::UnresolvedHistory);
                    acc.pending_seconds += d;
                }
                IntervalStatus::NotReconciled => {
                    if !matches!(e, Eligibility::Ineligible(_)) {
                        *e = Eligibility::Pending(PendingReason::NotReconciled);
                        acc.pending_seconds += d;
                    } else {
                        acc.ineligible_seconds += d;
                    }
                }
                IntervalStatus::Verified => match *e {
                    Eligibility::Verified {
                        units, x_lamports, ..
                    } => {
                        acc.equity_seconds = acc
                            .equity_seconds
                            .checked_add(
                                x_lamports
                                    .checked_mul(d as u128)
                                    .ok_or(EligibilityError::MathOverflow)?,
                            )
                            .ok_or(EligibilityError::MathOverflow)?;
                        acc.unit_seconds = acc
                            .unit_seconds
                            .checked_add(units as u128 * d as u128)
                            .ok_or(EligibilityError::MathOverflow)?;
                    }
                    Eligibility::Pending(_) => acc.pending_seconds += d,
                    Eligibility::Ineligible(_) => acc.ineligible_seconds += d,
                },
            }
        }
        match report.pool.last_mut() {
            Some(last) if last.status == status && last.end == seg.start => last.end = seg.end,
            _ => report.pool.push(PoolInterval {
                start: seg.start,
                end: seg.end,
                status,
            }),
        }
        report.segments.push(seg);
    }
    Ok(())
}
