/**
 * The CSP is shared by the emitters — `src/proxy.ts` (nonce, per request, on
 * a Node deployment) and `scripts/emit-static-headers.ts` (sha-256 hashes,
 * per page, in the static export's `_headers` file). These assert the
 * directives that Cherry and the wallet session actually depend on, so a
 * policy edit that breaks the embed fails here rather than in a chat room.
 */
import { afterEach, describe, expect, it } from 'vitest';

import { CHERRY_EMBED_ORIGIN } from '../src/lib/cherryChat';
import {
  buildCsp,
  CHERRY_HOST,
  nonceScriptSources,
  securityHeaders,
} from '../src/lib/csp';

describe('content security policy', () => {
  // A nonce the proxy might have minted; the value is arbitrary because
  // every assertion is about the policy around it, never the nonce itself.
  const csp = buildCsp({ dev: false, scriptSrc: nonceScriptSources('ZHVtbXlub25jZQ') });

  it('lets Cherry web frame the app', () => {
    expect(csp).toContain(`frame-ancestors 'self' ${CHERRY_HOST}`);
  });

  it('never sets X-Frame-Options, which would block the iframe', () => {
    expect(
      securityHeaders({ dev: false, scriptSrc: nonceScriptSources('ZHVtbXlub25jZQ') })
        .map((h) => h.key.toLowerCase()),
    ).not.toContain('x-frame-options');
  });

  it('keeps connect-src an allowlist rather than a scheme wildcard', () => {
    expect(csp).toContain("connect-src 'self' https://api.devnet.solana.com");
    expect(csp).not.toMatch(/connect-src[^;]*\shttps:(?!\/)/);
    expect(csp).not.toMatch(/connect-src[^;]*\swss:(?!\/)/);
  });

  it('permits the Cherry host, which is where JWKS is fetched from', () => {
    expect(csp).toMatch(new RegExp(`connect-src[^;]*${CHERRY_HOST}`));
  });

  it('allows eval only for the dev server', () => {
    expect(csp).not.toContain("'unsafe-eval'");
    expect(
      buildCsp({ dev: true, scriptSrc: nonceScriptSources('ZHVtbXlub25jZQ') }),
    ).toContain("'unsafe-eval'");
  });

  it('upgrades insecure requests in production only', () => {
    expect(csp).toContain('upgrade-insecure-requests');
    expect(
      buildCsp({ dev: true, scriptSrc: nonceScriptSources('ZHVtbXlub25jZQ') }),
    ).not.toContain('upgrade-insecure-requests');
  });
});

/** The script-src directive alone — `style-src` keeps 'unsafe-inline' for React's inline styles. */
const scriptSrc = (csp: string): string => csp.match(/script-src ([^;]*)/)?.[1] ?? '';

describe("script-src no longer admits 'unsafe-inline'", () => {
  it('serves a nonce policy on a Node deployment', () => {
    const csp = buildCsp({ dev: false, scriptSrc: nonceScriptSources('ZHVtbXlub25jZQ') });
    expect(scriptSrc(csp)).toBe(
      "'self' 'nonce-ZHVtbXlub25jZQ' 'strict-dynamic'",
    );
  });

  it('serves a hash policy on a static host', () => {
    const csp = buildCsp({
      dev: false,
      scriptSrc: ["'self'", "'sha256-ZHVtbXloYXNoZXRvbnVtYmVy'"],
    });
    expect(scriptSrc(csp)).toBe("'self' 'sha256-ZHVtbXloYXNoZXRvbnVtYmVy'");
  });

  it('refuses to build a policy that carries unsafe-inline, in either mode', () => {
    expect(() =>
      buildCsp({ dev: false, scriptSrc: ["'self'", "'unsafe-inline'"] }),
    ).toThrow(/unsafe-inline.*banned/);
  });

  it('refuses a malformed nonce, which would silently admit nothing', () => {
    // A nonce containing characters outside base64 makes Next's parser drop
    // it; the page then fails closed. Throwing at build time names the bug.
    expect(() => nonceScriptSources('not a nonce!')).toThrow(/malformed nonce/);
    expect(() => nonceScriptSources('')).toThrow(/malformed nonce/);
  });

  it("never widens script-src beyond 'self' plus one nonce or hashes", () => {
    const directive = scriptSrc(
      buildCsp({ dev: false, scriptSrc: nonceScriptSources('ZHVtbXlub25jZQ') }),
    );
    expect(directive).not.toContain("'unsafe-inline'");
    expect(directive).not.toMatch(/https:(?!\/)/);
    expect(directive).not.toContain('*');
  });
});

/**
 * The Cherry Chat embed is the only frame this app ever loads. `frame-src` is
 * therefore opt-in at the policy level as well as in the component tree: a
 * build with no embed configured must still refuse every frame.
 */
describe('frame-src follows the chat embed', () => {
  const ENABLED = {
    NEXT_PUBLIC_CHERRY_CHAT_APP_ID: 'ee009264-9c63-47cc-b346-46a44b25ff29',
    NEXT_PUBLIC_CHERRY_CHAT_ROOM_ID: '934ad16c-61b7-4cdb-b015-cd61dd217df4',
  };
  const ORIGINAL = { ...process.env };
  afterEach(() => {
    process.env = { ...ORIGINAL };
  });

  function frameSrc(): string {
    return (
      buildCsp({ dev: false, scriptSrc: nonceScriptSources('ZHVtbXlub25jZQ') })
        .split('; ')
        .find((d) => d.startsWith('frame-src')) ?? ''
    );
  }

  it("is 'none' when no embed is configured", () => {
    delete process.env.NEXT_PUBLIC_CHERRY_CHAT_APP_ID;
    delete process.env.NEXT_PUBLIC_CHERRY_CHAT_ROOM_ID;
    expect(frameSrc()).toBe("frame-src 'none'");
  });

  it('is exactly the embed origin when one is configured', () => {
    Object.assign(process.env, ENABLED);
    expect(frameSrc()).toBe(`frame-src ${CHERRY_EMBED_ORIGIN}`);
  });

  it('never widens to a scheme wildcard or a subdomain wildcard', () => {
    Object.assign(process.env, ENABLED);
    const directive = frameSrc();
    expect(directive).not.toContain('*');
    expect(directive).not.toMatch(/\shttps:(?!\/)/);
    expect(directive).not.toContain("'self'");
  });

  it('leaves frame-ancestors alone — Cherry web must still be able to frame us', () => {
    Object.assign(process.env, ENABLED);
    expect(
      buildCsp({ dev: false, scriptSrc: nonceScriptSources('ZHVtbXlub25jZQ') }),
    ).toContain(`frame-ancestors 'self' ${CHERRY_HOST}`);
  });
});
