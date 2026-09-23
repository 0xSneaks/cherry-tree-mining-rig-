/**
 * What the first-buy preflight expects to find. Every value is checked against
 * the chain at preflight time.
 *
 * EXPECTED_FOUNDER is the founder activation authority: the first successful
 * purchase this exact wallet signs is T0. It is pinned on-chain in
 * `config.founder` at initialize; the program enforces it.
 */
import { address } from '@solana/kit';

export const EXPECTED_FOUNDER = address('FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp');
export const EXPECTED_FEE_RECIPIENT = address('FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp');

/** Genesis hashes identify the cluster independently of the RPC URL. */
export const GENESIS_HASH = {
  'mainnet-beta': '5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d',
  devnet: 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG',
  testnet: '4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY',
} as const;

export type Cluster = keyof typeof GENESIS_HASH;

export function clusterFromGenesis(hash: string): Cluster | 'unknown' {
  for (const [name, h] of Object.entries(GENESIS_HASH)) {
    if (h === hash) return name as Cluster;
  }
  return 'unknown';
}
