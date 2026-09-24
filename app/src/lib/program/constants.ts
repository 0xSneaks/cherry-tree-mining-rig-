/**
 * Program identity, PDA seeds and Anchor discriminators.
 *
 * The discriminator bytes are baked in rather than derived at runtime so the
 * browser bundle carries no sha256. They are pinned on the Rust side by
 * `programs/sol-rigs/tests/discriminators.rs`, which fails the build if an
 * instruction or account is ever renamed without this file following.
 */
import { address, type Address } from '@solana/kit';

export const PROGRAM_ID: Address = address(
  'CQKvfYNqD5Z2j7rPdXP1q3egWJZS43SdZNtyz6wRrJsv',
);

export const SYSTEM_PROGRAM_ID: Address = address(
  '11111111111111111111111111111111',
);

export const LAMPORTS_PER_SOL = 1_000_000_000n;

/** Matches `rigs_engine::pricing::BPS_DENOMINATOR`. */
export const BPS_DENOMINATOR = 10_000n;
/** Matches `rigs_engine::pricing::MAX_RIGS_PER_PURCHASE`. */
export const MAX_RIGS_PER_PURCHASE = 1_000_000n;
/** Matches `rigs_engine::accumulator::PRECISION`. */
export const PRECISION = 1_000_000_000_000n;

export const SEEDS = {
  config: new TextEncoder().encode('config'),
  miner: new TextEncoder().encode('miner'),
  capitalVault: new TextEncoder().encode('capital_vault'),
  rewardVault: new TextEncoder().encode('reward_vault'),
  feeVault: new TextEncoder().encode('fee_vault'),
} as const;

function fromHex(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

export const IX_DISCRIMINATOR = {
  initialize: fromHex('afaf6d1f0d989bed'),
  fundRewards: fromHex('7240a370afa71379'),
  buyRigs: fromHex('0cfa7948abf4e5a3'),
  claim: fromHex('3ec6d6c1d59f6cd2'),
  compound: fromHex('a5d0fb4ef2a08d2f'),
  // Operator-only; used by app/scripts/ops, never by the browser bundle.
  setFeeRecipient: fromHex('e312d72aedf69742'),
  withdrawFees: fromHex('c6d4ab6d90d7ae59'),
} as const;

export const ACCOUNT_DISCRIMINATOR = {
  config: fromHex('9b0caae01efacc82'),
  miner: fromHex('df710f367b7a8c64'),
  vault: fromHex('d308e82b02987577'),
} as const;

/** Byte length of each account, discriminator included. */
export const ACCOUNT_SIZE = {
  config: 321,
  miner: 171,
} as const;
