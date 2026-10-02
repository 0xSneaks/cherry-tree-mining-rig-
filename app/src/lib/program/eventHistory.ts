/**
 * Event history over RPC — read-only. Walks `getSignaturesForAddress` for an
 * account our program writes (the reward vault for funding history) and
 * decodes our events from each transaction's logs. Failed transactions are
 * skipped: an event in a failed transaction never happened.
 */
import type { Address, Signature } from '@solana/kit';

import type { RigsRpc } from '../chain';
import { PROGRAM_ID } from './constants';
import { eventsFromLogs, type ProgramEvent } from './events';

export type EventRow = { signature: string; blockTime: bigint | null; event: ProgramEvent };

export async function eventHistory(
  rpc: RigsRpc,
  account: Address,
  opts: { limit?: number; names?: readonly ProgramEvent['name'][] } = {},
): Promise<EventRow[]> {
  const limit = Math.min(Math.max(opts.limit ?? 200, 1), 1_000);
  const sigs = await rpc.getSignaturesForAddress(account, { limit }).send();
  const successful = sigs.filter((signature) => signature.err === null);
  const transactions = await Promise.all(successful.map(async (s) => {
    const tx = await rpc
      .getTransaction(s.signature as Signature, { maxSupportedTransactionVersion: 0, encoding: 'json' })
      .send();
    return { s, tx };
  }));
  const out: EventRow[] = [];
  for (const { s, tx } of transactions) {
    const logs = tx?.meta?.logMessages ?? [];
    for (const event of eventsFromLogs(logs, PROGRAM_ID)) {
      if (opts.names && !opts.names.includes(event.name)) continue;
      out.push({
        signature: s.signature,
        blockTime: s.blockTime === null || s.blockTime === undefined ? null : BigInt(s.blockTime),
        event,
      });
    }
  }
  return out;
}
