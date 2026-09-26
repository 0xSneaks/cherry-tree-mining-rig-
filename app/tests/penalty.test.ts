/**
 * Cross-language check: the early-claim fee (architecture §7a item 5) shown
 * next to Claim must equal the one the program takes — rate, amount and the
 * 60/40 treasury/grove split — row for row.
 *
 * Vectors: `cargo test -p rigs-engine --test vectors emit_penalty_vectors -- --nocapture`.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import {
  claimPenaltyBps,
  earlyClaimFeeSplit,
  penaltyAmount,
  quoteClaim,
  topUpWarning,
} from '../src/lib/program/penalty';

const raw = readFileSync(join(import.meta.dirname, 'fixtures', 'penalty-vectors.txt'), 'utf8');
const rows = raw
  .split('\n')
  .map((l) => l.trim())
  .filter((l) => l && !l.startsWith('---'))
  .map((l) => l.split(',').map((v) => BigInt(v)));

describe('early-claim fee matches the Rust engine', () => {
  it('has vectors across every band edge', () => {
    expect(rows.length).toBeGreaterThan(100);
  });

  it('agrees on every row', () => {
    for (const [planted, now, bps, gross, penalty, treasury, grove] of rows as bigint[][]) {
      expect(claimPenaltyBps(planted!, now!)).toBe(Number(bps));
      expect(penaltyAmount(gross!, Number(bps))).toBe(penalty);
      expect(earlyClaimFeeSplit(penalty!)).toEqual({ treasury, grove });
    }
  });

  it('quotes net = gross − penalty and the time to the next step', () => {
    const q = quoteClaim(1_000n, 0n, 86_400n + 60n);
    expect(q).toEqual({
      gross: 1_000n,
      penaltyBps: 2_000,
      penalty: 200n,
      penaltyTreasury: 120n,
      penaltyGrove: 80n,
      net: 800n,
      secondsToNextStep: 86_340n,
    });
    expect(quoteClaim(1_000n, 0n, 3n * 86_400n).secondsToNextStep).toBeNull();
    expect(quoteClaim(1_000n, 10n, 5n).penaltyBps).toBe(3_000);
  });
});

describe('top-up warning (§7a item 6)', () => {
  it('is the exact mandated text with the gross pending amount', () => {
    expect(topUpWarning(1_234_000n, '0.001234')).toBe(
      'Buying more seeds restarts your early-claim timer. Your unclaimed rewards ' +
        '(0.001234 SOL) would then carry the 30% early-claim fee for the next 24 hours. ' +
        'Claim them first to keep the full amount.',
    );
  });

  it('is not shown when nothing is pending', () => {
    expect(topUpWarning(0n, '0')).toBeNull();
  });
});
