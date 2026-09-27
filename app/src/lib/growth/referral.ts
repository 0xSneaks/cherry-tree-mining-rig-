/**
 * Referral resolution — which referrer (if any) a first planting will bind.
 *
 * Product layer only. The program is the gate: `buy_rigs(count, referrer)`
 * binds a referrer once, at position creation, to an existing planter, never
 * to yourself, never on a later purchase (`rigs_engine::referral_weight`).
 * Nothing here changes those rules or the 100 / 105 weights. This module only
 * decides what the app proposes, and says so before the user signs.
 *
 * Precedence, strictly:
 *
 * 1. A wallet that already has a position binds nothing — its referrer (or
 *    lack of one) is permanent. No replacement.
 * 2. A referral the user supplied (`?ref=`) always wins. If it cannot bind
 *    (malformed, yourself, not a planter), the app sends no referrer and does
 *    NOT silently substitute the official fallback: the user supplied a
 *    choice, and swapping it would be a replacement in disguise.
 * 3. Only when nothing was supplied and nothing is bound: the official Cherry
 *    Tree referral, if configured AND verified as an existing planter on
 *    chain. Unconfigured or unverifiable fails closed to no referral — a
 *    non-planter referrer would make the whole purchase fail on chain.
 *
 * Like any direct referrer, the official referrer accrues the on-chain
 * referral commission (architecture §7a item 4: up to 20% of eligible fees,
 * 8% treasury margin, one level). The planter gets the existing 1.05× weight
 * on the same pre-funded emission — no new emission, no levels.
 */
import { isAddress, type Address } from '@solana/kit';

/** `Pubkey::default()` / the system program — never a valid referrer. */
const DEFAULT_PUBKEY = '11111111111111111111111111111111';

export type OfficialReferrerConfig =
  | { status: 'configured'; wallet: Address }
  | { status: 'unconfigured' }
  | { status: 'invalid'; raw: string };

/**
 * Parse `NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER`. No value is baked into
 * this repository: the official wallet is a founder decision, supplied as
 * deployment configuration. Anything that is not a plain base58 address
 * (or is the default key) is `invalid`, and invalid behaves as unconfigured.
 */
export function parseOfficialReferrer(raw: string | undefined | null): OfficialReferrerConfig {
  const value = (raw ?? '').trim();
  if (value === '') return { status: 'unconfigured' };
  if (!isAddress(value) || value === DEFAULT_PUBKEY) return { status: 'invalid', raw: value };
  return { status: 'configured', wallet: value as Address };
}

export const OFFICIAL_REFERRER: OfficialReferrerConfig = parseOfficialReferrer(
  process.env.NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER,
);

/** On-chain planter status of a candidate referrer: `null` = not yet known. */
export type PlanterStatus = boolean | null;

export type ReferralResolution =
  /** Position exists: nothing is sent; the binding (or none) is permanent. */
  | { kind: 'alreadyBound'; referrer: Address | null }
  /** User-supplied and sendable. `verified` is false while the check runs. */
  | { kind: 'explicit'; referrer: Address; verified: boolean }
  /** User-supplied but cannot bind. Nothing is sent, and no fallback. */
  | { kind: 'explicitInvalid'; raw: string; reason: 'malformed' | 'self' | 'notPlanter' }
  /** Nothing supplied: the official Cherry Tree referral, verified on chain. */
  | { kind: 'official'; referrer: Address }
  /** Nothing will bind. */
  | {
      kind: 'none';
      reason:
        | 'noFallbackConfigured'
        | 'fallbackIsSelf'
        | 'fallbackNotPlanter'
        | 'fallbackUnverified'
        | 'noWallet';
    };

export type ReferralInputs = {
  /** Raw `?ref=` value, or null when the launch URL carried none. */
  supplied: string | null;
  /** The connected wallet. */
  self: Address | null;
  /** True once the wallet has a Miner position (any purchase). */
  hasPosition: boolean;
  /** The bound referrer from the Miner account, when it has one. */
  boundReferrer?: Address | null;
  official: OfficialReferrerConfig;
  suppliedIsPlanter: PlanterStatus;
  officialIsPlanter: PlanterStatus;
};

/** Read `?ref=` from a query string; empty is the same as absent. */
export function suppliedReferral(search: string): string | null {
  const raw = new URLSearchParams(search).get('ref');
  if (raw === null) return null;
  const trimmed = raw.trim();
  return trimmed === '' ? null : trimmed;
}

export function resolveReferral(i: ReferralInputs): ReferralResolution {
  if (i.hasPosition) {
    const bound = i.boundReferrer && i.boundReferrer !== DEFAULT_PUBKEY ? i.boundReferrer : null;
    return { kind: 'alreadyBound', referrer: bound };
  }

  if (i.supplied !== null) {
    if (!isAddress(i.supplied) || i.supplied === DEFAULT_PUBKEY) {
      return { kind: 'explicitInvalid', raw: i.supplied, reason: 'malformed' };
    }
    if (i.self !== null && i.supplied === i.self) {
      return { kind: 'explicitInvalid', raw: i.supplied, reason: 'self' };
    }
    if (i.suppliedIsPlanter === false) {
      return { kind: 'explicitInvalid', raw: i.supplied, reason: 'notPlanter' };
    }
    return {
      kind: 'explicit',
      referrer: i.supplied as Address,
      verified: i.suppliedIsPlanter === true,
    };
  }

  if (i.self === null) return { kind: 'none', reason: 'noWallet' };
  if (i.official.status !== 'configured') return { kind: 'none', reason: 'noFallbackConfigured' };
  if (i.official.wallet === i.self) return { kind: 'none', reason: 'fallbackIsSelf' };
  if (i.officialIsPlanter === false) return { kind: 'none', reason: 'fallbackNotPlanter' };
  // Fail closed: the fallback is only ever sent once verified on chain.
  if (i.officialIsPlanter !== true) return { kind: 'none', reason: 'fallbackUnverified' };
  return { kind: 'official', referrer: i.official.wallet };
}

/** The referrer the buy transaction carries, or null. */
export function referrerToSend(r: ReferralResolution): Address | null {
  switch (r.kind) {
    case 'explicit':
    case 'official':
      return r.referrer;
    default:
      return null;
  }
}

/** Which wallet's planter status the resolver needs, for the RPC lookup. */
export function referralCandidates(i: {
  supplied: string | null;
  self: Address | null;
  hasPosition: boolean;
  official: OfficialReferrerConfig;
}): { supplied: Address | null; official: Address | null } {
  if (i.hasPosition) return { supplied: null, official: null };
  if (i.supplied !== null) {
    const ok = isAddress(i.supplied) && i.supplied !== DEFAULT_PUBKEY && i.supplied !== i.self;
    return { supplied: ok ? (i.supplied as Address) : null, official: null };
  }
  if (i.official.status === 'configured' && i.official.wallet !== i.self) {
    return { supplied: null, official: i.official.wallet };
  }
  return { supplied: null, official: null };
}

function short(a: string): string {
  return a.length <= 10 ? a : `${a.slice(0, 4)}…${a.slice(-4)}`;
}

/**
 * The plain-language disclosure shown before the user signs. `null` means
 * there is nothing to disclose (no wallet, or nothing supplied and no
 * fallback configured).
 */
export function referralDisclosure(r: ReferralResolution): string | null {
  switch (r.kind) {
    case 'alreadyBound':
      return null;
    case 'explicit':
      return (
        `Referred by ${short(r.referrer)}: your first planting binds this referrer permanently ` +
        `and plants at a 1.05× reward weight${r.verified ? '' : ', if the referrer is an existing planter'}. ` +
        'It takes a larger slice of the same funded emission — not extra APR. The referrer is paid nothing.'
      );
    case 'explicitInvalid':
      return r.reason === 'notPlanter'
        ? `Referred by ${short(r.raw)} — not a current planter, so this link cannot bind. ` +
            'Planting now binds no referrer, permanently. The official fallback is not substituted for a link you chose.'
        : 'This referral link is not valid ' +
            (r.reason === 'self' ? '(it is your own wallet)' : '(not a wallet address)') +
            '. Planting now binds no referrer, permanently.';
    case 'official':
      return (
        `No referral link was supplied, so this first planting uses the official Cherry Tree referral ` +
        `(${short(r.referrer)}). It binds permanently and plants at the standard 1.05× referred weight ` +
        'on the same funded emission. The official referrer receives nothing. ' +
        'Open a planter’s referral link before your first planting to use theirs instead.'
      );
    case 'none':
      switch (r.reason) {
        case 'fallbackNotPlanter':
        case 'fallbackUnverified':
          return 'No referral will be bound on this planting. Open a planter’s referral link first to plant at 1.05× weight.';
        default:
          return null;
      }
  }
}
