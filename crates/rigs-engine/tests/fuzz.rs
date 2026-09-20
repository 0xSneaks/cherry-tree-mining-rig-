//! Property-based fuzz over randomly interleaved actions.
//!
//! Spec §12 test 7 asks for exactly this: `sum(earned) + total_reward_claimed
//! <= total_reward_funded` must hold across arbitrary interleavings. The
//! harness additionally asserts the fully-funded invariant (test 8) after
//! every single action, so any violation anywhere in the sequence fails here.

mod common;

use common::{World, SOL};
use proptest::prelude::*;

#[derive(Debug, Clone, Copy)]
enum Action {
    Buy { who: usize, count: u64 },
    Claim { who: usize },
    Compound { who: usize },
    Fund { amount: u64, duration: u64 },
    Warp { secs: i64 },
}

fn action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        4 => (0usize..3, 1u64..200).prop_map(|(who, count)| Action::Buy { who, count }),
        3 => (0usize..3).prop_map(|who| Action::Claim { who }),
        3 => (0usize..3).prop_map(|who| Action::Compound { who }),
        2 => (1u64..50 * SOL, 1u64..100_000).prop_map(|(amount, duration)| Action::Fund { amount, duration }),
        6 => (0i64..100_000).prop_map(|secs| Action::Warp { secs }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn funding_invariants_hold_under_arbitrary_interleavings(
        price in 1u64..(SOL / 10),
        fee_bps in 0u16..=1000,
        actions in prop::collection::vec(action_strategy(), 1..120),
    ) {
        let mut w = World::new(3, price, fee_bps);

        for a in actions {
            // Every one of these calls asserts both invariants internally via
            // World::check_invariants. Errors are legitimate outcomes
            // (paused-style rejections, nothing to claim, rate floors to
            // zero); silent accounting corruption is not.
            match a {
                Action::Buy { who, count } => { let _ = w.buy(who, count); }
                Action::Claim { who } => { let _ = w.claim(who); }
                Action::Compound { who } => { let _ = w.compound(who); }
                Action::Fund { amount, duration } => { let _ = w.fund(amount, duration); }
                Action::Warp { secs } => w.warp(secs),
            }
        }

        // Settle everything and confirm the books close: once the schedule has
        // run out and everyone has claimed, nothing is owed.
        w.warp(1_000_000);
        for i in 0..3 {
            let _ = w.claim(i);
        }
        prop_assert_eq!(w.total_earned(), 0);
        prop_assert!(w.reward_vault >= w.globals.undistributed_dust);
    }
}
