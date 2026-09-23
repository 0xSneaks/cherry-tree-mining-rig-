/**
 * Claim penalty — mirror of `crates/rigs-engine/src/penalty.rs`, checked row
 * for row against Rust-emitted vectors (`app/tests/penalty.test.ts`).
 *
 * The rate is a pure function of chain time since the wallet's latest
 * purchase (`Miner.lastPlantedAt`): 25% under 24h, 15% under 48h, 5% under
 * 72h, then 0%. Claims and replants never move that anchor. A clock behind
 * the anchor counts as age 0. Replant is never penalised. The penalty stays
 * in the reward vault and is owed to the other planters.
 */
export const PENALTY_WINDOW_SECONDS = 86_400n;

export function claimPenaltyBps(lastPlantedAt: bigint, now: bigint): number {
  const age = now > lastPlantedAt ? now - lastPlantedAt : 0n;
  if (age < PENALTY_WINDOW_SECONDS) return 2_500;
  if (age < 2n * PENALTY_WINDOW_SECONDS) return 1_500;
  if (age < 3n * PENALTY_WINDOW_SECONDS) return 500;
  return 0;
}

/** `ceil(gross × bps / 10_000)`, never more than `gross`. */
export function penaltyAmount(gross: bigint, bps: number): bigint {
  const p = (gross * BigInt(bps) + 9_999n) / 10_000n;
  return p < gross ? p : gross;
}

export type ClaimQuote = {
  gross: bigint;
  penaltyBps: number;
  penalty: bigint;
  net: bigint;
  /** Seconds until the rate next drops; `null` once it is 0%. */
  secondsToNextStep: bigint | null;
};

export function quoteClaim(gross: bigint, lastPlantedAt: bigint, now: bigint): ClaimQuote {
  const penaltyBps = claimPenaltyBps(lastPlantedAt, now);
  const penalty = penaltyAmount(gross, penaltyBps);
  const age = now > lastPlantedAt ? now - lastPlantedAt : 0n;
  const secondsToNextStep =
    penaltyBps === 0 ? null : PENALTY_WINDOW_SECONDS - (age % PENALTY_WINDOW_SECONDS);
  return { gross, penaltyBps, penalty, net: gross - penalty, secondsToNextStep };
}
