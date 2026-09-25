'use client';

/**
 * GENESIS YIELD — the headline rate.
 *
 * Every number here is computed by `lib/program/yield.ts`, a port of
 * `rigs_engine::yield_curve` checked row-for-row against the Rust engine, from
 * decoded program state and the chain clock. The APR is the rate the reward
 * accumulator is paying right now; it is never a target, a cap or a promise.
 */
import { useMemo } from 'react';

import { formatCount, formatDuration, formatSol, formatUtcInstant } from '../lib/format';
import type { Config, Miner } from '../lib/program/accounts';
import { WEIGHT_BASE, WEIGHT_REFERRED, fundedRunwaySeconds } from '../lib/program/accrual';
import {
  PHASE_LABEL,
  SECONDS_PER_YEAR,
  aprBps,
  effectiveRateBps,
  formatAprBps,
  hypotheticalPlanting,
  nextThreshold,
  phaseFromAprBps,
  projectCompounding,
  yieldInputs,
  type YieldPhase,
} from '../lib/program/yield';
import { projectionsAllowed } from '../lib/growth/runway';
import { Readout } from './Readout';

const DAY = 86_400n;

const TAGLINE: Record<YieldPhase, string> = {
  genesis: 'Early roots grow fast.',
  earlyGrove: 'The grove is filling in.',
  growth: 'Branches spreading. Rates settling.',
  mature: 'A mature grove. Steadier, lower.',
  dormant: 'No funded emission right now.',
};

export function YieldPanel({
  config,
  miner,
  claimable,
  rewardVaultLamports,
  chainTime,
  snapshotTime,
  weight = 0,
}: {
  config: Config;
  miner: Miner | null;
  claimable: bigint;
  rewardVaultLamports: bigint;
  /** Live, interpolated chain clock. */
  chainTime: bigint;
  /** Chain clock at the last fetch — the projection is pinned to state. */
  snapshotTime: bigint;
  /** The viewer's reward weight per rig (0 = none / base). */
  weight?: number;
}) {
  const inputs = yieldInputs(config, chainTime);
  const apr = aprBps(inputs);
  const phase = phaseFromAprBps(apr);
  const threshold = nextThreshold(inputs);
  const runway = fundedRunwaySeconds(config, chainTime);

  // Daily compounding over the next year, capped by the funded window. For a
  // wallet with no rigs yet, it is the projection for planting one now.
  const projection = useMemo(() => {
    try {
      if (miner && miner.rigs > 0n) {
        return projectCompounding(config, miner, snapshotTime, DAY, SECONDS_PER_YEAR);
      }
      const h = hypotheticalPlanting(config, snapshotTime, 1n);
      return projectCompounding(h.config, h.miner, snapshotTime, DAY, SECONDS_PER_YEAR);
    } catch {
      return null;
    }
  }, [config, miner, snapshotTime]);

  const effective = miner ? effectiveRateBps(miner, claimable, chainTime) : null;
  const referred = weight === Number(WEIGHT_REFERRED);
  const yourApr = referred ? aprBps(inputs, 0n, WEIGHT_REFERRED) : null;

  const fullYear = projection !== null && projection.horizonSeconds >= SECONDS_PER_YEAR;
  // Only a full funded year earns the name APY. A shorter window is shown as
  // the return over that window — never annualised past what is funded.
  const horizon = projection?.horizonSeconds ?? 0n;
  const apyLabel = fullYear
    ? 'Projected APY · daily replant'
    : horizon > 0n
      ? `Projected return · next ${formatDuration(horizon)}`
      : 'Projected return · no funded window';

  let nextLabel: string;
  if (threshold.kind === 'shares') {
    // Shares → base-rig equivalents, rounded up: the phase turns on the rig
    // that crosses the threshold.
    const atRigs = (threshold.atTotalShares + WEIGHT_BASE - 1n) / WEIGHT_BASE;
    const toGoShares = threshold.atTotalShares - config.totalShares;
    const toGo = (toGoShares + WEIGHT_BASE - 1n) / WEIGHT_BASE;
    nextLabel = `${PHASE_LABEL[threshold.phase]} at ${formatCount(atRigs)} seeds · ${formatCount(toGo)} to go`;
  } else if (threshold.kind === 'windowEnds') {
    nextLabel = `Window closes ${formatUtcInstant(threshold.at)}`;
  } else {
    nextLabel = 'No funded emission · none scheduled';
  }

  return (
    <section className={`yield yield--${phase}`} data-testid="yield-panel">
      <p className="yield-phase">{PHASE_LABEL[phase]} yield</p>
      <p className="yield-apr" data-testid="yield-apr">
        {formatAprBps(apr)}
        <span className="unit">APR</span>
      </p>
      <p className="yield-tagline">{TAGLINE[phase]}</p>

      <div className="yield-readouts">
        <Readout
          label={apyLabel}
          value={projection && projectionsAllowed(config, chainTime) ? formatAprBps(projection.yieldBps) : '—'}
        />
        <Readout label="Next decay" value={nextLabel} />
        <Readout
          label="Reward pool"
          value={`${formatSol(rewardVaultLamports, 3)} SOL · ${runway > 0n ? formatDuration(runway) : 'ended'}`}
        />
        <Readout label="Your accrued" value={formatSol(claimable, 6)} unit="SOL" />
        {yourApr !== null ? (
          <Readout label="Your APR · 1.05× referred weight" value={formatAprBps(yourApr)} />
        ) : null}
        <Readout
          label="Your effective rate"
          value={effective === null ? '—' : formatAprBps(effective)}
        />
      </div>

      <p className="note">
        Variable rate: funded emission ÷ seeds planted, read from the program.
        It falls as the grove grows and stops when the funded window ends. Past
        and projected figures are not a promise of future returns.
      </p>
      <p className="yield-kicker">
        Genesis yields start absurd. They don&apos;t stay absurd.
      </p>
    </section>
  );
}
