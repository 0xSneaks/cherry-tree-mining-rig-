/**
 * Client-side accrual interpolation — **display only**.
 *
 * The on-chain accumulator is authoritative. This module exists so the
 * claimable figure ticks between reconciliations; it never decides what a
 * transaction does. Every value it produces is reconciled away the moment
 * fresh program state arrives (spec §10).
 *
 * It is a faithful port of `RewardGlobals::sync` + `MinerPosition::earned`,
 * so an interpolated figure and the chain's own figure can only differ by the
 * time since the last fetch.
 */
import { PRECISION } from './constants';
import type { Config, Miner } from './accounts';

/** Matches `WEIGHT_BASE`: a rig with no referral. */
export const WEIGHT_BASE = 100n;
/** Matches `WEIGHT_REFERRED`: a validly referred rig, 1.05×. */
export const WEIGHT_REFERRED = 105n;

/** `MinerPosition::effective_weight` — `0` reads as the base weight. */
export function effectiveWeight(miner: Pick<Miner, 'weight'>): bigint {
  return miner.weight === 0 ? WEIGHT_BASE : BigInt(miner.weight);
}

/** `MinerPosition::shares` — `rigs × weight`. */
export function minerShares(miner: Pick<Miner, 'rigs' | 'weight'>): bigint {
  return miner.rigs * effectiveWeight(miner);
}

export type ChainSnapshot = {
  config: Config;
  miner: Miner | null;
  /** Reward vault lamports, rent-exempt reserve already subtracted. */
  rewardVaultDistributable: bigint;
  /** The reward vault PDA, for the VERIFY ON SOLANA link. */
  rewardVaultAddress: string;
  /** CAPITAL and FEE balances, with rent reserve removed, from the same slot. */
  capitalVaultDistributable?: bigint;
  feeVaultDistributable?: bigint;
  capitalVaultAddress?: string;
  feeVaultAddress?: string;
  /** Unix seconds the snapshot was taken at, from the chain's own clock. */
  chainTimeSeconds: bigint;
  /** `performance.now()`-style local clock reading at fetch time, in ms. */
  fetchedAtMs: number;
};

/** `min(now, periodFinish)` advanced from `rewardLastUpdate`. */
function projectRewardPerRig(config: Config, nowSeconds: bigint): bigint {
  const applicable =
    nowSeconds < config.rewardPeriodFinish ? nowSeconds : config.rewardPeriodFinish;
  if (applicable <= config.rewardLastUpdate) return config.rewardPerRigStored;
  if (config.totalShares === 0n) return config.rewardPerRigStored;

  const elapsed = applicable - config.rewardLastUpdate;
  const emission = elapsed * config.rewardRate;
  return config.rewardPerRigStored + (emission * PRECISION) / config.totalShares;
}

/**
 * Projected claimable lamports at `nowSeconds`.
 *
 * Returns the same integer the program would compute for the same timestamp.
 */
export function projectedClaimable(
  snapshot: ChainSnapshot,
  nowSeconds: bigint,
): bigint {
  const { config, miner } = snapshot;
  if (!miner) return 0n;

  const stored = projectRewardPerRig(config, nowSeconds);
  const delta = stored > miner.rewardPerRigPaid ? stored - miner.rewardPerRigPaid : 0n;
  return miner.pendingRewards + (minerShares(miner) * delta) / PRECISION;
}

/**
 * The chain's current unix second, extrapolated from the snapshot using the
 * local monotonic clock.
 *
 * Deliberately derived from `fetchedAtMs` rather than `Date.now()` so that a
 * suspended WebView cannot free-run: on `resumed` the app refetches and gets a
 * fresh anchor instead of interpolating from a stale one (spec §8, §10).
 */
export function projectChainTime(
  snapshot: ChainSnapshot,
  localNowMs: number,
): bigint {
  const elapsedMs = Math.max(0, localNowMs - snapshot.fetchedAtMs);
  return snapshot.chainTimeSeconds + BigInt(Math.floor(elapsedMs / 1000));
}

/**
 * `estimated_output = (userShares / totalShares) * rewardRate`, in lamports/second.
 *
 * Display-only. Zero once the funded window has ended — an estimate that
 * outlives its schedule would be a fabricated number.
 */
export function estimatedOutputPerSecond(
  config: Config,
  miner: Miner | null,
  nowSeconds: bigint,
): bigint {
  if (!miner || config.totalShares === 0n) return 0n;
  if (nowSeconds >= config.rewardPeriodFinish) return 0n;
  return (minerShares(miner) * config.rewardRate) / config.totalShares;
}

export const SECONDS_PER_DAY = 86_400n;

/** The same estimate expressed per day, which is how the HUD shows it. */
export function estimatedOutputPerDay(
  config: Config,
  miner: Miner | null,
  nowSeconds: bigint,
): bigint {
  return estimatedOutputPerSecond(config, miner, nowSeconds) * SECONDS_PER_DAY;
}

/**
 * Seconds of emission the reward vault can still pay for at the current rate.
 * Drives "FUNDED UNTIL" in the transparency panel.
 */
export function fundedRunwaySeconds(config: Config, nowSeconds: bigint): bigint {
  if (nowSeconds >= config.rewardPeriodFinish) return 0n;
  return config.rewardPeriodFinish - nowSeconds;
}
