//! Property / adversarial coverage for the ownership timeline and equity
//! invariants. Fixture addresses only.

mod common;

use common::*;
use eligibility_engine::*;
use proptest::prelude::*;

const SUPPLY: u64 = 10_000;
const WALLETS: [Addr; 3] = [A, B, C];

#[derive(Clone, Debug)]
enum Op {
    Send {
        from: usize,
        to: usize,
        amount: u64,
    },
    SetOwner {
        acct: usize,
        to: usize,
    },
    Approve {
        acct: usize,
        amount: u64,
    },
    Revoke {
        acct: usize,
    },
    Freeze {
        acct: usize,
    },
    Thaw {
        acct: usize,
    },
    Debt {
        wallet: usize,
        lamports: u64,
        registry: bool,
    },
    /// A same-transaction carousel through every wallet and back.
    Carousel {
        start: usize,
        amount: u64,
    },
    /// Advances the clock, emits nothing.
    Noop,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0..3usize, 0..3usize, 0..3_000u64).prop_map(|(from, to, amount)| Op::Send {
            from,
            to,
            amount
        }),
        (0..3usize, 0..3usize).prop_map(|(acct, to)| Op::SetOwner { acct, to }),
        (0..3usize, 0..SUPPLY).prop_map(|(acct, amount)| Op::Approve { acct, amount }),
        (0..3usize).prop_map(|acct| Op::Revoke { acct }),
        (0..3usize).prop_map(|acct| Op::Freeze { acct }),
        (0..3usize).prop_map(|acct| Op::Thaw { acct }),
        (0..3usize, 0..30_000u64, any::<bool>()).prop_map(|(wallet, lamports, registry)| {
            Op::Debt {
                wallet,
                lamports,
                registry,
            }
        }),
        (0..3usize, 1..5_000u64).prop_map(|(start, amount)| Op::Carousel { start, amount }),
    ]
}

/// Account i is initially wallet i's; `SetOwner` may move it.
fn acct(i: usize) -> Addr {
    ta(WALLETS[i], SKR_MINT)
}

/// Build a timeline from ops, emitting only events the token program would
/// accept (a shadow ledger filters the rest), so replay must stay in sync.
/// `dt[i]` is the clock step before op i; 0 means "same timestamp, new slot".
fn build(ops: &[(Op, i64)], with_carousels: bool) -> Tl {
    let mut t = Tl::new();
    t.slot(1, 1_000);
    for w in WALLETS {
        t.init(w, SKR_MINT);
    }
    t.mint(A, SKR_MINT, SUPPLY).pinned(&[LENDER_A]);
    for w in WALLETS {
        t.register(w, AssetKind::Skr).no_debt(w, &[LENDER_A]);
    }
    let mut shadow = Ledger::default();
    for e in &t.events {
        shadow.apply(&e.kind).unwrap();
    }
    let (mut slot, mut ts) = (1u64, 1_000i64);
    for (op, step) in ops {
        slot += 1;
        ts += *step;
        t.slot(slot, ts);
        let kinds: Vec<EventKind> = match op {
            Op::Send { from, to, amount } => vec![EventKind::Transfer {
                source: acct(*from),
                destination: acct(*to),
                amount: *amount,
                authority: Authority::Owner,
            }],
            Op::SetOwner { acct: a, to } => vec![EventKind::SetAccountOwner {
                account: acct(*a),
                new_owner: WALLETS[*to],
            }],
            Op::Approve { acct: a, amount } => vec![EventKind::Approve {
                account: acct(*a),
                delegate: DELEGATE,
                amount: *amount,
            }],
            Op::Revoke { acct: a } => vec![EventKind::Revoke { account: acct(*a) }],
            Op::Freeze { acct: a } => vec![EventKind::Freeze { account: acct(*a) }],
            Op::Thaw { acct: a } => vec![EventKind::Thaw { account: acct(*a) }],
            Op::Debt {
                wallet,
                lamports,
                registry,
            } => vec![EventKind::DebtObserved {
                wallet: WALLETS[*wallet],
                program: if *registry { LENDER_A } else { OUTSIDE_LENDER },
                debts: vec![DebtAmount {
                    price_key: PriceKey::Sol,
                    units: *lamports,
                }],
            }],
            Op::Noop => continue,
            Op::Carousel { start, amount } => {
                if !with_carousels {
                    continue;
                }
                let s = *start;
                (0..3)
                    .map(|k| EventKind::Transfer {
                        source: acct((s + k) % 3),
                        destination: acct((s + k + 1) % 3),
                        amount: *amount,
                        authority: Authority::Owner,
                    })
                    .collect()
            }
        };
        // A carousel is all-or-nothing, like a transaction.
        let mut trial = shadow.clone();
        if kinds.iter().all(|k| trial.apply(k).is_ok()) {
            shadow = trial;
            for k in kinds {
                t.ev(k);
            }
        }
    }
    t.slot(slot + 1, ts + 1_000).close();
    t
}

fn cfg() -> EligibilityConfig {
    config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn timeline_invariants(ops in prop::collection::vec((op(), 0..400i64), 1..40)) {
        let t = build(&ops, true);
        let r = run(&cfg(), &t, i64::MAX / 4);
        // A truthful history never diverges: everything verifies.
        prop_assert!(r.pool.iter().all(|p| p.status != IntervalStatus::Unresolved));
        let start = 1_000i64;
        let end = t.events.last().unwrap().unix_timestamp;
        let mut total_unit_seconds: u128 = 0;
        for s in r.segments.iter().filter(|s| s.status == IntervalStatus::Verified) {
            let mut units_now: u128 = 0;
            for e in s.wallets.values() {
                if let Eligibility::Verified { units, gross_lamports, debt_lamports, x_lamports } = *e {
                    // X = max(0, G - D) >= 0, exactly.
                    prop_assert_eq!(x_lamports, gross_lamports.saturating_sub(debt_lamports));
                    prop_assert_eq!(gross_lamports, 2 * units as u128);
                    units_now += units as u128;
                }
            }
            // No unit belongs to two owners at one instant.
            prop_assert!(units_now <= SUPPLY as u128);
            total_unit_seconds += units_now * (s.end - s.start) as u128;
        }
        prop_assert!(total_unit_seconds <= SUPPLY as u128 * (end - start) as u128);
        let sum: u128 = WALLETS.iter().map(|w| r.wallet(w).unit_seconds).sum();
        prop_assert_eq!(sum, total_unit_seconds);
    }

    /// Same-transaction carousels add zero duration for everyone.
    #[test]
    fn carousels_create_no_duration(ops in prop::collection::vec((op(), 0..400i64), 1..30)) {
        let with = run(&cfg(), &build(&ops, true), i64::MAX / 4);
        let without = run(&cfg(), &build(&ops, false), i64::MAX / 4);
        for w in WALLETS {
            prop_assert_eq!(with.wallet(&w).unit_seconds, without.wallet(&w).unit_seconds);
            prop_assert_eq!(with.wallet(&w).equity_seconds, without.wallet(&w).equity_seconds);
        }
    }

    /// Non-registry debt never changes anything; registry debt only lowers X.
    #[test]
    fn only_registry_debt_is_netted(ops in prop::collection::vec((op(), 0..400i64), 1..30)) {
        let strip = |registry_only: bool| -> Vec<(Op, i64)> {
            ops.iter()
                .map(|(o, d)| match o {
                    Op::Debt { registry, .. } if !*registry || !registry_only => (Op::Noop, *d),
                    other => (other.clone(), *d),
                })
                .collect()
        };
        let all = run(&cfg(), &build(&ops, true), i64::MAX / 4);
        let registry_only = run(&cfg(), &build(&strip(true), true), i64::MAX / 4);
        let none = run(&cfg(), &build(&strip(false), true), i64::MAX / 4);
        for w in WALLETS {
            prop_assert_eq!(all.wallet(&w).equity_seconds, registry_only.wallet(&w).equity_seconds);
            prop_assert!(all.wallet(&w).equity_seconds <= none.wallet(&w).equity_seconds);
            prop_assert_eq!(all.wallet(&w).unit_seconds, none.wallet(&w).unit_seconds);
        }
    }

    /// A wallet whose registry state was never read never verifies.
    #[test]
    fn missing_registry_state_never_verifies(ops in prop::collection::vec((op(), 0..400i64), 1..20)) {
        let mut t = build(&ops, true);
        t.events.retain(|e| !matches!(&e.kind, EventKind::DebtObserved { wallet, .. } if *wallet == C));
        // Keep the closing checkpoint valid: rebuild it.
        t.events.pop();
        let last = t.events.last().unwrap().clone();
        t.slot(last.key.slot + 1, last.unix_timestamp + 1_000).close();
        let r = run(&cfg(), &t, i64::MAX / 4);
        prop_assert_eq!(r.wallet(&C).equity_seconds, 0);
        for s in &r.segments {
            let verified = matches!(s.wallets.get(&C), Some(Eligibility::Verified { .. }));
            prop_assert!(!verified);
        }
    }
}

#[test]
fn repeated_a_b_a_cycles_split_time_exactly() {
    let mut t = Tl::new();
    t.slot(1, 0)
        .init(A, SKR_MINT)
        .init(B, SKR_MINT)
        .mint(A, SKR_MINT, SUPPLY)
        .pinned(&[LENDER_A]);
    for w in [A, B] {
        t.register(w, AssetKind::Skr).no_debt(w, &[LENDER_A]);
    }
    let mut ts = 0;
    for i in 0..50u64 {
        ts += 10;
        t.slot(2 + 2 * i, ts).send(A, B, SKR_MINT, SUPPLY);
        ts += 30;
        t.slot(3 + 2 * i, ts).send(B, A, SKR_MINT, SUPPLY);
    }
    t.slot(1_000, ts).close();
    let r = run(&cfg(), &t, ts);
    assert_eq!(r.wallet(&A).unit_seconds, SUPPLY as u128 * 10 * 50);
    assert_eq!(r.wallet(&B).unit_seconds, SUPPLY as u128 * 30 * 50);
}

#[test]
fn arithmetic_overflow_is_an_error_never_a_wrap() {
    let mut t = Tl::new();
    t.slot(1, 0)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, u64::MAX)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::Skr)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 10).close();
    let huge = EpochReferences::new().with(ref_commit(SKR_MINT, price(u128::MAX, 1)));
    assert_eq!(
        replay(&cfg(), &huge, t.events.clone(), 10).unwrap_err(),
        EligibilityError::MathOverflow
    );
    // Minting past u64::MAX is impossible on chain: divergence, not a wrap.
    t.slot(3, 20).mint(A, SKR_MINT, 1);
    t.slot(4, 30).close();
    let r = replay(&cfg(), &refs(), t.events, 30).unwrap();
    assert!(r
        .pool
        .iter()
        .any(|p| p.status == IntervalStatus::Unresolved));
    // Out-of-order clock is refused outright.
    let mut bad = Tl::new();
    bad.slot(1, 100).init(A, SKR_MINT);
    bad.slot(2, 99).init(B, SKR_MINT);
    assert_eq!(
        replay(&cfg(), &refs(), bad.events, 200).unwrap_err(),
        EligibilityError::NonMonotonicTimestamp
    );
}

#[test]
fn delegate_spend_reduces_delegation_and_clears_it_at_zero() {
    let mut t = Tl::new();
    t.slot(1, 0)
        .init(A, SKR_MINT)
        .init(B, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::Skr)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 100).ev(EventKind::Approve {
        account: ta(A, SKR_MINT),
        delegate: DELEGATE,
        amount: 300,
    });
    t.slot(3, 200).ev(EventKind::Transfer {
        source: ta(A, SKR_MINT),
        destination: ta(B, SKR_MINT),
        amount: 300,
        authority: Authority::Delegate,
    });
    t.slot(4, 300).close();
    let r = run(&cfg(), &t, 300);
    assert_eq!(verified_x(x_at(&r, &A, 150)), 1_400); // 700 recognized
    assert_eq!(verified_x(x_at(&r, &A, 250)), 1_400); // 700 left, delegation spent
                                                      // Over-spending a delegation is impossible on chain.
    let mut t2 = Tl::new();
    t2.slot(1, 0)
        .init(A, SKR_MINT)
        .init(B, SKR_MINT)
        .mint(A, SKR_MINT, 1_000);
    t2.slot(2, 100).ev(EventKind::Approve {
        account: ta(A, SKR_MINT),
        delegate: DELEGATE,
        amount: 10,
    });
    t2.slot(3, 200).ev(EventKind::Transfer {
        source: ta(A, SKR_MINT),
        destination: ta(B, SKR_MINT),
        amount: 11,
        authority: Authority::Delegate,
    });
    t2.slot(4, 300).close();
    assert_eq!(
        run(&cfg(), &t2, 300).pool[0].status,
        IntervalStatus::Unresolved
    );
}

#[derive(Clone, Debug)]
enum StakeOp {
    Stake { who: usize, amount: u64 },
    Request { who: usize, amount: u64 },
    Cancel { who: usize, amount: u64 },
    Withdraw { who: usize, amount: u64 },
    Send { from: usize, to: usize, amount: u64 },
}

fn stake_op() -> impl Strategy<Value = StakeOp> {
    let w = 0..2usize;
    let a = 0..4_000u64;
    prop_oneof![
        (w.clone(), a.clone()).prop_map(|(who, amount)| StakeOp::Stake { who, amount }),
        (w.clone(), a.clone()).prop_map(|(who, amount)| StakeOp::Request { who, amount }),
        (w.clone(), a.clone()).prop_map(|(who, amount)| StakeOp::Cancel { who, amount }),
        (w.clone(), a.clone()).prop_map(|(who, amount)| StakeOp::Withdraw { who, amount }),
        (w.clone(), w, a).prop_map(|(from, to, amount)| StakeOp::Send { from, to, amount }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Wallet SKR + verified stake: staked units are counted once (through
    /// the beneficiary position, never also through custody), and pending
    /// unstake is excluded: counted units == supply - pending, always.
    #[test]
    fn skr_wallet_and_stake_counted_once(ops in prop::collection::vec((stake_op(), 0..300i64), 1..40)) {
        let two = [A, B];
        let cfg = config(vec![skr_adapter(verified_stake())], &[LENDER_A]);
        let mut t = Tl::new();
        t.slot(1, 1_000).init(A, SKR_MINT).init(B, SKR_MINT).mint(A, SKR_MINT, SUPPLY);
        t.ev(EventKind::InitializeAccount { account: CUSTODY, mint: SKR_MINT, owner: STAKE_CUSTODY_OWNER });
        t.pinned(&[LENDER_A]);
        for w in two {
            t.register(w, AssetKind::Skr).no_debt(w, &[LENDER_A]);
        }
        let mut shadow = Ledger::default();
        for e in &t.events {
            shadow.apply(&e.kind).unwrap();
        }
        let (mut slot, mut ts) = (1u64, 1_000i64);
        for (op, step) in &ops {
            slot += 1;
            ts += step;
            t.slot(slot, ts);
            let k = match *op {
                StakeOp::Stake { who, amount } => EventKind::Stake { source: ta(two[who], SKR_MINT), custody: CUSTODY, beneficiary: two[who], amount },
                StakeOp::Request { who, amount } => EventKind::RequestUnstake { mint: SKR_MINT, beneficiary: two[who], amount },
                StakeOp::Cancel { who, amount } => EventKind::CancelUnstake { mint: SKR_MINT, beneficiary: two[who], amount },
                StakeOp::Withdraw { who, amount } => EventKind::WithdrawUnstaked { custody: CUSTODY, destination: ta(two[who], SKR_MINT), beneficiary: two[who], amount },
                StakeOp::Send { from, to, amount } => EventKind::Transfer { source: ta(two[from], SKR_MINT), destination: ta(two[to], SKR_MINT), amount, authority: Authority::Owner },
            };
            if shadow.apply(&k).is_ok() {
                t.ev(k);
            }
        }
        t.slot(slot + 1, ts + 1_000).close();
        let r = run(&cfg, &t, ts + 1_000);
        prop_assert!(r.pool.iter().all(|p| p.status == IntervalStatus::Verified));
        let pending: u64 = shadow.stakes.values().map(|p| p.pending_unstake).sum();
        let last = r.segments.iter().rev().find(|s| s.end > s.start).unwrap();
        let counted: u64 = last.wallets.values().map(|e| match e {
            Eligibility::Verified { units, .. } => *units,
            _ => 0,
        }).sum();
        prop_assert_eq!(counted, SUPPLY - pending);
    }
}
