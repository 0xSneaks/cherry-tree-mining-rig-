/**
 * Vercel holds the chat ids unprefixed (`CHERRY_CHAT_APP_ID` / `_ROOM_ID`),
 * while the browser can only see `NEXT_PUBLIC_*`. `next.config.ts` bridges the
 * two. Without it the client config is null and the Community chat never
 * mounts on the standalone deployment — which is exactly what shipped.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CHERRY_EMBED_ORIGIN } from '../src/lib/cherryChat';
import { buildCsp, nonceScriptSources } from '../src/lib/csp';

const APP_ID = 'ee009264-9c63-47cc-b346-46a44b25ff29';
const ROOM_ID = '934ad16c-61b7-4cdb-b015-cd61dd217df4';
const SECRET = 'test-app-secret-not-a-real-one-0123456789';

const ORIGINAL = { ...process.env };
afterEach(() => {
  process.env = { ...ORIGINAL };
  vi.resetModules();
});

async function loadConfig(env: Record<string, string | undefined>) {
  for (const k of [
    'NEXT_PUBLIC_CHERRY_CHAT_APP_ID',
    'NEXT_PUBLIC_CHERRY_CHAT_ROOM_ID',
    'CHERRY_CHAT_APP_ID',
    'CHERRY_CHAT_ROOM_ID',
    'STATIC_EXPORT',
  ]) {
    delete process.env[k];
  }
  for (const [k, v] of Object.entries(env)) {
    if (v !== undefined) process.env[k] = v;
  }
  vi.resetModules();
  const mod = await import('../next.config');
  return mod.default;
}

/**
 * The CSP no longer comes from `headers()` — it carries a per-request nonce,
 * so `src/proxy.ts` builds it. What this file asserts about the policy is
 * that it still *follows the bridged env*: loadConfig runs the bridge by
 * importing next.config, and building afterwards reads the same process.env
 * the proxy would see.
 */
async function cspOf(): Promise<string> {
  return buildCsp({ dev: false, scriptSrc: nonceScriptSources('ZHVtbXlub25jZQ') });
}

describe('chat id bridge', () => {
  it('inlines the unprefixed Vercel ids under their NEXT_PUBLIC_ names', async () => {
    const config = await loadConfig({
      CHERRY_CHAT_APP_ID: APP_ID,
      CHERRY_CHAT_ROOM_ID: ROOM_ID,
    });
    expect(config.env).toEqual({
      NEXT_PUBLIC_CHERRY_CHAT_APP_ID: APP_ID,
      NEXT_PUBLIC_CHERRY_CHAT_ROOM_ID: ROOM_ID,
    });
    expect(await cspOf()).toContain(`frame-src ${CHERRY_EMBED_ORIGIN}`);
  });

  it('prefers an explicit NEXT_PUBLIC_ value over the unprefixed one', async () => {
    const config = await loadConfig({
      NEXT_PUBLIC_CHERRY_CHAT_APP_ID: APP_ID,
      NEXT_PUBLIC_CHERRY_CHAT_ROOM_ID: ROOM_ID,
      CHERRY_CHAT_APP_ID: 'other-app',
      CHERRY_CHAT_ROOM_ID: 'other-room',
    });
    expect(config.env).toEqual({
      NEXT_PUBLIC_CHERRY_CHAT_APP_ID: APP_ID,
      NEXT_PUBLIC_CHERRY_CHAT_ROOM_ID: ROOM_ID,
    });
  });

  it("leaves the chat off, and frame-src 'none', when nothing is configured", async () => {
    const config = await loadConfig({});
    expect(config.env).toEqual({});
    expect(await cspOf()).toContain("frame-src 'none'");
  });

  it('never carries the app secret into the inlined env', async () => {
    const config = await loadConfig({
      CHERRY_CHAT_APP_ID: APP_ID,
      CHERRY_CHAT_ROOM_ID: ROOM_ID,
      CHERRY_APP_SECRET: SECRET,
    });
    const inlined = JSON.stringify(config.env);
    expect(inlined).not.toContain(SECRET);
    expect(Object.keys(config.env ?? {}).some((k) => /SECRET/.test(k))).toBe(false);
  });

  it('keeps frame-ancestors for Cherry web with the chat enabled', async () => {
    await loadConfig({
      CHERRY_CHAT_APP_ID: APP_ID,
      CHERRY_CHAT_ROOM_ID: ROOM_ID,
    });
    expect(await cspOf()).toContain("frame-ancestors 'self' https://chat.cherry.fun");
  });
});

describe('static response headers', () => {
  it('carries the nonce-free headers and leaves the CSP to the proxy', async () => {
    const config = await loadConfig({});
    const rules = await config.headers!();
    expect(rules).toHaveLength(1);
    expect(rules[0]!.source).toBe('/(.*)');
    expect(rules[0]!.headers.map((h) => h.key)).toEqual([
      'Referrer-Policy',
      'X-Content-Type-Options',
      'Cross-Origin-Opener-Policy',
    ]);
    // Two CSP headers would be enforced as their intersection: the static
    // one has no nonce, so everything the proxy just nonced would be blocked.
    expect(rules[0]!.headers.some((h) => h.key === 'Content-Security-Policy')).toBe(false);
  });
});
