/**
 * Referral analytics, qualified referrals, tiers and leaderboards — OFF-CHAIN.
 *
 * Source of truth: indexed Miner accounts. A position's `referrer` is written
 * once, at creation, and never changes (the same fact `ReferralBound` emits),
 * and `createdAt` is its binding time. So every figure here is recomputable
 * from chain state alone; there is no referral database to drift.
 *
 * What this module computes for the REFERRER: status only (count, badge
 * tier, leaderboard rank). The on-chain referral commission (architecture
 * §7a item 4: up to 20% of eligible fees, 8% treasury margin, one level) is
 * a separate program path read from the referrer's ledger PDA; this module
 * neither computes nor adds to it. Separately funded season prizes may
 * reference these standings; they never come from a protocol vault.
 *
 * Qualification (anti-farming): raw wallet creation never counts. A referral
 * qualifies only when the referred wallet has made meaningful fresh purchases
 * with its own SOL (`lifetimeRigsBought`, compounding excluded) and held for
 * a minimum time. Both thresholds are a founder decision; until they are
 * configured the qualified count and leaderboard stay closed (fail closed).
 * This does not prove distinct humans: one person can refer their own second
 * wallet. Qualification makes that cost real, non-redeemable SOL.
 */
import type { Competition, Season } from './seasons';
import { inSeason, seasonEvaluationTime } from './seasons';

const NO_REFERRER = '11111111111111111111111111111111';

export type ReferralQualification = {
  /** Minimum lifetime FRESH seeds bought by the referred wallet. */
  minFreshSeeds: bigint;
  /** Minimum seconds since the referred wallet's first planting. */
  minHoldSeconds: bigint;
};

/**
 * Parse the qualification rule from configuration. Either value missing or
 * malformed → null: qualification is not configured, so nothing qualifies.
 */
export function parseQualification(
  minFreshSeeds: string | undefined,
  minHoldHours: string | undefined,
): ReferralQualification | null {
  const a = (minFreshSeeds ?? '').trim();
  const b = (minHoldHours ?? '').trim();
  if (!/^\d+$/.test(a) || !/^\d+$/.test(b)) return null;
  const seeds = BigInt(a);
  if (seeds < 1n) return null; // wallet creation alone must never qualify
  return { minFreshSeeds: seeds, minHoldSeconds: BigInt(b) * 3_600n };
}

export type ReferredPosition = {
  owner: string;
  referrer: string;
  createdAt: bigint;
  rigs: bigint;
  lifetimeRigsBought: bigint;
  lifetimeRigsCompounded: bigint;
};

export type ReferralStatus = 'qualified' | 'pending' | 'notConfigured';

export function referralStatus(
  p: ReferredPosition,
  q: ReferralQualification | null,
  at: bigint,
): ReferralStatus {
  if (!q) return 'notConfigured';
  if (p.rigs <= 0n) return 'pending';
  if (p.lifetimeRigsBought < q.minFreshSeeds) return 'pending';
  if (at < p.createdAt + q.minHoldSeconds) return 'pending';
  return 'qualified';
}

export type ReferrerTier = { id: string; label: string; minQualified: number };

/** Cosmetic status tiers by qualified referrals. No economic effect. */
export const REFERRER_TIERS: readonly ReferrerTier[] = [
  { id: 'sharer', label: 'Seed Sharer', minQualified: 1 },
  { id: 'guide', label: 'Grove Guide', minQualified: 5 },
  { id: 'keeper', label: 'Orchard Keeper', minQualified: 25 },
  { id: 'ambassador', label: 'Cherry Ambassador', minQualified: 100 },
];

export function referrerTier(qualified: number): ReferrerTier | null {
  let t: ReferrerTier | null = null;
  for (const tier of REFERRER_TIERS) if (qualified >= tier.minQualified) t = tier;
  return t;
}

export type FarmingFlag =
  /** Many qualified referrals bound within a short burst. */
  | 'burst'
  /** Most qualified referrals sit exactly at the minimum fresh purchase. */
  | 'minimumOnly';

export type AntiFarmingRules = {
  burstCount: number;
  burstWindowSeconds: bigint;
  /** Flag when at least this share (bps) of qualified referrals are exactly at the minimum. */
  minimumOnlyShareBps: number;
  /** Minimum qualified referrals before `minimumOnly` is evaluated. */
  minimumOnlyMinSample: number;
};

export const DEFAULT_ANTI_FARMING: AntiFarmingRules = {
  burstCount: 5,
  burstWindowSeconds: 600n,
  minimumOnlyShareBps: 8_000,
  minimumOnlyMinSample: 5,
};

export type ReferrerStats = {
  referrer: string;
  total: number;
  qualified: number;
  pending: number;
  tier: ReferrerTier | null;
  /** Review signals only — a flag never removes a referral or a position. */
  flags: FarmingFlag[];
};

function farmingFlags(
  qualifiedRows: readonly ReferredPosition[],
  q: ReferralQualification,
  rules: AntiFarmingRules,
): FarmingFlag[] {
  const flags: FarmingFlag[] = [];
  const times = qualifiedRows.map((r) => r.createdAt).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  for (let i = 0; i + rules.burstCount - 1 < times.length; i++) {
    if (times[i + rules.burstCount - 1]! - times[i]! <= rules.burstWindowSeconds) {
      flags.push('burst');
      break;
    }
  }
  if (qualifiedRows.length >= rules.minimumOnlyMinSample) {
    const atMin = qualifiedRows.filter((r) => r.lifetimeRigsBought === q.minFreshSeeds).length;
    if (atMin * 10_000 >= rules.minimumOnlyShareBps * qualifiedRows.length) flags.push('minimumOnly');
  }
  return flags;
}

export type LeaderboardOptions = {
  qualification: ReferralQualification | null;
  /** Evaluation time (chain seconds). */
  at: bigint;
  /** Only referrals bound in this window, when given. */
  window?: { startsAt: bigint; endsAt: bigint };
  /** Never ranked: the official Cherry Tree referrer and any house wallets. */
  excluded?: readonly string[];
  antiFarming?: AntiFarmingRules;
};

/** Per-referrer stats for every referrer with at least one referred position. */
export function referrerStats(
  positions: readonly ReferredPosition[],
  opts: LeaderboardOptions,
): ReferrerStats[] {
  const byReferrer = new Map<string, ReferredPosition[]>();
  for (const p of positions) {
    if (!p.referrer || p.referrer === NO_REFERRER || p.referrer === p.owner) continue;
    if (opts.window && !inSeason(opts.window, p.createdAt)) continue;
    const list = byReferrer.get(p.referrer) ?? [];
    list.push(p);
    byReferrer.set(p.referrer, list);
  }
  const rules = opts.antiFarming ?? DEFAULT_ANTI_FARMING;
  const out: ReferrerStats[] = [];
  for (const [referrer, rows] of byReferrer) {
    const statuses = rows.map((r) => referralStatus(r, opts.qualification, opts.at));
    const qualifiedRows = rows.filter((_, i) => statuses[i] === 'qualified');
    const qualified = qualifiedRows.length;
    out.push({
      referrer,
      total: rows.length,
      qualified,
      pending: statuses.filter((s) => s === 'pending').length,
      tier: opts.qualification ? referrerTier(qualified) : null,
      flags: opts.qualification ? farmingFlags(qualifiedRows, opts.qualification, rules) : [],
    });
  }
  return out;
}

export type LeaderboardRow = ReferrerStats & { rank: number };

export type Leaderboard =
  | { status: 'closed'; reason: 'qualificationNotConfigured' }
  | { status: 'open'; rows: LeaderboardRow[] };

/**
 * Ranked by qualified referrals, then earliest qualifying position (earlier
 * wins the tie), then address. Excluded wallets and referrers with zero
 * qualified referrals are not ranked.
 */
export function referralLeaderboard(
  positions: readonly ReferredPosition[],
  opts: LeaderboardOptions,
  limit = 50,
): Leaderboard {
  if (!opts.qualification) return { status: 'closed', reason: 'qualificationNotConfigured' };
  const excluded = new Set(opts.excluded ?? []);
  const firstQualified = new Map<string, bigint>();
  for (const p of positions) {
    if (referralStatus(p, opts.qualification, opts.at) !== 'qualified') continue;
    if (opts.window && !inSeason(opts.window, p.createdAt)) continue;
    const prev = firstQualified.get(p.referrer);
    if (prev === undefined || p.createdAt < prev) firstQualified.set(p.referrer, p.createdAt);
  }
  const rows = referrerStats(positions, opts)
    .filter((s) => !excluded.has(s.referrer) && s.qualified > 0)
    .sort((a, b) => {
      if (a.qualified !== b.qualified) return b.qualified - a.qualified;
      const fa = firstQualified.get(a.referrer) ?? 0n;
      const fb = firstQualified.get(b.referrer) ?? 0n;
      if (fa !== fb) return fa < fb ? -1 : 1;
      return a.referrer.localeCompare(b.referrer);
    })
    .slice(0, limit)
    .map((s, i) => ({ ...s, rank: i + 1 }));
  return { status: 'open', rows };
}

/** Season-scoped leaderboard: bound within the season, judged at end + grace. */
export function seasonLeaderboard(
  positions: readonly ReferredPosition[],
  season: Season,
  now: bigint,
  opts: Omit<LeaderboardOptions, 'at' | 'window'>,
  limit = 50,
): Leaderboard | { status: 'notACompetition' } {
  const referralsCompetition: Competition = 'referrals';
  if (!season.competitions.includes(referralsCompetition)) return { status: 'notACompetition' };
  return referralLeaderboard(
    positions,
    { ...opts, at: seasonEvaluationTime(season, now), window: season },
    limit,
  );
}

/** One wallet's own referral summary, for "your referrals". */
export function myReferrals(
  positions: readonly ReferredPosition[],
  me: string,
  opts: Pick<LeaderboardOptions, 'qualification' | 'at'>,
): { total: number; qualified: number; pending: number; tier: ReferrerTier | null; configured: boolean } {
  const mine = referrerStats(
    positions.filter((p) => p.referrer === me),
    { qualification: opts.qualification, at: opts.at },
  )[0];
  return {
    total: mine?.total ?? 0,
    qualified: mine?.qualified ?? 0,
    pending: mine?.pending ?? 0,
    tier: mine?.tier ?? null,
    configured: opts.qualification !== null,
  };
}
