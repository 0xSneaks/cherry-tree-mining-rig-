/**
 * The proxy is the Node deployment's CSP emitter: it mints a nonce, hands
 * Next the request-side CSP the nonce is parsed from, and enforces the same
 * string on the response. These check the header mechanics; that Next
 * actually stamps its scripts (and the app survives its own policy) is what
 * the `CSP is survivable` browser tests in e2e/render.spec.ts prove.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';

import { NextRequest } from 'next/server';

import proxy from '../src/proxy';

afterEach(() => {
  vi.unstubAllEnvs();
});

function cspFor(url = 'http://localhost/'): string {
  return proxy(new NextRequest(url)).headers.get('content-security-policy') ?? '';
}

/** The script-src directive alone — style-src keeps 'unsafe-inline' for React. */
function scriptSrc(csp: string): string {
  return csp.match(/script-src ([^;]*)/)?.[1] ?? '';
}

describe('proxy CSP', () => {
  it('serves a nonce policy, never unsafe-inline', () => {
    // Vitest runs with NODE_ENV=test (≠ production), which reads as dev here.
    vi.stubEnv('NODE_ENV', 'production');
    const csp = cspFor();
    expect(scriptSrc(csp)).toMatch(/^'self' 'nonce-[A-Za-z0-9_-]+' 'strict-dynamic'$/);
    expect(scriptSrc(csp)).not.toContain("'unsafe-inline'");
    // The framing and allowlist directives the embed depends on.
    expect(csp).toContain('frame-ancestors');
    expect(csp).toContain("connect-src 'self'");
  });

  it('mints a fresh nonce per request', () => {
    expect(scriptSrc(cspFor())).not.toBe(scriptSrc(cspFor()));
  });

  it('adds unsafe-eval only outside production (React Refresh)', () => {
    vi.stubEnv('NODE_ENV', 'development');
    const dev = scriptSrc(cspFor());
    expect(dev).toContain("'unsafe-eval'");
    expect(dev).not.toContain("'unsafe-inline'");

    vi.stubEnv('NODE_ENV', 'production');
    expect(scriptSrc(cspFor())).not.toContain("'unsafe-eval'");
  });

  it('marks nonced documents uncacheable — a replayed nonce is a broken or stolen one', () => {
    expect(proxy(new NextRequest('http://localhost/')).headers.get('cache-control')).toBe(
      'private, no-store',
    );
  });
});
