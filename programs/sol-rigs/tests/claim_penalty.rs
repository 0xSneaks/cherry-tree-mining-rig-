//! Early-claim fee through real instructions (architecture §7a item 5;
//! tests 85–87): exact chain-clock boundaries, the 60/40 treasury/grove
//! split with real lamport movement, replant and reset rules.

mod common;

use common::*;
use solana_sdk::signature::Signer;

const DAY: i64 = 86_400;

/// Activated grove, two public planters with equal seeds bought at the same
/// chain second. Returns (env, purchase time).
async fn grove() -> (Env, i64) {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    env.fund_rewards(30 * SOL, 30 * DAY as u64).await.unwrap();
    let f = env.founder.insecure_clone();
    env.buy(&f, 1).await.unwrap(); // activation
    let t0 = env.config().await.launch_t0;
    env.set_time(t0 + 2 * DAY).await; // public
    let (bob, erin) = (env.bob.insecure_clone(), env.erin.insecure_clone());
    env.buy(&bob, 100).await.unwrap();
    env.buy(&erin, 100).await.unwrap();
    let t = env.clock().await.unix_timestamp;
    assert_eq!(env.miner(&erin.pubkey()).await.last_planted_at, t);
    (env, t)
}

/// Claim for `who` at exactly `at`; returns (net received, fee x). Asserts
/// the exact lamport routing: net to the claimant, `floor(0.60 x)` REWARD →
/// FEE, the rest stays in REWARD, CAPITAL untouched.
async fn claim_at(env: &mut Env, who: &solana_sdk::signature::Keypair, at: i64) -> (u64, u64) {
    env.set_time(at).await;
    let (fee, cap) = (
        env.lamports(fee_vault_pda()).await,
        env.lamports(capital_vault_pda()).await,
    );
    let reward_before = env.lamports(reward_vault_pda()).await;
    let penalties_before = env.config().await.total_claim_penalties;
    let wallet_before = env.lamports(who.pubkey()).await;
    env.claim(who).await.unwrap();
    let net = env.lamports(who.pubkey()).await - wallet_before;
    let penalty = env.config().await.total_claim_penalties - penalties_before;
    let treasury = penalty * 6 / 10;
    assert_eq!(
        env.lamports(fee_vault_pda()).await,
        fee + treasury,
        "exactly floor(0.60 x) reaches FEE"
    );
    assert_eq!(
        env.lamports(capital_vault_pda()).await,
        cap,
        "capital vault untouched"
    );
    assert_eq!(
        env.lamports(reward_vault_pda()).await,
        reward_before - net - treasury,
        "net and the treasury share leave REWARD; the grove share stays"
    );
    (net, penalty)
}

/// `x = floor(gross × bps / 10_000)`: recover the exact rate band.
fn assert_band(net: u64, penalty: u64, want: u64) {
    let gross = net + penalty;
    assert_eq!(
        penalty,
        (gross as u128 * want as u128 / 10_000) as u64,
        "want {want} bps"
    );
}

#[tokio::test]
async fn schedule_enforced_at_every_boundary_second() {
    for (age, want) in [
        (1, 3_000u64),
        (DAY - 1, 3_000),
        (DAY, 2_000),
        (2 * DAY - 1, 2_000),
        (2 * DAY, 1_000),
        (3 * DAY - 1, 1_000),
        (3 * DAY, 0),
        (400 * DAY, 0),
    ] {
        let (mut env, t) = grove().await;
        let erin = env.erin.insecure_clone();
        let (net, penalty) = claim_at(&mut env, &erin, t + age).await;
        assert!(net > 0);
        assert_band(net, penalty, want);
    }
}

#[tokio::test]
async fn fee_splits_sixty_forty_treasury_and_grove() {
    let (mut env, t) = grove().await;
    let (bob, erin) = (env.bob.insecure_clone(), env.erin.insecure_clone());
    // Erin claims early (30%): 60% of her fee is treasury revenue in FEE, 40%
    // is re-credited to Bob. Bob's accrual jumps by the grove share.
    let cfg0 = env.config().await;
    let (_, penalty) = claim_at(&mut env, &erin, t + 3_600).await;
    assert!(penalty > 0);
    let cfg = env.config().await;
    let treasury = penalty * 6 / 10;
    assert_eq!(
        cfg.total_reward_to_fees - cfg0.total_reward_to_fees,
        treasury
    );
    assert_eq!(cfg.total_protocol_fees - cfg0.total_protocol_fees, treasury);
    assert_eq!(
        cfg.reserved_commissions, cfg0.reserved_commissions,
        "the treasury share is never referral-eligible"
    );
    // Erin's watermark is set after her penalty was re-credited, so none of
    // it is owed back to her (clock-independent; the same-second repeat
    // claim is covered in the engine suite).
    let m = env.miner(&erin.pubkey()).await;
    assert_eq!(m.pending_rewards, 0);
    assert_eq!(
        m.reward_per_rig_paid,
        env.config().await.reward_per_rig_stored,
        "claimer's watermark includes its own penalty"
    );
    // Bob's ore bin now holds his emission plus the share of Erin's penalty
    // re-credited to him; claiming it pays out of the reward vault only.
    let (bob_net, bob_penalty) = claim_at(&mut env, &bob, t + 3_600).await;
    assert!(bob_net > 0 && bob_penalty > 0);
    let cfg = env.config().await;
    assert!(cfg.total_reward_claimed <= cfg.total_reward_funded);
}

#[tokio::test]
async fn replant_is_never_penalised_and_keeps_the_clock() {
    let (mut env, t) = grove().await;
    let erin = env.erin.insecure_clone();
    env.set_time(t + 20 * 3_600).await;
    let penalties = env.config().await.total_claim_penalties;
    env.compound(&erin).await.unwrap();
    assert_eq!(
        env.config().await.total_claim_penalties,
        penalties,
        "no penalty on replant"
    );
    assert_eq!(
        env.miner(&erin.pubkey()).await.last_planted_at,
        t,
        "replant keeps the clock"
    );
    let (_, penalty) = claim_at(&mut env, &erin, t + 3 * DAY).await;
    assert_eq!(penalty, 0);
}

#[tokio::test]
async fn buying_resets_the_clock_claiming_does_not() {
    let (mut env, t) = grove().await;
    let erin = env.erin.insecure_clone();
    let (n1, p1) = claim_at(&mut env, &erin, t + DAY + 60).await;
    assert_band(n1, p1, 2_000);
    assert_eq!(
        env.miner(&erin.pubkey()).await.last_planted_at,
        t,
        "claim keeps the clock"
    );
    env.set_time(t + 3 * DAY - 60).await;
    env.buy(&erin, 1).await.unwrap();
    let reset = env.clock().await.unix_timestamp;
    assert_eq!(env.miner(&erin.pubkey()).await.last_planted_at, reset);
    let (n2, p2) = claim_at(&mut env, &erin, reset + 3_600).await;
    // Test 87: all pending, older rewards included, back under 30%.
    assert_band(n2, p2, 3_000);
}
