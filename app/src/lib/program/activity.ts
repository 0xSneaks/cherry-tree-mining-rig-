import type { Address } from '@solana/kit';

import { explorerTxUrl } from '../env';
import type { EventRow } from './eventHistory';
import type { ProgramEvent } from './events';

export type MinerActivity = {
  action: 'Fresh plant' | 'Replant' | 'Harvest' | 'Referral bound' | 'Founder activation';
  signature: string;
  blockTime: bigint | null;
  seeds: bigint | null;
  amountLamports: bigint | null;
  detail: string | null;
  explorerUrl: string | null;
};

export function safeExplorerTxUrl(signature: string): string | null {
  return /^[1-9A-HJ-NP-Za-km-z]{64,88}$/.test(signature)
    ? explorerTxUrl(signature)
    : null;
}

function belongsTo(event: ProgramEvent, owner: Address): boolean {
  switch (event.name) {
    case 'RigsPurchased':
    case 'Claim':
    case 'Compound':
      return event.owner === owner;
    case 'ReferralBound':
      return event.miner === owner;
    case 'LaunchActivated':
      return event.founder === owner;
    default:
      return false;
  }
}

export function minerActivity(rows: readonly EventRow[], owner: Address): MinerActivity[] {
  const out: MinerActivity[] = [];
  for (const row of rows) {
    const event = row.event;
    if (!belongsTo(event, owner)) continue;
    const base = {
      signature: row.signature,
      blockTime: row.blockTime,
      explorerUrl: safeExplorerTxUrl(row.signature),
    };
    switch (event.name) {
      case 'RigsPurchased':
        out.push({ ...base, action: 'Fresh plant', seeds: event.count, amountLamports: event.totalLamports, detail: null });
        break;
      case 'Compound':
        out.push({ ...base, action: 'Replant', seeds: event.rigsMinted, amountLamports: event.totalLamports, detail: null });
        break;
      case 'Claim':
        out.push({ ...base, action: 'Harvest', seeds: null, amountLamports: event.amount, detail: event.penaltyBps > 0 ? `${event.penaltyBps / 100}% early-claim fee` : 'Rooted' });
        break;
      case 'ReferralBound':
        out.push({ ...base, action: 'Referral bound', seeds: null, amountLamports: null, detail: `${event.weight} weight` });
        break;
      case 'LaunchActivated':
        out.push({ ...base, action: 'Founder activation', seeds: event.rigs, amountLamports: event.totalLamports, detail: null });
        break;
    }
  }
  return out;
}
