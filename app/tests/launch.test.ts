/**
 * Launch phases: the countdown on screen must be the gate the program runs.
 * Vectors: `cargo test -p rigs-engine --test vectors emit_launch_vectors`.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import {
  ACCESS_PHASES,
  accessPhase,
  formatCountdown,
  purchaseAllowed,
  secondsToNextPhase,
} from '../src/lib/program/launch';

const rows = readFileSync(join(import.meta.dirname, 'fixtures', 'launch-vectors.txt'), 'utf8')
  .split('\n')
  .map((l) => l.trim())
  .filter((l) => l && !l.startsWith('---'))
  .map((l) => l.split(',').map(BigInt));

describe('launch phases match the Rust engine', () => {
  it('has boundary vectors', () => {
    expect(rows.length).toBeGreaterThan(30);
  });

  it('agrees on every row', () => {
    for (const [t0, now, phase, next] of rows as bigint[][]) {
      const ctx = `${t0},${now}`;
      expect(accessPhase(t0!, now!), ctx).toBe(ACCESS_PHASES[Number(phase)]);
      expect(secondsToNextPhase(t0!, now!) ?? -1n, ctx).toBe(next);
    }
  });
});

describe('who may plant, per phase', () => {
  it('matches the on-chain identity matrix', () => {
    const table = {
      preLaunch: [false, false, false],
      skrGenesis: [true, false, false],
      skrSnsEarly: [true, true, false],
      public: [true, true, true],
    } as const;
    for (const [phase, [skr, sns, none]] of Object.entries(table)) {
      const p = phase as keyof typeof table;
      expect(purchaseAllowed(p, 'skr')).toBe(skr);
      expect(purchaseAllowed(p, 'sns')).toBe(sns);
      expect(purchaseAllowed(p, 'none')).toBe(none);
    }
  });
});

describe('countdown formatting', () => {
  it('formats and never goes negative', () => {
    expect(formatCountdown(86_399n)).toBe('23:59:59');
    expect(formatCountdown(90_061n)).toBe('1d 01:01:01');
    expect(formatCountdown(-5n)).toBe('00:00:00');
  });
});
