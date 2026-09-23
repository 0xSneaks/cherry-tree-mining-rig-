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

/// Emits yield vectors consumed by `app/tests/yield.test.ts`.
///
/// The yield panel must show the rate the accumulator pays. The TypeScript
/// port is checked against these rows: APR, phase, decay threshold,
/// compounding projection and effective rate.
#[test]
fn emit_yield_vectors() {
    use rigs_engine::{
        apr_bps, current_phase, effective_rate_bps, next_threshold, project_compounding,
        MinerPosition, NextThreshold, RewardGlobals, YieldInputs,
    };

    println!("---APR---");
    let rates = [0u64, 1, 2, 11_574, 1_000_000, 1_000_000_000_000];
    let totals = [0u64, 1, 37, 1_000, 5_000, 503_600, 1_000_000_000_000];
    let prices = [1u64, 10_000_000, 3_333_333, 18_000_000_000_000_000_000];
    let fees = [0u16, 300, 1_000];
    for &rate in &rates {
        for &total in &totals {
            for &price in &prices {
                for &fee in &fees {
                    for &(now, finish) in &[(0i64, 86_400i64), (86_400, 86_400)] {
                        for &extra in &[0u64, 1, 1_000] {
                            let i = YieldInputs {
                                reward_rate: rate,
                                reward_period_finish: finish,
                                total_rigs: total,
                                rig_price_lamports: price,
                                purchase_fee_bps: fee,
                                now,
                            };
                            let apr = apr_bps(&i, extra).unwrap();
                            let phase = current_phase(&i).unwrap().code();
                            let (kind, value) = match next_threshold(&i).unwrap() {
                                NextThreshold::Rigs { at_total_rigs, .. } => {
                                    (0, at_total_rigs as i128)
                                }
                                NextThreshold::WindowEnds { at } => (1, at as i128),
                                NextThreshold::None => (2, 0),
                            };
                            println!(
                                "{rate},{finish},{total},{price},{fee},{now},{extra},{apr},{phase},{kind},{value}"
                            );
                        }
                    }
                }
            }
        }
    }

    println!("---PROJECTION---");
    let day = 86_400i64;
    for &(total, rate, finish, stored) in &[
        (1_000u64, 6_430u64, 180 * day, 0u128),
        (5_000, 57_870, 7 * day, 123_456_789),
        (37, 1_000_000_000, 30 * day, 0),
        (100_000, 11_574, 365 * day, 999_999_999_999),
    ] {
        for &(rigs, pending) in &[(1u64, 0u64), (100, 5_000_000), (1_000, 0)] {
            if rigs > total {
                continue;
            }
            for &now in &[0i64, 3 * day] {
                for &interval in &[0u64, 3_600, 86_400, 7 * 86_400] {
                    for &horizon in &[86_400u64, 30 * 86_400, 365 * 86_400] {
                        let g = RewardGlobals {
                            total_rigs: total,
                            reward_rate: rate,
                            reward_period_finish: finish,
                            reward_last_update: 0,
                            reward_per_rig_stored: stored,
                            undistributed_dust: 0,
                        };
                        let pos = MinerPosition {
                            rigs,
                            reward_per_rig_paid: stored,
                            pending_rewards: pending,
                            ..Default::default()
                        };
                        let Ok(p) =
                            project_compounding(&g, &pos, now, 10_000_000, 300, interval, horizon)
                        else {
                            continue;
                        };
                        println!(
                            "{total},{rate},{finish},{stored},{rigs},{pending},{now},{interval},{horizon},{},{},{},{},{},{}",
                            p.horizon_seconds,
                            p.start_rigs,
                            p.end_rigs,
                            p.reward_lamports,
                            p.cost_basis_lamports,
                            p.yield_bps
                        );
                    }
                }
            }
        }
    }

    println!("---EFFECTIVE---");
    for &(spent, claimed, compounded, claimable, age) in &[
        (0u64, 0u64, 0u64, 0u64, 10_000i64),
        (1_000_000_000, 0, 0, 0, 3_599),
        (1_000_000_000, 0, 0, 5, 3_600),
        (10_000_000_000, 1_000_000_000, 0, 0, 15_768_000),
        (10_000_000_000, 1_000_000_000, 2_000_000_000, 333, 86_400),
    ] {
        let pos = MinerPosition {
            lifetime_sol_spent: spent,
            lifetime_sol_claimed: claimed,
            lifetime_sol_compounded: compounded,
            ..Default::default()
        };
        let r = effective_rate_bps(&pos, claimable, 1_000, 1_000 + age)
            .map(|v| v as i128)
            .unwrap_or(-1);
        println!(
            "{spent},{claimed},{compounded},{claimable},1000,{},{r}",
            1_000 + age
        );
    }
    println!("---END---");
}
