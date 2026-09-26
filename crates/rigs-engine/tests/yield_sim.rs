//! Genesis yield simulations.
//!
//! Every scenario drives the real engine through the lamport-exact `World`
//! harness, which asserts the fully-funded invariants after every single
//! action. On top of that each scenario asserts, at the end:
//!
//!   * rewards received + still owed ≤ everything ever funded
//!   * the reward vault never went below what it owed (checked per action)
//!   * the APR shown at each checkpoint equals `apr_bps` of on-chain state
//!
//! Print the tables with:
//!   cargo test -p rigs-engine --test yield_sim -- --nocapture --test-threads=1

mod common;

use common::{World, SOL};
use rigs_engine::*;

const PRICE: u64 = 10_000_000; // 0.01 SOL
const FEE: u16 = 300; // 3.00%
const DAY: i64 = 86_400;

fn apr(w: &World) -> u64 {
    apr_bps(&YieldInputs::from_globals(&w.globals, PRICE, FEE, w.now), 0).unwrap()
}

fn phase(w: &World) -> YieldPhase {
    current_phase(&YieldInputs::from_globals(&w.globals, PRICE, FEE, w.now)).unwrap()
}

fn pct(bps: u64) -> String {
    let whole = bps / 100;
    let s = whole.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("{out}.{:02}%", bps % 100)
}

fn sol(l: u128) -> String {
    format!("{}.{:04}", l / SOL as u128, (l % SOL as u128) / 100_000)
}

fn checkpoint(w: &World, label: &str) {
    let planted = w.globals.total_rigs as u128 * PRICE as u128 * (10_000 + FEE as u128) / 10_000;
    println!(
        "  {label:<34} rigs {:>9}  planted {:>12} SOL  APR {:>14}  {:?}",
        w.globals.total_rigs,
        sol(planted),
        pct(apr(w)),
        phase(w)
    );
}

/// Final solvency statement for a scenario.
fn settle_and_report(w: &mut World, name: &str) {
    let owed: u128 = (0..w.miners.len()).map(|i| w.earned(i) as u128).sum();
    let received = w.ledger.total_reward_claimed as u128 + w.ledger.total_reward_compounded as u128;
    let funded = w.ledger.total_reward_funded as u128;
    assert!(
        received + owed <= funded,
        "{name}: paid+owed exceeds funded"
    );
    assert!(
        w.reward_vault as u128 >= owed,
        "{name}: vault cannot cover what it owes"
    );
    println!(
        "  SOLVENCY funded {} | claimed {} | compounded {} | owed {} | vault {} | slack {} SOL — OK",
        sol(funded),
        sol(w.ledger.total_reward_claimed as u128),
        sol(w.ledger.total_reward_compounded as u128),
        sol(owed),
        sol(w.reward_vault as u128),
        sol(w.reward_vault as u128 - owed),
    );
}

#[test]
fn s01_single_planter() {
    println!("\n[s01] 1 planter · 100 SOL over 180 days");
    for rigs in [1u64, 100, 250] {
        let mut w = World::new(1, PRICE, FEE);
        w.fund(100 * SOL, 180 * DAY as u64).unwrap();
        w.buy(0, rigs).unwrap();
        checkpoint(&w, &format!("{rigs} rigs"));
        w.warp(180 * DAY);
        let got = w.claim(0).unwrap();
        // A sole planter receives the whole (floored) emission and no more.
        assert!(got <= 100 * SOL && 100 * SOL - got < 180 * DAY as u64 + rigs);
        settle_and_report(&mut w, "s01");
    }
}

#[test]
fn s02_planter_counts_10_100_1000() {
    for n in [10usize, 100, 1_000] {
        println!("\n[s02] {n} planters, mixed low/large deposits · 100 SOL over 180 days");
        let mut w = World::new(n, PRICE, FEE);
        w.fund(100 * SOL, 180 * DAY as u64).unwrap();
        for i in 0..n {
            // Low deposits (1 rig) with every 10th planter a whale at the
            // 250-seed wallet cap.
            let rigs = if i % 10 == 0 { 250 } else { 1 + (i as u64 % 7) };
            w.buy(i, rigs).unwrap();
            if i + 1 == 1 || i + 1 == n / 10 || i + 1 == n {
                checkpoint(&w, &format!("after planter {}", i + 1));
            }
            w.warp(60);
        }
        w.warp(180 * DAY);
        for i in 0..n {
            let _ = w.claim(i);
        }
        settle_and_report(&mut w, "s02");
    }
}

#[test]
fn s03_rapid_compounding() {
    println!("\n[s03] 10 planters compounding every 10 minutes for 7 days · 20 SOL over 7 days");
    let mut w = World::new(10, PRICE, FEE);
    w.fund(20 * SOL, 7 * DAY as u64).unwrap();
    for i in 0..10 {
        w.buy(i, 100).unwrap();
    }
    checkpoint(&w, "start");
    for step in 0..(7 * 24 * 6) {
        w.warp(600);
        for i in 0..10 {
            let _ = w.compound(i);
        }
        if step % (24 * 6) == 0 {
            checkpoint(&w, &format!("day {}", step / (24 * 6)));
        }
    }
    let emitted = w.globals.reward_rate as u128 * 7 * DAY as u128;
    let received = w.ledger.total_reward_compounded as u128
        + (0..10).map(|i| w.earned(i) as u128).sum::<u128>();
    assert!(
        received <= emitted,
        "compounding cannot out-earn the emission"
    );
    settle_and_report(&mut w, "s03");
}

#[test]
fn s04_maximum_claim_pressure() {
    println!("\n[s04] 100 planters claiming every hour for 30 days · 50 SOL over 30 days");
    let mut w = World::new(100, PRICE, FEE);
    w.fund(50 * SOL, 30 * DAY as u64).unwrap();
    for i in 0..100 {
        w.buy(i, 10 + i as u64).unwrap();
    }
    checkpoint(&w, "start");
    for _ in 0..(30 * 24) {
        w.warp(3_600);
        for i in 0..100 {
            let _ = w.claim(i);
        }
    }
    checkpoint(&w, "window over");
    settle_and_report(&mut w, "s04");
}

#[test]
fn s05_pool_exhaustion() {
    println!("\n[s05] exhaustion: 5 SOL over 10 days, run to day 40");
    let mut w = World::new(20, PRICE, FEE);
    w.fund(5 * SOL, 10 * DAY as u64).unwrap();
    for i in 0..10 {
        w.buy(i, 50).unwrap();
    }
    checkpoint(&w, "day 0");
    w.warp(10 * DAY);
    checkpoint(&w, "day 10 (window closes)");
    assert_eq!(apr(&w), 0);
    assert_eq!(phase(&w), YieldPhase::Dormant);
    // Late arrivals after exhaustion earn nothing.
    for i in 10..20 {
        w.buy(i, 50).unwrap();
    }
    w.warp(30 * DAY);
    checkpoint(&w, "day 40");
    for i in 10..20 {
        assert_eq!(w.earned(i), 0);
    }
    for i in 0..10 {
        w.claim(i).unwrap();
    }
    settle_and_report(&mut w, "s05");
    // Refunding revives the grove — the only way a phase moves back up.
    w.fund(5 * SOL, 10 * DAY as u64).unwrap();
    checkpoint(&w, "refunded 5 SOL / 10 days");
    assert!(apr(&w) > 0);
}

#[test]
fn s06_long_duration_ten_years() {
    println!("\n[s06] 10 years: yearly 20 SOL top-ups, 50 planters compounding monthly");
    let mut w = World::new(50, PRICE, FEE);
    w.fund(20 * SOL, 365 * DAY as u64).unwrap();
    for i in 0..50 {
        w.buy(i, 20).unwrap();
    }
    for year in 0..10 {
        checkpoint(&w, &format!("year {year}"));
        for _ in 0..12 {
            w.warp(30 * DAY);
            for i in 0..50 {
                let _ = w.compound(i);
            }
        }
        w.warp(5 * DAY);
        if year < 9 {
            w.fund(20 * SOL, 365 * DAY as u64).unwrap();
        }
    }
    checkpoint(&w, "year 10");
    settle_and_report(&mut w, "s06");
}

#[test]
fn s07_referral_heavy() {
    // Splitting across many wallets earns nothing extra: a sybil ring of 200
    // one-rig accounts earns exactly what one 200-rig account earns. Referral
    // weight is covered separately in `launch_referral.rs`
    // (`referral_heavy_population_stays_solvent`): it is bounded at 1.05× per
    // rig and never adds emission.
    println!("\n[s07] sybil/referral ring: 200 × 1 rig vs 1 × 200 rigs");
    let mut ring = World::new(201, PRICE, FEE);
    ring.fund(10 * SOL, 30 * DAY as u64).unwrap();
    for i in 0..200 {
        ring.buy(i, 1).unwrap();
    }
    ring.buy(200, 200).unwrap();
    ring.warp(30 * DAY);
    let ring_total: u64 = (0..200).map(|i| ring.earned(i)).sum();
    let single = ring.earned(200);
    println!(
        "  ring earned {} SOL, single account {} SOL",
        sol(ring_total as u128),
        sol(single as u128)
    );
    assert_eq!(ring_total, single);
    settle_and_report(&mut ring, "s07");
}

/// The target curve. A funder front-loads 100 SOL across four epochs by
/// calling the existing `fund_rewards` at each epoch boundary — no program
/// change, no multiplier past what is funded. Planters arrive throughout.
#[test]
fn s08_genesis_curve_front_loaded() {
    println!("\n[s08] genesis curve: 100 SOL front-loaded 40/30/20/10 over 7/23/60/90 days, 1,000 arrivals");
    let epochs: [(u64, i64); 4] = [(40, 7), (30, 23), (20, 60), (10, 90)];
    let mut w = World::new(1_000, PRICE, FEE);
    let mut next_planter = 0usize;
    for (e, &(amount, days)) in epochs.iter().enumerate() {
        w.fund(amount * SOL, days as u64 * DAY as u64).unwrap();
        checkpoint(&w, &format!("epoch {} opens", e + 1));
        // Arrivals: ~250 planters per epoch, 5 rigs each, spread evenly.
        let arrivals = 250usize;
        let gap = days * DAY / arrivals as i64;
        for _ in 0..arrivals {
            if next_planter < 1_000 {
                w.buy(next_planter, 5).unwrap();
                next_planter += 1;
            }
            w.warp(gap);
        }
        let used = gap * arrivals as i64;
        w.warp(days * DAY - used);
        checkpoint(&w, &format!("epoch {} closes", e + 1));
    }
    // First 10 planters vs last 10: earliest roots grew fastest.
    let early: u64 = (0..10).map(|i| w.earned(i)).sum();
    let late: u64 = (990..1_000).map(|i| w.earned(i)).sum();
    println!(
        "  first 10 planters earned {} SOL, last 10 earned {} SOL",
        sol(early as u128),
        sol(late as u128)
    );
    assert!(early > late * 10);
    settle_and_report(&mut w, "s08");
}

/// What the target phases cost. Pure arithmetic from `required_annual_emission`.
#[test]
fn s09_funding_required_for_target_phases() {
    println!(
        "\n[s09] reward emission required to hold each target APR (fee-inclusive planted capital)"
    );
    println!(
        "  {:<22} {:>10} {:>16} {:>16} {:>16}",
        "target", "planted", "per year", "per 90 days", "per 180 days"
    );
    for (label, bps) in [
        ("Genesis 3,000%", 300_000u64),
        ("Genesis floor 1,200%", 120_000),
        ("Early Grove floor 600%", 60_000),
        ("Growth floor 250%", 25_000),
        ("Mature floor 60%", 6_000),
    ] {
        for planted_sol in [10u64, 100, 1_000, 10_000] {
            let yearly = required_annual_emission(bps, planted_sol * SOL);
            println!(
                "  {label:<22} {:>6} SOL {:>12} SOL {:>12} SOL {:>12} SOL",
                planted_sol,
                sol(yearly),
                sol(yearly * 90 / 365),
                sol(yearly * 180 / 365),
            );
        }
    }
    // Sanity: holding 1,200% on 100 SOL for 180 days needs ~591.78 SOL.
    let need = required_annual_emission(120_000, 100 * SOL) * 180 / 365;
    assert_eq!(need / SOL as u128, 591);
}

/// Concrete planters for the report. 30 SOL over 30 days (1 SOL/day).
#[test]
fn s10_user_examples() {
    println!("\n[s10] user examples · 30 SOL over 30 days, 0.01 SOL rigs + 3% fee");
    let mut w = World::new(5, PRICE, FEE);
    w.fund(30 * SOL, 30 * DAY as u64).unwrap();
    let entry = |w: &World, n: u64| {
        pct(apr_bps(&YieldInputs::from_globals(&w.globals, PRICE, FEE, w.now), n).unwrap())
    };
    let report = |w: &World, who: usize, name: &str, spent: u64| {
        let pos = &w.miners[who];
        let got = w.earned(who) as u128
            + pos.lifetime_sol_claimed as u128
            + pos.lifetime_sol_compounded as u128;
        println!(
            "  {name:<40} paid {:>8} SOL  received {:>8} SOL  rigs now {:>6}  return {:>9}",
            sol(spent as u128),
            sol(got),
            pos.rigs,
            pct((got * 10_000 / spent as u128) as u64)
        );
    };

    println!(
        "  A plants 10 rigs at genesis — entry APR {}",
        entry(&w, 10)
    );
    let a = w.buy(0, 10).unwrap().total_lamports;
    println!(
        "  B plants 250 rigs (wallet cap) an hour later — entry APR {}",
        entry(&w, 250)
    );
    w.warp(3_600);
    let b = w.buy(1, 250).unwrap().total_lamports;
    checkpoint(&w, "after A+B");
    w.warp(5 * DAY);
    println!("  C plants 10 rigs on day 5 — entry APR {}", entry(&w, 10));
    let c = w.buy(2, 10).unwrap().total_lamports;
    println!(
        "  D plants 250 rigs (wallet cap) on day 5 — entry APR {}",
        entry(&w, 250)
    );
    let d = w.buy(3, 250).unwrap().total_lamports;
    checkpoint(&w, "after C+D");
    // B compounds daily; everyone else holds.
    for _ in 0..25 {
        w.warp(DAY);
        let _ = w.compound(1);
    }
    checkpoint(&w, "day 30 (window closes)");
    report(&w, 0, "A · 10 rigs, genesis, held", a);
    report(&w, 1, "B · 250 rigs, hour 1, compounded daily", b);
    report(&w, 2, "C · 10 rigs, day 5, held", c);
    report(&w, 3, "D · 250 rigs, day 5, held", d);
    settle_and_report(&mut w, "s10");
}
