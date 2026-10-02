'use client';

/**
 * Your slice of a fixed funded schedule, and how it shrinks as more seeds are
 * planted. Arithmetic on current program state — never a forecast, and never
 * shown as a projection when there is no funded emission.
 */
import { useMemo, useState } from 'react';

import { formatCount, formatDuration, formatSol } from '../lib/format';
import type { Config, Miner } from '../lib/program/accounts';
import { DILUTION_EXPLAINER, dilutionView, whatIfSeedsAdded } from '../lib/growth/dilution';
import { Readout } from './Readout';

const pct = (bps: number) => `${(bps / 100).toFixed(2)}%`;

export function DilutionPanel({
  config,
  miner,
  chainTime,
}: {
  config: Config;
  miner: Miner | null;
  chainTime: bigint;
}) {
  const view = useMemo(() => dilutionView(config, miner, chainTime), [config, miner, chainTime]);
  const [added, setAdded] = useState<bigint>(10_000n);
  const whatIf = useMemo(() => whatIfSeedsAdded(config, miner, chainTime, added), [config, miner, chainTime, added]);

  return (
    <details className="panel" data-testid="dilution-panel">
      <summary className="section-title">Your share &amp; dilution</summary>
      <Readout label="Your seeds · shares" value={`${formatCount(view.userRigs)} · ${formatCount(view.userShares)}`} />
      <Readout label="All seeds · shares" value={`${formatCount(view.totalRigs)} · ${formatCount(view.totalShares)}`} />
      <Readout label="Your share of emission" value={pct(view.shareBps)} />
      {view.projectionsAllowed ? (
        <>
          <Readout label="Est. distribution to you / day" value={formatSol(view.perDayLamports, 6)} unit="SOL" />
          <Readout
            label={`Your slice of the remaining ${formatDuration(view.runwaySeconds)} (if shares stay flat)`}
            value={formatSol(view.remainingFundedLamports, 6)}
            unit="SOL"
          />
          <table className="note" style={{ width: '100%', marginTop: 8 }} data-testid="dilution-table">
            <thead>
              <tr>
                <th align="left">If total shares grow by</th>
                <th align="right">Your share</th>
                <th align="right">Per day</th>
              </tr>
            </thead>
            <tbody>
              {view.rows.map((r) => (
                <tr key={r.increaseBps}>
                  <td>{r.increaseBps === 0 ? 'today' : `+${r.increaseBps / 100}%`}</td>
                  <td align="right">{pct(r.shareBps)}</td>
                  <td align="right">{formatSol(r.perDayLamports, 6)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <label className="note" style={{ display: 'block', marginTop: 8 }}>
            What if this many more seeds were planted:{' '}
            <input
              inputMode="numeric"
              aria-label="Additional seeds planted by others"
              value={added.toString()}
              onChange={(e) => {
                const d = e.target.value.replace(/\D/g, '').slice(0, 13);
                setAdded(d ? BigInt(d) : 0n);
              }}
              style={{ width: 120 }}
            />
          </label>
          <p className="note" data-testid="dilution-whatif">
            Your share would be {pct(whatIf.shareBps)} · about {formatSol(whatIf.perDayLamports, 6)} SOL / day at
            today&apos;s funded rate.
          </p>
        </>
      ) : (
        <p className="note">
          No funded emission right now, so there is nothing to project: future distribution is 0 until a new
          schedule is separately funded.
        </p>
      )}
      <p className="note">{DILUTION_EXPLAINER}</p>
    </details>
  );
}
