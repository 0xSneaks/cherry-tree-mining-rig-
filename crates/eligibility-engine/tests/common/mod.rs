//! Test harness. Every address here is a FIXTURE (`Addr::fixture`), never a
//! production mint, program or wallet.
#![allow(dead_code)]

use std::collections::BTreeSet;

use eligibility_engine::*;

pub const SKR_MINT: Addr = Addr::fixture(1);
pub const XSOL_MINT: Addr = Addr::fixture(2);
pub const STAKE_CUSTODY_OWNER: Addr = Addr::fixture(3);
pub const LENDER_A: Addr = Addr::fixture(4); // fixture registry program
pub const LENDER_B: Addr = Addr::fixture(5); // fixture registry program
pub const OUTSIDE_LENDER: Addr = Addr::fixture(6); // NOT in the registry
pub const XSOL_PROGRAM: Addr = Addr::fixture(7);
pub const RECEIPT_MINT: Addr = Addr::fixture(8);

pub const A: Addr = Addr::fixture(0x41);
pub const B: Addr = Addr::fixture(0x42);
pub const C: Addr = Addr::fixture(0x43);
pub const DELEGATE: Addr = Addr::fixture(0x44);

/// Token account of `wallet` for `mint` (deterministic fixture address).
pub fn ta(wallet: Addr, mint: Addr) -> Addr {
    let mut b = wallet.0;
    b[1] = mint.0[31];
    b[2] = 0xAC;
    Addr(b)
}

pub const CUSTODY: Addr = Addr::fixture(0x60);

pub const V1: [u8; 32] = [1; 32];
pub const V2: [u8; 32] = [2; 32];

pub fn price(num: u128, den: u128) -> UnitPrice {
    UnitPrice {
        lamports_num: num,
        units_den: den,
        basis: PriceBasis::Reference,
    }
}

/// Fixture epoch boundary for Pass #4-style worlds: every timeline in those
/// tests runs entirely *before* the epoch start, so all units are
/// reference-priced and no in-epoch observations are needed. Pass #5 tests
/// build explicit references with real epoch starts.
pub const PRE_REFERENCE_EPOCH_START: i64 = i64::MAX / 2;

/// 1 SKR unit = 2 lamports, xSOL NAV 3 lamports per unit.
pub fn refs() -> EpochReferences {
    EpochReferences::new()
        .with(ref_commit(SKR_MINT, price(2, 1)))
        .with(ref_commit(
            XSOL_MINT,
            UnitPrice {
                lamports_num: 3,
                units_den: 1,
                basis: PriceBasis::OwnedTokenNav,
            },
        ))
}

/// Reference committed at time 0 for a fixture epoch starting at
/// `PRE_REFERENCE_EPOCH_START`.
pub fn ref_commit(mint: Addr, price: UnitPrice) -> ReferenceCommit {
    ReferenceCommit::new(PriceKey::Mint(mint), price, 0, PRE_REFERENCE_EPOCH_START).unwrap()
}

/// Reference committed at time 0 for a fixture epoch starting at
/// `epoch_start` (Pass #5 in-epoch timelines).
pub fn ref_commit_at(mint: Addr, price: UnitPrice, epoch_start: i64) -> ReferenceCommit {
    ReferenceCommit::new(PriceKey::Mint(mint), price, 0, epoch_start).unwrap()
}

/// Owned-token NAV unit price (xSOL basis).
pub fn nav(num: u128, den: u128) -> UnitPrice {
    UnitPrice {
        lamports_num: num,
        units_den: den,
        basis: PriceBasis::OwnedTokenNav,
    }
}

/// Fixture observation-freshness bound (seconds) for the shared adapters —
/// NOT a production calibration (gate 14.2-D). Large enough that the shared
/// fixture timelines' observation-to-event gaps are always fresh; tests about
/// staleness itself construct tighter bounds via [`skr_adapter_max_age`].
pub const FIXTURE_OBSERVATION_MAX_AGE: i64 = 10_000;

/// xSOL adapter pinned to `XSOL_PROGRAM` at `V1`.
pub fn xsol_adapter() -> AdapterConfig {
    let mut a = AdapterConfig::xsol(
        XSOL_MINT,
        TokenProgram::SplToken,
        vec![ProgramPin {
            program: XSOL_PROGRAM,
            version: V1,
        }],
    );
    a.observation_max_age = Some(FIXTURE_OBSERVATION_MAX_AGE);
    a
}

pub fn skr_adapter(stake: StakeSupport) -> AdapterConfig {
    skr_adapter_max_age(stake, Some(FIXTURE_OBSERVATION_MAX_AGE))
}

/// SKR adapter with an explicit observation-freshness bound (`None` =
/// uncalibrated: every observation is stale, fail closed).
pub fn skr_adapter_max_age(stake: StakeSupport, max_age: Option<i64>) -> AdapterConfig {
    let mut a = AdapterConfig::skr(SKR_MINT, TokenProgram::SplToken, stake, vec![]);
    a.observation_max_age = max_age;
    a
}

pub fn verified_stake() -> StakeSupport {
    StakeSupport::Verified {
        custody_owners: BTreeSet::from([STAKE_CUSTODY_OWNER]),
    }
}

pub fn registry(entries: &[Addr]) -> DebtRegistry {
    DebtRegistry::new(
        entries
            .iter()
            .map(|p| RegistryEntry {
                pin: ProgramPin {
                    program: *p,
                    version: V1,
                },
            })
            .collect(),
    )
    .unwrap()
}

pub fn config(adapters: Vec<AdapterConfig>, reg: &[Addr]) -> EligibilityConfig {
    EligibilityConfig::new(adapters, registry(reg)).unwrap()
}

/// Event builder: explicit slots and timestamps, automatic tx/ix indices.
pub struct Tl {
    pub events: Vec<Event>,
    tx: u32,
    ix: u32,
    slot: u64,
    ts: i64,
}

impl Tl {
    pub fn new() -> Self {
        Self {
            events: vec![],
            tx: 0,
            ix: 0,
            slot: 0,
            ts: 0,
        }
    }

    /// Move to a new slot with its clock time. Starts a new transaction.
    pub fn slot(&mut self, slot: u64, ts: i64) -> &mut Self {
        self.slot = slot;
        self.ts = ts;
        self.tx = 0;
        self.ix = 0;
        self
    }

    /// Start a new transaction in the current slot.
    pub fn tx(&mut self) -> &mut Self {
        self.tx += 1;
        self.ix = 0;
        self
    }

    pub fn ev(&mut self, kind: EventKind) -> &mut Self {
        self.events.push(Event {
            key: EventKey::new(self.slot, self.tx, self.ix, 0),
            unix_timestamp: self.ts,
            kind,
        });
        self.ix += 1;
        self
    }

    pub fn init(&mut self, wallet: Addr, mint: Addr) -> &mut Self {
        self.ev(EventKind::InitializeAccount {
            account: ta(wallet, mint),
            mint,
            owner: wallet,
        })
    }

    pub fn mint(&mut self, wallet: Addr, mint: Addr, amount: u64) -> &mut Self {
        self.ev(EventKind::MintTo {
            account: ta(wallet, mint),
            amount,
        })
    }

    pub fn send(&mut self, from: Addr, to: Addr, mint: Addr, amount: u64) -> &mut Self {
        self.ev(EventKind::Transfer {
            source: ta(from, mint),
            destination: ta(to, mint),
            amount,
            authority: Authority::Owner,
        })
    }

    pub fn register(&mut self, wallet: Addr, asset: AssetKind) -> &mut Self {
        self.ev(EventKind::Register { wallet, asset })
    }

    /// Observe `programs` at their pinned fixture version `V1`.
    pub fn pinned(&mut self, programs: &[Addr]) -> &mut Self {
        for p in programs {
            self.ev(EventKind::ProgramVersion {
                program: *p,
                version: V1,
            });
        }
        self
    }

    pub fn no_debt(&mut self, wallet: Addr, programs: &[Addr]) -> &mut Self {
        for p in programs {
            self.ev(EventKind::DebtObserved {
                wallet,
                program: *p,
                debts: vec![],
            });
        }
        self
    }

    pub fn debt(&mut self, wallet: Addr, program: Addr, lamports: u64) -> &mut Self {
        self.ev(EventKind::DebtObserved {
            wallet,
            program,
            debts: vec![DebtAmount {
                price_key: PriceKey::Sol,
                units: lamports,
            }],
        })
    }

    /// A checkpoint that observes exactly the state `snap` describes.
    pub fn checkpoint(
        &mut self,
        accounts: Vec<AccountSnapshot>,
        stakes: Vec<StakeSnapshot>,
        supplies: Vec<SupplySnapshot>,
    ) -> &mut Self {
        self.ev(EventKind::Checkpoint {
            accounts,
            stakes,
            supplies,
        })
    }

    /// Verified in-epoch price observation (already validated upstream).
    pub fn observe(&mut self, mint: Addr, price: UnitPrice) -> &mut Self {
        self.ev(EventKind::PriceObserved { mint, price })
    }

    /// Verified NAV collapse / depeg reset from this event.
    pub fn emergency_nav(&mut self, mint: Addr, price: UnitPrice) -> &mut Self {
        self.ev(EventKind::EmergencyNav { mint, price })
    }

    /// Verified loss of the required operational route.
    pub fn route_halt(&mut self, mint: Addr) -> &mut Self {
        self.ev(EventKind::RouteHalt { mint })
    }

    /// Route recovery with a published prospective reference segment.
    pub fn route_resumed(&mut self, mint: Addr, price: UnitPrice) -> &mut Self {
        self.ev(EventKind::RouteResumed { mint, price })
    }
}

/// Plain (undelegated, unfrozen) account snapshot.
pub fn snap(wallet: Addr, mint: Addr, amount: u64) -> AccountSnapshot {
    AccountSnapshot {
        account: ta(wallet, mint),
        mint,
        owner: wallet,
        amount,
        delegate: None,
        delegated_amount: 0,
        frozen: false,
    }
}

pub fn supply(mint: Addr, supply: u64) -> SupplySnapshot {
    SupplySnapshot { mint, supply }
}

/// Checkpoint built from the replayed ledger itself (a truthful indexer).
pub fn truthful_checkpoint(events: &[Event]) -> EventKind {
    let mut l = Ledger::default();
    for e in order_events(events.to_vec()).unwrap() {
        match &e.kind {
            EventKind::Checkpoint {
                accounts,
                stakes,
                supplies,
            } => l = l.from_snapshot(accounts, stakes, supplies),
            k => {
                let _ = l.apply(k);
            }
        }
    }
    EventKind::Checkpoint {
        accounts: l
            .accounts
            .iter()
            .map(|(k, a)| AccountSnapshot {
                account: *k,
                mint: a.mint,
                owner: a.owner,
                amount: a.amount,
                delegate: a.delegate,
                delegated_amount: a.delegated_amount,
                frozen: a.frozen,
            })
            .collect(),
        stakes: l
            .stakes
            .iter()
            .map(|((m, b), p)| StakeSnapshot {
                mint: *m,
                beneficiary: *b,
                active: p.active,
                pending_unstake: p.pending_unstake,
            })
            .collect(),
        supplies: l
            .supplies
            .iter()
            .map(|(m, s)| SupplySnapshot {
                mint: *m,
                supply: *s,
            })
            .collect(),
    }
}

impl Tl {
    /// Append a checkpoint that truthfully observes the replayed state.
    pub fn close(&mut self) -> &mut Self {
        let k = truthful_checkpoint(&self.events);
        self.ev(k)
    }
}

pub fn run(cfg: &EligibilityConfig, tl: &Tl, end: i64) -> ReplayReport {
    replay(cfg, &refs(), tl.events.clone(), end).unwrap()
}

/// Run with explicit epoch references (Pass #5 in-epoch timelines).
pub fn run_with(
    cfg: &EligibilityConfig,
    refs: &EpochReferences,
    tl: &Tl,
    end: i64,
) -> ReplayReport {
    replay(cfg, refs, tl.events.clone(), end).unwrap()
}

pub fn x_at(r: &ReplayReport, wallet: &Addr, t: i64) -> Option<Eligibility> {
    r.segments
        .iter()
        .find(|s| s.start <= t && t < s.end)
        .and_then(|s| s.wallets.get(wallet).copied())
}

pub fn verified_x(e: Option<Eligibility>) -> u128 {
    match e {
        Some(Eligibility::Verified { x_lamports, .. }) => x_lamports,
        other => panic!("expected VERIFIED, got {other:?}"),
    }
}
