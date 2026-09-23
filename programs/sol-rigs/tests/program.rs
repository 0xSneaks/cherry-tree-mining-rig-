//! On-chain acceptance tests: spec §12 items 14-18, plus end-to-end lamport
//! accounting for items 3, 6 and 13 against the real runtime.

mod common;

use common::*;
use sol_rigs::error::RigsError;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

// ------------------------------------------------------------- end-to-end

#[tokio::test]
async fn e2e_buy_accrue_claim_moves_real_lamports() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();

    let reward_vault = reward_vault_pda();
    let capital_vault = capital_vault_pda();
    let fee_vault = fee_vault_pda();

    let reward_rent = env.lamports(reward_vault).await;
    let capital_rent = env.lamports(capital_vault).await;
    let fee_rent = env.lamports(fee_vault).await;

    // Fund a real schedule: 100 SOL over 1000 seconds.
    env.fund_rewards(100 * SOL, 1_000).await.unwrap();
    assert_eq!(env.lamports(reward_vault).await, reward_rent + 100 * SOL);

    let cfg = env.config().await;
    assert_eq!(cfg.reward_rate, 100 * SOL / 1_000);
    assert_eq!(cfg.total_reward_funded, 100 * SOL);

    // Alice buys 10 rigs. Capital and fee split exactly, reward vault untouched.
    let alice = env.alice.insecure_clone();
    env.buy(&alice, 10).await.unwrap();

    let capital = 10 * RIG_PRICE;
    let fee = capital * FEE_BPS as u64 / 10_000;
    assert_eq!(env.lamports(capital_vault).await, capital_rent + capital);
    assert_eq!(env.lamports(fee_vault).await, fee_rent + fee);
    assert_eq!(
        env.lamports(reward_vault).await,
        reward_rent + 100 * SOL,
        "spec §3: a purchase must not add reward backing"
    );

    // Accrue for half the window.
    env.warp(500).await;

    let before = env.lamports(alice.pubkey()).await;
    env.claim(&alice).await.unwrap();
    let after = env.lamports(alice.pubkey()).await;
    let received = after - before;

    // Sole miner for the whole elapsed window: ~rate * elapsed.
    assert!(received > 0, "claim paid nothing");
    let cfg = env.config().await;
    assert_eq!(cfg.total_reward_claimed, received);
    assert_eq!(
        env.lamports(reward_vault).await,
        reward_rent + 100 * SOL - received,
        "reward vault must drop by exactly the claimed amount"
    );

    let miner = env.miner(&alice.pubkey()).await;
    assert_eq!(miner.rigs, 10, "claim must not change rigs");
    assert_eq!(miner.pending_rewards, 0);
    assert_eq!(miner.lifetime_sol_claimed, received);

    // Fully-funded invariant holds on-chain.
    let outstanding =
        cfg.total_reward_funded - cfg.total_reward_claimed - cfg.total_reward_compounded;
    assert!(env.lamports(reward_vault).await - reward_rent >= outstanding);
}

#[tokio::test]
async fn e2e_compound_moves_reward_vault_into_capital_and_fee() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    env.fund_rewards(500 * SOL, 1_000).await.unwrap();

    let alice = env.alice.insecure_clone();
    env.buy(&alice, 5).await.unwrap();
    env.warp(400).await;

    let reward_before = env.lamports(reward_vault_pda()).await;
    let capital_before = env.lamports(capital_vault_pda()).await;
    let fee_before = env.lamports(fee_vault_pda()).await;
    let rigs_before = env.miner(&alice.pubkey()).await.rigs;

    env.compound(&alice).await.unwrap();

    let miner = env.miner(&alice.pubkey()).await;
    let minted = miner.rigs - rigs_before;
    assert!(minted > 0, "compound minted no rigs");

    let spent_capital = minted * RIG_PRICE;
    let spent_fee = spent_capital * FEE_BPS as u64 / 10_000;
    let spent = spent_capital + spent_fee;

    assert_eq!(
        env.lamports(reward_vault_pda()).await,
        reward_before - spent,
        "reward vault must drop by exactly the compounded amount"
    );
    assert_eq!(
        env.lamports(capital_vault_pda()).await,
        capital_before + spent_capital
    );
    assert_eq!(env.lamports(fee_vault_pda()).await, fee_before + spent_fee);

    // The remainder was preserved, not discarded.
    let cfg = env.config().await;
    assert_eq!(cfg.total_reward_compounded, spent);
    assert!(miner.pending_rewards < RIG_PRICE + RIG_PRICE * FEE_BPS as u64 / 10_000);
    assert_eq!(miner.lifetime_rigs_compounded, minted);
}

// ----------------------------------------------------------- Test 14 & 15

/// Test 14 — the authority cannot withdraw from the reward vault.
///
/// There is no instruction that pays the authority out of the reward vault.
/// The only admin money path is `withdraw_fees`, and pointing it at the reward
/// vault fails seed validation before any lamport moves.
#[tokio::test]
async fn t14_authority_cannot_withdraw_from_reward_vault() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    env.fund_rewards(50 * SOL, 1_000).await.unwrap();

    let balance_before = env.lamports(reward_vault_pda()).await;
    let auth = env.authority.insecure_clone();
    // The configured recipient, so the recipient constraint is satisfied and
    // the seed check is the only thing left that can reject this.
    let dest = env.fee_recipient;

    let ix = env.withdraw_fees_ix(&auth.pubkey(), reward_vault_pda(), dest, SOL);
    let err = env.send(ix, &[&auth]).await.unwrap_err();

    // Seed constraint on `fee_vault` rejects the reward vault PDA.
    let s = format!("{err:?}");
    assert!(
        s.contains("ConstraintSeeds") || s.contains("2006"),
        "expected a seeds-constraint failure, got: {s}"
    );
    assert_eq!(
        env.lamports(reward_vault_pda()).await,
        balance_before,
        "not one lamport may leave the reward vault"
    );
}

/// Test 15 — the authority cannot withdraw from the capital vault in V1.
#[tokio::test]
async fn t15_authority_cannot_withdraw_from_capital_vault() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    // Launch activation (alice = founder) requires a funded schedule.
    env.fund_rewards(SOL, 86_400).await.unwrap();

    let alice = env.alice.insecure_clone();
    env.buy(&alice, 100).await.unwrap();

    let balance_before = env.lamports(capital_vault_pda()).await;
    assert!(balance_before > 0);

    let auth = env.authority.insecure_clone();
    // As in t14: satisfy the recipient constraint so the seed check is the
    // only remaining reason this can fail.
    let dest = env.fee_recipient;
    let ix = env.withdraw_fees_ix(&auth.pubkey(), capital_vault_pda(), dest, 1);
    let err = env.send(ix, &[&auth]).await.unwrap_err();

    let s = format!("{err:?}");
    assert!(
        s.contains("ConstraintSeeds") || s.contains("2006"),
        "expected a seeds-constraint failure, got: {s}"
    );
    assert_eq!(env.lamports(capital_vault_pda()).await, balance_before);
}

/// The fee vault *is* withdrawable, and that path is auditable.
#[tokio::test]
async fn fee_vault_withdrawal_works_and_is_bounded() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    // Launch activation (alice = founder) requires a funded schedule.
    env.fund_rewards(SOL, 86_400).await.unwrap();

    let alice = env.alice.insecure_clone();
    env.buy(&alice, 100).await.unwrap();

    let fees = 100 * RIG_PRICE * FEE_BPS as u64 / 10_000;
    let dest = env.fee_recipient;
    let before = env.lamports(dest).await;
    let auth = env.authority.insecure_clone();

    // Over-withdrawing is rejected.
    let ix = env.withdraw_fees_ix(&auth.pubkey(), fee_vault_pda(), dest, fees + SOL);
    let err = env.send(ix, &[&auth]).await.unwrap_err();
    assert_rigs_error(err, RigsError::InsufficientFeeBalance);

    // The exact accrued amount goes through, and lands on the recipient.
    let ix = env.withdraw_fees_ix(&auth.pubkey(), fee_vault_pda(), dest, fees);
    env.send(ix, &[&auth]).await.unwrap();
    assert_eq!(env.lamports(dest).await, before + fees);
    assert_eq!(env.config().await.total_fees_withdrawn, fees);
}

// ------------------------------------------------------- fee recipient

/// The recipient is pinned in config, and it is exactly the address THE CHERRY
/// TREE is configured to be paid at.
#[tokio::test]
async fn fee_recipient_is_the_configured_address() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();

    let config = env.config().await;
    assert_eq!(
        config.fee_recipient.to_string(),
        FEE_RECIPIENT,
        "the configured fee recipient is not the Cherry Tree fee wallet"
    );
    // The recipient is a payout destination, not a role: it is none of the
    // three privileged identities.
    assert_ne!(config.fee_recipient, config.authority);
    assert_ne!(config.fee_recipient, config.reward_funder);
    assert_ne!(config.fee_recipient, config.pending_authority);
}

/// The authority signs the withdrawal but cannot choose where it lands.
#[tokio::test]
async fn authority_cannot_redirect_fees_away_from_the_recipient() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    // Launch activation (alice = founder) requires a funded schedule.
    env.fund_rewards(SOL, 86_400).await.unwrap();

    let alice = env.alice.insecure_clone();
    env.buy(&alice, 100).await.unwrap();

    let fees = 100 * RIG_PRICE * FEE_BPS as u64 / 10_000;
    let vault_before = env.lamports(fee_vault_pda()).await;
    let auth = env.authority.insecure_clone();

    // Paying itself is the obvious attempt.
    let ix = env.withdraw_fees_ix(&auth.pubkey(), fee_vault_pda(), auth.pubkey(), fees);
    let err = env.send(ix, &[&auth]).await.unwrap_err();
    assert_rigs_error(err, RigsError::FeeRecipientMismatch);

    // So is an unrelated account it controls.
    let attacker = Keypair::new().pubkey();
    let ix = env.withdraw_fees_ix(&auth.pubkey(), fee_vault_pda(), attacker, fees);
    let err = env.send(ix, &[&auth]).await.unwrap_err();
    assert_rigs_error(err, RigsError::FeeRecipientMismatch);

    assert_eq!(
        env.lamports(fee_vault_pda()).await,
        vault_before,
        "not one lamport may leave the fee vault to the wrong destination"
    );
}

/// A non-authority cannot withdraw the fee vault, even to the correct recipient.
#[tokio::test]
async fn non_authority_cannot_withdraw_fees() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    // Launch activation (alice = founder) requires a funded schedule.
    env.fund_rewards(SOL, 86_400).await.unwrap();

    let alice = env.alice.insecure_clone();
    env.buy(&alice, 100).await.unwrap();

    let fees = 100 * RIG_PRICE * FEE_BPS as u64 / 10_000;
    let vault_before = env.lamports(fee_vault_pda()).await;

    // Alice is an ordinary player; the reward funder is privileged elsewhere.
    for signer in [env.alice.insecure_clone(), env.funder.insecure_clone()] {
        let ix = env.withdraw_fees_ix(&signer.pubkey(), fee_vault_pda(), env.fee_recipient, fees);
        let err = env.send(ix, &[&signer]).await.unwrap_err();
        let s = format!("{err:?}");
        assert!(
            s.contains("ConstraintHasOne") || s.contains("2001"),
            "expected a has_one failure, got: {s}"
        );
    }

    assert_eq!(env.lamports(fee_vault_pda()).await, vault_before);
}

/// Changing the recipient is an authority action, and nobody else's.
#[tokio::test]
async fn only_the_authority_may_change_the_fee_recipient() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    // Launch activation (alice = founder) requires a funded schedule.
    env.fund_rewards(SOL, 86_400).await.unwrap();

    let replacement = Keypair::new().pubkey();

    // An ordinary player cannot.
    let alice = env.alice.insecure_clone();
    let ix = env.set_fee_recipient_ix(&alice.pubkey(), replacement);
    let err = env.send(ix, &[&alice]).await.unwrap_err();
    let s = format!("{err:?}");
    assert!(
        s.contains("ConstraintHasOne") || s.contains("2001"),
        "expected a has_one failure, got: {s}"
    );
    assert_eq!(env.config().await.fee_recipient.to_string(), FEE_RECIPIENT);

    // The authority can, and the change takes effect.
    let auth = env.authority.insecure_clone();
    let ix = env.set_fee_recipient_ix(&auth.pubkey(), replacement);
    env.send(ix, &[&auth]).await.unwrap();
    assert_eq!(env.config().await.fee_recipient, replacement);

    // And the withdrawal destination follows it, rather than the old address.
    let alice2 = env.alice.insecure_clone();
    env.buy(&alice2, 100).await.unwrap();
    let fees = 100 * RIG_PRICE * FEE_BPS as u64 / 10_000;

    let stale = env.fee_recipient;
    let ix = env.withdraw_fees_ix(&auth.pubkey(), fee_vault_pda(), stale, fees);
    let err = env.send(ix, &[&auth]).await.unwrap_err();
    assert_rigs_error(err, RigsError::FeeRecipientMismatch);

    let ix = env.withdraw_fees_ix(&auth.pubkey(), fee_vault_pda(), replacement, fees);
    env.send(ix, &[&auth]).await.unwrap();
    assert_eq!(env.lamports(replacement).await, fees);
}

/// The recipient cannot be unset, which would turn every withdrawal into a burn.
#[tokio::test]
async fn fee_recipient_cannot_be_the_default_pubkey() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();

    let auth = env.authority.insecure_clone();
    let ix = env.set_fee_recipient_ix(&auth.pubkey(), Pubkey::default());
    let err = env.send(ix, &[&auth]).await.unwrap_err();
    assert_rigs_error(err, RigsError::InvalidConfig);

    assert_eq!(env.config().await.fee_recipient.to_string(), FEE_RECIPIENT);
}

/// Fees are separate money from rewards and from capital, all the way through
/// a withdrawal. Paying the recipient must not touch either other vault.
#[tokio::test]
async fn withdrawing_fees_leaves_the_reward_and_capital_vaults_untouched() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    env.fund_rewards(50 * SOL, 1_000).await.unwrap();

    let alice = env.alice.insecure_clone();
    env.buy(&alice, 100).await.unwrap();

    let reward_before = env.lamports(reward_vault_pda()).await;
    let capital_before = env.lamports(capital_vault_pda()).await;

    let fees = 100 * RIG_PRICE * FEE_BPS as u64 / 10_000;
    let auth = env.authority.insecure_clone();
    let ix = env.withdraw_fees_ix(&auth.pubkey(), fee_vault_pda(), env.fee_recipient, fees);
    env.send(ix, &[&auth]).await.unwrap();

    assert_eq!(
        env.lamports(reward_vault_pda()).await,
        reward_before,
        "a fee withdrawal must not touch the reward vault"
    );
    assert_eq!(
        env.lamports(capital_vault_pda()).await,
        capital_before,
        "a fee withdrawal must not touch the capital vault"
    );

    // And the disclosed fee is exactly what the quote said it would be.
    let config = env.config().await;
    assert_eq!(config.total_protocol_fees, fees);
    assert_eq!(config.total_fees_withdrawn, fees);
}

// ----------------------------------------------------------------- Test 16

/// Test 16 — a non-`reward_funder` cannot call `fund_rewards`.
#[tokio::test]
async fn t16_only_reward_funder_may_fund() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();

    // Even the authority is not the funder.
    let auth = env.authority.insecure_clone();
    let ix = env.fund_rewards_ix(&auth.pubkey(), SOL, 100);
    let err = env.send(ix, &[&auth]).await.unwrap_err();
    assert_rigs_error(err, RigsError::UnauthorizedFunder);

    let alice = env.alice.insecure_clone();
    let ix = env.fund_rewards_ix(&alice.pubkey(), SOL, 100);
    let err = env.send(ix, &[&alice]).await.unwrap_err();
    assert_rigs_error(err, RigsError::UnauthorizedFunder);

    assert_eq!(env.config().await.total_reward_funded, 0);

    // The real funder succeeds.
    env.fund_rewards(SOL, 100).await.unwrap();
    assert_eq!(env.config().await.total_reward_funded, SOL);
}

// ----------------------------------------------------------------- Test 17

/// Test 17 — `paused` blocks buy / claim / compound.
#[tokio::test]
async fn t17_paused_blocks_user_instructions() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    env.fund_rewards(100 * SOL, 1_000).await.unwrap();

    let alice = env.alice.insecure_clone();
    env.buy(&alice, 10).await.unwrap();
    env.warp(200).await;

    env.set_paused(true).await.unwrap();

    let err = env.buy(&alice, 1).await.unwrap_err();
    assert_rigs_error(err, RigsError::ProgramPaused);
    let err = env.claim(&alice).await.unwrap_err();
    assert_rigs_error(err, RigsError::ProgramPaused);
    let err = env.compound(&alice).await.unwrap_err();
    assert_rigs_error(err, RigsError::ProgramPaused);

    // Funding stays open while paused so backing can still be topped up.
    env.fund_rewards(SOL, 1_000).await.unwrap();

    env.set_paused(false).await.unwrap();
    env.claim(&alice).await.unwrap();
}

// ----------------------------------------------------------------- Test 18

/// Test 18 — a wrong-owner miner position is rejected, and PDA seeds are
/// validated on every instruction.
#[tokio::test]
async fn t18_wrong_owner_miner_position_rejected() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    env.fund_rewards(100 * SOL, 1_000).await.unwrap();

    let alice = env.alice.insecure_clone();
    let bob = env.bob.insecure_clone();
    env.buy(&alice, 10).await.unwrap();
    env.buy(&bob, 1).await.unwrap();
    env.warp(300).await;

    let alice_miner = miner_pda(&alice.pubkey());
    let alice_balance_before = env.lamports(alice.pubkey()).await;

    // Bob signs, but points at Alice's miner PDA.
    let ix = env.claim_ix(&bob.pubkey(), alice_miner);
    let err = env.send(ix, &[&bob]).await.unwrap_err();
    let s = format!("{err:?}");
    assert!(
        s.contains("ConstraintSeeds")
            || s.contains("2006")
            || s.contains(&format!(
                "Custom({})",
                anchor_lang::error::ERROR_CODE_OFFSET + RigsError::MinerOwnerMismatch as u32
            )),
        "expected a seeds or owner-mismatch failure, got: {s}"
    );

    // Alice's position is intact and still claimable by Alice.
    assert_eq!(env.miner(&alice.pubkey()).await.rigs, 10);
    assert_eq!(env.lamports(alice.pubkey()).await, alice_balance_before);
    env.claim(&alice).await.unwrap();
}

/// Seeds are validated even when a caller substitutes a structurally valid but
/// wrong vault PDA.
#[tokio::test]
async fn t18b_vault_substitution_rejected() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();

    let alice = env.alice.insecure_clone();
    let mut ix = env.buy_ix(&alice.pubkey(), 1);
    // Swap the capital vault for the fee vault.
    ix.accounts[3].pubkey = fee_vault_pda();
    let err = env.send(ix, &[&alice]).await.unwrap_err();
    let s = format!("{err:?}");
    assert!(
        s.contains("ConstraintSeeds") || s.contains("2006"),
        "expected a seeds-constraint failure, got: {s}"
    );
}

/// An arbitrary account cannot pose as the config PDA.
#[tokio::test]
async fn t18c_config_substitution_rejected() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();

    let alice = env.alice.insecure_clone();
    let mut ix = env.buy_ix(&alice.pubkey(), 1);
    ix.accounts[1].pubkey = Pubkey::new_unique();
    let err = env.send(ix, &[&alice]).await.unwrap_err();
    let s = format!("{err:?}");
    assert!(
        s.contains("ConstraintSeeds")
            || s.contains("AccountNotInitialized")
            || s.contains("2006")
            || s.contains("3012"),
        "expected config validation to fail, got: {s}"
    );
}

// --------------------------------------------------------- misc guardrails

/// A second `initialize` cannot re-open the program.
#[tokio::test]
async fn reinitialize_is_rejected() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    // New slot so the retry is a distinct transaction rather than a replay.
    env.warp(1).await;
    let err = env.initialize().await.unwrap_err();
    let s = format!("{err:?}");
    assert!(
        s.contains("already in use") || s.contains("Custom(0)"),
        "expected re-init to fail, got: {s}"
    );
}

/// Claiming with an empty ore bin is an explicit error, not a silent no-op.
#[tokio::test]
async fn claim_with_nothing_earned_errors() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    // Launch activation (alice = founder) requires a funded schedule.
    env.fund_rewards(SOL, 86_400).await.unwrap();

    let alice = env.alice.insecure_clone();
    env.buy(&alice, 1).await.unwrap();
    let err = env.claim(&alice).await.unwrap_err();
    assert_rigs_error(err, RigsError::NothingToClaim);
}

/// Test 1 on-chain — a rig bought after emission has accrued earns nothing
/// retroactively.
#[tokio::test]
async fn t01_onchain_new_rig_earns_nothing_retroactively() {
    let mut env = Env::new().await;
    env.initialize().await.unwrap();
    env.fund_rewards(100 * SOL, 1_000).await.unwrap();

    let alice = env.alice.insecure_clone();
    let bob = env.bob.insecure_clone();

    env.buy(&alice, 10).await.unwrap();
    env.warp(500).await;
    env.buy(&bob, 10).await.unwrap();

    // Bob's watermark was set to the live accumulator before his rigs landed.
    let bob_miner = env.miner(&bob.pubkey()).await;
    assert_eq!(bob_miner.pending_rewards, 0);

    let cfg = env.config().await;
    assert_eq!(bob_miner.reward_per_rig_paid, cfg.reward_per_rig_stored);

    let err = env.claim(&bob).await.unwrap_err();
    assert_rigs_error(err, RigsError::NothingToClaim);
}
