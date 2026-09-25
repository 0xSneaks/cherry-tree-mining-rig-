/**
 * `GET /api/referrals[?season=<id>][&wallet=<address>]` — public referral
 * standings, derived from permanent Miner state (referrer + createdAt).
 *
 * Status only: no referrer payout exists and none is implied. Qualification
 * thresholds and seasons are deployment configuration; unconfigured, the
 * leaderboard is closed rather than ranking raw wallet creation.
 */
import { isAddress } from '@solana/kit';

import {
  myReferrals,
  referralLeaderboard,
  seasonLeaderboard,
  type ReferredPosition,
} from '../../../lib/growth/referralStats';
import { currentSeason, pastSeasons, seasonState } from '../../../lib/growth/seasons';
import { excludedReferrers, referralQualification, seasons } from '../../../lib/server/growthConfig';
import { allMinerPositions, chainNow } from '../../../lib/server/indexer';
import { json } from '../../../lib/server/launchToken';
import { programAddresses } from '../../../lib/program/pdas';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const NO_REFERRER = '11111111111111111111111111111111';

export async function GET(req: Request): Promise<Response> {
  const url = new URL(req.url);
  const seasonId = url.searchParams.get('season');
  const wallet = url.searchParams.get('wallet');
  if (wallet !== null && !isAddress(wallet)) return json({ error: 'wallet is not an address' }, 400);
  if (seasonId !== null && !/^[a-z0-9-]{1,32}$/.test(seasonId)) return json({ error: 'bad season id' }, 400);

  try {
    const pdas = await programAddresses();
    const { seasons: list } = seasons([pdas.rewardVault, pdas.capitalVault, pdas.feeVault, pdas.config]);
    const [positions, now] = await Promise.all([allMinerPositions(), chainNow()]);
    const referred: ReferredPosition[] = positions
      .filter((m) => m.referrer !== NO_REFERRER)
      .map((m) => ({
        owner: m.owner,
        referrer: m.referrer,
        createdAt: m.createdAt,
        rigs: m.rigs,
        lifetimeRigsBought: m.lifetimeRigsBought,
        lifetimeRigsCompounded: m.lifetimeRigsCompounded,
      }));
    const qualification = referralQualification();
    const excluded = excludedReferrers();

    let board;
    let season = null;
    if (seasonId) {
      const s = list.find((x) => x.id === seasonId);
      if (!s) return json({ error: 'unknown season' }, 404);
      season = s;
      board = seasonLeaderboard(referred, s, now, { qualification, excluded }, 25);
    } else {
      board = referralLeaderboard(referred, { qualification, at: now, excluded }, 25);
    }

    const ser = (v: unknown) => JSON.parse(JSON.stringify(v, (_, x) => (typeof x === 'bigint' ? x.toString() : x)));
    const seasonView = (s: (typeof list)[number]) => ({
      id: s.id,
      name: s.name,
      startsAt: s.startsAt.toString(),
      endsAt: s.endsAt.toString(),
      state: seasonState(s, now),
      competitions: s.competitions,
      prizes: s.prizes,
    });
    return json(
      {
        chainTime: now.toString(),
        qualification: qualification
          ? { minFreshSeeds: qualification.minFreshSeeds.toString(), minHoldHours: (qualification.minHoldSeconds / 3_600n).toString() }
          : null,
        season: season ? seasonView(season) : null,
        current: (() => {
          const c = currentSeason(list, now);
          return c ? seasonView(c) : null;
        })(),
        past: pastSeasons(list, now).map(seasonView),
        leaderboard: ser(board),
        me: wallet ? ser(myReferrals(referred, wallet, { qualification, at: now })) : null,
        note: 'Referral standings are status only. Referrers receive no payout from the program; any season prize is separately funded and disclosed.',
      },
      200,
      { 'cache-control': 'public, max-age=30, stale-while-revalidate=60' },
    );
  } catch (e) {
    return json({ error: e instanceof Error ? e.message : 'unavailable' }, 503);
  }
}
