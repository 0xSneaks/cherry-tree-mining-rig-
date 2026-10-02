/**
 * Cross-language check for the §7a protocol-fee mirrors
 * (`crates/rigs-engine/src/fees.rs`): the referral commission row for row
 * against Rust-emitted vectors, the fee-pair rule, the pending-change view,
 * and the independent compound fee (test 79, TypeScript half).
 *
 * Vectors: `cargo test -p rigs-engine --test vectors emit_commission_vectors -- --nocapture`.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { FEE_CHANGE_TIMELOCK_SECONDS, MAX_PURCHASE_FEE_BPS } from '../src/lib/program/constants';
import { address, type Address } from '@solana/kit';

import {
  commissionClaimBlock,
  commissionView,
  pendingFeeChange,
  referralCommission,
  validFeePair,
} from '../src/lib/program/fees';
import { EARLY_CLAIM_DISCLOSURE } from '../src/lib/program/penalty';
import { quotePurchase } from '../src/lib/program/pricing';

const raw = readFileSync(join(import.meta.dirname, 'fixtures', 'commission-vectors.txt'), 'utf8');
const rows = raw
  .split('\n')
  .map((l) => l.trim())
  .filter((l) => l && !l.startsWith('---'))
  .map((l) => l.split(',').map((v) => BigInt(v)));

describe('referral commission matches the Rust engine', () => {
  it('has vectors across the clamp', () => {
    expect(rows.length).toBeGreaterThan(200);
    expect(rows.some((r) => r[2] === 0n)).toBe(true);
    expect(rows.some((r) => (r[2] ?? 0n) > 0n)).toBe(true);
  });

  it('agrees on every row', () => {
    for (const [fee, volume, commission] of rows as bigint[][]) {
      expect(referralCommission(fee!, volume!), `${fee},${volume}`).toBe(commission);
    }
  });

  it('is exactly 20% at a 10% fee, clamped at 9%, zero at 8%', () => {
    const v = 1_000_000_000n;
    expect(referralCommission(100_000_000n, v)).toBe(20_000_000n);
    expect(referralCommission(90_000_000n, v)).toBe(10_000_000n);
    expect(referralCommission(80_000_000n, v)).toBe(0n);
  });
});

describe('fee pair and timelock', () => {
  it('requires compound <= purchase <= 1,000 bps', () => {
    expect(MAX_PURCHASE_FEE_BPS).toBe(1_000);
    expect(validFeePair(1_000, 1_000)).toBe(true);
    expect(validFeePair(1_000, 800)).toBe(true);
    expect(validFeePair(800, 900)).toBe(false);
    expect(validFeePair(1_001, 0)).toBe(false);
  });

  it('shows a scheduled change and when it may apply', () => {
    expect(FEE_CHANGE_TIMELOCK_SECONDS).toBe(259_200n);
    const cfg = { pendingPurchaseFeeBps: 800, pendingCompoundFeeBps: 500, feeChangeEffectiveAt: 1_000n };
    expect(pendingFeeChange({ ...cfg, feeChangeEffectiveAt: 0n }, 5n)).toBeNull();
    expect(pendingFeeChange(cfg, 999n)).toEqual({
      purchaseFeeBps: 800,
      compoundFeeBps: 500,
      effectiveAt: 1_000n,
      secondsRemaining: 1n,
      applicable: false,
    });
    expect(pendingFeeChange(cfg, 1_000n)?.applicable).toBe(true);
  });
});

describe('independent compound fee (test 79, TypeScript half)', () => {
  it('buy at 1,000 and compound at 800 differ exactly by rate', () => {
    const buy = quotePurchase(37n, 10_000_000n, 1_000n);
    const comp = quotePurchase(37n, 10_000_000n, 800n);
    expect(buy.capitalLamports).toBe(comp.capitalLamports);
    expect(buy.feeLamports - comp.feeLamports).toBe((buy.capitalLamports * 200n) / 10_000n);
  });
});

describe('commission claim UI state', () => {
  const W: Address = address('7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU');
  const OTHER: Address = address('9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM');
  const ledger = (accruedCum: bigint, claimedCum: bigint, referrer: Address = W) => ({
    referrer,
    accruedCum,
    claimedCum,
    bump: 255,
  });
  const base = { wallet: W, signerAddress: W, paused: false, busy: false };

  it('maps the ledger to accrued / claimed / claimable', () => {
    expect(commissionView(null, W)).toEqual({ state: 'none' });
    expect(commissionView(ledger(50n, 10n), W)).toEqual({
      state: 'ready',
      accrued: 50n,
      claimed: 10n,
      claimable: 40n,
    });
    expect(commissionView(ledger(50n, 50n), W)).toMatchObject({ claimable: 0n });
    // A ledger naming another referrer is never claimable (substitution).
    expect(commissionView(ledger(50n, 0n, OTHER), W)).toEqual({ state: 'invalid' });
  });

  it('allows a claim only for the referrer wallet with something unpaid', () => {
    const ready = commissionView(ledger(50n, 10n), W);
    expect(commissionClaimBlock({ ...base, view: ready })).toBeNull();
    expect(commissionClaimBlock({ ...base, view: ready, signerAddress: null })).toMatch(/Connect/);
    expect(commissionClaimBlock({ ...base, view: ready, signerAddress: OTHER })).toMatch(/Only the referrer/);
    expect(commissionClaimBlock({ ...base, view: { state: 'none' } })).toMatch(/No commission/);
    expect(commissionClaimBlock({ ...base, view: { state: 'invalid' } })).toMatch(/does not match/);
    expect(commissionClaimBlock({ ...base, view: commissionView(ledger(5n, 5n), W) })).toMatch(/Nothing/);
    expect(commissionClaimBlock({ ...base, view: ready, paused: true })).toMatch(/paused/);
    expect(commissionClaimBlock({ ...base, view: ready, busy: true })).toMatch(/in progress/);
  });
});

describe('financial disclosure copy (§7a)', () => {
  const read = (p: string) => readFileSync(join(import.meta.dirname, '..', 'src', p), 'utf8');
  it('never says referrers get no payout or that replanting is free', () => {
    for (const f of [
      'components/ReferralPanel.tsx',
      'app/api/referrals/route.ts',
      'components/ClaimConfirm.tsx',
      'components/MinerCockpit.tsx',
      'lib/program/penalty.ts',
      'lib/growth/referralStats.ts',
    ]) {
      const s = read(f);
      expect(s, f).not.toMatch(/never charged/i);
      expect(s, f).not.toMatch(/no payout|receive no payout|pays referrers nothing/i);
    }
  });

  it('discloses the compound fee alongside the early-claim fee', () => {
    expect(EARLY_CLAIM_DISCLOSURE).toContain('does not pay the early-claim fee');
    expect(EARLY_CLAIM_DISCLOSURE).toContain('replant (compound) fee applies');
    expect(read('components/ClaimConfirm.tsx')).toContain('replant (compound) fee applies');
  });

  it('discloses the 20/80 split and 8% clamp for referrals', () => {
    const s = read('app/api/referrals/route.ts');
    expect(s).toContain('up to 20%');
    expect(s).toContain('keeps 80%');
    expect(s).toContain('8% of the amount planted');
  });
});
