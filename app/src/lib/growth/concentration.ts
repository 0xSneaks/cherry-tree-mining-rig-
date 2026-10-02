/**
 * Distribution / concentration analytics over indexed Miner positions.
 *
 * Transparency, not enforcement. The program's only concentration control is
 * the cumulative per-WALLET Genesis cap during [T0, T0 + 24h). Nothing here
 * can detect or prevent one person holding many wallets: a 10-wallet actor
 * shows up as 10 unrelated rows. The numbers make concentration VISIBLE; they
 * do not prove distinct humans, and the UI must never say they do.
 *
 * Integer math only (basis points), so the same input always renders the
 * same figure.
 */
import { WEIGHT_BASE } from '../program/accrual';

export type Position = {
  owner: string;
  rigs: bigint;
  /** 100 base / 105 referred; 0 reads as base. */
  weight: number;
};

export function sharesOf(p: Position): bigint {
  return p.rigs * (p.weight === 0 ? WEIGHT_BASE : BigInt(p.weight));
}

/** `part / total` in basis points, floored. 0 when total is 0. */
export function bps(part: bigint, total: bigint): number {
  if (total <= 0n) return 0;
  return Number((part * 10_000n) / total);
}

export type ConcentrationLevel = 'dispersed' | 'moderate' | 'concentrated' | 'highlyConcentrated';

/** Warning thresholds on the largest single wallet's share of total shares. */
export const WHALE_WALLET_WARN_BPS = 1_000; // 10%
export const WHALE_WALLET_ALERT_BPS = 2_500; // 25%

/** HHI bands (0..10_000) as used for market concentration: <1500, <2500, ≥2500. */
export function concentrationLevel(hhi: number, largestBps: number): ConcentrationLevel {
  if (hhi >= 2_500 || largestBps >= WHALE_WALLET_ALERT_BPS * 2) return 'highlyConcentrated';
  if (hhi >= 1_500 || largestBps >= WHALE_WALLET_ALERT_BPS) return 'concentrated';
  if (hhi >= 500 || largestBps >= WHALE_WALLET_WARN_BPS) return 'moderate';
  return 'dispersed';
}

export type Distribution = {
  wallets: number;
  totalRigs: bigint;
  totalShares: bigint;
  /** Largest single wallet's share of total shares, bps. */
  largestBps: number;
  top5Bps: number;
  top10Bps: number;
  /** Share held by the top 1% of wallets (at least one wallet), bps. */
  top1PctBps: number;
  /** Herfindahl–Hirschman index over wallet shares, 0..10_000. */
  hhi: number;
  /** Gini coefficient over wallet shares, bps (0 = equal, 10_000 = one holder). */
  giniBps: number;
  /** Median wallet's seed count. */
  medianRigs: bigint;
  level: ConcentrationLevel;
  /** Wallets holding at least WHALE_WALLET_WARN_BPS of shares. */
  whales: { owner: string; rigs: bigint; shareBps: number }[];
};

export function distribution(positions: readonly Position[]): Distribution {
  const active = positions.filter((p) => p.rigs > 0n);
  const sorted = [...active].sort((a, b) => {
    const sa = sharesOf(a);
    const sb = sharesOf(b);
    return sa === sb ? a.owner.localeCompare(b.owner) : sa > sb ? -1 : 1;
  });
  const totalShares = sorted.reduce((s, p) => s + sharesOf(p), 0n);
  const totalRigs = sorted.reduce((s, p) => s + p.rigs, 0n);
  const topN = (n: number) => bps(sorted.slice(0, n).reduce((s, p) => s + sharesOf(p), 0n), totalShares);

  // HHI in bps²/10_000: Σ (share_bps)² / 10_000, computed exactly in bigint.
  let hhiNum = 0n;
  for (const p of sorted) {
    const s = sharesOf(p);
    hhiNum += s * s;
  }
  const hhi = totalShares === 0n ? 0 : Number((hhiNum * 10_000n) / (totalShares * totalShares));

  // Gini = Σ_i (2i − n − 1)·x_i / (n · Σx), x ascending, i from 1.
  const n = BigInt(sorted.length);
  let giniNum = 0n;
  const asc = [...sorted].reverse();
  asc.forEach((p, idx) => {
    const i = BigInt(idx + 1);
    giniNum += (2n * i - n - 1n) * sharesOf(p);
  });
  const giniBps = n === 0n || totalShares === 0n ? 0 : Number((giniNum * 10_000n) / (n * totalShares));

  const mid = asc.length === 0 ? 0n : asc[Math.floor((asc.length - 1) / 2)]!.rigs;
  const largestBps = topN(1);
  const onePct = Math.max(1, Math.ceil(sorted.length / 100));

  return {
    wallets: sorted.length,
    totalRigs,
    totalShares,
    largestBps,
    top5Bps: topN(5),
    top10Bps: topN(10),
    top1PctBps: sorted.length === 0 ? 0 : topN(onePct),
    hhi,
    giniBps,
    medianRigs: mid,
    level: concentrationLevel(hhi, largestBps),
    whales: sorted
      .map((p) => ({ owner: p.owner, rigs: p.rigs, shareBps: bps(sharesOf(p), totalShares) }))
      .filter((w) => w.shareBps >= WHALE_WALLET_WARN_BPS),
  };
}

/** Plain-language banner for the concentration level. Never claims Sybil resistance. */
export const CONCENTRATION_COPY: Record<ConcentrationLevel, string> = {
  dispersed: 'Emission is spread across many wallets.',
  moderate: 'A few wallets hold a noticeable share of the grove.',
  concentrated: 'A small number of wallets hold a large share of the grove.',
  highlyConcentrated: 'The grove is highly concentrated in very few wallets.',
};

/**
 * Required caveat wherever wallet analytics are shown. The Genesis cap and
 * every figure here are per wallet; one person can control many wallets.
 */
export const PER_WALLET_CAVEAT =
  'All limits and figures here are per wallet, not per person. One person can hold several wallets, ' +
  'and nothing on chain or in this app can tell. A .skr domain adds friction; it does not prove one human per wallet.';
