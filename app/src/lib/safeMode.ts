/**
 * Safe Mode mirror (architecture section 9, canonical tests 34–39).
 *
 * TypeScript port of `crates/eligibility-engine/src/safe_mode.rs` and
 * `src/consent.rs`: zone classification, the CR = 1 leverage singularity,
 * risk-consent validation and route availability. Exact `bigint` math only —
 * zone boundaries such as CR = 1.35 / 1.20 / 1.00 are compared by
 * cross-multiplication, never through floating point.
 *
 * Boundaries, confidence values and dwell durations are configuration
 * inputs calibrated per deployed pool version; the fixture constants here
 * mirror the architecture sheet and are NOT production values.
 */

/**
 * Mirrors `SAFE_MODE_CONSENT_DISCLOSURE` in
 * `crates/eligibility-engine/src/lib.rs` verbatim; the Rust test in
 * `tests/pricing_safe_mode.rs` fails if the two drift.
 */
export const SAFE_MODE_CONSENT_DISCLOSURE =
  'Risk zones: entering or increasing exposure in WARM or HOT collateral states requires a signed risk acknowledgement covering your wallet, the asset, the action and amount, the chain, the epoch and the risk-disclosure version, single-use and expiring. A signature is consent to risk, not permission to move funds, and it can never unlock an operation whose required protocol route is unavailable.';

/** Exact non-negative rational `num / den` (`den > 0`), unnormalized. */
export type Ratio = { num: bigint; den: bigint };

export type RiskZone = 'normal' | 'warm' | 'hot' | 'extreme' | 'unknown';

/** Zone boundaries. Production reads the deployed pool version. */
export type SafeModeBoundaries = { warmBelow: Ratio; hotBelow: Ratio };

/**
 * Fixture xSOL boundaries equal to the architecture sheet (NORMAL >= 1.35,
 * WARM >= 1.20, HOT >= 1.00, EXTREME below). NOT production constants.
 */
export const FIXTURE_XSOL_BOUNDARIES: SafeModeBoundaries = {
  warmBelow: { num: 135n, den: 100n },
  hotBelow: { num: 120n, den: 100n },
};

function le(a: Ratio, b: Ratio): boolean {
  if (a.den === 0n || b.den === 0n) return false;
  return a.num * b.den <= b.num * a.den;
}

function ge(a: Ratio, b: Ratio): boolean {
  return le(b, a);
}

/** Classify one verified CR observation; missing/stale data is `unknown`. */
export function zoneForCr(cr: Ratio | null, b: SafeModeBoundaries): RiskZone {
  if (cr === null || cr.den === 0n) return 'unknown';
  if (ge(cr, b.warmBelow)) return 'normal';
  if (ge(cr, b.hotBelow)) return 'warm';
  if (ge(cr, { num: 1n, den: 1n })) return 'hot';
  return 'extreme';
}

/**
 * `leverage = CR / (CR - 1)`, defined only for CR > 1. CR = 1 is singular
 * and CR < 1 is invalid: both return `null` (test 38).
 */
export function leverageForCr(cr: Ratio): Ratio | null {
  if (cr.den === 0n || cr.num <= cr.den) return null;
  return { num: cr.num, den: cr.num - cr.den };
}

/**
 * Product rendering of leverage: never a finite value at or below CR = 1
 * (test 38's display rule). Above 1 it renders the exact rational,
 * gcd-reduced for display only.
 */
export function formatLeverage(cr: Ratio): string {
  const l = leverageForCr(cr);
  if (l === null) return '—';
  let { num, den } = l;
  let a = num;
  let b = den;
  while (b !== 0n) {
    const t = a % b;
    a = b;
    b = t;
  }
  if (a > 1n) {
    num /= a;
    den /= a;
  }
  return `${num}/${den}`;
}

// ---------------------------------------------------------------------
// Route availability (tests 35, 39)
// ---------------------------------------------------------------------

export type QuoteStatus = 'ok' | 'failed';

/** Protocol path state + DEX quote outcome. Availability is protocol-only. */
export type RouteCheck = {
  protocolRouteHalted: boolean;
  dexQuote: QuoteStatus;
};

/**
 * Route availability means the documented protocol mint/redeem/unstake/
 * withdraw path is operational per on-chain state. A DEX quote failure or
 * thin pool depth alone is NOT route unavailability (test 39).
 */
export function routeAvailable(route: RouteCheck): boolean {
  return !route.protocolRouteHalted;
}

// ---------------------------------------------------------------------
// Risk consent (test 34)
// ---------------------------------------------------------------------

export type ConsentAction = 'enter' | 'increase';

/** Chain identity the consent binds to (fixture here, genesis hash in production). */
export type ChainId = bigint;

export const FIXTURE_CHAIN_ID: ChainId = 0xf1n;

export type RiskConsent = {
  wallet: string;
  asset: 'skr' | 'xsol';
  action: ConsentAction;
  /** Scope ceiling in lamports. */
  amountLamports: bigint;
  chain: ChainId;
  epoch: bigint;
  riskVersion: bigint;
  nonce: bigint;
  /** Valid while `now < expiresAt` (unix seconds). */
  expiresAt: bigint;
  /** Wallet signature over `consentPayload` (hex). */
  signature: string;
};

export type ConsentRequest = {
  wallet: string;
  asset: RiskConsent['asset'];
  action: ConsentAction;
  amountLamports: bigint;
  chain: ChainId;
  epoch: bigint;
  riskVersion: bigint;
  now: bigint;
};

export type ConsentError =
  | 'missing'
  | 'expired'
  | 'wrong-wallet'
  | 'wrong-asset'
  | 'wrong-action'
  | 'wrong-scope'
  | 'wrong-chain'
  | 'wrong-epoch'
  | 'wrong-version'
  | 'replayed-nonce'
  | 'bad-signature';

function u128LeBytes(n: bigint): bigint[] {
  const bytes: bigint[] = [];
  let x = n;
  for (let i = 0; i < 16; i += 1) {
    bytes.push(x & 0xffn);
    x >>= 8n;
  }
  return bytes;
}

/**
 * Canonical payload bytes (as a hex string) that the wallet signs. Mirrors
 * `RiskConsent::payload` in the Rust engine — every bound field serializes
 * in, so changing any field invalidates the binding (test 34).
 */
export function consentPayload(c: RiskConsent): string {
  const parts: string[] = ['CHERRY.RISK_CONSENT.v1'];
  // wallet: 32-byte address hex
  parts.push(c.wallet.replace(/^0x/, '').padStart(64, '0'));
  parts.push(c.asset === 'skr' ? '01' : '02');
  parts.push(c.action === 'enter' ? '01' : '02');
  const hex = (xs: bigint[]) => xs.map((b) => b.toString(16).padStart(2, '0')).join('');
  parts.push(hex(u128LeBytes(c.amountLamports)));
  parts.push(hex(u128LeBytes(c.chain)));
  parts.push(hex(u128LeBytes(c.epoch)));
  parts.push(hex(u128LeBytes(c.riskVersion)));
  parts.push(hex(u128LeBytes(c.nonce)));
  parts.push(hex(u128LeBytes(c.expiresAt)));
  return parts.join(':');
}

/** Single-use nonces per wallet. */
export class ConsentLedger {
  #used = new Set<string>();

  isUsed(wallet: string, nonce: bigint): boolean {
    return this.#used.has(`${wallet}:${nonce}`);
  }

  /** Validate; on success the nonce is burned (replay = rejection). */
  validate(
    request: ConsentRequest,
    consent: RiskConsent | null,
    verify: (wallet: string, payload: string, signature: string) => boolean,
  ): ConsentError | null {
    if (consent === null) return 'missing';
    if (consent.wallet !== request.wallet) return 'wrong-wallet';
    if (consent.asset !== request.asset) return 'wrong-asset';
    if (consent.action !== request.action) return 'wrong-action';
    if (consent.amountLamports < request.amountLamports) return 'wrong-scope';
    if (consent.chain !== request.chain) return 'wrong-chain';
    if (consent.epoch !== request.epoch) return 'wrong-epoch';
    if (consent.riskVersion !== request.riskVersion) return 'wrong-version';
    if (request.now >= consent.expiresAt) return 'expired';
    if (this.isUsed(consent.wallet, consent.nonce)) return 'replayed-nonce';
    if (!verify(consent.wallet, consentPayload(consent), consent.signature)) {
      return 'bad-signature';
    }
    this.#used.add(`${consent.wallet}:${consent.nonce}`);
    return null;
  }
}

// ---------------------------------------------------------------------
// Entry decision (tests 34, 35)
// ---------------------------------------------------------------------

export type EntryDecision =
  | { kind: 'permitted' }
  | { kind: 'permitted-with-consent' }
  | { kind: 'denied'; reason: 'unknown-risk-data' | 'route-unavailable' | ConsentError };

/**
 * Authorize a zone entry/increase. HOT is not automatically blocked: it
 * needs valid consent *and* an operational requested route (tests 34–35).
 * Severity (zone) and availability (route) are separate: WARM's requirement
 * is the acknowledgement alone.
 */
export function authorizeEntry(
  zone: RiskZone,
  request: ConsentRequest,
  consent: RiskConsent | null,
  route: RouteCheck,
  ledger: ConsentLedger,
  verify: (wallet: string, payload: string, signature: string) => boolean,
): EntryDecision {
  switch (zone) {
    case 'normal':
      return { kind: 'permitted' };
    case 'unknown':
      return { kind: 'denied', reason: 'unknown-risk-data' };
    case 'warm': {
      const err = ledger.validate(request, consent, verify);
      return err === null
        ? { kind: 'permitted-with-consent' }
        : { kind: 'denied', reason: err };
    }
    case 'hot':
    case 'extreme': {
      if (!routeAvailable(route)) return { kind: 'denied', reason: 'route-unavailable' };
      const err = ledger.validate(request, consent, verify);
      return err === null
        ? { kind: 'permitted-with-consent' }
        : { kind: 'denied', reason: err };
    }
  }
}
