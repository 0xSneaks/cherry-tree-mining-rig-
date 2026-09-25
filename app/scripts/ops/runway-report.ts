/**
 * RUNWAY REPORT — read-only. Sends nothing, signs nothing, holds no key.
 *
 *   CHERRY_TREE_RPC_URL=<rpc> npx tsx scripts/ops/runway-report.ts
 *   CHERRY_TREE_RPC_URL=<rpc> npx tsx scripts/ops/runway-report.ts --preview <sol> <days>
 *   CHERRY_TREE_RPC_URL=<rpc> npx tsx scripts/ops/runway-report.ts --history [limit]
 *
 * Prints runway, Dormant status, backing, and (optionally) the effect a
 * proposed `fund_rewards` would have on the live rate BEFORE anyone funds —
 * a small top-up over a long duration lowers the rate. It never calls
 * `fund_rewards`; funding is the separate, deliberate step in
 * docs/backlog/REWARD_RUNWAY_OPS.md.
 */
import { createSolanaRpc } from '@solana/kit';

import { clusterFromGenesis } from '../../src/lib/launch/expected';
import { refillHistory, runwayReport, DEFAULT_RUNWAY_THRESHOLDS, DAY } from '../../src/lib/growth/runway';
import { previewFunding } from '../../src/lib/model/rewardFunding';
import { eventHistory } from '../../src/lib/program/eventHistory';
import type { RewardsFundedEvent } from '../../src/lib/program/events';
import { programAddresses } from '../../src/lib/program/pdas';
import {
  chainTime,
  fail,
  fetchConfig,
  header,
  loadEnv,
  OpsError,
  requireEnv,
  row,
  section,
  sol,
  vaultDistributable,
  type Chain,
} from './common';

function parseSolToLamports(v: string): bigint {
  if (!/^\d+(\.\d{1,9})?$/.test(v)) fail(`'${v}' is not a SOL amount`);
  const [w, f = ''] = v.split('.');
  return BigInt(w!) * 1_000_000_000n + BigInt(f.padEnd(9, '0'));
}

async function main() {
  loadEnv();
  header('THE CHERRY TREE — reward runway report (read-only)');
  const rpcUrl = requireEnv('CHERRY_TREE_RPC_URL', 'The RPC to read from.');
  const rpc = createSolanaRpc(rpcUrl);
  const chain = { rpc, rpcUrl } as unknown as Chain;
  row('cluster (from genesis hash)', clusterFromGenesis(await rpc.getGenesisHash().send()));

  const config = await fetchConfig(chain);
  if (!config) fail('Config not initialised on this cluster');
  const { rewardVault } = await programAddresses();
  const [distributable, now] = await Promise.all([vaultDistributable(chain, rewardVault), chainTime(chain)]);
  const days = (k: string, d: bigint) => {
    const v = process.env[k];
    return v && /^\d+$/.test(v) ? BigInt(v) * DAY : d;
  };
  const r = runwayReport(config, distributable, now, {
    lowSeconds: days('CHERRY_TREE_RUNWAY_LOW_DAYS', DEFAULT_RUNWAY_THRESHOLDS.lowSeconds),
    criticalSeconds: days('CHERRY_TREE_RUNWAY_CRITICAL_DAYS', DEFAULT_RUNWAY_THRESHOLDS.criticalSeconds),
  });

  section('status');
  row('status', r.status);
  row('ALERT', r.alert);
  row('chain time', now);
  row('funded until', `${r.periodFinish} (${new Date(Number(r.periodFinish) * 1000).toISOString()})`);
  row('runway', `${r.runwaySeconds / DAY} d ${(r.runwaySeconds % DAY) / 3_600n} h`);
  row('emission / day (SOL)', sol(r.emissionPerDayLamports));
  row('future scheduled emission (SOL)', sol(r.futureEmissionLamports));
  section('ledger');
  row('total funded (SOL)', sol(r.totalFundedLamports));
  row('total claimed (SOL)', sol(r.totalClaimedLamports));
  row('total replanted (SOL)', sol(r.totalCompoundedLamports));
  row('funded − claimed − replanted', sol(r.ledgerRemainingLamports));
  row('vault distributable (SOL)', sol(r.distributableLamports));
  row('backing accrued claims (SOL)', sol(r.accruedBackingLamports));
  row('BACKED', r.backed ? 'yes' : 'NO — STOP, escalate (docs/backlog/REWARD_RUNWAY_OPS.md §Emergency)');

  const pi = process.argv.indexOf('--preview');
  if (pi > 0) {
    const amount = parseSolToLamports(process.argv[pi + 1] ?? '');
    const dur = process.argv[pi + 2] ?? '';
    if (!/^[1-9]\d*$/.test(dur)) fail('--preview <sol> <days>: days must be a positive integer');
    const p = previewFunding(
      { rate: config.rewardRate, finish: config.rewardPeriodFinish },
      now,
      amount,
      BigInt(dur) * DAY,
    );
    section('funding preview (nothing is sent)');
    row('leftover rolled in (SOL)', sol(p.leftoverLamports));
    row('new rate (lamports/s)', p.next.rate);
    row('new emission / day (SOL)', sol(p.next.rate * DAY));
    row('new funded until', new Date(Number(p.next.finish) * 1000).toISOString());
    row('rate change', p.rateChangeBps === null ? 'n/a (Dormant → new schedule)' : `${(p.rateChangeBps / 100).toFixed(2)}%`);
    if (p.lowersRate) row('WARNING', 'this funding LOWERS the current rate — announce it or choose a shorter duration');
  }

  const hi = process.argv.indexOf('--history');
  if (hi > 0) {
    const limit = Number(process.argv[hi + 1] ?? '200') || 200;
    const rows = await eventHistory(rpc, rewardVault, { limit, names: ['RewardsFunded'] });
    const h = refillHistory(rows.map((x) => ({ ...x, event: x.event as RewardsFundedEvent })));
    section(`refill history (${h.length})`);
    for (const e of h) {
      row(
        e.blockTime === null ? '?' : new Date(Number(e.blockTime) * 1000).toISOString(),
        `${sol(e.amountLamports)} SOL / ${e.durationSeconds / DAY} d by ${e.funder}${e.afterDormancy ? ' [restart after Dormant]' : ''} ${e.signature}`,
      );
    }
  }
}

main().catch((e) => {
  console.error(e instanceof OpsError ? `\n!! ${e.message}\n` : e);
  process.exitCode = 1;
});
