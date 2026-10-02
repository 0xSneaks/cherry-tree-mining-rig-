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
 *     make that promise false and show an empty card. Inside Cherry the
 *     snapshot is the only source: a field the host did not hand over is left
 *     blank, never filled from the query — everything after `token` in a
 *     shared URL is attacker-controlled.
 *  2. Query fields — only on a page positively classified as standalone.
 *
 * Which of the two applies is decided once, by `classifyResultEnvironment`
 * (`lib/resultSnapshot.ts`); ambiguous or conflicting launch signals get the
 * neutral card and read neither.
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

import { ResultCard } from '../../components/ResultCard';
import {
  readResultEnvironment,
  resolveSignedSnapshot,
  resolveStandaloneQuery,
  type Classification,
  type TrustedCherryContext,
} from '../../lib/resultSnapshot';

/** Inside Cherry: the host's snapshot, and nothing else. */
function BlinkResult({ context }: { context: TrustedCherryContext }) {
  const params = useCherryBlink()?.params;
  return <ResultCard params={resolveSignedSnapshot(context, params)} />;
}

/** Standalone: nothing to read but the query. */
function QueryResult() {
  const sp = useSearchParams();
  return <ResultCard params={resolveStandaloneQuery('standalone', sp)} />;
}

export default function ResultPage() {
  // Detection needs `window`, so the server render and the first client
  // render have to agree on the neutral card below.
  const [env, setEnv] = useState<Classification | undefined>(undefined);
  useEffect(() => setEnv(readResultEnvironment()), []);

  if (env === undefined) return <ResultCard params={{}} />;

  if (env.context !== null) {
    return (
      <CherryMiniAppProvider>
        <BlinkResult context={env.context} />
      </CherryMiniAppProvider>
    );
  }

  if (env.env === 'standalone') {
    // `useSearchParams` needs a Suspense boundary to prerender.
    return (
      <Suspense fallback={<ResultCard params={{}} />}>
        <QueryResult />
      </Suspense>
    );
  }

  // Ambiguous or spoofed launch signals: neither the snapshot nor the query.
  return <ResultCard params={{}} />;
}
