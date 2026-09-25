'use client';

/**
 * Mode selection — the only place the app branches.
 *
 * Detection is strict: strict mode accepts `window.__cherry` (WebView) or
 * `cherry_embed=1` (iframe) and nothing else, which keeps a wallet's in-app
 * browser from being mistaken for Cherry (spec §8, §12.20).
 */
import { useEffect, useState } from 'react';
import {
  CherryMiniAppProvider,
  useCherryEnvironment,
} from '@cherrydotfun/miniapp-sdk/react';

import { PRODUCT_NAME } from '../lib/brand';
import { EmbeddedGame } from './EmbeddedGame';
import { StandaloneGame } from './StandaloneGame';

export function GameRoot() {
  const { isEmbedded } = useCherryEnvironment({ strict: true });

  // Detection needs `window`, so the server and the first client render must
  // agree on something neutral or React reports a hydration mismatch. The
  // shell below is what the server emits; the real tree mounts immediately
  // after, which is still "no user action" for first paint.
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);

  if (!mounted) {
    return (
      <main className="shell">
        <h1 className="wordmark">{PRODUCT_NAME}</h1>
        <p className="note" style={{ marginTop: 10 }}>
          Spinning up the shaft…
        </p>
      </main>
    );
  }

  return isEmbedded ? (
    <CherryMiniAppProvider strict>
      <EmbeddedGame />
    </CherryMiniAppProvider>
  ) : (
    <StandaloneGame />
  );
}
