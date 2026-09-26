/**
 * Display rules that the spec pins down explicitly: the rig-visual tier
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
  parseSolInput,
  shortAddress,
} from '../src/lib/format';
import { rigTier } from '../src/components/RigVisual';

describe('rig visual tiers', () => {
  it('matches the spec thresholds exactly', () => {
    const cases: [bigint, string][] = [
      [0n, 'empty'],
      [1n, 'hand'],
      [4n, 'hand'],
      [5n, 'small'],
      [24n, 'small'],
      [25n, 'industrial'],
      [99n, 'industrial'],
      [100n, 'multi'],
      [499n, 'multi'],
      [500n, 'facility'],
      [1_000_000n, 'facility'],
    ];
    for (const [rigs, key] of cases) {
      expect(rigTier(rigs).key, `${rigs} rigs`).toBe(key);
    }
  });

  it('is deterministic — the same count always renders the same tier', () => {
    for (const n of [1n, 7n, 42n, 137n, 900n]) {
      expect(rigTier(n)).toEqual(rigTier(n));
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

describe('SOL amount input', () => {
  it('parses typed SOL to exact lamports, no float', () => {
    expect(parseSolInput('0.1')).toBe(100_000_000n);
    expect(parseSolInput('1')).toBe(1_000_000_000n);
    expect(parseSolInput('.25')).toBe(250_000_000n);
    expect(parseSolInput('2.')).toBe(2_000_000_000n);
    expect(parseSolInput(' 0.000000001 ')).toBe(1n);
    expect(parseSolInput('0.3')).toBe(300_000_000n);
    expect(parseSolInput('0')).toBe(0n);
  });

  it('rejects what is not a plain amount', () => {
    for (const bad of ['', '.', 'abc', '1.2.3', '-1', '0.0000000001', '1e3', '1,5']) {
      expect(parseSolInput(bad), bad).toBeNull();
    }
  });
});
