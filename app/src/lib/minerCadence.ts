/**
 * Read-only miner cadence helpers. All timestamps are chain timestamps and
 * every amount remains integer lamports; these helpers never forecast future
 * rewards or change protocol state.
 */
import type { Config, Miner } from './program/accounts';
import { effectiveWeight, minerShares } from './program/accrual';
import { claimPenaltyBps, PENALTY_WINDOW_SECONDS } from './program/penalty';
import { quotePurchase, rigsAffordable } from './program/pricing';

export type PenaltyCadence = {
  penaltyBps: number;
  nextPenaltyBps: number | null;
  secondsToNextStep: bigint | null;
  secondsToRooted: bigint;
  rooted: boolean;
};

export function penaltyCadence(lastPlantedAt: bigint, chainTime: bigint): PenaltyCadence {
  const age = chainTime > lastPlantedAt ? chainTime - lastPlantedAt : 0n;
  const penaltyBps = claimPenaltyBps(lastPlantedAt, chainTime);
  const rootedAge = 3n * PENALTY_WINDOW_SECONDS;
  const secondsToRooted = age < rootedAge ? rootedAge - age : 0n;

  if (penaltyBps === 0) {
    return {
      penaltyBps,
      nextPenaltyBps: null,
      secondsToNextStep: null,
      secondsToRooted,
      rooted: true,
    };
  }

  const completedWindows = age / PENALTY_WINDOW_SECONDS;
  const nextAge = (completedWindows + 1n) * PENALTY_WINDOW_SECONDS;
  const nextAt = lastPlantedAt + nextAge;
  return {
    penaltyBps,
    nextPenaltyBps: claimPenaltyBps(lastPlantedAt, nextAt),
    secondsToNextStep: nextAge - age,
    secondsToRooted,
    rooted: false,
  };
}

export type ReplantState = {
  availableLamports: bigint;
  wholeSeeds: bigint;
  costLamports: bigint;
  remainderLamports: bigint;
  resultingSeeds: bigint;
  resultingShares: bigint;
  totalSharesAfter: bigint;
};

/** Current-state arithmetic only: no future emission or return estimate. */
export function replantState(
  config: Pick<Config, 'rigPriceLamports' | 'purchaseFeeBps' | 'totalShares'>,
  miner: Pick<Miner, 'rigs' | 'weight'> | null,
  availableLamports: bigint,
): ReplantState {
  const safeAvailable = availableLamports > 0n ? availableLamports : 0n;
  const wholeSeeds = rigsAffordable(
    safeAvailable,
    config.rigPriceLamports,
    BigInt(config.purchaseFeeBps),
  );
  const quote = wholeSeeds > 0n
    ? quotePurchase(wholeSeeds, config.rigPriceLamports, BigInt(config.purchaseFeeBps))
    : null;
  const weight = miner ? effectiveWeight(miner) : 100n;
  const currentSeeds = miner?.rigs ?? 0n;
  const currentShares = miner ? minerShares(miner) : 0n;
  const addedShares = wholeSeeds * weight;

  return {
    availableLamports: safeAvailable,
    wholeSeeds,
    costLamports: quote?.totalLamports ?? 0n,
    remainderLamports: quote ? safeAvailable - quote.totalLamports : safeAvailable,
    resultingSeeds: currentSeeds + wholeSeeds,
    resultingShares: currentShares + addedShares,
    totalSharesAfter: config.totalShares + addedShares,
  };
}

export function buyResetDisclosure(hasPosition: boolean, accruedLamports: bigint): string {
  if (!hasPosition) {
    return 'This fresh planting starts the position’s 72-hour harvest penalty clock at the 25% band. Harvesting and replanting do not reset it.';
  }
  const accrued = accruedLamports > 0n
    ? ' Your currently accrued REWARD will also return to the 25% claim band.'
    : '';
  return `A fresh purchase restarts the whole position’s 72-hour harvest penalty clock.${accrued} Harvesting and replanting do not reset it.`;
}
