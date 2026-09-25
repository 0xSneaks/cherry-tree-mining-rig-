/**
 * Display rules that the spec pins down explicitly: the grove-visual tier
 * thresholds (§10) and the number formatting the HUD depends on.
 */
import { describe, expect, it } from 'vitest';

import {
  formatBps,
  formatCount,
  formatDuration,
  formatSharePercent,
  formatSol,
  formatUtcInstant,
  shortAddress,
} from '../src/lib/format';
import { groveTier } from '../src/components/GroveVisual';

describe('grove visual tiers', () => {
  it('matches the spec thresholds exactly', () => {
    const cases: [bigint, string][] = [
      [0n, 'empty'],
      [1n, 'seedling'],
      [4n, 'seedling'],
      [5n, 'sapling'],
      [24n, 'sapling'],
      [25n, 'young'],
      [99n, 'young'],
      [100n, 'tree'],
      [499n, 'tree'],
      [500n, 'grove'],
      [1_000_000n, 'grove'],
    ];
    for (const [seeds, key] of cases) {
      expect(groveTier(seeds).key, `${seeds} seeds`).toBe(key);
    }
  });

  it('is deterministic — the same count always renders the same tier', () => {
    for (const n of [1n, 7n, 42n, 137n, 900n]) {
      expect(groveTier(n)).toEqual(groveTier(n));
    }
  });
});

describe('lamport formatting', () => {
  it('renders SOL without floating point', () => {
    expect(formatSol(18_291_000n)).toBe('0.018291');
    expect(formatSol(1_000_000_000n)).toBe('1.000000');
    expect(formatSol(0n)).toBe('0.000000');
    expect(formatSol(1n, 9)).toBe('0.000000001');
    expect(formatSol(14_823_000_000n, 3)).toBe('14.823');
  });

  it('keeps full precision on very large balances', () => {
    // 10 million SOL — well past anything the vault will hold, and still exact.
    expect(formatSol(10_000_000_000_000_000n, 3)).toBe('10000000.000');
  });

  it('formats a share to three decimal places', () => {
    expect(formatSharePercent(42n, 14_281n)).toBe('0.294%');
    expect(formatSharePercent(0n, 14_281n)).toBe('0.000%');
    expect(formatSharePercent(1n, 1n)).toBe('100.000%');
    expect(formatSharePercent(1n, 0n)).toBe('0.000%');
  });

  it('formats the fee from basis points', () => {
    expect(formatBps(300)).toBe('3.00%');
    expect(formatBps(0)).toBe('0.00%');
    expect(formatBps(1)).toBe('0.01%');
  });

  it('formats the funded runway', () => {
    expect(formatDuration(0n)).toBe('ended');
    expect(formatDuration(-5n)).toBe('ended');
    expect(formatDuration(90n)).toBe('1m');
    expect(formatDuration(3_600n)).toBe('1h 00m');
    expect(formatDuration(540_000n)).toBe('6d 06h');
  });

  it('formats the funded-until instant in UTC', () => {
    // 2026-09-27T18:42:00Z
    expect(formatUtcInstant(1790534520n)).toBe('Sep 27 · 18:42 UTC');
  });

  it('shortens addresses', () => {
    expect(shortAddress('7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU')).toBe(
      '7xKX…gAsU',
    );
    expect(shortAddress('short')).toBe('short');
  });

  it('groups rig counts', () => {
    expect(formatCount(14_281n)).toBe('14,281');
    expect(formatCount(0n)).toBe('0');
  });
});
