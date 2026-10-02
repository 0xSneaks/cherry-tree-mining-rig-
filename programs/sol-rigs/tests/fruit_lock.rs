//! Lock Season — on-chain (architecture §8; Pass #7 P7-B, D-B1).
//!
//! Registration, cancellation and the allocation tally through the real
//! program in `solana-program-test`:
//!
//! * `assigned_total` is the exact sum of the per-registration awards the
//!   program computed; no instruction takes it as input (V-1 / V-2);
//! * registration and cancellation only on `[FRUIT_T0, t_s)`; cancellation
//!   decrements the active count and requested total exactly once; a
//!   cancelled wallet cannot register again;
//! * every rejected instruction leaves the pool, Season, registrations,
//!   ledger and vault byte-for-byte unchanged;
//! * the tally never moves `U`, `Z` or a lamport (P7-L2);
//! * kind and lifecycle come from PDA-bound program state only.

mod common;

use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use common::*;
use rigs_engine::{lock_allocation, lock_award, LOCK_KMAX_LAMPORTS};
use sol_rigs::constants::*;
use sol_rigs::error::RigsError;
use sol_rigs::fruit_state::{Ledger, LockRegistration, LockSeason, Pool};
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transport::TransportError,
};

const LOCK: u8 = 2;
const DAY: i64 = 86_400;
const WINDOW: i64 = 3_600; // fixture only — production window is unset
const B: u64 = 428_082_192;

fn fruit_config_pda() -> Pubkey {
    Pubkey::find_program_address(&[FRUIT_CONFIG_SEED], &sol_rigs::ID).0
}
fn ledger_pda(kind: u8) -> Pubkey {
    Pubkey::find_program_address(&[FRUIT_LEDGER_SEED, &[kind]], &sol_rigs::ID).0
}
fn fvault_pda(kind: u8) -> Pubkey {
    Pubkey::find_program_address(&[FRUIT_VAULT_SEED, &[kind]], &sol_rigs::ID).0
}
fn pool_pda(kind: u8, id: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[FRUIT_POOL_SEED, &[kind], &id.to_le_bytes()],
        &sol_rigs::ID,
    )
    .0
}
fn season_pda(pool: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[LOCK_SEASON_SEED, pool.as_ref()], &sol_rigs::ID).0
}
fn reg_pda(pool: &Pubkey, wallet: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[LOCK_REG_SEED, pool.as_ref(), wallet.as_ref()],
        &sol_rigs::ID,
    )
    .0
}
fn sys() -> Pubkey {
    solana_sdk::system_program::ID
}
fn funded() -> Account {
    Account {
        lamports: 1_000 * SOL,
        ..Account::default()
    }
}

fn ix(accounts: Vec<AccountMeta>, data: Vec<u8>) -> Instruction {
    Instruction {
        program_id: sol_rigs::ID,
        accounts,
        data,
    }
}

struct Fx {
    env: Env,
    /// The Season's lock pool id: its FRUIT_T0 (B-1).
    id: u64,
    poster: Keypair,
    vetoers: Vec<Keypair>,
    wallets: Vec<Keypair>,
    fruit_t0: i64,
    t_s: i64,
}

impl Fx {
    /// Activated base program (T0 set), Fruit config (poster, 3 veto
    /// signers, threshold 2, fixture window), all ledgers, lock ledger funded
    /// with `2 B`. No lock pool yet.
    async fn new() -> Fx {
        Fx::setup(true).await
    }

    async fn setup(activate: bool) -> Fx {
        let poster = Keypair::new();
        let vetoers: Vec<Keypair> = (0..3).map(|_| Keypair::new()).collect();
        let wallets: Vec<Keypair> = (0..6).map(|_| Keypair::new()).collect();
        let keys: Vec<Pubkey> = std::iter::once(poster.pubkey())
            .chain(vetoers.iter().map(|k| k.pubkey()))
            .chain(wallets.iter().map(|k| k.pubkey()))
            .collect();
        let mut env = Env::with(|pt| {
            for k in keys {
                pt.add_account(k, funded());
            }
        })
        .await;
        env.initialize().await.unwrap();
        env.fund_rewards(10 * SOL, 30 * DAY as u64).await.unwrap();
        if activate {
            let f = env.founder.insecure_clone();
            env.buy(&f, 1).await.unwrap(); // activation: sets T0
        }
        let t0 = env.config().await.launch_t0;
        assert_eq!(t0 > 0, activate);
        let fruit_t0 = t0 + DAY;
        let t_s = fruit_t0 + 6 * DAY;
        let mut fx = Fx {
            env,
            id: fruit_t0 as u64,
            poster,
            vetoers,
            wallets,
            fruit_t0,
            t_s,
        };
        let a = fx.admin();
        let veto: Vec<Pubkey> = fx.vetoers.iter().map(|k| k.pubkey()).collect();
        let init = ix(
            sol_rigs::accounts::InitFruitConfig {
                authority: a.pubkey(),
                config: config_pda(),
                fruit_config: fruit_config_pda(),
                system_program: sys(),
            }
            .to_account_metas(None),
            sol_rigs::instruction::InitFruitConfig {
                root_poster: fx.poster.pubkey(),
                veto_signers: veto,
                veto_threshold: 2,
                challenge_window_secs: WINDOW,
                reserve_recipient: Pubkey::new_unique(),
            }
            .data(),
        );
        fx.env.send(init, &[&a]).await.unwrap();
        for kind in [0u8, 1, LOCK] {
            let i = ix(
                sol_rigs::accounts::InitFruitLedger {
                    admin: a.pubkey(),
                    fruit_config: fruit_config_pda(),
                    ledger: ledger_pda(kind),
                    vault: fvault_pda(kind),
                    system_program: sys(),
                }
                .to_account_metas(None),
                sol_rigs::instruction::InitFruitLedger { kind }.data(),
            );
            fx.env.send(i, &[&a]).await.unwrap();
        }
        let funder = fx.env.funder.insecure_clone();
        let fund = ix(
            sol_rigs::accounts::FundFruit {
                funder: funder.pubkey(),
                fruit_config: fruit_config_pda(),
                ledger: ledger_pda(LOCK),
                vault: fvault_pda(LOCK),
                system_program: sys(),
            }
            .to_account_metas(None),
            sol_rigs::instruction::FundFruit {
                kind: LOCK,
                amount: 2 * B,
            }
            .data(),
        );
        fx.env.send(fund, &[&funder]).await.unwrap();
        fx
    }

    fn admin(&self) -> Keypair {
        self.env.authority.insecure_clone()
    }
    fn w(&self, i: usize) -> Keypair {
        self.wallets[i].insecure_clone()
    }

    // ---------------------------------------------------------- reads

    async fn season(&mut self, id: u64) -> LockSeason {
        self.env
            .anchor_account(season_pda(&pool_pda(LOCK, id)))
            .await
    }
    async fn pool(&mut self, id: u64) -> Pool {
        self.env.anchor_account(pool_pda(LOCK, id)).await
    }
    async fn reg(&mut self, id: u64, i: usize) -> LockRegistration {
        let k = reg_pda(&pool_pda(LOCK, id), &self.wallets[i].pubkey());
        self.env.anchor_account(k).await
    }
    async fn ledger(&mut self) -> Ledger {
        self.env.anchor_account(ledger_pda(LOCK)).await
    }

    /// Raw bytes of every account a lock instruction can touch, plus the
    /// lock vault's lamports.
    async fn snapshot(&mut self, id: u64) -> Vec<Option<(u64, Vec<u8>)>> {
        let pool = pool_pda(LOCK, id);
        let mut keys = vec![pool, season_pda(&pool), ledger_pda(LOCK), fvault_pda(LOCK)];
        keys.extend(self.wallets.iter().map(|w| reg_pda(&pool, &w.pubkey())));
        let mut out = Vec::new();
        for k in keys {
            out.push(self.env.account_raw(k).await.map(|a| (a.lamports, a.data)));
        }
        out
    }

    /// `r` must fail with `e` and leave every lock account unchanged.
    async fn refused(&mut self, id: u64, e: RigsError, ixs: Instruction, signers: &[&Keypair]) {
        let before = self.snapshot(id).await;
        let err = self.env.send(ixs, signers).await.unwrap_err();
        assert_rigs_error(err, e);
        assert_eq!(
            self.snapshot(id).await,
            before,
            "rejected {e:?} mutated state"
        );
    }

    // ---------------------------------------------------------- instructions

    fn commit_ix(&self, id: u64, budget: u64) -> Instruction {
        let pool = pool_pda(LOCK, id);
        ix(
            sol_rigs::accounts::CommitLockPool {
                admin: self.env.authority.pubkey(),
                fruit_config: fruit_config_pda(),
                config: config_pda(),
                ledger: ledger_pda(LOCK),
                vault: fvault_pda(LOCK),
                pool,
                season: season_pda(&pool),
                system_program: sys(),
            }
            .to_account_metas(None),
            sol_rigs::instruction::CommitLockPool {
                pool_id: id,
                budget,
                reference_hash: [7; 32],
            }
            .data(),
        )
    }
    async fn commit(&mut self, id: u64, budget: u64) -> Result<(), TransportError> {
        let a = self.admin();
        let i = self.commit_ix(id, budget);
        self.env.send(i, &[&a]).await
    }

    fn register_ix(&self, id: u64, wallet: &Pubkey, requested: u64) -> Instruction {
        let pool = pool_pda(LOCK, id);
        ix(
            sol_rigs::accounts::RegisterLock {
                wallet: *wallet,
                fruit_config: fruit_config_pda(),
                pool,
                season: season_pda(&pool),
                registration: reg_pda(&pool, wallet),
                system_program: sys(),
            }
            .to_account_metas(None),
            sol_rigs::instruction::RegisterLock { requested }.data(),
        )
    }
    async fn register(&mut self, id: u64, i: usize, requested: u64) -> Result<(), TransportError> {
        let w = self.w(i);
        let x = self.register_ix(id, &w.pubkey(), requested);
        self.env.send(x, &[&w]).await
    }

    fn cancel_ix(&self, id: u64, wallet: &Pubkey) -> Instruction {
        let pool = pool_pda(LOCK, id);
        ix(
            sol_rigs::accounts::CancelLock {
                wallet: *wallet,
                fruit_config: fruit_config_pda(),
                pool,
                season: season_pda(&pool),
                registration: reg_pda(&pool, wallet),
            }
            .to_account_metas(None),
            sol_rigs::instruction::CancelLock {}.data(),
        )
    }
    async fn cancel(&mut self, id: u64, i: usize) -> Result<(), TransportError> {
        let w = self.w(i);
        let x = self.cancel_ix(id, &w.pubkey());
        self.env.send(x, &[&w]).await
    }

    fn regs(&self, id: u64, idx: &[usize]) -> Vec<AccountMeta> {
        let pool = pool_pda(LOCK, id);
        idx.iter()
            .map(|&i| AccountMeta::new(reg_pda(&pool, &self.wallets[i].pubkey()), false))
            .collect()
    }

    fn weights_ix(
        &self,
        id: u64,
        poster: &Pubkey,
        idx: &[usize],
        weights: Vec<u64>,
    ) -> Instruction {
        let pool = pool_pda(LOCK, id);
        let mut metas = sol_rigs::accounts::PostLockWeights {
            poster: *poster,
            fruit_config: fruit_config_pda(),
            pool,
            season: season_pda(&pool),
        }
        .to_account_metas(None);
        metas.extend(self.regs(id, idx));
        ix(
            metas,
            sol_rigs::instruction::PostLockWeights { weights }.data(),
        )
    }
    async fn weigh(
        &mut self,
        id: u64,
        idx: &[usize],
        weights: Vec<u64>,
    ) -> Result<(), TransportError> {
        let p = self.poster.insecure_clone();
        let x = self.weights_ix(id, &p.pubkey(), idx, weights);
        self.env.send(x, &[&p]).await
    }

    fn step_metas(id: u64) -> Vec<AccountMeta> {
        let pool = pool_pda(LOCK, id);
        sol_rigs::accounts::LockTallyStep {
            fruit_config: fruit_config_pda(),
            pool,
            season: season_pda(&pool),
        }
        .to_account_metas(None)
    }
    fn seal_weights_ix(id: u64) -> Instruction {
        ix(
            Self::step_metas(id),
            sol_rigs::instruction::SealLockWeights {}.data(),
        )
    }
    fn award_ix(&self, id: u64, idx: &[usize]) -> Instruction {
        let mut metas = Self::step_metas(id);
        metas.extend(self.regs(id, idx));
        ix(
            metas,
            sol_rigs::instruction::AwardLockRegistrations {}.data(),
        )
    }
    fn seal_awards_ix(id: u64) -> Instruction {
        ix(
            Self::step_metas(id),
            sol_rigs::instruction::SealLockAwards {}.data(),
        )
    }
    fn finalize_ix(id: u64) -> Instruction {
        ix(
            Self::step_metas(id),
            sol_rigs::instruction::FinalizeLockAllocation {}.data(),
        )
    }
    fn veto_ix(id: u64, signer: &Pubkey) -> Instruction {
        let pool = pool_pda(LOCK, id);
        ix(
            sol_rigs::accounts::VetoLockAllocation {
                veto_signer: *signer,
                fruit_config: fruit_config_pda(),
                pool,
                season: season_pda(&pool),
            }
            .to_account_metas(None),
            sol_rigs::instruction::VetoLockAllocation {}.data(),
        )
    }

    /// Weigh `active` with `ells`, seal, award, seal awards (permissionless
    /// steps signed only by the fee payer).
    async fn tally(&mut self, id: u64, active: &[usize], ells: Vec<u64>) {
        if !active.is_empty() {
            self.weigh(id, active, ells).await.unwrap();
        }
        self.env.send(Self::seal_weights_ix(id), &[]).await.unwrap();
        if !active.is_empty() {
            let x = self.award_ix(id, active);
            self.env.send(x, &[]).await.unwrap();
        }
        self.env.send(Self::seal_awards_ix(id), &[]).await.unwrap();
    }
}

/// Engine-side expected `sum_i floor(B * 5 ell_i / H_lock)`.
fn expected_assigned(budget: u64, ells: &[u64]) -> (u64, u64, u64) {
    let sum: u64 = ells.iter().sum();
    let (h, gross) = lock_allocation(budget, sum).unwrap();
    let a = ells
        .iter()
        .filter(|&&e| e > 0)
        .map(|&e| lock_award(budget, e, h).unwrap())
        .sum();
    (h, gross, a)
}

// =================================================================== commit

#[tokio::test]
async fn commit_lock_pool_derives_the_season_and_fails_closed() {
    let mut fx = Fx::new().await;
    // commit_fruit_pool still refuses kind Lock.
    let a = fx.admin();
    let now = fx.env.clock().await.unix_timestamp;
    let generic = ix(
        sol_rigs::accounts::CommitFruitPool {
            admin: a.pubkey(),
            fruit_config: fruit_config_pda(),
            ledger: ledger_pda(LOCK),
            vault: fvault_pda(LOCK),
            pool: pool_pda(LOCK, 9),
            system_program: sys(),
        }
        .to_account_metas(None),
        sol_rigs::instruction::CommitFruitPool {
            kind: LOCK,
            pool_id: 9,
            budget: B,
            t_start: now + 100,
            t_end: now + 100 + 5 * DAY,
            reference_hash: [0; 32],
        }
        .data(),
    );
    let err = fx.env.send(generic, &[&a]).await.unwrap_err();
    assert_rigs_error(err, RigsError::LockPoolsNotImplemented);

    let l0 = fx.ledger().await;
    fx.commit(fx.id, B).await.unwrap();
    let (pool, season) = (fx.pool(fx.id).await, fx.season(fx.id).await);
    assert_eq!(pool.ledger_kind, LOCK);
    assert_eq!((pool.t_start, pool.t_end), (fx.t_s, fx.t_s + 5 * DAY));
    assert_eq!(pool.h_lock, 0);
    assert_eq!(season.pool, pool_pda(LOCK, fx.id));
    assert_eq!(
        (
            season.fruit_t0,
            season.budget,
            season.generation,
            season.status
        ),
        (fx.fruit_t0, B, 1, 0)
    );
    let l1 = fx.ledger().await;
    assert_eq!(
        (l1.committed, l1.reserve),
        (l0.committed + B, l0.reserve - B)
    );
    // B-1: one lock pool per Season. Any other id is refused; the Season's
    // id is already taken.
    let x = fx.commit_ix(fx.id + 1, B);
    fx.refused(fx.id + 1, RigsError::LockPoolIdMismatch, x, &[&a])
        .await;
    fx.env.warp(1).await;
    let x = fx.commit_ix(fx.id, B);
    let before = fx.snapshot(fx.id).await;
    assert!(fx.env.send(x, &[&a]).await.is_err());
    assert_eq!(fx.snapshot(fx.id).await, before);
    // Not at or after t_s.
    let mut fx = Fx::new().await;
    let a = fx.admin();
    fx.env.set_time(fx.t_s).await;
    let x = fx.commit_ix(fx.id, B);
    fx.refused(fx.id, RigsError::FruitPoolStarted, x, &[&a])
        .await;
}

#[tokio::test]
async fn commit_lock_pool_requires_launch_t0() {
    let mut fx = Fx::setup(false).await;
    let a = fx.admin();
    let x = fx.commit_ix(fx.id, B);
    fx.refused(fx.id, RigsError::LockSeasonUnscheduled, x, &[&a])
        .await;
    // Paused lock ledger also refuses (after activation).
    let mut fx = Fx::new().await;
    let a = fx.admin();
    let pause = ix(
        sol_rigs::accounts::FruitAdminOnly {
            admin: a.pubkey(),
            fruit_config: fruit_config_pda(),
        }
        .to_account_metas(None),
        sol_rigs::instruction::SetFruitPaused {
            kind: LOCK,
            paused: true,
        }
        .data(),
    );
    fx.env.send(pause, &[&a]).await.unwrap();
    let x = fx.commit_ix(fx.id, B);
    fx.refused(fx.id, RigsError::FruitPaused, x, &[&a]).await;
}

// =================================================================== D-B1

#[tokio::test]
async fn registration_and_cancellation_boundaries() {
    let mut fx = Fx::new().await;
    fx.commit(fx.id, B).await.unwrap();
    let t0 = fx.fruit_t0;
    // Registration at FRUIT_T0 - 1: closed.
    fx.env.set_time(t0 - 1).await;
    let w0 = fx.w(0);
    let x = fx.register_ix(fx.id, &w0.pubkey(), SOL);
    let before = fx.snapshot(fx.id).await;
    assert_rigs_error(
        fx.env.send(x, &[&w0]).await.unwrap_err(),
        RigsError::LockRegistrationClosed,
    );
    assert_eq!(fx.snapshot(fx.id).await, before);
    // At FRUIT_T0: register four wallets; bad requests refused.
    fx.env.set_time(t0).await;
    for (i, r) in [(0usize, SOL), (1, 2 * SOL), (2, 700_000_001), (3, 5)] {
        fx.register(fx.id, i, r).await.unwrap();
    }
    for bad in [0, LOCK_KMAX_LAMPORTS + 1] {
        let w4 = fx.w(4);
        let x = fx.register_ix(fx.id, &w4.pubkey(), bad);
        let before = fx.snapshot(fx.id).await;
        assert_rigs_error(
            fx.env.send(x, &[&w4]).await.unwrap_err(),
            RigsError::InvalidLockRequest,
        );
        assert_eq!(fx.snapshot(fx.id).await, before);
    }
    // Cancellation at FRUIT_T0 succeeds, exactly once.
    fx.cancel(fx.id, 0).await.unwrap();
    let s = fx.season(fx.id).await;
    assert_eq!(
        (
            s.total_reg_count,
            s.active_reg_count,
            s.active_requested_total
        ),
        (4, 3, 2 * SOL + 700_000_001 + 5)
    );
    let r0 = fx.reg(fx.id, 0).await;
    assert!(r0.cancelled && r0.cancelled_at == t0);
    fx.env.warp(1).await;
    let x = fx.cancel_ix(fx.id, &w0.pubkey());
    fx.refused(fx.id, RigsError::LockRegistrationCancelled, x, &[&w0])
        .await;
    // Registration -> cancellation -> re-registration: the PDA is taken.
    let x = fx.register_ix(fx.id, &w0.pubkey(), SOL);
    let before = fx.snapshot(fx.id).await;
    assert!(fx.env.send(x, &[&w0]).await.is_err());
    assert_eq!(
        fx.snapshot(fx.id).await,
        before,
        "re-registration mutated state"
    );
    // Another wallet cannot cancel someone else's registration.
    let w1 = fx.w(1);
    let w5 = fx.w(5);
    let mut x = fx.cancel_ix(fx.id, &w5.pubkey());
    x.accounts[4] = AccountMeta::new(reg_pda(&pool_pda(LOCK, fx.id), &w1.pubkey()), false);
    let before = fx.snapshot(fx.id).await;
    assert!(fx.env.send(x, &[&w5]).await.is_err());
    assert_eq!(fx.snapshot(fx.id).await, before);
    // Cancellation at t_s - 1 succeeds.
    fx.env.set_time(fx.t_s - 1).await;
    fx.cancel(fx.id, 1).await.unwrap();
    // At t_s: cancellation and registration are closed; counters frozen.
    fx.env.set_time(fx.t_s).await;
    let w2 = fx.w(2);
    let x = fx.cancel_ix(fx.id, &w2.pubkey());
    fx.refused(fx.id, RigsError::LockRegistrationClosed, x, &[&w2])
        .await;
    let w4 = fx.w(4);
    let x = fx.register_ix(fx.id, &w4.pubkey(), SOL);
    let before = fx.snapshot(fx.id).await;
    assert_rigs_error(
        fx.env.send(x, &[&w4]).await.unwrap_err(),
        RigsError::LockRegistrationClosed,
    );
    assert_eq!(fx.snapshot(fx.id).await, before);
    let s = fx.season(fx.id).await;
    assert_eq!(
        (
            s.total_reg_count,
            s.active_reg_count,
            s.active_requested_total
        ),
        (4, 2, 700_000_001 + 5)
    );
}

// =================================================================== tally

/// Mixed active / cancelled registrations: the tally seals on the active
/// count; cancelled registrations cannot be weighed or awarded; the Final
/// `assigned_total` is exactly the sum of the awards the program computed;
/// `U`, `Z` and the vault never move.
#[tokio::test]
async fn tally_is_exact_on_the_active_set_and_moves_no_lamports() {
    let mut fx = Fx::new().await;
    fx.commit(fx.id, B).await.unwrap();
    fx.env.set_time(fx.fruit_t0).await;
    let reqs = [
        LOCK_KMAX_LAMPORTS,
        1_111_111_111,
        777_777_777,
        SOL,
        123_456_789,
    ];
    for (i, r) in reqs.iter().enumerate() {
        fx.register(fx.id, i, *r).await.unwrap();
    }
    fx.cancel(fx.id, 1).await.unwrap();
    let ledger0 = fx.ledger().await;
    let vault0 = fx.env.lamports(fvault_pda(LOCK)).await;

    // Not before t_s.
    fx.env.set_time(fx.t_s - 1).await;
    let p = fx.poster.insecure_clone();
    let x = fx.weights_ix(fx.id, &p.pubkey(), &[0], vec![SOL]);
    fx.refused(fx.id, RigsError::LockSeasonNotStarted, x, &[&p])
        .await;
    fx.env.set_time(fx.t_s).await;
    // Cancelled registration cannot be weighed.
    let x = fx.weights_ix(fx.id, &p.pubkey(), &[0, 1], vec![SOL, SOL]);
    fx.refused(fx.id, RigsError::LockRegistrationCancelled, x, &[&p])
        .await;
    // Weight above request, batch mismatch, duplicate, outsider poster.
    let x = fx.weights_ix(fx.id, &p.pubkey(), &[3], vec![SOL + 1]);
    fx.refused(fx.id, RigsError::LockWeightExceedsRequest, x, &[&p])
        .await;
    let x = fx.weights_ix(fx.id, &p.pubkey(), &[0, 2], vec![SOL]);
    fx.refused(fx.id, RigsError::LockBatchMismatch, x, &[&p])
        .await;
    let x = fx.weights_ix(fx.id, &p.pubkey(), &[0, 0], vec![SOL, SOL]);
    fx.refused(fx.id, RigsError::DuplicateLockRegistration, x, &[&p])
        .await;
    let out = fx.w(5);
    let x = fx.weights_ix(fx.id, &out.pubkey(), &[0], vec![SOL]);
    fx.refused(fx.id, RigsError::UnauthorizedRootPoster, x, &[&out])
        .await;
    // Weigh 3 of the 4 active: sealing refused.
    let ells = [LOCK_KMAX_LAMPORTS, 0, 700_000_000, 999_999_999, 123_456_789];
    fx.weigh(fx.id, &[0, 2, 3], vec![ells[0], ells[2], ells[3]])
        .await
        .unwrap();
    fx.refused(
        1,
        RigsError::LockTallyIncomplete,
        Fx::seal_weights_ix(fx.id),
        &[],
    )
    .await;
    // Re-weighing in the same generation is refused.
    let x = fx.weights_ix(fx.id, &p.pubkey(), &[0], vec![1]);
    fx.refused(fx.id, RigsError::LockAlreadyWeighed, x, &[&p])
        .await;
    // Award before sealing: wrong phase.
    let x = fx.award_ix(fx.id, &[0]);
    fx.refused(fx.id, RigsError::LockAllocationPhase, x, &[])
        .await;
    fx.weigh(fx.id, &[4], vec![ells[4]]).await.unwrap();
    fx.env.send(Fx::seal_weights_ix(fx.id), &[]).await.unwrap();
    // Cancelled registration cannot be awarded; nor twice.
    let x = fx.award_ix(fx.id, &[1]);
    fx.refused(fx.id, RigsError::LockRegistrationCancelled, x, &[])
        .await;
    let x = fx.award_ix(fx.id, &[0, 2]);
    fx.env.send(x, &[]).await.unwrap();
    let x = fx.award_ix(fx.id, &[2]);
    fx.refused(fx.id, RigsError::LockAlreadyAwarded, x, &[])
        .await;
    fx.refused(
        1,
        RigsError::LockTallyIncomplete,
        Fx::seal_awards_ix(fx.id),
        &[],
    )
    .await;
    let x = fx.award_ix(fx.id, &[3, 4]);
    fx.env.send(x, &[]).await.unwrap();
    fx.env.send(Fx::seal_awards_ix(fx.id), &[]).await.unwrap();
    fx.refused(
        fx.id,
        RigsError::ChallengeWindowOpen,
        Fx::finalize_ix(fx.id),
        &[],
    )
    .await;
    fx.env.warp(WINDOW).await;
    fx.env.send(Fx::finalize_ix(fx.id), &[]).await.unwrap();

    let active_ells = [ells[0], ells[2], ells[3], ells[4]];
    let (h, gross, assigned) = expected_assigned(B, &active_ells);
    let s = fx.season(fx.id).await;
    assert_eq!(s.status, 4, "Final");
    assert_eq!(
        (s.h_lock, s.gross_allocatable, s.assigned_total),
        (h, gross, assigned)
    );
    let mut held = 0;
    for i in 0..5 {
        let r = fx.reg(fx.id, i).await;
        if i == 1 {
            assert!(r.cancelled);
            assert_eq!(r.award_gen, 0);
        } else {
            assert_eq!((r.weight_gen, r.award_gen), (1, 1));
            held += r.award;
        }
    }
    assert_eq!(held, s.assigned_total, "assigned = sum of on-chain awards");
    assert!(
        s.gross_allocatable - s.assigned_total < 4,
        "dust < n lamports"
    );
    assert_eq!(fx.pool(fx.id).await.h_lock, h, "Pool.h_lock mirrors H_lock");
    // P7-L2: the tally moved nothing.
    assert_eq!(fx.ledger().await.committed, ledger0.committed);
    assert_eq!(fx.ledger().await.reserve, ledger0.reserve);
    assert_eq!(fx.env.lamports(fvault_pda(LOCK)).await, vault0);
    // Final is immutable.
    for x in [
        Fx::seal_weights_ix(fx.id),
        Fx::seal_awards_ix(fx.id),
        Fx::finalize_ix(fx.id),
    ] {
        fx.refused(fx.id, RigsError::LockAllocationPhase, x, &[])
            .await;
    }
}

/// All registrations cancelled: the documented zero-active path.
#[tokio::test]
async fn all_cancelled_follows_the_zero_active_path() {
    let mut fx = Fx::new().await;
    fx.commit(fx.id, B).await.unwrap();
    fx.env.set_time(fx.fruit_t0).await;
    fx.register(fx.id, 0, SOL).await.unwrap();
    fx.register(fx.id, 1, 2 * SOL).await.unwrap();
    fx.cancel(fx.id, 0).await.unwrap();
    fx.cancel(fx.id, 1).await.unwrap();
    let s = fx.season(fx.id).await;
    assert_eq!(
        (
            s.total_reg_count,
            s.active_reg_count,
            s.active_requested_total
        ),
        (2, 0, 0)
    );
    fx.env.set_time(fx.t_s - 1).await;
    fx.refused(
        1,
        RigsError::LockSeasonNotStarted,
        Fx::seal_weights_ix(fx.id),
        &[],
    )
    .await;
    fx.env.set_time(fx.t_s).await;
    fx.tally(fx.id, &[], vec![]).await;
    fx.env.warp(WINDOW).await;
    fx.env.send(Fx::finalize_ix(fx.id), &[]).await.unwrap();
    let s = fx.season(fx.id).await;
    assert_eq!(s.status, 4);
    assert_eq!(
        (s.sum_ell, s.gross_allocatable, s.assigned_total),
        (0, 0, 0)
    );
    assert_eq!(fx.pool(fx.id).await.h_lock, 62_500_000_000);
    assert_eq!(fx.ledger().await.committed, B, "all of B stays in U");
}

#[tokio::test]
async fn veto_retallies_the_frozen_set_with_distinct_signers() {
    let mut fx = Fx::new().await;
    fx.commit(fx.id, B).await.unwrap();
    fx.env.set_time(fx.fruit_t0).await;
    fx.register(fx.id, 0, SOL).await.unwrap();
    fx.register(fx.id, 1, 2 * SOL).await.unwrap();
    fx.env.set_time(fx.t_s).await;
    // Veto before Pending: wrong phase.
    let v0 = fx.vetoers[0].insecure_clone();
    fx.refused(
        1,
        RigsError::LockAllocationPhase,
        Fx::veto_ix(fx.id, &v0.pubkey()),
        &[&v0],
    )
    .await;
    fx.tally(fx.id, &[0, 1], vec![SOL, 2 * SOL]).await;
    let first = fx.season(fx.id).await.assigned_total;
    // The poster is not a veto signer.
    let p = fx.poster.insecure_clone();
    fx.refused(
        1,
        RigsError::NotVetoSigner,
        Fx::veto_ix(fx.id, &p.pubkey()),
        &[&p],
    )
    .await;
    fx.env
        .send(Fx::veto_ix(fx.id, &v0.pubkey()), &[&v0])
        .await
        .unwrap();
    fx.env.warp(1).await;
    fx.refused(
        1,
        RigsError::DuplicateVeto,
        Fx::veto_ix(fx.id, &v0.pubkey()),
        &[&v0],
    )
    .await;
    let v1 = fx.vetoers[1].insecure_clone();
    fx.env
        .send(Fx::veto_ix(fx.id, &v1.pubkey()), &[&v1])
        .await
        .unwrap();
    let s = fx.season(fx.id).await;
    assert_eq!((s.status, s.generation, s.veto_bitmap), (1, 2, 0));
    assert_eq!(
        (
            s.total_reg_count,
            s.active_reg_count,
            s.active_requested_total
        ),
        (2, 2, 3 * SOL)
    );
    assert_eq!((s.weighed_count, s.assigned_total), (0, 0));
    // Re-tally generation 2 over the same set, with new weights.
    fx.tally(fx.id, &[0, 1], vec![SOL / 2, 2 * SOL]).await;
    fx.env.warp(WINDOW).await;
    fx.env.send(Fx::finalize_ix(fx.id), &[]).await.unwrap();
    let s = fx.season(fx.id).await;
    let (_, _, a) = expected_assigned(B, &[SOL / 2, 2 * SOL]);
    assert_eq!((s.generation, s.assigned_total), (2, a));
    assert!(a < first);
    let v2 = fx.vetoers[2].insecure_clone();
    fx.refused(
        1,
        RigsError::LockAllocationPhase,
        Fx::veto_ix(fx.id, &v2.pubkey()),
        &[&v2],
    )
    .await;
}

// =================================================================== isolation

/// Kind and lifecycle come only from PDA-bound program state: a non-
/// registration account in a batch, a relabelled pool, a Season that is not
/// the pool's and a broken `h_lock` mirror are all refused; generic Fruit
/// paths refuse the lock pool.
#[tokio::test]
async fn relabeling_and_foreign_accounts_are_refused() {
    let mut fx = Fx::new().await;
    fx.commit(fx.id, B).await.unwrap();
    fx.env.set_time(fx.fruit_t0).await;
    fx.register(fx.id, 0, SOL).await.unwrap();
    fx.env.set_time(fx.t_s).await;
    let p = fx.poster.insecure_clone();
    let pool_key = pool_pda(LOCK, fx.id);
    // A program-owned non-registration account (the Season) in the batch.
    let mut x = fx.weights_ix(fx.id, &p.pubkey(), &[0], vec![SOL]);
    x.accounts[4] = AccountMeta::new(season_pda(&pool_key), false);
    let before = fx.snapshot(fx.id).await;
    assert!(fx.env.send(x, &[&p]).await.is_err());
    assert_eq!(fx.snapshot(fx.id).await, before);
    // A Season that is not this pool's (derived for another pool key).
    let mut x = fx.weights_ix(fx.id, &p.pubkey(), &[0], vec![SOL]);
    x.accounts[3] = AccountMeta::new(season_pda(&pool_pda(LOCK, fx.id + 1)), false);
    let before = fx.snapshot(fx.id).await;
    assert!(fx.env.send(x, &[&p]).await.is_err());
    assert_eq!(fx.snapshot(fx.id).await, before);
    // Relabel pool 1 as core in its account data: its PDA no longer matches.
    use anchor_lang::AccountSerialize;
    let key = pool_pda(LOCK, fx.id);
    let mut acc = fx.env.account_raw(key).await.unwrap();
    let original = acc.clone();
    let mut pool: Pool = Pool::try_deserialize(&mut acc.data.as_slice()).unwrap();
    pool.ledger_kind = 0;
    let mut data = Vec::new();
    pool.try_serialize(&mut data).unwrap();
    acc.data = data;
    fx.env.ctx.set_account(&key, &acc.into());
    let x = fx.weights_ix(fx.id, &p.pubkey(), &[0], vec![SOL]);
    let err = fx.env.send(x, &[&p]).await.unwrap_err();
    assert!(format!("{err:?}").contains("Custom(2006)"), "{err:?}");
    // (New slot: the retried instruction below must not hit the status cache.)
    fx.env.warp(1).await;
    // Broken h_lock mirror.
    let mut acc = original.clone();
    let mut pool: Pool = Pool::try_deserialize(&mut acc.data.as_slice()).unwrap();
    pool.h_lock = 1;
    let mut data = Vec::new();
    pool.try_serialize(&mut data).unwrap();
    acc.data = data;
    fx.env.ctx.set_account(&key, &acc.into());
    let x = fx.weights_ix(fx.id, &p.pubkey(), &[0], vec![SOL]);
    fx.refused(fx.id, RigsError::LockTallyInvalid, x, &[&p])
        .await;
    fx.env.ctx.set_account(&key, &original.into());
    fx.env.warp(1).await;
    // Generic root on the lock pool: refused by the engine (F3).
    let x = ix(
        sol_rigs::accounts::PostFruitRoot {
            poster: p.pubkey(),
            fruit_config: fruit_config_pda(),
            pool: key,
            root: Pubkey::find_program_address(
                &[FRUIT_ROOT_SEED, key.as_ref(), &1u64.to_le_bytes()],
                &sol_rigs::ID,
            )
            .0,
            system_program: sys(),
        }
        .to_account_metas(None),
        sol_rigs::instruction::PostFruitRoot {
            version: 1,
            merkle_root: [1; 32],
            checkpoint_time: fx.t_s,
            total: 0,
            complete: false,
            forfeited: 0,
        }
        .data(),
    );
    fx.refused(fx.id, RigsError::InvalidLedgerKind, x, &[&p])
        .await;
    // The untouched pool still tallies normally.
    fx.tally(fx.id, &[0], vec![SOL]).await;
}

// =================================================================== P7-D settlement

fn root_pda(pool: &Pubkey, version: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[FRUIT_ROOT_SEED, pool.as_ref(), &version.to_le_bytes()],
        &sol_rigs::ID,
    )
    .0
}
fn claim_pda(pool: &Pubkey, wallet: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[FRUIT_CLAIM_SEED, pool.as_ref(), wallet.as_ref()],
        &sol_rigs::ID,
    )
    .0
}

impl Fx {
    #[allow(clippy::too_many_arguments)]
    fn post_root_ix(
        &self,
        version: u64,
        root: [u8; 32],
        cp: i64,
        total: u64,
        complete: bool,
        forfeited: u64,
    ) -> Instruction {
        let pool = pool_pda(LOCK, self.id);
        ix(
            sol_rigs::accounts::PostLockRoot {
                poster: self.poster.pubkey(),
                fruit_config: fruit_config_pda(),
                pool,
                season: season_pda(&pool),
                root: root_pda(&pool, version),
                system_program: sys(),
            }
            .to_account_metas(None),
            sol_rigs::instruction::PostLockRoot {
                version,
                merkle_root: root,
                checkpoint_time: cp,
                total,
                complete,
                forfeited,
            }
            .data(),
        )
    }
    fn finalize_root_ix(&self, version: u64) -> Instruction {
        let pool = pool_pda(LOCK, self.id);
        ix(
            sol_rigs::accounts::FinalizeLockRoot {
                fruit_config: fruit_config_pda(),
                pool,
                season: season_pda(&pool),
                root: root_pda(&pool, version),
                ledger: ledger_pda(LOCK),
                vault: fvault_pda(LOCK),
            }
            .to_account_metas(None),
            sol_rigs::instruction::FinalizeLockRoot {}.data(),
        )
    }
    fn claim_ix(&self, wallet: &Pubkey, cumulative: u64, proof: Vec<[u8; 32]>) -> Instruction {
        let pool = pool_pda(LOCK, self.id);
        ix(
            sol_rigs::accounts::ClaimLock {
                wallet: *wallet,
                pool,
                season: season_pda(&pool),
                registration: reg_pda(&pool, wallet),
                ledger: ledger_pda(LOCK),
                vault: fvault_pda(LOCK),
                claim_record: claim_pda(&pool, wallet),
                system_program: sys(),
            }
            .to_account_metas(None),
            sol_rigs::instruction::ClaimLock {
                cumulative_amount: cumulative,
                proof,
            }
            .data(),
        )
    }
    fn close_ix(&self) -> Instruction {
        let pool = pool_pda(LOCK, self.id);
        ix(
            sol_rigs::accounts::CloseLockPool {
                fruit_config: fruit_config_pda(),
                pool,
                season: season_pda(&pool),
                ledger: ledger_pda(LOCK),
                vault: fvault_pda(LOCK),
                config: config_pda(),
                treasury: fee_recipient(),
            }
            .to_account_metas(None),
            sol_rigs::instruction::CloseLockPool {}.data(),
        )
    }
    fn terminal_ix(&self, version: u64, cp: i64, total: u64, signers: &[Pubkey]) -> Instruction {
        let pool = pool_pda(LOCK, self.id);
        let mut metas = sol_rigs::accounts::ResolveLockPoolTerminal {
            payer: self.env.ctx.payer.pubkey(),
            fruit_config: fruit_config_pda(),
            pool,
            season: season_pda(&pool),
            root: root_pda(&pool, version),
            ledger: ledger_pda(LOCK),
            vault: fvault_pda(LOCK),
            system_program: sys(),
        }
        .to_account_metas(None);
        metas.extend(signers.iter().map(|k| AccountMeta::new_readonly(*k, true)));
        ix(
            metas,
            sol_rigs::instruction::ResolveLockPoolTerminal {
                version,
                merkle_root: [3; 32],
                checkpoint_time: cp,
                total,
            }
            .data(),
        )
    }
    /// Spendable lock-vault lamports.
    async fn v(&mut self) -> u64 {
        let acc = self.env.account_raw(fvault_pda(LOCK)).await.unwrap();
        let rent = self.env.ctx.banks_client.get_rent().await.unwrap();
        acc.lamports - rent.minimum_balance(acc.data.len())
    }
    async fn solvent(&mut self) {
        let l = self.ledger().await;
        let v = self.v().await;
        assert!(
            v >= l.liabilities + l.committed + l.reserve,
            "V < L + U + Z"
        );
    }
}

struct Tree {
    root: [u8; 32],
    proofs: Vec<Vec<[u8; 32]>>,
}
fn tree(pool_id: u64, leaves: &[(Pubkey, u64)]) -> Tree {
    let hashed: Vec<[u8; 32]> = leaves
        .iter()
        .map(|(w, c)| sol_rigs::fruit_merkle::leaf(&sol_rigs::ID, LOCK, pool_id, w, *c))
        .collect();
    let (root, proofs) = sol_rigs::fruit_merkle::build(&hashed);
    Tree { root, proofs }
}

/// Tests 50 / 77 / 88 / 89 on-chain: Final allocation -> cliff root with a
/// voluntary shortfall -> claims bounded by each wallet's award -> close
/// with 80% of the forfeiture to the treasury and everything else in Z.
#[tokio::test]
async fn full_season_settles_claims_and_closes() {
    let mut fx = Fx::new().await;
    let id = fx.id;
    fx.commit(id, B).await.unwrap();
    fx.env.set_time(fx.fruit_t0).await;
    let reqs = [LOCK_KMAX_LAMPORTS, 1_111_111_111, 777_777_777, SOL];
    for (i, r) in reqs.iter().enumerate() {
        fx.register(id, i, *r).await.unwrap();
    }
    fx.cancel(id, 3).await.unwrap();
    fx.env.set_time(fx.t_s).await;
    let ells = [LOCK_KMAX_LAMPORTS, 1_000_000_007, 700_000_000];
    fx.tally(id, &[0, 1, 2], ells.to_vec()).await;
    fx.env.warp(WINDOW).await;
    fx.env.send(Fx::finalize_ix(id), &[]).await.unwrap();
    let season = fx.season(id).await;
    let mut awards = Vec::new();
    for i in 0..3 {
        awards.push(fx.reg(id, i).await.award);
    }
    assert_eq!(awards.iter().sum::<u64>(), season.assigned_total);
    fx.solvent().await;

    // Cliff: nothing before t_s + 5 days, whatever the checkpoint.
    let p = fx.poster.insecure_clone();
    let te = fx.t_s + 5 * DAY;
    fx.env.set_time(te - 1).await;
    let x = fx.post_root_ix(1, [1; 32], te - 1, 1, false, 0);
    fx.refused(id, RigsError::RootExceedsCapacity, x, &[&p])
        .await;

    // Maturity. Wallet 1 kept half its weight (voluntary shortfall).
    fx.env.set_time(te).await;
    let w: Vec<Pubkey> = fx.wallets.iter().map(|k| k.pubkey()).collect();
    let vest1 = rigs_engine::lock_vest_at_maturity(awards[1], ells[1] / 2, ells[1]).unwrap();
    let vested = [awards[0], vest1, awards[2]];
    let total: u64 = vested.iter().sum();
    let forfeited = awards[1] - vest1;
    // A malicious tree also lists an over-award leaf for wallet 0 and a leaf
    // for the cancelled wallet 3: the per-wallet bound refuses both.
    let leaves = [
        (w[0], vested[0]),
        (w[1], vested[1]),
        (w[2], vested[2]),
        (w[0], awards[0] + 1),
        (w[3], 1),
    ];
    let t = tree(id, &leaves);
    // forfeited above assigned - total: refused.
    let x = fx.post_root_ix(
        1,
        t.root,
        te,
        total,
        true,
        season.assigned_total - total + 1,
    );
    fx.refused(id, RigsError::InvalidForfeited, x, &[&p]).await;
    // Above the assigned capacity: refused (even though <= B).
    let x = fx.post_root_ix(1, t.root, te, season.assigned_total + 1, true, 0);
    fx.refused(id, RigsError::RootExceedsCapacity, x, &[&p])
        .await;
    let x = fx.post_root_ix(1, t.root, te, total, true, forfeited);
    fx.env.send(x, &[&p]).await.unwrap();
    fx.refused(
        id,
        RigsError::ChallengeWindowOpen,
        fx.finalize_root_ix(1),
        &[],
    )
    .await;
    fx.env.warp(WINDOW).await;
    fx.env.send(fx.finalize_root_ix(1), &[]).await.unwrap();
    let l = fx.ledger().await;
    assert_eq!(l.liabilities, total);
    fx.solvent().await;

    // The generic claim path refuses lock pools.
    let w0 = fx.w(0);
    let generic = ix(
        sol_rigs::accounts::ClaimFruit {
            wallet: w[0],
            pool: pool_pda(LOCK, id),
            ledger: ledger_pda(LOCK),
            vault: fvault_pda(LOCK),
            claim_record: claim_pda(&pool_pda(LOCK, id), &w[0]),
            system_program: sys(),
        }
        .to_account_metas(None),
        sol_rigs::instruction::ClaimFruit {
            cumulative_amount: vested[0],
            proof: t.proofs[0].clone(),
        }
        .data(),
    );
    fx.refused(id, RigsError::InvalidLedgerKind, generic, &[&w0])
        .await;
    // Over-award leaf and cancelled wallet: refused.
    let x = fx.claim_ix(&w[0], awards[0] + 1, t.proofs[3].clone());
    fx.refused(id, RigsError::LockClaimAboveAward, x, &[&w0])
        .await;
    let w3 = fx.w(3);
    let x = fx.claim_ix(&w[3], 1, t.proofs[4].clone());
    fx.refused(id, RigsError::LockRegistrationCancelled, x, &[&w3])
        .await;
    // Valid claims pay exactly the vested amounts, once.
    for i in 0..3 {
        let k = fx.w(i);
        let before = fx.env.lamports(w[i]).await;
        let x = fx.claim_ix(&w[i], vested[i], t.proofs[i].clone());
        fx.env.send(x, &[&k]).await.unwrap();
        let after = fx.env.lamports(w[i]).await;
        let rec: sol_rigs::fruit_state::ClaimRecord = fx
            .env
            .anchor_account(claim_pda(&pool_pda(LOCK, id), &w[i]))
            .await;
        assert_eq!(rec.claimed_cum, vested[i]);
        // Received the payment minus the claim record's rent (first claim).
        assert!(after + 10_000_000 > before + vested[i] && after < before + vested[i] + 1);
        fx.solvent().await;
    }
    assert_eq!(fx.ledger().await.liabilities, 0);

    // Close: B - total back to Z, 80% of forfeited to the treasury.
    let treasury0 = fx.env.lamports(fee_recipient()).await;
    let l0 = fx.ledger().await;
    fx.env.send(fx.close_ix(), &[]).await.unwrap();
    let l1 = fx.ledger().await;
    let release = forfeited * 8 / 10;
    assert_eq!(fx.env.lamports(fee_recipient()).await, treasury0 + release);
    assert_eq!(l0.committed, B - total, "U held the unestablished capacity");
    assert_eq!(l1.committed, 0);
    assert_eq!(l1.reserve, l0.reserve + (B - total) - release);
    assert_eq!(l1.liabilities, 0);
    assert_eq!(fx.pool(id).await.forfeit_treasury_released, release);
    let v = fx.v().await;
    assert_eq!(v, l1.liabilities + l1.committed + l1.reserve, "V == L+U+Z");
    // Exactly once.
    fx.env.warp(1).await;
    fx.refused(id, RigsError::FruitPoolClosed, fx.close_ix(), &[])
        .await;
}

#[tokio::test]
async fn lock_terminal_resolution_is_capped_and_needs_quorum() {
    let mut fx = Fx::new().await;
    let id = fx.id;
    fx.commit(id, B).await.unwrap();
    fx.env.set_time(fx.fruit_t0).await;
    fx.register(id, 0, LOCK_KMAX_LAMPORTS).await.unwrap();
    fx.env.set_time(fx.t_s).await;
    fx.tally(id, &[0], vec![LOCK_KMAX_LAMPORTS]).await;
    fx.env.warp(WINDOW).await;
    fx.env.send(Fx::finalize_ix(id), &[]).await.unwrap();
    let assigned = fx.season(id).await.assigned_total;
    let te = fx.t_s + 5 * DAY;
    fx.env.set_time(te).await;
    let v: Vec<Keypair> = fx.vetoers.iter().map(|k| k.insecure_clone()).collect();
    let keys = [v[0].pubkey(), v[1].pubkey()];
    // One signer: no quorum.
    let x = fx.terminal_ix(1, te, assigned, &keys[..1]);
    fx.refused(id, RigsError::TerminalQuorumNotMet, x, &[&v[0]])
        .await;
    // Above the assigned awards: refused.
    let x = fx.terminal_ix(1, te, assigned + 1, &keys);
    fx.refused(id, RigsError::RootExceedsCapacity, x, &[&v[0], &v[1]])
        .await;
    let x = fx.terminal_ix(1, te, assigned, &keys);
    fx.env.send(x, &[&v[0], &v[1]]).await.unwrap();
    assert_eq!(fx.ledger().await.liabilities, assigned);
    fx.env.send(fx.close_ix(), &[]).await.unwrap();
    fx.solvent().await;
}
