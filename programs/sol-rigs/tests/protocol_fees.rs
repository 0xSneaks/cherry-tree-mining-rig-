//! Protocol fee architecture through real instructions on a real runtime
//! (architecture §7a; tests 68–72, 78–84, 90, 91 and the fee half of 6).
//!
//! Every scenario checks, after every money-moving step, the fee-vault
//! identities of §7 / §7a item 11:
//!
//! ```text
//! distributable(FEE)  = total_protocol_fees - total_fees_withdrawn - total_commissions_paid
//! reserved            = total_commissions_reserved - total_commissions_paid
//!                     = Σ_referrers (accrued_cum - claimed_cum)
//! distributable(FEE) >= reserved
//! CAPITAL             = rent + total_capital_received        (never debited)
//! ```

mod common;

use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use common::*;
use sol_rigs::error::RigsError;
use sol_rigs::state::ReferralLedger;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};

const DAY: i64 = 86_400;
const TIMELOCK: i64 = 259_200;

// ------------------------------------------------------------------ setup

/// Initialized with `(purchase, compound)` fees, funded, activated by the
/// founder (alice, 10 seeds), and moved into the public phase.
async fn grove(purchase: u16, compound: u16) -> Env {
    let mut env = Env::new().await;
    env.initialize_with_fees(purchase, compound).await.unwrap();
    env.fund_rewards(30 * SOL, 30 * DAY as u64).await.unwrap();
    let f = env.founder.insecure_clone();
    env.buy(&f, 10).await.unwrap();
    let t0 = env.config().await.launch_t0;
    env.set_time(t0 + 2 * DAY).await;
    env
}

async fn rent_of(env: &mut Env, key: Pubkey) -> u64 {
    let acc = env.account_raw(key).await.unwrap();
    let rent = env.ctx.banks_client.get_rent().await.unwrap();
    rent.minimum_balance(acc.data.len())
}

async fn fee_distributable(env: &mut Env) -> u64 {
    env.lamports(fee_vault_pda()).await - rent_of(env, fee_vault_pda()).await
}

async fn ledger(env: &mut Env, referrer: &Pubkey) -> Option<ReferralLedger> {
    let acc = env.account_raw(referral_pda(referrer)).await?;
    Some(ReferralLedger::try_deserialize(&mut acc.data.as_slice()).unwrap())
}

async fn unpaid(env: &mut Env, referrer: &Pubkey) -> u64 {
    ledger(env, referrer).await.map(|l| l.unpaid()).unwrap_or(0)
}

/// The fee-vault and capital-vault identities, from on-chain data.
async fn check_books(env: &mut Env, referrers: &[Pubkey]) {
    let cfg = env.config().await;
    let fee = fee_distributable(env).await;
    assert_eq!(
        fee,
        cfg.total_protocol_fees - cfg.total_fees_withdrawn - cfg.total_commissions_paid,
        "FEE = gross protocol fees - withdrawn - commissions paid"
    );
    assert_eq!(
        cfg.reserved_commissions,
        cfg.total_commissions_reserved - cfg.total_commissions_paid
    );
    let mut sum = 0;
    for r in referrers {
        sum += unpaid(env, r).await;
    }
    assert_eq!(sum, cfg.reserved_commissions, "Σ ledgers = reserved");
    assert!(fee >= cfg.reserved_commissions, "FEE covers reservations");
    let cap_rent = rent_of(env, capital_vault_pda()).await;
    assert_eq!(
        env.lamports(capital_vault_pda()).await,
        cap_rent + cfg.total_capital_received,
        "CAPITAL is never debited"
    );
    let reward_rent = rent_of(env, reward_vault_pda()).await;
    assert!(
        env.lamports(reward_vault_pda()).await - reward_rent
            >= cfg.total_reward_funded
                - cfg.total_reward_claimed
                - cfg.total_reward_compounded
                - cfg.total_reward_to_fees,
        "REWARD fully funded"
    );
}

/// Send, returning the error text on failure. (The native `processor!`
/// harness prints `emit!` data to stdout rather than the transaction log, so
/// events are not asserted here; state is.)
async fn send_logs(
    env: &mut Env,
    ix: Instruction,
    signers: &[&Keypair],
) -> Result<Vec<String>, String> {
    let payer = env.ctx.payer.insecure_clone();
    let mut all: Vec<&Keypair> = vec![&payer];
    all.extend_from_slice(signers);
    let bh = env.ctx.banks_client.get_latest_blockhash().await.unwrap();
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &all, bh);
    let r = env
        .ctx
        .banks_client
        .process_transaction_with_metadata(tx)
        .await
        .unwrap();
    let logs = r.metadata.map(|m| m.log_messages).unwrap_or_default();
    match r.result {
        Ok(()) => Ok(logs),
        Err(e) => Err(format!("{e:?} {logs:?}")),
    }
}

fn admin_ix(env: &Env, signer: &Pubkey, data: Vec<u8>) -> Instruction {
    let _ = env;
    Instruction {
        program_id: sol_rigs::ID,
        accounts: sol_rigs::accounts::AdminOnly {
            authority: *signer,
            config: config_pda(),
        }
        .to_account_metas(None),
        data,
    }
}

fn schedule_data(p: u16, c: u16) -> Vec<u8> {
    sol_rigs::instruction::ScheduleFeeChange {
        purchase_fee_bps: p,
        compound_fee_bps: c,
    }
    .data()
}

fn apply_ix(caller: &Pubkey) -> Instruction {
    Instruction {
        program_id: sol_rigs::ID,
        accounts: sol_rigs::accounts::ApplyFeeChange {
            caller: *caller,
            config: config_pda(),
        }
        .to_account_metas(None),
        data: sol_rigs::instruction::ApplyFeeChange {}.data(),
    }
}

fn claim_commission_ix(referrer: &Pubkey) -> Instruction {
    Instruction {
        program_id: sol_rigs::ID,
        accounts: sol_rigs::accounts::ClaimCommission {
            referrer: *referrer,
            config: config_pda(),
            referral_ledger: referral_pda(referrer),
            fee_vault: fee_vault_pda(),
        }
        .to_account_metas(None),
        data: sol_rigs::instruction::ClaimCommission {}.data(),
    }
}

async fn schedule(env: &mut Env, p: u16, c: u16) -> Result<Vec<String>, String> {
    let a = env.authority.insecure_clone();
    let ix = admin_ix(env, &a.pubkey(), schedule_data(p, c));
    send_logs(env, ix, &[&a]).await
}

async fn cancel(env: &mut Env) -> Result<Vec<String>, String> {
    let a = env.authority.insecure_clone();
    let ix = admin_ix(
        env,
        &a.pubkey(),
        sol_rigs::instruction::CancelFeeChange {}.data(),
    );
    send_logs(env, ix, &[&a]).await
}

async fn apply(env: &mut Env, who: &Keypair) -> Result<Vec<String>, String> {
    send_logs(env, apply_ix(&who.pubkey()), &[who]).await
}

fn assert_err(r: Result<Vec<String>, String>, e: RigsError) {
    let code = anchor_lang::error::ERROR_CODE_OFFSET + e as u32;
    let s = r.expect_err("expected failure");
    assert!(
        s.contains(&format!("Custom({code})")),
        "expected {e:?} ({code}), got {s}"
    );
}

/// Buy `n` seeds for `who` (named referrer at the first purchase if given);
/// returns the fee-vault delta.
async fn buy(env: &mut Env, who: &Keypair, n: u64, referrer: Option<&Keypair>) -> u64 {
    let before = env.lamports(fee_vault_pda()).await;
    match referrer {
        Some(r) => {
            let ix = env.buy_ix_with(
                &who.pubkey(),
                n,
                env.identities
                    .get(&who.pubkey())
                    .cloned()
                    .unwrap_or_default(),
                Some((r.pubkey(), miner_pda(&r.pubkey()))),
            );
            env.send(ix, &[who]).await.unwrap();
        }
        None => env.buy(who, n).await.unwrap(),
    }
    env.lamports(fee_vault_pda()).await - before
}

// ------------------------------------------------------------- 78 / 80

/// Test 78: production 1,000 bps; a purchase of n seeds charges exactly
/// floor(n P 1,000 / 10,000) into FEE; any fee above 1,000 is rejected at
/// initialize and in schedule_fee_change.
#[tokio::test]
async fn t78_production_purchase_fee_is_ten_percent_and_capped() {
    let mut env = grove(1_000, 1_000).await;
    let erin = env.erin.insecure_clone();
    let cfg = env.config().await;
    assert_eq!((cfg.purchase_fee_bps, cfg.compound_fee_bps), (1_000, 1_000));
    for n in [1u64, 7, 33] {
        let d = buy(&mut env, &erin, n, None).await;
        assert_eq!(d, n * RIG_PRICE * 1_000 / 10_000);
    }
    assert_err(
        schedule(&mut env, 1_001, 1_000).await,
        RigsError::InvalidConfig,
    );
    assert_err(
        schedule(&mut env, u16::MAX, 0).await,
        RigsError::InvalidConfig,
    );
    assert_eq!(env.config().await.fee_change_effective_at, 0);
    check_books(&mut env, &[]).await;

    for (p, c) in [(1_001u16, 0u16), (1_000, 1_001)] {
        let mut env = Env::new().await;
        let err = env.initialize_with_fees(p, c).await.unwrap_err();
        let want = if p > 1_000 {
            RigsError::InvalidConfig
        } else {
            RigsError::CompoundFeeAbovePurchaseFee
        };
        assert_rigs_error(err, want);
        assert!(env.account_raw(config_pda()).await.is_none());
    }
}

/// Test 80: compound > purchase rejected at initialize and in
/// schedule_fee_change, including a purchase decrease below the current
/// compound fee without a matching compound decrease.
#[tokio::test]
async fn t80_compound_fee_never_above_purchase_fee() {
    let mut env = Env::new().await;
    assert_rigs_error(
        env.initialize_with_fees(800, 900).await.unwrap_err(),
        RigsError::CompoundFeeAbovePurchaseFee,
    );
    let mut env = grove(1_000, 1_000).await;
    assert_err(
        schedule(&mut env, 900, 1_000).await,
        RigsError::CompoundFeeAbovePurchaseFee,
    );
    assert_eq!(env.config().await.fee_change_effective_at, 0);
    schedule(&mut env, 900, 900).await.unwrap();
    assert_eq!(env.config().await.pending_compound_fee_bps, 900);
}

// ------------------------------------------------------------- 79 / 81

/// Test 79: compound charges compound_fee_bps, buy charges
/// purchase_fee_bps (1,000 / 800); the quotes differ exactly by rate.
#[tokio::test]
async fn t79_compound_charges_its_own_fee() {
    let mut env = grove(1_000, 800).await;
    let erin = env.erin.insecure_clone();
    let d = buy(&mut env, &erin, 10, None).await;
    assert_eq!(d, 10 * RIG_PRICE / 10);
    env.warp(DAY).await;
    let (fee0, cap0) = (
        env.lamports(fee_vault_pda()).await,
        env.lamports(capital_vault_pda()).await,
    );
    let m0 = env.miner(&erin.pubkey()).await;
    env.compound(&erin).await.unwrap();
    let m1 = env.miner(&erin.pubkey()).await;
    let minted = m1.rigs - m0.rigs;
    assert!(minted > 0);
    let capital = env.lamports(capital_vault_pda()).await - cap0;
    let fee = env.lamports(fee_vault_pda()).await - fee0;
    assert_eq!(capital, minted * RIG_PRICE);
    assert_eq!(fee, capital * 800 / 10_000);
    let buy_quote = rigs_engine::quote_purchase(minted, RIG_PRICE, 1_000).unwrap();
    assert_eq!(buy_quote.fee_lamports - fee, capital * 200 / 10_000);
    assert_eq!(
        m1.last_planted_at, m0.last_planted_at,
        "compound keeps the anchor"
    );
    check_books(&mut env, &[]).await;
}

/// Test 81: capped (S = 250) and sub-seed compounds move no lamport in any
/// vault; pending rewards and the claim anchor stay unchanged and claimable.
#[tokio::test]
async fn t81_noop_compounds_move_nothing() {
    let mut env = grove(1_000, 1_000).await;
    let erin = env.erin.insecure_clone();
    buy(&mut env, &erin, 250, None).await;
    env.warp(DAY).await;
    let before = vaults3(&mut env).await;
    let m0 = env.miner(&erin.pubkey()).await;
    assert_rigs_error(
        env.compound(&erin).await.unwrap_err(),
        RigsError::WalletCapExceeded,
    );
    assert_eq!(vaults3(&mut env).await, before);
    let m1 = env.miner(&erin.pubkey()).await;
    assert_eq!(
        (m1.rigs, m1.pending_rewards, m1.last_planted_at),
        (m0.rigs, m0.pending_rewards, m0.last_planted_at)
    );

    // Sub-seed: dave just planted; one second of accrual buys nothing.
    let dave = env.dave.insecure_clone();
    buy(&mut env, &dave, 1, None).await;
    env.warp(1).await;
    let before = vaults3(&mut env).await;
    let d0 = env.miner(&dave.pubkey()).await;
    assert_rigs_error(
        env.compound(&dave).await.unwrap_err(),
        RigsError::NothingToCompound,
    );
    assert_eq!(vaults3(&mut env).await, before);
    let d1 = env.miner(&dave.pubkey()).await;
    assert_eq!(
        (d1.rigs, d1.pending_rewards, d1.last_planted_at),
        (d0.rigs, d0.pending_rewards, d0.last_planted_at)
    );
    // Earned SOL stays claimable.
    env.claim(&erin).await.unwrap();
    check_books(&mut env, &[]).await;
}

async fn vaults3(env: &mut Env) -> [u64; 3] {
    [
        env.lamports(fee_vault_pda()).await,
        env.lamports(capital_vault_pda()).await,
        env.lamports(reward_vault_pda()).await,
    ]
}

// ------------------------------------------------------------------- 82

/// Test 82: schedule → pending values and effective_at = now + 259,200
/// readable on-chain, event emitted; at effective_at − 1 buys and compounds
/// use the old fees and apply is rejected; at effective_at a permissionless
/// apply writes both atomically. Reschedule restarts the full delay; cancel
/// works; nothing bypasses the delay (set_pricing refuses fee changes).
#[tokio::test]
async fn t82_fee_timelock_end_to_end() {
    let mut env = grove(1_000, 1_000).await;
    let (erin, frank) = (env.erin.insecure_clone(), env.frank.insecure_clone());
    buy(&mut env, &frank, 20, None).await;

    let t = env.clock().await.unix_timestamp;
    schedule(&mut env, 800, 500).await.unwrap();
    let cfg = env.config().await;
    assert_eq!(
        (
            cfg.pending_purchase_fee_bps,
            cfg.pending_compound_fee_bps,
            cfg.fee_change_effective_at
        ),
        (800, 500, t + TIMELOCK)
    );
    assert_eq!((cfg.purchase_fee_bps, cfg.compound_fee_bps), (1_000, 1_000));

    // effective_at − 1: old fees everywhere, apply refused.
    env.set_time(t + TIMELOCK - 1).await;
    assert_eq!(buy(&mut env, &erin, 10, None).await, 10 * RIG_PRICE / 10);
    let f0 = env.lamports(fee_vault_pda()).await;
    let c0 = env.lamports(capital_vault_pda()).await;
    env.compound(&frank).await.unwrap();
    let cap = env.lamports(capital_vault_pda()).await - c0;
    assert_eq!(env.lamports(fee_vault_pda()).await - f0, cap / 10);
    assert_err(apply(&mut env, &erin).await, RigsError::FeeTimelockActive);

    // effective_at: anyone applies; both values land together.
    env.set_time(t + TIMELOCK).await;
    apply(&mut env, &erin).await.unwrap();
    let cfg = env.config().await;
    assert_eq!((cfg.purchase_fee_bps, cfg.compound_fee_bps), (800, 500));
    assert_eq!(cfg.fee_change_effective_at, 0);
    assert_eq!(
        buy(&mut env, &erin, 10, None).await,
        10 * RIG_PRICE * 8 / 100
    );
    env.warp(1).await;
    assert_err(apply(&mut env, &erin).await, RigsError::NoPendingFeeChange);

    // Reschedule restarts the full delay.
    let t2 = env.clock().await.unix_timestamp;
    schedule(&mut env, 900, 900).await.unwrap();
    env.set_time(t2 + 100).await;
    schedule(&mut env, 700, 700).await.unwrap();
    assert_eq!(
        env.config().await.fee_change_effective_at,
        t2 + 100 + TIMELOCK
    );
    env.set_time(t2 + TIMELOCK).await;
    assert_err(apply(&mut env, &erin).await, RigsError::FeeTimelockActive);

    // Cancel clears it; apply has nothing to apply even much later.
    cancel(&mut env).await.unwrap();
    assert_eq!(env.config().await.fee_change_effective_at, 0);
    env.set_time(t2 + 10 * TIMELOCK).await;
    assert_err(apply(&mut env, &erin).await, RigsError::NoPendingFeeChange);
    assert_err(cancel(&mut env).await, RigsError::NoPendingFeeChange);
    let cfg = env.config().await;
    assert_eq!((cfg.purchase_fee_bps, cfg.compound_fee_bps), (800, 500));

    // Only the authority schedules or cancels.
    let ix = admin_ix(&env, &erin.pubkey(), schedule_data(0, 0));
    assert!(send_logs(&mut env, ix, &[&erin]).await.is_err());
    schedule(&mut env, 0, 0).await.unwrap();
    let ix = admin_ix(
        &env,
        &erin.pubkey(),
        sol_rigs::instruction::CancelFeeChange {}.data(),
    );
    assert!(send_logs(&mut env, ix, &[&erin]).await.is_err());
    assert_ne!(env.config().await.fee_change_effective_at, 0);

    // No bypass: set_pricing refuses any fee change, allows a price change.
    let a = env.authority.insecure_clone();
    let ix = admin_ix(
        &env,
        &a.pubkey(),
        sol_rigs::instruction::SetPricing {
            rig_price_lamports: RIG_PRICE,
            purchase_fee_bps: 100,
        }
        .data(),
    );
    assert_err(
        send_logs(&mut env, ix, &[&a]).await,
        RigsError::FeeChangeRequiresTimelock,
    );
    let ix = admin_ix(
        &env,
        &a.pubkey(),
        sol_rigs::instruction::SetPricing {
            rig_price_lamports: RIG_PRICE * 2,
            purchase_fee_bps: 800,
        }
        .data(),
    );
    send_logs(&mut env, ix, &[&a]).await.unwrap();
    let cfg = env.config().await;
    assert_eq!(
        (cfg.rig_price_lamports, cfg.purchase_fee_bps),
        (RIG_PRICE * 2, 800)
    );
    // Test 6 (price half): Fruit K stays on the frozen P_ref.
    assert_eq!(rigs_engine::P_REF_LAMPORTS, 10_000_000);
    check_books(&mut env, &[]).await;
}

// ------------------------------------------------- 83 / 69 / 70 / 72

/// Tests 83, 69, 70: at 10% a referred wallet's purchase and compound each
/// reserve exactly floor(0.20 f) for the direct referrer, in the instruction
/// that receives the fee; an unreferred wallet reserves nothing; the
/// referrer's own referrer receives nothing (one level); each fee event is
/// consumed once and a failed event reserves nothing.
#[tokio::test]
async fn t83_referral_twenty_eighty_one_level() {
    let mut env = grove(1_000, 1_000).await;
    let (alice, erin, dave, frank) = (
        env.alice.insecure_clone(),
        env.erin.insecure_clone(),
        env.dave.insecure_clone(),
        env.frank.insecure_clone(),
    );
    let refs = [alice.pubkey(), erin.pubkey()];

    // erin ← alice. Fresh purchase.
    let f = buy(&mut env, &erin, 20, Some(&alice)).await;
    assert_eq!(f, 20 * RIG_PRICE / 10);
    assert_eq!(unpaid(&mut env, &alice.pubkey()).await, f / 5);
    assert_eq!(env.config().await.reserved_commissions, f / 5);
    check_books(&mut env, &refs).await;

    // dave ← erin: erin earns; alice (erin's referrer) earns nothing.
    let a0 = unpaid(&mut env, &alice.pubkey()).await;
    let f2 = buy(&mut env, &dave, 10, Some(&erin)).await;
    assert_eq!(unpaid(&mut env, &erin.pubkey()).await, f2 / 5);
    assert_eq!(
        unpaid(&mut env, &alice.pubkey()).await,
        a0,
        "one level only"
    );

    // Unreferred frank: 100% retained.
    let r0 = env.config().await.reserved_commissions;
    buy(&mut env, &frank, 10, None).await;
    assert_eq!(env.config().await.reserved_commissions, r0);
    check_books(&mut env, &refs).await;

    // Test 69: erin's compound fee — reserved in the compound instruction.
    env.warp(DAY).await;
    let before = env.lamports(fee_vault_pda()).await;
    let a1 = unpaid(&mut env, &alice.pubkey()).await;
    let r_before = env.config().await.total_commissions_reserved;
    env.compound(&erin).await.unwrap();
    let cf = env.lamports(fee_vault_pda()).await - before;
    assert!(cf > 0);
    assert_eq!(unpaid(&mut env, &alice.pubkey()).await - a1, cf / 5);
    assert_eq!(
        env.config().await.total_commissions_reserved - r_before,
        cf / 5,
        "one fee event, one reservation"
    );
    check_books(&mut env, &refs).await;

    // Test 70: repeated events accrue once each; a failed event reserves
    // nothing (erin over the cap).
    let r1 = env.config().await.reserved_commissions;
    let err = env.buy(&erin, 250).await.unwrap_err();
    assert_rigs_error(err, RigsError::WalletCapExceeded);
    assert_eq!(env.config().await.reserved_commissions, r1);
    let f3 = buy(&mut env, &erin, 1, None).await;
    let f4 = buy(&mut env, &erin, 1, None).await;
    assert_eq!(
        env.config().await.reserved_commissions,
        r1 + f3 / 5 + f4 / 5
    );
    check_books(&mut env, &refs).await;

    // Test 72 on the hard-cap build: self-referral and a non-planter
    // referrer are rejected (full cases in launch.rs).
    let carol = env.carol.insecure_clone();
    let ix = env.buy_ix_with(
        &carol.pubkey(),
        1,
        env.identities[&carol.pubkey()].clone(),
        Some((carol.pubkey(), miner_pda(&carol.pubkey()))),
    );
    assert_rigs_error(
        env.send(ix, &[&carol]).await.unwrap_err(),
        RigsError::SelfReferral,
    );
    let stranger = Keypair::new();
    let ix = env.buy_ix_with(
        &carol.pubkey(),
        1,
        env.identities[&carol.pubkey()].clone(),
        Some((stranger.pubkey(), miner_pda(&stranger.pubkey()))),
    );
    assert_rigs_error(
        env.send(ix, &[&carol]).await.unwrap_err(),
        RigsError::InvalidReferrer,
    );
}

/// Test 84 / 6 (fee half): after timelocked cuts the clamp shrinks the
/// commission first: at 9% the treasury keeps exactly 8% of volume, at 8% or
/// less the commission is 0 (and no ledger lamport moves).
#[tokio::test]
async fn t84_retained_margin_clamp_after_timelocked_cut() {
    let mut env = grove(1_000, 1_000).await;
    let (alice, erin) = (env.alice.insecure_clone(), env.erin.insecure_clone());
    buy(&mut env, &erin, 10, Some(&alice)).await;
    let t = env.clock().await.unix_timestamp;
    schedule(&mut env, 900, 800).await.unwrap();
    env.set_time(t + TIMELOCK).await;
    apply(&mut env, &erin).await.unwrap();

    let u0 = unpaid(&mut env, &alice.pubkey()).await;
    let f = buy(&mut env, &erin, 100, None).await;
    let v = 100 * RIG_PRICE;
    assert_eq!(f, v * 9 / 100);
    let c = unpaid(&mut env, &alice.pubkey()).await - u0;
    assert_eq!(c, f - (v * 8).div_ceil(100));
    assert_eq!(f - c, v * 8 / 100, "treasury keeps exactly 8% of volume");

    // Compound at 8%: commission 0.
    env.warp(DAY).await;
    let u1 = unpaid(&mut env, &alice.pubkey()).await;
    let r1 = env.config().await.reserved_commissions;
    env.compound(&erin).await.unwrap();
    assert_eq!(unpaid(&mut env, &alice.pubkey()).await, u1);
    assert_eq!(env.config().await.reserved_commissions, r1);
    check_books(&mut env, &[alice.pubkey()]).await;
}

// -------------------------------------------------------- 68 / 91 / 70

/// Tests 68 / 91 / 70: withdraw_fees above distributable − reserved is
/// rejected, exactly the unreserved balance is withdrawable, the referrer's
/// claim pays exactly its unpaid commission once, and the books reconcile.
#[tokio::test]
async fn t68_t91_withdraw_guard_and_commission_claims() {
    let mut env = grove(1_000, 1_000).await;
    let (alice, erin) = (env.alice.insecure_clone(), env.erin.insecure_clone());
    buy(&mut env, &erin, 50, Some(&alice)).await;
    env.warp(DAY).await;
    env.compound(&erin).await.unwrap();
    env.warp(3_600).await;
    env.claim(&erin).await.unwrap(); // early-claim fee: treasury share into FEE
    check_books(&mut env, &[alice.pubkey()]).await;

    let cfg = env.config().await;
    assert!(cfg.reserved_commissions > 0 && cfg.total_reward_to_fees > 0);
    let withdrawable = fee_distributable(&mut env).await - cfg.reserved_commissions;
    let a = env.authority.insecure_clone();
    let ix = env.withdraw_fees_ix(
        &a.pubkey(),
        fee_vault_pda(),
        env.fee_recipient,
        withdrawable + 1,
    );
    assert_rigs_error(
        env.send(ix, &[&a]).await.unwrap_err(),
        RigsError::InsufficientFeeBalance,
    );
    let r0 = env.lamports(env.fee_recipient).await;
    let ix = env.withdraw_fees_ix(
        &a.pubkey(),
        fee_vault_pda(),
        env.fee_recipient,
        withdrawable,
    );
    env.send(ix, &[&a]).await.unwrap();
    assert_eq!(env.lamports(env.fee_recipient).await - r0, withdrawable);
    assert_eq!(
        fee_distributable(&mut env).await,
        cfg.reserved_commissions,
        "only reserved commissions remain"
    );
    // Net treasury revenue = gross protocol fees − referral liabilities.
    let cfg = env.config().await;
    assert_eq!(
        cfg.total_fees_withdrawn,
        cfg.total_protocol_fees - cfg.total_commissions_reserved
    );
    env.warp(1).await;
    let ix = env.withdraw_fees_ix(&a.pubkey(), fee_vault_pda(), env.fee_recipient, 1);
    assert_rigs_error(
        env.send(ix, &[&a]).await.unwrap_err(),
        RigsError::InsufficientFeeBalance,
    );
    check_books(&mut env, &[alice.pubkey()]).await;

    // Paused: the claim waits; the reservation still protects it.
    env.set_paused(true).await.unwrap();
    assert!(env
        .send(claim_commission_ix(&alice.pubkey()), &[&alice])
        .await
        .is_err());
    env.set_paused(false).await.unwrap();
    env.warp(1).await; // fresh blockhash: the claim below is a new transaction

    // Commission claim: exact, once.
    let owed = unpaid(&mut env, &alice.pubkey()).await;
    let w0 = env.lamports(alice.pubkey()).await;
    send_logs(&mut env, claim_commission_ix(&alice.pubkey()), &[&alice])
        .await
        .unwrap();
    assert_eq!(env.lamports(alice.pubkey()).await - w0, owed);
    let cfg = env.config().await;
    assert_eq!(
        (cfg.reserved_commissions, cfg.total_commissions_paid),
        (0, owed)
    );
    env.warp(1).await;
    assert_rigs_error(
        env.send(claim_commission_ix(&alice.pubkey()), &[&alice])
            .await
            .unwrap_err(),
        RigsError::NothingToClaimCommission,
    );
    check_books(&mut env, &[alice.pubkey()]).await;

    // Someone else cannot claim alice's ledger.
    let mut ix = claim_commission_ix(&erin.pubkey());
    ix.accounts[2].pubkey = referral_pda(&alice.pubkey());
    assert!(env.send(ix, &[&erin]).await.is_err());
}

/// The referral ledger is mandatory for a referred position and must be the
/// canonical PDA of the bound referrer.
#[tokio::test]
async fn referred_positions_must_carry_the_right_ledger() {
    let mut env = grove(1_000, 1_000).await;
    let (alice, bob, erin) = (
        env.alice.insecure_clone(),
        env.bob.insecure_clone(),
        env.erin.insecure_clone(),
    );
    buy(&mut env, &erin, 5, Some(&alice)).await;
    // Missing.
    let ix = env.buy_ix_raw(&erin.pubkey(), 1, None);
    assert_rigs_error(
        env.send(ix, &[&erin]).await.unwrap_err(),
        RigsError::MissingReferralLedger,
    );
    let mut ix = env.compound_ix(&erin.pubkey());
    ix.accounts.pop();
    assert_rigs_error(
        env.send(ix, &[&erin]).await.unwrap_err(),
        RigsError::MissingReferralLedger,
    );
    // Another referrer's ledger.
    let mut ix = env.buy_ix_raw(&erin.pubkey(), 1, None);
    ix.accounts
        .push(AccountMeta::new(referral_pda(&bob.pubkey()), false));
    assert_rigs_error(
        env.send(ix, &[&erin]).await.unwrap_err(),
        RigsError::ReferralLedgerInvalid,
    );
    // The canonical one works.
    env.buy(&erin, 1).await.unwrap();
    check_books(&mut env, &[alice.pubkey()]).await;
}

// ------------------------------------------------------------------- 90

/// Test 90: CAPITAL cannot become treasury revenue. No instruction debits
/// it; withdraw_fees cannot name it as source; a treasury destination can
/// never be a program-owned account (so fees cannot be routed into a vault).
#[tokio::test]
async fn t90_capital_never_becomes_treasury_revenue() {
    let mut env = grove(1_000, 1_000).await;
    let (alice, erin) = (env.alice.insecure_clone(), env.erin.insecure_clone());
    buy(&mut env, &erin, 40, Some(&alice)).await;
    env.warp(DAY).await;
    env.compound(&erin).await.unwrap();
    env.claim(&erin).await.unwrap();
    check_books(&mut env, &[alice.pubkey()]).await;

    let a = env.authority.insecure_clone();
    // CAPITAL as the withdrawal source: rejected by seeds / vault kind.
    let ix = env.withdraw_fees_ix(&a.pubkey(), capital_vault_pda(), env.fee_recipient, 1);
    assert!(env.send(ix, &[&a]).await.is_err());
    // Fee recipient pointed at a vault: the payout is refused.
    for vault in [capital_vault_pda(), reward_vault_pda(), fee_vault_pda()] {
        let ix = env.set_fee_recipient_ix(&a.pubkey(), vault);
        env.send(ix, &[&a]).await.unwrap();
        let ix = env.withdraw_fees_ix(&a.pubkey(), fee_vault_pda(), vault, 1);
        assert_rigs_error(
            env.send(ix, &[&a]).await.unwrap_err(),
            RigsError::TreasuryDestinationInvalid,
        );
        env.warp(1).await;
    }
    let cfg = env.config().await;
    assert!(cfg.total_protocol_fees < cfg.total_capital_received);
    check_books(&mut env, &[alice.pubkey()]).await;
}

// ------------------------------------------------- randomized drift hunt

/// Seeded random sequence of buys, compounds, claims, commission claims,
/// withdrawals and fee-change schedule / apply / cancel across two referral
/// chains and an unreferred wallet. After every step, every §7 / §7a
/// identity in `check_books` must hold exactly (no drift, no reserved
/// balance ever withdrawable, CAPITAL never debited, REWARD fully funded).
#[tokio::test]
async fn randomized_sequences_never_drift() {
    for seed in [7u64, 1_234_567, 0xC0FFEE] {
        let mut env = grove(1_000, 1_000).await;
        let (alice, erin, dave, frank) = (
            env.alice.insecure_clone(),
            env.erin.insecure_clone(),
            env.dave.insecure_clone(),
            env.frank.insecure_clone(),
        );
        buy(&mut env, &erin, 5, Some(&alice)).await; // erin ← alice
        buy(&mut env, &dave, 5, Some(&erin)).await; // dave ← erin
        buy(&mut env, &frank, 5, None).await; // unreferred
        let refs = [alice.pubkey(), erin.pubkey()];
        let planters = [&erin, &dave, &frank];
        let mut rng = seed;
        let mut next = || {
            rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            rng >> 33
        };
        for _ in 0..40 {
            let who = planters[(next() % 3) as usize].insecure_clone();
            env.warp(1 + (next() % 20_000) as i64).await;
            match next() % 8 {
                0 | 1 => {
                    let _ = env.buy(&who, 1 + next() % 8).await;
                }
                2 => {
                    let _ = env.compound(&who).await;
                }
                3 => {
                    let _ = env.claim(&who).await;
                }
                4 => {
                    let r = if next() % 2 == 0 { &alice } else { &erin };
                    let before = unpaid(&mut env, &r.pubkey()).await;
                    let w0 = env.lamports(r.pubkey()).await;
                    if env
                        .send(claim_commission_ix(&r.pubkey()), &[r])
                        .await
                        .is_ok()
                    {
                        // The payer is the fixture payer, so the referrer's
                        // balance moves by exactly the commission.
                        assert_eq!(env.lamports(r.pubkey()).await - w0, before);
                        assert_eq!(unpaid(&mut env, &r.pubkey()).await, 0);
                    }
                }
                5 => {
                    // Try to over-withdraw by one lamport, then withdraw the
                    // exact unreserved balance.
                    let reserved = env.config().await.reserved_commissions;
                    let free = fee_distributable(&mut env).await - reserved;
                    let a = env.authority.insecure_clone();
                    let ix = env.withdraw_fees_ix(
                        &a.pubkey(),
                        fee_vault_pda(),
                        env.fee_recipient,
                        free + 1,
                    );
                    assert!(env.send(ix, &[&a]).await.is_err());
                    if free > 0 {
                        env.warp(1).await;
                        let ix = env.withdraw_fees_ix(
                            &a.pubkey(),
                            fee_vault_pda(),
                            env.fee_recipient,
                            free,
                        );
                        env.send(ix, &[&a]).await.unwrap();
                        assert_eq!(fee_distributable(&mut env).await, reserved);
                    }
                }
                6 => {
                    let p = 800 + (next() % 201) as u16;
                    let c = (next() % (p as u64 + 1)) as u16;
                    let _ = schedule(&mut env, p, c).await;
                }
                _ => {
                    let at = env.config().await.fee_change_effective_at;
                    if at != 0 && next() % 2 == 0 {
                        env.set_time(at).await;
                        apply(&mut env, &who).await.unwrap();
                    } else {
                        let _ = cancel(&mut env).await;
                    }
                }
            }
            let cfg = env.config().await;
            assert!(cfg.compound_fee_bps <= cfg.purchase_fee_bps);
            assert!(cfg.purchase_fee_bps <= 1_000);
            check_books(&mut env, &refs).await;
        }
    }
}
