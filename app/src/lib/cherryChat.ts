/**
 * Cherry Chat embed — configuration and theme.
 *
 * This is THE CHERRY TREE's *second* Cherry surface, and it points the other
 * way from the first one:
 *
 *   mini app   — THE CHERRY TREE running inside Cherry   (@cherrydotfun/miniapp-sdk)
 *   chat embed — Cherry chat running inside THE CHERRY TREE (this file)
 *
 * They share nothing but a brand. The mini-app path owns the launch token, the
 * wallet bridge and the blink card; none of it is touched here, and the embed
 * never renders on that path — inside Cherry the user is already in the chat,
 * so framing it again would be both recursive and wrong. `CommunityChat`
 * enforces that.
 *
 * Auth mode is `wallet-only`: the iframe runs its own wallet UX and there is
 * no backend token, which means **no app secret exists anywhere in this
 * project**. `appId` and `roomId` are public identifiers — the SDK takes them
 * in browser code by design — so they are `NEXT_PUBLIC_*` and nothing else.
 * If this ever moves to `app-trusted+wallet`, the HS256 signing key stays
 * server-side and never becomes a `NEXT_PUBLIC_` anything.
 */

/**
 * The single origin the embed iframe is served from.
 *
 * `csp.ts` reads this to widen `frame-src` by exactly one entry — and only
 * when the embed is configured. A build without chat keeps `frame-src 'none'`.
 */
export const CHERRY_EMBED_ORIGIN = 'https://embed.cherry.fun';

export type CherryChatConfig = {
  appId: string;
  roomId: string;
};

/**
 * The configured embed, or null when this deployment has none.
 *
 * Both reads are written out literally: Next only inlines `NEXT_PUBLIC_*` into
 * the client bundle when it can see the full member expression at build time.
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
 * Exact colours, taken from `globals.css` rather than invented.
 *
 * The embed's own defaults are Cherry's chrome — dark navy with a purple
 * incoming bubble (#4a1d56) and a primary→accent gradient on the send button.
 * Every one of those is overridden here so the chat reads as part of the
 * machine room instead of as a pasted-in widget: flat fills (`gradients:
 * 'off'`), the blossom as the only accent, and the same monospace stack the
 * rest of the app sets on `body`.
 *
 * Token map — change `globals.css`, change these:
 *   --bg          #080c16   page / chat area
 *   --bg-panel    #0f1526   surface, incoming bubbles
 *   --bg-inset    #0b101d   header, input
 *   --rule        #1c2740   borders
 *   --text        #e9ecf5   body text
 *   --text-dim    #8e9ab8   secondary text
 *   --blossom     #ff5fa2   primary / CTA
 *   --blossom-soft#ff8fbf   accent, links
 *   --blossom-dim #8c3560   own bubble fill
 *   --alert       #ff6b81   destructive
 */
export const CHERRY_CHAT_THEME = {
  mode: 'dark',
  // (C) exact colours: no brand sweep, flat fills only.
  gradients: 'off',

  primaryColor: '#ff5fa2',
  accentColor: '#ff8fbf',
  backgroundColor: '#080c16',
  surfaceColor: '#0f1526',
  borderColor: '#1c2740',
  textColor: '#e9ecf5',
  textSecondaryColor: '#8e9ab8',

  // Clickable glyphs: soft blossom on the dark side, inset navy on the
  // blossom-filled own bubble, so both stay legible against their own ground.
  linkColor: '#ff8fbf',
  linkColorOwn: '#0b101d',

  // Cherry's default incoming bubble is purple. Ours is the panel tone.
  incomingBubbleColor: '#0f1526',
  incomingBubbleBorderColor: '#1c2740',
  ownBubbleColor: '#8c3560',
  ownBubbleTextColor: '#e9ecf5',

  headerColor: '#0b101d',
  headerTextColor: '#e9ecf5',
  inputColor: '#0b101d',
  inputTextColor: '#e9ecf5',
  sendButtonColor: '#ff5fa2',
  sendButtonTextColor: '#080c16',

  embedCardColor: '#0b101d',
  tooltipColor: '#0f1526',
  tooltipTextColor: '#e9ecf5',
  emojiPickerColor: '#0f1526',
  loaderColor: '#ff5fa2',
  iconButtonColor: '#8e9ab8',
  iconButtonHoverColor: '#ff5fa2',
  dangerColor: '#ff6b81',

  fontFamily:
    'ui-monospace, "SF Mono", "JetBrains Mono", Menlo, Consolas, monospace',
  fontSize: 'md',
} as const;

export const CHERRY_CHAT_LAYOUT = {
  showHeader: true,
  headerTitle: 'THE CHERRY TREE',
  showMemberCount: true,
  showInput: true,
} as const;
