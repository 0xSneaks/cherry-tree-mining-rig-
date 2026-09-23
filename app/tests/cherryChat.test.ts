/**
 * The Cherry Chat embed is THE CHERRY TREE's second Cherry surface, pointing
 * the opposite way from the mini app. These assert the boundaries that keep
 * the two from interfering, that no secret reaches the browser, and that the
 * theme is a set of honest seeds rather than a hand-tuned palette.
 */
import { readFileSync } from 'node:fs';
import { afterEach, describe, expect, it } from 'vitest';

import {
  CHERRY_CHAT_LAYOUT,
  CHERRY_CHAT_THEME,
  CHERRY_EMBED_ORIGIN,
  EMBED_NONCE_ROUTE,
  EMBED_TOKEN_ROUTE,
} from '../src/lib/cherryChat';

const APP_ID = 'ee009264-9c63-47cc-b346-46a44b25ff29';
const ROOM_ID = '934ad16c-61b7-4cdb-b015-cd61dd217df4';

async function loadWith(env: Record<string, string | undefined>) {
  for (const [k, v] of Object.entries(env)) {
    if (v === undefined) delete process.env[k];
    else process.env[k] = v;
  }
  // `cherryChatConfig` reads `process.env` on every call rather than closing
  // over it, so setting the env before calling is enough and module caching
  // does not matter. (Next inlines these for the client bundle at build time;
  // under vitest they are ordinary reads.)
  return import('../src/lib/cherryChat');
}

const ORIGINAL = { ...process.env };
afterEach(() => {
  process.env = { ...ORIGINAL };
});

// ------------------------------------------------------------ contrast

function luminance(hex: string): number {
  const h = hex.replace('#', '');
  const channel = (i: number) => {
    const c = Number.parseInt(h.slice(i, i + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4);
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

// ------------------------------------------------------------ config

describe('embed configuration', () => {
  it('is null — and so disabled — when nothing is configured', async () => {
    const { cherryChatConfig, cherryChatEnabled } = await loadWith({
      NEXT_PUBLIC_CHERRY_CHAT_APP_ID: undefined,
      NEXT_PUBLIC_CHERRY_CHAT_ROOM_ID: undefined,
    });
    expect(cherryChatConfig()).toBeNull();
    expect(cherryChatEnabled()).toBe(false);
  });

  it('is null when only one of the two ids is present', async () => {
    const { cherryChatConfig } = await loadWith({
      NEXT_PUBLIC_CHERRY_CHAT_APP_ID: APP_ID,
      NEXT_PUBLIC_CHERRY_CHAT_ROOM_ID: undefined,
    });
    expect(cherryChatConfig()).toBeNull();

    const again = await loadWith({
      NEXT_PUBLIC_CHERRY_CHAT_APP_ID: undefined,
      NEXT_PUBLIC_CHERRY_CHAT_ROOM_ID: ROOM_ID,
    });
    expect(again.cherryChatConfig()).toBeNull();
  });

  it('carries the Cherry-issued ids when both are set', async () => {
    const { cherryChatConfig, cherryChatEnabled } = await loadWith({
      NEXT_PUBLIC_CHERRY_CHAT_APP_ID: APP_ID,
      NEXT_PUBLIC_CHERRY_CHAT_ROOM_ID: ROOM_ID,
    });
    expect(cherryChatConfig()).toEqual({ appId: APP_ID, roomId: ROOM_ID });
    expect(cherryChatEnabled()).toBe(true);
  });
});

/**
 * Strip comments before asserting about source.
 *
 * These checks are about what the code *does*. The files explain at length
 * why the secret is server-side and why the handler goes in the constructor,
 * and prose that names a thing is not the same as code that uses it.
 */
function code(text: string): string {
  return text
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/(^|[^:])\/\/.*$/gm, '$1');
}

// ------------------------------------------------------------ secrets

describe('no secret reaches the client', () => {
  const clientFiles = {
    'cherryChat.ts': readFileSync(new URL('../src/lib/cherryChat.ts', import.meta.url), 'utf8'),
    'CommunityChat.tsx': readFileSync(
      new URL('../src/components/CommunityChat.tsx', import.meta.url),
      'utf8',
    ),
    'StandaloneGame.tsx': readFileSync(
      new URL('../src/components/StandaloneGame.tsx', import.meta.url),
      'utf8',
    ),
  };

  it('never reads or names the app secret in client code', () => {
    for (const [name, text] of Object.entries(clientFiles)) {
      expect(code(text), name).not.toMatch(/CHERRY_APP_SECRET/);
      expect(code(text), name).not.toMatch(/appSecret/);
    }
  });

  it('never mints or signs a token client-side — it asks the backend', () => {
    const component = code(clientFiles['CommunityChat.tsx']);
    expect(component).not.toMatch(/createHmac|HS256|jwt\.sign/);
    // It fetches from the backend routes by their shared constants rather
    // than inlining paths, so assert the identifiers it actually uses.
    expect(component).toMatch(/fetch\(EMBED_NONCE_ROUTE/);
    expect(component).toMatch(/fetch\(EMBED_TOKEN_ROUTE/);
  });

  it('exposes only NEXT_PUBLIC_ ids, which are public identifiers', () => {
    const envReads = code(clientFiles['cherryChat.ts']).match(/process\.env\.[A-Z_]+/g) ?? [];
    expect(envReads.length).toBeGreaterThan(0);
    for (const read of envReads) {
      expect(read).toMatch(/^process\.env\.NEXT_PUBLIC_/);
      expect(read).not.toMatch(/SECRET/);
    }
  });

  it('keeps the token minter server-only', () => {
    for (const file of ['../src/lib/server/embedToken.ts', '../src/lib/server/walletProof.ts']) {
      const text = readFileSync(new URL(file, import.meta.url), 'utf8');
      expect(text, file).toMatch(/^import 'server-only';/m);
    }
  });
});

// ------------------------------------------------------------ auth wiring

describe('app-trusted+wallet wiring', () => {
  const component = code(
    readFileSync(new URL('../src/components/CommunityChat.tsx', import.meta.url), 'utf8'),
  );

  it('registers signChallengeHandler in the constructor, not after mount', () => {
    const constructorAt = component.indexOf('new CherryEmbed(');
    const handlerAt = component.indexOf('signChallengeHandler:');
    const mountAt = component.indexOf('.mount()');
    expect(constructorAt).toBeGreaterThan(-1);
    expect(handlerAt).toBeGreaterThan(constructorAt);
    expect(handlerAt).toBeLessThan(mountAt);
    // The racy pattern Cherry warns about explicitly.
    expect(component).not.toMatch(/onSignChallenge\s*\(/);
  });

  it('re-arms auth on walletConnectRequested, so the button is not a no-op', () => {
    expect(component).toContain("walletConnectRequested");
    expect(component).toMatch(/setToken\(/);
    expect(component).toMatch(/setWalletAddress\(/);
  });

  it('pins the embed url and single-room mode', () => {
    expect(component).toContain('embedUrl: CHERRY_EMBED_ORIGIN');
    expect(component).toContain("mode: 'single'");
    expect(component).toContain("position: 'inline'");
  });

  it('sets no apiUrl — the iframe calls Cherry from its own origin', () => {
    expect(component).not.toMatch(/apiUrl/);
  });
});

// ------------------------------------------------------------ theme

describe('theme is honest seeds, not a hand-tuned palette', () => {
  const css = readFileSync(new URL('../src/app/globals.css', import.meta.url), 'utf8');
  function token(name: string): string {
    const m = css.match(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{3,8});`));
    if (!m?.[1]) throw new Error(`token --${name} not found in globals.css`);
    return m[1].toLowerCase();
  }

  it('passes at most three seeds plus mode', () => {
    // The embed derives ~60 variables from these. Hand-tuning the granular
    // ones fights that engine and lands on a tint-over-default look.
    const keys = Object.keys(CHERRY_CHAT_THEME);
    expect(keys).toEqual(['mode', 'primaryColor', 'backgroundColor', 'accentColor', 'gmColor']);
    const seeds = keys.filter((k) => k !== 'mode' && k !== 'gmColor');
    expect(seeds).toHaveLength(3);
  });

  it("paints Cherry's GM badge with the canonical --blossom, cosmetically", () => {
    // A documented EmbedTheme field; the app never reads the GM count.
    expect(CHERRY_CHAT_THEME.gmColor).toBe(token('blossom'));
    expect(CHERRY_CHAT_THEME.gmColor).toBe('#ff5fa2');
  });

  it('sets none of the granular derived variables', () => {
    for (const derived of [
      'surfaceColor',
      'borderColor',
      'textColor',
      'textSecondaryColor',
      'linkColor',
      'ownBubbleColor',
      'headerColor',
      'inputColor',
      'sendButtonColor',
      'gradients',
      'fontFamily',
    ]) {
      expect(CHERRY_CHAT_THEME, derived).not.toHaveProperty(derived);
    }
  });

  it('takes the background from the content surface, not the panel chrome', () => {
    expect(CHERRY_CHAT_THEME.backgroundColor).toBe(token('bg'));
    expect(CHERRY_CHAT_THEME.backgroundColor).not.toBe(token('bg-panel'));
  });

  it('takes the accent from the secondary highlight', () => {
    expect(CHERRY_CHAT_THEME.accentColor).toBe(token('blossom-soft'));
  });

  it('takes the primary from the canonical --blossom token, exactly', () => {
    // One brand pink. Cherry's portal holds #FF5BA8 (1.013:1 from --blossom,
    // the same pink); the app token wins so chat and app cannot drift apart.
    expect(CHERRY_CHAT_THEME.primaryColor).toBe(token('blossom'));
    expect(contrast('#FF5BA8', token('blossom'))).toBeLessThan(1.05);
  });

  it('omits incomingBubbleColor, because the panel fill does not separate', () => {
    // The rule is to pass a soft tinted fill only when it clears the floors
    // and separates from the page ground. --bg-panel sits at ~1.07 against
    // --bg: a near-identical neutral navy, not a second brand colour.
    expect(CHERRY_CHAT_THEME).not.toHaveProperty('incomingBubbleColor');
    expect(contrast(token('bg-panel'), token('bg'))).toBeLessThan(1.2);
  });

  it('clears the contrast floors the theming guide sets', () => {
    const bg = CHERRY_CHAT_THEME.backgroundColor;
    expect(contrast(token('text'), bg)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(CHERRY_CHAT_THEME.accentColor, bg)).toBeGreaterThanOrEqual(4.5);
    // Send glyph sits on the primary fill.
    expect(contrast(bg, CHERRY_CHAT_THEME.primaryColor)).toBeGreaterThanOrEqual(3);
  });

  it('is dark, matching the real app surface', () => {
    expect(CHERRY_CHAT_THEME.mode).toBe('dark');
    expect(luminance(CHERRY_CHAT_THEME.backgroundColor)).toBeLessThan(0.05);
  });

  it('titles the room with the product name', () => {
    expect(CHERRY_CHAT_LAYOUT.headerTitle).toBe('THE CHERRY TREE');
  });
});

describe('backend routes', () => {
  it('are the paths Cherry\'s contract names, under /api', () => {
    expect(EMBED_TOKEN_ROUTE).toBe('/api/cherry-embed-token');
    expect(EMBED_NONCE_ROUTE).toBe('/api/cherry-embed-nonce');
  });
});

describe('frame origin', () => {
  it('is a single exact https origin, with no path and no wildcard', () => {
    expect(CHERRY_EMBED_ORIGIN).toBe('https://embed.cherry.fun');
    expect(new URL(CHERRY_EMBED_ORIGIN).origin).toBe(CHERRY_EMBED_ORIGIN);
    expect(CHERRY_EMBED_ORIGIN).not.toContain('*');
  });
});
