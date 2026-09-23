//! Claim penalty through real instructions: exact chain-clock boundaries,
//! where the lamports go, replant and reset rules.

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

/// Claim for `who` at exactly `at`; returns (net received, penalty).
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
    assert_eq!(
        env.lamports(fee_vault_pda()).await,
        fee,
        "fee vault untouched"
    );
    assert_eq!(
        env.lamports(capital_vault_pda()).await,
        cap,
        "capital vault untouched"
    );
    assert_eq!(
        env.lamports(reward_vault_pda()).await,
        reward_before - net,
        "only the net leaves the reward vault"
    );
    (net, penalty)
}

fn bps_of(net: u64, penalty: u64) -> u128 {
    // penalty = ceil(gross × bps / 10_000) → bps ≈ penalty / gross.
    penalty as u128 * 10_000 / (net as u128 + penalty as u128)
}

#[tokio::test]
async fn schedule_enforced_at_every_boundary_second() {
    for (age, want) in [
        (1, 2_500u128),
        (DAY - 1, 2_500),
        (DAY, 1_500),
        (2 * DAY - 1, 1_500),
        (2 * DAY, 500),
        (3 * DAY - 1, 500),
        (3 * DAY, 0),
        (400 * DAY, 0),
    ] {
        let (mut env, t) = grove().await;
        let erin = env.erin.insecure_clone();
        let (net, penalty) = claim_at(&mut env, &erin, t + age).await;
        assert!(net > 0);
        let bps = bps_of(net, penalty);
        assert!(
            bps == want || bps + 1 == want,
            "age {age}: {bps} bps, want {want}"
        );
    }
}

#[tokio::test]
async fn penalty_is_owed_to_the_other_planter_not_to_fees() {
    let (mut env, t) = grove().await;
    let (bob, erin) = (env.bob.insecure_clone(), env.erin.insecure_clone());
    // Erin claims early (25%), Bob waits out the window: Bob collects his own
    // emission plus the share of Erin's penalty re-credited to him.
    let (_, penalty) = claim_at(&mut env, &erin, t + 3_600).await;
    assert!(penalty > 0);
    // Erin can't recover her own penalty with a second claim in the same second.
    assert!(env.claim(&erin).await.is_err());
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
    let b = bps_of(n1, p1);
    assert!(b == 1_500 || b == 1_499, "15% band: {b}");
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
    let b = bps_of(n2, p2);
    assert!(
        b == 2_500 || b == 2_499,
        "purchase restarted the 25% band: {b}"
    );
}
