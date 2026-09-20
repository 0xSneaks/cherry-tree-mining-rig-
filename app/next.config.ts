import type { NextConfig } from 'next';

/**
 * Cherry embeds the mini-app in an iframe on Cherry web, so every HTML
 * response needs `frame-ancestors` naming chat.cherry.fun (spec §8, SDK
 * step 2b). `X-Frame-Options` is deliberately never set: a stray
 * DENY/SAMEORIGIN silently blocks the iframe in Chrome and Firefox.
 */
const CSP = [
  "frame-ancestors 'self' https://chat.cherry.fun",
  "default-src 'self'",
  // Solana RPC + the Cherry JWKS endpoint.
  "connect-src 'self' https: wss:",
  "img-src 'self' data: https:",
  "style-src 'self' 'unsafe-inline'",
  "script-src 'self' 'unsafe-inline' 'unsafe-eval'",
  "base-uri 'self'",
  "form-action 'self'",
].join('; ');

const nextConfig: NextConfig = {
  async headers() {
    return [
      {
        source: '/(.*)',
        headers: [
          { key: 'Content-Security-Policy', value: CSP },
          { key: 'Referrer-Policy', value: 'no-referrer' },
          { key: 'X-Content-Type-Options', value: 'nosniff' },
        ],
      },
    ];
  },
};

export default nextConfig;
