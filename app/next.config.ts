import type { NextConfig } from 'next';

import { cherryChatEnabled } from './src/lib/cherryChat';
import { securityHeaders } from './src/lib/csp';

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
        async headers() {
          const dev = process.env.NODE_ENV !== 'production';
          return [{ source: '/(.*)', headers: securityHeaders({ dev }) }];
        },
      }),
};

export default nextConfig;
