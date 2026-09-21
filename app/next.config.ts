import type { NextConfig } from 'next';

/**
 * Content Security Policy.
 *
 * Cherry embeds the mini-app in an iframe on Cherry web, so every HTML
 * response needs `frame-ancestors` naming chat.cherry.fun (spec §8, SDK step
 * 2b). `X-Frame-Options` is deliberately never set: a stray DENY/SAMEORIGIN
 * silently blocks the iframe in Chrome and Firefox.
 *
 * `connect-src` is derived from configuration rather than left open, so a
 * deployment only permits the origins it actually talks to: its own RPC and
 * the Cherry JWKS host. A blanket `https:` would let any injected script
 * exfiltrate to anywhere, which matters more here than in most apps because
 * the page holds a live wallet session.
 */
const CHERRY_HOST = 'https://chat.cherry.fun';

/** Origin of a URL, or null when it is unset or unparseable. */
function origin(url: string | undefined): string | null {
  if (!url) return null;
  try {
    return new URL(url).origin;
  } catch {
    return null;
  }
}

function connectSrc(): string[] {
  const sources = new Set<string>(["'self'"]);

  // The RPC the browser actually calls, plus its websocket counterpart.
  const rpc = origin(process.env.NEXT_PUBLIC_SOLANA_RPC_URL) ?? origin('https://api.devnet.solana.com');
  if (rpc) {
    sources.add(rpc);
    sources.add(rpc.replace(/^http/, 'ws'));
  }

  // Cherry's JWKS endpoint, and any override pointed elsewhere.
  sources.add(CHERRY_HOST);
  const jwks = origin(process.env.CHERRY_JWKS_URL);
  if (jwks) sources.add(jwks);

  // An escape hatch for deployments behind a proxy or an extra RPC, as a
  // space-separated allowlist. Still an allowlist, never a wildcard.
  for (const extra of (process.env.CSP_EXTRA_CONNECT_SRC ?? '').split(/\s+/)) {
    const o = origin(extra);
    if (o) sources.add(o);
  }

  return [...sources];
}

function buildCsp({ dev }: { dev: boolean }): string {
  const scriptSrc = [
    "'self'",
    // Next.js inlines a bootstrap script into the document. Serving it under
    // a nonce would force every page to render dynamically, which costs the
    // embedded path its prerendered first paint — the one thing the spec is
    // most insistent about. Tracked in DEPLOY.md as pre-mainnet hardening.
    "'unsafe-inline'",
    // Only the dev server needs eval (React Refresh). Production does not.
    ...(dev ? ["'unsafe-eval'"] : []),
  ];

  return [
    "default-src 'self'",
    `script-src ${scriptSrc.join(' ')}`,
    // React sets element styles through the `style` attribute throughout the
    // UI, which CSP counts as inline style.
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data:",
    "font-src 'self' data:",
    `connect-src ${connectSrc().join(' ')}`,
    `frame-ancestors 'self' ${CHERRY_HOST}`,
    // Nothing in this app embeds a frame, loads a plugin, or posts a form.
    "frame-src 'none'",
    "object-src 'none'",
    "form-action 'none'",
    "base-uri 'self'",
    "worker-src 'self'",
    "manifest-src 'self'",
    ...(dev ? [] : ['upgrade-insecure-requests']),
  ].join('; ');
}

const nextConfig: NextConfig = {
  async headers() {
    const dev = process.env.NODE_ENV !== 'production';
    return [
      {
        source: '/(.*)',
        headers: [
          { key: 'Content-Security-Policy', value: buildCsp({ dev }) },
          { key: 'Referrer-Policy', value: 'no-referrer' },
          { key: 'X-Content-Type-Options', value: 'nosniff' },
          { key: 'Cross-Origin-Opener-Policy', value: 'same-origin' },
        ],
      },
    ];
  },
};

export default nextConfig;
