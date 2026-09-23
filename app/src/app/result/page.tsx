'use client';

/**
 * `/result` — the blink render target.
 *
 * A pure render of `params`. No chain reads, no state, no callbacks. The
 * authoritative copy of these params lives inside the signed launch token;
 * `/inline` renders that server-side on a deployment that has a server.
 *
 * Read on the client rather than from server `searchParams` so the page is
 * also valid in a fully static export (GitHub Pages), where there is no
 * request to read. It shows only what it is handed either way.
 */
import { Suspense } from 'react';
import { useSearchParams } from 'next/navigation';

import { ResultCard, type ResultParams } from '../../components/ResultCard';

function ResultFromQuery() {
  const sp = useSearchParams();
  const pick = (k: string): string | undefined => sp.get(k) ?? undefined;

  return (
    <ResultCard
      params={
        {
          kind: pick('kind'),
          rigs: pick('rigs'),
          compounded: pick('compounded'),
          claimed: pick('claimed'),
        } satisfies ResultParams
      }
    />
  );
}

export default function ResultPage() {
  // `useSearchParams` needs a Suspense boundary to prerender.
  return (
    <Suspense fallback={<ResultCard params={{}} />}>
      <ResultFromQuery />
    </Suspense>
  );
}
