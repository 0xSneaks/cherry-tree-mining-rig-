//! Shared test harness: a lamport-exact model of the three vaults driving the
//! real engine. Every test that claims "the vault balance dropped by X" is
//! asserting against actual modelled lamport movement, not against a counter.

#![allow(dead_code)]

use rigs_engine::*;

pub const SOL: u64 = LAMPORTS_PER_SOL;

/// Three vaults plus user wallets, wired to the engine exactly the way the
/// on-chain instruction handlers wire them.
pub struct World {
    pub globals: RewardGlobals,
    pub ledger: RewardLedger,
    pub miners: Vec<MinerPosition>,
    pub wallets: Vec<u64>,

    pub capital_vault: u64,
    pub reward_vault: u64,
    pub fee_vault: u64,

    pub rig_price: u64,
    /// Fresh-purchase fee.
    pub fee_bps: u16,
    /// Compound fee (architecture §7a item 2). Defaults to `fee_bps`.
    pub compound_fee_bps: u16,
    pub now: i64,

    /// Every lamport that ever reached the fee vault, by source.
    pub fees_from_purchases: u64,
    pub fees_from_compounds: u64,
    pub fees_from_early_claims: u64,

    /// Everything the funder has ever paid in. Used to prove no purchase
    /// lamport ever became reward backing.
    pub funded_in: u64,
}

impl World {
    pub fn new(miner_count: usize, rig_price: u64, fee_bps: u16) -> Self {
        World {
            globals: RewardGlobals {
                reward_last_update: 0,
                reward_period_finish: 0,
                ..Default::default()
            },
            ledger: RewardLedger::default(),
            miners: vec![MinerPosition::default(); miner_count],
            wallets: vec![1_000_000 * SOL; miner_count],
            capital_vault: 0,
            reward_vault: 0,
            fee_vault: 0,
            rig_price,
            fee_bps,
            compound_fee_bps: fee_bps,
            now: 0,
            fees_from_purchases: 0,
            fees_from_compounds: 0,
            fees_from_early_claims: 0,
            funded_in: 0,
        }
    }

    /// Wallets needed to hold `total` rigs under the 250-seed wallet cap.
    pub fn wallets_for(total: u64) -> usize {
        total.div_ceil(MAX_WALLET_RIGS) as usize
    }

    /// Plant `total` rigs across miners `from..`, at most `MAX_WALLET_RIGS`
    /// each (the wallet cap). Returns the first miner index left unused.
    pub fn plant_across(&mut self, from: usize, total: u64) -> usize {
        let (mut who, mut left) = (from, total);
        while left > 0 {
            let n = left.min(MAX_WALLET_RIGS);
            self.buy(who, n).unwrap();
            left -= n;
            who += 1;
        }
        who
    }

    pub fn warp(&mut self, secs: i64) {
        self.now += secs;
    }

    /// `fund_rewards`: lamports land in the reward vault first, then the
    /// schedule is written, then the fully-funded invariant is asserted.
    pub fn fund(&mut self, amount: u64, duration: u64) -> EngineResult<()> {
        self.reward_vault += amount;
        self.funded_in += amount;
        let r = self
            .globals
            .fund(&mut self.ledger, self.now, amount, duration);
        if r.is_err() {
            // Instruction aborts: the transfer is rolled back with it.
            self.reward_vault -= amount;
            self.funded_in -= amount;
        }
        r?;
        self.check_invariants();
        Ok(())
    }

    /// `buy_rigs`: user wallet -> capital vault + fee vault. The reward vault
    /// is not a participant.
    pub fn buy(&mut self, who: usize, count: u64) -> EngineResult<PurchaseQuote> {
        let reward_before = self.reward_vault;
        let quote = buy_rigs(
            &mut self.globals,
            &mut self.miners[who],
            self.now,
            count,
            self.rig_price,
            self.fee_bps,
        )?;
        self.wallets[who] -= quote.total_lamports;
        self.capital_vault += quote.capital_lamports;
        self.fee_vault += quote.fee_lamports;
        self.fees_from_purchases += quote.fee_lamports;
        assert_eq!(
            self.reward_vault, reward_before,
            "buy_rigs must not touch the reward vault"
        );
        self.check_invariants();
        Ok(quote)
    }

    /// `claim`: reward vault -> user wallet (`net`) and reward vault -> fee
    /// vault (the early-claim fee's 60% treasury share). The 40% grove share
    /// stays in the reward vault, re-credited to the other shares. Returns
    /// `net`.
    pub fn claim(&mut self, who: usize) -> EngineResult<u64> {
        Ok(self.claim_outcome(who)?.net)
    }

    pub fn claim_outcome(&mut self, who: usize) -> EngineResult<ClaimOutcome> {
        let (fee_before, cap_before) = (self.fee_vault, self.capital_vault);
        let outcome = claim(
            &mut self.globals,
            &mut self.miners[who],
            &mut self.ledger,
            self.now,
        )?;
        assert_eq!(outcome.gross, outcome.net + outcome.penalty);
        assert_eq!(
            outcome.penalty,
            outcome.penalty_treasury + outcome.penalty_grove
        );
        self.reward_vault = self
            .reward_vault
            .checked_sub(outcome.net + outcome.penalty_treasury)
            .expect("reward vault underflow — fully-funded invariant broken");
        self.wallets[who] += outcome.net;
        self.fee_vault += outcome.penalty_treasury;
        self.fees_from_early_claims += outcome.penalty_treasury;
        assert_eq!(
            (self.fee_vault, self.capital_vault),
            (fee_before + outcome.penalty_treasury, cap_before),
            "only the early-claim fee's treasury share reaches FEE; never CAPITAL"
        );
        self.check_invariants();
        Ok(outcome)
    }

    /// `compound`: reward vault -> capital vault + fee vault.
    pub fn compound(&mut self, who: usize) -> EngineResult<CompoundOutcome> {
        let outcome = compound(
            &mut self.globals,
            &mut self.miners[who],
            &mut self.ledger,
            self.now,
            self.rig_price,
            self.compound_fee_bps,
        )?;
        self.reward_vault = self
            .reward_vault
            .checked_sub(outcome.total_lamports)
            .expect("reward vault underflow — fully-funded invariant broken");
        self.capital_vault += outcome.capital_lamports;
        self.fee_vault += outcome.fee_lamports;
        self.fees_from_compounds += outcome.fee_lamports;
        self.check_invariants();
        Ok(outcome)
    }

    /// Live earned figure for a miner, without mutating state.
    pub fn earned(&self, who: usize) -> u64 {
        let mut g = self.globals;
        g.sync(self.now).unwrap();
        self.miners[who].earned(&g).unwrap()
    }

    pub fn total_earned(&self) -> u64 {
        (0..self.miners.len()).map(|i| self.earned(i)).sum()
    }

    /// Asserted after every single instruction, exactly as the program does.
    pub fn check_invariants(&self) {
        // Test 8 / §5: the reward vault always backs what it owes.
        assert_fully_funded(self.reward_vault, &self.ledger)
            .expect("reward vault no longer backs outstanding liabilities");

        // Test 13: reward backing came only from the funder.
        assert_eq!(
            self.reward_vault,
            self.funded_in
                - self.ledger.total_reward_claimed
                - self.ledger.total_reward_compounded
                - self.ledger.total_reward_to_fees,
            "reward vault balance must equal funded - claimed - compounded - early-claim treasury"
        );

        // Test 7: nothing distributed can exceed what was funded.
        assert!(
            self.total_earned()
                + self.ledger.total_reward_claimed
                + self.ledger.total_reward_compounded
                + self.ledger.total_reward_to_fees
                <= self.ledger.total_reward_funded,
            "earned + claimed + compounded + treasury share exceeded funded"
        );

        // §7a item 11: FEE holds exactly purchase + compound fees + early-claim
        // treasury shares — never CAPITAL, never reward liabilities.
        assert_eq!(
            self.fee_vault,
            self.fees_from_purchases + self.fees_from_compounds + self.fees_from_early_claims
        );
        assert_eq!(
            self.fees_from_early_claims,
            self.ledger.total_reward_to_fees
        );

        // Architecture §4 check 2 / test 10: no position ever holds more than
        // the permanent wallet cap, whatever sequence of actions got it here.
        for (i, m) in self.miners.iter().enumerate() {
            assert!(
                m.rigs <= MAX_WALLET_RIGS,
                "miner {i} holds {} rigs, above the {MAX_WALLET_RIGS} wallet cap",
                m.rigs
            );
        }
    }
}
