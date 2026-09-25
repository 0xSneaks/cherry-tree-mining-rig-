/**
 * Official Cherry Tree referral fallback + explicit referral precedence.
 *
 * The program rules are unchanged (bind once at creation, existing planter,
 * never self, 100 / 105 weights). These tests pin the product layer that
 * decides what the app proposes.
 */
import { describe, expect, it } from 'vitest';

import {
  parseOfficialReferrer,
  referralCandidates,
  referralDisclosure,
  referrerToSend,
  resolveReferral,
  suppliedReferral,
  type ReferralInputs,
} from '../src/lib/growth/referral';
import { SYS, WALLET_1, WALLET_2, WALLET_3 } from './growthFixtures';

const OFFICIAL = parseOfficialReferrer(WALLET_3);
const ME = WALLET_1;

function base(over: Partial<ReferralInputs> = {}): ReferralInputs {
  return {
    supplied: null,
    self: ME,
    hasPosition: false,
    boundReferrer: null,
    official: OFFICIAL,
    suppliedIsPlanter: null,
    officialIsPlanter: true,
    ...over,
  };
}

describe('official referrer configuration', () => {
  it('is unconfigured when empty and never invents a wallet', () => {
    expect(parseOfficialReferrer(undefined)).toEqual({ status: 'unconfigured' });
    expect(parseOfficialReferrer('')).toEqual({ status: 'unconfigured' });
    expect(parseOfficialReferrer('   ')).toEqual({ status: 'unconfigured' });
  });

  it('rejects malformed values and the default key', () => {
    expect(parseOfficialReferrer('not-a-wallet').status).toBe('invalid');
    expect(parseOfficialReferrer(SYS).status).toBe('invalid');
  });

  it('accepts a base58 wallet', () => {
    expect(parseOfficialReferrer(` ${WALLET_3} `)).toEqual({ status: 'configured', wallet: WALLET_3 });
  });

  it('ships unconfigured in this repository', async () => {
    const mod = await import('../src/lib/growth/referral');
    expect(process.env.NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER ?? '').toBe('');
    expect(mod.OFFICIAL_REFERRER.status).toBe('unconfigured');
  });
});

describe('explicit referral always wins', () => {
  it('a supplied planter referral is sent, not the fallback', () => {
    const r = resolveReferral(base({ supplied: WALLET_2, suppliedIsPlanter: true }));
    expect(r).toEqual({ kind: 'explicit', referrer: WALLET_2, verified: true });
    expect(referrerToSend(r)).toBe(WALLET_2);
  });

  it('while the planter check is pending, the explicit referral is still the one proposed', () => {
    const r = resolveReferral(base({ supplied: WALLET_2, suppliedIsPlanter: null }));
    expect(r).toEqual({ kind: 'explicit', referrer: WALLET_2, verified: false });
  });

  it('an explicit referral equal to the official one is explicit, not a fallback', () => {
    const r = resolveReferral(base({ supplied: WALLET_3, suppliedIsPlanter: true }));
    expect(r.kind).toBe('explicit');
  });

  it('a supplied non-planter sends nothing and is NOT replaced by the fallback', () => {
    const r = resolveReferral(base({ supplied: WALLET_2, suppliedIsPlanter: false }));
    expect(r).toEqual({ kind: 'explicitInvalid', raw: WALLET_2, reason: 'notPlanter' });
    expect(referrerToSend(r)).toBeNull();
  });

  it('a malformed supplied referral sends nothing and is NOT replaced', () => {
    const r = resolveReferral(base({ supplied: 'junk' }));
    expect(r).toEqual({ kind: 'explicitInvalid', raw: 'junk', reason: 'malformed' });
    expect(referrerToSend(r)).toBeNull();
  });

  it('a supplied self-referral sends nothing and is NOT replaced', () => {
    const r = resolveReferral(base({ supplied: ME }));
    expect(r).toEqual({ kind: 'explicitInvalid', raw: ME, reason: 'self' });
    expect(referrerToSend(r)).toBeNull();
  });
});

describe('official fallback only when nothing is supplied or bound', () => {
  it('applies with no link, no position, configured and verified', () => {
    const r = resolveReferral(base());
    expect(r).toEqual({ kind: 'official', referrer: WALLET_3 });
    expect(referrerToSend(r)).toBe(WALLET_3);
  });

  it('never applies to an existing position — no replacement', () => {
    const bound = resolveReferral(base({ hasPosition: true, boundReferrer: WALLET_2 }));
    expect(bound).toEqual({ kind: 'alreadyBound', referrer: WALLET_2 });
    expect(referrerToSend(bound)).toBeNull();
    const unbound = resolveReferral(base({ hasPosition: true, boundReferrer: SYS }));
    expect(unbound).toEqual({ kind: 'alreadyBound', referrer: null });
    expect(referrerToSend(unbound)).toBeNull();
    // even with a link present
    expect(referrerToSend(resolveReferral(base({ hasPosition: true, supplied: WALLET_2 })))).toBeNull();
  });

  it('fails closed when unconfigured or invalid', () => {
    expect(resolveReferral(base({ official: { status: 'unconfigured' } }))).toEqual({
      kind: 'none',
      reason: 'noFallbackConfigured',
    });
    expect(resolveReferral(base({ official: parseOfficialReferrer('bad') })).kind).toBe('none');
  });

  it('fails closed until the official wallet is verified as a planter', () => {
    expect(resolveReferral(base({ officialIsPlanter: null }))).toEqual({
      kind: 'none',
      reason: 'fallbackUnverified',
    });
    expect(resolveReferral(base({ officialIsPlanter: false }))).toEqual({
      kind: 'none',
      reason: 'fallbackNotPlanter',
    });
  });

  it('never self-refers the official wallet', () => {
    expect(resolveReferral(base({ self: WALLET_3 }))).toEqual({ kind: 'none', reason: 'fallbackIsSelf' });
  });

  it('needs a wallet', () => {
    expect(resolveReferral(base({ self: null }))).toEqual({ kind: 'none', reason: 'noWallet' });
  });
});

describe('planter lookups', () => {
  it('asks only for the wallet that decides the outcome', () => {
    expect(referralCandidates({ supplied: WALLET_2, self: ME, hasPosition: false, official: OFFICIAL })).toEqual({
      supplied: WALLET_2,
      official: null,
    });
    expect(referralCandidates({ supplied: null, self: ME, hasPosition: false, official: OFFICIAL })).toEqual({
      supplied: null,
      official: WALLET_3,
    });
    expect(referralCandidates({ supplied: null, self: ME, hasPosition: true, official: OFFICIAL })).toEqual({
      supplied: null,
      official: null,
    });
    expect(referralCandidates({ supplied: 'junk', self: ME, hasPosition: false, official: OFFICIAL })).toEqual({
      supplied: null,
      official: null,
    });
  });
});

describe('disclosure before signing', () => {
  it('says the official fallback is being used, that it binds permanently, and pays the referrer nothing', () => {
    const text = referralDisclosure(resolveReferral(base()))!;
    expect(text).toMatch(/official Cherry Tree referral/);
    expect(text).toMatch(/binds permanently/);
    expect(text).toMatch(/1\.05×/);
    expect(text).toMatch(/receives nothing/);
    expect(text).not.toMatch(/guarantee|ROI/i);
  });

  it('keeps the "Referred by" wording for explicit links', () => {
    const text = referralDisclosure(resolveReferral(base({ supplied: WALLET_2, suppliedIsPlanter: true })))!;
    expect(text).toMatch(/^Referred by FtUi…3GYp|^Referred by Ftui…3GYp/);
  });

  it('explains an invalid link without the fallback', () => {
    const text = referralDisclosure(resolveReferral(base({ supplied: 'junk' })))!;
    expect(text).toMatch(/not valid/);
    expect(text).not.toMatch(/Referred by/);
    expect(text).not.toMatch(/official Cherry Tree referral \(/);
  });

  it('has nothing to disclose for an existing position', () => {
    expect(referralDisclosure(resolveReferral(base({ hasPosition: true })))).toBeNull();
  });
});

describe('query parsing', () => {
  it('treats empty ?ref= as absent', () => {
    expect(suppliedReferral('?ref=')).toBeNull();
    expect(suppliedReferral('?ref=%20')).toBeNull();
    expect(suppliedReferral('')).toBeNull();
    expect(suppliedReferral(`?ref=${WALLET_2}&x=1`)).toBe(WALLET_2);
  });
});
