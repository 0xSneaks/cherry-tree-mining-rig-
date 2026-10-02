/**
 * Operator-tooling fee arithmetic (architecture §7a): compound quotes use
 * `compound_fee_bps`, purchase quotes use `purchase_fee_bps`, and fee
 * withdrawals never reach reserved referral commissions.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import {
  compoundQuote,
  feeWithdrawable,
  purchaseQuote,
  resolveWithdrawAmount,
} from '../scripts/ops/fee-math';

const ops = join(import.meta.dirname, '..', 'scripts', 'ops');
const src = (f: string) => readFileSync(join(ops, f), 'utf8');

// Purchase and compound fees differ, so a script that quotes compound at the
// purchase rate fails here instead of silently on devnet.
const config = { rigPriceLamports: 10_000_000n, purchaseFeeBps: 1_000, compoundFeeBps: 400 };

describe('compound vs purchase quotes', () => {
  it('compound uses compound_fee_bps, purchase uses purchase_fee_bps', () => {
    expect(purchaseQuote(3n, config)).toMatchObject({
      capitalLamports: 30_000_000n,
      feeLamports: 3_000_000n,
      totalLamports: 33_000_000n,
    });
    expect(compoundQuote(3n, config)).toMatchObject({
      capitalLamports: 30_000_000n,
      feeLamports: 1_200_000n,
      totalLamports: 31_200_000n,
    });
    expect(compoundQuote(3n, config).totalLamports).not.toBe(
      purchaseQuote(3n, config).totalLamports,
    );
  });

  it('equal fees give equal quotes', () => {
    const same = { ...config, compoundFeeBps: 1_000 };
    expect(compoundQuote(7n, same)).toEqual(purchaseQuote(7n, same));
  });

  it('the smoke and rehearsal quote compound at the compound rate', () => {
    const smoke = src('devnet-smoke.ts');
    expect(smoke).toMatch(/compoundedB === compoundQuote\(mintedB, config\)\.totalLamports/);
    expect(smoke).not.toMatch(/quotePurchase\(mintedB/);
    const rehearsal = src('devnet-rehearsal.ts');
    expect(rehearsal).toMatch(/const seed = compoundQuote\(1n, config\)\.totalLamports/);
  });

  it('the devnet workflow initialises distinct purchase and compound fees', () => {
    const wf = readFileSync(
      join(import.meta.dirname, '..', '..', '.github', 'workflows', 'devnet.yml'),
      'utf8',
    );
    const p = Number(wf.match(/CHERRY_TREE_PURCHASE_FEE_BPS=(\d+)/)?.[1]);
    const c = Number(wf.match(/CHERRY_TREE_COMPOUND_FEE_BPS=(\d+)/)?.[1]);
    expect(Number.isInteger(p) && Number.isInteger(c)).toBe(true);
    expect(c).toBeLessThan(p);
  });
});

describe('fee withdrawal limits', () => {
  it('withdrawable = distributable(FEE) - reserved_commissions', () => {
    expect(feeWithdrawable(1_000n, 300n)).toEqual({
      distributable: 1_000n,
      reserved: 300n,
      withdrawable: 700n,
    });
    expect(feeWithdrawable(500n, 500n).withdrawable).toBe(0n);
    expect(feeWithdrawable(500n, 0n).withdrawable).toBe(500n);
  });

  it('fails safely when the reservation is not backed', () => {
    expect(() => feeWithdrawable(299n, 300n)).toThrow(/inconsistent/);
    expect(() => feeWithdrawable(-1n, 0n)).toThrow();
  });

  it('`all` withdraws exactly the withdrawable amount, never the reserve', () => {
    const w = feeWithdrawable(1_000n, 300n);
    expect(resolveWithdrawAmount('all', w)).toBe(700n);
    expect(resolveWithdrawAmount(700n, w)).toBe(700n);
    expect(resolveWithdrawAmount(1n, w)).toBe(1n);
    expect(() => resolveWithdrawAmount(701n, w)).toThrow(/withdrawable/);
    expect(() => resolveWithdrawAmount(1_000n, w)).toThrow(/withdrawable/);
    expect(() => resolveWithdrawAmount(0n, w)).toThrow(/nothing/);
    expect(() => resolveWithdrawAmount('all', feeWithdrawable(300n, 300n))).toThrow(/nothing/);
  });

  it('the withdraw script uses the reserved-aware limit', () => {
    const s = src('devnet-withdraw-fees.ts');
    expect(s).toContain('feeWithdrawable(distributable, config.reservedCommissions)');
    expect(s).toContain('resolveWithdrawAmount(requested, limits)');
    expect(s).toMatch(/reserved commissions/);
    expect(s).toMatch(/gross distributable FEE/);
  });
});
