/**
 * Cross-language check: the yield panel's APR, phase, decay threshold,
 * compounding projection and effective rate must equal the Rust engine's,
 * row for row. Any difference here is a yield on screen that the accumulator
 * does not pay.
 *
 * Vectors: `cargo test -p rigs-engine --test vectors emit_yield_vectors -- --nocapture`.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import type { Config } from '../src/lib/program/accounts';
import {
  PHASES,
  aprBps,
  currentPhase,
  effectiveRateBps,
  formatAprBps,
  hypotheticalPlanting,
  nextThreshold,
  projectCompounding,
  type YieldInputs,
} from '../src/lib/program/yield';

const raw = readFileSync(join(import.meta.dirname, 'fixtures', 'yield-vectors.txt'), 'utf8');

function section(name: string): bigint[][] {
  const lines = raw.split('\n');
  const start = lines.indexOf(`---${name}---`);
  expect(start, `section ${name} missing`).toBeGreaterThanOrEqual(0);
  const out: bigint[][] = [];
  for (let i = start + 1; i < lines.length; i++) {
    const l = (lines[i] ?? '').trim();
    if (l.startsWith('---')) break;
    if (l) out.push(l.split(',').map((v) => BigInt(v)));
  }
  return out;
}

function config(over: Partial<Config>): Config {
  return {
    authority: 'x' as never,
    pendingAuthority: 'x' as never,
    rewardFunder: 'x' as never,
    feeRecipient: 'x' as never,
    paused: false,
    rigPriceLamports: 10_000_000n,
    purchaseFeeBps: 300,
    totalRigs: 0n,
    rewardRate: 0n,
    rewardPeriodFinish: 0n,
    rewardLastUpdate: 0n,
    rewardPerRigStored: 0n,
    undistributedDust: 0n,
    totalRewardFunded: 0n,
    totalRewardClaimed: 0n,
    totalRewardCompounded: 0n,
    totalCapitalReceived: 0n,
    totalProtocolFees: 0n,
    totalFeesWithdrawn: 0n,
    minerCount: 0n,
    capitalVaultBump: 255,
    rewardVaultBump: 255,
    feeVaultBump: 255,
    version: 1,
    bump: 255,
    founder: 'x' as never,
    launchT0: 0n,
    totalShares: 0n,
    totalClaimPenalties: 0n,
    ...over,
  } as Config;
}

describe('APR, phase and threshold match the Rust engine', () => {
  const rows = section('APR');

  it('has a broad set of vectors', () => {
    expect(rows.length).toBeGreaterThan(2_000);
  });

  it('agrees on every row', () => {
    for (const r of rows) {
      const [rate, finish, total, price, fee, now, extra, weight, apr, phase, kind, value] =
        r as [
          bigint, bigint, bigint, bigint, bigint, bigint, bigint, bigint, bigint, bigint, bigint,
          bigint,
        ];
      const i: YieldInputs = {
        rewardRate: rate,
        rewardPeriodFinish: finish,
        totalShares: total,
        rigPriceLamports: price,
        purchaseFeeBps: fee,
        now,
      };
      const ctx = r.join(',');
      expect(aprBps(i, extra, weight), ctx).toBe(apr);
      expect(currentPhase(i), ctx).toBe(PHASES[Number(phase)]);
      const t = nextThreshold(i);
      if (kind === 0n) expect(t, ctx).toMatchObject({ kind: 'shares', atTotalShares: value });
      else if (kind === 1n) expect(t, ctx).toEqual({ kind: 'windowEnds', at: value });
      else expect(t, ctx).toEqual({ kind: 'none' });
    }
  });
});

describe('compounding projection matches the Rust engine', () => {
  const rows = section('PROJECTION');

  it('has vectors with and without compounding', () => {
    expect(rows.length).toBeGreaterThan(100);
    expect(rows.some((r) => r[9] === 0n)).toBe(true);
    expect(rows.some((r) => (r[9] ?? 0n) > 0n)).toBe(true);
    expect(rows.some((r) => r[2] === 105n)).toBe(true);
  });

  it('agrees on every row', () => {
    for (const r of rows) {
      const [total, shares, weight, rate, finish, stored, rigs, pending, now, interval, horizon, ...expected] =
        r as bigint[];
      const p = projectCompounding(
        config({
          totalRigs: total!,
          totalShares: shares!,
          rewardRate: rate!,
          rewardPeriodFinish: finish!,
          rewardLastUpdate: 0n,
          rewardPerRigStored: stored!,
        }),
        { rigs: rigs!, weight: Number(weight!), rewardPerRigPaid: stored!, pendingRewards: pending! },
        now!,
        interval!,
        horizon!,
      );
      expect(
        [p.horizonSeconds, p.startRigs, p.endRigs, p.rewardLamports, p.costBasisLamports, p.yieldBps],
        r.join(','),
      ).toEqual(expected);
    }
  });
});

describe('effective rate matches the Rust engine', () => {
  it('agrees on every row', () => {
    for (const r of section('EFFECTIVE')) {
      const [spent, claimed, compounded, claimable, created, now, rate] = r as bigint[];
      const got = effectiveRateBps(
        {
          lifetimeSolSpent: spent!,
          lifetimeSolClaimed: claimed!,
          lifetimeSolCompounded: compounded!,
          createdAt: created!,
        },
        claimable!,
        now!,
      );
      expect(got ?? -1n, r.join(',')).toBe(rate);
    }
  });
});

describe('APR formatting', () => {
  it('floors and groups', () => {
    expect(formatAprBps(284_799n)).toBe('2,847%');
    expect(formatAprBps(10_000n)).toBe('100%');
    expect(formatAprBps(9_999n)).toBe('99.9%');
    expect(formatAprBps(425n)).toBe('4.2%');
    expect(formatAprBps(0n)).toBe('0.0%');
  });
});

describe('hypothetical planting', () => {
  it('earns exactly its share from now on, and nothing from before', () => {
    const base = config({
      totalRigs: 999n,
      totalShares: 99_900n,
      rewardRate: 1_000_000n,
      rewardPeriodFinish: 1_000_000n,
      rewardLastUpdate: 0n,
      rewardPerRigStored: 0n,
    });
    const now = 500n;
    const h = hypotheticalPlanting(base, now, 1n);
    expect(h.config.totalRigs).toBe(1_000n);
    expect(h.config.totalShares).toBe(100_000n);
    expect(h.config.rewardLastUpdate).toBe(now);
    const p = projectCompounding(h.config, h.miner, now, 0n, 10_000n);
    const PRECISION = 1_000_000_000_000n;
    expect(p.rewardLamports).toBe((10_000n * 1_000_000n * PRECISION) / 1_000n / PRECISION);
  });
});

describe('yield copy never promises a return', () => {
  it('uses no guarantee language anywhere in the UI', async () => {
    const { readdirSync, readFileSync: read } = await import('node:fs');
    const dir = join(import.meta.dirname, '..', 'src', 'components');
    for (const f of readdirSync(dir)) {
      const text = read(join(dir, f), 'utf8').toLowerCase();
      for (const banned of ['guarantee', 'risk-free', 'risk free', 'fixed return', 'assured']) {
        expect(text, `${f} contains "${banned}"`).not.toContain(banned);
      }
    }
  });

  it('says the rate is variable wherever it is shown', () => {
    const panel = readFileSync(
      join(import.meta.dirname, '..', 'src', 'components', 'YieldPanel.tsx'),
      'utf8',
    );
    expect(panel).toMatch(/Variable rate/);
    expect(panel).toMatch(/not a promise of future returns/);
  });
});
