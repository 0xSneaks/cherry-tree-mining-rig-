/**
 * `GET /api/runway` — public reward runway, Dormant status and refill
 * history. Chain-derived: Config, the reward vault, and `RewardsFunded`
 * events. Nothing here funds, schedules or promises a refill.
 */
import { refillHistory, runwayReport, STATUS_COPY } from '../../../lib/growth/runway';
import { eventHistory } from '../../../lib/program/eventHistory';
import type { RewardsFundedEvent } from '../../../lib/program/events';
import { programAddresses } from '../../../lib/program/pdas';
import { runwayThresholds } from '../../../lib/server/growthConfig';
import { chainNow, globalStats, serverRpc } from '../../../lib/server/indexer';
import { json } from '../../../lib/server/launchToken';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const HISTORY_TTL_MS = 5 * 60_000;
let historyCache: { at: number; rows: ReturnType<typeof refillHistory> } | null = null;

export async function GET(): Promise<Response> {
  try {
    const [{ config, rewardVaultLamports }, now, pdas] = await Promise.all([globalStats(), chainNow(), programAddresses()]);
    const rent = await serverRpc().getMinimumBalanceForRentExemption(BigInt(8 + 1 + 1)).send();
    const distributable = rewardVaultLamports > BigInt(rent) ? rewardVaultLamports - BigInt(rent) : 0n;
    const r = runwayReport(config, distributable, now, runwayThresholds());

    if (!historyCache || Date.now() - historyCache.at > HISTORY_TTL_MS) {
      try {
        const rows = await eventHistory(serverRpc(), pdas.rewardVault, { limit: 100, names: ['RewardsFunded'] });
        historyCache = {
          at: Date.now(),
          rows: refillHistory(rows.map((x) => ({ ...x, event: x.event as RewardsFundedEvent }))),
        };
      } catch {
        // History is supplementary; the runway itself never depends on it.
      }
    }
    const ser = (v: unknown) => JSON.parse(JSON.stringify(v, (_, x) => (typeof x === 'bigint' ? x.toString() : x)));
    return json(
      {
        chainTime: now.toString(),
        report: ser(r),
        copy: STATUS_COPY[r.status],
        refills: historyCache ? ser(historyCache.rows) : null,
        note: 'Rewards are pre-funded by deliberate external deposits. Purchases and purchase fees never fund rewards, and there is no automatic refill.',
      },
      200,
      { 'cache-control': 'public, max-age=30, stale-while-revalidate=60' },
    );
  } catch (e) {
    return json({ error: e instanceof Error ? e.message : 'unavailable' }, 503);
  }
}
