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
/// Genesis fresh-purchase cap: the only value `initialize` accepts (250),
/// equal to the permanent wallet cap.
pub const GENESIS_CAP: u64 = rigs_engine::GENESIS_CAP_RIGS;
/// Permanent per-wallet seed cap.
pub const WALLET_CAP: u64 = rigs_engine::MAX_WALLET_RIGS;

/// THE CHERRY TREE protocol fee recipient — the Seeker fee-receiving wallet.
///
/// A public address and nothing more: it never signs, holds no role in the
/// program beyond being the one account `withdraw_fees` may pay, and no
/// private key for it exists anywhere in this repository or its CI.
pub const FEE_RECIPIENT: &str = "FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp";

pub fn fee_recipient() -> Pubkey {
    Pubkey::from_str(FEE_RECIPIENT).expect("FEE_RECIPIENT is not a valid base58 pubkey")
}

/// Raw bytes of an ANS (AllDomains) name record header.
pub fn ans_record(parent: &Pubkey, owner: &Pubkey, expires_at: u64) -> Vec<u8> {
    let mut d = vec![0u8; 200 + 32];
    d[..8].copy_from_slice(&ANS_NAME_RECORD_DISCRIMINATOR);
    d[8..40].copy_from_slice(parent.as_ref());
    d[40..72].copy_from_slice(owner.as_ref());
    d[104..112].copy_from_slice(&expires_at.to_le_bytes());
    d
}

/// Raw bytes of an SNS name record header.
pub fn sns_record(parent: &Pubkey, owner: &Pubkey) -> Vec<u8> {
    let mut d = vec![0u8; 96 + 32];
    d[..32].copy_from_slice(parent.as_ref());
    d[32..64].copy_from_slice(owner.as_ref());
    d
}

/// AllDomains Name House `NftRecord` bytes.
pub fn ans_nft_record(tag: u8, name: &Pubkey, mint: &Pubkey) -> Vec<u8> {
    let mut d = vec![0u8; 8 + 1 + 1 + 32 * 4];
    d[..8].copy_from_slice(&ANS_NFT_RECORD_DISCRIMINATOR);
    d[8] = tag;
    d[10..42].copy_from_slice(name.as_ref());
    d[74..106].copy_from_slice(mint.as_ref());
    d
}

/// SNS tokenizer `NftRecord` bytes.
pub fn sns_nft_record(tag: u8, name: &Pubkey, mint: &Pubkey) -> Vec<u8> {
    let mut d = vec![0u8; 1 + 1 + 32 * 3];
    d[0] = tag;
    d[2..34].copy_from_slice(name.as_ref());
    d[66..98].copy_from_slice(mint.as_ref());
    d
}

/// SPL token account bytes (165).
pub fn token_account(mint: &Pubkey, owner: &Pubkey, amount: u64, state: u8) -> Vec<u8> {
    let mut d = vec![0u8; 165];
    d[..32].copy_from_slice(mint.as_ref());
    d[32..64].copy_from_slice(owner.as_ref());
    d[64..72].copy_from_slice(&amount.to_le_bytes());
    d[108] = state;
    d
}

pub fn program_owned(owner_program: Pubkey, data: Vec<u8>) -> Account {
    Account {
        lamports: SOL,
        data,
        owner: owner_program,
        ..Account::default()
    }
}

/// The ways a test wallet can hold (or not hold) a domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    SkrDirect,
    SkrWrapped,
    SnsDirect,
    SnsTokenized,
}

pub fn skr_wrapped_record_key(name: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[b"nft_record", SKR_NAME_HOUSE.as_ref(), name.as_ref()],
        &ANS_NAME_HOUSE_PROGRAM_ID,
    )
    .0
}

pub fn sns_tokenized_record_key(name: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"nft_record", name.as_ref()], &SNS_NAME_TOKENIZER_ID).0
}

/// Plant a domain for `wallet`; returns the identity accounts to pass.
pub fn plant_domain(pt: &mut ProgramTest, wallet: &Pubkey, kind: Domain) -> Vec<Pubkey> {
    let name = Pubkey::new_unique();
    match kind {
        Domain::SkrDirect => {
            pt.add_account(
                name,
                program_owned(ANS_PROGRAM_ID, ans_record(&SKR_TLD_PARENT, wallet, 0)),
            );
            vec![name]
        }
        Domain::SnsDirect => {
            pt.add_account(
                name,
                program_owned(SNS_NAME_PROGRAM_ID, sns_record(&SNS_TLD_ROOT, wallet)),
            );
            vec![name]
        }
        Domain::SkrWrapped => {
            let record = skr_wrapped_record_key(&name);
            let (mint, token) = (Pubkey::new_unique(), Pubkey::new_unique());
            pt.add_account(
                name,
                program_owned(ANS_PROGRAM_ID, ans_record(&SKR_TLD_PARENT, &record, 0)),
            );
            pt.add_account(
                record,
                program_owned(ANS_NAME_HOUSE_PROGRAM_ID, ans_nft_record(1, &name, &mint)),
            );
            pt.add_account(
                token,
                program_owned(SPL_TOKEN_PROGRAM_ID, token_account(&mint, wallet, 1, 1)),
            );
            vec![name, record, token]
        }
        Domain::SnsTokenized => {
            let record = sns_tokenized_record_key(&name);
            let (mint, token) = (Pubkey::new_unique(), Pubkey::new_unique());
            pt.add_account(
                name,
                program_owned(SNS_NAME_PROGRAM_ID, sns_record(&SNS_TLD_ROOT, &record)),
            );
            pt.add_account(
                record,
                program_owned(SNS_NAME_TOKENIZER_ID, sns_nft_record(2, &name, &mint)),
            );
            pt.add_account(
                token,
                program_owned(SPL_TOKEN_PROGRAM_ID, token_account(&mint, wallet, 1, 1)),
            );
            vec![name, record, token]
        }
    }
}

pub struct Env {
    pub ctx: ProgramTestContext,
    pub authority: Keypair,
    /// Pinned as `config.founder`. Alias of `alice`, so every legacy test's
    /// first buy is the activation. Also holds a direct `.skr`, so her later
    /// purchases pass the genesis gate like any `.skr` holder's.
    pub founder: Keypair,
    /// bob: wrapped `.skr` (AllDomains NFT). carol: direct `.skr`.
    pub carol: Keypair,
    /// Tokenized `.sns` only.
    pub dave: Keypair,
    /// Holds no domain at all.
    pub erin: Keypair,
    /// Direct `.sns` only.
    pub frank: Keypair,
    /// wallet → identity accounts passed with its purchases.
    pub identities: std::collections::HashMap<Pubkey, Vec<Pubkey>>,
    /// wallet → its `.skr` name account, for wallets planted with a `.skr`.
    /// `buy_ix` appends that domain's binding PDA, as the client does.
    pub skr_names: std::collections::HashMap<Pubkey, Pubkey>,
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
/// Day-1 `.skr` binding PDA for a domain's ANS name account.
pub fn skr_binding_pda(name: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[SKR_BIND_SEED, name.as_ref()], &sol_rigs::ID).0
}

/// The loader-derived ProgramData address of the program: what a real
/// `anchor deploy` / `solana program deploy` creates, and the only account
/// `initialize` accepts as proof of the upgrade authority.
pub fn program_data_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[sol_rigs::ID.as_ref()],
        &solana_sdk::bpf_loader_upgradeable::ID,
    )
    .0
}

/// Bincode `UpgradeableLoaderState::ProgramData { slot, upgrade_authority }`
/// header, byte for byte what the upgradeable loader writes (45 bytes), owned
/// by that loader. Built by hand so the test pins the wire format itself.
pub fn program_data_account(upgrade_authority: Option<Pubkey>) -> Account {
    let mut data = vec![0u8; 45];
    data[..4].copy_from_slice(&3u32.to_le_bytes()); // ProgramData variant
                                                    // slot (u64) stays 0
    if let Some(a) = upgrade_authority {
        data[12] = 1;
        data[13..45].copy_from_slice(a.as_ref());
    }
    Account {
        lamports: SOL,
        data,
        owner: solana_sdk::bpf_loader_upgradeable::ID,
        ..Account::default()
    }
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
        Env::with(|_| {}).await
    }

    /// Build an environment, letting a test plant extra accounts first.
    pub async fn with(extra: impl FnOnce(&mut ProgramTest)) -> Env {
        let mut pt = ProgramTest::new("sol_rigs", sol_rigs::ID, processor!(entry_shim));
        pt.set_compute_max_units(400_000);

        let authority = funded(&mut pt, 1_000 * SOL);
        // The deployer is the upgrade authority, as `devnet-deploy.sh` leaves it.
        pt.add_account(
            program_data_pda(),
            program_data_account(Some(authority.pubkey())),
        );
        let fundr = funded(&mut pt, 10_000 * SOL);
        let alice = funded(&mut pt, 10_000 * SOL);
        let bob = funded(&mut pt, 10_000 * SOL);
        let carol = funded(&mut pt, 10_000 * SOL);
        let dave = funded(&mut pt, 10_000 * SOL);
        let erin = funded(&mut pt, 10_000 * SOL);
        let frank = funded(&mut pt, 10_000 * SOL);

        let mut identities = std::collections::HashMap::new();
        let mut skr_names = std::collections::HashMap::new();
        for (who, kind) in [
            (&alice, Domain::SkrDirect),
            (&bob, Domain::SkrWrapped),
            (&carol, Domain::SkrDirect),
            (&dave, Domain::SnsTokenized),
            (&frank, Domain::SnsDirect),
        ] {
            let accounts = plant_domain(&mut pt, &who.pubkey(), kind);
            if matches!(kind, Domain::SkrDirect | Domain::SkrWrapped) {
                skr_names.insert(who.pubkey(), accounts[0]);
            }
            identities.insert(who.pubkey(), accounts);
        }
        extra(&mut pt);

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
            founder: alice.insecure_clone(),
            alice,
            bob,
            carol,
            dave,
            erin,
            frank,
            identities,
            skr_names,
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

    /// Set the on-chain clock to exactly `unix_timestamp`.
    pub async fn set_time(&mut self, unix_timestamp: i64) {
        let now = self.clock().await.unix_timestamp;
        self.warp(unix_timestamp - now).await;
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
        self.initialize_with_cap(GENESIS_CAP).await
    }

    /// `initialize` with an explicit Genesis cap, so cap-specific tests can
    /// pin tight values.
    pub async fn initialize_with_cap(
        &mut self,
        genesis_cap_rigs: u64,
    ) -> Result<(), TransportError> {
        let mut ix = self.initialize_ix(&self.authority.pubkey(), program_data_pda());
        ix.data = sol_rigs::instruction::Initialize {
            rig_price_lamports: RIG_PRICE,
            purchase_fee_bps: FEE_BPS,
            reward_funder: self.funder.pubkey(),
            fee_recipient: self.fee_recipient,
            founder: self.founder.pubkey(),
            genesis_cap_rigs,
        }
        .data();
        let auth = self.authority.insecure_clone();
        self.send(ix, &[&auth]).await
    }

    /// Canonical `initialize` with the fixture's config values, signed by
    /// `signer` and naming `program_data` as the upgrade-authority proof.
    pub fn initialize_ix(&self, signer: &Pubkey, program_data: Pubkey) -> Instruction {
        Instruction {
            program_id: sol_rigs::ID,
            accounts: sol_rigs::accounts::Initialize {
                authority: *signer,
                config: config_pda(),
                capital_vault: capital_vault_pda(),
                reward_vault: reward_vault_pda(),
                fee_vault: fee_vault_pda(),
                system_program: solana_sdk::system_program::ID,
                program_data,
            }
            .to_account_metas(None),
            data: sol_rigs::instruction::Initialize {
                rig_price_lamports: RIG_PRICE,
                purchase_fee_bps: FEE_BPS,
                reward_funder: self.funder.pubkey(),
                fee_recipient: self.fee_recipient,
                founder: self.founder.pubkey(),
                genesis_cap_rigs: GENESIS_CAP,
            }
            .data(),
        }
    }

    /// Raw on-chain account bytes (or `None`), for layout-level assertions.
    pub async fn account_raw(&mut self, key: Pubkey) -> Option<solana_sdk::account::Account> {
        self.ctx.banks_client.get_account(key).await.unwrap()
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

    /// `buy_rigs` with the buyer's planted identity (none for erin) and no
    /// referrer. A `.skr` identity carries its binding PDA, as the client
    /// sends it; the program only consults it during Day 1.
    pub fn buy_ix(&self, owner: &Pubkey, count: u64) -> Instruction {
        let identity = self.identities.get(owner).cloned().unwrap_or_default();
        self.buy_ix_with(owner, count, identity, None)
    }

    /// `buy_rigs` with explicit identity accounts followed by a writable
    /// binding account, and no referrer.
    pub fn buy_ix_bound(
        &self,
        owner: &Pubkey,
        count: u64,
        identity: Vec<Pubkey>,
        binding: Pubkey,
    ) -> Instruction {
        let mut ix = self.buy_ix_unbound(owner, count, identity, None);
        ix.accounts
            .push(solana_sdk::instruction::AccountMeta::new(binding, false));
        ix
    }

    /// `buy_rigs` with explicit remaining accounts: identity accounts, the
    /// `.skr` binding PDA when the identity is a planted `.skr` domain (as the
    /// client sends it), then `(referrer, referrer_miner)` if given.
    pub fn buy_ix_with(
        &self,
        owner: &Pubkey,
        count: u64,
        identity: Vec<Pubkey>,
        referrer: Option<(Pubkey, Pubkey)>,
    ) -> Instruction {
        let binding = identity
            .first()
            .filter(|name| self.skr_names.values().any(|n| n == *name))
            .map(skr_binding_pda);
        let mut ix = self.buy_ix_unbound(owner, count, identity, referrer.map(|r| r.0));
        use solana_sdk::instruction::AccountMeta;
        if let Some(b) = binding {
            ix.accounts.push(AccountMeta::new(b, false));
        }
        if let Some((_, referrer_miner)) = referrer {
            ix.accounts
                .push(AccountMeta::new_readonly(referrer_miner, false));
        }
        ix
    }

    /// `buy_rigs` with exactly the identity accounts given — no binding
    /// appended, no referrer account (the referrer arg is still encoded).
    pub fn buy_ix_unbound(
        &self,
        owner: &Pubkey,
        count: u64,
        identity: Vec<Pubkey>,
        referrer: Option<Pubkey>,
    ) -> Instruction {
        let mut ix = self.buy_ix_raw(owner, count, referrer);
        use solana_sdk::instruction::AccountMeta;
        for key in identity {
            ix.accounts.push(AccountMeta::new_readonly(key, false));
        }
        ix
    }

    pub fn buy_ix_raw(&self, owner: &Pubkey, count: u64, referrer: Option<Pubkey>) -> Instruction {
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
            data: sol_rigs::instruction::BuyRigs { count, referrer }.data(),
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
