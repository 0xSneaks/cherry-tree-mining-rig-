/**
 * Genesis yield — a line-for-line port of `rigs_engine::yield_curve`.
 *
 * Every figure the yield panel shows comes from here, and every function here
 * is checked against the Rust engine's own output
 * (`tests/yield.test.ts` ← `cargo test -p rigs-engine --test vectors`).
 * The inputs are decoded program state and the chain clock — nothing the app
 * estimates — so the APR on screen is the rate the accumulator pays.
 *
 * bigint only. No floating point touches a rate.
 */
import type { Config, Miner } from './accounts';
import { BPS_DENOMINATOR, PRECISION } from './constants';
import { quotePurchase, rigsAffordable } from './pricing';

/** Matches `yield_curve::SECONDS_PER_YEAR` (365 days). */
export const SECONDS_PER_YEAR = 31_536_000n;
/** Matches `yield_curve::MAX_PROJECTION_STEPS`. */
export const MAX_PROJECTION_STEPS = 1_000n;

export type YieldPhase = 'genesis' | 'earlyGrove' | 'growth' | 'mature' | 'dormant';

/** Order and codes match `YieldPhase::code()`. */
export const PHASES: readonly YieldPhase[] = [
  'genesis',
  'earlyGrove',
  'growth',
  'mature',
  'dormant',
];

export const PHASE_FLOOR_BPS: Record<YieldPhase, bigint> = {
  genesis: 120_000n,
  earlyGrove: 60_000n,
  growth: 25_000n,
  mature: 1n,
  dormant: 0n,
};

export const PHASE_LABEL: Record<YieldPhase, string> = {
  genesis: 'Genesis',
  earlyGrove: 'Early Grove',
  growth: 'Growth',
  mature: 'Mature Grove',
  dormant: 'Dormant',
};

export function nextPhase(p: YieldPhase): YieldPhase {
  const i = PHASES.indexOf(p);
  return p === 'dormant' ? 'dormant' : (PHASES[i + 1] ?? 'dormant');
}

export function phaseFromAprBps(aprBps: bigint): YieldPhase {
  for (const p of ['genesis', 'earlyGrove', 'growth', 'mature'] as const) {
    if (aprBps >= PHASE_FLOOR_BPS[p]) return p;
  }
  return 'dormant';
}

export type YieldInputs = {
  rewardRate: bigint;
  rewardPeriodFinish: bigint;
  totalRigs: bigint;
  rigPriceLamports: bigint;
  purchaseFeeBps: bigint;
  now: bigint;
};

export function yieldInputs(config: Config, now: bigint): YieldInputs {
  return {
    rewardRate: config.rewardRate,
    rewardPeriodFinish: config.rewardPeriodFinish,
    totalRigs: config.totalRigs,
    rigPriceLamports: config.rigPriceLamports,
    purchaseFeeBps: BigInt(config.purchaseFeeBps),
    now,
  };
}

const U64_MAX = (1n << 64n) - 1n;
const sat = (v: bigint) => (v > U64_MAX ? U64_MAX : v);

function numerator(i: YieldInputs): bigint {
  return i.rewardRate * SECONDS_PER_YEAR * BPS_DENOMINATOR * BPS_DENOMINATOR;
}

function costPerRigScaled(i: YieldInputs): bigint {
  if (i.rigPriceLamports <= 0n) throw new Error('invalid rig price');
  return i.rigPriceLamports * (BPS_DENOMINATOR + i.purchaseFeeBps);
}

/** `yield_curve::apr_bps`. */
export function aprBps(i: YieldInputs, extraRigs = 0n): bigint {
  const cost = costPerRigScaled(i);
  if (i.rewardRate === 0n || i.now >= i.rewardPeriodFinish) return 0n;
  const rigs = i.totalRigs + extraRigs > 0n ? i.totalRigs + extraRigs : 1n;
  return sat(numerator(i) / (rigs * cost));
}

export function currentPhase(i: YieldInputs): YieldPhase {
  return phaseFromAprBps(aprBps(i));
}

export type NextThreshold =
  | { kind: 'rigs'; phase: YieldPhase; atTotalRigs: bigint }
  | { kind: 'windowEnds'; at: bigint }
  | { kind: 'none' };

/** `yield_curve::next_threshold`. */
export function nextThreshold(i: YieldInputs): NextThreshold {
  const phase = currentPhase(i);
  if (phase === 'dormant') return { kind: 'none' };
  if (phase === 'mature') return { kind: 'windowEnds', at: i.rewardPeriodFinish };
  const t = numerator(i) / (PHASE_FLOOR_BPS[phase] * costPerRigScaled(i)) + 1n;
  return { kind: 'rigs', phase: nextPhase(phase), atTotalRigs: sat(t) };
}

export type Projection = {
  horizonSeconds: bigint;
  startRigs: bigint;
  endRigs: bigint;
  rewardLamports: bigint;
  costBasisLamports: bigint;
  yieldBps: bigint;
};

type Globals = {
  totalRigs: bigint;
  rewardRate: bigint;
  rewardPeriodFinish: bigint;
  rewardLastUpdate: bigint;
  rewardPerRigStored: bigint;
};
type Position = { rigs: bigint; rewardPerRigPaid: bigint; pendingRewards: bigint };

function sync(g: Globals, now: bigint): void {
  const applicable = now < g.rewardPeriodFinish ? now : g.rewardPeriodFinish;
  if (applicable <= g.rewardLastUpdate) return;
  const elapsed = applicable - g.rewardLastUpdate;
  // A projection always carries the planter's own rigs, so total_rigs > 0
  // and the zero-rig dust branch of the engine cannot be reached here.
  if (g.totalRigs > 0n) {
    g.rewardPerRigStored += (elapsed * g.rewardRate * PRECISION) / g.totalRigs;
  }
  g.rewardLastUpdate = applicable;
}

function settle(g: Globals, p: Position, now: bigint): void {
  sync(g, now);
  p.pendingRewards += (p.rigs * (g.rewardPerRigStored - p.rewardPerRigPaid)) / PRECISION;
  p.rewardPerRigPaid = g.rewardPerRigStored;
}

/**
 * `yield_curve::project_compounding`: run the position forward applying the
 * real compound transition every `intervalSeconds` (0 = never), for
 * `min(horizonSeconds, funded runway)`. Everyone else is held where they are.
 */
export function projectCompounding(
  config: Config,
  miner: Pick<Miner, 'rigs' | 'rewardPerRigPaid' | 'pendingRewards'>,
  now: bigint,
  intervalSeconds: bigint,
  horizonSeconds: bigint,
): Projection {
  if (miner.rigs <= 0n) throw new Error('invalid rig count');
  const runway = now < config.rewardPeriodFinish ? config.rewardPeriodFinish - now : 0n;
  const horizon = horizonSeconds < runway ? horizonSeconds : runway;
  if (intervalSeconds > 0n && horizon / intervalSeconds > MAX_PROJECTION_STEPS) {
    throw new Error('projection interval too fine');
  }
  const price = config.rigPriceLamports;
  const fee = BigInt(config.purchaseFeeBps);

  const g: Globals = {
    totalRigs: config.totalRigs,
    rewardRate: config.rewardRate,
    rewardPeriodFinish: config.rewardPeriodFinish,
    rewardLastUpdate: config.rewardLastUpdate,
    rewardPerRigStored: config.rewardPerRigStored,
  };
  const p: Position = {
    rigs: miner.rigs,
    rewardPerRigPaid: miner.rewardPerRigPaid,
    pendingRewards: miner.pendingRewards,
  };
  settle(g, p, now);
  const startPending = p.pendingRewards;
  const startRigs = p.rigs;
  const end = now + horizon;

  let compounded = 0n;
  if (intervalSeconds > 0n) {
    for (let t = now + intervalSeconds; t <= end; t += intervalSeconds) {
      settle(g, p, t);
      const count = rigsAffordable(p.pendingRewards, price, fee);
      if (count === 0n) continue;
      const q = quotePurchase(count, price, fee);
      p.pendingRewards -= q.totalLamports;
      g.totalRigs += count;
      p.rigs += count;
      compounded += q.totalLamports;
    }
  }
  settle(g, p, end);

  const reward = p.pendingRewards + compounded - startPending;
  const cost = (startRigs * price * (BPS_DENOMINATOR + fee)) / BPS_DENOMINATOR;
  return {
    horizonSeconds: horizon,
    startRigs,
    endRigs: p.rigs,
    rewardLamports: reward,
    costBasisLamports: sat(cost),
    yieldBps: cost === 0n ? 0n : sat((reward * BPS_DENOMINATOR) / cost),
  };
}

/** `yield_curve::effective_rate_bps`. `null` for the first hour. */
export function effectiveRateBps(
  miner: Pick<
    Miner,
    'lifetimeSolSpent' | 'lifetimeSolClaimed' | 'lifetimeSolCompounded' | 'createdAt'
  >,
  claimableLamports: bigint,
  now: bigint,
): bigint | null {
  const age = now - miner.createdAt;
  if (age < 3_600n || miner.lifetimeSolSpent === 0n) return null;
  const received =
    miner.lifetimeSolClaimed + miner.lifetimeSolCompounded + claimableLamports;
  return sat((received * SECONDS_PER_YEAR * BPS_DENOMINATOR) / (miner.lifetimeSolSpent * age));
}

/** `2,847%` at and above 100%, `42.5%` below — rates are floored, never rounded up. */
export function formatAprBps(bps: bigint): string {
  if (bps >= 10_000n) {
    return `${(bps / 100n).toLocaleString('en-US')}%`;
  }
  return `${bps / 100n}.${((bps % 100n) / 10n).toString()}%`;
}

/**
 * The state a `buy_rigs(rigs)` at `now` would leave behind, for projecting a
 * position that does not exist yet: the accumulator is synced at the current
 * rig count first (so the new rigs earn nothing that predates them), then the
 * rigs are added — the same settle-then-transition order the program uses.
 */
export function hypotheticalPlanting(
  config: Config,
  now: bigint,
  rigs: bigint,
): { config: Config; miner: Pick<Miner, 'rigs' | 'rewardPerRigPaid' | 'pendingRewards'> } {
  const g: Globals = {
    totalRigs: config.totalRigs,
    rewardRate: config.rewardRate,
    rewardPeriodFinish: config.rewardPeriodFinish,
    rewardLastUpdate: config.rewardLastUpdate,
    rewardPerRigStored: config.rewardPerRigStored,
  };
  sync(g, now);
  return {
    config: {
      ...config,
      totalRigs: g.totalRigs + rigs,
      rewardLastUpdate: g.rewardLastUpdate,
      rewardPerRigStored: g.rewardPerRigStored,
    },
    miner: { rigs, rewardPerRigPaid: g.rewardPerRigStored, pendingRewards: 0n },
  };
}
