'use client';

/**
 * Funded-runway status: Live / funded window ending soon / Dormant / never
 * funded. Dormant is a first-class state, not an error: runway is 0, nothing
 * further is distributed, everything already accrued stays claimable and
 * backed, and planting does not restart emission.
 */
import { formatDuration, formatSol, formatUtcInstant } from '../lib/format';
import type { Config } from '../lib/program/accounts';
import { runwayReport, STATUS_COPY } from '../lib/growth/runway';
import { Readout } from './Readout';

export function GroveStatus({
  config,
  rewardVaultLamports,
  chainTime,
  claimable,
}: {
  config: Config;
  rewardVaultLamports: bigint;
  chainTime: bigint;
  claimable: bigint;
}) {
  const r = runwayReport(config, rewardVaultLamports, chainTime);
  if (r.status === 'live') return null;
  const copy = STATUS_COPY[r.status];
  return (
    <section
      className={`banner${r.status === 'lowRunway' ? '' : ' banner--alert'}`}
      data-testid="grove-status"
      data-status={r.status}
    >
      <p className="section-title" style={{ marginBottom: 6 }}>
        {copy.title}
      </p>
      <p style={{ margin: '0 0 8px' }}>{copy.body}</p>
      <Readout label="Funded runway" value={r.runwaySeconds > 0n ? formatDuration(r.runwaySeconds) : '0'} />
      <Readout
        label={r.status === 'dormant' ? 'Final schedule ended' : 'Funded until'}
        value={r.periodFinish > 0n ? formatUtcInstant(r.periodFinish) : '—'}
      />
      <Readout label="Future emission (all planters)" value={formatSol(r.futureEmissionLamports, 4)} unit="SOL" />
      <Readout label="Accrued to you · claimable" value={formatSol(claimable, 6)} unit="SOL" />
    </section>
  );
}
