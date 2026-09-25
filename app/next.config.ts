import type { NextConfig } from 'next';

import { cherryChatEnabled } from './src/lib/cherryChat';
import { staticSecurityHeaders } from './src/lib/csp';

/**
 * Static-export mode, used by the GitHub Pages workflow.
 *
 * A static host serves the game itself completely — every figure on screen is
 * read from Solana in the browser, and buy / claim / compound are signed by
 * the user's wallet. What it cannot serve is the launch-token backend
 * (`/api/*`) and the server-rendered blink (`/inline`); the Pages workflow
 * drops those routes before building. Both of those already require a
 * `CHERRY_APP_ID` from Cherry registration, so a static deployment loses
 * nothing that works without one.
 *
 * The caveat that matters: a static *host* is what sends headers, not Next,
 * and `frame-ancestors` is ignored in a <meta> tag. So the export also writes
 * a `_headers` file (`scripts/emit-static-headers.ts`) carrying the same
 * policy. Hosts that read it — Cloudflare Pages, Netlify — then satisfy
 * Cherry web's iframe requirement with no code change. GitHub Pages ignores
 * it and cannot send headers at all, which is fine for Cherry's mobile
 * WebView and for a standalone browser, and not enough for Cherry web.
 */
const STATIC_EXPORT = process.env.STATIC_EXPORT === '1';

/**
 * The chat ids as Vercel holds them — `CHERRY_CHAT_APP_ID` / `_ROOM_ID`,
 * unprefixed — bridged onto the `NEXT_PUBLIC_` names the code reads.
 *
 * Next only inlines `NEXT_PUBLIC_*` into the browser bundle, so unprefixed ids
 * left the client config null: no chat, `frame-src 'none'`, and a token route
 * answering 503. Both ids are public identifiers; only these two are bridged.
 * `CHERRY_APP_SECRET` is never touched here and stays server-only.
 *
 * Assigning `process.env` covers this file's own reads (the CSP and the
 * static-export guard below); the `env` block inlines the same values into
 * the client and server bundles.
 */
const CHAT_ID_BRIDGE = {
  NEXT_PUBLIC_CHERRY_CHAT_APP_ID: 'CHERRY_CHAT_APP_ID',
  NEXT_PUBLIC_CHERRY_CHAT_ROOM_ID: 'CHERRY_CHAT_ROOM_ID',
} as const;
const chatEnv: Record<string, string> = {};
for (const [publicName, serverName] of Object.entries(CHAT_ID_BRIDGE)) {
  const value = process.env[publicName] || process.env[serverName];
  if (value) {
    process.env[publicName] = value;
    chatEnv[publicName] = value;
  }
}

/**
 * The chat embed authenticates with `app-trusted+wallet`, which needs
 * `/api/cherry-embed-nonce` and `/api/cherry-embed-token` — and a static
 * export has no server to run them on (the Pages workflow deletes
 * `src/app/api` outright). Configuring the embed into a static build would
 * ship a chat whose every sign-in attempt 404s.
 *
 * Fail the build instead of discovering it in production.
 */
if (STATIC_EXPORT && cherryChatEnabled()) {
  throw new Error(
    'The Cherry Chat embed is configured (NEXT_PUBLIC_CHERRY_CHAT_APP_ID / ' +
      '_ROOM_ID) but STATIC_EXPORT=1. Its auth mode is app-trusted+wallet, ' +
      'which needs the /api/cherry-embed-* routes, and a static export has no ' +
      'server to serve them. Either deploy to a host that runs Node, or leave ' +
      'the chat ids unset for the static build.',
  );
}

/** Repo-name subpath, e.g. "/cherry-tree-mining-rig-" on GitHub Pages. */
const BASE_PATH = process.env.NEXT_PUBLIC_BASE_PATH ?? '';

const nextConfig: NextConfig = {
  env: chatEnv,
  ...(STATIC_EXPORT
    ? {
        output: 'export' as const,
        // Pages serves directories, so /result needs /result/index.html.
        trailingSlash: true,
        ...(BASE_PATH ? { basePath: BASE_PATH, assetPrefix: BASE_PATH } : {}),
        images: { unoptimized: true },
      }
    : {
        // `headers()` has no effect in a static export — the host sends them.
        // The CSP is absent on purpose: it carries a per-request nonce, so
        // `src/proxy.ts` serves it. A CSP built here would be nonce-less, and
        // serving both would enforce their intersection — the static one's
        // `'unsafe-inline'`-free script-src would block everything the proxy
        // just nonced, or vice versa.
        async headers() {
          return [{ source: '/(.*)', headers: staticSecurityHeaders }];
        },
      }),
};

export default nextConfig;
