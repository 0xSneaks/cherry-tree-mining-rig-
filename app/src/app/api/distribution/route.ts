/**
 * `GET /api/distribution` — public concentration analytics over every Miner
 * position. Chain-derived, cached, display only. Figures are PER WALLET: the
 * response carries the caveat that one person can hold many wallets.
 */
import { distribution, PER_WALLET_CAVEAT, CONCENTRATION_COPY } from '../../../lib/growth/concentration';
import { allMinerPositions } from '../../../lib/server/indexer';
import { json } from '../../../lib/server/launchToken';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(): Promise<Response> {
  try {
    const d = distribution(
      (await allMinerPositions()).map((m) => ({ owner: m.owner, rigs: m.rigs, weight: m.weight })),
    );
    return json(
      {
        wallets: d.wallets,
        totalSeeds: d.totalRigs.toString(),
        totalShares: d.totalShares.toString(),
        largestBps: d.largestBps,
        top5Bps: d.top5Bps,
        top10Bps: d.top10Bps,
        top1PctBps: d.top1PctBps,
        hhi: d.hhi,
        giniBps: d.giniBps,
        medianSeeds: d.medianRigs.toString(),
        level: d.level,
        summary: CONCENTRATION_COPY[d.level],
        whales: d.whales.map((w) => ({ owner: w.owner, seeds: w.rigs.toString(), shareBps: w.shareBps })),
        caveat: PER_WALLET_CAVEAT,
      },
      200,
      { 'cache-control': 'public, max-age=30, stale-while-revalidate=60' },
    );
  } catch (e) {
    return json({ error: e instanceof Error ? e.message : 'unavailable' }, 503);
  }
}
