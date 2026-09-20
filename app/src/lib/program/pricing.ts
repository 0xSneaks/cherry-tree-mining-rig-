/**
 * TypeScript port of `crates/rigs-engine/src/pricing.rs`.
 *
 * Spec §6 requires the frontend quote and the program to agree *exactly*, so
 * this is a line-for-line port over `bigint`, not an approximation. There is
 * no `number` arithmetic on lamports anywhere in this file.
 *
 * `app/tests/pricing.test.ts` cross-checks it against vectors produced by the
 * Rust implementation.
 */
import { BPS_DENOMINATOR, MAX_RIGS_PER_PURCHASE } from './constants';

export type PurchaseQuote = {
  count: bigint;
  /** Routed to the CAPITAL vault. */
  capitalLamports: bigint;
  /** Routed to the FEE vault. */
  feeLamports: bigint;
  /** `capital + fee`. What the user actually pays. */
  totalLamports: bigint;
};

export class PricingError extends Error {}

/**
 * Exact cost of `count` rigs. The fee sits *on top* of capital.
 *
 * ```text
 * capital = count * rigPrice
 * fee     = capital * feeBps / 10_000   (floor)
 * total   = capital + fee
 * ```
 */
export function quotePurchase(
  count: bigint,
  rigPriceLamports: bigint,
  purchaseFeeBps: bigint,
): PurchaseQuote {
  if (rigPriceLamports <= 0n) throw new PricingError('invalid rig price');
  if (count <= 0n || count > MAX_RIGS_PER_PURCHASE) {
    throw new PricingError('invalid rig count');
  }

  const capitalLamports = count * rigPriceLamports;
  const feeLamports = (capitalLamports * purchaseFeeBps) / BPS_DENOMINATOR;
  return {
    count,
    capitalLamports,
    feeLamports,
    totalLamports: capitalLamports + feeLamports,
  };
}

/**
 * Largest `n` with `quotePurchase(n).totalLamports <= budget`.
 *
 * Mirrors `rigs_engine::pricing::rigs_affordable`, including the single
 * upward probe for the exact-equality case. Used for the MAX button and to
 * preview what a `compound` will buy.
 */
export function rigsAffordable(
  budget: bigint,
  rigPriceLamports: bigint,
  purchaseFeeBps: bigint,
): bigint {
  if (rigPriceLamports <= 0n) throw new PricingError('invalid rig price');
  if (budget <= 0n) return 0n;

  const denom = rigPriceLamports * (BPS_DENOMINATOR + purchaseFeeBps);
  let n = (budget * BPS_DENOMINATOR) / denom;
  if (n > MAX_RIGS_PER_PURCHASE) n = MAX_RIGS_PER_PURCHASE;

  if (n < MAX_RIGS_PER_PURCHASE) {
    const next = quotePurchase(n + 1n, rigPriceLamports, purchaseFeeBps);
    if (next.totalLamports <= budget) n += 1n;
  }
  while (n > 0n) {
    if (
      quotePurchase(n, rigPriceLamports, purchaseFeeBps).totalLamports <= budget
    ) {
      break;
    }
    n -= 1n;
  }
  return n;
}
