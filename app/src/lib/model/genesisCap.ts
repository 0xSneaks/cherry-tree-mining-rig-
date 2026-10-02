/**
 * Genesis-cap / concentration simulator — a DECISION MODEL, not protocol code.
 *
 * Models the qualified V2 rule exactly:
 *
 *   during [T0, T0 + 24h): lifetime_fresh_seeds(wallet) + request <= CAP
 *   founder activation counts toward the founder's own cap
 *   compounding never counts; at T0 + 24h the cap disappears (Public)
 *
 * and asks: for a candidate CAP, how concentrated can Genesis end up, in
 * particular for one actor controlling k wallets (the cap is per WALLET;
 * nothing on chain links wallets to people), and how much does Public demand
 * dilute the Genesis cohort afterwards?
 *
 * Deterministic by construction: demand is drawn from fixed Pareto quantiles
 * (no randomness), so the same inputs always print the same table. Every
 * population / participation / demand number is an ASSUMPTION supplied by the
 * caller — nothing here is measured data about .skr holders.
 */

export type DemandProfile = {
  /** Smallest desired fresh purchase, seeds. */
  minSeeds: number;
  /** Pareto tail index; lower = heavier tail (more whales). 1.16 ≈ "80/20". */
  alpha: number;
  /** Budget ceiling on any one ordinary wallet's desire, seeds. */
  maxSeeds: number;
};

export type Scenario = {
  name: string;
  /** Plausible .skr-eligible wallets (ASSUMPTION). */
  eligibleWallets: number;
  /** Fraction of eligible wallets that plant during Genesis (ASSUMPTION). */
  participation: number;
  demand: DemandProfile;
  /** Founder activation seeds (counts against the founder's cap). */
  founderSeeds: number;
  /** One strategic actor: wallets controlled and total seeds wanted. */
  actorWallets: number;
  actorDesiredSeeds: number;
  /** Genesis emission converted to seeds by compounding during the 24h (seeds). */
  genesisCompoundSeeds: number;
  /** Fraction of ordinary participants who replant during Genesis. */
  othersCompoundFraction: number;
  /** Public fresh demand after T0+24h, in seeds (low / medium / high ASSUMPTIONS). */
  publicDemandSeeds: readonly number[];
  /** Whether the actor tops up (uncapped) in Public to its desired size. */
  actorTopsUpInPublic: boolean;
};

export type CapResult = {
  cap: number | null; // null = no cap (reference)
  participants: number;
  /** Seeds bought during Genesis by all wallets incl. founder and actor. */
  genesisFreshSeeds: number;
  /** Share (bps) of ordinary participants whose demand exceeded the cap. */
  capBindingBps: number;
  /** Seeds of Genesis demand the cap deferred to Public (oversubscription signal). */
  deferredSeeds: number;
  largestOrdinaryWalletBps: number;
  actorBps: number;
  founderBps: number;
  top10WalletsBps: number;
  hhi: number;
  /** Actor's share after Genesis compounding. */
  actorBpsAfterCompounding: number;
  /** Per public-demand multiple: Genesis cohort share and actor share after Public. */
  afterPublic: { publicSeeds: number; genesisCohortBps: number; actorBps: number }[];
};

/** Deterministic Pareto quantile demand for `n` wallets, descending. */
export function demandCurve(n: number, d: DemandProfile): number[] {
  if (!Number.isInteger(n) || n < 0) throw new RangeError('n must be a non-negative integer');
  if (!(d.alpha > 0) || d.minSeeds < 1 || d.maxSeeds < d.minSeeds) throw new RangeError('bad demand profile');
  const out: number[] = [];
  for (let i = 0; i < n; i++) {
    // Survival quantile u = (i + 0.5) / n  →  x = xm · u^(−1/α).
    const u = (i + 0.5) / n;
    const x = Math.floor(d.minSeeds * Math.pow(u, -1 / d.alpha));
    out.push(Math.min(d.maxSeeds, Math.max(d.minSeeds, x)));
  }
  return out;
}

function bps(part: number, total: number): number {
  return total <= 0 ? 0 : Math.floor((part * 10_000) / total);
}

export function simulateCap(s: Scenario, cap: number | null): CapResult {
  if (cap !== null && (!Number.isInteger(cap) || cap < 1)) throw new RangeError('cap must be a positive integer');
  if (s.participation < 0 || s.participation > 1) throw new RangeError('participation must be in [0, 1]');
  if (!Number.isInteger(s.actorWallets) || s.actorWallets < 1) throw new RangeError('actorWallets must be >= 1');
  const limit = (want: number) => (cap === null ? want : Math.min(want, cap));

  const participants = Math.round(s.eligibleWallets * s.participation);
  const demand = demandCurve(participants, s.demand);
  const ordinary = demand.map(limit);
  const binding = demand.filter((d) => cap !== null && d > cap).length;
  const deferredOrdinary = demand.reduce((acc, d, i) => acc + (d - ordinary[i]!), 0);

  // Founder activation is a fresh purchase under its own cap (A <= C).
  const founder = limit(s.founderSeeds);
  // The actor spreads its demand across k wallets, each capped.
  const perWallet = Math.ceil(s.actorDesiredSeeds / s.actorWallets);
  const actorWallets = Array.from({ length: s.actorWallets }, () => limit(perWallet));
  const actor = Math.min(s.actorDesiredSeeds, actorWallets.reduce((a, b) => a + b, 0));
  const deferred = deferredOrdinary + (s.actorDesiredSeeds - actor);

  const ordinaryTotal = ordinary.reduce((a, b) => a + b, 0);
  const total = ordinaryTotal + founder + actor;

  // Wallet-level concentration (the actor's wallets appear as separate rows).
  const walletRows = [...ordinary, founder, ...actorWallets.map((w) => Math.min(w, actor))].sort((a, b) => b - a);
  const top10 = walletRows.slice(0, 10).reduce((a, b) => a + b, 0);
  const hhi = total <= 0 ? 0 : Math.floor(walletRows.reduce((acc, w) => acc + (w * w * 10_000) / (total * total), 0));

  // Genesis compounding: emission converted to seeds pro rata by share among
  // those who replant. The actor always replants; the founder is modelled as
  // replanting too; ordinary wallets replant with `othersCompoundFraction`.
  const compoundingBase = actor + founder + ordinaryTotal * s.othersCompoundFraction;
  const actorCompound = compoundingBase > 0 ? (s.genesisCompoundSeeds * actor) / compoundingBase : 0;
  const totalAfterCompound = total + (compoundingBase > 0 ? s.genesisCompoundSeeds : 0);
  const actorAfterCompound = actor + actorCompound;

  const afterPublic = s.publicDemandSeeds.map((publicSeeds) => {
    const topUp = s.actorTopsUpInPublic ? s.actorDesiredSeeds - actor : 0;
    const grand = totalAfterCompound + publicSeeds + topUp;
    return {
      publicSeeds,
      genesisCohortBps: bps(totalAfterCompound, grand),
      actorBps: bps(actorAfterCompound + topUp, grand),
    };
  });

  return {
    cap,
    participants,
    genesisFreshSeeds: total,
    capBindingBps: bps(binding, participants),
    deferredSeeds: deferred,
    largestOrdinaryWalletBps: bps(ordinary[0] ?? 0, total),
    actorBps: bps(actor, total),
    founderBps: bps(founder, total),
    top10WalletsBps: bps(top10, total),
    hhi,
    actorBpsAfterCompounding: bps(actorAfterCompound, totalAfterCompound),
    afterPublic,
  };
}

/**
 * Largest candidate cap whose multi-wallet actor share stays at or below
 * `targetBps` in this scenario, or null if even the smallest candidate fails.
 */
export function maxCapForTarget(s: Scenario, candidates: readonly number[], targetBps: number): number | null {
  let best: number | null = null;
  for (const c of [...candidates].sort((a, b) => a - b)) {
    if (simulateCap(s, c).actorBps <= targetBps) best = c;
  }
  return best;
}

// ---------------------------------------------------------------- presets
// All ASSUMPTIONS, chosen to bracket plausible outcomes, not to predict them.

export const CANDIDATE_CAPS = [10, 25, 50, 100, 250, 500, 1_000, 2_500, 5_000] as const;
export const ACTOR_WALLET_COUNTS = [1, 2, 3, 5, 10] as const;

export const ELIGIBLE_POPULATIONS = [2_000, 10_000, 50_000, 150_000] as const;

export const PARTICIPATION_LEVELS = [
  { name: 'low', participation: 0.005 },
  { name: 'expected', participation: 0.02 },
  { name: 'high', participation: 0.06 },
  { name: 'very high', participation: 0.15 },
] as const;

export const DEFAULT_DEMAND: DemandProfile = { minSeeds: 5, alpha: 1.16, maxSeeds: 100_000 };

export function presetScenario(over: Partial<Scenario> = {}): Scenario {
  return {
    name: 'expected',
    eligibleWallets: 10_000,
    participation: 0.02,
    demand: DEFAULT_DEMAND,
    founderSeeds: 10,
    actorWallets: 5,
    actorDesiredSeeds: 50_000,
    genesisCompoundSeeds: 0,
    othersCompoundFraction: 0.5,
    publicDemandSeeds: [10_000, 100_000, 1_000_000],
    actorTopsUpInPublic: false,
    ...over,
  };
}
