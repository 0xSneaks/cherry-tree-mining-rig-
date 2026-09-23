/**
 * Borsh decoders for the program's accounts.
 *
 * Layouts mirror `programs/sol-rigs/src/state.rs` field for field. The
 * fixtures in `app/tests/accounts.test.ts` are serialised by the Rust
 * implementation, so a layout drift fails the test rather than silently
 * shifting every number on screen.
 */
import {
  getAddressDecoder,
  getBooleanDecoder,
  getI64Decoder,
  getStructDecoder,
  getU8Decoder,
  getU16Decoder,
  getU64Decoder,
  getU128Decoder,
  type Address,
} from '@solana/kit';

import { ACCOUNT_DISCRIMINATOR } from './constants';

export type Config = {
  authority: Address;
  pendingAuthority: Address;
  rewardFunder: Address;
  feeRecipient: Address;
  paused: boolean;
  rigPriceLamports: bigint;
  purchaseFeeBps: number;
  totalRigs: bigint;
  rewardRate: bigint;
  rewardPeriodFinish: bigint;
  rewardLastUpdate: bigint;
  rewardPerRigStored: bigint;
  undistributedDust: bigint;
  totalRewardFunded: bigint;
  totalRewardClaimed: bigint;
  totalRewardCompounded: bigint;
  totalCapitalReceived: bigint;
  totalProtocolFees: bigint;
  totalFeesWithdrawn: bigint;
  minerCount: bigint;
  capitalVaultBump: number;
  rewardVaultBump: number;
  feeVaultBump: number;
  version: number;
  bump: number;
};

export type Miner = {
  owner: Address;
  rigs: bigint;
  rewardPerRigPaid: bigint;
  pendingRewards: bigint;
  lifetimeRigsBought: bigint;
  lifetimeRigsCompounded: bigint;
  lifetimeSolSpent: bigint;
  lifetimeSolCompounded: bigint;
  lifetimeSolClaimed: bigint;
  createdAt: bigint;
  lastActionAt: bigint;
  bump: number;
};

const configDecoder = getStructDecoder([
  ['authority', getAddressDecoder()],
  ['pendingAuthority', getAddressDecoder()],
  ['rewardFunder', getAddressDecoder()],
  ['feeRecipient', getAddressDecoder()],
  ['paused', getBooleanDecoder()],
  ['rigPriceLamports', getU64Decoder()],
  ['purchaseFeeBps', getU16Decoder()],
  ['totalRigs', getU64Decoder()],
  ['rewardRate', getU64Decoder()],
  ['rewardPeriodFinish', getI64Decoder()],
  ['rewardLastUpdate', getI64Decoder()],
  ['rewardPerRigStored', getU128Decoder()],
  ['undistributedDust', getU64Decoder()],
  ['totalRewardFunded', getU64Decoder()],
  ['totalRewardClaimed', getU64Decoder()],
  ['totalRewardCompounded', getU64Decoder()],
  ['totalCapitalReceived', getU64Decoder()],
  ['totalProtocolFees', getU64Decoder()],
  ['totalFeesWithdrawn', getU64Decoder()],
  ['minerCount', getU64Decoder()],
  ['capitalVaultBump', getU8Decoder()],
  ['rewardVaultBump', getU8Decoder()],
  ['feeVaultBump', getU8Decoder()],
  ['version', getU16Decoder()],
  ['bump', getU8Decoder()],
]);

const minerDecoder = getStructDecoder([
  ['owner', getAddressDecoder()],
  ['rigs', getU64Decoder()],
  ['rewardPerRigPaid', getU128Decoder()],
  ['pendingRewards', getU64Decoder()],
  ['lifetimeRigsBought', getU64Decoder()],
  ['lifetimeRigsCompounded', getU64Decoder()],
  ['lifetimeSolSpent', getU64Decoder()],
  ['lifetimeSolCompounded', getU64Decoder()],
  ['lifetimeSolClaimed', getU64Decoder()],
  ['createdAt', getI64Decoder()],
  ['lastActionAt', getI64Decoder()],
  ['bump', getU8Decoder()],
]);

export class AccountDecodeError extends Error {}

function stripDiscriminator(data: Uint8Array, expected: Uint8Array, what: string): Uint8Array {
  if (data.length < 8) {
    throw new AccountDecodeError(`${what}: account data too short`);
  }
  for (let i = 0; i < 8; i++) {
    if (data[i] !== expected[i]) {
      throw new AccountDecodeError(`${what}: wrong account discriminator`);
    }
  }
  return data.subarray(8);
}

export function decodeConfig(data: Uint8Array): Config {
  return configDecoder.decode(
    stripDiscriminator(data, ACCOUNT_DISCRIMINATOR.config, 'Config'),
  ) as Config;
}

export function decodeMiner(data: Uint8Array): Miner {
  return minerDecoder.decode(
    stripDiscriminator(data, ACCOUNT_DISCRIMINATOR.miner, 'Miner'),
  ) as Miner;
}
