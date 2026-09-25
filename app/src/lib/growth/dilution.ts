/**
 * Public dilution — what the viewer's share of a FIXED funded emission looks
 * like as total shares grow.
 *
 * Emission is split by `Σ rigs × weight`. After Public opens, fresh purchases
 * are unlimited, so every new seed shrinks every existing holder's slice of
 * the same funded schedule. That never threatens solvency (nothing is owed
 * beyond what was funded) — it changes each holder's share. These figures are
 * arithmetic on current program state, not forecasts: they assume the rate
 * and runway stay exactly as funded today and say nothing about whether more
 * seeds will actually be planted.
 */
import type { Config, Miner } from '../program/accounts';
import { WEIGHT_BASE, minerShares } from '../program/accrual';
import { bps } from './concentration';
import { DAY, projectionsAllowed } from './runway';

export type DilutionRow = {
  /** Increase in total shares applied, in bps of today's total (0 = today). */
  increaseBps: number;
  /** Additional base-weight seeds that increase corresponds to. */
  addedSeeds: bigint;
  totalShares: bigint;
  /** Viewer's share of emission, bps. */
  shareBps: number;
  /** Viewer's distribution per day at the funded rate, lamports. */
  perDayLamports: bigint;
  /** Viewer's slice of the remaining funded emission if total shares stayed here. */
  remainingFundedLamports: bigint;
};

export type DilutionView = {
  /** False when there is no funded emission: every forward figure is 0 and hidden. */
  projectionsAllowed: boolean;
  userRigs: bigint;
  userShares: bigint;
  totalRigs: bigint;
  totalShares: bigint;
  shareBps: number;
  perDayLamports: bigint;
  remainingFundedLamports: bigint;
  runwaySeconds: bigint;
  rows: DilutionRow[];
};

export const DEFAULT_SCENARIOS_BPS = [0, 1_000, 2_500, 5_000, 10_000, 20_000, 50_000] as const;

function row(
  userShares: bigint,
  baseTotal: bigint,
  increaseBps: number,
  rate: bigint,
  runway: bigint,
  live: boolean,
): DilutionRow {
  const added = (baseTotal * BigInt(increaseBps)) / 10_000n;
  // A viewer with no position is modelled as holding nothing: total grows, share is 0.
  const total = baseTotal + added;
  const perDay = live && total > 0n ? (userShares * rate * DAY) / total : 0n;
  const remaining = live && total > 0n ? (userShares * rate * runway) / total : 0n;
  return {
    increaseBps,
    addedSeeds: added / WEIGHT_BASE,
    totalShares: total,
    shareBps: bps(userShares, total),
    perDayLamports: perDay,
    remainingFundedLamports: remaining,
  };
}

export function dilutionView(
  config: Config,
  miner: Pick<Miner, 'rigs' | 'weight'> | null,
  now: bigint,
  scenariosBps: readonly number[] = DEFAULT_SCENARIOS_BPS,
): DilutionView {
  const live = projectionsAllowed(config, now);
  const runway = live ? config.rewardPeriodFinish - now : 0n;
  const userShares = miner ? minerShares(miner) : 0n;
  const today = row(userShares, config.totalShares, 0, config.rewardRate, runway, live);
  return {
    projectionsAllowed: live,
    userRigs: miner?.rigs ?? 0n,
    userShares,
    totalRigs: config.totalRigs,
    totalShares: config.totalShares,
    shareBps: today.shareBps,
    perDayLamports: today.perDayLamports,
    remainingFundedLamports: today.remainingFundedLamports,
    runwaySeconds: runway,
    rows: scenariosBps.map((s) =>
      row(userShares, config.totalShares, s, config.rewardRate, runway, live),
    ),
  };
}

/** "What if N more seeds are planted" — N at base weight. */
export function whatIfSeedsAdded(
  config: Config,
  miner: Pick<Miner, 'rigs' | 'weight'> | null,
  now: bigint,
  addedSeeds: bigint,
): DilutionRow {
  if (addedSeeds < 0n) throw new RangeError('addedSeeds must be >= 0');
  const live = projectionsAllowed(config, now);
  const runway = live ? config.rewardPeriodFinish - now : 0n;
  const userShares = miner ? minerShares(miner) : 0n;
  const total = config.totalShares + addedSeeds * WEIGHT_BASE;
  const increaseBps =
    config.totalShares === 0n ? 0 : Number(((addedSeeds * WEIGHT_BASE) * 10_000n) / config.totalShares);
  return {
    increaseBps,
    addedSeeds,
    totalShares: total,
    shareBps: bps(userShares, total),
    perDayLamports: live && total > 0n ? (userShares * config.rewardRate * DAY) / total : 0n,
    remainingFundedLamports: live && total > 0n ? (userShares * config.rewardRate * runway) / total : 0n,
  };
}

export const DILUTION_EXPLAINER =
  'Rewards come from a fixed, pre-funded schedule shared by everyone’s seeds. ' +
  'When more seeds are planted, the schedule does not grow — each seed’s slice gets smaller. ' +
  'Your share falls even though nothing you hold changes. That is dilution. It never makes the vault ' +
  'insolvent: nothing is owed beyond what was funded. Figures below assume today’s funded rate and ' +
  'runway and are arithmetic, not a forecast or a promise.';
