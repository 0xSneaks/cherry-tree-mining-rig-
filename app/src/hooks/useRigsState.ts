'use client';

/**
 * Authoritative chain state plus the lifecycle handling that keeps it honest.
 *
 * Spec §8/§10:
 * - reconcile on a fixed interval, on every action, and on every `resumed`
 * - the ticker must *stop* on `suspended`
 * - on `resumed`, re-anchor to on-chain state rather than resuming
 *   interpolation from a stale anchor
 * - `walletDisconnected` degrades to read-only without unmounting state
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import type { Address } from '@solana/kit';

import { fetchSnapshot } from '../lib/chain';
import type { ChainSnapshot } from '../lib/program/accrual';

const RECONCILE_INTERVAL_MS = 20_000;

export type RigsState = {
  snapshot: ChainSnapshot | null;
  loading: boolean;
  error: Error | null;
  /** False while suspended — the ticker must not run. */
  live: boolean;
  /** Re-read the chain and re-anchor interpolation. */
  refresh: () => Promise<void>;
};

export type LifecycleSource = {
  on(event: 'suspended' | 'resumed' | 'walletDisconnected', fn: () => void): void;
  off(event: 'suspended' | 'resumed' | 'walletDisconnected', fn: () => void): void;
} | null;

export function useRigsState(
  owner: Address | null,
  lifecycle: LifecycleSource,
): RigsState {
  const [snapshot, setSnapshot] = useState<ChainSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [live, setLive] = useState(true);

  // Guards against a slow in-flight fetch overwriting a newer one.
  const generation = useRef(0);

  const refresh = useCallback(async () => {
    const mine = ++generation.current;
    try {
      const next = await fetchSnapshot(owner);
      if (mine !== generation.current) return;
      setSnapshot(next);
      setError(null);
    } catch (e) {
      if (mine !== generation.current) return;
      setError(e instanceof Error ? e : new Error(String(e)));
    } finally {
      if (mine === generation.current) setLoading(false);
    }
  }, [owner]);

  useEffect(() => {
    setLoading(true);
    void refresh();
  }, [refresh]);

  // Fixed-interval reconciliation, paused while suspended.
  useEffect(() => {
    if (!live) return;
    const id = setInterval(() => void refresh(), RECONCILE_INTERVAL_MS);
    return () => clearInterval(id);
  }, [live, refresh]);

  // Cherry lifecycle.
  useEffect(() => {
    if (!lifecycle) return;

    const onSuspended = () => setLive(false);
    const onResumed = () => {
      // Re-anchor from the chain BEFORE the ticker restarts. Resuming
      // interpolation from the pre-suspend anchor is how the display drifts
      // away from the chain.
      void refresh().finally(() => setLive(true));
    };

    lifecycle.on('suspended', onSuspended);
    lifecycle.on('resumed', onResumed);
    return () => {
      lifecycle.off('suspended', onSuspended);
      lifecycle.off('resumed', onResumed);
    };
  }, [lifecycle, refresh]);

  // A backgrounded WebView may not always emit `suspended`; the page
  // visibility API is the browser-level backstop for the same condition.
  useEffect(() => {
    if (typeof document === 'undefined') return;
    const onVisibility = () => {
      if (document.visibilityState === 'hidden') {
        setLive(false);
      } else {
        void refresh().finally(() => setLive(true));
      }
    };
    document.addEventListener('visibilitychange', onVisibility);
    return () => document.removeEventListener('visibilitychange', onVisibility);
  }, [refresh]);

  return { snapshot, loading, error, live, refresh };
}
