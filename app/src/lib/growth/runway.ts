/**
 * Reward runway, Dormant state and refill history — derived from Config,
 * the reward vault balance and `RewardsFunded` events only.
 *
 * Economic facts this module encodes (and must never contradict):
 *
 * * Rewards are pre-funded through `fund_rewards`; purchases and purchase
 *   fees never reach the reward vault; there is no automatic refill.
 * * When `now >= reward_period_finish` future emission is zero. Accrued
 *   rewards stay claimable and backed. Planting does NOT restart emission.
 * * A later schedule exists only if someone separately funds one; nothing
 *   here predicts, promises or implies that it will happen.
 */
import type { Config } from '../program/accounts';
import type { RewardsFundedEvent } from '../program/events';

export const DAY = 86_400n;

/** Ops alert thresholds. Reversible defaults; override per deployment. */
export type RunwayThresholds = { lowSeconds: bigint; criticalSeconds: bigint };
export const DEFAULT_RUNWAY_THRESHOLDS: RunwayThresholds = {
  lowSeconds: 14n * DAY,
  criticalSeconds: 3n * DAY,
};

export type GroveStatus =
  /** No schedule has ever been funded. */
  | 'unfunded'
  /** Emission is live. */
  | 'live'
  /** Emission is live but the funded window ends soon. */
  | 'lowRunway'
  /** A schedule existed and has ended: runway 0, no future emission. */
  | 'dormant';

export type RunwayReport = {
  status: GroveStatus;
  alert: 'healthy' | 'low' | 'critical' | 'dormant' | 'unfunded' | 'backingShortfall';
  /** Seconds of scheduled emission left. 0 when dormant. */
  runwaySeconds: bigint;
  /** Lamports still scheduled to be emitted: rate × runway. */
  futureEmissionLamports: bigint;
  /** Current distribution per day; 0 when dormant. */
  emissionPerDayLamports: bigint;
  /** Reward vault lamports above rent. */
  distributableLamports: bigint;
  /** funded − claimed − compounded: an upper bound on everything owed plus unemitted. */
  ledgerRemainingLamports: bigint;
  /** Vault minus future emission: backs accrued claims, penalties re-credit and dust. */
  accruedBackingLamports: bigint;
  /** True when the vault covers the ledger (the program asserts this on every instruction). */
  backed: boolean;
  totalFundedLamports: bigint;
  totalClaimedLamports: bigint;
  totalCompoundedLamports: bigint;
  /** Unix seconds the funded window ends (or ended); 0 if never funded. */
  periodFinish: bigint;
};

export function runwayReport(
  config: Config,
  distributableLamports: bigint,
  now: bigint,
  thresholds: RunwayThresholds = DEFAULT_RUNWAY_THRESHOLDS,
): RunwayReport {
  const everFunded = config.totalRewardFunded > 0n;
  const live = config.rewardRate > 0n && now < config.rewardPeriodFinish;
  const runwaySeconds = live ? config.rewardPeriodFinish - now : 0n;
  const futureEmission = config.rewardRate * runwaySeconds;
  const ledgerRemaining =
    config.totalRewardFunded - config.totalRewardClaimed - config.totalRewardCompounded;
  const backed = distributableLamports >= ledgerRemaining;
  const accruedBacking =
    distributableLamports > futureEmission ? distributableLamports - futureEmission : 0n;

  let status: GroveStatus;
  if (!everFunded) status = 'unfunded';
  else if (!live) status = 'dormant';
  else if (runwaySeconds < thresholds.lowSeconds) status = 'lowRunway';
  else status = 'live';

  let alert: RunwayReport['alert'];
  if (!backed) alert = 'backingShortfall';
  else if (status === 'unfunded') alert = 'unfunded';
  else if (status === 'dormant') alert = 'dormant';
  else if (runwaySeconds < thresholds.criticalSeconds) alert = 'critical';
  else if (runwaySeconds < thresholds.lowSeconds) alert = 'low';
  else alert = 'healthy';

  return {
    status,
    alert,
    runwaySeconds,
    futureEmissionLamports: futureEmission,
    emissionPerDayLamports: live ? config.rewardRate * DAY : 0n,
    distributableLamports,
    ledgerRemainingLamports: ledgerRemaining,
    accruedBackingLamports: accruedBacking,
    backed,
    totalFundedLamports: config.totalRewardFunded,
    totalClaimedLamports: config.totalRewardClaimed,
    totalCompoundedLamports: config.totalRewardCompounded,
    periodFinish: config.rewardPeriodFinish,
  };
}

/** Product copy for each status. No promise of refill, no ROI language. */
export const STATUS_COPY: Record<GroveStatus, { title: string; body: string }> = {
  unfunded: {
    title: 'No reward schedule funded',
    body:
      'No rewards have been funded yet, so nothing is being distributed. Seeds planted now earn nothing until a schedule is separately funded.',
  },
  live: {
    title: 'Funded emission is live',
    body: 'Rewards are distributed from a pre-funded vault until the funded window ends.',
  },
  lowRunway: {
    title: 'Funded window ending soon',
    body:
      'Emission stops when the funded window ends. There is no automatic refill: any further schedule must be separately funded and announced, and none is promised.',
  },
  dormant: {
    title: 'Dormant — funded runway is 0',
    body:
      'The funded reward window has ended, so no further rewards are being distributed. ' +
      'Everything already accrued stays claimable and is backed in the reward vault. ' +
      'Planting new seeds does not restart emission. A new schedule would have to be separately funded; none is promised.',
  },
};

export type RefillEntry = {
  signature: string;
  blockTime: bigint | null;
  funder: string;
  amountLamports: bigint;
  durationSeconds: bigint;
  newRewardRate: bigint;
  periodFinish: bigint;
  /** True when the grove was Dormant immediately before this funding. */
  afterDormancy: boolean;
};

/**
 * Refill history, oldest first, from `RewardsFunded` events. `afterDormancy`
 * is decided from the previous entry's `periodFinish`: a funding that lands
 * after the previous window ended re-started emission from Dormant, and the
 * UI must label it as a separate, deliberate funding event.
 */
export function refillHistory(
  rows: readonly { signature: string; blockTime: bigint | null; event: RewardsFundedEvent }[],
): RefillEntry[] {
  const sorted = [...rows].sort((a, b) => {
    const ta = a.blockTime ?? 0n;
    const tb = b.blockTime ?? 0n;
    return ta === tb ? 0 : ta < tb ? -1 : 1;
  });
  let prevFinish: bigint | null = null;
  return sorted.map((r) => {
    const afterDormancy =
      prevFinish !== null && r.blockTime !== null && r.blockTime >= prevFinish;
    prevFinish = r.event.periodFinish;
    return {
      signature: r.signature,
      blockTime: r.blockTime,
      funder: r.event.funder,
      amountLamports: r.event.amount,
      durationSeconds: r.event.duration,
      newRewardRate: r.event.newRewardRate,
      periodFinish: r.event.periodFinish,
      afterDormancy,
    };
  });
}

/**
 * Whether a projection may be shown at all. With no funded emission every
 * forward-looking figure is exactly zero, and showing a "projected" number
 * would imply emission that does not exist.
 */
export function projectionsAllowed(config: Config, now: bigint): boolean {
  return config.rewardRate > 0n && now < config.rewardPeriodFinish;
}
