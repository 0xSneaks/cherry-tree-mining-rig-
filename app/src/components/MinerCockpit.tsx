'use client';

import { formatCount, formatDuration, formatSharePercent, formatSol, formatUtcInstant } from '../lib/format';
import { penaltyCadence, replantState } from '../lib/minerCadence';
import { minerShares } from '../lib/program/accrual';
import type { Config, Miner } from '../lib/program/accounts';
import { quoteClaim } from '../lib/program/penalty';
import { runwayReport } from '../lib/growth/runway';
import { groveTier, nextGroveTier } from '../lib/growth/milestones';
import type { ReferralSummary } from './ReferralPanel';
import { OreBin } from './OreBin';
import { Readout } from './Readout';

const STATUS_LABEL = {
  unfunded: 'Unfunded',
  live: 'Live',
  lowRunway: 'Low runway',
  dormant: 'Dormant',
} as const;

export function MinerCockpit({
  config,
  miner,
  claimable,
  rewardVaultLamports,
  chainTime,
  currentOutputPerDay,
  canAct,
  busy,
  onReplant,
  onHarvest,
  claimLabel,
  hasReferralLink,
  referralSummary,
}: {
  config: Config;
  miner: Miner | null;
  claimable: bigint;
  rewardVaultLamports: bigint;
  chainTime: bigint;
  currentOutputPerDay: bigint;
  canAct: boolean;
  busy: null | 'buy' | 'claim' | 'compound';
  onReplant: () => void;
  onHarvest: () => void;
  claimLabel: string;
  hasReferralLink: boolean;
  referralSummary: ReferralSummary | null;
}) {
  const rigs = miner?.rigs ?? 0n;
  const quote = miner ? quoteClaim(claimable, miner.lastPlantedAt, chainTime) : null;
  const cadence = miner ? penaltyCadence(miner.lastPlantedAt, chainTime) : null;
  const replant = replantState(config, miner, claimable);
  const health = runwayReport(config, rewardVaultLamports, chainTime);
  const milestone = groveTier(rigs);
  const nextMilestone = nextGroveTier(rigs);

  return (
    <section className="cockpit" data-testid="miner-cockpit">
      <div className="cockpit-heading">
        <div>
          <p className="section-title">Daily miner cockpit</p>
          <p className="cockpit-cadence" aria-label="Miner cadence">
            Plant <span>→</span> tend <span>→</span> watch <span>→</span> rooted <span>→</span> harvest or replant <span>→</span> share
          </p>
        </div>
        <span className={`cockpit-state cockpit-state--${health.status}`}>
          {STATUS_LABEL[health.status]}
        </span>
      </div>

      <div className="cockpit-grid">
        <div className="cockpit-block">
          <h3>Position</h3>
          <Readout label="My grove" value={formatCount(rigs)} unit="SEEDS" rigs />
          <Readout
            label="Production share"
            value={formatSharePercent(miner ? minerShares(miner) : 0n, config.totalShares)}
          />
          <Readout label="Reward weight" value={`${miner?.weight || 100}`} />
          <Readout label="Current growth" value={`≈ ${formatSol(currentOutputPerDay, 5)}`} unit="SOL / DAY" />
          <Readout label="Accrued REWARD" value={formatSol(claimable, 6)} unit="SOL" />
          <OreBin lamports={claimable} />
        </div>

        <div className="cockpit-block" data-testid="harvest-state">
          <h3>Harvest state</h3>
          <Readout
            label="Current penalty"
            value={cadence ? (cadence.rooted ? '0% · Rooted' : `${cadence.penaltyBps / 100}%`) : '—'}
          />
          <Readout label="Net claim now" value={quote ? formatSol(quote.net, 6) : '0.000000'} unit="SOL" />
          <Readout
            label="Next reduction"
            value={
              cadence?.secondsToNextStep !== null && cadence?.secondsToNextStep !== undefined
                ? `${cadence.nextPenaltyBps! / 100}% in ${formatDuration(cadence.secondsToNextStep)}`
                : 'Rooted'
            }
          />
          <Readout
            label="Penalty-free"
            value={cadence?.rooted ? 'Rooted now' : cadence ? `in ${formatDuration(cadence.secondsToRooted)}` : '—'}
          />
          {quote && quote.gross > 0n ? (
            <p className="note" data-testid="claim-penalty">
              Claim penalty now: {quote.penaltyBps / 100}% · you receive {formatSol(quote.net, 6)} SOL
              {quote.secondsToNextStep !== null ? ` · drops in ${formatDuration(quote.secondsToNextStep)}` : ''}.
              The penalty stays in the reward pool for other planters. Replanting has no penalty.
            </p>
          ) : null}
        </div>

        <div className="cockpit-block" data-testid="replant-state">
          <h3>Replant</h3>
          <Readout label="Available now" value={formatSol(replant.availableLamports, 6)} unit="SOL" />
          <Readout label="Whole seeds now" value={formatCount(replant.wholeSeeds)} />
          <Readout label="Resulting grove" value={formatCount(replant.resultingSeeds)} unit="SEEDS" />
          <Readout
            label="Resulting share"
            value={formatSharePercent(replant.resultingShares, replant.totalSharesAfter)}
          />
          <p className="note" data-testid="compound-preview">
            {replant.wholeSeeds > 0n
              ? `Replant plants ${formatCount(replant.wholeSeeds)} more seed${replant.wholeSeeds === 1n ? '' : 's'} for ${formatSol(replant.costLamports, 6)} SOL of your accrued harvest — ${formatSol(replant.remainderLamports, 6)} SOL stays ready to harvest.`
              : `No whole seed is available yet. ${formatSol(replant.remainderLamports, 6)} SOL remains ready to harvest.`}
          </p>
        </div>

        <div className="cockpit-block" data-testid="reward-health">
          <h3>Reward health</h3>
          <Readout label="State" value={STATUS_LABEL[health.status]} />
          <Readout label="Funded until" value={health.periodFinish > 0n ? formatUtcInstant(health.periodFinish) : 'Not funded'} />
          <Readout label="Runway" value={health.runwaySeconds > 0n ? formatDuration(health.runwaySeconds) : '0'} />
          <Readout label="Future emission" value={formatSol(health.futureEmissionLamports, 4)} unit="SOL" />
          <Readout
            label="Accrued backing"
            value={`${formatSol(health.accruedBackingLamports, 4)} SOL · ${health.backed ? 'backed' : 'shortfall'}`}
          />
        </div>

        <div className="cockpit-block" data-testid="referral-milestone-state">
          <h3>Referral &amp; milestone</h3>
          <Readout label="Referral link" value={hasReferralLink ? 'Available' : 'After first planting'} />
          <Readout label="Referrals" value={referralSummary ? referralSummary.total : 'Pending'} />
          <Readout
            label="Qualified"
            value={
              referralSummary
                ? referralSummary.configured
                  ? `${referralSummary.qualified} · ${referralSummary.pending} pending`
                  : 'Not configured'
                : 'Pending'
            }
          />
          <Readout label="Referral status" value={referralSummary?.tier ?? 'Not configured'} />
          <Readout label="Grove milestone" value={milestone?.label ?? 'Not available'} />
          <Readout
            label="Next milestone"
            value={nextMilestone ? `${nextMilestone.tier.label} · ${formatCount(nextMilestone.seedsToGo)} seeds` : 'Highest reached'}
          />
        </div>
      </div>

      <p className="note">
        Based on the current funded reward schedule and seeds planted. Changes when seeds planted or reward funding changes.
      </p>
      <div className="actions">
        <button type="button" onClick={onReplant} disabled={!canAct || replant.wholeSeeds === 0n}>
          {busy === 'compound'
            ? 'Replanting…'
            : replant.wholeSeeds > 0n
              ? `Replant · ${formatCount(replant.wholeSeeds)}`
              : 'Replant'}
        </button>
        <button type="button" className="primary" onClick={onHarvest} disabled={!canAct || claimable === 0n}>
          {busy === 'claim' ? 'Claiming…' : claimLabel}
        </button>
      </div>
    </section>
  );
}
