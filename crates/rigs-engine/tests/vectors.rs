//! Emits cross-language test vectors consumed by `app/tests/pricing.test.ts`.
//!
//! Spec §6/§12.29 require the frontend quote and the program to agree exactly.
//! Rather than trust two independent implementations to stay in step, the Rust
//! side is the source of truth and the TypeScript port is checked against
//! these vectors.
//!
//! Regenerate with:
//!   cargo test -p rigs-engine --test vectors -- --nocapture > /tmp/vectors.txt

use rigs_engine::{quote_purchase, rigs_affordable};

#[test]
fn emit_pricing_vectors() {
    let prices = [1u64, 7, 1_000_000, 10_000_000, 10_000_001, 3_333_333];
    let fees = [0u16, 1, 37, 250, 300, 999, 1000];
    let counts = [1u64, 2, 5, 10, 25, 99, 1_000, 123_456];
    let budgets = [
        0u64,
        1,
        9_999_999,
        10_000_000,
        10_000_001,
        128_000_000,
        103_000_000,
        999_999_999,
        123_456_789_012,
    ];

    println!("---QUOTES---");
    for &p in &prices {
        for &f in &fees {
            for &c in &counts {
                let q = quote_purchase(c, p, f).unwrap();
                println!(
                    "{c},{p},{f},{},{},{}",
                    q.capital_lamports, q.fee_lamports, q.total_lamports
                );
            }
        }
    }

    println!("---AFFORDABLE---");
    for &p in &prices {
        for &f in &fees {
            for &b in &budgets {
                println!("{b},{p},{f},{}", rigs_affordable(b, p, f).unwrap());
            }
        }
    }
    println!("---END---");
}

/// Emits accrual vectors consumed by `app/tests/accrual.test.ts`.
///
/// Spec §10/§12.28: the frontend interpolates between reconciliations, and the
/// interpolated figure must reconcile to the on-chain value. The only way to
/// guarantee that is for the client projection to be the same function — so
/// it is checked against the engine's own output, timestamp for timestamp.
#[test]
fn emit_accrual_vectors() {
    use rigs_engine::{MinerPosition, RewardGlobals};

    let scenarios: Vec<(&str, RewardGlobals, MinerPosition)> = vec![
        (
            "steady",
            RewardGlobals {
                total_rigs: 1_000,
                reward_rate: 2_500_000,
                reward_period_finish: 10_000,
                reward_last_update: 0,
                reward_per_rig_stored: 0,
                undistributed_dust: 0,
            },
            MinerPosition {
                rigs: 42,
                reward_per_rig_paid: 0,
                pending_rewards: 0,
                ..Default::default()
            },
        ),
        (
            "mid-window-with-pending",
            RewardGlobals {
                total_rigs: 14_281,
                reward_rate: 25_462_962,
                reward_period_finish: 8_000,
                reward_last_update: 1_200,
                reward_per_rig_stored: 987_654_321_098_765,
                undistributed_dust: 0,
            },
            MinerPosition {
                rigs: 7,
                reward_per_rig_paid: 123_456_789_012,
                pending_rewards: 18_291,
                ..Default::default()
            },
        ),
        (
            "no-rigs",
            RewardGlobals {
                total_rigs: 0,
                reward_rate: 1_000,
                reward_period_finish: 5_000,
                reward_last_update: 0,
                reward_per_rig_stored: 0,
                undistributed_dust: 0,
            },
            MinerPosition::default(),
        ),
        (
            "already-finished",
            RewardGlobals {
                total_rigs: 10,
                reward_rate: 1_000_000,
                reward_period_finish: 500,
                reward_last_update: 500,
                reward_per_rig_stored: 50_000_000_000_000_000,
                undistributed_dust: 0,
            },
            MinerPosition {
                rigs: 3,
                reward_per_rig_paid: 0,
                pending_rewards: 12_345,
                ..Default::default()
            },
        ),
    ];

    let times: [i64; 10] = [0, 1, 599, 1_200, 1_201, 4_999, 5_000, 8_000, 10_000, 99_999];

    println!("---ACCRUAL---");
    for (name, globals, position) in &scenarios {
        for &t in &times {
            let mut g = *globals;
            g.sync(t).unwrap();
            let earned = position.earned(&g).unwrap();
            println!(
                "{name},{t},{},{},{},{},{},{},{},{},{}",
                globals.total_rigs,
                globals.reward_rate,
                globals.reward_period_finish,
                globals.reward_last_update,
                globals.reward_per_rig_stored,
                position.rigs,
                position.reward_per_rig_paid,
                position.pending_rewards,
                earned
            );
        }
    }
    println!("---END---");
}
