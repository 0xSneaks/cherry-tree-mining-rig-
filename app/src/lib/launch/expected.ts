/**
 * What the first-buy preflight expects to find. Every value is checked against
 * the chain at preflight time; none is trusted on its own.
 *
 * EXPECTED_FOUNDER provenance:
 *   - `sneaks.skr` → this wallet per AllDomains' own resolver
 *     (alldomains.id/api/domain-owner/sneaks.skr, read 2026-09-23), and
 *   - the same address is the Seeker fee-receiving wallet the founder
 *     supplied for `fee_recipient` earlier in this repository.
 *   Two independent sources agree, but neither is an on-chain read, so the
 *   preflight re-reads the `sneaks.skr` name record from chain and the
 *   program verifies it again inside the activation purchase.
 */
import { address } from '@solana/kit';

export const EXPECTED_FOUNDER = address('FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp');
export const EXPECTED_FEE_RECIPIENT = address('FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp');
export const FOUNDER_NAME = 'sneaks.skr';

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
