/** Environment configuration. Nothing here is hardcoded to a deployment. */

export const RPC_URL =
  process.env.NEXT_PUBLIC_SOLANA_RPC_URL ?? 'https://api.devnet.solana.com';

export const CLUSTER = process.env.NEXT_PUBLIC_SOLANA_CLUSTER ?? 'devnet';

export const EXPLORER_CLUSTER =
  process.env.NEXT_PUBLIC_EXPLORER_CLUSTER ?? CLUSTER;

const rawReleaseId =
  process.env.NEXT_PUBLIC_RELEASE_ID ?? process.env.NEXT_PUBLIC_VERCEL_GIT_COMMIT_SHA;

/** Public build identifier only; never reads server-only deployment secrets. */
export const RELEASE_ID = rawReleaseId && /^[a-zA-Z0-9._-]{1,64}$/.test(rawReleaseId)
  ? rawReleaseId.slice(0, 12)
  : null;

function configuredPublicUrl(raw: string | undefined): string | null {
  if (!raw) return null;
  try {
    const url = new URL(raw);
    return url.protocol === 'https:' ? url.toString() : null;
  } catch {
    return null;
  }
}

/** Optional public repository/source URL. Invalid or non-HTTPS values fail closed. */
export const SOURCE_URL = configuredPublicUrl(process.env.NEXT_PUBLIC_SOURCE_URL);

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
