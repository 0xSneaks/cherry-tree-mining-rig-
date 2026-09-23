/**
 * The Cherry Chat embed is THE CHERRY TREE's second Cherry surface, pointing
 * the opposite way from the mini app. These assert the boundaries that keep
 * the two from interfering: it is opt-in, it carries no secret, and its
 * palette is the app's own rather than Cherry's chrome.
 */
import { readFileSync } from 'node:fs';
import { afterEach, describe, expect, it } from 'vitest';

import {
  CHERRY_CHAT_LAYOUT,
  CHERRY_CHAT_THEME,
  CHERRY_EMBED_ORIGIN,
} from '../src/lib/cherryChat';

const APP_ID = 'ee009264-9c63-47cc-b346-46a44b25ff29';
const ROOM_ID = '934ad16c-61b7-4cdb-b015-cd61dd217df4';

/**
 * `cherryChatConfig` reads `process.env` on every call rather than closing
 * over it at module scope, so setting the env before calling is enough and
 * module caching does not matter. (Next inlines these at build time for the
 * client bundle; under vitest they are ordinary reads.)
 */
async function loadWith(env: Record<string, string | undefined>) {
  for (const [k, v] of Object.entries(env)) {
    if (v === undefined) delete process.env[k];
    else process.env[k] = v;
  }
  const mod = await import('../src/lib/cherryChat');
  return mod;
}

const ORIGINAL = { ...process.env };
afterEach(() => {
  process.env = { ...ORIGINAL };
});

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

describe('no secret reaches the client', () => {
  const source = readFileSync(
    new URL('../src/lib/cherryChat.ts', import.meta.url),
    'utf8',
  );
  const component = readFileSync(
    new URL('../src/components/CommunityChat.tsx', import.meta.url),
    'utf8',
  );

  it('never references an app secret', () => {
    // `wallet-only` needs no backend token, so no HS256 signing key exists to
    // leak. If the embed ever moves to `app-trusted+wallet`, the secret stays
    // server-side and this still holds.
    for (const [name, text] of [
      ['cherryChat.ts', source],
      ['CommunityChat.tsx', component],
    ] as const) {
      expect(text, name).not.toMatch(/APP_SECRET/);
      expect(text, name).not.toMatch(/appSecret/);
    }
  });

  it('passes no token and no signing handler — the iframe owns wallet UX', () => {
    expect(component).not.toMatch(/signChallengeHandler/);
    expect(component).not.toMatch(/\btoken:/);
  });

  it('exposes only NEXT_PUBLIC_ ids, which are public identifiers', () => {
    const envReads = source.match(/process\.env\.[A-Z_]+/g) ?? [];
    expect(envReads.length).toBeGreaterThan(0);
    for (const read of envReads) {
      expect(read).toMatch(/^process\.env\.NEXT_PUBLIC_/);
    }
  });
});

describe('theme is the app palette, not Cherry chrome', () => {
  /** The tokens as `globals.css` actually defines them. */
  const css = readFileSync(new URL('../src/app/globals.css', import.meta.url), 'utf8');
  function token(name: string): string {
    const m = css.match(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{3,8});`));
    if (!m?.[1]) throw new Error(`token --${name} not found in globals.css`);
    return m[1].toLowerCase();
  }

  it('uses the exact background and surface tokens', () => {
    expect(CHERRY_CHAT_THEME.backgroundColor).toBe(token('bg'));
    expect(CHERRY_CHAT_THEME.surfaceColor).toBe(token('bg-panel'));
    expect(CHERRY_CHAT_THEME.headerColor).toBe(token('bg-inset'));
    expect(CHERRY_CHAT_THEME.inputColor).toBe(token('bg-inset'));
    expect(CHERRY_CHAT_THEME.borderColor).toBe(token('rule'));
  });

  it('uses the exact blossom tokens for primary, accent and CTA', () => {
    expect(CHERRY_CHAT_THEME.primaryColor).toBe(token('blossom'));
    expect(CHERRY_CHAT_THEME.sendButtonColor).toBe(token('blossom'));
    expect(CHERRY_CHAT_THEME.accentColor).toBe(token('blossom-soft'));
    expect(CHERRY_CHAT_THEME.linkColor).toBe(token('blossom-soft'));
    expect(CHERRY_CHAT_THEME.ownBubbleColor).toBe(token('blossom-dim'));
    expect(CHERRY_CHAT_THEME.dangerColor).toBe(token('alert'));
  });

  it('uses the exact text tokens', () => {
    expect(CHERRY_CHAT_THEME.textColor).toBe(token('text'));
    expect(CHERRY_CHAT_THEME.textSecondaryColor).toBe(token('text-dim'));
  });

  it("overrides Cherry's purple incoming bubble", () => {
    // The embed defaults to #4a1d56. Left alone, the chat would read as a
    // pasted-in widget rather than part of the machine room.
    expect(CHERRY_CHAT_THEME.incomingBubbleColor).toBe(token('bg-panel'));
    expect(CHERRY_CHAT_THEME.incomingBubbleColor).not.toBe('#4a1d56');
  });

  it('pins flat fills, since the brief asked for exact colours', () => {
    expect(CHERRY_CHAT_THEME.gradients).toBe('off');
  });

  it('matches the monospace stack the app sets on body', () => {
    const bodyFont = css.match(/--mono:\s*([^;]+);/)?.[1]?.trim();
    expect(bodyFont).toBeTruthy();
    expect(CHERRY_CHAT_THEME.fontFamily).toBe(bodyFont);
  });

  it('is dark, and titled with the product name', () => {
    expect(CHERRY_CHAT_THEME.mode).toBe('dark');
    expect(CHERRY_CHAT_LAYOUT.headerTitle).toBe('THE CHERRY TREE');
  });
});

describe('frame origin', () => {
  it('is a single exact https origin, with no path and no wildcard', () => {
    expect(CHERRY_EMBED_ORIGIN).toBe('https://embed.cherry.fun');
    expect(new URL(CHERRY_EMBED_ORIGIN).origin).toBe(CHERRY_EMBED_ORIGIN);
    expect(CHERRY_EMBED_ORIGIN).not.toContain('*');
  });
});
