/**
 * Decision models: Genesis-cap concentration simulator and reward-funding
 * model. These are not protocol code, but the funding model claims to port
 * the on-chain rule exactly, and both feed founder decisions, so they are
 * pinned here.
 */
import { describe, expect, it } from 'vitest';

import {
  CANDIDATE_CAPS,
  demandCurve,
  maxCapForTarget,
  presetScenario,
  simulateCap,
} from '../src/lib/model/genesisCap';
import {
  DAY,
  LAMPORTS_PER_SOL,
  aprBpsAt,
  fund,
  previewFunding,
  requiredFundingLamports,
  simulateRefills,
} from '../src/lib/model/rewardFunding';
import { aprBps } from '../src/lib/program/yield';
import { buildReport as capReport } from '../scripts/model/genesis-cap';
import { buildReport as fundingReport } from '../scripts/model/reward-funding';

describe('Genesis cap simulator', () => {
  it('demand curve is deterministic, descending and bounded', () => {
    const d = { minSeeds: 5, alpha: 1.16, maxSeeds: 1_000 };
    const a = demandCurve(100, d);
    expect(a).toEqual(demandCurve(100, d));
    expect(a).toHaveLength(100);
    for (let i = 1; i < a.length; i++) expect(a[i]!).toBeLessThanOrEqual(a[i - 1]!);
    expect(Math.min(...a)).toBeGreaterThanOrEqual(5);
    expect(Math.max(...a)).toBeLessThanOrEqual(1_000);
    expect(() => demandCurve(-1, d)).toThrow();
    expect(() => demandCurve(1, { ...d, alpha: 0 })).toThrow();
  });

  it('a k-wallet actor gets exactly k × cap when demand exceeds it', () => {
    const s = presetScenario({ actorWallets: 5, actorDesiredSeeds: 1_000_000 });
    const r = simulateCap(s, 100);
    const ordinary = demandCurve(r.participants, s.demand).reduce((a, d) => a + Math.min(d, 100), 0);
    const founder = Math.min(s.founderSeeds, 100);
    expect(r.genesisFreshSeeds).toBe(ordinary + founder + 500);
    expect(r.actorBps).toBe(Math.floor((500 * 10_000) / r.genesisFreshSeeds));
  });

  it('the founder activation counts against the founder cap (A <= C)', () => {
    const s = presetScenario({ founderSeeds: 1_000 });
    expect(simulateCap(s, 50).founderBps).toBe(Math.floor((50 * 10_000) / simulateCap(s, 50).genesisFreshSeeds));
  });

  it('actor share rises with cap and with wallets controlled', () => {
    const s = presetScenario();
    let prev = -1;
    for (const c of CANDIDATE_CAPS) {
      const b = simulateCap(s, c).actorBps;
      expect(b).toBeGreaterThanOrEqual(prev);
      prev = b;
    }
    const byK = [1, 2, 3, 5, 10].map((k) => simulateCap({ ...s, actorWallets: k }, 100).actorBps);
    for (let i = 1; i < byK.length; i++) expect(byK[i]!).toBeGreaterThan(byK[i - 1]!);
  });

  it('more participation dilutes the actor at the same cap', () => {
    const lo = simulateCap(presetScenario({ participation: 0.005 }), 100).actorBps;
    const hi = simulateCap(presetScenario({ participation: 0.15 }), 100).actorBps;
    expect(hi).toBeLessThan(lo);
  });

  it('Genesis compounding is uncapped and raises a full-replanting actor', () => {
    const s = presetScenario({ genesisCompoundSeeds: 2_000, othersCompoundFraction: 0 });
    const r = simulateCap(s, 100);
    expect(r.actorBpsAfterCompounding).toBeGreaterThan(r.actorBps);
  });

  it('Public demand dilutes the Genesis cohort monotonically', () => {
    const r = simulateCap(presetScenario(), 100);
    const cohort = r.afterPublic.map((p) => p.genesisCohortBps);
    expect(cohort[0]!).toBeGreaterThan(cohort[1]!);
    expect(cohort[1]!).toBeGreaterThan(cohort[2]!);
  });

  it('no cap reproduces unconstrained demand', () => {
    const r = simulateCap(presetScenario(), null);
    expect(r.deferredSeeds).toBe(0);
    expect(r.capBindingBps).toBe(0);
  });

  it('max cap for a target is the largest candidate at or below the target', () => {
    const s = presetScenario();
    const best = maxCapForTarget(s, CANDIDATE_CAPS, 1_000)!;
    expect(simulateCap(s, best).actorBps).toBeLessThanOrEqual(1_000);
    const next = CANDIDATE_CAPS[CANDIDATE_CAPS.indexOf(best as (typeof CANDIDATE_CAPS)[number]) + 1]!;
    expect(simulateCap(s, next).actorBps).toBeGreaterThan(1_000);
    expect(maxCapForTarget(s, CANDIDATE_CAPS, 0)).toBeNull();
  });

  it('rejects invalid inputs', () => {
    expect(() => simulateCap(presetScenario(), 0)).toThrow();
    expect(() => simulateCap(presetScenario({ participation: 2 }), 10)).toThrow();
    expect(() => simulateCap(presetScenario({ actorWallets: 0 }), 10)).toThrow();
  });

  it('report is deterministic and never recommends the qualification cap', () => {
    const a = capReport().markdown;
    expect(a).toBe(capReport().markdown);
    expect(a).toMatch(/10 \(qualification test value only\)/);
    expect(a).toMatch(/assumption/i);
  });
});

describe('reward funding model', () => {
  it('ports the on-chain funding rule, including leftover roll-over', () => {
    const s0 = fund({ rate: 0n, finish: 0n }, 0n, 100n * LAMPORTS_PER_SOL, 100n * DAY);
    expect(s0.rate).toBe((100n * LAMPORTS_PER_SOL) / (100n * DAY));
    const s1 = fund(s0, 50n * DAY, 10n * LAMPORTS_PER_SOL, 100n * DAY);
    expect(s1.rate).toBe((50n * DAY * s0.rate + 10n * LAMPORTS_PER_SOL) / (100n * DAY));
    expect(s1.finish).toBe(150n * DAY);
    const after = fund(s0, 200n * DAY, 1n * LAMPORTS_PER_SOL, 10n * DAY);
    expect(after.rate).toBe(LAMPORTS_PER_SOL / (10n * DAY)); // nothing left over once Dormant
    expect(() => fund(s0, 0n, 0n, DAY)).toThrow();
    expect(() => fund(s0, 0n, 1n, 0n)).toThrow();
    expect(() => fund(s0, 0n, 1n, 11n * 365n * DAY)).toThrow();
  });

  it('preview flags a top-up that lowers the rate', () => {
    const cur = fund({ rate: 0n, finish: 0n }, 0n, 100n * LAMPORTS_PER_SOL, 180n * DAY);
    const p = previewFunding(cur, 30n * DAY, 5n * LAMPORTS_PER_SOL, 365n * DAY);
    expect(p.lowersRate).toBe(true);
    expect(p.rateChangeBps!).toBeLessThan(0);
    const up = previewFunding(cur, 30n * DAY, 100n * LAMPORTS_PER_SOL, 150n * DAY);
    expect(up.lowersRate).toBe(false);
    expect(previewFunding({ rate: 0n, finish: 0n }, 0n, LAMPORTS_PER_SOL, DAY).rateChangeBps).toBeNull();
  });

  it('rate at planted level matches the app’s yield port', () => {
    const rate = 12_345n;
    for (const seeds of [1n, 10n, 1_000n, 1_000_000n]) {
      const fromApp = aprBps(
        {
          rewardRate: rate,
          rewardPeriodFinish: 10n ** 12n,
          totalShares: seeds * 100n,
          rigPriceLamports: 10_000_000n,
          purchaseFeeBps: 300n,
          now: 0n,
        },
        0n,
      );
      expect(aprBpsAt(rate, seeds, 10_000_000n, 300n)).toBe(fromApp);
    }
    expect(aprBpsAt(rate, 0n, 1n, 0n)).toBe(0n);
  });

  it('required funding is target × capital × time', () => {
    // 1,200% on 1,000 seeds × 0.0103 SOL for 365 days = 12 × 10.3 SOL.
    expect(requiredFundingLamports(120_000n, 1_000n, 10_000_000n, 300n, 365n)).toBe(123_600_000_000n);
  });

  it('refill simulation walks Dormant and a separately funded restart', () => {
    const r = simulateRefills({
      initialSol: 100n,
      initialDays: 180n,
      refills: [{ atDay: 200n, sol: 50n, days: 90n }],
      sampleDays: [0n, 180n, 199n, 200n, 289n, 290n],
    });
    expect(r.samples.map((s) => s.dormant)).toEqual([false, true, true, false, false, true]);
    expect(r.samples[1]!.rate).toBe(0n);
    expect(r.dormantFromDay).toBe(290n);
    expect(r.totalFundedSol).toBe(150n);
  });

  it('report is deterministic and never models purchase money as reward funding', () => {
    const m = fundingReport().markdown;
    expect(m).toBe(fundingReport().markdown);
    expect(m).toMatch(/external, non-purchase SOL/);
    expect(m).not.toMatch(/guarantee|ROI/i);
  });
});

describe('committed decision tables', () => {
  it('docs/models match the generators (regenerate with --write)', async () => {
    const { readFileSync } = await import('node:fs');
    const { join } = await import('node:path');
    const dir = join(import.meta.dirname, '..', '..', 'docs', 'models');
    expect(readFileSync(join(dir, 'genesis-cap-scenarios.md'), 'utf8')).toBe(capReport().markdown);
    expect(readFileSync(join(dir, 'reward-funding-scenarios.md'), 'utf8')).toBe(fundingReport().markdown);
  });
});
