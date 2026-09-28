/**
 * Early-claim fee — mirror of `crates/rigs-engine/src/penalty.rs` and
 * `fees.rs::early_claim_fee_split` (architecture §7a item 5), checked row for
 * row against Rust-emitted vectors (`app/tests/penalty.test.ts`).
 *
 * The rate is a pure function of chain time since the wallet's latest fresh
 * purchase (`Miner.lastPlantedAt`): 30% under 24h, 20% under 48h, 10% under
 * 72h, then 0%, on the whole claimed amount. Claims and replants never move
 * that anchor; a fresh purchase moves it for the whole position. A clock
 * behind the anchor counts as age 0. A replant never pays the early-claim
 * fee; it pays the separate compound fee (`compound_fee_bps`) instead.
 *
 * Destination: 60% of the fee (`floor(0.60 x)`) is Protocol Treasury revenue
 * (moved from the reward vault to the fee vault); the other 40% stays in the
 * reward vault and is redistributed to the other planters.
 *
 * The `penalty*` names are kept from V1 for the on-chain field and event
 * names; the product calls it the early-claim fee.
 */
import { EARLY_CLAIM_TREASURY_BPS } from './constants';

export const PENALTY_WINDOW_SECONDS = 86_400n;

export function claimPenaltyBps(lastPlantedAt: bigint, now: bigint): number {
  const age = now > lastPlantedAt ? now - lastPlantedAt : 0n;
  if (age < PENALTY_WINDOW_SECONDS) return 3_000;
  if (age < 2n * PENALTY_WINDOW_SECONDS) return 2_000;
  if (age < 3n * PENALTY_WINDOW_SECONDS) return 1_000;
  return 0;
}

/** Early-claim fee `x = floor(gross × bps / 10_000)`, never more than `gross`. */
export function penaltyAmount(gross: bigint, bps: number): bigint {
  const p = (gross * BigInt(bps)) / 10_000n;
  return p < gross ? p : gross;
}

/**
 * Split `x`: `treasury = floor(0.60 x)` (REWARD → FEE), `grove = x − treasury`
 * (stays in REWARD for the other planters). Rounding favours the grove.
 */
export function earlyClaimFeeSplit(fee: bigint): { treasury: bigint; grove: bigint } {
  const treasury = (fee * EARLY_CLAIM_TREASURY_BPS) / 10_000n;
  return { treasury, grove: fee - treasury };
}

export type ClaimQuote = {
  gross: bigint;
  penaltyBps: number;
  /** The early-claim fee `x`. */
  penalty: bigint;
  /** 60% of `x`: Protocol Treasury revenue. */
  penaltyTreasury: bigint;
  /** 40% of `x`: redistributed to the other planters. */
  penaltyGrove: bigint;
  net: bigint;
  /** Seconds until the rate next drops; `null` once it is 0%. */
  secondsToNextStep: bigint | null;
};

export function quoteClaim(gross: bigint, lastPlantedAt: bigint, now: bigint): ClaimQuote {
  const penaltyBps = claimPenaltyBps(lastPlantedAt, now);
  const penalty = penaltyAmount(gross, penaltyBps);
  const { treasury, grove } = earlyClaimFeeSplit(penalty);
  const age = now > lastPlantedAt ? now - lastPlantedAt : 0n;
  const secondsToNextStep =
    penaltyBps === 0 ? null : PENALTY_WINDOW_SECONDS - (age % PENALTY_WINDOW_SECONDS);
  return {
    gross,
    penaltyBps,
    penalty,
    penaltyTreasury: treasury,
    penaltyGrove: grove,
    net: gross - penalty,
    secondsToNextStep,
  };
}

/**
 * Architecture §7a item 6: a fresh purchase restarts the early-claim timer
 * for the whole position. Shown before any top-up by a wallet with pending
 * rewards; `null` (no warning) when nothing is pending. Compounding never
 * moves the timer and needs no warning. `pendingLamports` is the current
 * gross pending amount.
 */
export function topUpWarning(pendingLamports: bigint, formattedSol: string): string | null {
  if (pendingLamports <= 0n) return null;
  return (
    'Buying more seeds restarts your early-claim timer. Your unclaimed rewards ' +
    `(${formattedSol} SOL) would then carry the 30% early-claim fee for the next ` +
    '24 hours. Claiming first avoids that reset, but a claim now still pays the ' +
    'early-claim fee that currently applies.'
  );
}

/** The disclosure shown before any purchase or plant (§7a item 5). */
export const EARLY_CLAIM_DISCLOSURE =
  'Early-claim fee on rewards claimed within 72 hours of your most recent ' +
  'seed purchase: 30% under 24h, 20% from 24h to 48h, 10% from 48h to 72h, ' +
  '0% after. It applies to the whole claimed amount. 60% of the fee goes to ' +
  'the Protocol Treasury and 40% is redistributed to the other planters. ' +
  'Replanting (compounding) does not pay the early-claim fee and does not ' +
  'restart the timer, but the displayed replant (compound) fee applies to ' +
  'the seeds it plants. Buying more seeds restarts the timer.';
