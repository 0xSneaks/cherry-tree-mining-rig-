/**
 * FIRST-BUY PREFLIGHT — read-only. Sends nothing, signs nothing, holds no key.
 *
 *   CHERRY_TREE_RPC_URL=<mainnet RPC> \
 *   CHERRY_TREE_INTENDED_CLUSTER=mainnet-beta \
 *   CHERRY_TREE_FIRST_BUY_SEEDS=<explicit count> \
 *   npx tsx scripts/ops/first-buy-preflight.ts
 *
 * Unlike the signing ops scripts (devnet-only by design), this may read
 * mainnet: reading is how the founder learns whether the one live
 * transaction is safe. The activation itself is signed by the founder's own
 * Seed Vault wallet in the app, after re-running this same preflight.
 *
 * Exit 0 only if every check passes. Passing does not submit anything.
 */
import { address, createSolanaRpc } from '@solana/kit';

import { gatherPreflight } from '../../src/lib/launch/gather';
import { EXPECTED_FOUNDER, GENESIS_HASH, type Cluster } from '../../src/lib/launch/expected';
import { confirmationPhrase, evaluatePreflight } from '../../src/lib/launch/preflight';
import { fail, header, loadEnv, OpsError, requireEnv, row, section } from './common';

async function main() {
  loadEnv();
  header('THE CHERRY TREE — first-buy preflight (read-only)');

  const rpcUrl = requireEnv('CHERRY_TREE_RPC_URL', 'The RPC to read from.');
  const intended = requireEnv(
    'CHERRY_TREE_INTENDED_CLUSTER',
    'State the cluster you intend: mainnet-beta for the live activation, devnet for a rehearsal.',
  );
  if (!(intended in GENESIS_HASH)) fail(`Unknown cluster '${intended}'.`);
  const seedsRaw = requireEnv(
    'CHERRY_TREE_FIRST_BUY_SEEDS',
    'The founder supplies the amount explicitly. There is no default.',
  );
  if (!/^[1-9][0-9]*$/.test(seedsRaw)) fail(`CHERRY_TREE_FIRST_BUY_SEEDS must be a positive integer.`);
  const seeds = BigInt(seedsRaw);
  const founder = address(process.env.CHERRY_TREE_FOUNDER ?? EXPECTED_FOUNDER);

  const rpc = createSolanaRpc(rpcUrl);
  const { input, founderMiner } = await gatherPreflight(rpc, founder, seeds, intended as Cluster);
  const r = evaluatePreflight(input, founderMiner);

  section('checks');
  for (const c of r.checks) row(`${c.pass ? 'PASS' : 'FAIL'}  ${c.name}`, c.detail);

  section('the one transaction');
  row('founder', founder);
  row('seeds', seeds);
  if (r.quote) {
    row('capital (lamports)', r.quote.capitalLamports);
    row('purchase fee (lamports)', r.quote.feeLamports);
    row('purchase total (lamports)', r.quote.totalLamports);
  }
  row('all-in cost (lamports)', r.totalCostLamports ?? '—');
  row('APR now (bps)', r.aprBpsNow ?? '—');
  row('APR after planting (bps)', r.aprBpsAfter ?? '—');
  if (input.simulation) {
    section('simulation logs');
    for (const l of input.simulation.logs) console.log(`  ${l}`);
  }

  section('result');
  if (r.ok) {
    row('PREFLIGHT', 'PASS');
    console.log(
      `\n  Nothing was sent. To activate, the founder opens the app with the\n` +
        `  founder wallet, enters ${seeds} seeds, re-runs the preflight there and\n` +
        `  types "${confirmationPhrase(seeds)}". The app simulates once more, then\n` +
        `  submits ONE transaction.\n`,
    );
  } else {
    row('PREFLIGHT', 'FAIL — do not activate');
    process.exitCode = 1;
  }
}

main().catch((e) => {
  console.error(e instanceof OpsError ? `\n!! ${e.message}\n` : e);
  process.exitCode = 1;
});
