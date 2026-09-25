/**
 * Production reward-funding model — a DECISION MODEL, not protocol code.
 *
 * Mirrors the program's funding rule exactly (`RewardGlobals::fund`):
 *
 *   leftover  = now < finish ? (finish − now) × rate : 0
 *   rate      = (leftover + amount) / duration      (integer, floor)
 *   finish    = now + duration
 *
 * Reward funding is ONLY external, non-purchase SOL deposited by the reward
 * funder. Purchase capital and purchase fees are never modelled as reward
 * funding, because the program makes that impossible.
 *
 * Output figures are the rate the accumulator would pay at a given planted
 * level — "APR at that level" — never a return anyone will receive. Every
 * participation figure is an ASSUMPTION supplied by the caller.
 */

export const LAMPORTS_PER_SOL = 1_000_000_000n;
export const DAY = 86_400n;
export const YEAR = 31_536_000n;

export type Schedule = { rate: bigint; finish: bigint };

/** Exact port of the on-chain funding rule. */
export function fund(s: Schedule, now: bigint, amountLamports: bigint, durationSeconds: bigint): Schedule {
  if (amountLamports <= 0n) throw new RangeError('amount must be > 0');
  if (durationSeconds <= 0n || durationSeconds > 10n * 365n * DAY) throw new RangeError('invalid duration');
  const leftover = now < s.finish ? (s.finish - now) * s.rate : 0n;
  const rate = (leftover + amountLamports) / durationSeconds;
  if (rate === 0n) throw new RangeError('rate would be zero');
  return { rate, finish: now + durationSeconds };
}

/**
 * What a proposed funding does to the live schedule. Operators must look at
 * this BEFORE funding: a small top-up spread over a long duration LOWERS the
 * current rate, because leftover and new amount are re-spread together.
 */
export function previewFunding(
  current: Schedule,
  now: bigint,
  amountLamports: bigint,
  durationSeconds: bigint,
): { next: Schedule; rateChangeBps: number | null; leftoverLamports: bigint; lowersRate: boolean } {
  const next = fund(current, now, amountLamports, durationSeconds);
  const live = now < current.finish && current.rate > 0n;
  const leftover = live ? (current.finish - now) * current.rate : 0n;
  const rateChangeBps = live ? Number(((next.rate - current.rate) * 10_000n) / current.rate) : null;
  return { next, rateChangeBps, leftoverLamports: leftover, lowersRate: live && next.rate < current.rate };
}

/** Rate in bps/year at `plantedSeeds` (fee-inclusive price), 0 with no seeds. */
export function aprBpsAt(rate: bigint, plantedSeeds: bigint, priceLamports: bigint, feeBps: bigint): bigint {
  if (plantedSeeds <= 0n || rate <= 0n) return 0n;
  return (rate * YEAR * 10_000n * 10_000n) / (plantedSeeds * priceLamports * (10_000n + feeBps));
}

export type FundingScenario = {
  amountSol: bigint;
  durationDays: bigint;
  priceLamports: bigint;
  feeBps: bigint;
  /** Planted seeds levels to evaluate (ASSUMPTIONS about participation). */
  plantedSeeds: readonly bigint[];
};

export type FundingRow = {
  amountSol: bigint;
  durationDays: bigint;
  rateLamportsPerSecond: bigint;
  emissionPerDayLamports: bigint;
  /** Per planted level: APR bps and emission per seed per day (lamports). */
  levels: { seeds: bigint; aprBps: bigint; perSeedPerDayLamports: bigint }[];
};

export function fundingRow(s: FundingScenario): FundingRow {
  const sched = fund({ rate: 0n, finish: 0n }, 0n, s.amountSol * LAMPORTS_PER_SOL, s.durationDays * DAY);
  return {
    amountSol: s.amountSol,
    durationDays: s.durationDays,
    rateLamportsPerSecond: sched.rate,
    emissionPerDayLamports: sched.rate * DAY,
    levels: s.plantedSeeds.map((seeds) => ({
      seeds,
      aprBps: aprBpsAt(sched.rate, seeds, s.priceLamports, s.feeBps),
      perSeedPerDayLamports: seeds > 0n ? (sched.rate * DAY) / seeds : 0n,
    })),
  };
}

/**
 * Required funding to hold `targetAprBps` at `plantedSeeds` for `days`:
 * `target × planted_capital × days / 365` (fee-inclusive capital).
 */
export function requiredFundingLamports(
  targetAprBps: bigint,
  plantedSeeds: bigint,
  priceLamports: bigint,
  feeBps: bigint,
  days: bigint,
): bigint {
  const capital = (plantedSeeds * priceLamports * (10_000n + feeBps)) / 10_000n;
  return (targetAprBps * capital * days * DAY) / (10_000n * YEAR);
}

export type RefillPlan = {
  /** Initial funding. */
  initialSol: bigint;
  initialDays: bigint;
  /** External refills: at day `atDay`, fund `sol` over `days`. */
  refills: readonly { atDay: bigint; sol: bigint; days: bigint }[];
  /** Evaluate the rate on these days. */
  sampleDays: readonly bigint[];
};

/**
 * Walk a schedule of initial funding + external refills with the exact
 * on-chain rule. Reports the rate and remaining runway on each sample day and
 * the day emission stops (Dormant) if it does within the horizon.
 */
export function simulateRefills(p: RefillPlan): {
  samples: { day: bigint; rate: bigint; runwayDays: bigint; dormant: boolean }[];
  dormantFromDay: bigint | null;
  totalFundedSol: bigint;
} {
  let sched = fund({ rate: 0n, finish: 0n }, 0n, p.initialSol * LAMPORTS_PER_SOL, p.initialDays * DAY);
  const refills = [...p.refills].sort((a, b) => (a.atDay < b.atDay ? -1 : a.atDay > b.atDay ? 1 : 0));
  let ri = 0;
  let total = p.initialSol;
  const samples: { day: bigint; rate: bigint; runwayDays: bigint; dormant: boolean }[] = [];
  const horizon = [...p.sampleDays].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  let dormantFrom: bigint | null = null;
  for (const day of horizon) {
    while (ri < refills.length && refills[ri]!.atDay <= day) {
      const r = refills[ri]!;
      if (dormantFrom !== null && r.atDay * DAY >= sched.finish) dormantFrom = null;
      sched = fund(sched, r.atDay * DAY, r.sol * LAMPORTS_PER_SOL, r.days * DAY);
      total += r.sol;
      ri++;
    }
    const now = day * DAY;
    const dormant = now >= sched.finish;
    if (dormant && dormantFrom === null) dormantFrom = sched.finish / DAY;
    samples.push({
      day,
      rate: dormant ? 0n : sched.rate,
      runwayDays: dormant ? 0n : (sched.finish - now) / DAY,
      dormant,
    });
  }
  return { samples, dormantFromDay: dormantFrom, totalFundedSol: total };
}
