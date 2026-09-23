//! `solana-program-test` harness for THE CHERRY TREE program.
//!
//! Runs the real Anchor entrypoint against a real runtime: real account
//! validation, real PDAs, real lamport movement. No SBF toolchain required.

#![allow(dead_code)]

use std::str::FromStr;

use anchor_lang::{InstructionData, ToAccountMetas};
use solana_program_test::{processor, ProgramTest, ProgramTestContext};
use solana_sdk::{
    account::Account,
    clock::Clock,
    instruction::Instruction,
    native_token::LAMPORTS_PER_SOL,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
    transport::TransportError,
};

use sol_rigs::constants::*;

pub const SOL: u64 = LAMPORTS_PER_SOL;
pub const RIG_PRICE: u64 = SOL / 100; // 0.01 SOL — test fixture only
pub const FEE_BPS: u16 = 300; // 3.00%

/// THE CHERRY TREE protocol fee recipient — the Seeker fee-receiving wallet.
///
/// A public address and nothing more: it never signs, holds no role in the
/// program beyond being the one account `withdraw_fees` may pay, and no
/// private key for it exists anywhere in this repository or its CI.
pub const FEE_RECIPIENT: &str = "FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp";

pub fn fee_recipient() -> Pubkey {
    Pubkey::from_str(FEE_RECIPIENT).expect("FEE_RECIPIENT is not a valid base58 pubkey")
}

pub struct Env {
    pub ctx: ProgramTestContext,
    pub authority: Keypair,
    /// Where fees are configured to land. Not a signer — see [`FEE_RECIPIENT`].
    pub fee_recipient: Pubkey,
    pub funder: Keypair,
    pub alice: Keypair,
    pub bob: Keypair,
}

pub fn config_pda() -> Pubkey {
    Pubkey::find_program_address(&[CONFIG_SEED], &sol_rigs::ID).0
}
pub fn miner_pda(owner: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[MINER_SEED, owner.as_ref()], &sol_rigs::ID).0
}
pub fn capital_vault_pda() -> Pubkey {
    Pubkey::find_program_address(&[CAPITAL_VAULT_SEED], &sol_rigs::ID).0
}
pub fn reward_vault_pda() -> Pubkey {
    Pubkey::find_program_address(&[REWARD_VAULT_SEED], &sol_rigs::ID).0
}
pub fn fee_vault_pda() -> Pubkey {
    Pubkey::find_program_address(&[FEE_VAULT_SEED], &sol_rigs::ID).0
}

/// `solana-program-test` wants a processor whose account-slice lifetime is
/// free, while Anchor's generated `entry` ties the slice and its contents to
/// one `'info`. This shim re-borrows the slice for a long-enough lifetime.
/// Test-only: the leak is bounded by the test process.
fn entry_shim<'info>(
    program_id: &anchor_lang::prelude::Pubkey,
    accounts: &[anchor_lang::prelude::AccountInfo<'info>],
    data: &[u8],
) -> anchor_lang::solana_program::entrypoint::ProgramResult {
    let accounts: &'info [anchor_lang::prelude::AccountInfo<'info>] =
        Box::leak(accounts.to_vec().into_boxed_slice());
    sol_rigs::entry(program_id, accounts, data)
}

fn funded(pt: &mut ProgramTest, lamports: u64) -> Keypair {
    let kp = Keypair::new();
    pt.add_account(
        kp.pubkey(),
        Account {
            lamports,
            ..Account::default()
        },
    );
    kp
}

impl Env {
    pub async fn new() -> Env {
        let mut pt = ProgramTest::new("sol_rigs", sol_rigs::ID, processor!(entry_shim));
        pt.set_compute_max_units(400_000);

        let authority = funded(&mut pt, 1_000 * SOL);
        let fundr = funded(&mut pt, 10_000 * SOL);
        let alice = funded(&mut pt, 10_000 * SOL);
        let bob = funded(&mut pt, 10_000 * SOL);

        // Give the recipient an existing account so a withdrawal is a plain
        // credit, the way it will be on devnet.
        pt.add_account(
            fee_recipient(),
            Account {
                lamports: SOL,
                ..Account::default()
            },
        );

        let ctx = pt.start_with_context().await;
        Env {
            ctx,
            authority,
            fee_recipient: fee_recipient(),
            funder: fundr,
            alice,
            bob,
        }
    }

    pub async fn send(
        &mut self,
        ix: Instruction,
        signers: &[&Keypair],
    ) -> Result<(), TransportError> {
        let payer = self.ctx.payer.insecure_clone();
        let mut all: Vec<&Keypair> = vec![&payer];
        all.extend_from_slice(signers);
        let blockhash = self.ctx.banks_client.get_latest_blockhash().await.unwrap();
        let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &all, blockhash);
        self.ctx
            .banks_client
            .process_transaction(tx)
            .await
            .map_err(Into::into)
    }

    pub async fn lamports(&mut self, key: Pubkey) -> u64 {
        self.ctx
            .banks_client
            .get_account(key)
            .await
            .unwrap()
            .map(|a| a.lamports)
            .unwrap_or(0)
    }

    pub async fn config(&mut self) -> sol_rigs::state::Config {
        self.anchor_account(config_pda()).await
    }

    pub async fn miner(&mut self, owner: &Pubkey) -> sol_rigs::state::Miner {
        self.anchor_account(miner_pda(owner)).await
    }

    pub async fn anchor_account<T: anchor_lang::AccountDeserialize>(&mut self, key: Pubkey) -> T {
        let acc = self
            .ctx
            .banks_client
            .get_account(key)
            .await
            .unwrap()
            .expect("account not found");
        T::try_deserialize(&mut acc.data.as_slice()).unwrap()
    }

    pub async fn clock(&mut self) -> Clock {
        self.ctx.banks_client.get_sysvar::<Clock>().await.unwrap()
    }

    /// Advance the on-chain `unix_timestamp` by `secs` without waiting.
    pub async fn warp(&mut self, secs: i64) {
        let mut clock = self.clock().await;
        clock.unix_timestamp += secs;
        let slot = self.ctx.banks_client.get_root_slot().await.unwrap();
        self.ctx.warp_to_slot(slot + 2).unwrap();
        clock.slot = slot + 2;
        self.ctx.set_sysvar(&clock);
    }

    // ---------------------------------------------------------- instructions

    pub async fn initialize(&mut self) -> Result<(), TransportError> {
        let ix = Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::Initialize {
                authority: self.authority.pubkey(),
                config: config_pda(),
                capital_vault: capital_vault_pda(),
                reward_vault: reward_vault_pda(),
                fee_vault: fee_vault_pda(),
                system_program: solana_sdk::system_program::ID,
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::Initialize {
                rig_price_lamports: RIG_PRICE,
                purchase_fee_bps: FEE_BPS,
                reward_funder: self.funder.pubkey(),
                fee_recipient: self.fee_recipient,
            }
            .data(),
        };
        let auth = self.authority.insecure_clone();
        self.send(ix, &[&auth]).await
    }

    pub fn fund_rewards_ix(&self, funder: &Pubkey, amount: u64, duration: u64) -> Instruction {
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::FundRewards {
                funder: *funder,
                config: config_pda(),
                reward_vault: reward_vault_pda(),
                system_program: solana_sdk::system_program::ID,
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::FundRewards { amount, duration }.data(),
        }
    }

    pub async fn fund_rewards(&mut self, amount: u64, duration: u64) -> Result<(), TransportError> {
        let f = self.funder.insecure_clone();
        let ix = self.fund_rewards_ix(&f.pubkey(), amount, duration);
        self.send(ix, &[&f]).await
    }

    pub fn buy_ix(&self, owner: &Pubkey, count: u64) -> Instruction {
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::BuyRigs {
                owner: *owner,
                config: config_pda(),
                miner: miner_pda(owner),
                capital_vault: capital_vault_pda(),
                fee_vault: fee_vault_pda(),
                reward_vault: reward_vault_pda(),
                system_program: solana_sdk::system_program::ID,
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::BuyRigs { count }.data(),
        }
    }

    pub async fn buy(&mut self, who: &Keypair, count: u64) -> Result<(), TransportError> {
        let ix = self.buy_ix(&who.pubkey(), count);
        let who = who.insecure_clone();
        self.send(ix, &[&who]).await
    }

    pub fn claim_ix(&self, owner: &Pubkey, miner: Pubkey) -> Instruction {
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::ClaimRewards {
                owner: *owner,
                config: config_pda(),
                miner,
                reward_vault: reward_vault_pda(),
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::Claim {}.data(),
        }
    }

    pub async fn claim(&mut self, who: &Keypair) -> Result<(), TransportError> {
        let ix = self.claim_ix(&who.pubkey(), miner_pda(&who.pubkey()));
        let who = who.insecure_clone();
        self.send(ix, &[&who]).await
    }

    pub fn compound_ix(&self, owner: &Pubkey) -> Instruction {
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::CompoundRewards {
                owner: *owner,
                config: config_pda(),
                miner: miner_pda(owner),
                reward_vault: reward_vault_pda(),
                capital_vault: capital_vault_pda(),
                fee_vault: fee_vault_pda(),
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::Compound {}.data(),
        }
    }

    pub async fn compound(&mut self, who: &Keypair) -> Result<(), TransportError> {
        let ix = self.compound_ix(&who.pubkey());
        let who = who.insecure_clone();
        self.send(ix, &[&who]).await
    }

    pub async fn set_paused(&mut self, paused: bool) -> Result<(), TransportError> {
        let ix = Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::AdminOnly {
                authority: self.authority.pubkey(),
                config: config_pda(),
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::SetPaused { paused }.data(),
        };
        let a = self.authority.insecure_clone();
        self.send(ix, &[&a]).await
    }

    pub fn set_fee_recipient_ix(&self, authority: &Pubkey, fee_recipient: Pubkey) -> Instruction {
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::AdminOnly {
                authority: *authority,
                config: config_pda(),
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::SetFeeRecipient { fee_recipient }.data(),
        }
    }

    pub fn withdraw_fees_ix(
        &self,
        authority: &Pubkey,
        vault: Pubkey,
        destination: Pubkey,
        amount: u64,
    ) -> Instruction {
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::WithdrawFees {
                authority: *authority,
                config: config_pda(),
                fee_vault: vault,
                destination,
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::WithdrawFees { amount }.data(),
        }
    }
}

/// Assert a transaction failed with a specific `RigsError`.
pub fn assert_rigs_error(err: TransportError, expected: sol_rigs::error::RigsError) {
    let code = anchor_lang::error::ERROR_CODE_OFFSET + expected as u32;
    let s = format!("{err:?}");
    assert!(
        s.contains(&format!("Custom({code})")),
        "expected {expected:?} (custom {code}), got: {s}"
    );
}
