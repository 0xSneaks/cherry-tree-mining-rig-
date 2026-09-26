/**
 * Protocol fee mirrors — `crates/rigs-engine/src/fees.rs` (architecture §7a),
 * checked against Rust-emitted vectors (`app/tests/fees.test.ts`).
 */
import type { Config } from './accounts';
import {
  BPS_DENOMINATOR,
  FEE_CHANGE_TIMELOCK_SECONDS,
  MAX_PURCHASE_FEE_BPS,
  REFERRAL_COMMISSION_BPS,
  REFERRAL_RETAINED_MARGIN_BPS,
} from './constants';

/**
 * Referral commission on one eligible fee event:
 * `min(floor(0.20 f), max(0, f − ceil(0.08 v)))`, with `f` the fee received
 * and `v` the fee-bearing principal. The Protocol Treasury keeps the rest.
 */
export function referralCommission(fee: bigint, volume: bigint): bigint {
  const raw = (fee * REFERRAL_COMMISSION_BPS) / BPS_DENOMINATOR;
  const margin = (volume * REFERRAL_RETAINED_MARGIN_BPS + BPS_DENOMINATOR - 1n) / BPS_DENOMINATOR;
  const headroom = fee > margin ? fee - margin : 0n;
  return raw < headroom ? raw : headroom;
}

/** `compound <= purchase <= MAX_PURCHASE_FEE_BPS`. */
export function validFeePair(purchaseFeeBps: number, compoundFeeBps: number): boolean {
  return purchaseFeeBps <= MAX_PURCHASE_FEE_BPS && compoundFeeBps <= purchaseFeeBps;
}

export type PendingFeeChange = {
  purchaseFeeBps: number;
  compoundFeeBps: number;
  effectiveAt: bigint;
  /** Seconds until it may be applied; `0n` once applicable. */
  secondsRemaining: bigint;
  applicable: boolean;
};

/** The scheduled fee change, if any, as users must see it before it lands. */
export function pendingFeeChange(
  config: Pick<
    Config,
    'pendingPurchaseFeeBps' | 'pendingCompoundFeeBps' | 'feeChangeEffectiveAt'
  >,
  now: bigint,
): PendingFeeChange | null {
  if (config.feeChangeEffectiveAt === 0n) return null;
  const secondsRemaining =
    config.feeChangeEffectiveAt > now ? config.feeChangeEffectiveAt - now : 0n;
  return {
    purchaseFeeBps: config.pendingPurchaseFeeBps,
    compoundFeeBps: config.pendingCompoundFeeBps,
    effectiveAt: config.feeChangeEffectiveAt,
    secondsRemaining,
    applicable: secondsRemaining === 0n,
  };
}

export { FEE_CHANGE_TIMELOCK_SECONDS };
