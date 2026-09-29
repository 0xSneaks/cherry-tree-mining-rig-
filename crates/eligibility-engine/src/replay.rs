//! Global ownership timeline replay and equity integration (section 5,
//! tests 16–29; Pass #5 adds reference pricing, Safe Mode resets and route
//! halts, tests 30–39; Pass #6 adds epoch-level selection gating and the
//! epoch-boundary carry, architecture §5 "switching is effective next
//! epoch").
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
//! * Selection gating (Pass #6, P6-D3): one selected asset per wallet per
//!   epoch. A wallet's FIRST activation of an epoch (or the selection
//!   carried in across the boundary) fixes that epoch's asset; a later
//!   `Register` naming a different asset is a deferred switch, effective
//!   NEXT epoch — including after an exit, so exit + re-register cannot
//!   buy an immediate same-epoch switch. Exiting itself is immediate.
//! * Valuation (Pass #5): units are carried as FIFO lots. Units held at the
//!   reference timestamp use the committed reference price; units added or
//!   registered after it use `min(reference, verified price at the event)`
//!   (test 30) — where "verified at the event" includes a freshness bound
//!   calibrated per adapter (uncalibrated = fail closed), and registration
//!   pricing applies exactly once, at the holder's first registration
//!   event. An `EmergencyNav` event re-bases every lot of the mint from
//!   its event — prospective only, settled segments untouched (test 36). A
//!   `RouteHalt` pauses recognition (reserved, PENDING) until
//!   `RouteResumed` publishes a fresh reference segment.

use std::collections::{BTreeMap, BTreeSet};

use crate::adapter::{AdapterConfig, AssetKind, MintRole, RiskExtension, StakeSupport};
use crate::debt::{DebtBook, DebtPending, DebtReading, DebtRegistry, WalletDebt};
use crate::error::{EligibilityError, EligibilityResult};
use crate::ledger::Ledger;
use crate::lots::{LotBasis, LotLedger};
use crate::pricing::{min_price, EpochReferences};
use crate::types::{Addr, Event, EventKind};
use crate::valuation::{PriceKey, UnitPrice};

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

    fn adapter_of_mint(&self, mint: &Addr) -> Option<&AdapterConfig> {
        self.adapters
            .values()
            .find(|a| a.role_of(mint) == Some(MintRole::Underlying))
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
    /// Units acquired after the reference timestamp whose addition price
    /// could not be verified (no usable observation). Fail closed: the
    /// wallet is reserved, never silently reference-priced (test 30).
    AdditionUnpriced,
    /// Verified loss of the required operational eligibility route:
    /// recognition pauses (reserve) until a published recovery (tests 35–36).
    RouteHalted,
    /// A NAV/reset datum for the asset is unusable (wrong basis): valuation
    /// cannot be verified, so recognition pauses.
    NavUnverified,
}

/// Why a wallet is INELIGIBLE (known before the fact; weight 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IneligibleReason {
    AssetNotConfigured,
    /// Token-2022 risk extension the adapter has not cleared.
    Token2022Extension(RiskExtension),
    /// No committed reference price: inactive, not UNKNOWN (test 32).
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
    references: &'a EpochReferences,
    ledger: Ledger,
    /// Whether `ledger` is the chain's state (false after divergence until a
    /// consistent checkpoint).
    in_sync: bool,
    registrations: BTreeMap<Addr, AssetKind>,
    /// The one asset each wallet may activate THIS epoch (Pass #6, P6-D3):
    /// fixed at the wallet's first activation of the epoch (or by the
    /// selection carried in across the boundary). A `Register` naming any
    /// other asset is deferred to the next epoch.
    anchors: BTreeMap<Addr, AssetKind>,
    /// Deferred next-epoch selections: a switch requested during this
    /// epoch (possibly after an exit). Never active within this epoch.
    deferred: BTreeMap<Addr, AssetKind>,
    debt: DebtBook,
    unknown_assets: BTreeSet<AssetKind>,
    /// Latest observed version of every pinned program.
    observed_versions: BTreeMap<Addr, [u8; 32]>,
    /// Reference overrides from verified reset/recovery events
    /// (`EmergencyNav`, `RouteResumed`); the epoch commit backs the rest.
    active: BTreeMap<PriceKey, UnitPrice>,
    /// Latest verified in-epoch observation per mint (basis-checked), with
    /// the finalized time it was observed at: an observation is usable only
    /// inside the adapter's calibrated freshness window (`None` = never).
    observations: BTreeMap<Addr, (UnitPrice, i64)>,
    /// Mints whose required route is halted (recognition paused).
    halted_mints: BTreeSet<Addr>,
    /// Mints with an unusable NAV/reset datum (wrong basis).
    nav_invalid: BTreeSet<Addr>,
    lots: LotLedger,
}

impl State<'_> {
    /// The price in force for `key`: a verified reset override, else the
    /// committed epoch reference. `None` when nothing is committed.
    fn price(&self, key: &PriceKey) -> Option<UnitPrice> {
        self.active
            .get(key)
            .copied()
            .or_else(|| self.references.get(key).map(|c| c.price))
    }

    /// Basis- and freshness-checked latest observation for a mint, if
    /// usable at `now`. A previously valid observation does not remain
    /// usable indefinitely: outside the adapter's calibrated window it is
    /// stale (and with no calibration, every observation is stale), so it
    /// cannot act as the "verified price at the event" for an addition or
    /// late registration — those units are unpriced and PENDING instead.
    fn observation_for(&self, mint: &Addr, now: i64) -> Option<UnitPrice> {
        let adapter = self.config.adapter_of_mint(mint)?;
        let required = adapter.asset.required_basis();
        let &(obs, observed_at) = self.observations.get(mint)?;
        if obs.basis != required {
            return None;
        }
        let max_age = adapter.observation_max_age?;
        (now.saturating_sub(observed_at) <= max_age).then_some(obs)
    }

    /// Acquisition price for units credited at `ts` (test 30): reference
    /// basis at or before the epoch start; afterwards
    /// `min(active reference, verified observation)`, or unpriced when no
    /// usable (basis-correct, fresh) observation exists.
    fn acquisition(
        &self,
        mint: &Addr,
        ts: i64,
    ) -> EligibilityResult<(Option<UnitPrice>, LotBasis)> {
        let key = PriceKey::Mint(*mint);
        let Some(commit) = self.references.get(&key) else {
            return Ok((None, LotBasis::Reference));
        };
        if ts <= commit.epoch_start {
            return Ok((Some(commit.price), LotBasis::Reference));
        }
        let reference = self.price(&key).unwrap_or(commit.price);
        match self.observation_for(mint, ts) {
            None => Ok((None, LotBasis::Addition)),
            Some(obs) => Ok((Some(min_price(&reference, &obs)?), LotBasis::Addition)),
        }
    }

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
        let key = PriceKey::Mint(underlying);
        if self.references.get(&key).is_none() {
            return Ok(Eligibility::Ineligible(
                IneligibleReason::InactiveNoReference,
            ));
        }
        let Some(price) = self.price(&key) else {
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
        if self.nav_invalid.contains(&underlying) {
            return Ok(Eligibility::Pending(PendingReason::NavUnverified));
        }
        if self.halted_mints.contains(&underlying) {
            return Ok(Eligibility::Pending(PendingReason::RouteHalted));
        }

        let mut units: u64 = 0;
        let mut gross: u128 = 0;
        let mut unpriced = false;
        for (account, acc) in &self.ledger.accounts {
            if &acc.owner != wallet || adapter.is_custody(&acc.owner) {
                continue;
            }
            if adapter.role_of(&acc.mint) == Some(MintRole::Underlying) {
                let recognized = acc.recognized();
                units = units
                    .checked_add(recognized)
                    .ok_or(EligibilityError::MathOverflow)?;
                let head = self.lots.head_value(acc.mint, *account, recognized)?;
                gross = gross
                    .checked_add(head.lamports)
                    .ok_or(EligibilityError::MathOverflow)?;
                unpriced |= head.unpriced;
            }
        }
        if let Some(p) = self.ledger.stakes.get(&(underlying, *wallet)) {
            match &adapter.stake {
                StakeSupport::Verified { .. } => {
                    // Pending unstake is excluded; only active stake counts.
                    units = units
                        .checked_add(p.active)
                        .ok_or(EligibilityError::MathOverflow)?;
                    let head = self.lots.head_value(underlying, *wallet, p.active)?;
                    gross = gross
                        .checked_add(head.lamports)
                        .ok_or(EligibilityError::MathOverflow)?;
                    unpriced |= head.unpriced;
                }
                StakeSupport::Unverified | StakeSupport::None => {
                    if p.active != 0 || p.pending_unstake != 0 {
                        return Ok(Eligibility::Pending(PendingReason::StakeUnverified));
                    }
                }
            }
        }
        if unpriced {
            return Ok(Eligibility::Pending(PendingReason::AdditionUnpriced));
        }

        let debt = match self.debt.wallet_state(self.config.registry(), wallet) {
            WalletDebt::Pending(r) => return Ok(Eligibility::Pending(PendingReason::Debt(r))),
            WalletDebt::Known(entries) => {
                let mut d: u128 = 0;
                for entry in entries {
                    for amount in entry {
                        let Some(p) = self.price(&amount.price_key) else {
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

    fn mint_of_account(&self, account: &Addr) -> Option<Addr> {
        self.ledger.accounts.get(account).map(|a| a.mint)
    }

    /// Credit acquisition-priced units, anchoring pre-reference credits:
    /// units credited while their holder's wallet is already registered for
    /// the asset are held under a registration at the epoch-start boundary,
    /// so their reference pricing is permanent and no later registration
    /// event may reprice them (accounts acquired after the wallet's own
    /// registration would otherwise escape the registration anchoring).
    fn credit_anchored(
        &mut self,
        mint: Addr,
        holder: Addr,
        units: u64,
        price: Option<UnitPrice>,
        basis: LotBasis,
    ) -> EligibilityResult<()> {
        self.lots.credit(mint, holder, units, price, basis)?;
        if basis == LotBasis::Reference {
            if let Some(adapter) = self.config.adapter_of_mint(&mint) {
                let registered = self
                    .ledger
                    .accounts
                    .get(&holder)
                    .and_then(|a| self.registrations.get(&a.owner));
                if registered == Some(&adapter.asset) {
                    self.lots.mark_registered(mint, &[holder]);
                }
            }
        }
        Ok(())
    }

    /// Beneficiary-bucket credits with the same registration anchoring as
    /// [`Self::credit_anchored`], for checkpoint adoption: the holder key
    /// IS the beneficiary wallet (there is no token account to look up).
    /// Pre-reference (Reference-basis) credits of a wallet whose selection
    /// is active are anchored, so a redundant mid-epoch re-registration can
    /// never reprice them (Pass #6 carry-in equivalence: a selection
    /// carried across the boundary behaves exactly like a registration at
    /// the boundary). For every non-carry flow this marking is idempotent —
    /// an already-registered wallet's keys were marked by its registration
    /// event — so verified Pass #4/#5 behavior is unchanged.
    fn credit_beneficiary_anchored(
        &mut self,
        mint: Addr,
        beneficiary: Addr,
        units: u64,
        price: Option<UnitPrice>,
        basis: LotBasis,
        pending: bool,
    ) -> EligibilityResult<()> {
        if pending {
            self.lots
                .credit_pending(mint, beneficiary, units, price, basis)?;
        } else {
            self.lots.credit(mint, beneficiary, units, price, basis)?;
        }
        if basis == LotBasis::Reference {
            if let Some(adapter) = self.config.adapter_of_mint(&mint) {
                if self.registrations.get(&beneficiary) == Some(&adapter.asset) {
                    self.lots.mark_registered(mint, &[beneficiary]);
                }
            }
        }
        Ok(())
    }

    /// Keep lots in step with a token event that the ledger just applied.
    fn apply_lots(&mut self, kind: &EventKind, ts: i64) -> EligibilityResult<()> {
        match kind {
            EventKind::InitializeAccount { account, .. } => {
                // A freshly initialized account has no pricing history:
                // any anchor or residual lots left by a previous life of
                // the SAME address must not survive the reuse (F1-A,
                // close + re-initialize). The ledger refuses to
                // double-initialize a live account, so this only ever
                // erases stale state.
                self.lots.erase_account(*account);
                Ok(())
            }
            EventKind::CloseAccount { account } => {
                // The account key dies with the account (the ledger only
                // closes EMPTY accounts): its registration anchor and any
                // residual lots must not survive for the next owner of
                // the address (F1-A, close + re-initialize).
                self.lots.erase_account(*account);
                Ok(())
            }
            EventKind::MintTo { account, amount } => {
                let mint = self
                    .mint_of_account(account)
                    .ok_or(EligibilityError::InvariantViolation("unknown account"))?;
                let (price, basis) = self.acquisition(&mint, ts)?;
                self.credit_anchored(mint, *account, *amount, price, basis)
            }
            EventKind::Burn {
                account, amount, ..
            } => {
                let mint = self
                    .mint_of_account(account)
                    .ok_or(EligibilityError::InvariantViolation("unknown account"))?;
                self.lots.consume(mint, *account, *amount)
            }
            EventKind::Transfer {
                source,
                destination,
                amount,
                ..
            } => {
                let mint = self
                    .mint_of_account(source)
                    .ok_or(EligibilityError::InvariantViolation("unknown account"))?;
                self.lots.consume(mint, *source, *amount)?;
                let (price, basis) = self.acquisition(&mint, ts)?;
                self.credit_anchored(mint, *destination, *amount, price, basis)
            }
            EventKind::SetAccountOwner { account, .. } => {
                // Test 19: an account-owner change is a transfer of the
                // whole account, so the recipient acquires every unit at
                // this event. Source and destination share the account key:
                // consume the lots FIFO and re-credit them at the event's
                // acquisition price (min rule), exactly like a token
                // transfer — the recipient never inherits the previous
                // holder's pre-drop reference pricing. The prior owner's
                // registration anchor dies with the transfer (F1-A):
                // `credit_anchored` re-anchors only if the RECIPIENT
                // wallet is itself already registered, so a recipient's
                // own first registration always applies.
                let mint = self
                    .mint_of_account(account)
                    .ok_or(EligibilityError::InvariantViolation("unknown account"))?;
                let units = self.lots.units(mint, *account);
                self.lots.consume(mint, *account, units)?;
                self.lots.clear_registered(mint, *account);
                let (price, basis) = self.acquisition(&mint, ts)?;
                self.credit_anchored(mint, *account, units, price, basis)
            }
            EventKind::Stake {
                source,
                beneficiary,
                amount,
                ..
            } => {
                let mint = self
                    .mint_of_account(source)
                    .ok_or(EligibilityError::InvariantViolation("unknown account"))?;
                self.lots.stake(mint, *source, *beneficiary, *amount)
            }
            EventKind::RequestUnstake {
                mint,
                beneficiary,
                amount,
            } => self.lots.request_unstake(*mint, *beneficiary, *amount),
            EventKind::CancelUnstake {
                mint,
                beneficiary,
                amount,
            } => self.lots.cancel_unstake(*mint, *beneficiary, *amount),
            EventKind::WithdrawUnstaked {
                custody,
                destination,
                beneficiary,
                amount,
            } => {
                let mint = self
                    .mint_of_account(custody)
                    .ok_or(EligibilityError::InvariantViolation("unknown account"))?;
                self.lots.withdraw_unstaked(mint, *beneficiary, *amount)?;
                let (price, basis) = self.acquisition(&mint, ts)?;
                self.credit_anchored(mint, *destination, *amount, price, basis)
            }
            _ => Ok(()),
        }
    }

    /// Registration pricing applies ONCE per holder, at the wallet's FIRST
    /// registration event for the asset:
    /// * at or before the epoch start the units are legitimately
    ///   reference-priced — the holder is merely anchored, so a later
    ///   registration can never reprice them;
    /// * after the reference timestamp the wallet's reference lots re-base
    ///   to `min(reference, verified at the event)` (section 5: "added
    ///   **or registered** after it"); unpriced when no usable observation
    ///   exists (test 30).
    ///
    /// Re-registrations (exit + register, duplicates) and registrations
    /// after a verified reset are no-ops for already-anchored holders: a
    /// later registration event never reprices established lots.
    fn registration_rebase(
        &mut self,
        wallet: &Addr,
        asset: AssetKind,
        ts: i64,
    ) -> EligibilityResult<()> {
        let Some(adapter) = self.config.adapter(asset) else {
            return Ok(());
        };
        let underlying = adapter.underlying_mint();
        let Some(commit) = self.references.get(&PriceKey::Mint(underlying)) else {
            return Ok(());
        };
        let mut holders: Vec<Addr> = self
            .ledger
            .accounts
            .iter()
            .filter(|(_, acc)| {
                &acc.owner == wallet && acc.mint == underlying && !adapter.is_custody(&acc.owner)
            })
            .map(|(a, _)| *a)
            .collect();
        holders.push(*wallet); // stake beneficiary bucket
        if ts <= commit.epoch_start {
            self.lots.mark_registered(underlying, &holders);
            return Ok(());
        }
        let reference = self
            .price(&PriceKey::Mint(underlying))
            .unwrap_or(commit.price);
        let reg_price = match self.observation_for(&underlying, ts) {
            None => None,
            Some(obs) => Some(min_price(&reference, &obs)?),
        };
        self.lots.rebase_registered(underlying, &holders, reg_price)
    }

    /// Align lots with an adopted checkpoint: observed amounts replace the
    /// replayed ones; unexplained post-reference balances become unpriced
    /// additions (fail closed), pre-reference ones reference-priced.
    fn reconcile_lots(
        &mut self,
        accounts: &[(Addr, u64, Addr)],    // (account, amount, mint)
        stakes: &[(Addr, Addr, u64, u64)], // (mint, beneficiary, active, pending)
        ts: i64,
    ) -> EligibilityResult<()> {
        for (account, amount, mint) in accounts {
            let cur = self.lots.units(*mint, *account);
            if *amount > cur {
                let (price, basis) = self.acquisition(mint, ts)?;
                self.credit_anchored(*mint, *account, *amount - cur, price, basis)?;
            } else if cur > *amount {
                self.lots.consume(*mint, *account, cur - *amount)?;
            }
        }
        for (mint, beneficiary, active, pending) in stakes {
            let cur = self.lots.units(*mint, *beneficiary);
            if *active > cur {
                let (price, basis) = self.acquisition(mint, ts)?;
                self.credit_beneficiary_anchored(
                    *mint,
                    *beneficiary,
                    *active - cur,
                    price,
                    basis,
                    false,
                )?;
            } else if cur > *active {
                self.lots.consume(*mint, *beneficiary, cur - *active)?;
            }
            let cur_pending = self.lots.pending_units(*mint, *beneficiary);
            if *pending > cur_pending {
                let (price, basis) = self.acquisition(mint, ts)?;
                self.credit_beneficiary_anchored(
                    *mint,
                    *beneficiary,
                    *pending - cur_pending,
                    price,
                    basis,
                    true,
                )?;
            } else if cur_pending > *pending {
                self.lots
                    .consume_pending(*mint, *beneficiary, cur_pending - *pending)?;
            }
        }
        Ok(())
    }

    fn apply(&mut self, ev: &Event) -> EligibilityResult<()> {
        let ts = ev.unix_timestamp;
        match &ev.kind {
            EventKind::Register { wallet, asset } => {
                // Epoch-level selection gating (Pass #6, P6-D3). First
                // activation of the epoch, or re-activation of the epoch's
                // anchored asset (a duplicate, or a rejoin after an exit):
                // immediate — "a later join accrues only from its
                // finalized registration" (§5), and the rebase is a no-op
                // for already-anchored holders (Pass #5). A registration
                // naming a DIFFERENT asset is a switch: effective next
                // epoch, never within this one.
                match self.anchors.get(wallet) {
                    None => {
                        self.anchors.insert(*wallet, *asset);
                        self.registrations.insert(*wallet, *asset);
                        self.registration_rebase(wallet, *asset, ts)?;
                    }
                    Some(&anchored) if anchored == *asset => {
                        self.registrations.insert(*wallet, *asset);
                        self.registration_rebase(wallet, *asset, ts)?;
                    }
                    Some(_) => {
                        self.deferred.insert(*wallet, *asset);
                    }
                }
            }
            EventKind::Exit { wallet } => {
                // Exiting is always permitted and ends eligibility
                // immediately (§5). The epoch's anchor SURVIVES the exit,
                // so a later same-epoch registration of a different asset
                // is still a deferred switch: exit + re-register must not
                // permit an immediate same-epoch switch (P6-D3).
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
            EventKind::PriceObserved { mint, price } => {
                // Only basis-correct, well-formed observations are
                // recorded (with the finalized time they were observed
                // at). An INVALID newer observation (wrong basis, zero
                // denominator) does not leave an older valid reading
                // usable: it poisons the mint's cached entry, so
                // additions and late registrations price from nothing
                // (PENDING) until a fresh valid observation arrives (F2-A,
                // fail closed). Freshness is enforced at use: an
                // observation outside the adapter's calibrated window is
                // stale and unusable.
                if let Some(adapter) = self.config.adapter_of_mint(mint) {
                    if price.basis == adapter.asset.required_basis() && price.units_den != 0 {
                        self.observations.insert(*mint, (*price, ts));
                    } else {
                        self.observations.remove(mint);
                    }
                }
            }
            EventKind::EmergencyNav { mint, price } => {
                self.apply_reset(*mint, *price)?;
            }
            EventKind::RouteHalt { mint } => {
                if self.config.adapter_of_mint(mint).is_some() {
                    self.halted_mints.insert(*mint);
                }
            }
            EventKind::RouteResumed { mint, price } => {
                if self.config.adapter_of_mint(mint).is_some() {
                    self.halted_mints.remove(mint);
                    self.apply_reset(*mint, *price)?;
                }
            }
            EventKind::DeclaredGap => self.in_sync = false,
            EventKind::Checkpoint { .. } => unreachable!("handled by replay"),
            token => {
                if self.in_sync {
                    let before_ledger = self.ledger.clone();
                    let before_lots = self.lots.clone();
                    let ok = self.ledger.apply(token).is_ok()
                        && self.apply_lots(token, ts).is_ok()
                        && self.stakes_backed();
                    if ok {
                        self.ledger.check_invariants()?;
                    } else {
                        self.ledger = before_ledger;
                        self.lots = before_lots;
                        self.in_sync = false;
                    }
                }
            }
        }
        Ok(())
    }

    /// A verified reset (emergency NAV or published recovery reference):
    /// from this event the mint prices at `price`, prospectively only
    /// (settled segments were already committed). Wrong basis = unusable
    /// datum: recognition pauses (fail closed).
    fn apply_reset(&mut self, mint: Addr, price: UnitPrice) -> EligibilityResult<()> {
        if price.units_den == 0 {
            self.nav_invalid.insert(mint);
            return Ok(());
        }
        match self.config.adapter_of_mint(&mint) {
            Some(adapter) if price.basis == adapter.asset.required_basis() => {
                self.nav_invalid.remove(&mint);
                self.active.insert(PriceKey::Mint(mint), price);
                self.lots.rebase_mint(mint, price)
            }
            _ => {
                self.nav_invalid.insert(mint);
                Ok(())
            }
        }
    }
}

fn dt(start: i64, end: i64) -> EligibilityResult<u64> {
    u64::try_from(end.saturating_sub(start).max(0)).map_err(|_| EligibilityError::MathOverflow)
}

/// Replay `events` and integrate eligibility up to `end` (exclusive).
/// `references` are the committed epoch reference prices (section 5).
///
/// One replay is ONE epoch's settlement context (Pass #6, P6-D7): no
/// selection state comes in, so every wallet's first `Register` of the
/// timeline is its first activation of the epoch (immediate). Chained
/// epochs use [`replay_epoch`] with the previous epoch's
/// [`EpochCarry`].
pub fn replay(
    config: &EligibilityConfig,
    references: &EpochReferences,
    events: Vec<Event>,
    end: i64,
) -> EligibilityResult<ReplayReport> {
    Ok(replay_inner(config, references, EpochCarry::default(), events, end)?.report)
}

/// Selection and Safe-Mode state that crosses an epoch boundary (Pass #6,
/// P6-D3 / P6-D7). Holdings, debt and seed counts also carry across epochs,
/// but they do so as chain state re-observed by the next epoch's
/// checkpoints — lot history is never mutated across epochs (P6-D7).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EpochCarry {
    /// The selection in force for each wallet at the boundary: its
    /// deferred switch (requested last epoch, effective now) or, failing
    /// that, its still-active selection. Seeded as both the active
    /// selection and the epoch's anchor.
    pub selections: BTreeMap<Addr, AssetKind>,
    /// Mints whose required route was halted when the epoch ended. A halt
    /// persists across the boundary until a `RouteResumed` event lifts it
    /// (P6-D7); it is not erased by a new reference commit.
    pub halted_mints: BTreeSet<Addr>,
    /// Emergency/reset price overrides still in force at the boundary.
    /// Superseded here by the new epoch's committed reference for the same
    /// key (P6-D7: "until superseded by a later committed reference").
    pub active_prices: BTreeMap<PriceKey, UnitPrice>,
}

/// The result of one epoch's replay: its report plus the state to carry
/// into the next epoch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpochReplay {
    pub report: ReplayReport,
    pub carry: EpochCarry,
}

/// Replay ONE epoch of a chained sequence with the previous epoch's
/// [`EpochCarry`] (Pass #6). The carried selections are active from the
/// boundary — a switch requested last epoch is effective now, with no
/// registration event of its own — and anchor pre-reference credits exactly
/// like a registration at the boundary would. Carried route halts stay in
/// force; carried reset prices survive only where the new epoch did not
/// commit a fresh reference.
pub fn replay_epoch(
    config: &EligibilityConfig,
    references: &EpochReferences,
    carry: EpochCarry,
    events: Vec<Event>,
    end: i64,
) -> EligibilityResult<EpochReplay> {
    replay_inner(config, references, carry, events, end)
}

fn replay_inner(
    config: &EligibilityConfig,
    references: &EpochReferences,
    carry: EpochCarry,
    events: Vec<Event>,
    end: i64,
) -> EligibilityResult<EpochReplay> {
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
    // Carried selections are both the active selection and the epoch's
    // anchor: this epoch can no longer switch them. Carried halts apply
    // only to mints a configured adapter still claims. A carried reset
    // price survives only where this epoch's references did not supersede
    // it with a fresh committed reference.
    let mut st = State {
        config,
        references,
        ledger: Ledger::default(),
        in_sync: true,
        registrations: carry.selections.clone(),
        anchors: carry.selections,
        deferred: BTreeMap::new(),
        debt,
        unknown_assets,
        observed_versions: BTreeMap::new(),
        active: carry
            .active_prices
            .into_iter()
            .filter(|(k, _)| references.get(k).is_none())
            .collect(),
        observations: BTreeMap::new(),
        halted_mints: carry
            .halted_mints
            .into_iter()
            .filter(|m| config.adapter_of_mint(m).is_some())
            .collect(),
        nav_invalid: BTreeSet::new(),
        lots: LotLedger::default(),
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
            // Ownership before adoption: an account whose observed owner
            // differs was transferred inside the gap, and the prior
            // owner's registration anchor must not survive the adoption
            // (F1-A, checkpoint path).
            let prev_owners: BTreeMap<Addr, Addr> = st
                .ledger
                .accounts
                .iter()
                .map(|(a, acc)| (*a, acc.owner))
                .collect();
            st.ledger = observed;
            // Observed state is trusted only if it is itself coherent.
            st.in_sync = st.ledger.conserved() && st.stakes_backed();
            // Lots follow the adopted state; unexplained post-reference
            // balances become unpriced additions (reserved, never free).
            let account_triples: Vec<(Addr, u64, Addr)> = accounts
                .iter()
                .map(|a| (a.account, a.amount, a.mint))
                .collect();
            let stake_quads: Vec<(Addr, Addr, u64, u64)> = stakes
                .iter()
                .map(|s| (s.mint, s.beneficiary, s.active, s.pending_unstake))
                .collect();
            st.reconcile_lots(&account_triples, &stake_quads, ts)?;
            // Re-acquire every account whose owner changed in the gap,
            // replaying exactly what a live `SetAccountOwner` at the
            // checkpoint would do: consume every lot, drop the prior
            // owner's anchor, and re-credit the units at the checkpoint
            // acquisition price (min rule; the recipient re-anchors only
            // as the live credit path would). Checkpoint adoption and the
            // equivalent live event must leave economically identical
            // state — the observed owner is a NEW holder acquiring the
            // account, never an inheritor of the prior owner's reference
            // lots (F1-A, checkpoint path).
            for a in accounts {
                if let Some(prev) = prev_owners.get(&a.account) {
                    if *prev != a.owner {
                        let units = st.lots.units(a.mint, a.account);
                        st.lots.consume(a.mint, a.account, units)?;
                        st.lots.clear_registered(a.mint, a.account);
                        if units > 0 {
                            let (price, basis) = st.acquisition(&a.mint, ts)?;
                            st.credit_anchored(a.mint, a.account, units, price, basis)?;
                        }
                    }
                }
            }
        } else {
            st.apply(ev)?;
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

    // Carry out: a deferred switch wins over the still-active selection
    // (it was requested later); halts and surviving reset prices persist
    // into the next epoch (P6-D7).
    let mut selections = st.deferred.clone();
    for (w, a) in &st.registrations {
        selections.entry(*w).or_insert(*a);
    }
    let out = EpochCarry {
        selections,
        halted_mints: st.halted_mints.clone(),
        active_prices: st.active.clone(),
    };
    Ok(EpochReplay { report, carry: out })
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
