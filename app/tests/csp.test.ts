/**
 * The CSP is shared by two emitters — `next.config.ts` (real response headers
 * on a Node deployment) and `scripts/emit-static-headers.ts` (the `_headers`
 * file in the static export). These assert the directives that Cherry and the
 * wallet session actually depend on, so a policy edit that breaks the embed
 * fails here rather than in a chat room.
 */
import { describe, expect, it } from 'vitest';

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
