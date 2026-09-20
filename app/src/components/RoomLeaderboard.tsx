'use client';

/**
 * Top miners among the people who launched from this room.
 *
 * Display only. Room attribution is stored off-chain and has zero effect on
 * reward math — the accumulator does not know rooms exist (spec §8).
 */
import { useEffect, useState } from 'react';

import { formatCount, shortAddress } from '../lib/format';

export type LeaderboardEntry = {
  address: string;
  rigs: string;
  displayName?: string;
};

export function RoomLeaderboard({
  roomId,
  roomTitle,
  launchToken,
  viewer,
  onOpenProfile,
}: {
  roomId: string;
  roomTitle: string;
  launchToken: string | null;
  viewer: string | null;
  onOpenProfile?: (address: string) => void;
}) {
  const [entries, setEntries] = useState<LeaderboardEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const res = await fetch(
          `/api/room/${encodeURIComponent(roomId)}/leaderboard`,
          {
            headers: launchToken
              ? { authorization: `Bearer ${launchToken}` }
              : undefined,
          },
        );
        if (!res.ok) throw new Error(`leaderboard unavailable (${res.status})`);
        const body = (await res.json()) as { entries: LeaderboardEntry[] };
        if (!cancelled) setEntries(body.entries);
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [roomId, launchToken]);

  return (
    <section>
      <h2 className="section-title">This room</h2>
      <div className="panel">
        <p
          style={{
            margin: '0 0 10px',
            fontSize: 12,
            color: 'var(--text-dim)',
          }}
        >
          Top miners in {roomTitle}
        </p>
        {error ? (
          <p className="note">{error}</p>
        ) : !entries ? (
          <p className="note">Reading the chain…</p>
        ) : entries.length === 0 ? (
          <p className="note">
            No rigs mined from this room yet. Be the first shaft.
          </p>
        ) : (
          <ol className="leaderboard">
            {entries.map((e, i) => (
              <li key={e.address} className={e.address === viewer ? 'is-you' : undefined}>
                <span className="rank">{i + 1}</span>
                <button
                  type="button"
                  className="who"
                  onClick={() => onOpenProfile?.(e.address)}
                >
                  {e.displayName ?? shortAddress(e.address)}
                </button>
                <span style={{ fontVariantNumeric: 'tabular-nums' }}>
                  {formatCount(BigInt(e.rigs))}
                </span>
              </li>
            ))}
          </ol>
        )}
      </div>
    </section>
  );
}
