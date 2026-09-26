/**
 * Where a blink result card's fields may come from — the one place that
 * decides it.
 *
 * Invariant (restores the fix from `1e25925`): inside a Cherry inline/preview
 * render the host's snapshot is the only source. A field the host did not hand
 * over stays absent; it is never filled from the URL query, which is
 * attacker-controlled in a shared link. The query is read only on a page that
 * is positively standalone.
 *
 * Environment signals, all read from the launch URL and `window`:
 *
 *  - `mode` — every value in the search string *and* the hash (the SDK reads
 *    both). It is URL data, so on its own it proves nothing.
 *  - `cherryEmbedded` — the SDK's strict detection (`window.__cherry` for the
 *    WebView, `cherry_embed=1` for the iframe), the same check `GameRoot` uses.
 *  - the presence of `token` or `cherry_embed` — Cherry launch markers.
 *
 * Classification, in order; the first match wins and nothing falls through to
 * standalone by default:
 *
 *  1. the URL cannot be parsed                                → unknown
 *  2. `mode` appears more than once, in either or both places → unknown
 *  3. exactly one `mode`, `inline` or `preview` (exact case),
 *     and strict Cherry detection agrees                     → inline / preview
 *  4. no `mode`, no `token`, no `cherry_embed`, and strict
 *     detection says standalone                              → standalone
 *  5. anything else (`mode=inline` without Cherry, an unknown
 *     or empty mode, Cherry without a mode, a bare `token`)  → unknown
 *
 * `unknown` renders the neutral card: no snapshot, no query.
 */
import { getCherryEnvironment } from '@cherrydotfun/miniapp-sdk';

import type { ResultParams } from '../components/ResultCard';

/** Every field the result card reads. */
export const RESULT_FIELDS = [
  'kind',
  'rigs',
  'compounded',
  'claimed',
  'rooted',
  'milestone',
  'ref',
] as const;

export type ResultField = (typeof RESULT_FIELDS)[number];

// Fails to compile if `ResultParams` gains a field that is not listed above,
// or the list names one `ResultParams` does not have.
type Exact<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;
const fieldsMatchSchema: Exact<ResultField, keyof ResultParams> = true;
void fieldsMatchSchema;

export type ResultEnvironment = 'inline' | 'preview' | 'standalone' | 'unknown';

declare const trusted: unique symbol;
/** Minted only by `classifyResultEnvironment`, for an inline or preview render. */
export type TrustedCherryContext = {
  readonly mode: 'inline' | 'preview';
  readonly [trusted]: true;
};

export type EnvironmentSignals = {
  search: string;
  hash: string;
  cherryEmbedded: boolean;
};

export type Classification =
  | { env: 'inline' | 'preview'; context: TrustedCherryContext }
  | { env: 'standalone' | 'unknown'; context: null };

const UNKNOWN: Classification = { env: 'unknown', context: null };

export function classifyResultEnvironment(signals: EnvironmentSignals): Classification {
  let search: URLSearchParams;
  let hash: URLSearchParams;
  try {
    search = new URLSearchParams(signals.search);
    hash = new URLSearchParams(signals.hash.replace(/^#/, ''));
  } catch {
    return UNKNOWN;
  }

  const modes = [...search.getAll('mode'), ...hash.getAll('mode')];
  if (modes.length > 1) return UNKNOWN;

  if (modes.length === 1) {
    const mode = modes[0];
    if ((mode === 'inline' || mode === 'preview') && signals.cherryEmbedded === true) {
      return { env: mode, context: { mode } as TrustedCherryContext };
    }
    return UNKNOWN;
  }

  const launchMarker = ['token', 'cherry_embed'].some((k) => search.has(k) || hash.has(k));
  if (!launchMarker && signals.cherryEmbedded === false) {
    return { env: 'standalone', context: null };
  }
  return UNKNOWN;
}

/** Browser adapter: reads the real signals. Any failure is `unknown`. */
export function readResultEnvironment(): Classification {
  if (typeof window === 'undefined') return UNKNOWN;
  try {
    return classifyResultEnvironment({
      search: window.location.search,
      hash: window.location.hash,
      cherryEmbedded: getCherryEnvironment({ strict: true }).isEmbedded,
    });
  } catch {
    return UNKNOWN;
  }
}

/** Own properties only, string or number only; anything else is absent. */
function fromSnapshot(snapshot: unknown): ResultParams {
  const out: ResultParams = {};
  if (typeof snapshot !== 'object' || snapshot === null || Array.isArray(snapshot)) return out;
  for (const key of RESULT_FIELDS) {
    if (!Object.hasOwn(snapshot, key)) continue;
    const v: unknown = (snapshot as Record<string, unknown>)[key];
    if (typeof v === 'string') out[key] = v;
    else if (typeof v === 'number') out[key] = String(v);
  }
  return out;
}

/**
 * Inside Cherry: the host's snapshot, and nothing else. Takes no query input,
 * and can only be called with a context the classifier minted.
 */
export function resolveSignedSnapshot(
  context: TrustedCherryContext,
  snapshot: unknown,
): ResultParams {
  if (context?.mode !== 'inline' && context?.mode !== 'preview') {
    throw new Error('resolveSignedSnapshot requires a trusted Cherry context');
  }
  return fromSnapshot(snapshot);
}

/**
 * The server `/inline` route: `params` from a launch token whose signature has
 * already been verified against Cherry's JWKS.
 */
export function resolveSignedTokenParams(verifiedParams: unknown): ResultParams {
  return fromSnapshot(verifiedParams);
}

/**
 * Standalone only: the query. A field given more than once is ambiguous and is
 * left out; values are still validated by `ResultCard`.
 */
export function resolveStandaloneQuery(env: 'standalone', query: URLSearchParams): ResultParams {
  if (env !== 'standalone') {
    throw new Error('resolveStandaloneQuery is only for a standalone page');
  }
  const out: ResultParams = {};
  for (const key of RESULT_FIELDS) {
    const values = query.getAll(key);
    if (values.length === 1) out[key] = values[0];
  }
  return out;
}
