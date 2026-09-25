'use client';

import { useEffect, useState } from 'react';
import type { Address } from '@solana/kit';

import { rpc } from '../lib/chain';
import { formatCount, formatSol, formatUtcInstant, shortAddress } from '../lib/format';
import { minerActivity, type MinerActivity } from '../lib/program/activity';
import { eventHistory } from '../lib/program/eventHistory';
import { minerPda } from '../lib/program/pdas';

const NAMES = ['RigsPurchased', 'Claim', 'Compound', 'ReferralBound', 'LaunchActivated'] as const;

export function ActivityHistory({ wallet, refreshKey }: { wallet: Address | null; refreshKey: bigint }) {
  const requestKey = wallet ? `${wallet}:${refreshKey}` : 'disconnected';
  const [result, setResult] = useState<{
    key: string;
    rows: MinerActivity[];
    failed: boolean;
  } | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (!wallet) return;
    void minerPda(wallet)
      .then((account) => eventHistory(rpc(), account, { limit: 100, names: NAMES }))
      .then((history) => {
        if (!cancelled) setResult({ key: requestKey, rows: minerActivity(history, wallet), failed: false });
      })
      .catch(() => {
        if (!cancelled) setResult({ key: requestKey, rows: [], failed: true });
      });
    return () => {
      cancelled = true;
    };
  }, [wallet, refreshKey, requestKey]);

  const rows = result?.key === requestKey ? result.rows : null;
  const failed = result?.key === requestKey && result.failed;

  return (
    <details className="panel" data-testid="activity-history" open>
      <summary className="section-title">My activity</summary>
      {!wallet ? (
        <p className="note">Connect a wallet to read its public grove history.</p>
      ) : rows === null ? (
        <p className="note">Reading your grove history…</p>
      ) : failed ? (
        <div className="banner banner--alert" data-testid="activity-error">
          Activity history is temporarily unavailable. Your on-chain position is unaffected.
        </div>
      ) : rows.length === 0 ? (
        <p className="note" data-testid="activity-empty">No decoded grove activity found for this wallet yet.</p>
      ) : (
        <ol className="activity-list" data-testid="activity-list">
          {rows.map((row, index) => (
            <li key={`${row.signature}-${row.action}-${index}`}>
              <div>
                <strong>{row.action}</strong>
                <span>
                  {row.seeds !== null ? `${formatCount(row.seeds)} seeds` : null}
                  {row.seeds !== null && row.amountLamports !== null ? ' · ' : null}
                  {row.amountLamports !== null ? `${formatSol(row.amountLamports, 6)} SOL` : null}
                  {row.detail ? ` · ${row.detail}` : null}
                </span>
              </div>
              <div className="activity-meta">
                <span>{row.blockTime !== null ? formatUtcInstant(row.blockTime) : 'Timestamp unavailable'}</span>
                {row.explorerUrl ? (
                  <a href={row.explorerUrl} target="_blank" rel="noreferrer noopener" title={row.signature}>
                    {shortAddress(row.signature)} ↗
                  </a>
                ) : (
                  <span>Signature unavailable</span>
                )}
              </div>
            </li>
          ))}
        </ol>
      )}
    </details>
  );
}
