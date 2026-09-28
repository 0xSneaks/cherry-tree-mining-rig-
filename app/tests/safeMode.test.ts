/**
 * Safe Mode mirror tests (architecture section 9, canonical tests 34–39 —
 * TypeScript half). Exact bigint math; fixtures only.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import {
  ConsentLedger,
  FIXTURE_XSOL_BOUNDARIES,
  SAFE_MODE_CONSENT_DISCLOSURE,
  authorizeEntry,
  consentPayload,
  formatLeverage,
  leverageForCr,
  routeAvailable,
  zoneForCr,
  type ConsentRequest,
  type Ratio,
  type RiskConsent,
} from '../src/lib/safeMode';

const r = (num: bigint, den: bigint): Ratio => ({ num, den });
const ONE = r(1n, 1n);

const okRoute = { protocolRouteHalted: false, dexQuote: 'ok' as const };
const haltedRoute = { protocolRouteHalted: true, dexQuote: 'ok' as const };
const quoteFailedRoute = { protocolRouteHalted: false, dexQuote: 'failed' as const };

describe('safe-mode disclosure mirrors the engine', () => {
  it('matches SAFE_MODE_CONSENT_DISCLOSURE in lib.rs verbatim', () => {
    const rs = readFileSync(
      join(import.meta.dirname, '..', '..', 'crates', 'eligibility-engine', 'src', 'lib.rs'),
      'utf8',
    );
    const m = rs.match(/SAFE_MODE_CONSENT_DISCLOSURE: &str = "([^"]+)";/);
    expect(m?.[1]).toBe(SAFE_MODE_CONSENT_DISCLOSURE);
  });

  it('states consent is not permission to move funds', () => {
    expect(SAFE_MODE_CONSENT_DISCLOSURE).toContain('consent to risk');
    expect(SAFE_MODE_CONSENT_DISCLOSURE).toContain('never');
    expect(SAFE_MODE_CONSENT_DISCLOSURE).toContain('unavailable');
  });
});

describe('zone boundaries are exact (test 38 boundary table)', () => {
  const b = FIXTURE_XSOL_BOUNDARIES;
  it.each([
    [r(135n, 100n), 'normal'],
    [r(27n, 20n), 'normal'], // 1.35 unnormalized
    [r(136n, 100n), 'normal'],
    [r(134n, 100n), 'warm'],
    [r(120n, 100n), 'warm'],
    [r(119n, 100n), 'hot'],
    [r(101n, 100n), 'hot'],
    [r(100n, 100n), 'hot'], // CR = 1 exactly
    [r(99n, 100n), 'extreme'],
  ] as const)('%p -> %s', (cr, zone) => {
    expect(zoneForCr(cr, b)).toBe(zone);
  });

  it('missing or malformed data is unknown, never extreme', () => {
    expect(zoneForCr(null, b)).toBe('unknown');
    expect(zoneForCr(r(1n, 0n), b)).toBe('unknown');
  });
});

describe('CR = 1 never displays finite leverage (test 38)', () => {
  it('returns null at and below one', () => {
    expect(leverageForCr(ONE)).toBeNull();
    expect(leverageForCr(r(99n, 100n))).toBeNull();
    expect(leverageForCr(r(1n, 10n ** 18n))).toBeNull();
  });

  it('is exact just above one and at the zone boundaries', () => {
    const l = leverageForCr(r(1_001n, 1_000n))!;
    // leverage * (CR - 1) === CR, cross-multiplied.
    expect(l.num * (1_001n - 1_000n) === 1_001n * l.den).toBe(true);
    const at120 = leverageForCr(r(120n, 100n))!;
    expect(at120.num * 20n === 120n * at120.den).toBe(true);
    const at135 = leverageForCr(r(135n, 100n))!;
    expect(at135.num * 35n === 135n * at135.den).toBe(true);
  });

  it('formatLeverage never renders a finite value at CR <= 1', () => {
    expect(formatLeverage(ONE)).toBe('—');
    expect(formatLeverage(r(99n, 100n))).toBe('—');
    expect(formatLeverage(r(120n, 100n))).toBe('6/1');
  });
});

describe('route availability (tests 35, 39)', () => {
  it('DEX quote failure alone is not unavailability', () => {
    expect(routeAvailable(quoteFailedRoute)).toBe(true);
  });

  it('a halted protocol route is unavailable whatever the quote says', () => {
    expect(routeAvailable(haltedRoute)).toBe(false);
    expect(routeAvailable({ protocolRouteHalted: true, dexQuote: 'failed' })).toBe(false);
  });
});

describe('risk consent (test 34)', () => {
  const request: ConsentRequest = {
    wallet: 'W'.repeat(64),
    asset: 'xsol',
    action: 'enter',
    amountLamports: 1_000n,
    chain: 0xf1n,
    epoch: 7n,
    riskVersion: 3n,
    now: 1_000n,
  };

  const valid = (nonce = 1n): RiskConsent => ({
    wallet: request.wallet,
    asset: 'xsol',
    action: 'enter',
    amountLamports: 1_000n,
    chain: 0xf1n,
    epoch: 7n,
    riskVersion: 3n,
    nonce,
    expiresAt: 1_600n,
    signature: 'sig',
  });

  const verifySig = (_w: string, _p: string, s: string) => s === 'sig';

  it('accepts a fully bound fresh consent and burns its nonce', () => {
    const ledger = new ConsentLedger();
    expect(ledger.validate(request, valid(), verifySig)).toBeNull();
    expect(ledger.validate(request, valid(), verifySig)).toBe('replayed-nonce');
    // A different nonce is a different consent.
    expect(ledger.validate(request, valid(2n), verifySig)).toBeNull();
  });

  it.each([
    ['missing', null],
    ['expired', { ...valid(), expiresAt: 1_000n }],
    ['wrong-wallet', { ...valid(), wallet: 'X'.repeat(64) }],
    ['wrong-asset', { ...valid(), asset: 'skr' as const }],
    ['wrong-action', { ...valid(), action: 'increase' as const }],
    ['wrong-scope', { ...valid(), amountLamports: 999n }],
    ['wrong-chain', { ...valid(), chain: 0xeen }],
    ['wrong-epoch', { ...valid(), epoch: 8n }],
    ['wrong-version', { ...valid(), riskVersion: 4n }],
    ['bad-signature', { ...valid(), signature: 'other' }],
  ])('rejects %s', (_name, consent) => {
    expect(new ConsentLedger().validate(request, consent, verifySig)).toBe(_name);
  });

  it('binds every field into the signed payload', () => {
    const base = consentPayload(valid());
    const mutations: RiskConsent[] = [
      { ...valid(), wallet: 'X'.repeat(64) },
      { ...valid(), asset: 'skr' as const },
      { ...valid(), action: 'increase' as const },
      { ...valid(), amountLamports: 1_001n },
      { ...valid(), chain: 0xf2n },
      { ...valid(), epoch: 8n },
      { ...valid(), riskVersion: 4n },
      { ...valid(), nonce: 2n },
      { ...valid(), expiresAt: 1_601n },
    ];
    for (const m of mutations) {
      expect(consentPayload(m)).not.toBe(base);
    }
    expect(base.startsWith('CHERRY.RISK_CONSENT.v1:')).toBe(true);
  });
});

describe('entry decision (tests 34, 35)', () => {
  const request: ConsentRequest = {
    wallet: 'W'.repeat(64),
    asset: 'xsol',
    action: 'enter',
    amountLamports: 1_000n,
    chain: 0xf1n,
    epoch: 7n,
    riskVersion: 3n,
    now: 1_000n,
  };
  const consent: RiskConsent = {
    wallet: request.wallet,
    asset: 'xsol',
    action: 'enter',
    amountLamports: 1_000n,
    chain: 0xf1n,
    epoch: 7n,
    riskVersion: 3n,
    nonce: 1n,
    expiresAt: 1_600n,
    signature: 'sig',
  };
  const verify = (_w: string, _p: string, s: string) => s === 'sig';

  it('normal needs no consent; unknown pauses; warm/hot need consent', () => {
    expect(authorizeEntry('normal', request, null, okRoute, new ConsentLedger(), verify)).toEqual({
      kind: 'permitted',
    });
    expect(
      authorizeEntry('unknown', request, consent, okRoute, new ConsentLedger(), verify),
    ).toEqual({ kind: 'denied', reason: 'unknown-risk-data' });
    expect(authorizeEntry('warm', request, null, okRoute, new ConsentLedger(), verify)).toEqual({
      kind: 'denied',
      reason: 'missing',
    });
    expect(authorizeEntry('hot', request, consent, okRoute, new ConsentLedger(), verify)).toEqual({
      kind: 'permitted-with-consent',
    });
  });

  it('hot entry to an unavailable route fails even with valid consent', () => {
    expect(
      authorizeEntry('hot', request, consent, haltedRoute, new ConsentLedger(), verify),
    ).toEqual({ kind: 'denied', reason: 'route-unavailable' });
    expect(
      authorizeEntry('extreme', request, consent, haltedRoute, new ConsentLedger(), verify),
    ).toEqual({ kind: 'denied', reason: 'route-unavailable' });
  });

  it('a halted route denies entry in EVERY zone, not only HOT/EXTREME (F3)', () => {
    // Severity and availability are separate axes: a low zone never
    // authorizes an unavailable operation, and a signature repairs it
    // nowhere (section 9).
    expect(
      authorizeEntry('normal', request, null, haltedRoute, new ConsentLedger(), verify),
    ).toEqual({ kind: 'denied', reason: 'route-unavailable' });
    expect(
      authorizeEntry('normal', request, consent, haltedRoute, new ConsentLedger(), verify),
    ).toEqual({ kind: 'denied', reason: 'route-unavailable' });
    expect(
      authorizeEntry('warm', request, consent, haltedRoute, new ConsentLedger(), verify),
    ).toEqual({ kind: 'denied', reason: 'route-unavailable' });
    expect(
      authorizeEntry('warm', request, null, haltedRoute, new ConsentLedger(), verify),
    ).toEqual({ kind: 'denied', reason: 'route-unavailable' });
    // With the route operational the zone rules stand unchanged: NORMAL
    // needs no consent, WARM does.
    expect(authorizeEntry('normal', request, null, okRoute, new ConsentLedger(), verify)).toEqual({
      kind: 'permitted',
    });
    expect(authorizeEntry('warm', request, consent, okRoute, new ConsentLedger(), verify)).toEqual({
      kind: 'permitted-with-consent',
    });
  });

  it('dex quote failure with a healthy protocol route does not block hot entry', () => {
    expect(
      authorizeEntry('hot', request, consent, quoteFailedRoute, new ConsentLedger(), verify),
    ).toEqual({ kind: 'permitted-with-consent' });
  });
});
