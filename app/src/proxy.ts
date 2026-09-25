/**
 * Request-time CSP for the Node deployment — the layer that can mint a
 * nonce. (Next.js 16 renamed the middleware convention to "proxy"; the
 * file is inert in a static export, where `scripts/emit-static-headers.ts`
 * serves a hash-based policy instead.)
 *
 * Next.js inlines a bootstrap script and the RSC flight data into every
 * document, which is why the old policy needed `'unsafe-inline'`. When this
 * layer sets a CSP *on the request* carrying `'nonce-…'`, Next parses the
 * nonce out (server/app-render/get-script-nonce-from-header) and stamps it
 * on every script it emits — inline and external alike. The same CSP on the
 * *response* is what the browser enforces, so the two must be built
 * together, which is why both come from one `buildCsp` call.
 *
 * The cost the repo docs weighed: a nonce is only knowable per request, so
 * pages render dynamically instead of being served from the build-time
 * prerender. DEPLOY.md records that as the accepted pre-mainnet trade.
 *
 * `Cache-Control: private, no-store` is part of the nonce's correctness,
 * not an extra hardening flourish: a document replayed from a shared cache
 * would carry a nonce the attacker has already seen, and one replayed from
 * a private cache would break outright when the fresh header disagrees.
 * Static assets are outside the matcher and keep their immutable caching.
 */
import { NextResponse, type NextRequest } from 'next/server';

import { buildCsp, nonceScriptSources } from './lib/csp';

export default function proxy(request: NextRequest) {
  const nonce = crypto.randomUUID();
  const dev = process.env.NODE_ENV !== 'production';
  const csp = buildCsp({ dev, scriptSrc: nonceScriptSources(nonce) });

  const requestHeaders = new Headers(request.headers);
  // The copy on the request is what Next reads to stamp its scripts.
  requestHeaders.set('content-security-policy', csp);

  const response = NextResponse.next({ request: { headers: requestHeaders } });
  // The copy on the response is what the browser enforces.
  response.headers.set('Content-Security-Policy', csp);
  response.headers.set('Cache-Control', 'private, no-store');
  return response;
}

export const config = {
  // Everything except immutable build assets, which a nonce policy has no
  // business un-caching, and which never execute in a document context.
  matcher: [
    '/((?!_next/static|_next/image|favicon.ico|.*\\.(?:svg|png|jpg|jpeg|gif|webp|ico|txt|xml|webmanifest)$).*)',
  ],
};
