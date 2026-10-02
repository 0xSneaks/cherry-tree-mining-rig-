import 'server-only';

/**
 * Server-side growth configuration. Every value is deployment configuration
 * and a founder decision; nothing here has a baked-in production value.
 * Missing values fail closed (leaderboard closed, no seasons).
 */
import { isAddress } from '@solana/kit';

import { OFFICIAL_REFERRER } from '../growth/referral';
import { parseQualification, type ReferralQualification } from '../growth/referralStats';
import { DEFAULT_RUNWAY_THRESHOLDS, DAY, type RunwayThresholds } from '../growth/runway';
import { parseSeasons, validateSeasons, type Season } from '../growth/seasons';

export function referralQualification(): ReferralQualification | null {
  return parseQualification(
    process.env.CHERRY_TREE_REFERRAL_MIN_FRESH_SEEDS,
    process.env.CHERRY_TREE_REFERRAL_MIN_HOLD_HOURS,
  );
}

/** Never ranked: the official Cherry Tree referrer and any configured house wallets. */
export function excludedReferrers(): string[] {
  const extra = (process.env.CHERRY_TREE_EXCLUDED_REFERRERS ?? '')
    .split(/[\s,]+/)
    .filter((a) => a !== '' && isAddress(a));
  return OFFICIAL_REFERRER.status === 'configured' ? [OFFICIAL_REFERRER.wallet, ...extra] : extra;
}

/** Seasons from `CHERRY_TREE_SEASONS_JSON`. An invalid registry yields none (fail closed). */
export function seasons(protocolVaults: readonly string[]): { seasons: Season[]; errors: string[] } {
  try {
    const list = parseSeasons(process.env.CHERRY_TREE_SEASONS_JSON);
    const errors = validateSeasons(list, protocolVaults);
    return errors.length ? { seasons: [], errors } : { seasons: list, errors: [] };
  } catch (e) {
    return { seasons: [], errors: [e instanceof Error ? e.message : 'invalid seasons config'] };
  }
}

export function runwayThresholds(): RunwayThresholds {
  const days = (v: string | undefined, d: bigint) => (v && /^\d+$/.test(v) ? BigInt(v) * DAY : d);
  return {
    lowSeconds: days(process.env.CHERRY_TREE_RUNWAY_LOW_DAYS, DEFAULT_RUNWAY_THRESHOLDS.lowSeconds),
    criticalSeconds: days(process.env.CHERRY_TREE_RUNWAY_CRITICAL_DAYS, DEFAULT_RUNWAY_THRESHOLDS.criticalSeconds),
  };
}
