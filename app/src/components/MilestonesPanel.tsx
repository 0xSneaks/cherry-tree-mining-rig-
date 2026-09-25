'use client';

/**
 * Grove milestones — status and cosmetics from the permanent position.
 * Nothing here pays, gates, expires or resets. Missing a day costs nothing.
 */
import { useEffect, useState } from 'react';

import { formatCount, formatDuration } from '../lib/format';
import type { Config, Miner } from '../lib/program/accounts';
import {
  achievements,
  groveTier,
  isRooted,
  nextGroveTier,
  parseVisitLog,
  recordVisit,
  reminders,
  type VisitLog,
} from '../lib/growth/milestones';
import { Readout } from './Readout';

const VISIT_KEY = 'cherry-tree:days-tended';

function useDaysTended(chainTime: bigint): VisitLog | null {
  // Read once when the panel first mounts (always client-side, after the
  // chain snapshot arrives); persisted in an effect. Storage unavailable
  // (private mode, blocked) simply hides the readout.
  const [log] = useState<VisitLog | null>(() => {
    const day = Number(chainTime / 86_400n);
    if (typeof window === 'undefined' || day <= 0) return null;
    try {
      return recordVisit(parseVisitLog(window.localStorage.getItem(VISIT_KEY)), day);
    } catch {
      return null;
    }
  });
  useEffect(() => {
    if (!log) return;
    try {
      window.localStorage.setItem(VISIT_KEY, JSON.stringify(log));
    } catch {
      // ignore
    }
  }, [log]);
  return log;
}

export function MilestonesPanel({ config, miner, chainTime }: { config: Config; miner: Miner; chainTime: bigint }) {
  const tier = groveTier(miner.rigs);
  const next = nextGroveTier(miner.rigs);
  const list = achievements(miner, config, chainTime);
  const earned = list.filter((a) => a.earned);
  const upcoming = reminders(miner, config, chainTime);
  const days = useDaysTended(chainTime);

  return (
    <details className="panel" data-testid="milestones-panel">
      <summary className="section-title">Grove milestones</summary>
      <Readout label="Grove status" value={tier ? tier.label : '—'} />
      <Readout
        label="Next status"
        value={next ? `${next.tier.label} · ${formatCount(next.seedsToGo)} seeds to go` : 'Highest status reached'}
      />
      <Readout label="Claims" value={isRooted(miner, chainTime) ? 'Rooted · penalty-free' : 'Early-claim band active'} />
      {days ? <Readout label="Days tended (this device)" value={formatCount(days.days)} /> : null}
      <p className="note" style={{ marginTop: 8 }}>
        Earned {earned.length} of {list.length}:
      </p>
      <ul className="note" data-testid="achievements">
        {list.map((a) => (
          <li key={a.id} data-earned={a.earned} style={{ opacity: a.earned ? 1 : 0.45 }}>
            {a.earned ? '✿' : '○'} {a.label} — {a.detail}
          </li>
        ))}
      </ul>
      {upcoming.length ? (
        <>
          <p className="note">Coming up:</p>
          <ul className="note" data-testid="reminders">
            {upcoming.map((r) => (
              <li key={r.id}>
                in {formatDuration(r.atSeconds - chainTime)} — {r.text}
              </li>
            ))}
          </ul>
        </>
      ) : null}
      <p className="note">
        Milestones are status only. They never expire, never reset and never require another purchase.
      </p>
    </details>
  );
}
