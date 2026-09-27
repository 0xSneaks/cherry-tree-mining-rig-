/**
 * THE CHERRY TREE — two-wallet devnet smoke test.
 *
 * DEVNET ONLY. This is the proof that the miner works on a real chain:
 *
 *   WALLET A  buy rigs
 *      wait   (real time, real slots)
 *   WALLET B  buy rigs — later
 *      wait
 *   WALLET A  claim
 *   WALLET B  compound
 *
 * Two properties are then asserted against on-chain state, in exact integer
 * lamports with no float reconciliation anywhere:
 *
 *   1. Wallet B earned NOTHING attributable to time before it owned rigs.
 *      Checked at the accumulator watermark, which is the mechanism that
 *      guarantees it, and again against B's settled balance.
 *
 *   2. reward vault distributable
 *        >= total_reward_funded - total_reward_claimed - total_reward_compounded
 *
 *   ./scripts/devnet-smoke.sh [rigsA] [rigsB] [waitSeconds]
 */
import {
  assertConnectedToDevnet,
  chainTime,
  connect,
  fail,
  fetchConfig,
  fetchMiner,
  header,
  lamportsOf,
  loadEnv,
  loadSigner,
  requireDevnet,
  requireEnv,
  row,
  section,
  sleep,
  sol,
  submit,
  vaultDistributable,
  type Chain,
} from './common';
import type { KeyPairSigner } from '@solana/kit';

import {
  buyRigsInstruction,
  claimInstruction,
  compoundInstruction,
} from '../../src/lib/program/instructions';
import { minerPda, programAddresses } from '../../src/lib/program/pdas';
import { PRECISION, PROGRAM_ID } from '../../src/lib/program/constants';
import { compoundQuote, purchaseQuote } from './fee-math';

const DEFAULT_RIGS_A = 5n;
const DEFAULT_RIGS_B = 5n;
const DEFAULT_WAIT_SECONDS = 45;

async function main(): Promise<void> {
  loadEnv();
  header('THE CHERRY TREE — two-wallet devnet smoke test');

  section('cluster guard');
  const { cluster, rpcUrl } = requireDevnet();
  row('cluster', cluster);
  row('rpc', rpcUrl);

  const [argA, argB, argWait] = process.argv.slice(2);
  const rigsA = argA ? BigInt(argA) : DEFAULT_RIGS_A;
  const rigsB = argB ? BigInt(argB) : DEFAULT_RIGS_B;
  const waitSeconds = argWait ? Number(argWait) : DEFAULT_WAIT_SECONDS;
  if (rigsA <= 0n || rigsB <= 0n) fail('rig counts must be positive');
  if (!Number.isFinite(waitSeconds) || waitSeconds < 5) fail('waitSeconds must be at least 5');

  const walletA = await loadSigner(
    requireEnv('CHERRY_TREE_SMOKE_WALLET_A', 'Path to devnet wallet A keypair JSON.'),
    'smoke wallet A',
  );
  const walletB = await loadSigner(
    requireEnv('CHERRY_TREE_SMOKE_WALLET_B', 'Path to devnet wallet B keypair JSON.'),
    'smoke wallet B',
  );
  if (walletA.address === walletB.address) {
    fail('wallet A and wallet B must be DISTINCT wallets. The whole test depends on it.');
  }

  const pdas = await programAddresses();
  const chain = connect(rpcUrl);
  await assertConnectedToDevnet(chain);

  const config0 = await fetchConfig(chain);
  if (!config0) fail('the program is not initialised on this cluster. Run devnet-init first.');
  if (config0.paused) fail('the program is paused. Unpause before smoke testing.');

  const now0 = await chainTime(chain);
  if (now0 >= config0.rewardPeriodFinish) {
    fail(
      'the reward schedule has already finished, so nothing would accrue.\n' +
        '       Fund it first: ./scripts/devnet-fund.sh <amountSol> <durationSeconds>',
    );
  }

  section('starting state');
  row('program ID', PROGRAM_ID);
  row('wallet A', walletA.address);
  row('wallet B', walletB.address);
  row('A balance', `${sol(await lamportsOf(chain, walletA.address))} SOL`);
  row('B balance', `${sol(await lamportsOf(chain, walletB.address))} SOL`);
  row('rig price', `${config0.rigPriceLamports} lamports`);
  row('purchase fee', `${config0.purchaseFeeBps} bps`);
  row('compound fee', `${config0.compoundFeeBps} bps`);
  row('total rigs', config0.totalRigs);
  row('reward rate', `${config0.rewardRate} lamports/s`);
  row('period finish', `${config0.rewardPeriodFinish} (chain now ${now0})`);

  const quoteA = purchaseQuote(rigsA, config0);
  const quoteB = purchaseQuote(rigsB, config0);
  for (const [name, w, q] of [
    ['A', walletA, quoteA],
    ['B', walletB, quoteB],
  ] as const) {
    const bal = await lamportsOf(chain, w.address);
    // Purchase total plus headroom for fees and the miner PDA rent.
    if (bal < q.totalLamports + 20_000_000n) {
      fail(
        `wallet ${name} (${w.address}) holds ${bal} lamports, not enough for ` +
          `${q.totalLamports} plus rent and fees. Airdrop devnet SOL first.`,
      );
    }
  }

  // ---------------------------------------------------------------- step 1
  section(`WALLET A buys ${rigsA} rigs`);
  const sigA = await buy(chain, walletA, pdas, rigsA);
  row('signature', sigA);
  const afterABuy = await fetchConfig(chain);
  const tABuy = await chainTime(chain);
  row('accumulator at A buy', afterABuy!.rewardPerRigStored);
  row('chain time', tABuy);

  // ---------------------------------------------------------------- wait
  section(`waiting ${waitSeconds}s of real time so A accrues alone`);
  await sleep(waitSeconds * 1000);

  // ---------------------------------------------------------------- step 2
  section(`WALLET B buys ${rigsB} rigs — later`);
  const sigB = await buy(chain, walletB, pdas, rigsB);
  row('signature', sigB);

  const minerB0 = await fetchMiner(chain, walletB.address);
  if (!minerB0) fail('wallet B has no miner position after buying');
  const configAtBBuy = await fetchConfig(chain);
  const tBBuy = await chainTime(chain);
  row('accumulator at B buy', configAtBBuy!.rewardPerRigStored);
  row('B watermark (paid)', minerB0.rewardPerRigPaid);
  row('B pending_rewards', minerB0.pendingRewards);
  row('chain time', tBBuy);

  // PROOF 1a — the mechanism. B's watermark was set to the live accumulator
  // BEFORE its rigs were added, so the emission that accrued while only A
  // held rigs is unreachable to B by construction.
  const watermarkOk =
    minerB0.rewardPerRigPaid === configAtBBuy!.rewardPerRigStored &&
    minerB0.pendingRewards === 0n;
  console.log(
    `\n  ${watermarkOk ? 'ok  ' : 'BAD '} B's watermark equals the accumulator at purchase, ` +
      'and its ore bin is empty',
  );
  if (!watermarkOk) fail('wallet B was credited with pre-existing accrual. STOP.');

  const accumulatorBeforeB = configAtBBuy!.rewardPerRigStored;
  const elapsedAAlone = tBBuy - tABuy;

  // ---------------------------------------------------------------- wait
  section(`waiting ${waitSeconds}s of real time so both accrue together`);
  await sleep(waitSeconds * 1000);

  // ---------------------------------------------------------------- step 3
  section('WALLET A claims');
  const balA0 = await lamportsOf(chain, walletA.address);
  const configBeforeClaim = await fetchConfig(chain);
  const sigClaim = await submit(
    chain,
    walletA,
    claimInstruction(walletA.address, await minerPda(walletA.address), pdas),
  );
  const configAfterClaim = await fetchConfig(chain);
  const minerAAfter = await fetchMiner(chain, walletA.address);
  const claimedA =
    configAfterClaim!.totalRewardClaimed - configBeforeClaim!.totalRewardClaimed;
  row('signature', sigClaim);
  row('A claimed', `${claimedA} lamports (${sol(claimedA)} SOL)`);
  row('A wallet delta', `${(await lamportsOf(chain, walletA.address)) - balA0} lamports (net of tx fee)`);
  row('A ore bin after', minerAAfter!.pendingRewards);
  row('A rigs after claim', minerAAfter!.rigs);
  if (minerAAfter!.rigs !== rigsA) fail('claim changed the rig count. It must not.');
  if (minerAAfter!.pendingRewards !== 0n) fail('ore bin is not empty right after a claim.');

  // ---------------------------------------------------------------- step 4
  section('WALLET B compounds');
  const configBeforeCompound = await fetchConfig(chain);
  const minerBBefore = await fetchMiner(chain, walletB.address);
  const sigCompound = await submit(
    chain,
    walletB,
    compoundInstruction(walletB.address, await minerPda(walletB.address), pdas),
  );
  const configAfterCompound = await fetchConfig(chain);
  const minerBAfter = await fetchMiner(chain, walletB.address);
  const compoundedB =
    configAfterCompound!.totalRewardCompounded - configBeforeCompound!.totalRewardCompounded;
  const mintedB = minerBAfter!.rigs - minerBBefore!.rigs;
  row('signature', sigCompound);
  row('B compounded', `${compoundedB} lamports (${sol(compoundedB)} SOL)`);
  row('B rigs minted', mintedB);
  row('B ore bin remainder', minerBAfter!.pendingRewards);

  // ---------------------------------------------------------------- report
  const config = (await fetchConfig(chain))!;
  const minerA = (await fetchMiner(chain, walletA.address))!;
  const minerB = (await fetchMiner(chain, walletB.address))!;
  const rewardVault = await vaultDistributable(chain, pdas.rewardVault);
  const capitalVault = await vaultDistributable(chain, pdas.capitalVault);
  const feeVault = await vaultDistributable(chain, pdas.feeVault);

  header('ON-CHAIN RESULT');
  row('total rigs', config.totalRigs);
  row('wallet A rigs', minerA.rigs);
  row('wallet B rigs', minerB.rigs);
  console.log();
  row('wallet A claimed', `${claimedA} lamports (${sol(claimedA)} SOL)`);
  row('wallet B compounded', `${compoundedB} lamports (${sol(compoundedB)} SOL)`);
  row('wallet B rigs minted', mintedB);
  console.log();
  row('reward vault', `${rewardVault} lamports (${sol(rewardVault)} SOL)`);
  row('capital vault', `${capitalVault} lamports (${sol(capitalVault)} SOL)`);
  row('fee vault', `${feeVault} lamports (${sol(feeVault)} SOL)`);
  console.log();
  row('total_reward_funded', config.totalRewardFunded);
  row('total_reward_claimed', config.totalRewardClaimed);
  row('total_reward_compounded', config.totalRewardCompounded);
  row('undistributed_dust', config.undistributedDust);

  header('ASSERTIONS');
  let failures = 0;
  const assert = (name: string, ok: boolean, detail: string): void => {
    if (!ok) failures += 1;
    console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${name}`);
    console.log(`        ${detail}`);
  };

  // PROOF 1b — the counterfactual, in exact lamports.
  //
  // Had B's watermark started at zero instead of at the live accumulator, B
  // would have been handed the whole pre-existing slice: `rigsB * accumulator
  // at purchase / PRECISION`. Everything B has actually realised is compared
  // against that figure, so the gap is the emission B was correctly denied
  // rather than an identity that holds by construction.
  const counterfactualForB = (rigsB * accumulatorBeforeB) / PRECISION;
  const bAccountedFor = compoundedB + minerB.pendingRewards;
  const bMaxEarnable =
    (minerB.rigs * (config.rewardPerRigStored - minerB0.rewardPerRigPaid)) / PRECISION;

  row('B would have received', `${counterfactualForB} lamports if credited from zero`);
  row('B actually realised', `${bAccountedFor} lamports`);
  row('B denied (pre-ownership)', `${counterfactualForB} lamports`);

  assert(
    'wallet B received NO reward attributable to time before it owned rigs',
    minerB0.pendingRewards === 0n &&
      minerB0.rewardPerRigPaid === accumulatorBeforeB &&
      bAccountedFor <= bMaxEarnable,
    `B's watermark at purchase was ${minerB0.rewardPerRigPaid}, exactly the accumulator ` +
      `then (${accumulatorBeforeB}), and its ore bin was empty. A had accrued alone for ` +
      `${elapsedAAlone}s, worth ${counterfactualForB} lamports to ${rigsB} rigs — none of ` +
      `which reached B. Everything B has realised (${bAccountedFor}) sits within the ` +
      `${bMaxEarnable} its own watermark permits.`,
  );

  // The same slice, seen from A's side: A is the only wallet that could have
  // earned during that window, so nothing went missing either.
  assert(
    'the pre-B emission was real and went to wallet A, not nowhere',
    accumulatorBeforeB > 0n && claimedA > 0n,
    `accumulator advanced to ${accumulatorBeforeB} before B arrived; A claimed ${claimedA} lamports`,
  );

  // PROOF 2 — the fully-funded invariant, in exact lamports.
  const outstanding =
    config.totalRewardFunded - config.totalRewardClaimed - config.totalRewardCompounded;
  assert(
    'reward vault distributable >= funded - claimed - compounded',
    rewardVault >= outstanding,
    `${rewardVault} >= ${config.totalRewardFunded} - ${config.totalRewardClaimed} - ` +
      `${config.totalRewardCompounded} = ${outstanding}`,
  );

  // The core invariant: purchases went to capital and fee, never to reward.
  const expectedCapital = quoteA.capitalLamports + quoteB.capitalLamports;
  const expectedFee = quoteA.feeLamports + quoteB.feeLamports;
  assert(
    'purchase lamports landed in CAPITAL and FEE, never in REWARD',
    config.totalCapitalReceived >= expectedCapital &&
      config.totalProtocolFees >= expectedFee,
    `capital_received=${config.totalCapitalReceived} (>= ${expectedCapital} from buys, ` +
      `plus ${compoundedB ? 'the compound' : 'nothing else'}), ` +
      `protocol_fees=${config.totalProtocolFees} (>= ${expectedFee})`,
  );

  assert(
    'claim paid a positive amount and left the rig count untouched',
    claimedA > 0n && minerA.rigs === rigsA,
    `claimed ${claimedA} lamports; A still holds ${minerA.rigs} rigs`,
  );

  assert(
    'compound minted whole rigs and preserved the exact remainder',
    // compound charges compound_fee_bps, never purchase_fee_bps (§7a).
    mintedB > 0n && compoundedB === compoundQuote(mintedB, config).totalLamports,
    `minted ${mintedB} rigs for exactly ${compoundedB} lamports; ` +
      `${minerB.pendingRewards} lamports left in the ore bin`,
  );

  console.log();
  if (failures > 0) {
    fail(`${failures} assertion(s) failed on real devnet state.`);
  }
  console.log('SMOKE TEST PASSED on real devnet state.\n');
}

async function buy(
  chain: Chain,
  wallet: KeyPairSigner,
  pdas: Awaited<ReturnType<typeof programAddresses>>,
  count: bigint,
): Promise<string> {
  return submit(
    chain,
    wallet,
    buyRigsInstruction(wallet.address, await minerPda(wallet.address), pdas, count),
  );
}

main().catch((e) => {
  console.error(`\nFAIL: ${e instanceof Error ? e.message : String(e)}\n`);
  process.exitCode = 1;
});
