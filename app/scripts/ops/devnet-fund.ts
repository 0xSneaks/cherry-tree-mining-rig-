/**
 * THE CHERRY TREE — devnet reward funding.
 *
 * DEVNET ONLY. Calls `fund_rewards(amount, duration)` explicitly. Nothing
 * else in this repository funds rewards: deploy does not, and initialize
 * does not. Funding is always a deliberate act.
 *
 *   ./scripts/devnet-fund.sh <amountSol> <durationSeconds>
 */
import {
  assertConnectedToDevnet,
  chainTime,
  connect,
  fail,
  fetchConfig,
  header,
  loadEnv,
  loadSigner,
  lamportsOf,
  requireDevnet,
  requireEnv,
  row,
  section,
  sol,
  submit,
  vaultDistributable,
} from './common';

import { programAddresses } from '../../src/lib/program/pdas';
import { LAMPORTS_PER_SOL, PROGRAM_ID } from '../../src/lib/program/constants';
import { fundRewardsInstruction } from './admin-instructions';

/** Devnet sanity ceiling. Funding more than this is almost certainly a typo. */
const MAX_DEVNET_FUNDING_SOL = 100n;

function parseArgs(): { amountLamports: bigint; durationSeconds: bigint } {
  const [amountArg, durationArg] = process.argv.slice(2);
  if (!amountArg || !durationArg) {
    fail('usage: devnet-fund.sh <amountSol> <durationSeconds>   e.g. 0.5 3600');
  }

  if (!/^\d+(\.\d{1,9})?$/.test(amountArg)) {
    fail(`amountSol must be a plain decimal with at most 9 places, got '${amountArg}'`);
  }
  // Parsed as integer lamports via string arithmetic — no float in the money path.
  const [whole, frac = ''] = amountArg.split('.');
  const amountLamports =
    BigInt(whole ?? '0') * LAMPORTS_PER_SOL + BigInt(frac.padEnd(9, '0'));

  if (amountLamports <= 0n) fail('amountSol must be greater than zero');
  if (amountLamports > MAX_DEVNET_FUNDING_SOL * LAMPORTS_PER_SOL) {
    fail(`refusing to fund more than ${MAX_DEVNET_FUNDING_SOL} SOL on devnet`);
  }

  if (!/^\d+$/.test(durationArg)) fail('durationSeconds must be a whole number of seconds');
  const durationSeconds = BigInt(durationArg);
  if (durationSeconds <= 0n) fail('durationSeconds must be greater than zero');
  // Matches rigs_engine::MAX_REWARD_DURATION_SECONDS.
  if (durationSeconds > 10n * 365n * 24n * 60n * 60n) {
    fail('durationSeconds exceeds the program maximum of 10 years');
  }

  return { amountLamports, durationSeconds };
}

async function main(): Promise<void> {
  loadEnv();
  header('THE CHERRY TREE — devnet fund_rewards');

  section('cluster guard');
  const { cluster, rpcUrl } = requireDevnet();
  row('cluster', cluster);
  row('rpc', rpcUrl);

  const { amountLamports, durationSeconds } = parseArgs();

  const funderKeypairPath = requireEnv(
    'CHERRY_TREE_REWARD_FUNDER_KEYPAIR',
    "Set it to the reward funder's keypair JSON path (kept outside this repo).",
  );
  const funder = await loadSigner(funderKeypairPath, 'reward funder');

  const pdas = await programAddresses();
  const chain = connect(rpcUrl);
  await assertConnectedToDevnet(chain);

  const before = await fetchConfig(chain);
  if (!before) fail('the program is not initialised on this cluster. Run devnet-init first.');

  // The program enforces this too; failing here gives a readable message
  // instead of an opaque custom error code.
  if (before.rewardFunder !== funder.address) {
    fail(
      `this keypair is not the authorized reward funder.\n` +
        `       config.reward_funder = ${before.rewardFunder}\n` +
        `       supplied key         = ${funder.address}`,
    );
  }

  const rateAfter = 'computed by the program from amount + leftover';
  const now = await chainTime(chain);

  section('about to fund');
  row('program ID', PROGRAM_ID);
  row('reward funder', funder.address);
  row('funder balance', `${sol(await lamportsOf(chain, funder.address))} SOL`);
  row('funding amount', `${amountLamports} lamports (${sol(amountLamports)} SOL)`);
  row('duration', `${durationSeconds} s`);
  console.log();
  row('REWARD VAULT PDA', pdas.rewardVault);
  row('reward vault balance', `${sol(await vaultDistributable(chain, pdas.rewardVault))} SOL`);
  row('current reward rate', `${before.rewardRate} lamports/s`);
  row('current period finish', `${before.rewardPeriodFinish} (chain now ${now})`);
  row('total reward funded', `${sol(before.totalRewardFunded)} SOL`);
  row('new rate', rateAfter);

  if (await lamportsOf(chain, funder.address) < amountLamports) {
    fail('the reward funder does not hold enough devnet SOL for this funding amount.');
  }

  section('sending fund_rewards');
  const signature = await submit(
    chain,
    funder,
    fundRewardsInstruction(funder.address, pdas, amountLamports, durationSeconds),
  );

  section('confirmed — reading state back from chain');
  const after = await fetchConfig(chain);
  if (!after) fail('config disappeared after funding. Something is very wrong.');

  const vaultAfter = await vaultDistributable(chain, pdas.rewardVault);
  row('signature', signature);
  row('reward vault balance', `${vaultAfter} lamports (${sol(vaultAfter)} SOL)`);
  row('reward rate', `${after.rewardRate} lamports/s`);
  row('period finish', `${after.rewardPeriodFinish}`);
  row('total reward funded', `${after.totalRewardFunded} lamports (${sol(after.totalRewardFunded)} SOL)`);

  const fundedDelta = after.totalRewardFunded - before.totalRewardFunded;
  if (fundedDelta !== amountLamports) {
    fail(`total_reward_funded moved by ${fundedDelta}, expected ${amountLamports}`);
  }

  // The fully-funded invariant, read straight off the chain.
  const outstanding =
    after.totalRewardFunded - after.totalRewardClaimed - after.totalRewardCompounded;
  row('outstanding liabilities', `${outstanding} lamports`);
  if (vaultAfter < outstanding) {
    fail(
      `reward vault (${vaultAfter}) does not back outstanding liabilities (${outstanding}).`,
    );
  }
  console.log('\n  ok   reward vault distributable >= funded - claimed - compounded');

  console.log('\nFUNDED.');
}

main().catch((e) => {
  console.error(`\nFAIL: ${e instanceof Error ? e.message : String(e)}\n`);
  process.exitCode = 1;
});
