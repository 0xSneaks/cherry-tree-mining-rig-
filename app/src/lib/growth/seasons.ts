/**
 * Seasons — an OFF-CHAIN overlay of named time windows for competitions,
 * badges and separately funded prizes.
 *
 * A season is a view over immutable chain timestamps (a position's
 * `createdAt`, which is also its referral-binding time). Seasons never touch
 * a Miner account: when a season ends nothing resets, no seed is removed, no
 * weight changes and no purchase is prompted. Seasonal standings are computed
 * fresh from permanent state every time; there is no seasonal balance to zero.
 *
 * Prizes are metadata only. They must be funded from a disclosed source that
 * is NOT the program's reward, capital or fee vault — a prize paid from the
 * reward vault would take emission from every other planter, and one paid
 * from purchase capital would be the deposit-funded loop this project exists
 * to avoid. `validateSeasons` rejects both.
 */

export type PrizeMetadata = {
  /** What is awarded, in plain words. */
  description: string;
  /** Always 'separately-funded'. Present so the rule is explicit in data. */
  fundingSource: 'separately-funded';
  /** Public key or named entity paying the prize. */
  funder: string;
  /** Where the funding and eligibility are publicly disclosed. */
  disclosureUrl: string;
};

export type Competition = 'referrals' | 'newPlanters' | 'replanters';

export type Season = {
  /** Stable slug: lowercase letters, digits and dashes. */
  id: string;
  name: string;
  /** Unix seconds, inclusive. */
  startsAt: bigint;
  /** Unix seconds, exclusive. */
  endsAt: bigint;
  competitions: readonly Competition[];
  /** Seconds after `endsAt` during which pending referrals may still qualify. */
  qualificationGraceSeconds: bigint;
  prizes: readonly PrizeMetadata[];
};

export type SeasonState = 'upcoming' | 'active' | 'ended';

export function seasonState(s: Pick<Season, 'startsAt' | 'endsAt'>, now: bigint): SeasonState {
  if (now < s.startsAt) return 'upcoming';
  if (now < s.endsAt) return 'active';
  return 'ended';
}

export function inSeason(s: Pick<Season, 'startsAt' | 'endsAt'>, t: bigint): boolean {
  return t >= s.startsAt && t < s.endsAt;
}

/** The moment seasonal qualification is judged: never later than end + grace. */
export function seasonEvaluationTime(
  s: Pick<Season, 'endsAt' | 'qualificationGraceSeconds'>,
  now: bigint,
): bigint {
  const cutoff = s.endsAt + s.qualificationGraceSeconds;
  return now < cutoff ? now : cutoff;
}

export function currentSeason(seasons: readonly Season[], now: bigint): Season | null {
  return seasons.find((s) => seasonState(s, now) === 'active') ?? null;
}

export function pastSeasons(seasons: readonly Season[], now: bigint): Season[] {
  return seasons
    .filter((s) => seasonState(s, now) === 'ended')
    .sort((a, b) => (a.endsAt > b.endsAt ? -1 : a.endsAt < b.endsAt ? 1 : 0));
}

const ID = /^[a-z0-9](?:[a-z0-9-]{0,30}[a-z0-9])?$/;
const MAX_SEASON_SECONDS = 366n * 86_400n;

/**
 * Validate a season registry. `protocolVaults` are the reward, capital and fee
 * vault addresses (and any other program-owned account) — a prize funder may
 * never be one of them.
 */
export function validateSeasons(seasons: readonly Season[], protocolVaults: readonly string[]): string[] {
  const errors: string[] = [];
  const ids = new Set<string>();
  const forbidden = new Set(protocolVaults);
  for (const s of seasons) {
    if (!ID.test(s.id)) errors.push(`${s.id}: id must be a lowercase slug`);
    if (ids.has(s.id)) errors.push(`${s.id}: duplicate id`);
    ids.add(s.id);
    if (s.name.trim() === '') errors.push(`${s.id}: name is required`);
    if (s.endsAt <= s.startsAt) errors.push(`${s.id}: endsAt must be after startsAt`);
    if (s.endsAt - s.startsAt > MAX_SEASON_SECONDS) errors.push(`${s.id}: longer than 366 days`);
    if (s.qualificationGraceSeconds < 0n) errors.push(`${s.id}: negative qualification grace`);
    if (s.competitions.length === 0) errors.push(`${s.id}: no competitions`);
    for (const p of s.prizes) {
      if (p.fundingSource !== 'separately-funded') {
        errors.push(`${s.id}: prize must be separately funded`);
      }
      if (forbidden.has(p.funder)) {
        errors.push(`${s.id}: prize funder ${p.funder} is a protocol vault — prizes may never come from program vaults`);
      }
      if (p.funder.trim() === '') errors.push(`${s.id}: prize funder is required`);
      if (!/^https:\/\//.test(p.disclosureUrl)) errors.push(`${s.id}: prize needs an https disclosure URL`);
      if (/\b(guarantee|guaranteed|roi|risk[- ]free)\b/i.test(p.description)) {
        errors.push(`${s.id}: prize description must not promise returns`);
      }
    }
  }
  const sorted = [...seasons].sort((a, b) => (a.startsAt < b.startsAt ? -1 : a.startsAt > b.startsAt ? 1 : 0));
  for (let i = 1; i < sorted.length; i++) {
    if (sorted[i]!.startsAt < sorted[i - 1]!.endsAt) {
      errors.push(`${sorted[i]!.id}: overlaps ${sorted[i - 1]!.id}`);
    }
  }
  return errors;
}

/** JSON form used by configuration (bigints as decimal strings). */
export type SeasonJson = {
  id: string;
  name: string;
  startsAt: string;
  endsAt: string;
  competitions: Competition[];
  qualificationGraceSeconds?: string;
  prizes?: PrizeMetadata[];
};

export function parseSeasons(json: string | undefined | null): Season[] {
  if (!json || json.trim() === '') return [];
  const raw = JSON.parse(json) as SeasonJson[];
  if (!Array.isArray(raw)) throw new Error('seasons config must be a JSON array');
  const num = (v: string, what: string) => {
    if (!/^\d+$/.test(v)) throw new Error(`${what} must be a decimal integer string`);
    return BigInt(v);
  };
  return raw.map((s) => ({
    id: s.id,
    name: s.name,
    startsAt: num(s.startsAt, `${s.id}.startsAt`),
    endsAt: num(s.endsAt, `${s.id}.endsAt`),
    competitions: s.competitions,
    qualificationGraceSeconds: num(s.qualificationGraceSeconds ?? '0', `${s.id}.qualificationGraceSeconds`),
    prizes: s.prizes ?? [],
  }));
}
