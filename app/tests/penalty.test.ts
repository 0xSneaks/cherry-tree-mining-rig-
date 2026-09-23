/**
 * Cross-language check: the claim penalty shown next to Claim must equal the
 * one the program takes, row for row.
 *
 * Vectors: `cargo test -p rigs-engine --test vectors emit_penalty_vectors -- --nocapture`.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { claimPenaltyBps, penaltyAmount, quoteClaim } from '../src/lib/program/penalty';

const raw = readFileSync(join(import.meta.dirname, 'fixtures', 'penalty-vectors.txt'), 'utf8');
const rows = raw
  .split('\n')
  .map((l) => l.trim())
  .filter((l) => l && !l.startsWith('---'))
  .map((l) => l.split(',').map((v) => BigInt(v)));

describe('claim penalty matches the Rust engine', () => {
  it('has vectors across every band edge', () => {
    expect(rows.length).toBeGreaterThan(100);
  });

  it('agrees on every row', () => {
    for (const [planted, now, bps, gross, penalty] of rows as bigint[][]) {
      expect(claimPenaltyBps(planted!, now!)).toBe(Number(bps));
      expect(penaltyAmount(gross!, Number(bps))).toBe(penalty);
    }
  });

  it('quotes net = gross − penalty and the time to the next step', () => {
    const q = quoteClaim(1_000n, 0n, 86_400n + 60n);
    expect(q).toEqual({
      gross: 1_000n,
      penaltyBps: 1_500,
      penalty: 150n,
      net: 850n,
      secondsToNextStep: 86_340n,
    });
    expect(quoteClaim(1_000n, 0n, 3n * 86_400n).secondsToNextStep).toBeNull();
    expect(quoteClaim(1_000n, 10n, 5n).penaltyBps).toBe(2_500);
  });
});
