'use client';

/**
 * DAILY MINER COCKPIT — the position and its two actions.
 *
 * Top to bottom: what you hold and what is ready, the claim disclosure and
 * the Replant / Claim buttons, then the detail behind each button (harvest
 * clock, replant arithmetic). Pool and runway figures live in the yield panel,
 * referral and milestone status in their own panels, the decoded account in
 * the grove record — none of it is repeated here.
 */
import { formatCount, formatDuration, formatSharePercent, formatSol } from '../lib/format';
import { penaltyCadence, replantState } from '../lib/minerCadence';
import { WEIGHT_REFERRED, minerShares } from '../lib/program/accrual';
import type { Config, Miner } from '../lib/program/accounts';
import { quoteClaim } from '../lib/program/penalty';
import { aprBps, effectiveRateBps, formatAprBps, yieldInputs } from '../lib/program/yield';
import { runwayReport } from '../lib/growth/runway';
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
  error = null,
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
  /** The last failed claim or replant, in product language. */
  error?: string | null;
}) {
  const rigs = miner?.rigs ?? 0n;
  const quote = miner ? quoteClaim(claimable, miner.lastPlantedAt, chainTime) : null;
  const cadence = miner ? penaltyCadence(miner.lastPlantedAt, chainTime) : null;
  const replant = replantState(config, miner, claimable);
  const health = runwayReport(config, rewardVaultLamports, chainTime);

  const heading = (
    <div className="cockpit-heading">
      <div>
        <p className="section-title">Daily miner cockpit</p>
        <p className="cockpit-cadence" aria-label="Miner cadence">
          Plant <span>→</span> tend <span>→</span> watch <span>→</span> rooted <span>→</span> harvest or replant <span>→</span> share
        </p>
      </div>
      <span className={`cockpit-state cockpit-state--${health.status}`} data-testid="reward-state">
        {STATUS_LABEL[health.status]}
      </span>
    </div>
  );

  // A wallet with no seeds has nothing to manage: harvest, replant and the
  // cadence are all empty, so the full instrument panel would be a wall of
  // zeros. One compact card; the panel appears with a position.
  if (!miner) {
    return (
      <section className="cockpit cockpit--empty" data-testid="miner-cockpit">
        {heading}
        <p className="note" data-testid="cockpit-empty">
          Plant your first seeds to activate your grove.
        </p>
      </section>
    );
  }

  const effective = effectiveRateBps(miner, claimable, chainTime);
  const referredApr =
    miner.weight === Number(WEIGHT_REFERRED)
      ? aprBps(yieldInputs(config, chainTime), 0n, WEIGHT_REFERRED)
      : null;

  return (
    <section className="cockpit" data-testid="miner-cockpit">
      {heading}

      <div className="cockpit-summary">
        <Readout label="My grove" value={formatCount(rigs)} unit="SEEDS" rigs />
        <OreBin lamports={claimable} />
        <Readout label="Current growth" value={`≈ ${formatSol(currentOutputPerDay, 5)}`} unit="SOL / DAY" />
        <Readout label="Production share" value={formatSharePercent(minerShares(miner), config.totalShares)} />
        <Readout label="Your effective rate" value={effective === null ? '—' : formatAprBps(effective)} />
        {referredApr !== null ? (
          <Readout label="Your APR · 1.05× referred weight" value={formatAprBps(referredApr)} />
        ) : null}
        <p className="note">
          Based on the current funded reward schedule and seeds planted. Changes when seeds planted or reward funding changes.
        </p>
      </div>

      {quote && quote.gross > 0n ? (
        <p className="note" data-testid="claim-penalty">
          Claim penalty now: {quote.penaltyBps / 100}% · you receive {formatSol(quote.net, 6)} SOL
          {quote.secondsToNextStep !== null ? ` · drops in ${formatDuration(quote.secondsToNextStep)}` : ''}.
          The penalty stays in the reward pool for other planters. Replanting has no penalty.
        </p>
      ) : null}
      <div className="actions">
        <button type="button" onClick={onReplant} disabled={!canAct || replant.wholeSeeds === 0n}>
          {busy === 'compound'
            ? 'Replanting…'
            : replant.wholeSeeds > 0n
              ? `Replant · ${formatCount(replant.wholeSeeds)}`
              : 'Replant'}
        </button>
        <button type="button" className="primary cta" onClick={onHarvest} disabled={!canAct || claimable === 0n}>
          {busy === 'claim' ? 'Claiming…' : claimLabel}
        </button>
      </div>
      {error ? <div className="banner banner--alert">{error}</div> : null}

      <div className="cockpit-grid">
        <div className="cockpit-block" data-testid="harvest-state">
          <h3>Harvest clock</h3>
          <Readout
            label="Current penalty"
            value={cadence ? (cadence.rooted ? '0% · Rooted' : `${cadence.penaltyBps / 100}%`) : '—'}
          />
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
        </div>

        <div className="cockpit-block" data-testid="replant-state">
          <h3>Replant</h3>
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
      </div>
    </section>
  );
}
