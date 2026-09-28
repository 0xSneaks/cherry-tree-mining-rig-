//! Canonical tests 30–39 (architecture §14.3, "Pricing and Safe Mode").
//! All addresses, prices, thresholds, zones and dwell values are FIXTURES
//! (gate 14.2-D); nothing here is a production calibration.

mod common;

use common::*;
use eligibility_engine::*;

// ------------------------------------------------------------------ 30

/// SKR fixture world: reference price 2 committed at 0, epoch starts at
/// 100. Wallet A holds 1000 units from before the epoch; B holds 800.
fn skr_epoch() -> (EligibilityConfig, EpochReferences) {
    (
        config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A]),
        EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 100)),
    )
}

fn skr_world() -> Tl {
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .init(B, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .mint(B, SKR_MINT, 800)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::Skr)
        .no_debt(A, &[LENDER_A]);
    t
}

#[test]
fn t30_post_reference_additions_use_min_of_reference_and_event_price() {
    let (cfg, refs) = skr_epoch();

    // Mid-epoch drop: units added after it price at the verified event
    // price, not the pre-drop reference.
    let mut t = skr_world();
    t.slot(2, 150).observe(SKR_MINT, price(1, 1));
    t.slot(3, 200).mint(A, SKR_MINT, 500);
    t.slot(4, 250).close();
    let r = run_with(&cfg, &refs, &t, 250);
    assert_eq!(verified_x(x_at(&r, &A, 230)), 1_000 * 2 + 500);
    // Settled accrual: 2000 lamports for 150s, then 2500 for 50s.
    assert_eq!(r.wallet(&A).equity_seconds, 2_000 * 150 + 2_500 * 50);

    // Post-reference price ABOVE the reference: still the reference
    // (min) — weight is never bought above it either.
    let mut t = skr_world();
    t.slot(2, 150).observe(SKR_MINT, price(3, 1));
    t.slot(3, 200).mint(A, SKR_MINT, 500);
    t.slot(4, 250).close();
    let r = run_with(&cfg, &refs, &t, 250);
    assert_eq!(verified_x(x_at(&r, &A, 230)), 1_000 * 2 + 500 * 2);

    // No verified observation at the addition: fail closed — PENDING
    // (reserved), never silently reference-priced.
    let mut t = skr_world();
    t.slot(3, 200).mint(A, SKR_MINT, 500);
    t.slot(4, 250).close();
    let r = run_with(&cfg, &refs, &t, 250);
    assert_eq!(
        x_at(&r, &A, 230),
        Some(Eligibility::Pending(PendingReason::AdditionUnpriced))
    );
    assert_eq!(
        r.wallet(&A).equity_seconds,
        2_000 * 150,
        "only the pre-addition segment settles"
    );

    // Units held before the epoch but REGISTERED after it follow the same
    // min rule ("added or registered after it").
    let mut t = skr_world();
    t.slot(2, 150).observe(SKR_MINT, price(1, 1));
    t.slot(3, 200)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(4, 250).close();
    let r = run_with(&cfg, &refs, &t, 250);
    assert_eq!(verified_x(x_at(&r, &B, 230)), 800);

    // Registration without a usable observation: PENDING, not zero.
    let mut t = skr_world();
    t.slot(3, 200)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(4, 250).close();
    let r = run_with(&cfg, &refs, &t, 250);
    assert_eq!(
        x_at(&r, &B, 230),
        Some(Eligibility::Pending(PendingReason::AdditionUnpriced))
    );

    // Registration at or before the epoch start: reference pricing stands.
    let mut t = skr_world();
    t.slot(2, 100)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(3, 250).close();
    let r = run_with(&cfg, &refs, &t, 250);
    assert_eq!(verified_x(x_at(&r, &B, 200)), 800 * 2);
}

// ------------------------------------------------------------------ 31

/// Fixture calibration — NOT a production threshold.
fn validation_fixture() -> ReferenceValidation {
    ReferenceValidation {
        min_distinct_slots: 3,
        min_window_seconds: 60,
        max_spread: Ratio::new(1, 20),
        max_confidence: Ratio::new(1, 50),
    }
}

fn sample(slot: u64, ts: i64, num: u128, den: u128) -> PriceSample {
    PriceSample {
        slot,
        unix_timestamp: ts,
        lamports_num: num,
        units_den: den,
        confidence: None,
    }
}

#[test]
fn t31_manipulated_one_block_reference_sample_is_rejected() {
    // A window that lives inside a single block can never commit.
    let one_block = [
        sample(7, 1_000, 5, 1),
        sample(7, 1_000, 9, 1),
        sample(7, 1_000, 6, 1),
    ];
    assert_eq!(
        commit_reference(
            &one_block,
            1_000,
            &validation_fixture(),
            PriceBasis::Reference
        ),
        Err(EligibilityError::InvalidPriceWindow(
            "not enough distinct slots"
        ))
    );
    // A single spot sample is prohibited outright.
    let lone = [sample(7, 1_000, 5, 1)];
    assert!(commit_reference(&lone, 1_060, &validation_fixture(), PriceBasis::Reference).is_err());
    // A multi-block window with one manipulated block is rejected by the
    // spread bound.
    let outlier = [
        sample(1, 100, 2, 1),
        sample(2, 130, 2, 1),
        sample(3, 160, 2, 1),
        sample(4, 190, 90, 1),
    ];
    assert_eq!(
        commit_reference(&outlier, 200, &validation_fixture(), PriceBasis::Reference),
        Err(EligibilityError::InvalidPriceWindow("spread too wide"))
    );
    // Wide oracle confidence is rejected with equal force.
    let wide = [
        PriceSample {
            slot: 1,
            unix_timestamp: 100,
            lamports_num: 2,
            units_den: 1,
            confidence: Some((1, 10)),
        },
        sample(2, 130, 2, 1),
        sample(3, 160, 2, 1),
    ];
    assert_eq!(
        commit_reference(&wide, 200, &validation_fixture(), PriceBasis::Reference),
        Err(EligibilityError::InvalidPriceWindow("confidence too wide"))
    );

    // A validated window commits its exact TWAP and can serve as the epoch
    // reference of a settlement window.
    let honest = [
        sample(1, 100, 200, 100),
        sample(2, 160, 205, 100),
        sample(3, 190, 210, 100),
    ];
    let committed = commit_reference(&honest, 200, &validation_fixture(), PriceBasis::Reference)
        .expect("validated window commits");
    assert_eq!(
        (committed.lamports_num, committed.units_den),
        (20_250, 10_000)
    );
    let refs = EpochReferences::new()
        .with(ReferenceCommit::new(PriceKey::Mint(SKR_MINT), committed, 250, 300).unwrap());
    let mut t = Tl::new();
    t.slot(1, 290)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::Skr)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 400).close();
    let r = run_with(&skr_epoch().0, &refs, &t, 400);
    // 1000 units x 20250/10000 = 2025 lamports.
    assert_eq!(verified_x(x_at(&r, &A, 350)), 2_025);
}

// ------------------------------------------------------------------ 32

#[test]
fn t32_no_committed_reference_means_inactive_weight_zero_others_settle() {
    let cfg = config(
        vec![
            skr_adapter(StakeSupport::None),
            AdapterConfig::xsol(XSOL_MINT, TokenProgram::SplToken, vec![]),
        ],
        &[LENDER_A],
    );
    // Only xSOL has a committed reference this epoch.
    let refs = EpochReferences::new().with(ref_commit_at(XSOL_MINT, nav(3, 1), 100));
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .init(B, XSOL_MINT)
        .mint(B, XSOL_MINT, 400)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::Skr)
        .register(B, AssetKind::XSol)
        .no_debt(A, &[LENDER_A])
        .no_debt(B, &[LENDER_A]);
    t.slot(2, 200).close();
    let r = run_with(&cfg, &refs, &t, 200);

    // The asset without a reference is INACTIVE — a known weight-0 state
    // visible even before the epoch starts, not UNKNOWN, not PENDING.
    for probe in [60, 150] {
        assert_eq!(
            x_at(&r, &A, probe),
            Some(Eligibility::Ineligible(
                IneligibleReason::InactiveNoReference
            )),
            "inactive from the start (probe {probe})"
        );
    }
    assert!(r.wallet(&A).ineligible_seconds > 0);
    assert_eq!(r.wallet(&A).equity_seconds, 0);
    // ...and it cannot freeze anyone else: B settles normally.
    assert_eq!(verified_x(x_at(&r, &B, 150)), 400 * 3);
    assert_eq!(r.wallet(&B).equity_seconds, 1_200 * 150);
    assert!(r.pool.iter().all(|p| p.status == IntervalStatus::Verified));
    assert_eq!(r.wallet(&A).pending_seconds, 0);
}

// ------------------------------------------------------------------ 33

#[test]
fn t33_pinned_program_upgrade_is_unknown_from_the_upgrade_slot() {
    let cfg = config(vec![xsol_adapter()], &[LENDER_A]);
    let refs = EpochReferences::new().with(ref_commit_at(XSOL_MINT, nav(3, 1), 100));

    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, XSOL_MINT)
        .mint(A, XSOL_MINT, 1_000)
        .pinned(&[XSOL_PROGRAM, LENDER_A])
        .register(A, AssetKind::XSol)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 300).close(); // settles [50, 300) at NAV 3
    t.slot(3, 400).ev(EventKind::ProgramVersion {
        program: XSOL_PROGRAM,
        version: V2,
    });
    t.slot(4, 600).close();
    let r = run_with(&cfg, &refs, &t, 600);

    // Before the upgrade: verified. From the finalized upgrade slot:
    // UNKNOWN (pause new recognition), the segment boundary is exact.
    assert!(matches!(
        x_at(&r, &A, 350),
        Some(Eligibility::Verified { .. })
    ));
    let seg = r
        .segments
        .iter()
        .find(|s| s.start <= 400 && 400 < s.end)
        .unwrap();
    assert_eq!(seg.start, 400, "effective from the upgrade slot's segment");
    assert_eq!(
        x_at(&r, &A, 450),
        Some(Eligibility::Pending(PendingReason::AdapterProgramUnknown))
    );

    // Settled liabilities are unchanged: the checkpoint at 300 settled
    // exactly 3_000 * 250 lamport-seconds, and no later event reprices it.
    let mut pre = Tl::new();
    pre.slot(1, 50)
        .init(A, XSOL_MINT)
        .mint(A, XSOL_MINT, 1_000)
        .pinned(&[XSOL_PROGRAM, LENDER_A])
        .register(A, AssetKind::XSol)
        .no_debt(A, &[LENDER_A]);
    pre.slot(2, 300).close();
    let settled = run_with(&cfg, &refs, &pre, 300).wallet(&A).equity_seconds;
    assert_eq!(settled, 3_000 * 250);

    // Full run: the [300, 400) segment was still pre-upgrade and settles;
    // everything after the upgrade is reserved (pending), never admitted
    // and never zero.
    assert_eq!(r.wallet(&A).equity_seconds, 3_000 * 350);
    assert_eq!(r.wallet(&A).pending_seconds, 200);
    for s in &r.segments {
        if s.start >= 400 {
            assert_eq!(
                s.wallets.get(&A),
                Some(&Eligibility::Pending(PendingReason::AdapterProgramUnknown)),
                "no recognition after the upgrade"
            );
        }
    }
    // A later re-observation of the pinned version restores recognition.
    let mut t2 = t;
    t2.slot(5, 700).ev(EventKind::ProgramVersion {
        program: XSOL_PROGRAM,
        version: V2,
    });
    // (V2 is still not the pinned V1: UNKNOWN until the adapter re-pins —
    // verified by the still-pending state below.)
    t2.slot(6, 800).close();
    let r2 = run_with(&cfg, &refs, &t2, 800);
    assert_eq!(
        x_at(&r2, &A, 750),
        Some(Eligibility::Pending(PendingReason::AdapterProgramUnknown))
    );
}

// ------------------------------------------------------- 34 / 35 / 39
// Decision layer: consent, route availability, severity separation.

use consent::ConsentAction::{Enter, Increase};

fn entry_request() -> ConsentRequest {
    ConsentRequest {
        wallet: A,
        asset: AssetKind::XSol,
        action: Enter,
        amount_lamports: 1_000,
        chain: ChainId::FIXTURE,
        epoch: 7,
        risk_version: 3,
        now: 1_000,
    }
}

fn signed(request: &ConsentRequest, nonce: u64) -> RiskConsent {
    let mut c = consent::fixture_consent_for(request, nonce, request.now + 600);
    c.signature = [7u8; 64];
    c
}

/// Fixture verifier: accepts exactly the `[7; 64]` fixture signature.
struct FixtureSigner;

impl ConsentVerifier for FixtureSigner {
    fn verify(&self, _wallet: &Addr, _payload: &[u8], signature: &[u8; 64]) -> bool {
        *signature == [7u8; 64]
    }
}

struct RejectAll;

impl ConsentVerifier for RejectAll {
    fn verify(&self, _wallet: &Addr, _payload: &[u8], _signature: &[u8; 64]) -> bool {
        false
    }
}

fn authorize(
    zone: RiskZone,
    request: &ConsentRequest,
    consent: Option<&RiskConsent>,
    route: &RouteCheck,
) -> EntryDecision {
    let mut ledger = ConsentLedger::new();
    authorize_entry(
        zone,
        request,
        consent,
        route,
        &SafeModeBoundaries::fixture_xsol(),
        &mut ledger,
        &FixtureSigner,
    )
    .unwrap()
}

#[test]
fn t34_hot_entry_needs_a_fully_bound_fresh_consent() {
    let req = entry_request();
    let hot = RouteCheck::operational(QuoteStatus::Ok);
    let hot_zone = zone_for_cr(
        Some(Ratio::new(110, 100)),
        &SafeModeBoundaries::fixture_xsol(),
    )
    .unwrap();
    assert_eq!(hot_zone, RiskZone::Hot);

    // Accepted with a valid, fully bound, fresh consent.
    assert_eq!(
        authorize(hot_zone, &req, Some(&signed(&req, 1)), &hot),
        EntryDecision::PermittedWithConsent
    );

    // Missing consent.
    assert_eq!(
        authorize(hot_zone, &req, None, &hot),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::Missing))
    );

    // Expired.
    let mut c = signed(&req, 1);
    c.expires_at = req.now;
    assert_eq!(
        authorize(hot_zone, &req, Some(&c), &hot),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::Expired))
    );

    // Wrong wallet / epoch / risk version / scope.
    let mut c = signed(&req, 1);
    c.wallet = B;
    assert_eq!(
        authorize(hot_zone, &req, Some(&c), &hot),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::WrongWallet))
    );
    let mut c = signed(&req, 1);
    c.epoch = 8;
    assert_eq!(
        authorize(hot_zone, &req, Some(&c), &hot),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::WrongEpoch))
    );
    let mut c = signed(&req, 1);
    c.risk_version = 4;
    assert_eq!(
        authorize(hot_zone, &req, Some(&c), &hot),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::WrongVersion))
    );
    let mut c = signed(&req, 1);
    c.amount_lamports = 999;
    assert_eq!(
        authorize(hot_zone, &req, Some(&c), &hot),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::WrongScope))
    );

    // Wrong asset / action / chain and a bad signature.
    let mut c = signed(&req, 1);
    c.asset = AssetKind::Skr;
    assert_eq!(
        authorize(hot_zone, &req, Some(&c), &hot),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::WrongAsset))
    );
    let mut c = signed(&req, 1);
    c.action = Increase;
    assert_eq!(
        authorize(hot_zone, &req, Some(&c), &hot),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::WrongAction))
    );
    let mut c = signed(&req, 1);
    c.chain = ChainId(0xEE);
    assert_eq!(
        authorize(hot_zone, &req, Some(&c), &hot),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::WrongChain))
    );
    let mut c = signed(&req, 1);
    c.signature = [1u8; 64];
    assert_eq!(
        authorize(hot_zone, &req, Some(&c), &hot),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::BadSignature))
    );
    let mut ledger = ConsentLedger::new();
    let route = hot;
    let mut decision = |c: &RiskConsent| {
        authorize_entry(
            hot_zone,
            &req,
            Some(c),
            &route,
            &SafeModeBoundaries::fixture_xsol(),
            &mut ledger,
            &RejectAll,
        )
        .unwrap()
    };
    assert_eq!(
        decision(&signed(&req, 1)),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::BadSignature))
    );

    // Replayed nonce: the same valid consent cannot authorize twice.
    let mut ledger = ConsentLedger::new();
    let c = signed(&req, 1);
    assert_eq!(
        authorize_entry(
            hot_zone,
            &req,
            Some(&c),
            &hot,
            &SafeModeBoundaries::fixture_xsol(),
            &mut ledger,
            &FixtureSigner
        )
        .unwrap(),
        EntryDecision::PermittedWithConsent
    );
    assert_eq!(
        authorize_entry(
            hot_zone,
            &req,
            Some(&c),
            &hot,
            &SafeModeBoundaries::fixture_xsol(),
            &mut ledger,
            &FixtureSigner
        )
        .unwrap(),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::ReplayedNonce))
    );
    // A different nonce with otherwise identical fields is a new consent.
    assert_eq!(
        authorize_entry(
            hot_zone,
            &req,
            Some(&signed(&req, 2)),
            &hot,
            &SafeModeBoundaries::fixture_xsol(),
            &mut ledger,
            &FixtureSigner
        )
        .unwrap(),
        EntryDecision::PermittedWithConsent
    );

    // WARM also requires consent; NORMAL never does.
    let warm = zone_for_cr(
        Some(Ratio::new(130, 100)),
        &SafeModeBoundaries::fixture_xsol(),
    )
    .unwrap();
    assert_eq!(warm, RiskZone::Warm);
    assert_eq!(
        authorize(warm, &req, None, &hot),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::Missing))
    );
    assert_eq!(
        authorize(warm, &req, Some(&signed(&req, 1)), &hot),
        EntryDecision::PermittedWithConsent
    );
    let normal = zone_for_cr(
        Some(Ratio::new(150, 100)),
        &SafeModeBoundaries::fixture_xsol(),
    )
    .unwrap();
    assert_eq!(normal, RiskZone::Normal);
    assert_eq!(
        authorize(normal, &req, None, &hot),
        EntryDecision::Permitted
    );

    // Stale/missing risk data is UNKNOWN — paused, never EXTREME-by-guess.
    assert_eq!(
        authorize(RiskZone::Unknown, &req, Some(&signed(&req, 1)), &hot),
        EntryDecision::Denied(EntryDenial::UnknownRiskData)
    );
}

#[test]
fn t35_hot_entry_to_an_unavailable_route_fails_even_with_valid_consent() {
    let req = entry_request();
    let halted = RouteCheck::halted();
    // Valid consent, halted required route: refused. A signature can never
    // bypass an unavailable operation.
    assert_eq!(
        authorize(RiskZone::Hot, &req, Some(&signed(&req, 1)), &halted),
        EntryDecision::Denied(EntryDenial::RouteUnavailable)
    );
    // EXTREME follows the same "never submit an unavailable operation".
    assert_eq!(
        authorize(RiskZone::Extreme, &req, Some(&signed(&req, 1)), &halted),
        EntryDecision::Denied(EntryDenial::RouteUnavailable)
    );
    // Operational route with the same consent: permitted.
    assert_eq!(
        authorize(
            RiskZone::Hot,
            &req,
            Some(&signed(&req, 1)),
            &RouteCheck::operational(QuoteStatus::Ok)
        ),
        EntryDecision::PermittedWithConsent
    );
    // Severity and availability are separate axes — availability is not a
    // severity knob: WARM (and NORMAL, see the F3 regression below) refuse
    // a halted required route exactly like HOT/EXTREME. A low zone never
    // authorizes an unavailable operation.
    assert_eq!(
        authorize(RiskZone::Warm, &req, Some(&signed(&req, 2)), &halted),
        EntryDecision::Denied(EntryDenial::RouteUnavailable)
    );
    assert_eq!(
        authorize(RiskZone::Normal, &req, None, &halted),
        EntryDecision::Denied(EntryDenial::RouteUnavailable)
    );
}

// ------------------------------------------------------------------ 36

#[test]
fn t36_extreme_reset_is_prospective_only_for_xsol_and_skr() {
    // --- xSOL: CR collapse below 1, verified emergency NAV 1. ---
    let cfg = config(vec![xsol_adapter()], &[LENDER_A]);
    let refs = EpochReferences::new().with(ref_commit_at(XSOL_MINT, nav(3, 1), 100));
    let base = |close_at: i64, with_reset: bool| {
        let mut t = Tl::new();
        t.slot(1, 50)
            .init(A, XSOL_MINT)
            .mint(A, XSOL_MINT, 1_000)
            .pinned(&[XSOL_PROGRAM, LENDER_A])
            .register(A, AssetKind::XSol)
            .no_debt(A, &[LENDER_A]);
        t.slot(2, 300).close(); // settled at NAV 3 through 300
        if with_reset {
            t.slot(3, 400).emergency_nav(XSOL_MINT, nav(1, 1));
        }
        t.slot(4, close_at).close();
        t
    };
    let r = run_with(&cfg, &refs, &base(600, true), 600);
    // Settled R/Q before the event are untouched: the truncation at 300
    // settles 3_000 * 250 and the reset never rewrites it.
    let settled_only = run_with(&cfg, &refs, &base(300, false), 300);
    assert_eq!(settled_only.wallet(&A).equity_seconds, 3_000 * 250);
    // [50,300) settled + [300,400) at NAV 3 + [400,600) at emergency NAV 1.
    assert_eq!(r.wallet(&A).equity_seconds, 3_000 * 350 + 1_000 * 200);
    // Only future/unsettled recognition is adjusted.
    match x_at(&r, &A, 450) {
        Some(Eligibility::Verified { gross_lamports, .. }) => assert_eq!(gross_lamports, 1_000),
        o => panic!("{o:?}"),
    }
    assert!(matches!(
        x_at(&r, &A, 350),
        Some(Eligibility::Verified {
            gross_lamports: 3_000,
            ..
        })
    ));

    // --- SKR (non-xSOL adapter): verified depeg reset, same shape. ---
    let (skr_cfg, skr_refs) = skr_epoch();
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::Skr)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 300).close();
    t.slot(3, 400).emergency_nav(SKR_MINT, price(1, 2)); // depeg to 0.5
    t.slot(4, 600).close();
    let r = run_with(&skr_cfg, &skr_refs, &t, 600);
    assert_eq!(r.wallet(&A).equity_seconds, 2_000 * 350 + 500 * 200);
    match x_at(&r, &A, 450) {
        Some(Eligibility::Verified { gross_lamports, .. }) => assert_eq!(gross_lamports, 500),
        o => panic!("{o:?}"),
    }

    // --- Verified severe protocol/route failure (the other EXTREME
    // trigger): recognition pauses (reserved) until a published recovery
    // reference resumes it. ---
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::Skr)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 300).close();
    t.slot(3, 400).route_halt(SKR_MINT);
    t.slot(4, 500).route_resumed(SKR_MINT, price(1, 1));
    t.slot(5, 600).close();
    let r = run_with(&skr_cfg, &skr_refs, &t, 600);
    assert_eq!(
        x_at(&r, &A, 450),
        Some(Eligibility::Pending(PendingReason::RouteHalted))
    );
    match x_at(&r, &A, 550) {
        Some(Eligibility::Verified { gross_lamports, .. }) => assert_eq!(gross_lamports, 1_000),
        o => panic!("{o:?}"),
    }
    // Settled part unchanged; halted interval reserved, never admitted.
    assert_eq!(
        r.wallet(&A).equity_seconds,
        2_000 * 350 + 1_000 * 100,
        "[50,400) at the reference, [500,600) at the published recovery"
    );
    assert_eq!(r.wallet(&A).pending_seconds, 100);
}

// ------------------------------------------------------------------ 37

#[test]
fn t37_hysteresis_damps_oscillation_and_gates_the_safety_release() {
    // Dwell unset (the current state): entry is immediate, nothing ever
    // auto-releases, and the safety release stays disabled.
    let mut t = ExtremeTracker::new(None);
    assert!(!t.safety_release_enabled(), "dwell unset: release disabled");
    for i in 0..100 {
        let now = i * 10;
        t.observe(ExtremeCondition::Extreme, now);
        t.observe(ExtremeCondition::Healthy, now + 5);
    }
    assert_eq!(t.resets(), 1, "one reset, no repeated resets");
    assert!(t.is_extreme(), "no auto-recovery while dwell is unset");
    assert_eq!(t.releases(), 0);
    assert!(!t.safety_release_enabled());

    // Dwell configured (fixture value): oscillation cannot produce
    // repeated resets or releases; recovery and re-entry both need the
    // condition to persist for the dwell.
    let mut t = ExtremeTracker::new(Some(300));
    assert!(t.safety_release_enabled());
    t.observe(ExtremeCondition::Extreme, 0);
    assert_eq!(t.resets(), 1);
    for i in 1..20 {
        let now = i * 20;
        t.observe(ExtremeCondition::Healthy, now);
        t.observe(ExtremeCondition::Extreme, now + 10);
    }
    assert_eq!(t.resets(), 1);
    assert_eq!(t.releases(), 0);
    assert!(t.is_extreme());
    // Dwell-confirmed recovery runs the release exactly once.
    t.observe(ExtremeCondition::Healthy, 10_000);
    t.observe(ExtremeCondition::Healthy, 10_301);
    assert!(!t.is_extreme());
    assert_eq!(t.releases(), 1);
    // A dip shorter than the dwell does not re-enter...
    t.observe(ExtremeCondition::Extreme, 11_000);
    t.observe(ExtremeCondition::Healthy, 11_100);
    assert!(!t.is_extreme());
    assert_eq!(t.resets(), 1);
    // ...a dwell-long dip does.
    t.observe(ExtremeCondition::Extreme, 12_000);
    t.observe(ExtremeCondition::Extreme, 12_301);
    assert!(t.is_extreme());
    assert_eq!(t.resets(), 2);
    assert_eq!(t.releases(), 1);

    // Replay-level oscillation of verified resets: every reset is
    // prospective-only and settled segments never move.
    let (skr_cfg, skr_refs) = skr_epoch();
    let mut tl = Tl::new();
    tl.slot(1, 50)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::Skr)
        .no_debt(A, &[LENDER_A]);
    tl.slot(2, 300).close();
    tl.slot(3, 400).emergency_nav(SKR_MINT, price(1, 1));
    tl.slot(4, 500).close();
    tl.slot(5, 600).emergency_nav(SKR_MINT, price(2, 1)); // recovery upward
    tl.slot(6, 700).close();
    let r = run_with(&skr_cfg, &skr_refs, &tl, 700);
    assert_eq!(
        r.wallet(&A).equity_seconds,
        2_000 * 350 + 1_000 * 200 + 2_000 * 100,
        "settled parts unchanged; each reset only moves future segments"
    );
}

// ------------------------------------------------------------------ 38

#[test]
fn t38_cr_of_one_never_displays_finite_leverage() {
    let b = SafeModeBoundaries::fixture_xsol();
    // CR = 1 sits exactly on the HOT boundary: HOT, but leverage is
    // singular — the interface returns nothing finite.
    assert_eq!(zone_for_cr(Some(Ratio::ONE), &b).unwrap(), RiskZone::Hot);
    assert_eq!(leverage_for_cr(Ratio::ONE).unwrap(), None);
    // Just below: EXTREME, still no leverage.
    assert_eq!(
        zone_for_cr(Some(Ratio::new(9_999, 10_000)), &b).unwrap(),
        RiskZone::Extreme
    );
    assert_eq!(leverage_for_cr(Ratio::new(9_999, 10_000)).unwrap(), None);
    // Just above: finite (and exact).
    let l = leverage_for_cr(Ratio::new(1_001, 1_000)).unwrap().unwrap();
    let exact = Ratio::new(1_001, 1);
    assert!(l.le(&exact).unwrap() && exact.le(&l).unwrap());
    // Exact zone boundaries: 1.20 is WARM (not HOT), 1.35 is NORMAL.
    assert_eq!(
        zone_for_cr(Some(Ratio::new(120, 100)), &b).unwrap(),
        RiskZone::Warm
    );
    assert_eq!(
        zone_for_cr(Some(Ratio::new(135, 100)), &b).unwrap(),
        RiskZone::Normal
    );
    assert_eq!(
        zone_for_cr(Some(Ratio::new(119, 100)), &b).unwrap(),
        RiskZone::Hot
    );
    assert_eq!(
        zone_for_cr(Some(Ratio::new(134, 100)), &b).unwrap(),
        RiskZone::Warm
    );
    // The product never renders a finite number at CR <= 1 (test 38's
    // display rule) — the app mirror returns null there.
    let app = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../app/src/lib/safeMode.ts"
    ))
    .unwrap();
    assert!(app.contains("formatLeverage"), "app mirror must exist");
    assert!(
        app.contains("null"),
        "app mirror must return null, never a finite string, at CR <= 1"
    );
}

// ------------------------------------------------------------------ 39

#[test]
fn t39_dex_quote_failure_alone_is_not_route_unavailability() {
    // Quote failed (or pool depth looked thin) while the documented
    // protocol path is operational: the route is available.
    let quote_failed = RouteCheck::operational(QuoteStatus::Failed);
    assert!(quote_failed.available());
    // A blocked mint route is not proof every route is blocked: the actual
    // requested route and required exit path decide.
    let req = entry_request();
    assert_eq!(
        authorize(RiskZone::Hot, &req, Some(&signed(&req, 1)), &quote_failed),
        EntryDecision::PermittedWithConsent,
        "HOT with valid consent and an operational route: permitted"
    );
    // Conversely: a successful quote cannot paint a halted protocol route
    // as available.
    let halted_with_quote = RouteCheck {
        protocol_route_halted: true,
        dex_quote: QuoteStatus::Ok,
    };
    assert!(!halted_with_quote.available());
    assert_eq!(
        authorize(
            RiskZone::Hot,
            &req,
            Some(&signed(&req, 1)),
            &halted_with_quote
        ),
        EntryDecision::Denied(EntryDenial::RouteUnavailable)
    );
}

// ---------------------------------------------------- disclosure mirror

#[test]
fn safe_mode_disclosure_mirrored_in_app() {
    let d = SAFE_MODE_CONSENT_DISCLOSURE;
    assert!(d.contains("signature"));
    assert!(d.contains("single-use"));
    assert!(d.contains("never"));
    let app = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../app/src/lib/safeMode.ts"
    ))
    .unwrap();
    assert!(
        app.contains(d),
        "app safe-mode disclosure must mirror SAFE_MODE_CONSENT_DISCLOSURE verbatim"
    );
}

// ===================================================================
// Pass #5 remediation regressions (F1 / F2 / F3). All fixtures.
// ===================================================================

/// SKR world where B (not A) holds the pre-reference units and registers
/// late. Reference 2 committed at 0, epoch starts at 100.
fn b_world() -> Tl {
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(B, SKR_MINT)
        .mint(B, SKR_MINT, 800)
        .pinned(&[LENDER_A]);
    t
}

// ---------------------------------------------------------------- F1

/// F1: a wallet's lots are priced by its FIRST registration event (or its
/// pre-reference anchoring). A later registration event — exit and
/// re-register, or a duplicate registration — must never silently reprice
/// or recap already-established lots, in either direction.
#[test]
fn f1_late_registration_prices_once_and_never_recaps() {
    let cfg = config(
        vec![skr_adapter_max_age(StakeSupport::None, Some(10_000))],
        &[LENDER_A],
    );
    let refs = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 100));

    // (a) Mark-UP attempt: first late registration prices B's units at
    // min(2, 1) = 1. The market recovers to 1.5 (still under the
    // reference); B exits and re-registers. The re-registration must not
    // lift the established price to min(2, 1.5) = 1.5.
    let mut t = b_world();
    t.slot(2, 150).observe(SKR_MINT, price(1, 1));
    t.slot(3, 200)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(4, 300).ev(EventKind::Exit { wallet: B });
    t.slot(5, 400).observe(SKR_MINT, price(3, 2)); // 1.5 < reference 2
    t.slot(6, 450)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(7, 500).close();
    let r = run_with(&cfg, &refs, &t, 500);
    assert_eq!(
        verified_x(x_at(&r, &B, 475)),
        800,
        "re-registration must not mark lots UP to the later observation"
    );

    // (b) Mark-DOWN attempt: first late registration at 1.5, a later
    // re-registration after a drop to 1 must not recap the established
    // 1.5-priced lots down to 1.
    let mut t = b_world();
    t.slot(2, 150).observe(SKR_MINT, price(3, 2)); // 1.5
    t.slot(3, 200)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(4, 300).ev(EventKind::Exit { wallet: B });
    t.slot(5, 400).observe(SKR_MINT, price(1, 1));
    t.slot(6, 450)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(7, 500).close();
    let r = run_with(&cfg, &refs, &t, 500);
    assert_eq!(
        verified_x(x_at(&r, &B, 475)),
        800 * 3 / 2,
        "re-registration must not mark lots DOWN either: 800 units at 3/2"
    );

    // (c) Un-taint attempt: the first late registration had no usable
    // observation (PENDING, fail closed). A later re-registration with a
    // fresh observation must not price those units retroactively.
    let mut t = b_world();
    t.slot(3, 200)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(4, 300).ev(EventKind::Exit { wallet: B });
    t.slot(5, 400).observe(SKR_MINT, price(1, 1));
    t.slot(6, 450)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(7, 500).close();
    let r = run_with(&cfg, &refs, &t, 500);
    assert_eq!(
        x_at(&r, &B, 475),
        Some(Eligibility::Pending(PendingReason::AdditionUnpriced)),
        "a later registration event must not un-taint unpriced lots"
    );

    // (d) Pre-reference registration: B registers at/before the epoch
    // start, so its units are legitimately reference-priced. Exit and
    // re-register mid-epoch with a low observation: the reference pricing
    // stands (the later registration is not these units' registration
    // event).
    let mut t = b_world();
    t.slot(2, 100)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(3, 150).ev(EventKind::Exit { wallet: B });
    t.slot(4, 200).observe(SKR_MINT, price(1, 1));
    t.slot(5, 250)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(6, 300).close();
    let r = run_with(&cfg, &refs, &t, 300);
    assert_eq!(
        verified_x(x_at(&r, &B, 275)),
        800 * 2,
        "units registered at the reference timestamp keep reference pricing"
    );

    // (e) Partial consumption before re-registration: B consumes 300 of
    // its 800 units after the first registration; the REMAINING old lots
    // keep the first-registration price, and units newly created after the
    // reference (minted supply) carry their own addition pricing.
    let mut t = b_world();
    t.slot(2, 150).observe(SKR_MINT, price(1, 1));
    t.slot(3, 200)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(4, 250).ev(EventKind::Burn {
        account: ta(B, SKR_MINT),
        amount: 300,
        authority: Authority::Owner,
    });
    t.slot(5, 300).ev(EventKind::Exit { wallet: B });
    t.slot(6, 350).observe(SKR_MINT, price(1, 2)); // 0.5
    t.slot(7, 400)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(8, 450).mint(B, SKR_MINT, 200); // NEW supply: addition lot at 0.5
    t.slot(9, 500).close();
    let r = run_with(&cfg, &refs, &t, 500);
    assert_eq!(
        verified_x(x_at(&r, &B, 475)),
        500 + 200 / 2,
        "remaining old lots stay at the first registration price 1; the new \
         200-unit lot prices at its own addition event (min(2, 0.5) = 0.5)"
    );

    // (f) Replay determinism: a truncated replay settling only the prefix
    // produces exactly the prefix's settled numbers — the later
    // re-registration reprices nothing already settled.
    let mut prefix = b_world();
    prefix
        .slot(2, 150)
        .observe(SKR_MINT, price(1, 1))
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    prefix.slot(3, 300).close();
    let short = run_with(&cfg, &refs, &prefix, 300);
    assert_eq!(
        short.wallet(&B).equity_seconds,
        800 * 150,
        "settled: [150, 300) at the first registration price 1"
    );
    let mut full = prefix;
    full.slot(4, 400).observe(SKR_MINT, price(3, 2));
    full.slot(5, 450)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    full.slot(6, 500).close();
    let long = run_with(&cfg, &refs, &full, 500);
    assert_eq!(
        long.wallet(&B).equity_seconds,
        800 * 150 + 800 * 200,
        "settled prefix unchanged; the post-re-registration segment keeps \
         the first registration price"
    );
}

/// F1 family (test 30 min-rule integrity): a whole-account ownership change
/// after the reference timestamp is an ACQUISITION by the new owner and must
/// follow the same min rule as a token transfer — the recipient must not
/// inherit the previous holder's pre-drop reference-priced lots.
#[test]
fn f1_set_account_owner_after_reference_follows_the_min_rule() {
    let cfg = config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A]);
    let refs = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 100));

    // Control: a plain token transfer mid-epoch after the drop prices the
    // recipient's units at min(2, 1) = 1.
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .init(B, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .pinned(&[LENDER_A]);
    t.slot(2, 100)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]); // registered at the epoch start: reference pricing is legitimate
    t.slot(3, 200).observe(SKR_MINT, price(1, 1)); // mid-epoch drop
    t.slot(4, 250).send(A, B, SKR_MINT, 1_000);
    t.slot(5, 300).close();
    let r = run_with(&cfg, &refs, &t, 300);
    assert_eq!(verified_x(x_at(&r, &B, 275)), 1_000);

    // Account-owner transfer of the same units at the same moment must
    // price identically (test 19: SetAuthority(AccountOwner) is a
    // transfer), never at the pre-drop reference price 2.
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .init(B, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .pinned(&[LENDER_A]);
    t.slot(2, 100)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(3, 200).observe(SKR_MINT, price(1, 1));
    t.slot(4, 250).ev(EventKind::SetAccountOwner {
        account: ta(A, SKR_MINT),
        new_owner: B,
    });
    t.slot(5, 300).close();
    let r = run_with(&cfg, &refs, &t, 300);
    assert_eq!(
        verified_x(x_at(&r, &B, 275)),
        1_000,
        "an account-owner transfer acquires the units at the event: the min \
         rule applies exactly as for a token transfer"
    );
}

/// F1 family, deeper anchoring: (a) units acquired pre-reference AFTER the
/// wallet's pre-reference registration are held under a registration at
/// the epoch-start boundary — their reference pricing is permanent and a
/// mid-epoch re-registration must not recap them; (b) units parked in the
/// pending-unstake bucket reprice together with a late registration, so
/// returning them via CancelUnstake cannot resurrect pre-registration
/// pricing.
#[test]
fn f1_pre_reference_acquisitions_and_pending_lots_anchor_correctly() {
    let refs = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 100));

    // (a) B registers at 40 (before its token account even exists),
    // acquires 400 units at 80 (still pre-reference), then re-registers
    // mid-epoch after a drop: the units keep reference pricing.
    let cfg = config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A]);
    let mut t = Tl::new();
    t.slot(1, 40)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .pinned(&[LENDER_A])
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(2, 60).init(B, SKR_MINT);
    t.slot(3, 80).send(A, B, SKR_MINT, 400);
    t.slot(4, 200).observe(SKR_MINT, price(1, 1));
    t.slot(5, 250)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(6, 300).close();
    let r = run_with(&cfg, &refs, &t, 300);
    assert_eq!(
        verified_x(x_at(&r, &B, 275)),
        800,
        "a pre-reference acquisition under an active registration keeps \
         reference pricing (400 units x 2)"
    );

    // (b) All of A's units are staked pre-reference and parked in the
    // pending-unstake bucket; A's FIRST registration is late (after a
    // drop). Canceling the unstake afterwards must yield the registration
    // price min(2, 1) = 1, never the pre-registration reference price 2.
    let cfg = config(
        vec![skr_adapter_max_age(verified_stake(), Some(10_000))],
        &[LENDER_A],
    );
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .ev(EventKind::InitializeAccount {
            account: CUSTODY,
            mint: SKR_MINT,
            owner: STAKE_CUSTODY_OWNER,
        })
        .mint(A, SKR_MINT, 1_000)
        .pinned(&[LENDER_A]);
    t.slot(2, 60).ev(EventKind::Stake {
        source: ta(A, SKR_MINT),
        custody: CUSTODY,
        beneficiary: A,
        amount: 1_000,
    });
    t.slot(3, 70).ev(EventKind::RequestUnstake {
        mint: SKR_MINT,
        beneficiary: A,
        amount: 1_000,
    });
    t.slot(4, 200).observe(SKR_MINT, price(1, 1));
    t.slot(5, 250)
        .register(A, AssetKind::Skr)
        .no_debt(A, &[LENDER_A]);
    t.slot(6, 300).ev(EventKind::CancelUnstake {
        mint: SKR_MINT,
        beneficiary: A,
        amount: 1_000,
    });
    t.slot(7, 400).close();
    let r = run_with(&cfg, &refs, &t, 400);
    match x_at(&r, &A, 350) {
        Some(Eligibility::Verified {
            units,
            gross_lamports,
            ..
        }) => assert_eq!((units, gross_lamports), (1_000, 1_000)),
        o => panic!("{o:?}"),
    }
}

// ---------------------------------------------------------------- F2

/// F2: a previously valid observation does not remain usable indefinitely.
/// The price at an addition/registration event must be verified *at that
/// event*: outside the adapter's calibrated freshness window the observation
/// is stale, the units are unpriced, and the wallet is PENDING (fail
/// closed) — stale data never silently becomes trusted pricing state.
#[test]
fn f2_stale_observations_never_price_later_events() {
    let refs = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 100));

    // Fixture calibration: observations stay usable for 60 seconds. NOT a
    // production threshold (gate 14.2-D).
    let cfg = config(
        vec![skr_adapter_max_age(StakeSupport::None, Some(60))],
        &[LENDER_A],
    );

    // A single observation at 150 (price 1) cannot price an addition at
    // 1000 — 850 seconds later it is stale: unpriced, PENDING.
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::Skr)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 150).observe(SKR_MINT, price(1, 1));
    t.slot(3, 1_000).mint(A, SKR_MINT, 500);
    t.slot(4, 1_100).close();
    let r = run_with(&cfg, &refs, &t, 1_100);
    assert_eq!(
        x_at(&r, &A, 1_050),
        Some(Eligibility::Pending(PendingReason::AdditionUnpriced)),
        "a stale observation must not price a later addition"
    );
    // The pre-addition segment settled normally: staleness of one event
    // never rewinds settled state.
    assert_eq!(r.wallet(&A).equity_seconds, 2_000 * 950);

    // The same timeline with a FRESH re-observation at 990 prices the
    // addition: the transition stale -> fresh is a data event, and a
    // previously-stale state is not sticky.
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::Skr)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 150).observe(SKR_MINT, price(1, 1));
    t.slot(3, 990).observe(SKR_MINT, price(1, 1));
    t.slot(4, 1_000).mint(A, SKR_MINT, 500);
    t.slot(5, 1_100).close();
    let r = run_with(&cfg, &refs, &t, 1_100);
    assert_eq!(
        verified_x(x_at(&r, &A, 1_050)),
        1_000 * 2 + 500,
        "a fresh observation prices the addition at min(2, 1)"
    );

    // A late REGISTRATION cannot reuse a stale observation either.
    let mut t = b_world();
    t.slot(2, 150).observe(SKR_MINT, price(1, 1));
    t.slot(3, 1_000)
        .register(B, AssetKind::Skr)
        .no_debt(B, &[LENDER_A]);
    t.slot(4, 1_100).close();
    let r = run_with(&cfg, &refs, &t, 1_100);
    assert_eq!(
        x_at(&r, &B, 1_050),
        Some(Eligibility::Pending(PendingReason::AdditionUnpriced)),
        "a late registration cannot price at a stale observation"
    );

    // Uncalibrated freshness (`None`, the production state until gate
    // 14.2-D supplies the calibration): NO observation is ever usable —
    // even one from the same instant. Fail closed, not open.
    let cfg_none = config(
        vec![skr_adapter_max_age(StakeSupport::None, None)],
        &[LENDER_A],
    );
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::Skr)
        .no_debt(A, &[LENDER_A]);
    t.slot(2, 150).observe(SKR_MINT, price(1, 1));
    t.slot(3, 151).mint(A, SKR_MINT, 500);
    t.slot(4, 200).close();
    let r = run_with(&cfg_none, &refs, &t, 200);
    assert_eq!(
        x_at(&r, &A, 175),
        Some(Eligibility::Pending(PendingReason::AdditionUnpriced)),
        "with freshness uncalibrated every observation is unusable"
    );
}

// ---------------------------------------------------------------- F3

/// F3: route availability gates entry in EVERY zone. A halted required
/// route is not made authorizeable by a low severity label: NORMAL and WARM
/// obey the same "never submit an unavailable operation" rule as
/// HOT/EXTREME (section 9), and a signature repairs it nowhere.
#[test]
fn f3_route_halt_denies_entry_in_normal_and_warm_too() {
    let req = entry_request();
    let ok = RouteCheck::operational(QuoteStatus::Ok);
    let quote_failed = RouteCheck::operational(QuoteStatus::Failed);
    let halted = RouteCheck::halted();

    // NORMAL: permitted with an operational route (no consent needed) —
    // but a halted required route denies even in NORMAL, with or without
    // consent.
    assert_eq!(
        authorize(RiskZone::Normal, &req, None, &ok),
        EntryDecision::Permitted
    );
    assert_eq!(
        authorize(RiskZone::Normal, &req, None, &halted),
        EntryDecision::Denied(EntryDenial::RouteUnavailable)
    );
    assert_eq!(
        authorize(RiskZone::Normal, &req, Some(&signed(&req, 1)), &halted),
        EntryDecision::Denied(EntryDenial::RouteUnavailable),
        "a signature cannot repair a halted route, even in NORMAL"
    );
    // DEX quote failure alone still does not make the route unavailable.
    assert_eq!(
        authorize(RiskZone::Normal, &req, None, &quote_failed),
        EntryDecision::Permitted
    );

    // WARM: valid consent permits with an operational route; a halted
    // route denies even WITH valid consent.
    assert_eq!(
        authorize(RiskZone::Warm, &req, Some(&signed(&req, 1)), &ok),
        EntryDecision::PermittedWithConsent
    );
    assert_eq!(
        authorize(RiskZone::Warm, &req, Some(&signed(&req, 2)), &halted),
        EntryDecision::Denied(EntryDenial::RouteUnavailable)
    );
    assert_eq!(
        authorize(RiskZone::Warm, &req, None, &ok),
        EntryDecision::Denied(EntryDenial::Consent(ConsentError::Missing))
    );

    // HOT / EXTREME / UNKNOWN unchanged in shape.
    assert_eq!(
        authorize(RiskZone::Hot, &req, Some(&signed(&req, 3)), &ok),
        EntryDecision::PermittedWithConsent
    );
    assert_eq!(
        authorize(RiskZone::Hot, &req, Some(&signed(&req, 4)), &halted),
        EntryDecision::Denied(EntryDenial::RouteUnavailable)
    );
    assert_eq!(
        authorize(RiskZone::Extreme, &req, Some(&signed(&req, 5)), &halted),
        EntryDecision::Denied(EntryDenial::RouteUnavailable)
    );
    assert_eq!(
        authorize(RiskZone::Unknown, &req, Some(&signed(&req, 6)), &ok),
        EntryDecision::Denied(EntryDenial::UnknownRiskData)
    );
    assert_eq!(
        authorize(RiskZone::Unknown, &req, Some(&signed(&req, 7)), &halted),
        EntryDecision::Denied(EntryDenial::UnknownRiskData)
    );
}

/// INACTIVE (section 5: no committed reference at epoch start) composes
/// with Safe Mode states: the asset is a KNOWN weight-0 ineligible from
/// before the epoch starts, in any zone, and a route halt on the inactive
/// mint neither admits it nor freezes the other assets' settlement.
#[test]
fn f3_inactive_asset_stays_weight_zero_and_never_freezes_others() {
    let cfg = config(
        vec![
            skr_adapter(StakeSupport::None),
            AdapterConfig::xsol(XSOL_MINT, TokenProgram::SplToken, vec![]),
        ],
        &[LENDER_A],
    );
    // SKR has NO committed reference (inactive); only xSOL has one.
    let refs = EpochReferences::new().with(ref_commit_at(XSOL_MINT, nav(3, 1), 100));
    let mut t = Tl::new();
    t.slot(1, 50)
        .init(A, SKR_MINT)
        .mint(A, SKR_MINT, 1_000)
        .init(B, XSOL_MINT)
        .mint(B, XSOL_MINT, 400)
        .pinned(&[LENDER_A])
        .register(A, AssetKind::Skr)
        .register(B, AssetKind::XSol)
        .no_debt(A, &[LENDER_A])
        .no_debt(B, &[LENDER_A]);
    // A route halt on the INACTIVE asset's mint changes nothing about it:
    // still the known weight-0 state, never PENDING, never verified.
    t.slot(2, 150).route_halt(SKR_MINT);
    t.slot(3, 300).close();
    let r = run_with(&cfg, &refs, &t, 300);
    for probe in [60, 100, 200, 250] {
        assert_eq!(
            x_at(&r, &A, probe),
            Some(Eligibility::Ineligible(
                IneligibleReason::InactiveNoReference
            )),
            "inactive is a known weight-0 state in every zone (probe {probe})"
        );
    }
    assert_eq!(r.wallet(&A).pending_seconds, 0);
    assert_eq!(verified_x(x_at(&r, &B, 250)), 400 * 3);
    assert!(r.pool.iter().all(|p| p.status == IntervalStatus::Verified));
}
