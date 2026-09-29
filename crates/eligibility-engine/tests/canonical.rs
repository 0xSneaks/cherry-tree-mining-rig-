//! Canonical tests 16–29 (architecture §14.3, "Ownership, encumbrance and
//! debt"). All addresses are fixtures.

mod common;

use std::collections::BTreeSet;

use common::*;
use eligibility_engine::*;

const SUPPLY: u64 = 1_000;
const END: i64 = 1_000;

/// Base world: A, B, C token accounts for SKR; A minted SUPPLY; A and B
/// (and C) registered for SKR with a clean read of every registry entry.
fn world(reg: &[Addr]) -> Tl {
    let mut t = Tl::new();
    t.slot(1, 100)
        .init(A, SKR_MINT)
        .init(B, SKR_MINT)
        .init(C, SKR_MINT)
        .mint(A, SKR_MINT, SUPPLY)
        .pinned(&[LENDER_A, LENDER_B]);
    for w in [A, B, C] {
        t.register(w, AssetKind::Skr).no_debt(w, reg);
    }
    t
}

fn skr_cfg() -> EligibilityConfig {
    config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A])
}

fn assert_no_overlap(r: &ReplayReport, elapsed: i64) {
    let total: u128 = [A, B, C].iter().map(|w| r.wallet(w).unit_seconds).sum();
    assert_eq!(
        total,
        SUPPLY as u128 * elapsed as u128,
        "unit-seconds exceed owned supply x time"
    );
}

// ------------------------------------------------------------------ 16

#[test]
fn t16_a_b_a_across_slots_each_earns_only_its_intervals() {
    let mut t = world(&[LENDER_A]);
    t.slot(2, 200).send(A, B, SKR_MINT, SUPPLY);
    t.slot(3, 500).send(B, A, SKR_MINT, SUPPLY);
    t.slot(4, END).close();
    let r = run(&skr_cfg(), &t, END);
    assert_eq!(r.wallet(&A).unit_seconds, SUPPLY as u128 * (100 + 500));
    assert_eq!(r.wallet(&B).unit_seconds, SUPPLY as u128 * 300);
    assert_eq!(r.wallet(&C).unit_seconds, 0);
    assert_eq!(
        r.wallet(&A).equity_seconds,
        2 * SUPPLY as u128 * 600,
        "X = units x price"
    );
    assert_no_overlap(&r, END - 100);
    assert!(r.pool.iter().all(|p| p.status == IntervalStatus::Verified));
}

// ------------------------------------------------------------------ 17

#[test]
fn t17_a_b_c_inside_one_transaction_creates_no_duration() {
    let mut t = world(&[LENDER_A]);
    t.slot(2, 200)
        .tx()
        .send(A, B, SKR_MINT, SUPPLY)
        .send(B, C, SKR_MINT, SUPPLY);
    t.slot(3, END).close();
    let r = run(&skr_cfg(), &t, END);
    assert_eq!(
        r.wallet(&B).unit_seconds,
        0,
        "B held only inside one transaction"
    );
    assert_eq!(r.wallet(&B).equity_seconds, 0);
    assert_eq!(r.wallet(&A).unit_seconds, SUPPLY as u128 * 100);
    assert_eq!(r.wallet(&C).unit_seconds, SUPPLY as u128 * 800);
    assert_no_overlap(&r, END - 100);
}

// ------------------------------------------------------------------ 18

#[test]
fn t18_different_slots_sharing_a_timestamp_have_zero_overlap() {
    let mut t = world(&[LENDER_A]);
    t.slot(2, 200).send(A, B, SKR_MINT, SUPPLY);
    t.slot(3, 200).send(B, C, SKR_MINT, SUPPLY); // same unix_timestamp
    t.slot(4, END).close();
    let r = run(&skr_cfg(), &t, END);
    assert_eq!(r.wallet(&B).unit_seconds, 0);
    assert_eq!(r.wallet(&C).unit_seconds, SUPPLY as u128 * 800);
    assert_no_overlap(&r, END - 100);
    // Two slots with one timestamp: allowed. One slot with two: refused.
    let mut bad = Tl::new();
    bad.slot(1, 100).init(A, SKR_MINT);
    bad.slot(1, 101).tx().init(B, SKR_MINT);
    assert_eq!(
        replay(&skr_cfg(), &refs(), bad.events.clone(), END).unwrap_err(),
        EligibilityError::SlotTimestampMismatch
    );
}

// ------------------------------------------------------------------ 19

#[test]
fn t19_set_authority_account_owner_is_a_transfer() {
    let mut t = world(&[LENDER_A]);
    t.slot(2, 150).ev(EventKind::Approve {
        account: ta(A, SKR_MINT),
        delegate: DELEGATE,
        amount: 100,
    });
    t.slot(3, 200).ev(EventKind::SetAccountOwner {
        account: ta(A, SKR_MINT),
        new_owner: B,
    });
    t.slot(4, END).close();
    let r = run(&skr_cfg(), &t, END);
    // A: 1000 for 50s, then 900 (delegated 100) for 50s; B from 200, with
    // the delegate cleared by the owner change.
    assert_eq!(r.wallet(&A).unit_seconds, 1000 * 50 + 900 * 50);
    assert_eq!(r.wallet(&B).unit_seconds, SUPPLY as u128 * 800);
    assert!(matches!(
        x_at(&r, &B, 300),
        Some(Eligibility::Verified { units: 1000, .. })
    ));
}

// ------------------------------------------------------------------ 20

#[test]
fn t20_full_approve_drops_x_and_revoke_restores_prospectively() {
    let mut t = world(&[LENDER_A]);
    t.slot(2, 200).ev(EventKind::Approve {
        account: ta(A, SKR_MINT),
        delegate: DELEGATE,
        amount: SUPPLY,
    });
    t.slot(3, 500).ev(EventKind::Revoke {
        account: ta(A, SKR_MINT),
    });
    t.slot(4, 600).ev(EventKind::Approve {
        account: ta(A, SKR_MINT),
        delegate: DELEGATE,
        amount: 400,
    });
    t.slot(5, END).close();
    let r = run(&skr_cfg(), &t, END);
    assert_eq!(verified_x(x_at(&r, &A, 150)), 2_000);
    assert_eq!(
        verified_x(x_at(&r, &A, 300)),
        0,
        "X drops by delegated_amount"
    );
    assert_eq!(
        verified_x(x_at(&r, &A, 550)),
        2_000,
        "restored from the Revoke"
    );
    assert_eq!(
        verified_x(x_at(&r, &A, 700)),
        1_200,
        "partial delegation: amount - delegated"
    );
    assert_eq!(
        r.wallet(&A).equity_seconds,
        2_000 * 100 + 2_000 * 100 + 1_200 * 400 // + 0 x 300 while delegated
    );
}

// ------------------------------------------------------------------ 21

#[test]
fn t21_frozen_account_is_excluded() {
    let mut t = world(&[LENDER_A]);
    t.slot(2, 200).ev(EventKind::Freeze {
        account: ta(A, SKR_MINT),
    });
    t.slot(3, 500).ev(EventKind::Thaw {
        account: ta(A, SKR_MINT),
    });
    t.slot(4, END).close();
    let r = run(&skr_cfg(), &t, END);
    assert_eq!(verified_x(x_at(&r, &A, 300)), 0);
    assert_eq!(r.wallet(&A).unit_seconds, SUPPLY as u128 * (100 + 500));
    // A transfer out of a frozen account is impossible on chain: replay
    // treats it as divergence, not as a valid move.
    let mut t = world(&[LENDER_A]);
    t.slot(2, 200).ev(EventKind::Freeze {
        account: ta(A, SKR_MINT),
    });
    t.slot(3, 300).send(A, B, SKR_MINT, 1);
    t.slot(4, END).close();
    let r = run(&skr_cfg(), &t, END);
    assert!(r
        .pool
        .iter()
        .any(|p| p.status == IntervalStatus::Unresolved));
}

// ------------------------------------------------------------------ 22

#[test]
fn t22_token_2022_risk_extensions_rejected_unless_adapter_cleared() {
    for ext in [
        RiskExtension::PermanentDelegate,
        RiskExtension::TransferHook,
        RiskExtension::ConfidentialTransfer,
    ] {
        let program = TokenProgram::Token2022 {
            extensions: BTreeSet::from([ext]),
        };
        let adapter = AdapterConfig::skr(SKR_MINT, program.clone(), StakeSupport::None, vec![]);
        let mut t = world(&[LENDER_A]);
        t.slot(2, END).close();
        let r = run(&config(vec![adapter.clone()], &[LENDER_A]), &t, END);
        assert_eq!(
            x_at(&r, &A, 500),
            Some(Eligibility::Ineligible(
                IneligibleReason::Token2022Extension(ext)
            ))
        );
        assert_eq!(r.wallet(&A).equity_seconds, 0);

        let mut cleared = adapter;
        cleared.cleared_extensions.insert(ext);
        let r = run(&config(vec![cleared], &[LENDER_A]), &t, END);
        assert_eq!(verified_x(x_at(&r, &A, 500)), 2_000);
    }
    // A plain Token-2022 mint without risk extensions is fine.
    let plain = AdapterConfig::skr(
        SKR_MINT,
        TokenProgram::Token2022 {
            extensions: BTreeSet::new(),
        },
        StakeSupport::None,
        vec![],
    );
    let mut t = world(&[LENDER_A]);
    t.slot(2, END).close();
    assert_eq!(
        verified_x(x_at(
            &run(&config(vec![plain], &[LENDER_A]), &t, END),
            &A,
            500
        )),
        2_000
    );
}

// ------------------------------------------------------------------ 23

#[test]
fn t23_flash_loan_sized_intra_transaction_balance_has_zero_weight() {
    let big = 1_000_000_000u64;
    let mut t = world(&[LENDER_A]);
    t.mint(C, SKR_MINT, big); // C is the flash lender
    t.slot(2, 200)
        .tx()
        .send(C, B, SKR_MINT, big)
        .send(B, C, SKR_MINT, big);
    t.slot(3, END).close();
    let r = run(&skr_cfg(), &t, END);
    assert_eq!(r.wallet(&B).unit_seconds, 0);
    assert_eq!(r.wallet(&B).equity_seconds, 0);
    assert!(r.segments().iter().all(|s| s.start == s.end
        || !matches!(
            s.wallets.get(&B),
            Some(Eligibility::Verified { units, .. }) if *units > 0
        )));
}

// ------------------------------------------------------------------ 24

#[test]
fn t24_missing_transfer_reserves_the_whole_common_pool_interval() {
    // Truth: A -> B at t=300. The indexer dropped that event; the checkpoint
    // at t=500 observes B holding the units.
    let mut t = world(&[LENDER_A]);
    t.slot(2, 500).checkpoint(
        vec![
            snap(A, SKR_MINT, 0),
            snap(B, SKR_MINT, SUPPLY),
            snap(C, SKR_MINT, 0),
        ],
        vec![],
        vec![supply(SKR_MINT, SUPPLY)],
    );
    t.slot(3, 700).send(B, C, SKR_MINT, 100);
    t.slot(4, END).close();
    let r = run(&skr_cfg(), &t, END);

    // The whole window up to the mismatching checkpoint is unresolved, and
    // EVERY participant is pending in it — including C, whose own history
    // is complete (the missing event changes every denominator).
    assert_eq!(
        r.pool[0],
        PoolInterval {
            start: 100,
            end: 500,
            status: IntervalStatus::Unresolved
        }
    );
    for w in [A, B, C] {
        assert_eq!(
            x_at(&r, &w, 200),
            Some(Eligibility::Pending(PendingReason::UnresolvedHistory))
        );
        assert_eq!(r.wallet(&w).pending_seconds, 400);
    }
    // Replay resynchronises on the observed state and later intervals verify.
    assert_eq!(
        r.pool[1],
        PoolInterval {
            start: 500,
            end: END,
            status: IntervalStatus::Verified
        }
    );
    assert_eq!(r.wallet(&B).unit_seconds, 1000 * 200 + 900 * 300);
    assert_eq!(r.wallet(&C).unit_seconds, 100 * 300);

    // A declared gap does the same without a mismatch.
    let mut t = world(&[LENDER_A]);
    t.slot(2, 300).ev(EventKind::DeclaredGap);
    t.slot(3, 500).close();
    let r = run(&skr_cfg(), &t, 500);
    assert!(r
        .pool
        .iter()
        .all(|p| p.status == IntervalStatus::Unresolved));

    // An event the token program would refuse (history missing a mint).
    let mut t = world(&[LENDER_A]);
    t.slot(2, 300).send(B, C, SKR_MINT, 5);
    t.slot(3, 500).close();
    let r = run(&skr_cfg(), &t, 500);
    assert_eq!(r.pool[0].status, IntervalStatus::Unresolved);

    // Without a closing checkpoint nothing verifies: intervals stay pending.
    let mut t = world(&[LENDER_A]);
    t.slot(2, 300).send(A, B, SKR_MINT, 5);
    let r = run(&skr_cfg(), &t, 500);
    assert!(r
        .pool
        .iter()
        .all(|p| p.status == IntervalStatus::NotReconciled));
    assert_eq!(r.wallet(&A).equity_seconds, 0);
}

// ------------------------------------------------------------------ 25

fn stake_world(stake: StakeSupport) -> (EligibilityConfig, Tl) {
    let cfg = config(vec![skr_adapter(stake)], &[LENDER_A]);
    let mut t = world(&[LENDER_A]);
    t.ev(EventKind::InitializeAccount {
        account: CUSTODY,
        mint: SKR_MINT,
        owner: STAKE_CUSTODY_OWNER,
    });
    (cfg, t)
}

#[test]
fn t25_staked_skr_pending_unstake_excluded_and_units_counted_once() {
    let (cfg, mut t) = stake_world(verified_stake());
    t.slot(2, 200).ev(EventKind::Stake {
        source: ta(A, SKR_MINT),
        custody: CUSTODY,
        beneficiary: A,
        amount: 600,
    });
    t.slot(3, 400).ev(EventKind::RequestUnstake {
        mint: SKR_MINT,
        beneficiary: A,
        amount: 200,
    });
    t.slot(4, 700).ev(EventKind::WithdrawUnstaked {
        custody: CUSTODY,
        destination: ta(A, SKR_MINT),
        beneficiary: A,
        amount: 200,
    });
    t.slot(5, END).close();
    let r = run(&cfg, &t, END);
    let units = |t| match x_at(&r, &A, t) {
        Some(Eligibility::Verified { units, .. }) => units,
        o => panic!("{o:?}"),
    };
    assert_eq!(units(150), 1000);
    assert_eq!(
        units(300),
        1000,
        "400 wallet + 600 staked: the custody units are not also counted"
    );
    assert_eq!(units(500), 800, "200 pending unstake excluded");
    assert_eq!(units(800), 1000, "withdrawn back to the wallet");
    // Custody's own token account is never attributed to a wallet, even if
    // the custody owner registers.
    let (cfg, mut t) = stake_world(verified_stake());
    t.register(STAKE_CUSTODY_OWNER, AssetKind::Skr)
        .no_debt(STAKE_CUSTODY_OWNER, &[LENDER_A]);
    t.slot(2, 200).ev(EventKind::Stake {
        source: ta(A, SKR_MINT),
        custody: CUSTODY,
        beneficiary: A,
        amount: 600,
    });
    t.slot(3, END).close();
    let r = run(&cfg, &t, END);
    assert!(matches!(
        x_at(&r, &STAKE_CUSTODY_OWNER, 500),
        Some(Eligibility::Verified { units: 0, .. })
    ));

    // Unverified stake decoding: fail closed to PENDING.
    let (cfg, mut t) = stake_world(StakeSupport::Unverified);
    t.slot(2, 200).ev(EventKind::Stake {
        source: ta(A, SKR_MINT),
        custody: CUSTODY,
        beneficiary: A,
        amount: 600,
    });
    t.slot(3, END).close();
    let r = run(&cfg, &t, END);
    assert_eq!(
        x_at(&r, &A, 500),
        Some(Eligibility::Pending(PendingReason::StakeUnverified))
    );

    // A receipt token for the same staked units is never counted again.
    let mut adapter = skr_adapter(verified_stake());
    adapter.mints.push(AdapterMint {
        mint: RECEIPT_MINT,
        role: MintRole::DuplicateReceipt,
        program: TokenProgram::SplToken,
    });
    let cfg = config(vec![adapter], &[LENDER_A]);
    let (_, mut t) = stake_world(verified_stake());
    t.init(A, RECEIPT_MINT);
    t.slot(2, 200)
        .ev(EventKind::Stake {
            source: ta(A, SKR_MINT),
            custody: CUSTODY,
            beneficiary: A,
            amount: 600,
        })
        .mint(A, RECEIPT_MINT, 600);
    t.slot(3, END).close();
    let r = run(&cfg, &t, END);
    assert!(matches!(
        x_at(&r, &A, 500),
        Some(Eligibility::Verified { units: 1000, .. })
    ));

    // Stake positions not backed by custody balances: divergence.
    let (cfg, mut t) = stake_world(verified_stake());
    t.slot(2, 200).ev(EventKind::RequestUnstake {
        mint: SKR_MINT,
        beneficiary: A,
        amount: 1,
    });
    t.slot(3, END).close();
    assert_eq!(
        run(&cfg, &t, END).pool[0].status,
        IntervalStatus::Unresolved
    );
}

// ------------------------------------------------------------------ 26

#[test]
fn t26_registry_debt_netted_once_at_wallet_level_never_negative() {
    let cfg = config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A, LENDER_B]);
    let mut t = world(&[LENDER_A, LENDER_B]);
    t.slot(2, 200).debt(A, LENDER_A, 500).debt(A, LENDER_B, 700);
    t.slot(3, 400).debt(A, LENDER_B, 5_000);
    t.slot(4, END).close();
    let r = run(&cfg, &t, END);
    match x_at(&r, &A, 300) {
        Some(Eligibility::Verified {
            gross_lamports,
            debt_lamports,
            x_lamports,
            ..
        }) => {
            assert_eq!(
                (gross_lamports, debt_lamports, x_lamports),
                (2_000, 1_200, 800)
            );
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(
        verified_x(x_at(&r, &A, 500)),
        0,
        "debt above holdings: X = 0, never negative"
    );
    // Debt priced in a token uses the same price table as holdings.
    let mut t = world(&[LENDER_A]);
    t.slot(2, 200).ev(EventKind::DebtObserved {
        wallet: A,
        program: LENDER_A,
        debts: vec![DebtAmount {
            price_key: PriceKey::Mint(SKR_MINT),
            units: 250,
        }],
    });
    t.slot(3, END).close();
    assert_eq!(
        verified_x(x_at(&run(&skr_cfg(), &t, END), &A, 500)),
        2_000 - 500
    );
}

// ------------------------------------------------------------------ 27

#[test]
fn t27_debt_increase_reduces_x_from_its_event_and_unreadable_is_pending() {
    let mut t = world(&[LENDER_A]);
    t.slot(2, 500).debt(A, LENDER_A, 1_500);
    t.slot(3, 700).ev(EventKind::DebtUnreadable {
        wallet: A,
        program: LENDER_A,
    });
    t.slot(4, 800).debt(A, LENDER_A, 0);
    t.slot(5, 900).ev(EventKind::ProgramVersion {
        program: LENDER_A,
        version: V2,
    });
    t.slot(6, END).close();
    let r = run(&skr_cfg(), &t, END);
    assert_eq!(verified_x(x_at(&r, &A, 400)), 2_000, "not retroactive");
    assert_eq!(verified_x(x_at(&r, &A, 600)), 500, "from the debt event on");
    assert_eq!(
        x_at(&r, &A, 750),
        Some(Eligibility::Pending(PendingReason::Debt(
            DebtPending::Unreadable
        )))
    );
    assert_eq!(verified_x(x_at(&r, &A, 850)), 2_000);
    assert_eq!(
        x_at(&r, &A, 950),
        Some(Eligibility::Pending(PendingReason::Debt(
            DebtPending::RegistryProgramUnknown
        ))),
        "pinned registry program upgraded"
    );
    assert_eq!(
        r.wallet(&A).equity_seconds,
        2_000 * 400 + 500 * 200 + 2_000 * 100
    );
    assert_eq!(r.wallet(&A).pending_seconds, 100 + 100);

    // A registered wallet never read in the registry is pending, not zero.
    let mut t = world(&[]);
    t.slot(2, END).close();
    let r = run(&skr_cfg(), &t, END);
    assert_eq!(
        x_at(&r, &A, 500),
        Some(Eligibility::Pending(PendingReason::Debt(
            DebtPending::NotYetRead
        )))
    );
    // A registry program whose pinned version was never observed is UNKNOWN.
    let mut t = Tl::new();
    t.slot(1, 100)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, SUPPLY)
        .register(A, AssetKind::Skr)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, END).close();
    let r = run(&skr_cfg(), &t, END);
    assert_eq!(
        x_at(&r, &A, 500),
        Some(Eligibility::Pending(PendingReason::Debt(
            DebtPending::RegistryProgramUnknown
        )))
    );
}

// ------------------------------------------------------------------ 28

#[test]
fn t28_non_registry_debt_is_not_detected_and_disclosed() {
    let mut t = world(&[LENDER_A]);
    t.slot(2, 200).debt(A, OUTSIDE_LENDER, 1_999);
    t.slot(3, END).close();
    let r = run(&skr_cfg(), &t, END);
    assert_eq!(
        verified_x(x_at(&r, &A, 500)),
        2_000,
        "debt outside the registry is not netted"
    );
    let d = NON_REGISTRY_DEBT_DISCLOSURE;
    assert!(d.contains("cannot detect"));
    assert!(d.contains("does not prove"));
    assert!(!d.to_lowercase().contains("all borrow"));
    // The product shows the same text.
    let app = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../app/src/lib/eligibility.ts"
    ))
    .unwrap();
    assert!(
        app.contains(d),
        "app disclosure must mirror NON_REGISTRY_DEBT_DISCLOSURE verbatim"
    );
}

// ------------------------------------------------------------------ 29

#[test]
fn t29_owned_xsol_valued_at_owned_token_nav_never_notional() {
    let xsol = AdapterConfig::xsol(
        XSOL_MINT,
        TokenProgram::SplToken,
        vec![ProgramPin {
            program: XSOL_PROGRAM,
            version: V1,
        }],
    );
    let cfg = config(vec![xsol], &[LENDER_A]);
    let world = |debt: u64| {
        let mut t = Tl::new();
        t.slot(1, 100)
            .init(A, XSOL_MINT)
            .mint(A, XSOL_MINT, 1_000)
            .pinned(&[XSOL_PROGRAM, LENDER_A])
            .register(A, AssetKind::XSol)
            .debt(A, LENDER_A, debt);
        t.slot(2, END).close();
        t
    };
    let r = replay(&cfg, &refs(), world(0).events, END).unwrap();
    match x_at(&r, &A, 500) {
        Some(Eligibility::Verified {
            units,
            gross_lamports,
            ..
        }) => assert_eq!((units, gross_lamports), (1_000, 3_000)),
        o => panic!("{o:?}"),
    }
    // Feeding the leveraged notional (say 3x) is refused, never used.
    let notional = EpochReferences::new().with(
        ReferenceCommit::new(
            PriceKey::Mint(XSOL_MINT),
            UnitPrice {
                lamports_num: 9,
                units_den: 1,
                basis: PriceBasis::UnderlyingNotional,
            },
            0,
            PRE_REFERENCE_EPOCH_START,
        )
        .unwrap(),
    );
    let r = replay(&cfg, &notional, world(0).events, END).unwrap();
    assert_eq!(
        x_at(&r, &A, 500),
        Some(Eligibility::Ineligible(IneligibleReason::InvalidPriceBasis))
    );
    assert_eq!(r.wallet(&A).equity_seconds, 0);
    // A plain reference price is not an owned-token NAV either.
    let reference = EpochReferences::new().with(ref_commit(XSOL_MINT, price(3, 1)));
    let r = replay(&cfg, &reference, world(0).events, END).unwrap();
    assert_eq!(
        x_at(&r, &A, 500),
        Some(Eligibility::Ineligible(IneligibleReason::InvalidPriceBasis))
    );
    // Borrowing to buy it is wallet debt, netted if in the registry.
    let r = replay(&cfg, &refs(), world(1_000).events, END).unwrap();
    assert_eq!(verified_x(x_at(&r, &A, 500)), 2_000);
    // An unobserved pin is UNKNOWN from the start (fail closed).
    let mut t = Tl::new();
    t.slot(1, 100)
        .init(A, XSOL_MINT)
        .mint(A, XSOL_MINT, 1_000)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::XSol)
        .debt(A, LENDER_A, 0);
    t.slot(2, END).close();
    let r = replay(&cfg, &refs(), t.events, END).unwrap();
    assert_eq!(
        x_at(&r, &A, 500),
        Some(Eligibility::Pending(PendingReason::AdapterProgramUnknown))
    );
    // A pinned xSOL program upgrade makes the asset UNKNOWN from that event.
    let mut t = world(0);
    t.slot(3, 1_500).ev(EventKind::ProgramVersion {
        program: XSOL_PROGRAM,
        version: V2,
    });
    let r = replay(&cfg, &refs(), t.events, 2_000).unwrap();
    assert_eq!(
        x_at(&r, &A, 1_700),
        Some(Eligibility::Pending(PendingReason::NotReconciled))
    );
    // (no closing checkpoint above; with one the reason is the program pin)
    let mut t = world(0);
    t.slot(3, 1_500).ev(EventKind::ProgramVersion {
        program: XSOL_PROGRAM,
        version: V2,
    });
    t.slot(4, 2_000).close();
    let r = replay(&cfg, &refs(), t.events, 2_000).unwrap();
    assert_eq!(
        x_at(&r, &A, 1_700),
        Some(Eligibility::Pending(PendingReason::AdapterProgramUnknown))
    );
}

// ------------------------------------------------------- launch scope

#[test]
fn only_skr_and_xsol_can_be_activated() {
    for asset in [
        AssetKind::JitoSol,
        AssetKind::SpotJlp,
        AssetKind::XBtc,
        AssetKind::XHype,
    ] {
        let mut a = skr_adapter(StakeSupport::None);
        a.asset = asset;
        assert_eq!(
            a.validate(),
            Err(EligibilityError::AssetNotEnabled),
            "{asset:?}"
        );
    }
    let mut a = skr_adapter(StakeSupport::None);
    a.asset = AssetKind::LeveragedJlp;
    assert_eq!(a.validate(), Err(EligibilityError::AssetExcluded));
    // One mint cannot back two assets.
    let x = AdapterConfig::xsol(SKR_MINT, TokenProgram::SplToken, vec![]);
    assert_eq!(
        EligibilityConfig::new(vec![skr_adapter(StakeSupport::None), x], registry(&[]))
            .unwrap_err(),
        EligibilityError::DuplicateMint
    );
    // No committed price: inactive (known weight 0), not pending.
    let mut t = world(&[LENDER_A]);
    t.slot(2, END).close();
    let r = replay(&skr_cfg(), &EpochReferences::new(), t.events, END).unwrap();
    assert_eq!(
        x_at(&r, &A, 500),
        Some(Eligibility::Ineligible(
            IneligibleReason::InactiveNoReference
        ))
    );
}

#[test]
fn encumbered_units_are_excluded_and_released_prospectively() {
    let mut t = world(&[LENDER_A]);
    t.slot(2, 200).ev(EventKind::Encumber {
        account: ta(A, SKR_MINT),
        kind: EncumbranceKind::Collateral,
        amount: 300,
    });
    t.slot(3, 300).ev(EventKind::Encumber {
        account: ta(A, SKR_MINT),
        kind: EncumbranceKind::Vesting,
        amount: 200,
    });
    t.slot(4, 400).ev(EventKind::Release {
        account: ta(A, SKR_MINT),
        kind: EncumbranceKind::Collateral,
        amount: 300,
    });
    t.slot(5, END).close();
    let r = run(&skr_cfg(), &t, END);
    assert_eq!(verified_x(x_at(&r, &A, 250)), 1_400);
    assert_eq!(verified_x(x_at(&r, &A, 350)), 1_000);
    assert_eq!(verified_x(x_at(&r, &A, 500)), 1_600);
}

#[test]
fn exit_ends_eligibility_and_late_join_accrues_from_registration() {
    let mut t = Tl::new();
    t.slot(1, 100)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, SUPPLY)
        .pinned(&[LENDER_A])
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 300).register(A, AssetKind::Skr);
    t.slot(3, 600).ev(EventKind::Exit { wallet: A });
    t.slot(4, END).close();
    let r = run(&skr_cfg(), &t, END);
    assert_eq!(r.wallet(&A).unit_seconds, SUPPLY as u128 * 300);
}
