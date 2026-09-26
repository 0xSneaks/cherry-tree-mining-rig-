//! Fruit ledger — on-chain (architecture §10 / §10a).
//!
//! Frozen-architecture tests 11 (Fruit part), 55–67 and 73–76 against the
//! real program in `solana-program-test`. After every successful Fruit
//! transition [`Fx::check`] asserts, from real account data and real vault
//! lamports:
//!
//! * `V_k >= L_k + U_k + Z_k` per ledger and in aggregate;
//! * every tracked pool: `final_total <= E`, `paid_cum <= final_total`;
//! * every tracked claim record: `claimed_cum <= floor(E/5)`;
//! * `L` never decreases except by the payments observed;
//! * base CAPITAL / REWARD / FEE vault lamports unchanged.

mod common;

use anchor_lang::{AccountDeserialize, InstructionData, Space, ToAccountMetas};
use common::*;
use sol_rigs::constants::*;
use sol_rigs::error::RigsError;
use sol_rigs::fruit_merkle;
use sol_rigs::fruit_state::{ClaimRecord, FruitConfig, Ledger, Pool, Root};
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
    transport::TransportError,
};

const CORE: u8 = 0;
const LOYALTY: u8 = 1;
const LOCK: u8 = 2;
const WINDOW: i64 = 3_600; // fixture only — production window is unset

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

fn sys() -> Pubkey {
    solana_sdk::system_program::ID
}

fn funded_account() -> Account {
    Account {
        lamports: 1_000 * SOL,
        ..Account::default()
    }
}

/// A Merkle tree over `(wallet, cumulative)` leaves for one pool.
struct Tree {
    root: [u8; 32],
    proofs: Vec<Vec<[u8; 32]>>,
}

fn tree_for(program: &Pubkey, kind: u8, pool_id: u64, leaves: &[(Pubkey, u64)]) -> Tree {
    let hashed: Vec<[u8; 32]> = leaves
        .iter()
        .map(|(w, c)| fruit_merkle::leaf(program, kind, pool_id, w, *c))
        .collect();
    let (root, proofs) = fruit_merkle::build(&hashed);
    Tree { root, proofs }
}

fn tree(kind: u8, pool_id: u64, leaves: &[(Pubkey, u64)]) -> Tree {
    tree_for(&sol_rigs::ID, kind, pool_id, leaves)
}

struct Fx {
    env: Env,
    poster: Keypair,
    vetoers: Vec<Keypair>,
    outsider: Keypair,
    reserve_recipient: Pubkey,
    /// Pools and claim records whose invariants [`Fx::check`] asserts.
    pools: Vec<Pubkey>,
    claims: Vec<Pubkey>,
    base: [u64; 3],
    last_l: [u64; 3],
}

impl Fx {
    /// Base program initialized; Fruit config (poster, 3 veto signers,
    /// threshold 2, fixture window) and all three ledgers created.
    async fn new() -> Fx {
        let poster = Keypair::new();
        let vetoers: Vec<Keypair> = (0..3).map(|_| Keypair::new()).collect();
        let outsider = Keypair::new();
        let reserve_recipient = Pubkey::new_unique();
        let keys: Vec<Pubkey> = [&poster, &outsider]
            .iter()
            .map(|k| k.pubkey())
            .chain(vetoers.iter().map(|k| k.pubkey()))
            .chain([reserve_recipient])
            .collect();
        let mut env = Env::with(|pt| {
            for k in keys {
                pt.add_account(k, funded_account());
            }
        })
        .await;
        env.initialize().await.unwrap();
        let mut fx = Fx {
            env,
            poster,
            vetoers,
            outsider,
            reserve_recipient,
            pools: Vec::new(),
            claims: Vec::new(),
            base: [0; 3],
            last_l: [0; 3],
        };
        let set: Vec<Pubkey> = fx.vetoers.iter().map(|k| k.pubkey()).collect();
        fx.init_config(fx.poster.pubkey(), set, 2, WINDOW, fx.reserve_recipient)
            .await
            .unwrap();
        for kind in [CORE, LOYALTY, LOCK] {
            let ix = fx.init_ledger_ix(kind);
            let a = fx.admin();
            fx.env.send(ix, &[&a]).await.unwrap();
        }
        fx.base = fx.base_lamports().await;
        fx.check().await;
        fx
    }

    fn admin(&self) -> Keypair {
        self.env.authority.insecure_clone()
    }

    async fn now(&mut self) -> i64 {
        self.env.clock().await.unix_timestamp
    }

    async fn send_many(
        &mut self,
        ixs: &[Instruction],
        signers: &[&Keypair],
    ) -> Result<(), TransportError> {
        let payer = self.env.ctx.payer.insecure_clone();
        let mut all: Vec<&Keypair> = vec![&payer];
        all.extend_from_slice(signers);
        let bh = self
            .env
            .ctx
            .banks_client
            .get_latest_blockhash()
            .await
            .unwrap();
        let tx = Transaction::new_signed_with_payer(ixs, Some(&payer.pubkey()), &all, bh);
        self.env
            .ctx
            .banks_client
            .process_transaction(tx)
            .await
            .map_err(Into::into)
    }

    // ---------------------------------------------------------- reads

    async fn ledger(&mut self, kind: u8) -> Ledger {
        self.env.anchor_account(ledger_pda(kind)).await
    }
    async fn pool(&mut self, kind: u8, id: u64) -> Pool {
        self.env.anchor_account(pool_pda(kind, id)).await
    }
    async fn root(&mut self, kind: u8, id: u64, v: u64) -> Root {
        self.env
            .anchor_account(root_pda(&pool_pda(kind, id), v))
            .await
    }
    async fn claimed(&mut self, kind: u8, id: u64, w: &Pubkey) -> u64 {
        let r: ClaimRecord = self
            .env
            .anchor_account(claim_pda(&pool_pda(kind, id), w))
            .await;
        r.claimed_cum
    }
    async fn config(&mut self) -> FruitConfig {
        self.env.anchor_account(fruit_config_pda()).await
    }

    /// Spendable Fruit vault lamports: balance minus rent-exempt minimum.
    async fn v(&mut self, kind: u8) -> u64 {
        let acc = self.env.account_raw(fvault_pda(kind)).await.unwrap();
        let rent = self.env.ctx.banks_client.get_rent().await.unwrap();
        acc.lamports - rent.minimum_balance(acc.data.len())
    }

    async fn base_lamports(&mut self) -> [u64; 3] {
        [
            self.env.lamports(capital_vault_pda()).await,
            self.env.lamports(reward_vault_pda()).await,
            self.env.lamports(fee_vault_pda()).await,
        ]
    }

    /// Every §10a invariant, from on-chain data. `paid` is the lamports paid
    /// out by the transition just checked (the only allowed decrease of L).
    async fn check_paid(&mut self, paid: [u64; 3]) {
        let mut agg_v = 0u64;
        let mut agg_o = 0u64;
        for kind in [CORE, LOYALTY, LOCK] {
            let l = self.ledger(kind).await;
            let v = self.v(kind).await;
            let o = l.liabilities + l.committed + l.reserve;
            assert!(v >= o, "ledger {kind}: V {v} < L+U+Z {o}");
            assert!(
                l.liabilities + paid[kind as usize] >= self.last_l[kind as usize],
                "ledger {kind}: L decreased by more than the payment"
            );
            self.last_l[kind as usize] = l.liabilities;
            agg_v += v;
            agg_o += o;
        }
        assert!(agg_v >= agg_o, "aggregate V < L+U+Z");
        for p in self.pools.clone() {
            let pool: Pool = self.env.anchor_account(p).await;
            assert!(pool.final_total <= pool.budget, "sum allocations > E");
            assert!(pool.paid_cum <= pool.final_total, "paid_cum > final_total");
            assert_eq!(pool.wallet_cap, pool.budget / 5);
        }
        for c in self.claims.clone() {
            if self.env.account_raw(c).await.is_none() {
                continue;
            }
            let rec: ClaimRecord = self.env.anchor_account(c).await;
            let pool: Pool = self.env.anchor_account(rec.pool).await;
            assert!(
                rec.claimed_cum <= pool.wallet_cap,
                "wallet entitlement > E/5"
            );
        }
        assert_eq!(self.base_lamports().await, self.base, "a base vault moved");
    }

    async fn check(&mut self) {
        self.check_paid([0; 3]).await;
    }

    // ---------------------------------------------------------- instructions

    async fn init_config(
        &mut self,
        poster: Pubkey,
        veto: Vec<Pubkey>,
        threshold: u8,
        window: i64,
        recipient: Pubkey,
    ) -> Result<(), TransportError> {
        let a = self.admin();
        let ix = init_config_ix(&a.pubkey(), poster, veto, threshold, window, recipient);
        self.env.send(ix, &[&a]).await
    }

    fn init_ledger_ix(&self, kind: u8) -> Instruction {
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::InitFruitLedger {
                admin: self.env.authority.pubkey(),
                fruit_config: fruit_config_pda(),
                ledger: ledger_pda(kind),
                vault: fvault_pda(kind),
                system_program: sys(),
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::InitFruitLedger { kind }.data(),
        }
    }

    fn fund_ix(funder: &Pubkey, kind: u8, amount: u64) -> Instruction {
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::FundFruit {
                funder: *funder,
                fruit_config: fruit_config_pda(),
                ledger: ledger_pda(kind),
                vault: fvault_pda(kind),
                system_program: sys(),
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::FundFruit { kind, amount }.data(),
        }
    }

    async fn fund(&mut self, kind: u8, amount: u64) -> Result<(), TransportError> {
        let f = self.env.funder.insecure_clone();
        self.env
            .send(Self::fund_ix(&f.pubkey(), kind, amount), &[&f])
            .await?;
        self.check().await;
        Ok(())
    }

    fn commit_ix(admin: &Pubkey, kind: u8, id: u64, e: u64, t0: i64, t1: i64) -> Instruction {
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::CommitFruitPool {
                admin: *admin,
                fruit_config: fruit_config_pda(),
                ledger: ledger_pda(kind),
                vault: fvault_pda(kind),
                pool: pool_pda(kind, id),
                system_program: sys(),
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::CommitFruitPool {
                kind,
                pool_id: id,
                budget: e,
                t_start: t0,
                t_end: t1,
                reference_hash: [9; 32],
            }
            .data(),
        }
    }

    async fn commit(
        &mut self,
        kind: u8,
        id: u64,
        e: u64,
        t0: i64,
        t1: i64,
    ) -> Result<(), TransportError> {
        let a = self.admin();
        self.env
            .send(Self::commit_ix(&a.pubkey(), kind, id, e, t0, t1), &[&a])
            .await?;
        self.pools.push(pool_pda(kind, id));
        self.check().await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn post_ix(
        poster: &Pubkey,
        kind: u8,
        id: u64,
        v: u64,
        root: [u8; 32],
        cp: i64,
        total: u64,
        complete: bool,
    ) -> Instruction {
        let pool = pool_pda(kind, id);
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::PostFruitRoot {
                poster: *poster,
                fruit_config: fruit_config_pda(),
                pool,
                root: root_pda(&pool, v),
                system_program: sys(),
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::PostFruitRoot {
                version: v,
                merkle_root: root,
                checkpoint_time: cp,
                total,
                complete,
            }
            .data(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn post(
        &mut self,
        kind: u8,
        id: u64,
        v: u64,
        root: [u8; 32],
        cp: i64,
        total: u64,
        complete: bool,
    ) -> Result<(), TransportError> {
        let p = self.poster.insecure_clone();
        self.env
            .send(
                Self::post_ix(&p.pubkey(), kind, id, v, root, cp, total, complete),
                &[&p],
            )
            .await?;
        self.check().await;
        Ok(())
    }

    fn veto_ix(signer: &Pubkey, kind: u8, id: u64, v: u64) -> Instruction {
        let pool = pool_pda(kind, id);
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::VetoFruitRoot {
                veto_signer: *signer,
                fruit_config: fruit_config_pda(),
                pool,
                root: root_pda(&pool, v),
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::VetoFruitRoot {}.data(),
        }
    }

    async fn veto(
        &mut self,
        who: &Keypair,
        kind: u8,
        id: u64,
        v: u64,
    ) -> Result<(), TransportError> {
        self.env
            .send(Self::veto_ix(&who.pubkey(), kind, id, v), &[who])
            .await?;
        self.check().await;
        Ok(())
    }

    fn finalize_ix(kind: u8, id: u64, v: u64) -> Instruction {
        let pool = pool_pda(kind, id);
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::FinalizeFruitRoot {
                fruit_config: fruit_config_pda(),
                pool,
                root: root_pda(&pool, v),
                ledger: ledger_pda(kind),
                vault: fvault_pda(kind),
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::FinalizeFruitRoot {}.data(),
        }
    }

    async fn finalize(&mut self, kind: u8, id: u64, v: u64) -> Result<(), TransportError> {
        self.env.send(Self::finalize_ix(kind, id, v), &[]).await?;
        self.check().await;
        Ok(())
    }

    /// Post, wait out the window, finalize.
    #[allow(clippy::too_many_arguments)]
    async fn settle(
        &mut self,
        kind: u8,
        id: u64,
        v: u64,
        root: [u8; 32],
        cp: i64,
        total: u64,
        complete: bool,
    ) {
        self.post(kind, id, v, root, cp, total, complete)
            .await
            .unwrap();
        self.env.warp(WINDOW).await;
        self.finalize(kind, id, v).await.unwrap();
    }

    fn claim_ix_for(
        kind: u8,
        id: u64,
        wallet: &Pubkey,
        cumulative: u64,
        proof: Vec<[u8; 32]>,
    ) -> Instruction {
        let pool = pool_pda(kind, id);
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::ClaimFruit {
                wallet: *wallet,
                pool,
                ledger: ledger_pda(kind),
                vault: fvault_pda(kind),
                claim_record: claim_pda(&pool, wallet),
                system_program: sys(),
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::ClaimFruit {
                cumulative_amount: cumulative,
                proof,
            }
            .data(),
        }
    }

    /// Claim; returns the lamports the pool paid (from `paid_cum`).
    async fn claim(
        &mut self,
        who: &Keypair,
        kind: u8,
        id: u64,
        cumulative: u64,
        proof: Vec<[u8; 32]>,
    ) -> Result<u64, TransportError> {
        let before = self.pool(kind, id).await.paid_cum;
        let ix = Self::claim_ix_for(kind, id, &who.pubkey(), cumulative, proof);
        self.env.send(ix, &[who]).await?;
        let paid = self.pool(kind, id).await.paid_cum - before;
        self.claims
            .push(claim_pda(&pool_pda(kind, id), &who.pubkey()));
        let mut p = [0; 3];
        p[kind as usize] = paid;
        self.check_paid(p).await;
        Ok(paid)
    }

    fn close_ix(kind: u8, id: u64) -> Instruction {
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::CloseFruitPool {
                fruit_config: fruit_config_pda(),
                pool: pool_pda(kind, id),
                ledger: ledger_pda(kind),
                vault: fvault_pda(kind),
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::CloseFruitPool {}.data(),
        }
    }

    async fn close(&mut self, kind: u8, id: u64) -> Result<(), TransportError> {
        self.env.send(Self::close_ix(kind, id), &[]).await?;
        self.check().await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn terminal_ix(
        payer: &Pubkey,
        kind: u8,
        id: u64,
        v: u64,
        root: [u8; 32],
        cp: i64,
        total: u64,
        signers: &[Pubkey],
    ) -> Instruction {
        let pool = pool_pda(kind, id);
        let mut accounts = sol_rigs::accounts::ResolveFruitPoolTerminal {
            payer: *payer,
            fruit_config: fruit_config_pda(),
            pool,
            root: root_pda(&pool, v),
            ledger: ledger_pda(kind),
            vault: fvault_pda(kind),
            system_program: sys(),
        }
        .to_account_metas(None);
        accounts.extend(signers.iter().map(|k| AccountMeta::new_readonly(*k, true)));
        Instruction {
            program_id: sol_rigs::ID,
            accounts,
            data: sol_rigs::instruction::ResolveFruitPoolTerminal {
                version: v,
                merkle_root: root,
                checkpoint_time: cp,
                total,
            }
            .data(),
        }
    }

    fn withdraw_ix(admin: &Pubkey, kind: u8, amount: u64, dest: Pubkey) -> Instruction {
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::WithdrawFruitReserve {
                admin: *admin,
                fruit_config: fruit_config_pda(),
                ledger: ledger_pda(kind),
                vault: fvault_pda(kind),
                destination: dest,
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::WithdrawFruitReserve { kind, amount }.data(),
        }
    }

    async fn withdraw(&mut self, kind: u8, amount: u64) -> Result<(), TransportError> {
        let a = self.admin();
        let ix = Self::withdraw_ix(&a.pubkey(), kind, amount, self.reserve_recipient);
        self.env.send(ix, &[&a]).await?;
        self.check().await;
        Ok(())
    }

    fn pause_ix(admin: &Pubkey, kind: u8, paused: bool) -> Instruction {
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::FruitAdminOnly {
                admin: *admin,
                fruit_config: fruit_config_pda(),
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::SetFruitPaused { kind, paused }.data(),
        }
    }

    /// A pool on `kind` with budget `e`, running [now+100, now+100+dur).
    /// Returns `(t_start, t_end)`.
    async fn live_pool(&mut self, kind: u8, id: u64, e: u64, dur: i64) -> (i64, i64) {
        let t0 = self.now().await + 100;
        self.commit(kind, id, e, t0, t0 + dur).await.unwrap();
        (t0, t0 + dur)
    }
}

fn init_config_ix(
    authority: &Pubkey,
    poster: Pubkey,
    veto: Vec<Pubkey>,
    threshold: u8,
    window: i64,
    recipient: Pubkey,
) -> Instruction {
    Instruction {
        program_id: sol_rigs::ID,
        accounts: sol_rigs::accounts::InitFruitConfig {
            authority: *authority,
            config: config_pda(),
            fruit_config: fruit_config_pda(),
            system_program: sys(),
        }
        .to_account_metas(None),
        data: sol_rigs::instruction::InitFruitConfig {
            root_poster: poster,
            veto_signers: veto,
            veto_threshold: threshold,
            challenge_window_secs: window,
            reserve_recipient: recipient,
        }
        .data(),
    }
}

// =================================================================== setup

#[tokio::test]
async fn config_pins_frozen_constants_and_governance_inputs() {
    let mut fx = Fx::new().await;
    let c = fx.config().await;
    assert_eq!(c.state_version, FRUIT_STATE_VERSION);
    assert_eq!(c.admin, fx.env.authority.pubkey());
    assert_eq!(c.root_poster, fx.poster.pubkey());
    assert_eq!(c.veto_set().len(), 3);
    assert_eq!(c.veto_threshold, 2);
    assert_eq!(c.challenge_window_secs, WINDOW);
    assert_eq!(c.p_ref_lamports, 10_000_000);
    assert_eq!(c.m_seeds, 250);
    assert_eq!((c.qmax_num, c.qmax_den), (1, 5));
    assert_eq!(c.paused, [false; 3]);
    for kind in [CORE, LOYALTY, LOCK] {
        let l = fx.ledger(kind).await;
        assert_eq!((l.kind, l.vault), (kind, fvault_pda(kind)));
        assert_eq!(
            (l.liabilities, l.committed, l.reserve, l.paid_cum),
            (0, 0, 0, 0)
        );
        // Each Fruit vault is its own program-owned account, distinct from
        // every base vault.
        let v = fx.env.account_raw(fvault_pda(kind)).await.unwrap();
        assert_eq!(v.owner, sol_rigs::ID);
        assert!(
            ![capital_vault_pda(), reward_vault_pda(), fee_vault_pda()].contains(&fvault_pda(kind))
        );
    }
}

#[tokio::test]
async fn governance_inputs_are_validated_not_defaulted() {
    let poster = Pubkey::new_unique();
    let (a, b) = (Pubkey::new_unique(), Pubkey::new_unique());
    let rr = Pubkey::new_unique();
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    let auth = env.authority.insecure_clone();
    let me = auth.pubkey();
    let bad: Vec<(Pubkey, Vec<Pubkey>, u8, i64, Pubkey)> = vec![
        (poster, vec![a, poster], 1, WINDOW, rr), // poster in veto set
        (me, vec![a, b], 1, WINDOW, rr),          // poster is the admin
        (poster, vec![a, b], 0, WINDOW, rr),      // threshold 0
        (poster, vec![a, b], 3, WINDOW, rr),      // threshold > set
        (poster, vec![], 1, WINDOW, rr),          // empty set
        (poster, vec![a, a], 1, WINDOW, rr),      // duplicate signer
        (poster, vec![a, b], 1, 0, rr),           // no challenge window
        (poster, vec![a, b], 1, WINDOW, Pubkey::default()),
        (Pubkey::default(), vec![a, b], 1, WINDOW, rr),
        (poster, vec![Pubkey::new_unique(); 8], 1, WINDOW, rr),
    ];
    for (p, set, th, w, r) in bad {
        let err = env
            .send(init_config_ix(&me, p, set, th, w, r), &[&auth])
            .await
            .unwrap_err();
        assert_rigs_error(err, RigsError::InvalidFruitGovernance);
    }
    // Only the base authority can create the Fruit config.
    let erin = env.erin.insecure_clone();
    assert!(env
        .send(
            init_config_ix(&erin.pubkey(), poster, vec![a, b], 1, WINDOW, rr),
            &[&erin]
        )
        .await
        .is_err());
    env.send(
        init_config_ix(&me, poster, vec![a, b], 2, WINDOW, rr),
        &[&auth],
    )
    .await
    .unwrap();
    // One-shot (a different transaction, so not deduplicated).
    assert!(env
        .send(
            init_config_ix(&me, poster, vec![a, b], 1, WINDOW, rr),
            &[&auth]
        )
        .await
        .is_err());
}

// =================================================================== separation (test 11)

#[tokio::test]
async fn fruit_vaults_are_separate_from_base_vaults() {
    let mut fx = Fx::new().await;
    fx.env.fund_rewards(10 * SOL, 1_000).await.unwrap();
    fx.base = fx.base_lamports().await;
    let f = fx.env.funder.insecure_clone();

    // A base vault cannot pose as a Fruit vault (seeds and type differ)...
    for base in [capital_vault_pda(), reward_vault_pda(), fee_vault_pda()] {
        let mut ix = Fx::fund_ix(&f.pubkey(), CORE, SOL);
        ix.accounts[3].pubkey = base;
        assert!(fx.env.send(ix, &[&f]).await.is_err());
    }
    // ...nor one ledger's vault as another's.
    let mut ix = Fx::fund_ix(&f.pubkey(), CORE, SOL);
    ix.accounts[3].pubkey = fvault_pda(LOYALTY);
    assert!(fx.env.send(ix, &[&f]).await.is_err());

    // A Fruit vault cannot pose as the base reward vault.
    let mut ix = fx.env.fund_rewards_ix(&f.pubkey(), SOL, 1_000);
    ix.accounts[2].pubkey = fvault_pda(CORE);
    assert!(fx.env.send(ix, &[&f]).await.is_err());

    // Fruit funding lands only in its own vault; base reward backing is
    // unchanged, so base `assert_fully_funded` never counts Fruit SOL.
    let reward_funded = fx.env.config().await.total_reward_funded;
    fx.fund(CORE, 5 * SOL).await.unwrap();
    assert_eq!(fx.v(CORE).await, 5 * SOL);
    assert_eq!((fx.v(LOYALTY).await, fx.v(LOCK).await), (0, 0));
    assert_eq!(fx.env.config().await.total_reward_funded, reward_funded);

    // A full lifecycle, including a claim by a planter, never moves a base
    // vault or a seed count.
    let alice = fx.env.alice.insecure_clone();
    fx.env.buy(&alice, 10).await.unwrap();
    fx.base = fx.base_lamports().await;
    let rigs = fx.env.miner(&alice.pubkey()).await.rigs;
    let (_, t1) = fx.live_pool(CORE, 1, 5 * SOL, 1_000).await;
    fx.env.set_time(t1).await;
    let t = tree(CORE, 1, &[(alice.pubkey(), SOL)]);
    fx.settle(CORE, 1, 1, t.root, t1, SOL, true).await;
    assert_eq!(
        fx.claim(&alice, CORE, 1, SOL, t.proofs[0].clone())
            .await
            .unwrap(),
        SOL
    );
    fx.close(CORE, 1).await.unwrap();
    assert_eq!(fx.env.miner(&alice.pubkey()).await.rigs, rigs);
    assert_eq!(fx.base_lamports().await, fx.base);
}

// =================================================================== 55–58 roots

/// 55: root total above scheduled capacity rejected.
#[tokio::test]
async fn t55_root_above_scheduled_capacity_rejected() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 10 * SOL).await.unwrap();
    let (t0, t1) = fx.live_pool(CORE, 1, 10 * SOL, 1_000).await;
    fx.env.set_time(t0 + 500).await;
    let r = [1; 32];
    let err = fx
        .post(CORE, 1, 1, r, t0 + 500, 5 * SOL + 1, false)
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::RootExceedsCapacity);
    fx.post(CORE, 1, 1, r, t0 + 500, 5 * SOL, false)
        .await
        .unwrap();

    // 63 (on-chain part): at many checkpoints the exact scheduled capacity
    // is accepted and one lamport more is not.
    let mut v = 2;
    for cp in [t0 + 501, t0 + 777, t1 - 1, t1] {
        fx.env.warp(WINDOW).await;
        fx.finalize(CORE, 1, v - 1).await.unwrap();
        let target = fx.now().await.max(cp);
        fx.env.set_time(target).await;
        let cap = rigs_engine::scheduled_capacity(10 * SOL, t0, t1, cp).unwrap();
        let err = fx
            .post(CORE, 1, v, r, cp, cap + 1, false)
            .await
            .unwrap_err();
        assert_rigs_error(err, RigsError::RootExceedsCapacity);
        fx.post(CORE, 1, v, r, cp, cap, false).await.unwrap();
        v += 1;
    }
    assert_eq!(fx.root(CORE, 1, v - 1).await.total, 10 * SOL);
}

/// 56: future checkpoint, non-increasing version, second Pending root.
#[tokio::test]
async fn t56_future_checkpoint_bad_version_second_pending_rejected() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 10 * SOL).await.unwrap();
    let (t0, _) = fx.live_pool(CORE, 1, 10 * SOL, 1_000).await;
    fx.env.set_time(t0 + 500).await;
    let now = fx.now().await;
    let r = [1; 32];
    let err = fx.post(CORE, 1, 1, r, now + 1, 1, false).await.unwrap_err();
    assert_rigs_error(err, RigsError::FutureCheckpoint);
    let err = fx.post(CORE, 1, 0, r, now, 1, false).await.unwrap_err();
    assert_rigs_error(err, RigsError::StaleRootVersion);
    fx.post(CORE, 1, 5, r, now, 1, false).await.unwrap();
    let err = fx.post(CORE, 1, 6, r, now, 1, false).await.unwrap_err();
    assert_rigs_error(err, RigsError::RootAlreadyPending);
    assert_eq!(fx.config().await.pending_roots, 1);
    fx.env.warp(WINDOW).await;
    fx.finalize(CORE, 1, 5).await.unwrap();
    assert_eq!(fx.config().await.pending_roots, 0);
    let err = fx.post(CORE, 1, 4, r, now, 1, false).await.unwrap_err();
    assert_rigs_error(err, RigsError::StaleRootVersion);
    // Only the poster posts.
    let admin = fx.admin();
    let ix = Fx::post_ix(&admin.pubkey(), CORE, 1, 9, r, now, 1, false);
    assert_rigs_error(
        fx.env.send(ix, &[&admin]).await.unwrap_err(),
        RigsError::UnauthorizedRootPoster,
    );
}

/// 57: total below final_total rejected.
#[tokio::test]
async fn t57_total_below_final_total_rejected() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 10 * SOL).await.unwrap();
    let (t0, _) = fx.live_pool(CORE, 1, 10 * SOL, 1_000).await;
    fx.env.set_time(t0 + 500).await;
    fx.settle(CORE, 1, 1, [1; 32], t0 + 500, 4 * SOL, false)
        .await;
    let err = fx
        .post(CORE, 1, 2, [2; 32], t0 + 900, 4 * SOL - 1, false)
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::RootBelowFinalTotal);
    fx.post(CORE, 1, 2, [2; 32], t0 + 900, 4 * SOL, false)
        .await
        .unwrap();
}

/// 58: a leaf above the wallet cap is rejected even inside a Final root.
#[tokio::test]
async fn t58_leaf_above_wallet_cap_rejected() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 10 * SOL).await.unwrap();
    let (_, t1) = fx.live_pool(CORE, 1, 10 * SOL, 1_000).await;
    fx.env.set_time(t1).await;
    let bob = fx.env.bob.insecure_clone();
    let carol = fx.env.carol.insecure_clone();
    let t = tree(
        CORE,
        1,
        &[(bob.pubkey(), 2 * SOL + 1), (carol.pubkey(), 2 * SOL)],
    );
    fx.settle(CORE, 1, 1, t.root, t1, 10 * SOL, true).await;
    assert_eq!(fx.pool(CORE, 1).await.wallet_cap, 2 * SOL);
    let err = fx
        .claim(&bob, CORE, 1, 2 * SOL + 1, t.proofs[0].clone())
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::LeafAboveWalletCap);
    assert_eq!(
        fx.claim(&carol, CORE, 1, 2 * SOL, t.proofs[1].clone())
            .await
            .unwrap(),
        2 * SOL
    );
}

// =================================================================== 59 replay

/// 59: a leaf cannot be replayed across program, ledger, pool or wallet.
#[tokio::test]
async fn t59_cross_program_ledger_pool_wallet_replay_rejected() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 20 * SOL).await.unwrap();
    fx.fund(LOYALTY, 10 * SOL).await.unwrap();
    let t0 = fx.now().await + 100;
    fx.commit(CORE, 1, 10 * SOL, t0, t0 + 1_000).await.unwrap();
    fx.commit(CORE, 2, 10 * SOL, t0, t0 + 1_000).await.unwrap();
    fx.commit(LOYALTY, 1, 10 * SOL, t0, t0 + 1_000)
        .await
        .unwrap();
    fx.env.set_time(t0 + 1_000).await;
    let bob = fx.env.bob.insecure_clone();
    let erin = fx.env.erin.insecure_clone();
    let amt = SOL;

    // All three pools finalize the SAME Merkle root, built for (core, 1).
    let t = tree(CORE, 1, &[(bob.pubkey(), amt), (erin.pubkey(), 0)]);
    for (kind, id) in [(CORE, 1), (CORE, 2), (LOYALTY, 1)] {
        fx.settle(kind, id, 1, t.root, t0 + 1_000, amt, true).await;
    }
    // Another pool: rejected.
    let err = fx
        .claim(&bob, CORE, 2, amt, t.proofs[0].clone())
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::InvalidMerkleProof);
    // Another ledger, same pool id: rejected.
    let err = fx
        .claim(&bob, LOYALTY, 1, amt, t.proofs[0].clone())
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::InvalidMerkleProof);
    // Another wallet presenting bob's leaf and proof: rejected.
    let err = fx
        .claim(&erin, CORE, 1, amt, t.proofs[0].clone())
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::InvalidMerkleProof);
    // A different amount under bob's proof: rejected.
    let err = fx
        .claim(&bob, CORE, 1, amt - 1, t.proofs[0].clone())
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::InvalidMerkleProof);
    // The genuine claim succeeds exactly once.
    assert_eq!(
        fx.claim(&bob, CORE, 1, amt, t.proofs[0].clone())
            .await
            .unwrap(),
        amt
    );

    // Another program: a root whose leaves are bound to a different program
    // id pays nothing here.
    fx.fund(CORE, 5 * SOL).await.unwrap();
    let t3 = fx.now().await + 100;
    fx.commit(CORE, 3, 5 * SOL, t3, t3 + 10).await.unwrap();
    fx.env.set_time(t3 + 10).await;
    let foreign = tree_for(
        &Pubkey::new_unique(),
        CORE,
        3,
        &[(bob.pubkey(), amt), (erin.pubkey(), 0)],
    );
    fx.settle(CORE, 3, 1, foreign.root, t3 + 10, amt, true)
        .await;
    let err = fx
        .claim(&bob, CORE, 3, amt, foreign.proofs[0].clone())
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::InvalidMerkleProof);

    // An interior node cannot be passed off as a leaf: the leaf and node
    // hashes are domain-separated.
    let node = fruit_merkle::node(&[1; 32], &[2; 32]);
    assert_ne!(
        node,
        fruit_merkle::leaf(&sol_rigs::ID, 0, 0, &Pubkey::new_from_array([1; 32]), 0)
    );
}

// =================================================================== 60 challenge

/// 60: Pending and Vetoed roots cannot be claimed; a vetoed root never
/// finalizes; the poster cannot veto; one signer below threshold cannot veto.
#[tokio::test]
async fn t60_pending_vetoed_roots_and_poster_veto() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 10 * SOL).await.unwrap();
    let (_, t1) = fx.live_pool(CORE, 1, 10 * SOL, 1_000).await;
    fx.env.set_time(t1).await;
    let dave = fx.env.dave.insecure_clone();
    let t = tree(
        CORE,
        1,
        &[(dave.pubkey(), SOL), (Pubkey::new_unique(), SOL)],
    );
    fx.post(CORE, 1, 1, t.root, t1, 2 * SOL, true)
        .await
        .unwrap();

    // Pending: not claimable, not finalizable before the window.
    let err = fx
        .claim(&dave, CORE, 1, SOL, t.proofs[0].clone())
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::NoFinalRoot);
    let err = fx.finalize(CORE, 1, 1).await.unwrap_err();
    assert_rigs_error(err, RigsError::ChallengeWindowOpen);

    // The poster cannot veto, alone or at all; nor can an outsider.
    let poster = fx.poster.insecure_clone();
    assert_rigs_error(
        fx.veto(&poster, CORE, 1, 1).await.unwrap_err(),
        RigsError::NotVetoSigner,
    );
    let outsider = fx.outsider.insecure_clone();
    assert_rigs_error(
        fx.veto(&outsider, CORE, 1, 1).await.unwrap_err(),
        RigsError::NotVetoSigner,
    );

    // One veto signer (threshold 2) does not veto; it cannot vote twice.
    let v0 = fx.vetoers[0].insecure_clone();
    let v1 = fx.vetoers[1].insecure_clone();
    fx.veto(&v0, CORE, 1, 1).await.unwrap();
    fx.env.warp(1).await;
    assert_rigs_error(
        fx.veto(&v0, CORE, 1, 1).await.unwrap_err(),
        RigsError::DuplicateVeto,
    );
    assert_eq!(fx.root(CORE, 1, 1).await.status, 0);
    fx.veto(&v1, CORE, 1, 1).await.unwrap();
    assert_eq!(fx.root(CORE, 1, 1).await.status, 1); // Vetoed

    // Vetoed: never finalizes, never pays, L untouched.
    fx.env.warp(10 * WINDOW).await;
    assert_rigs_error(
        fx.finalize(CORE, 1, 1).await.unwrap_err(),
        RigsError::RootNotPending,
    );
    let err = fx
        .claim(&dave, CORE, 1, SOL, t.proofs[0].clone())
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::NoFinalRoot);
    assert_eq!(fx.ledger(CORE).await.liabilities, 0);

    // A fresh root with a higher version can proceed; veto after the window
    // is refused.
    fx.post(CORE, 1, 2, t.root, t1, 2 * SOL, true)
        .await
        .unwrap();
    fx.env.warp(WINDOW).await;
    assert_rigs_error(
        fx.veto(&v0, CORE, 1, 2).await.unwrap_err(),
        RigsError::ChallengeWindowClosed,
    );
    fx.finalize(CORE, 1, 2).await.unwrap();
    assert_eq!(
        fx.claim(&dave, CORE, 1, SOL, t.proofs[0].clone())
            .await
            .unwrap(),
        SOL
    );
}

// =================================================================== 61–62 claims

/// 61: a corrected lower entitlement never claws back a prior payment.
#[tokio::test]
async fn t61_lower_corrected_entitlement_never_claws_back() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 10 * SOL).await.unwrap();
    let (t0, _) = fx.live_pool(CORE, 1, 10 * SOL, 1_000).await;
    fx.env.set_time(t0 + 500).await;
    let alice = fx.env.alice.insecure_clone();
    let frank = fx.env.frank.insecure_clone();
    let v1 = tree(CORE, 1, &[(alice.pubkey(), 2 * SOL), (frank.pubkey(), SOL)]);
    fx.settle(CORE, 1, 1, v1.root, t0 + 500, 3 * SOL, false)
        .await;
    assert_eq!(
        fx.claim(&alice, CORE, 1, 2 * SOL, v1.proofs[0].clone())
            .await
            .unwrap(),
        2 * SOL
    );

    // v2 lowers alice to 1 SOL, raises frank to 2 SOL; the total counts
    // alice's 2 SOL already paid.
    let v2 = tree(CORE, 1, &[(alice.pubkey(), SOL), (frank.pubkey(), 2 * SOL)]);
    fx.settle(CORE, 1, 2, v2.root, t0 + 600, 4 * SOL, false)
        .await;
    let bal = fx.env.lamports(alice.pubkey()).await;
    assert_eq!(
        fx.claim(&alice, CORE, 1, SOL, v2.proofs[0].clone())
            .await
            .unwrap(),
        0
    );
    assert_eq!(fx.claimed(CORE, 1, &alice.pubkey()).await, 2 * SOL);
    assert!(
        fx.env.lamports(alice.pubkey()).await <= bal,
        "no payment to alice"
    );
    // The old v1 leaf is no longer valid (claims use the latest Final root).
    let err = fx
        .claim(&alice, CORE, 1, 2 * SOL, v1.proofs[0].clone())
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::InvalidMerkleProof);
    assert_eq!(
        fx.claim(&frank, CORE, 1, 2 * SOL, v2.proofs[1].clone())
            .await
            .unwrap(),
        2 * SOL
    );
    let p = fx.pool(CORE, 1).await;
    assert_eq!((p.paid_cum, p.final_total), (4 * SOL, 4 * SOL));
    assert_eq!(fx.ledger(CORE).await.liabilities, 0);
}

/// 62: two claims for one wallet in one transaction pay once; many
/// claimants in one transaction never take V below L.
#[tokio::test]
async fn t62_duplicate_and_concurrent_claims() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 10 * SOL).await.unwrap();
    let (_, t1) = fx.live_pool(CORE, 1, 10 * SOL, 1_000).await;
    fx.env.set_time(t1).await;
    let ws: Vec<Keypair> = [
        &fx.env.alice,
        &fx.env.bob,
        &fx.env.carol,
        &fx.env.dave,
        &fx.env.erin,
    ]
    .iter()
    .map(|k| k.insecure_clone())
    .collect();
    let leaves: Vec<(Pubkey, u64)> = ws.iter().map(|k| (k.pubkey(), 2 * SOL)).collect();
    let t = tree(CORE, 1, &leaves);
    fx.settle(CORE, 1, 1, t.root, t1, 10 * SOL, true).await;

    // Same wallet, same leaf, twice in one transaction.
    let ix = Fx::claim_ix_for(CORE, 1, &ws[0].pubkey(), 2 * SOL, t.proofs[0].clone());
    let v_before = fx.v(CORE).await;
    fx.send_many(&[ix.clone(), ix], &[&ws[0]]).await.unwrap();
    assert_eq!(v_before - fx.v(CORE).await, 2 * SOL);
    assert_eq!(fx.pool(CORE, 1).await.paid_cum, 2 * SOL);
    fx.claims
        .push(claim_pda(&pool_pda(CORE, 1), &ws[0].pubkey()));
    let mut paid = [0; 3];
    paid[0] = 2 * SOL;
    fx.check_paid(paid).await;

    // Three other wallets in one transaction, then the last one.
    let ixs: Vec<Instruction> = (1..4)
        .map(|i| Fx::claim_ix_for(CORE, 1, &ws[i].pubkey(), 2 * SOL, t.proofs[i].clone()))
        .collect();
    let signers: Vec<&Keypair> = ws[1..4].iter().collect();
    fx.send_many(&ixs, &signers).await.unwrap();
    for w in &ws[1..4] {
        fx.claims.push(claim_pda(&pool_pda(CORE, 1), &w.pubkey()));
    }
    paid[0] = 6 * SOL;
    fx.check_paid(paid).await;
    let last = ws[4].insecure_clone();
    assert_eq!(
        fx.claim(&last, CORE, 1, 2 * SOL, t.proofs[4].clone())
            .await
            .unwrap(),
        2 * SOL
    );
    let (l, p) = (fx.ledger(CORE).await, fx.pool(CORE, 1).await);
    assert_eq!((l.liabilities, p.paid_cum), (0, 10 * SOL));
    assert!(fx.v(CORE).await >= l.liabilities);
    // A later repeat pays zero.
    fx.env.warp(1).await;
    assert_eq!(
        fx.claim(&ws[3], CORE, 1, 2 * SOL, t.proofs[3].clone())
            .await
            .unwrap(),
        0
    );
}

// =================================================================== 64–66 close

/// 64: the final lamport remainder is released exactly once at close.
#[tokio::test]
async fn t64_final_remainder_released_exactly_once() {
    let mut fx = Fx::new().await;
    let e = 1_000_000_007;
    fx.fund(CORE, e).await.unwrap();
    let (_, t1) = fx.live_pool(CORE, 1, e, 3).await;
    fx.env.set_time(t1).await;
    fx.settle(CORE, 1, 1, [7; 32], t1, 999_999_999, true).await;
    let z = fx.ledger(CORE).await.reserve;
    fx.close(CORE, 1).await.unwrap();
    let l = fx.ledger(CORE).await;
    assert_eq!(
        (l.reserve - z, l.committed, l.liabilities),
        (8, 0, 999_999_999)
    );
    assert_eq!(fx.pool(CORE, 1).await.status, 4);
    fx.env.warp(1).await;
    assert_rigs_error(
        fx.close(CORE, 1).await.unwrap_err(),
        RigsError::FruitPoolClosed,
    );
    let err = fx
        .post(CORE, 1, 2, [7; 32], t1, 999_999_999, true)
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::FruitPoolClosed);
    // Released reserve is withdrawable; established L is not.
    fx.withdraw(CORE, 8).await.unwrap();
    assert_rigs_error(
        fx.withdraw(CORE, 1).await.unwrap_err(),
        RigsError::ReserveWithdrawalExceedsReserve,
    );
}

/// 65: close without reconciliation or with a Pending root is rejected;
/// elapsed time alone releases nothing.
#[tokio::test]
async fn t65_close_without_reconciliation_rejected() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 10 * SOL).await.unwrap();
    let (t0, t1) = fx.live_pool(CORE, 1, 10 * SOL, 1_000).await;
    fx.env.set_time(t0 + 10).await;
    assert_rigs_error(
        fx.close(CORE, 1).await.unwrap_err(),
        RigsError::FruitPoolNotEnded,
    );
    fx.env.set_time(t1 + 365 * 86_400).await;
    assert_rigs_error(
        fx.close(CORE, 1).await.unwrap_err(),
        RigsError::FruitPoolNotReconciled,
    );
    fx.settle(CORE, 1, 1, [1; 32], t1 - 100, 9 * SOL, false)
        .await;
    fx.env.warp(1).await;
    assert_rigs_error(
        fx.close(CORE, 1).await.unwrap_err(),
        RigsError::FruitPoolNotReconciled,
    );
    let err = fx
        .post(CORE, 1, 2, [1; 32], t1 - 1, 9 * SOL, true)
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::IncompleteCheckpoint);
    fx.post(CORE, 1, 2, [1; 32], t1, 9 * SOL, true)
        .await
        .unwrap();
    assert_eq!(fx.pool(CORE, 1).await.status, 3); // Reconciling
    assert_rigs_error(
        fx.close(CORE, 1).await.unwrap_err(),
        RigsError::RootAlreadyPending,
    );
    assert_eq!(fx.ledger(CORE).await.committed, SOL);
    fx.env.warp(WINDOW).await;
    fx.finalize(CORE, 1, 2).await.unwrap();
    fx.close(CORE, 1).await.unwrap();
    assert_eq!(fx.ledger(CORE).await.reserve, SOL);
}

/// 66: terminal resolution needs a veto quorum, cannot reduce L, and can
/// only add liabilities / release excess to Z.
#[tokio::test]
async fn t66_terminal_resolution_cannot_reduce_l() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 10 * SOL).await.unwrap();
    let (t0, t1) = fx.live_pool(CORE, 1, 10 * SOL, 1_000).await;
    fx.env.set_time(t0 + 600).await;
    fx.settle(CORE, 1, 1, [1; 32], t0 + 600, 6 * SOL, false)
        .await;
    fx.env.set_time(t1 + 100).await;
    let payer = fx.env.erin.insecure_clone();
    let vs: Vec<Keypair> = fx.vetoers.iter().map(|k| k.insecure_clone()).collect();
    let poster = fx.poster.insecure_clone();
    let t = tree(CORE, 1, &[(payer.pubkey(), SOL)]);

    // The poster alone: rejected. One veto signer (threshold 2): rejected.
    // The same veto signer twice: rejected.
    let ix = Fx::terminal_ix(
        &payer.pubkey(),
        CORE,
        1,
        2,
        t.root,
        t1,
        7 * SOL,
        &[poster.pubkey()],
    );
    assert_rigs_error(
        fx.env.send(ix, &[&payer, &poster]).await.unwrap_err(),
        RigsError::NotVetoSigner,
    );
    let ix = Fx::terminal_ix(
        &payer.pubkey(),
        CORE,
        1,
        2,
        t.root,
        t1,
        7 * SOL,
        &[vs[0].pubkey()],
    );
    assert_rigs_error(
        fx.env.send(ix, &[&payer, &vs[0]]).await.unwrap_err(),
        RigsError::TerminalQuorumNotMet,
    );
    let ix = Fx::terminal_ix(
        &payer.pubkey(),
        CORE,
        1,
        2,
        t.root,
        t1,
        7 * SOL,
        &[vs[0].pubkey(), vs[0].pubkey()],
    );
    assert!(fx.env.send(ix, &[&payer, &vs[0]]).await.is_err());

    let q = [vs[0].pubkey(), vs[2].pubkey()];
    // Below final_total: rejected — L cannot be reduced.
    let ix = Fx::terminal_ix(&payer.pubkey(), CORE, 1, 2, t.root, t1, 6 * SOL - 1, &q);
    assert_rigs_error(
        fx.env
            .send(ix, &[&payer, &vs[0], &vs[2]])
            .await
            .unwrap_err(),
        RigsError::RootBelowFinalTotal,
    );
    // Above E: rejected.
    let ix = Fx::terminal_ix(&payer.pubkey(), CORE, 1, 2, t.root, t1, 10 * SOL + 1, &q);
    assert_rigs_error(
        fx.env
            .send(ix, &[&payer, &vs[0], &vs[2]])
            .await
            .unwrap_err(),
        RigsError::RootExceedsCapacity,
    );
    assert_eq!(fx.ledger(CORE).await.liabilities, 6 * SOL);

    // Quorum, total >= final_total: final immediately, adds 1 SOL of L.
    let ix = Fx::terminal_ix(&payer.pubkey(), CORE, 1, 2, t.root, t1, 7 * SOL, &q);
    fx.env.send(ix, &[&payer, &vs[0], &vs[2]]).await.unwrap();
    fx.check().await;
    let r = fx.root(CORE, 1, 2).await;
    assert!(r.terminal && r.complete && r.status == 2);
    assert_eq!(fx.ledger(CORE).await.liabilities, 7 * SOL);
    fx.close(CORE, 1).await.unwrap();
    let l = fx.ledger(CORE).await;
    assert_eq!(
        (l.liabilities, l.committed, l.reserve),
        (7 * SOL, 0, 3 * SOL)
    );
    assert_eq!(
        fx.claim(&payer, CORE, 1, SOL, t.proofs[0].clone())
            .await
            .unwrap(),
        SOL
    );
}

// =================================================================== 67 versioning

/// 67: every Fruit account is versioned and layout-pinned; an account of any
/// other version is refused by every instruction that reads it (fail closed)
/// and nothing moves; history (every root version) is preserved on-chain.
#[tokio::test]
async fn t67_versioned_accounts_fail_closed_and_preserve_history() {
    // Layout pins: a migration must preserve exactly these fields.
    assert_eq!(
        8 + FruitConfig::INIT_SPACE,
        8 + 2 + 32 + 32 + 32 * 7 + 1 + 1 + 8 + 8 + 8 + 1 + 1 + 3 + 4 + 32 + 1
    );
    assert_eq!(8 + Ledger::INIT_SPACE, 8 + 2 + 1 + 32 + 8 * 6 + 1 + 1);
    assert_eq!(
        8 + Pool::INIT_SPACE,
        8 + 2 + 1 + 8 * 5 + 32 + 8 * 2 + 1 + 8 * 3 + 1 + 8 + 1 + 32 + 8 + 1
    );
    assert_eq!(
        8 + Root::INIT_SPACE,
        8 + 2 + 32 + 8 + 32 + 8 + 8 + 1 + 8 + 8 + 1 + 1 + 1 + 1 + 1
    );
    assert_eq!(8 + ClaimRecord::INIT_SPACE, 8 + 2 + 32 + 32 + 8 + 1);

    let mut fx = Fx::new().await;
    fx.fund(CORE, 10 * SOL).await.unwrap();
    let (_, t1) = fx.live_pool(CORE, 1, 10 * SOL, 1_000).await;
    fx.env.set_time(t1).await;
    let bob = fx.env.bob.insecure_clone();
    let t = tree(CORE, 1, &[(bob.pubkey(), SOL), (Pubkey::new_unique(), SOL)]);
    fx.settle(CORE, 1, 1, [3; 32], t1, SOL, false).await;
    fx.settle(CORE, 1, 2, t.root, t1, 2 * SOL, true).await;
    // Full root history is retained: v1 and v2 both still readable.
    assert_eq!(fx.root(CORE, 1, 1).await.merkle_root, [3; 32]);
    assert_eq!(fx.root(CORE, 1, 2).await.merkle_root, t.root);

    // Bump each account's state_version (bytes 8..10) in turn: every
    // instruction touching it refuses, and the raw state is unchanged.
    let ledger = ledger_pda(CORE);
    let pool = pool_pda(CORE, 1);
    let f = fx.env.funder.insecure_clone();
    for (key, ix, signer) in [
        (
            ledger,
            Fx::fund_ix(&f.pubkey(), CORE, SOL),
            f.insecure_clone(),
        ),
        (
            pool,
            Fx::claim_ix_for(CORE, 1, &bob.pubkey(), SOL, t.proofs[0].clone()),
            bob.insecure_clone(),
        ),
        (
            ledger,
            Fx::claim_ix_for(CORE, 1, &bob.pubkey(), SOL, t.proofs[0].clone()),
            bob.insecure_clone(),
        ),
        (
            fruit_config_pda(),
            Fx::pause_ix(&fx.env.authority.pubkey(), CORE, true),
            fx.admin(),
        ),
    ] {
        let mut acc = fx.env.account_raw(key).await.unwrap();
        let original = acc.clone();
        acc.data[8..10].copy_from_slice(&2u16.to_le_bytes());
        fx.env.ctx.set_account(&key, &acc.clone().into());
        let err = fx.env.send(ix, &[&signer]).await.unwrap_err();
        assert_rigs_error(err, RigsError::FruitVersionMismatch);
        assert_eq!(fx.env.account_raw(key).await.unwrap().data, acc.data);
        fx.env.ctx.set_account(&key, &original.into());
        fx.env.warp(1).await;
    }
    // Restored to the supported version, the same accounting continues:
    // lamports, L, U and claimed_cum were never touched.
    assert_eq!(
        fx.claim(&bob, CORE, 1, SOL, t.proofs[0].clone())
            .await
            .unwrap(),
        SOL
    );
    let l = fx.ledger(CORE).await;
    assert_eq!(
        (l.liabilities, l.committed, l.paid_cum),
        (SOL, 8 * SOL, SOL)
    );
    let raw = fx
        .env
        .account_raw(claim_pda(&pool, &bob.pubkey()))
        .await
        .unwrap();
    let rec = ClaimRecord::try_deserialize(&mut raw.data.as_slice()).unwrap();
    assert_eq!(
        (rec.state_version, rec.claimed_cum),
        (FRUIT_STATE_VERSION, SOL)
    );
}

// =================================================================== pause

#[tokio::test]
async fn pause_stops_pools_and_roots_but_never_claims_or_l() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 20 * SOL).await.unwrap();
    let (_, t1) = fx.live_pool(CORE, 1, 10 * SOL, 1_000).await;
    fx.env.set_time(t1).await;
    let carol = fx.env.carol.insecure_clone();
    let t = tree(CORE, 1, &[(carol.pubkey(), 2 * SOL)]);
    fx.settle(CORE, 1, 1, t.root, t1, 5 * SOL, false).await;

    let a = fx.admin();
    fx.env
        .send(Fx::pause_ix(&a.pubkey(), CORE, true), &[&a])
        .await
        .unwrap();
    let l = fx.ledger(CORE).await.liabilities;
    let now = fx.now().await;
    let err = fx
        .commit(CORE, 2, SOL, now + 100, now + 200)
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::FruitPaused);
    let err = fx
        .post(CORE, 1, 2, t.root, t1, 6 * SOL, true)
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::FruitPaused);
    assert_rigs_error(fx.close(CORE, 1).await.unwrap_err(), RigsError::FruitPaused);
    assert_eq!(fx.ledger(CORE).await.liabilities, l, "pause never erases L");
    // Claims continue while paused.
    assert_eq!(
        fx.claim(&carol, CORE, 1, 2 * SOL, t.proofs[0].clone())
            .await
            .unwrap(),
        2 * SOL
    );
    // Other ledgers are unaffected.
    fx.fund(LOYALTY, SOL).await.unwrap();
    fx.commit(LOYALTY, 1, SOL, now + 100, now + 200)
        .await
        .unwrap();
    // Only the admin pauses.
    let o = fx.outsider.insecure_clone();
    let err = fx
        .env
        .send(Fx::pause_ix(&o.pubkey(), CORE, false), &[&o])
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::UnauthorizedFruitAdmin);
    fx.env
        .send(Fx::pause_ix(&a.pubkey(), CORE, false), &[&a])
        .await
        .unwrap();
    fx.post(CORE, 1, 2, t.root, t1, 6 * SOL, true)
        .await
        .unwrap();
}

// =================================================================== 73–76 funding

/// 73: Z < E commit rejected; commit at or after t_start rejected.
#[tokio::test]
async fn t73_underfunded_or_late_commit_rejected() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, SOL).await.unwrap();
    let now = fx.now().await;
    let err = fx
        .commit(CORE, 1, SOL + 1, now + 100, now + 200)
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::InsufficientFruitReserve);
    let err = fx.commit(CORE, 1, SOL, now, now + 200).await.unwrap_err();
    assert_rigs_error(err, RigsError::FruitPoolStarted);
    let err = fx
        .commit(CORE, 1, SOL, now - 10, now + 200)
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::FruitPoolStarted);
    let err = fx
        .commit(CORE, 1, SOL, now + 200, now + 200)
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::InvalidPoolWindow);
    let l = fx.ledger(CORE).await;
    assert_eq!((l.reserve, l.committed), (SOL, 0));
    // Only the admin commits.
    let o = fx.outsider.insecure_clone();
    let ix = Fx::commit_ix(&o.pubkey(), CORE, 1, SOL, now + 100, now + 200);
    assert_rigs_error(
        fx.env.send(ix, &[&o]).await.unwrap_err(),
        RigsError::UnauthorizedFruitAdmin,
    );
    // Lock pools fail closed until the lock mechanics exist.
    fx.fund(LOCK, SOL).await.unwrap();
    let err = fx
        .commit(LOCK, 1, SOL, now + 100, now + 200)
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::LockPoolsNotImplemented);
    fx.commit(CORE, 1, SOL, now + 100, now + 200).await.unwrap();
    let l = fx.ledger(CORE).await;
    assert_eq!((l.reserve, l.committed), (0, SOL));
}

/// 74: the same reserve cannot fund two commitments — not on one ledger,
/// not across ledgers, and a pool id commits once.
#[tokio::test]
async fn t74_same_reserve_cannot_fund_two_commitments() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 5 * SOL).await.unwrap();
    let now = fx.now().await;
    fx.commit(CORE, 1, 5 * SOL, now + 100, now + 200)
        .await
        .unwrap();
    let err = fx
        .commit(CORE, 2, 5 * SOL, now + 100, now + 200)
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::InsufficientFruitReserve);
    // Loyalty cannot draw on core's reserve.
    let err = fx
        .commit(LOYALTY, 1, 1, now + 100, now + 200)
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::InsufficientFruitReserve);
    // Re-committing an existing pool id is impossible even with reserve.
    fx.fund(CORE, 5 * SOL).await.unwrap();
    fx.env.warp(1).await; // a fresh transaction, not a deduplicated resend
    let err = fx
        .commit(CORE, 1, 5 * SOL, now + 100, now + 200)
        .await
        .unwrap_err();
    assert!(
        format!("{err:?}").contains("Custom(0)"),
        "account already in use: {err:?}"
    );
    let l = fx.ledger(CORE).await;
    assert_eq!((l.reserve, l.committed), (5 * SOL, 5 * SOL));
}

/// 75: a withdrawal touching L or U is rejected; only Z, only to the
/// configured reserve recipient.
#[tokio::test]
async fn t75_withdrawal_touching_l_or_u_rejected() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 11 * SOL).await.unwrap();
    let (t0, _) = fx.live_pool(CORE, 1, 10 * SOL, 1_000).await;
    fx.env.set_time(t0 + 400).await;
    fx.settle(CORE, 1, 1, [1; 32], t0 + 400, 4 * SOL, false)
        .await; // L=4 U=6 Z=1
    assert_eq!(fx.v(CORE).await, 11 * SOL);
    for amount in [SOL + 1, 5 * SOL, 11 * SOL] {
        let err = fx.withdraw(CORE, amount).await.unwrap_err();
        assert_rigs_error(err, RigsError::ReserveWithdrawalExceedsReserve);
    }
    let a = fx.admin();
    let ix = Fx::withdraw_ix(&a.pubkey(), CORE, SOL, a.pubkey());
    assert_rigs_error(
        fx.env.send(ix, &[&a]).await.unwrap_err(),
        RigsError::ReserveRecipientMismatch,
    );
    let o = fx.outsider.insecure_clone();
    let ix = Fx::withdraw_ix(&o.pubkey(), CORE, SOL, fx.reserve_recipient);
    assert_rigs_error(
        fx.env.send(ix, &[&o]).await.unwrap_err(),
        RigsError::UnauthorizedFruitAdmin,
    );
    let before = fx.env.lamports(fx.reserve_recipient).await;
    fx.withdraw(CORE, SOL).await.unwrap();
    assert_eq!(fx.env.lamports(fx.reserve_recipient).await - before, SOL);
    let l = fx.ledger(CORE).await;
    assert_eq!(
        (l.liabilities, l.committed, l.reserve),
        (4 * SOL, 6 * SOL, 0)
    );
    fx.env.warp(1).await;
    assert_rigs_error(
        fx.withdraw(CORE, 1).await.unwrap_err(),
        RigsError::ReserveWithdrawalExceedsReserve,
    );
}

/// 76: exhausted future funding does not invalidate existing commitments.
#[tokio::test]
async fn t76_exhausted_funding_leaves_commitments_intact() {
    let mut fx = Fx::new().await;
    fx.fund(CORE, 4 * SOL).await.unwrap();
    let now = fx.now().await;
    fx.commit(CORE, 1, 2 * SOL, now + 100, now + 200)
        .await
        .unwrap();
    fx.commit(CORE, 2, 2 * SOL, now + 300, now + 400)
        .await
        .unwrap();
    let err = fx
        .commit(CORE, 3, 1, now + 500, now + 600)
        .await
        .unwrap_err();
    assert_rigs_error(err, RigsError::InsufficientFruitReserve);
    fx.env.set_time(now + 400).await;
    let alice = fx.env.alice.insecure_clone();
    let cap = 2 * SOL / 5;
    for id in [1, 2] {
        let t = tree(CORE, id, &[(alice.pubkey(), cap)]);
        let end = if id == 1 { now + 200 } else { now + 400 };
        fx.settle(CORE, id, 1, t.root, end, 2 * SOL, true).await;
        assert_eq!(
            fx.claim(&alice, CORE, id, cap, t.proofs[0].clone())
                .await
                .unwrap(),
            cap
        );
        fx.close(CORE, id).await.unwrap();
    }
    let l = fx.ledger(CORE).await;
    assert_eq!(
        (l.liabilities, l.committed, l.reserve),
        (4 * SOL - 2 * cap, 0, 0)
    );
}
