/**
 * Content Security Policy — the single source.
 *
 * Three consumers read from here, which is the point: `src/proxy.ts` serves
 * a nonce-based policy per request on a Node deployment,
 * `next.config.ts` serves the static (nonce-free) headers on the same
 * deployment, and `scripts/emit-static-headers.ts` writes a hash-based
 * policy into the static export's `_headers` file for a header-capable
 * static host. Hand-maintaining several copies of a security policy is how
 * they drift.
 *
 * `script-src` never carries `'unsafe-inline'`. The inline scripts in every
 * document are Next.js's own — the bootstrap and the RSC flight data — so
 * the policy admits them by nonce on a deployment that can compute one per
 * request, and by build-time hash on a static host that cannot. The throw
 * in `buildCsp` exists so no future emitter can quietly reintroduce the
 * blanket allowance.
 *
 * Cherry embeds the mini-app in an iframe on Cherry web, so every HTML
 * response needs `frame-ancestors` naming chat.cherry.fun (SDK skill step 2b).
 * `X-Frame-Options` is deliberately never set: a stray DENY/SAMEORIGIN
 * silently blocks the iframe in Chrome and Firefox.
 *
 * `connect-src` is derived from configuration rather than left open, so a
 * deployment only permits the origins it actually talks to: its own RPC and
 * the Cherry JWKS host. A blanket `https:` would let any injected script
 * exfiltrate to anywhere, which matters more here than in most apps because
 * the page holds a live wallet session.
 */
import { CHERRY_EMBED_ORIGIN, cherryChatEnabled } from './cherryChat';

export const CHERRY_HOST = 'https://chat.cherry.fun';

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
  const rpc =
    origin(process.env.NEXT_PUBLIC_SOLANA_RPC_URL) ??
    origin('https://api.devnet.solana.com');
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

/**
 * The sources `script-src` admits, supplied by the caller because only it
 * knows which kind of deployment is serving the policy: the request-time
 * proxy passes a nonce, the static emitter passes build-time hashes.
 */
export type ScriptSrcSources = string[];

/**
 * The nonce sources for a Node deployment, where `src/proxy.ts` mints one
 * per request and Next.js stamps it on every script it emits. The format is
 * the base64-value token Next's own parser accepts (`getScriptNonceFromHeader`);
 * anything else makes Next silently drop the nonce and the page then fails
 * closed under the policy below, so a malformed nonce throws instead.
 *
 * `'strict-dynamic'` lets Next's nonced bootstrap load its chunk graph
 * without listing every chunk. `'self'` is the fallback for browsers that
 * predate strict-dynamic — modern ones ignore it.
 */
export function nonceScriptSources(nonce: string): ScriptSrcSources {
  if (!/^[A-Za-z0-9+/_-]+={0,2}$/.test(nonce)) {
    throw new Error(`refusing to serve a CSP with a malformed nonce: ${JSON.stringify(nonce)}`);
  }
  return ["'self'", `'nonce-${nonce}'`, "'strict-dynamic'"];
}

export function buildCsp({ dev, scriptSrc }: { dev: boolean; scriptSrc: ScriptSrcSources }): string {
  if (scriptSrc.includes("'unsafe-inline'")) {
    throw new Error(
      "script-src 'unsafe-inline' is banned: inline scripts must be admitted by nonce or hash",
    );
  }
  const scriptSources = [
    ...scriptSrc,
    // Only the dev server needs eval (React Refresh). Production does not.
    ...(dev ? ["'unsafe-eval'"] : []),
  ];

  return [
    "default-src 'self'",
    `script-src ${scriptSources.join(' ')}`,
    // React sets element styles through the `style` attribute throughout the
    // UI, which CSP counts as inline style.
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data:",
    "font-src 'self' data:",
    `connect-src ${connectSrc().join(' ')}`,
    `frame-ancestors 'self' ${CHERRY_HOST}`,
    // The Cherry Chat embed is the only frame this app ever loads, and only
    // when it is configured. Unconfigured, this stays `'none'` — the embed is
    // opt-in at the policy level too, not just in the component tree.
    `frame-src ${cherryChatEnabled() ? CHERRY_EMBED_ORIGIN : "'none'"}`,
    "object-src 'none'",
    "form-action 'none'",
    "base-uri 'self'",
    "worker-src 'self'",
    "manifest-src 'self'",
    ...(dev ? [] : ['upgrade-insecure-requests']),
  ].join('; ');
}

/**
 * Headers that carry no per-request value — static on every host, so they
 * belong in `next.config.ts` (Node) and in the static export's `_headers`
 * alike. The CSP is deliberately absent: a Node deployment serves it from
 * `src/proxy.ts`, where the nonce lives.
 */
export const staticSecurityHeaders: Array<{ key: string; value: string }> = [
  { key: 'Referrer-Policy', value: 'no-referrer' },
  { key: 'X-Content-Type-Options', value: 'nosniff' },
  { key: 'Cross-Origin-Opener-Policy', value: 'same-origin' },
];

/**
 * Every security header a response should carry, in one list, for emitters
 * that know their script sources — the static-export writer. The proxy sets
 * the CSP itself (see `src/proxy.ts`) and takes the rest from
 * `staticSecurityHeaders`.
 */
export function securityHeaders({
  dev,
  scriptSrc,
}: {
  dev: boolean;
  scriptSrc: ScriptSrcSources;
}): Array<{
  key: string;
  value: string;
}> {
  return [
    { key: 'Content-Security-Policy', value: buildCsp({ dev, scriptSrc }) },
    ...staticSecurityHeaders,
  ];
}
