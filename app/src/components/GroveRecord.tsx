'use client';

/**
 * GROVE RECORD — the user's position in full, every field decoded straight
 * from the miner account: referral binding, reward weight, the planting clock
 * that sets the claim penalty, and the lifetime ledger. Nothing here is
 * projected or cached; it is what the program says, rendered.
 */
import { address, type Address } from '@solana/kit';

import { formatCount, formatDuration, formatSol } from '../lib/format';
import { claimPenaltyBps } from '../lib/program/penalty';
import type { Miner } from '../lib/program/accounts';
import { Readout } from './Readout';

/** The all-ones address the program writes for "no referrer". */
const NO_REFERRER: Address = address('11111111111111111111111111111111');

function short(address_: Address): string {
  return `${address_.slice(0, 4)}…${address_.slice(-4)}`;
}

export function GroveRecord({ miner, chainTime }: { miner: Miner; chainTime: bigint }) {
  const referred = miner.referrer !== NO_REFERRER;
  const age = chainTime > miner.lastPlantedAt ? chainTime - miner.lastPlantedAt : 0n;
  const band = claimPenaltyBps(miner.lastPlantedAt, chainTime);

  return (
    <section className="panel" data-testid="grove-record">
      <h2 className="section-title">Grove record</h2>
      <Readout label="Reward weight" value={`${(miner.weight / 100).toFixed(2)}×`} />
      <Readout
        label="Referral"
        value={referred ? `Bound to ${short(miner.referrer)}` : 'None'}
      />
      <Readout
        label="Last planted"
        value={
          miner.rigs === 0n
            ? '—'
            : `${formatDuration(age)} ago · ${band / 100}% claim band`
        }
      />
      <Readout
        label="Seeds bought · replanted"
        value={`${formatCount(miner.lifetimeRigsBought)} · ${formatCount(miner.lifetimeRigsCompounded)}`}
      />
      <Readout
        label="SOL planted (lifetime)"
        value={formatSol(miner.lifetimeSolSpent, 4)}
        unit="SOL"
      />
      <Readout
        label="SOL replanted · harvested"
        value={`${formatSol(miner.lifetimeSolCompounded, 4)} · ${formatSol(miner.lifetimeSolClaimed, 4)}`}
        unit="SOL"
      />
    </section>
  );
}
