/**
 * Pure fee arithmetic shared by the operator scripts, kept free of RPC and
 * filesystem imports so it is unit-tested (tests/opsFeeMath.test.ts).
 *
 * Architecture §7a: `buy_rigs` charges `purchase_fee_bps`; `compound`
 * charges the independent `compound_fee_bps` (never above the purchase
 * fee). `withdraw_fees` may move at most
 * `distributable(FEE) - reserved_commissions`.
 */
import type { Config } from '../../src/lib/program/accounts';
import { quotePurchase, type PurchaseQuote } from '../../src/lib/program/pricing';

/** What `buy_rigs(count)` costs under this Config. */
export function purchaseQuote(
  count: bigint,
  config: Pick<Config, 'rigPriceLamports' | 'purchaseFeeBps'>,
): PurchaseQuote {
  return quotePurchase(count, config.rigPriceLamports, BigInt(config.purchaseFeeBps));
}

/** What `compound` debits from pending rewards to mint `count` rigs. */
export function compoundQuote(
  count: bigint,
  config: Pick<Config, 'rigPriceLamports' | 'compoundFeeBps'>,
): PurchaseQuote {
  return quotePurchase(count, config.rigPriceLamports, BigInt(config.compoundFeeBps));
}

export interface FeeWithdrawable {
  /** FEE vault lamports above its rent-exempt reserve. */
  distributable: bigint;
  /** Referral commissions reserved inside FEE (`config.reserved_commissions`). */
  reserved: bigint;
  /** `distributable - reserved`: the most `withdraw_fees` accepts. */
  withdrawable: bigint;
}

/**
 * `withdrawable = distributable(FEE) - reserved_commissions`. Throws when the
 * reservation is not backed (reserved > distributable): the program would
 * refuse with `CommissionReservationBroken`, and an operator must
 * investigate rather than withdraw.
 */
export function feeWithdrawable(distributable: bigint, reserved: bigint): FeeWithdrawable {
  if (distributable < 0n || reserved < 0n) {
    throw new Error('negative FEE accounting input');
  }
  if (reserved > distributable) {
    throw new Error(
      `FEE accounting inconsistent: reserved commissions ${reserved} exceed ` +
        `distributable ${distributable}. Nothing is withdrawable; investigate.`,
    );
  }
  return { distributable, reserved, withdrawable: distributable - reserved };
}

/** Resolve `all` or an explicit lamport amount against the withdrawable limit. */
export function resolveWithdrawAmount(requested: 'all' | bigint, w: FeeWithdrawable): bigint {
  const amount = requested === 'all' ? w.withdrawable : requested;
  if (amount <= 0n) throw new Error('nothing to withdraw.');
  if (amount > w.withdrawable) {
    throw new Error(
      `requested ${amount} lamports but only ${w.withdrawable} are withdrawable ` +
        `(distributable ${w.distributable} - reserved commissions ${w.reserved}).`,
    );
  }
  return amount;
}
