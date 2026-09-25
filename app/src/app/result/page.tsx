'use client';

/**
 * `/result` — the blink render target.
 *
 * A pure render of a snapshot. No chain reads, no state, no callbacks.
 *
 * Where the snapshot comes from, in order:
 *
 *  1. `useCherryBlink().params` — what the Cherry host hands the card over the
 *     bridge when it renders an inline blink (`mode=inline`) or the live
 *     preview shown in the share picker (`mode=preview`). This is the real
 *     path: `share()` is called with `previewable: true`, which promises the
 *     host that this route renders correctly from `route` + `params` alone,
 *     with no `messageId` and no room. Reading the query string instead would
 *     make that promise false and show an empty card.
 *  2. Query fields — standalone and development only.
 *
 * The provider is mounted only for an inline/preview launch. Outside one,
 * `CherryMiniApp.init()` waits on a `cherry:init` the host will never send and
 * times out, and the Cherry hooks throw without a provider above them — so a
 * plain browser hit gets the query-only path and no provider at all.
 *
 * `/inline` renders the same card server-side from the *verified* launch
 * token on a deployment that has a server; this page is the client-side
 * equivalent, and is what a static deployment serves.
 */
import { Suspense, useEffect, useState } from 'react';
import { useSearchParams } from 'next/navigation';
import { CherryMiniAppProvider, useCherryBlink } from '@cherrydotfun/miniapp-sdk/react';

import { ResultCard, type ResultParams } from '../../components/ResultCard';

/**
 * The SDK reads its launch parameters from the query string and then the hash;
 * `mode` is what marks an inline or preview render. Mirrored here because the
 * decision of whether to mount the provider has to be made above it.
 */
function launchMode(): string | null {
  if (typeof window === 'undefined') return null;
  try {
    const fromSearch = new URLSearchParams(window.location.search).get('mode');
    if (fromSearch !== null) return fromSearch;
    const hash = window.location.hash.replace(/^#/, '');
    return new URLSearchParams(hash).get('mode');
  } catch {
    return null;
  }
}

function str(v: unknown): string | undefined {
  return typeof v === 'string' ? v : typeof v === 'number' ? String(v) : undefined;
}

function toParams(source: (key: string) => string | undefined): ResultParams {
  return {
    kind: source('kind'),
    rigs: source('rigs'),
    compounded: source('compounded'),
    claimed: source('claimed'),
    rooted: source('rooted'),
    milestone: source('milestone'),
    ref: source('ref'),
  } satisfies ResultParams;
}

/** Inside Cherry: the host's snapshot, with the query as a last resort. */
function BlinkResult() {
  const blink = useCherryBlink();
  const sp = useSearchParams();
  const params = blink?.params;

  return (
    <ResultCard
      params={toParams((key) => str(params?.[key]) ?? sp.get(key) ?? undefined)}
    />
  );
}

/** Standalone: nothing to read but the query. */
function QueryResult() {
  const sp = useSearchParams();
  return <ResultCard params={toParams((key) => sp.get(key) ?? undefined)} />;
}

export default function ResultPage() {
  // `launchMode` needs `window`, so the server render and the first client
  // render have to agree on the neutral card below.
  const [mode, setMode] = useState<string | null | undefined>(undefined);
  useEffect(() => setMode(launchMode()), []);

  if (mode === undefined) return <ResultCard params={{}} />;

  // `useSearchParams` needs a Suspense boundary to prerender.
  if (mode === 'inline' || mode === 'preview') {
    return (
      <CherryMiniAppProvider>
        <Suspense fallback={<ResultCard params={{}} />}>
          <BlinkResult />
        </Suspense>
      </CherryMiniAppProvider>
    );
  }

  return (
    <Suspense fallback={<ResultCard params={{}} />}>
      <QueryResult />
    </Suspense>
  );
}
