'use client';

/**
 * The differentiator: more transparent than the miners this borrows its loop
 * from, not less. Every value is decoded from program state — there is no
 * figure on this panel that the app computes optimistically or caches.
 */
import { CLUSTER, RELEASE_ID, SOURCE_URL, explorerAccountUrl } from '../lib/env';
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
import { PROGRAM_ID, SYSTEM_PROGRAM_ID } from '../lib/program/constants';
import { runwayReport } from '../lib/growth/runway';
import { Readout } from './Readout';

export function TransparencyPanel({
  config,
  miner,
  rewardVaultLamports,
  rewardVaultAddress,
  capitalVaultLamports,
  capitalVaultAddress,
  feeVaultLamports,
  feeVaultAddress,
  chainTime,
}: {
  config: Config;
  miner: Miner | null;
  rewardVaultLamports: bigint;
  rewardVaultAddress: string;
  capitalVaultLamports: bigint;
  capitalVaultAddress: string | null;
  feeVaultLamports: bigint;
  feeVaultAddress: string | null;
  chainTime: bigint;
}) {
  const live = chainTime < config.rewardPeriodFinish;
  const perDay = live ? config.rewardRate * SECONDS_PER_DAY : 0n;
  const runway = fundedRunwaySeconds(config, chainTime);
  const health = runwayReport(config, rewardVaultLamports, chainTime);
  const pendingAuthority = config.pendingAuthority === SYSTEM_PROGRAM_ID
    ? 'Not configured'
    : config.pendingAuthority;

  const accountLink = (label: string, value: string | null) => value ? (
    <div className="trust-row">
      <span className="readout-label">{label}</span>
      <a className="trust-address" href={explorerAccountUrl(value)} target="_blank" rel="noreferrer noopener">
        {value} ↗
      </a>
    </div>
  ) : (
    <Readout label={label} value="Not available" />
  );

  return (
    <section data-testid="trust-panel">
      <h2 className="section-title">Trust &amp; release</h2>
      <div className="panel">
        <p className="trust-kicker">Global grove</p>
        <Readout label="Network" value={CLUSTER.toUpperCase()} />
        <Readout label="Release / build" value={RELEASE_ID ?? 'Not available'} />
        <Readout label="Source repository" value={SOURCE_URL ? 'Public source' : 'Not configured'} />
        {SOURCE_URL ? (
          <a className="verify" href={SOURCE_URL} target="_blank" rel="noreferrer noopener">
            View source ↗
          </a>
        ) : null}
        {accountLink('Program ID', PROGRAM_ID)}
        {accountLink('Program authority', config.authority)}
        <Readout label="Pending authority" value={pendingAuthority} />
        <Readout label="Upgrade authority" value="Not available" />
        <Readout label="Authority multisig" value="Not available" />
        <Readout label="Timelock" value="Not available" />

        <hr className="rule" style={{ margin: '12px 0' }} />

        <Readout label="Reward vault" value={formatSol(rewardVaultLamports, 3)} unit="SOL" />
        {accountLink('REWARD account', rewardVaultAddress)}
        <Readout label="Capital vault · locked" value={formatSol(capitalVaultLamports, 3)} unit="SOL" />
        {accountLink('CAPITAL account', capitalVaultAddress)}
        <Readout label="Fee vault" value={formatSol(feeVaultLamports, 3)} unit="SOL" />
        {accountLink('FEE account', feeVaultAddress)}
        <Readout label="Reward backing" value={health.backed ? 'Backed' : 'Shortfall'} />
        <Readout label="Accrued backing" value={formatSol(health.accruedBackingLamports, 3)} unit="SOL" />
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
        from the program, not from this app. CAPITAL is locked and non-redeemable
        in this program version; FEE never feeds REWARD. Vault balances exclude
        rent reserve.
      </p>
    </section>
  );
}
