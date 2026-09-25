/**
 * Post-72h engagement layer — status, milestones and reminders derived from
 * the permanent Miner account.
 *
 * Rules this layer obeys (and tests pin):
 *
 * * Read-only over chain state. Nothing here writes, resets or decays a
 *   position; seeds are non-redeemable, so every seed-count tier, once
 *   reached, is kept forever.
 * * No mechanic pays anything, gates anything or punishes absence. There are
 *   no cooldowns, no daily-click requirements, no weekly resets, and nothing
 *   whose purpose is to prompt another purchase. Buying resets the claim
 *   penalty clock on chain; the reminders say so plainly rather than
 *   encouraging it.
 * * "Days tended" counts distinct days the viewer opened the app, stored in
 *   this browser only. It only ever goes up — missing a day costs nothing.
 */
import type { Config, Miner } from '../program/accounts';
import { WEIGHT_REFERRED } from '../program/accrual';
import { ACCESS_WINDOW_SECONDS, accessPhase } from '../program/launch';
import { claimPenaltyBps, PENALTY_WINDOW_SECONDS } from '../program/penalty';

export const DAY = 86_400n;

export type GroveTier = { id: string; label: string; minSeeds: bigint };

/** Cosmetic grove tiers by current seeds (seeds never decrease). */
export const GROVE_TIERS: readonly GroveTier[] = [
  { id: 'seedling', label: 'Seedling', minSeeds: 1n },
  { id: 'sapling', label: 'Sapling', minSeeds: 10n },
  { id: 'youngTree', label: 'Young Tree', minSeeds: 100n },
  { id: 'blossoming', label: 'Blossoming Tree', minSeeds: 1_000n },
  { id: 'orchard', label: 'Orchard', minSeeds: 10_000n },
];

export const TENURE_DAYS = [7n, 30n, 90n, 180n, 365n] as const;
export const REPLANT_MILESTONES = [1n, 10n, 100n, 1_000n] as const;

export type Achievement = {
  id: string;
  label: string;
  detail: string;
  earned: boolean;
};

export function groveTier(seeds: bigint): GroveTier | null {
  let tier: GroveTier | null = null;
  for (const t of GROVE_TIERS) if (seeds >= t.minSeeds) tier = t;
  return tier;
}

export function nextGroveTier(seeds: bigint): { tier: GroveTier; seedsToGo: bigint } | null {
  for (const t of GROVE_TIERS) if (seeds < t.minSeeds) return { tier: t, seedsToGo: t.minSeeds - seeds };
  return null;
}

/** Planted during `[T0, T0 + 24h)` — the founder activation included. */
export function isGenesisPlanter(miner: Pick<Miner, 'createdAt'>, launchT0: bigint): boolean {
  return launchT0 > 0n && miner.createdAt >= launchT0 && miner.createdAt < launchT0 + ACCESS_WINDOW_SECONDS;
}

export function achievements(miner: Miner, config: Config, now: bigint): Achievement[] {
  const tenure = now > miner.createdAt ? (now - miner.createdAt) / DAY : 0n;
  const out: Achievement[] = [
    {
      id: 'firstSeed',
      label: 'First seed',
      detail: 'Planted a first seed.',
      earned: miner.lifetimeRigsBought > 0n,
    },
    {
      id: 'genesisPlanter',
      label: 'Genesis planter',
      detail: 'First planted during the 24-hour .skr Genesis window.',
      earned: isGenesisPlanter(miner, config.launchT0),
    },
    {
      id: 'referred',
      label: 'Referred planter',
      detail: 'Planted through a referral at the 1.05× weight.',
      earned: miner.weight === Number(WEIGHT_REFERRED),
    },
    {
      id: 'firstHarvest',
      label: 'First harvest',
      detail: 'Claimed SOL for the first time.',
      earned: miner.lifetimeSolClaimed > 0n,
    },
  ];
  for (const n of REPLANT_MILESTONES) {
    out.push({
      id: `replant${n}`,
      label: n === 1n ? 'First replant' : `Replanted ${n.toLocaleString('en-US')} seeds`,
      detail: 'Replanting is never penalised and never consumes the Genesis allowance.',
      earned: miner.lifetimeRigsCompounded >= n,
    });
  }
  for (const d of TENURE_DAYS) {
    out.push({
      id: `tenure${d}`,
      label: `${d} days in the grove`,
      detail: 'Counted from your first planting. It never resets.',
      earned: tenure >= d,
    });
  }
  for (const t of GROVE_TIERS) {
    out.push({
      id: `tier-${t.id}`,
      label: t.label,
      detail: `${t.minSeeds.toLocaleString('en-US')}+ seeds in your grove.`,
      earned: miner.rigs >= t.minSeeds,
    });
  }
  return out;
}

export type Reminder = { id: string; atSeconds: bigint; text: string };

/**
 * Upcoming chain-time events that matter to this wallet, soonest first.
 * Informational only — none asks the user to do anything.
 */
export function reminders(miner: Miner | null, config: Config, now: bigint): Reminder[] {
  const out: Reminder[] = [];
  if (config.launchT0 > 0n && accessPhase(config.launchT0, now) === 'skrGenesis') {
    out.push({
      id: 'public',
      atSeconds: config.launchT0 + ACCESS_WINDOW_SECONDS,
      text: 'Public access opens and the Genesis allowance no longer applies.',
    });
  }
  if (miner && miner.rigs > 0n) {
    const bps = claimPenaltyBps(miner.lastPlantedAt, now);
    if (bps > 0) {
      const age = now > miner.lastPlantedAt ? now - miner.lastPlantedAt : 0n;
      const nextEdge = (age / PENALTY_WINDOW_SECONDS + 1n) * PENALTY_WINDOW_SECONDS;
      const next = claimPenaltyBps(miner.lastPlantedAt, miner.lastPlantedAt + nextEdge);
      out.push({
        id: 'penalty',
        atSeconds: miner.lastPlantedAt + nextEdge,
        text:
          next === 0
            ? 'Claims become penalty-free. A new fresh purchase would restart this clock; replanting would not.'
            : `Claim penalty drops to ${next / 100}%.`,
      });
    }
  }
  if (config.rewardRate > 0n && now < config.rewardPeriodFinish) {
    out.push({
      id: 'windowEnd',
      atSeconds: config.rewardPeriodFinish,
      text: 'The funded reward window ends. Emission stops unless a new schedule is separately funded.',
    });
  }
  return out.sort((a, b) => (a.atSeconds < b.atSeconds ? -1 : a.atSeconds > b.atSeconds ? 1 : 0));
}

/** Whether a claim now pays no penalty (72h+ since the latest fresh purchase). */
export function isRooted(miner: Pick<Miner, 'lastPlantedAt' | 'rigs'>, now: bigint): boolean {
  return miner.rigs > 0n && claimPenaltyBps(miner.lastPlantedAt, now) === 0;
}

export type VisitLog = { days: number; lastDay: number };

/**
 * Record a visit on UTC day `day`. Counts distinct days; a gap never
 * subtracts. Returns the same object when the day was already counted.
 */
export function recordVisit(log: VisitLog | null, day: number): VisitLog {
  if (!Number.isInteger(day) || day < 0) throw new RangeError('day must be a non-negative integer');
  if (!log) return { days: 1, lastDay: day };
  if (day <= log.lastDay) return log;
  return { days: log.days + 1, lastDay: day };
}

export function parseVisitLog(raw: string | null): VisitLog | null {
  if (!raw) return null;
  try {
    const v = JSON.parse(raw) as Partial<VisitLog>;
    if (
      typeof v.days === 'number' &&
      typeof v.lastDay === 'number' &&
      Number.isInteger(v.days) &&
      Number.isInteger(v.lastDay) &&
      v.days >= 1 &&
      v.lastDay >= 0
    ) {
      return { days: v.days, lastDay: v.lastDay };
    }
  } catch {
    // fall through
  }
  return null;
}
