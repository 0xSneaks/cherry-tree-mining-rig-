/**
 * The CSP is shared by two emitters — `next.config.ts` (real response headers
 * on a Node deployment) and `scripts/emit-static-headers.ts` (the `_headers`
 * file in the static export). These assert the directives that Cherry and the
 * wallet session actually depend on, so a policy edit that breaks the embed
 * fails here rather than in a chat room.
 */
import { afterEach, describe, expect, it } from 'vitest';

import { CHERRY_EMBED_ORIGIN } from '../src/lib/cherryChat';
import { buildCsp, CHERRY_HOST, securityHeaders } from '../src/lib/csp';

describe('content security policy', () => {
  const csp = buildCsp({ dev: false });

  it('lets Cherry web frame the app', () => {
    expect(csp).toContain(`frame-ancestors 'self' ${CHERRY_HOST}`);
  });

  it('never sets X-Frame-Options, which would block the iframe', () => {
    expect(securityHeaders({ dev: false }).map((h) => h.key.toLowerCase())).not.toContain(
      'x-frame-options',
    );
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
    expect(buildCsp({ dev: true })).toContain("'unsafe-eval'");
  });

  it('upgrades insecure requests in production only', () => {
    expect(csp).toContain('upgrade-insecure-requests');
    expect(buildCsp({ dev: true })).not.toContain('upgrade-insecure-requests');
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
      buildCsp({ dev: false })
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
    expect(buildCsp({ dev: false })).toContain(
      `frame-ancestors 'self' ${CHERRY_HOST}`,
    );
  });
});
