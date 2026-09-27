'use client';

/**
 * Referrals — how they work, your standing, and the season board, from
 * `/api/referrals`. The standings here (count, tier, rank) are status. The
 * on-chain referral commission is separate: it accrues in the referrer's
 * ledger and is claimed from the Referral commission panel. Any season prize
 * is separately funded and disclosed. The link itself is copied from the
 * Share the grove card, which follows this panel.
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

export function ReferralPanel({
  wallet,
  hasPosition,
}: {
  wallet: string | null;
  hasPosition: boolean;
}) {
  const [result, setResult] = useState<{ key: string; data: Api } | null>(null);
  const [season, setSeason] = useState<string | null>(null);
  const requestKey = `${wallet ?? 'disconnected'}:${season ?? 'all'}`;

  useEffect(() => {
    let cancelled = false;
    const q = new URLSearchParams();
    if (wallet) q.set('wallet', wallet);
    if (season) q.set('season', season);
    fetch(`/api/referrals?${q.toString()}`)
      .then((r) => (r.ok ? (r.json() as Promise<Api>) : null))
      .then((j) => {
        if (!cancelled && j && 'leaderboard' in j) setResult({ key: requestKey, data: j });
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [wallet, season, requestKey]);

  const data = result?.key === requestKey ? result.data : null;

  return (
    <details className="panel" data-testid="referral-panel">
      <summary className="section-title">Referrals</summary>
      {wallet && hasPosition ? (
        <p className="note">
          A new planter who uses your link on their first planting gets the 1.05× referred weight on the same
          funded emission. As their direct referrer you accrue an on-chain commission on the protocol fees their
          seed purchases and replants pay: up to 20% of each fee, with 80% kept by the Protocol Treasury, and clamped
          so the treasury always keeps 8% of the amount planted — so at fees below 10% it is smaller, and at 8% or
          below it is zero. It is reserved in the fee vault and claimed from Referral commission. One level only.
          Copy your link from Share the grove.
        </p>
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
