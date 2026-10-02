/**
 * Protocol fee mirrors — `crates/rigs-engine/src/fees.rs` (architecture §7a),
 * checked against Rust-emitted vectors (`app/tests/fees.test.ts`).
 */
import type { Address } from '@solana/kit';

import { unpaidCommission, type Config, type ReferralLedger } from './accounts';
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

/**
 * A referrer's commission ledger as the claim UI shows it. `none`: no
 * ledger exists yet (nothing has ever accrued). `invalid`: the account at
 * the wallet's referral PDA names a different referrer — never claimable.
 */
export type CommissionView =
  | { state: 'none' }
  | { state: 'invalid' }
  | { state: 'ready'; accrued: bigint; claimed: bigint; claimable: bigint };

export function commissionView(ledger: ReferralLedger | null, wallet: Address): CommissionView {
  if (!ledger) return { state: 'none' };
  if (ledger.referrer !== wallet) return { state: 'invalid' };
  return {
    state: 'ready',
    accrued: ledger.accruedCum,
    claimed: ledger.claimedCum,
    claimable: unpaidCommission(ledger),
  };
}

/**
 * Why a commission claim cannot be submitted right now, or `null` when it
 * can. Only the referrer's own connected wallet may claim; the program
 * re-checks the same (ledger PDA seeded by the signer, `has_one = referrer`).
 */
export function commissionClaimBlock(i: {
  view: CommissionView;
  wallet: Address | null;
  signerAddress: Address | null;
  paused: boolean;
  busy: boolean;
}): string | null {
  if (!i.wallet || !i.signerAddress) return 'Connect the referrer wallet to claim.';
  if (i.signerAddress !== i.wallet) return 'Only the referrer wallet itself can claim.';
  if (i.view.state === 'none') return 'No commission has accrued yet.';
  if (i.view.state === 'invalid') return 'The commission account does not match this wallet.';
  if (i.view.claimable === 0n) return 'Nothing to claim right now.';
  if (i.paused) return 'Claims are paused while the program is paused.';
  if (i.busy) return 'A claim is already in progress.';
  return null;
}

export { FEE_CHANGE_TIMELOCK_SECONDS };
