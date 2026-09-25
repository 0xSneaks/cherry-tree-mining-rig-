/**
 * Read path: the chain is the only source of truth.
 *
 * Everything the UI displays comes from these decoded accounts. There is no
 * cached "expected" value that survives a disagreement with the chain — if the
 * two differ, the chain wins and the local figure is discarded.
 */
import {
  createSolanaRpc,
  getBase64Encoder,
  type Address,
  type Rpc,
  type SolanaRpcApi,
} from '@solana/kit';

import { RPC_URL } from './env';
import { decodeConfig, decodeMiner, type Config, type Miner } from './program/accounts';
import { minerPda, programAddresses } from './program/pdas';
import type { ChainSnapshot } from './program/accrual';

export type RigsRpc = Rpc<SolanaRpcApi>;

let rpcSingleton: RigsRpc | null = null;

export function rpc(url: string = RPC_URL): RigsRpc {
  if (!rpcSingleton) rpcSingleton = createSolanaRpc(url);
  return rpcSingleton;
}

function decodeBase64(data: string): Uint8Array {
  return new Uint8Array(getBase64Encoder().encode(data));
}

/**
 * One round trip for everything the main screen needs.
 *
 * `getMultipleAccounts` keeps config, miner and all three economic vaults on
 * the same slot, so claimable/backing and the trust panel never combine
 * values from different points in time.
 */
export async function fetchSnapshot(
  owner: Address | null,
  client: RigsRpc = rpc(),
): Promise<ChainSnapshot | null> {
  const pdas = await programAddresses();
  const miner = owner ? await minerPda(owner) : null;

  const keys: Address[] = [
    pdas.config,
    pdas.rewardVault,
    pdas.capitalVault,
    pdas.feeVault,
  ];
  if (miner) keys.push(miner);

  const [accounts, blockTime] = await Promise.all([
    client.getMultipleAccounts(keys, { encoding: 'base64' }).send(),
    currentChainTime(client),
  ]);

  const configAccount = accounts.value[0];
  if (!configAccount) return null; // program not initialised on this cluster

  const config: Config = decodeConfig(decodeBase64(configAccount.data[0]));

  const rewardVaultAccount = accounts.value[1];
  const capitalVaultAccount = accounts.value[2];
  const feeVaultAccount = accounts.value[3];
  const representativeVault = rewardVaultAccount ?? capitalVaultAccount ?? feeVaultAccount;
  const vaultRent = representativeVault
    ? await rentExemptMinimum(BigInt(representativeVault.space ?? 0n), client)
    : 0n;
  const distributable = (account: typeof rewardVaultAccount): bigint => {
    const balance = account ? BigInt(account.lamports) : 0n;
    return balance > vaultRent ? balance - vaultRent : 0n;
  };

  const minerAccount = miner ? accounts.value[4] : null;
  const minerState: Miner | null = minerAccount
    ? decodeMiner(decodeBase64(minerAccount.data[0]))
    : null;

  return {
    config,
    miner: minerState,
    rewardVaultDistributable: distributable(rewardVaultAccount),
    rewardVaultAddress: pdas.rewardVault,
    capitalVaultDistributable: distributable(capitalVaultAccount),
    feeVaultDistributable: distributable(feeVaultAccount),
    capitalVaultAddress: pdas.capitalVault,
    feeVaultAddress: pdas.feeVault,
    chainTimeSeconds: blockTime,
    fetchedAtMs: nowMs(),
  };
}

/** Rent-exempt reserve for an account of `space` bytes. */
async function rentExemptMinimum(space: bigint, client: RigsRpc): Promise<bigint> {
  const lamports = await client.getMinimumBalanceForRentExemption(space).send();
  return BigInt(lamports);
}

/**
 * The chain's own clock, not the device's.
 *
 * A WebView's wall clock can be wrong by minutes; anchoring interpolation to
 * the device clock would show a claimable figure that never reconciles.
 */
export async function currentChainTime(client: RigsRpc = rpc()): Promise<bigint> {
  const slot = await client.getSlot({ commitment: 'confirmed' }).send();
  const blockTime = await client.getBlockTime(slot).send();
  return BigInt(blockTime);
}

/** Monotonic where available, so a suspended WebView cannot skew the ticker. */
export function nowMs(): number {
  return typeof performance !== 'undefined' ? performance.now() : Date.now();
}

/**
 * Wait for a signature to be confirmed before the UI trusts it.
 *
 * Spec §8: the Cherry bridge returns a signature string, and confirmation is
 * the app's job — optimistic UI must not mutate until the chain agrees.
 */
export async function confirmSignature(
  signature: string,
  client: RigsRpc = rpc(),
  timeoutMs = 45_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const res = await client
      .getSignatureStatuses([signature as never], { searchTransactionHistory: false })
      .send();
    const status = res.value[0];
    if (status) {
      if (status.err) {
        throw new Error(`transaction failed on chain: ${JSON.stringify(status.err)}`);
      }
      if (
        status.confirmationStatus === 'confirmed' ||
        status.confirmationStatus === 'finalized'
      ) {
        return;
      }
    }
    if (Date.now() > deadline) {
      throw new Error('timed out waiting for confirmation');
    }
    await new Promise((r) => setTimeout(r, 750));
  }
}
