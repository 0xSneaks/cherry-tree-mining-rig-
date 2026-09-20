import 'server-only';

/**
 * Chain-derived read model.
 *
 * Everything here is indexed from program accounts. There is no parallel
 * source of truth: if this disagrees with the chain, the chain wins and this
 * is wrong (spec §9). Nothing in this module can move SOL, sign anything, or
 * hold a key that could.
 */
import {
  createSolanaRpc,
  getAddressEncoder,
  getBase58Decoder,
  getBase64Encoder,
  type Address,
} from '@solana/kit';

import { decodeConfig, decodeMiner, type Config } from '../program/accounts';
import { ACCOUNT_DISCRIMINATOR, ACCOUNT_SIZE, PROGRAM_ID } from '../program/constants';
import { programAddresses } from '../program/pdas';

const RPC_URL =
  process.env.SOLANA_RPC_URL ??
  process.env.NEXT_PUBLIC_SOLANA_RPC_URL ??
  'https://api.devnet.solana.com';

function client() {
  return createSolanaRpc(RPC_URL);
}

function b64(data: string): Uint8Array {
  return new Uint8Array(getBase64Encoder().encode(data));
}

export type MinerRow = { address: string; owner: string; rigs: bigint };

/**
 * Every miner position, straight from the program.
 *
 * `getProgramAccounts` filtered by the account discriminator and exact size —
 * a `dataSlice` keeps the payload small, but the rig count and owner both sit
 * in the first 48 bytes so one slice covers it.
 */
export async function allMiners(): Promise<MinerRow[]> {
  const discriminator = getBase58Decoder().decode(ACCOUNT_DISCRIMINATOR.miner);
  const accounts = await client()
    .getProgramAccounts(PROGRAM_ID, {
      encoding: 'base64',
      filters: [
        { dataSize: BigInt(ACCOUNT_SIZE.miner) },
        { memcmp: { offset: 0n, bytes: discriminator as never, encoding: 'base58' } },
      ],
    })
    .send();

  return accounts.map((a) => {
    const miner = decodeMiner(b64(a.account.data[0]));
    return {
      address: a.pubkey as string,
      owner: miner.owner as string,
      rigs: miner.rigs,
    };
  });
}

export async function globalStats(): Promise<{
  config: Config;
  rewardVaultLamports: bigint;
  capitalVaultLamports: bigint;
  feeVaultLamports: bigint;
}> {
  const pdas = await programAddresses();
  const rpc = client();
  const res = await rpc
    .getMultipleAccounts(
      [pdas.config, pdas.rewardVault, pdas.capitalVault, pdas.feeVault],
      { encoding: 'base64' },
    )
    .send();

  const configAccount = res.value[0];
  if (!configAccount) throw new Error('program is not initialised on this cluster');

  return {
    config: decodeConfig(b64(configAccount.data[0])),
    rewardVaultLamports: BigInt(res.value[1]?.lamports ?? 0n),
    capitalVaultLamports: BigInt(res.value[2]?.lamports ?? 0n),
    feeVaultLamports: BigInt(res.value[3]?.lamports ?? 0n),
  };
}

/** Used to confirm a claimed wallet address parses before it is stored. */
export function encodeAddress(a: Address): Uint8Array {
  return new Uint8Array(getAddressEncoder().encode(a));
}
