'use client';

/**
 * Referrals — your link, your standing, and the season board, from
 * `/api/referrals`. Status only: the program pays referrers nothing and has no
 * levels. Any season prize is separately funded and disclosed.
 */
import { useEffect, useState } from 'react';

import { shortAddress } from '../lib/format';
import { Readout } from './Readout';

type Row = { referrer: string; rank: number; qualified: number; total: number; tier: { label: string } | null };
type SeasonView = { id: string; name: string; state: string; endsAt: string; prizes: { description: string; funder: string; disclosureUrl: string }[] };
type Api = {
  qualification: { minFreshSeeds: string; minHoldHours: string } | null;
  current: SeasonView | null;
  past: SeasonView[];
  leaderboard: { status: 'open'; rows: Row[] } | { status: 'closed' } | { status: 'notACompetition' };
  me: { total: number; qualified: number; pending: number; tier: { label: string } | null; configured: boolean } | null;
  note: string;
};

export function ReferralPanel({ wallet, hasPosition }: { wallet: string | null; hasPosition: boolean }) {
  const [data, setData] = useState<Api | null>(null);
  const [copied, setCopied] = useState(false);
  const [season, setSeason] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const q = new URLSearchParams();
    if (wallet) q.set('wallet', wallet);
    if (season) q.set('season', season);
    fetch(`/api/referrals?${q.toString()}`)
      .then((r) => (r.ok ? (r.json() as Promise<Api>) : null))
      .then((j) => {
        if (!cancelled && j && 'leaderboard' in j) setData(j);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [wallet, season]);

  const link =
    wallet && typeof window !== 'undefined'
      ? `${window.location.origin}${window.location.pathname}?ref=${wallet}`
      : null;

  return (
    <details className="panel" data-testid="referral-panel">
      <summary className="section-title">Referrals</summary>
      {link && hasPosition ? (
        <>
          <p className="note">
            Share your link. A new planter who uses it on their first planting gets the 1.05× referred weight on the
            same funded emission. What you get is status here — no payout, no levels.
          </p>
          <button
            type="button"
            onClick={() => {
              void navigator.clipboard?.writeText(link).then(() => setCopied(true), () => setCopied(false));
            }}
          >
            {copied ? 'Link copied' : 'Copy referral link'}
          </button>
        </>
      ) : (
        <p className="note">Your referral link appears after your first planting — a referrer must already be a planter.</p>
      )}

      {data?.me ? (
        <>
          <Readout label="Wallets that used your link" value={data.me.total} />
          <Readout
            label="Qualified referrals"
            value={data.me.configured ? `${data.me.qualified} · ${data.me.pending} pending` : 'not yet published'}
          />
          {data.me.tier ? <Readout label="Referral status" value={data.me.tier.label} /> : null}
        </>
      ) : null}
      {data?.qualification ? (
        <p className="note">
          A referral qualifies once the new planter has bought at least {data.qualification.minFreshSeeds} seeds with
          their own SOL (replanting does not count) and held for {data.qualification.minHoldHours}h. Creating a wallet
          never counts.
        </p>
      ) : null}

      {data ? (
        <>
          <div className="quick" style={{ marginTop: 8 }}>
            <button type="button" aria-pressed={season === null} onClick={() => setSeason(null)}>
              All time
            </button>
            {[...(data.current ? [data.current] : []), ...data.past].map((s) => (
              <button key={s.id} type="button" aria-pressed={season === s.id} onClick={() => setSeason(s.id)}>
                {s.name}
                {s.state === 'active' ? ' · live' : ''}
              </button>
            ))}
          </div>
          {data.leaderboard.status === 'open' ? (
            data.leaderboard.rows.length ? (
              <ol className="leaderboard" data-testid="referral-leaderboard">
                {data.leaderboard.rows.map((r) => (
                  <li key={r.referrer} className={r.referrer === wallet ? 'is-you' : undefined}>
                    <span className="rank">{r.rank}</span>
                    <span className="who">
                      {r.referrer === wallet ? 'you' : shortAddress(r.referrer)}
                      {r.tier ? ` · ${r.tier.label}` : ''}
                    </span>
                    <span>{r.qualified}</span>
                  </li>
                ))}
              </ol>
            ) : (
              <p className="note">No qualified referrals yet.</p>
            )
          ) : (
            <p className="note">The referral board opens once qualification rules are published.</p>
          )}
          {data.current?.prizes.length ? (
            <p className="note">
              Season prizes are separately funded and never come from the reward, capital or fee vault:{' '}
              {data.current.prizes.map((p) => `${p.description} (funded by ${p.funder})`).join('; ')}.
            </p>
          ) : null}
          <p className="note">{data.note}</p>
        </>
      ) : null}
    </details>
  );
}
