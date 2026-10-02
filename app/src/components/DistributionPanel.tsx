'use client';

/**
 * Public distribution analytics from `/api/distribution`. Makes concentration
 * visible; does not and cannot prevent one person from using many wallets.
 * Hidden entirely on deployments without the API (static export).
 */
import { useEffect, useState } from 'react';

import { shortAddress } from '../lib/format';
import { Readout } from './Readout';

type Dist = {
  wallets: number;
  largestBps: number;
  top10Bps: number;
  top1PctBps: number;
  giniBps: number;
  medianSeeds: string;
  level: string;
  summary: string;
  whales: { owner: string; seeds: string; shareBps: number }[];
  caveat: string;
};

const pct = (bps: number) => `${(bps / 100).toFixed(2)}%`;

export function DistributionPanel({ viewer }: { viewer: string | null }) {
  const [d, setD] = useState<Dist | null>(null);
  useEffect(() => {
    let cancelled = false;
    fetch('/api/distribution')
      .then((r) => (r.ok ? (r.json() as Promise<Dist>) : null))
      .then((j) => {
        if (!cancelled && j && typeof j.wallets === 'number') setD(j);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);
  if (!d) return null;

  return (
    <details className="panel" data-testid="distribution-panel">
      <summary className="section-title">Distribution</summary>
      <p className="note">{d.summary}</p>
      <Readout label="Wallets planted" value={d.wallets.toLocaleString('en-US')} />
      <Readout label="Largest wallet" value={pct(d.largestBps)} />
      <Readout label="Top 10 wallets" value={pct(d.top10Bps)} />
      <Readout label="Top 1% of wallets" value={pct(d.top1PctBps)} />
      <Readout label="Median wallet" value={`${d.medianSeeds} seeds`} />
      <Readout label="Inequality (Gini)" value={(d.giniBps / 10_000).toFixed(2)} />
      {d.whales.length ? (
        <ul className="note" data-testid="whales">
          {d.whales.map((w) => (
            <li key={w.owner}>
              {w.owner === viewer ? 'you' : shortAddress(w.owner)} holds {pct(w.shareBps)} of all shares
            </li>
          ))}
        </ul>
      ) : null}
      <p className="note">{d.caveat}</p>
    </details>
  );
}
