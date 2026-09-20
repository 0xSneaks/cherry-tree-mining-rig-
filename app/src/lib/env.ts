/** Environment configuration. Nothing here is hardcoded to a deployment. */

export const RPC_URL =
  process.env.NEXT_PUBLIC_SOLANA_RPC_URL ?? 'https://api.devnet.solana.com';

export const CLUSTER = process.env.NEXT_PUBLIC_SOLANA_CLUSTER ?? 'devnet';

export const EXPLORER_CLUSTER =
  process.env.NEXT_PUBLIC_EXPLORER_CLUSTER ?? CLUSTER;

/** Explorer URL for an account, used by VERIFY ON SOLANA. */
export function explorerAccountUrl(address: string): string {
  const q = EXPLORER_CLUSTER === 'mainnet-beta' ? '' : `?cluster=${EXPLORER_CLUSTER}`;
  return `https://explorer.solana.com/address/${address}${q}`;
}

/** Explorer URL for a transaction signature. */
export function explorerTxUrl(signature: string): string {
  const q = EXPLORER_CLUSTER === 'mainnet-beta' ? '' : `?cluster=${EXPLORER_CLUSTER}`;
  return `https://explorer.solana.com/tx/${signature}${q}`;
}
