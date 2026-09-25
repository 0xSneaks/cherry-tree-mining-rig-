'use client';

/**
 * Launch state, from `config.launch_t0` and the chain clock only.
 *
 * PRE-LAUNCH → .SKR GENESIS (24h) → PUBLIC FOREVER.
 * Nothing on this panel can move a phase: the program enforces the same
 * function of the same two numbers inside `buy_rigs`.
 */
import type { Address } from '@solana/kit';

import type { Config } from '../lib/program/accounts';
import type { Identity } from '../lib/program/launch';
import {
  accessPhase,
  formatCountdown,
  genesisAllowance,
  purchaseAllowed,
  secondsToNextPhase,
} from '../lib/program/launch';

const IDENTITY_LABEL: Record<Identity, string> = {
  skr: '.skr domain verified',
  sns: '.sns domain verified',
  none: 'No .skr or .sns domain found',
};

export function LaunchPanel({
  config,
  chainTime,
  wallet,
  identity,
  lifetimeRigsBought,
}: {
  config: Config;
  chainTime: bigint;
  wallet: Address | null;
  identity: Identity | null;
  lifetimeRigsBought: bigint;
}) {
  const phase = accessPhase(config.launchT0, chainTime);
  const next = secondsToNextPhase(config.launchT0, chainTime);
  const isFounder = wallet !== null && wallet === config.founder;

  let title: string;
  let line: string;
  let countdown: string | null = null;
  switch (phase) {
    case 'preLaunch':
      title = 'The grove is dormant';
      line = 'Founder activation pending';
      break;
    case 'skrGenesis':
      title = 'Genesis is live';
      line = '.SKR genesis access';
      countdown = next !== null ? `Public access in ${formatCountdown(next)}` : null;
      break;
    case 'skrSnsEarly':
      // Stable phase-code compatibility only; V2's phase function never
      // returns this value.
      title = 'Public';
      line = 'The grove is open';
      break;
    case 'public':
      title = 'Public';
      line = 'The grove is open';
      break;
  }

  const allowance = genesisAllowance(
    config.launchT0,
    chainTime,
    lifetimeRigsBought,
    config.genesisCapRigs,
  );

  let eligibility: string | null = null;
  if (wallet && phase !== 'public') {
    if (phase === 'preLaunch') {
      eligibility = isFounder
        ? 'Founder wallet connected. One deliberate first planting activates the grove.'
        : 'Planting opens when the founder activates the grove.';
    } else if (identity) {
      eligibility = `${IDENTITY_LABEL[identity]} · ${
        purchaseAllowed(phase, identity) ? 'you may plant now' : 'not yet open to you'
      }`;
    }
  }

  return (
    <section className={`launch launch--${phase}`} data-testid="launch-panel" data-phase={phase}>
      <p className="launch-title">{title}</p>
      <p className="launch-line">{line}</p>
      {countdown ? (
        <p className="launch-countdown" data-testid="launch-countdown">
          {countdown}
        </p>
      ) : null}
      {phase === 'skrGenesis' && allowance !== null ? (
        <p className="note" data-testid="genesis-allowance">
          Genesis fresh buys: {lifetimeRigsBought.toString()} /{' '}
          {config.genesisCapRigs.toString()} · {allowance.toString()} remaining
        </p>
      ) : null}
      {eligibility ? <p className="note">{eligibility}</p> : null}
    </section>
  );
}
