'use client';

/**
 * The visible tick on CLAIMABLE.
 *
 * Display-only interpolation over the last authoritative snapshot. It runs at
 * 5fps: fast enough to read as live, slow enough that a mid-range phone in a
 * WebView is not burning battery on a number that changes in the sixth
 * decimal place (spec §10).
 *
 * The ticker halts entirely when `live` is false, and every value it produces
 * is thrown away the moment `useRigsState` re-anchors.
 */
import { useEffect, useState } from 'react';

import { nowMs } from '../lib/chain';
import {
  projectChainTime,
  projectedClaimable,
  type ChainSnapshot,
} from '../lib/program/accrual';

const TICK_MS = 200; // 5fps

export function useLiveAccrual(
  snapshot: ChainSnapshot | null,
  live: boolean,
): { claimable: bigint; chainTime: bigint } {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    if (!live || !snapshot) return;
    const id = setInterval(() => setTick((t) => t + 1), TICK_MS);
    return () => clearInterval(id);
  }, [live, snapshot]);

  if (!snapshot) return { claimable: 0n, chainTime: 0n };

  // While suspended, freeze on the snapshot's own timestamp rather than
  // projecting forward through time the app did not observe.
  const chainTime = live
    ? projectChainTime(snapshot, nowMs())
    : snapshot.chainTimeSeconds;

  void tick; // the interval drives re-render; the value itself is unused
  return { claimable: projectedClaimable(snapshot, chainTime), chainTime };
}
