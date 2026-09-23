'use client';

/**
 * The differentiator: more transparent than the miners this borrows its loop
 * from, not less. Every value is decoded from program state — there is no
 * figure on this panel that the app computes optimistically or caches.
 */
import { explorerAccountUrl } from '../lib/env';
import {
  formatBps,
  formatCount,
  formatDuration,
  formatSharePercent,
  formatSol,
  formatUtcInstant,
} from '../lib/format';
import { SECONDS_PER_DAY, fundedRunwaySeconds, minerShares } from '../lib/program/accrual';
import type { Config, Miner } from '../lib/program/accounts';
import { Readout } from './Readout';

export function TransparencyPanel({
  config,
  miner,
  rewardVaultLamports,
  rewardVaultAddress,
  chainTime,
}: {
  config: Config;
  miner: Miner | null;
  rewardVaultLamports: bigint;
  rewardVaultAddress: string;
  chainTime: bigint;
}) {
  const live = chainTime < config.rewardPeriodFinish;
  const perDay = live ? config.rewardRate * SECONDS_PER_DAY : 0n;
  const runway = fundedRunwaySeconds(config, chainTime);

  return (
    <section>
      <h2 className="section-title">Global grove</h2>
      <div className="panel">
        <Readout label="Reward vault" value={formatSol(rewardVaultLamports, 3)} unit="SOL" />
        <Readout label="Current distribution" value={formatSol(perDay, 3)} unit="SOL / day" />
        <Readout
          label="Funded until"
          value={live ? formatUtcInstant(config.rewardPeriodFinish) : 'schedule ended'}
        />
        <Readout label="Funded runway" value={formatDuration(runway)} />
        <Readout label="Seeds planted" value={formatCount(config.totalRigs)} />
        <Readout
          label="Your share"
          value={formatSharePercent(miner ? minerShares(miner) : 0n, config.totalShares)}
        />
        <Readout label="Protocol fee" value={formatBps(config.purchaseFeeBps)} />
        <Readout label="Planters" value={formatCount(config.minerCount)} />

        <hr className="rule" style={{ margin: '12px 0' }} />

        <Readout label="Total funded" value={formatSol(config.totalRewardFunded, 3)} unit="SOL" />
        <Readout label="Total claimed" value={formatSol(config.totalRewardClaimed, 3)} unit="SOL" />
        <Readout
          label="Total replanted"
          value={formatSol(config.totalRewardCompounded, 3)}
          unit="SOL"
        />

        <a
          className="verify"
          href={explorerAccountUrl(rewardVaultAddress)}
          target="_blank"
          rel="noreferrer noopener"
        >
          Verify on Solana ↗
        </a>
      </div>

      <p className="note">
        Rewards are paid from a funded reward vault. Every lamport shown as
        claimable is already sitting in that vault — the figures above are read
        from the program, not from this app.
      </p>
    </section>
  );
}
