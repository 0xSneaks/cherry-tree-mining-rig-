/**
 * Cherry Chat embed — configuration and theme.
 *
 * This is THE CHERRY TREE's *second* Cherry surface, pointing the other way
 * from the first:
 *
 *   mini app   — THE CHERRY TREE inside Cherry   (@cherrydotfun/miniapp-sdk)
 *   chat embed — Cherry chat inside THE CHERRY TREE (this file)
 *
 * They share nothing but a brand. The mini-app path owns the launch token, the
 * wallet bridge and the blink card; none of it is touched here, and the embed
 * never renders on that path — inside Cherry the user is already in the chat,
 * so framing it again would be recursive. `CommunityChat` enforces that.
 *
 * Auth mode is `app-trusted+wallet`: Cherry trusts a short-lived HS256 token
 * this backend mints AND a live signature from the user's wallet. The signing
 * key (`CHERRY_APP_SECRET`) is server-side only and appears nowhere in this
 * file — everything here ships to the browser.
 */

/** The single origin the embed iframe is served from. */
export const CHERRY_EMBED_ORIGIN = 'https://embed.cherry.fun';

/** Route that mints the embed token. Server-only; see `app/api/`. */
export const EMBED_TOKEN_ROUTE = '/api/cherry-embed-token';
/** Route that issues the nonce the wallet signs to prove ownership. */
export const EMBED_NONCE_ROUTE = '/api/cherry-embed-nonce';

export type CherryChatConfig = {
  appId: string;
  roomId: string;
};

/**
 * The configured embed, or null when this deployment has none.
 *
 * Both reads are written out literally: Next only inlines `NEXT_PUBLIC_*` into
 * the client bundle when it can see the full member expression at build time.
 *
 * These two are public identifiers — the SDK takes them in browser code by
 * design. The app *secret* is a different thing entirely and is never read
 * here, in any form.
 */
export function cherryChatConfig(): CherryChatConfig | null {
  const appId = process.env.NEXT_PUBLIC_CHERRY_CHAT_APP_ID;
  const roomId = process.env.NEXT_PUBLIC_CHERRY_CHAT_ROOM_ID;
  if (!appId || !roomId) return null;
  return { appId, roomId };
}

/** Whether a build should allow the embed to be framed at all. */
export function cherryChatEnabled(): boolean {
  return cherryChatConfig() !== null;
}

/**
 * Theme seeds — and only seeds.
 *
 * The embed runs a calibrated derivation engine: hand it 2–4 honest brand
 * colours and it derives the whole ~60-variable palette with contrast floors
 * baked in. Hand-tuning the granular variables fights that engine and lands on
 * a tint-filter-over-the-default look, so there are exactly three seeds here
 * plus `mode`, extracted by role from `globals.css`:
 *
 *   mode            dark, from the app's real content surface
 *   primaryColor    brand CTA  → own bubble + send button
 *   backgroundColor content surface (--bg), NOT the panel chrome
 *   accentColor     secondary highlight (--blossom-soft) → links + mentions
 *
 * `incomingBubbleColor` is deliberately NOT set. It is worth passing only when
 * the brand has a soft tinted fill that genuinely separates from the page
 * ground; ours (`--bg-panel` #0f1526) sits at a 1.075 contrast ratio against
 * `--bg` #080c16 — a near-identical neutral navy, not a second brand colour —
 * so the engine derives a better incoming bubble than we would.
 *
 * Measured against the floors the theming guide sets:
 *   body text on background   16.55:1  (need 4.5)
 *   accent on background       9.24:1  (need 4.5)
 *   send glyph on primary      6.90:1  (need 3)
 */
export const CHERRY_CHAT_THEME = {
  mode: 'dark',
  /**
   * `--blossom`: the canonical brand pink, the same value the app renders.
   *
   * Cherry's portal stores #FF5BA8 for this embed — a 1.013 contrast ratio
   * from #ff5fa2, i.e. the same pink. Passing the app token instead of the
   * portal value means the chat's bubble and send button match the wordmark
   * and CTAs exactly; `tests/cherryChat.test.ts` fails if they drift.
   */
  primaryColor: '#ff5fa2',
  /** `--bg`: the content surface the mine renders on. */
  backgroundColor: '#080c16',
  /** `--blossom-soft`: the app's secondary highlight. */
  accentColor: '#ff8fbf',
} as const;

export const CHERRY_CHAT_LAYOUT = {
  showHeader: true,
  headerTitle: 'THE CHERRY TREE',
  showMemberCount: true,
  showInput: true,
} as const;
