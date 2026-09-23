/**
 * Spec §10 / §12.28: the ticker interpolates between reconciliations, and the
 * interpolated figure must reconcile to the on-chain value.
 *
 * The only way to guarantee that is for the client projection to be the same
 * function the program runs, so it is checked against the engine's own output
 * timestamp for timestamp. Any drift here is drift on a user's screen.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import type { Config, Miner } from '../src/lib/program/accounts';
import {
  estimatedOutputPerDay,
  fundedRunwaySeconds,
  projectChainTime,
  projectedClaimable,
  type ChainSnapshot,
} from '../src/lib/program/accrual';

const raw = readFileSync(
  join(import.meta.dirname, 'fixtures', 'accrual-vectors.txt'),
  'utf8',
);

type Row = {
  name: string;
  t: bigint;
  config: Config;
  miner: Miner;
  earned: bigint;
};

function baseConfig(over: Partial<Config>): Config {
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
    ...over,
  };
}

function baseMiner(over: Partial<Miner>): Miner {
  return {
    owner: 'x' as never,
    rigs: 0n,
    rewardPerRigPaid: 0n,
    pendingRewards: 0n,
    lifetimeRigsBought: 0n,
    lifetimeRigsCompounded: 0n,
    lifetimeSolSpent: 0n,
    lifetimeSolCompounded: 0n,
    lifetimeSolClaimed: 0n,
    createdAt: 0n,
    lastActionAt: 0n,
    bump: 255,
    weight: 0,
    referrer: 'x' as never,
    ...over,
  };
}

const rows: Row[] = raw
  .split('\n')
  .map((l) => l.trim())
  .filter((l) => l && !l.startsWith('---'))
  .map((line) => {
    const p = line.split(',');
    const n = (i: number) => BigInt(p[i] ?? '0');
    return {
      name: p[0] ?? '',
      t: n(1),
      config: baseConfig({
        totalRigs: n(2),
        rewardRate: n(3),
        rewardPeriodFinish: n(4),
        rewardLastUpdate: n(5),
        rewardPerRigStored: n(6),
        totalShares: n(11),
      }),
      miner: baseMiner({
        rigs: n(7),
        rewardPerRigPaid: n(8),
        pendingRewards: n(9),
        weight: Number(n(12)),
      }),
      earned: n(10),
    };
  });

function snapshot(row: Row): ChainSnapshot {
  return {
    config: row.config,
    miner: row.miner,
    rewardVaultDistributable: 0n,
    rewardVaultAddress: '',
    chainTimeSeconds: row.t,
    fetchedAtMs: 0,
  };
}

describe('projectedClaimable matches the on-chain accumulator', () => {
  it('has vectors covering every scenario', () => {
    expect(rows.length).toBeGreaterThan(30);
    expect(new Set(rows.map((r) => r.name)).size).toBe(5);
  });

  it('agrees with the engine at every timestamp', () => {
    for (const row of rows) {
      expect(
        projectedClaimable(snapshot(row), row.t),
        `${row.name} @ t=${row.t}`,
      ).toBe(row.earned);
    }
  });

  it('never accrues past the funded window', () => {
    const finished = rows.filter((r) => r.t >= r.config.rewardPeriodFinish);
    expect(finished.length).toBeGreaterThan(0);
    for (const row of finished) {
      const atFinish = projectedClaimable(snapshot(row), row.config.rewardPeriodFinish);
      const later = projectedClaimable(snapshot(row), row.config.rewardPeriodFinish + 999_999n);
      expect(later, `${row.name}`).toBe(atFinish);
    }
  });

  it('credits nothing while there are no rigs', () => {
    for (const row of rows.filter((r) => r.name === 'no-rigs')) {
      expect(projectedClaimable(snapshot(row), row.t)).toBe(0n);
    }
  });
});

describe('the ticker clock', () => {
  const snap: ChainSnapshot = {
    config: baseConfig({
      totalRigs: 100n,
      totalShares: 10_000n,
      rewardRate: 1_000_000n,
      rewardPeriodFinish: 1_000_000n,
      rewardLastUpdate: 1_000n,
    }),
    miner: baseMiner({ rigs: 10n }),
    rewardVaultDistributable: 0n,
    rewardVaultAddress: '',
    chainTimeSeconds: 1_000n,
    fetchedAtMs: 5_000,
  };

  it('advances chain time from the local monotonic clock, not the wall clock', () => {
    expect(projectChainTime(snap, 5_000)).toBe(1_000n);
    expect(projectChainTime(snap, 6_500)).toBe(1_001n);
    expect(projectChainTime(snap, 65_000)).toBe(1_060n);
  });

  it('never runs backwards if the local clock jumps back', () => {
    expect(projectChainTime(snap, 0)).toBe(1_000n);
  });

  /**
   * §12.23 — after a five-minute suspend the app re-anchors instead of
   * interpolating forward. The freeze semantic is what makes that possible:
   * a suspended ticker reads the snapshot's own timestamp, so nothing on
   * screen claims time the app did not observe.
   */
  it('a frozen ticker reports the snapshot instant, not five minutes later', () => {
    const frozen = projectedClaimable(snap, snap.chainTimeSeconds);
    const freeRunning = projectedClaimable(
      snap,
      projectChainTime(snap, 5_000 + 5 * 60 * 1_000),
    );
    expect(freeRunning).toBeGreaterThan(frozen);

    // Re-anchoring is what closes the gap: a fresh snapshot at the later
    // chain time reproduces the chain's own figure exactly.
    const reanchored: ChainSnapshot = {
      ...snap,
      chainTimeSeconds: snap.chainTimeSeconds + 300n,
      fetchedAtMs: 305_000,
    };
    expect(projectedClaimable(reanchored, reanchored.chainTimeSeconds)).toBe(
      freeRunning,
    );
  });
});

describe('display estimates', () => {
  const config = baseConfig({
    totalRigs: 14_281n,
    totalShares: 1_428_100n,
    rewardRate: 25_462_962n,
    rewardPeriodFinish: 2_000_000n,
  });

  it('scales the user share of the emission rate', () => {
    const miner = baseMiner({ rigs: 42n });
    const perDay = estimatedOutputPerDay(config, miner, 1_000n);
    expect(perDay).toBe(((4_200n * 25_462_962n) / 1_428_100n) * 86_400n);
    // A referred planter's 42 rigs are 4,410 shares: 1.05× the slice.
    const referred = estimatedOutputPerDay(config, baseMiner({ rigs: 42n, weight: 105 }), 1_000n);
    expect(referred).toBe(((4_410n * 25_462_962n) / 1_428_100n) * 86_400n);
  });

  it('reports zero once the schedule has ended, never a stale rate', () => {
    const miner = baseMiner({ rigs: 42n });
    expect(estimatedOutputPerDay(config, miner, 2_000_000n)).toBe(0n);
    expect(fundedRunwaySeconds(config, 2_000_001n)).toBe(0n);
  });

  it('reports zero for someone with no position', () => {
    expect(estimatedOutputPerDay(config, null, 1_000n)).toBe(0n);
  });
});
