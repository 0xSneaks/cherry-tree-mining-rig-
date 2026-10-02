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

import { ACCOUNT_DISCRIMINATOR, ACCOUNT_SIZE } from './constants';

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
  /** Pinned at initialize; the only wallet whose purchase can activate launch. */
  founder: Address;
  /** T0: chain time of the founder activation purchase; `0n` until then. */
  launchT0: bigint;
  /** `Σ rigs × weight`; emission is divided by this. */
  totalShares: bigint;
  /**
   * Cumulative early-claim fees (both shares: 60% to the Protocol Treasury,
   * 40% re-credited to other planters).
   */
  totalClaimPenalties: bigint;
  /**
   * Cumulative fresh-buy cap during [T0, T0 + 24h); ignored once Public.
   * `null` on a V1 account — the canonical Devnet Config until the V2
   * migration appends this field — because the V1 program enforces no cap.
   */
  genesisCapRigs: bigint | null;
  // --- V3 protocol fees (architecture §7a) ---
  /**
   * Fee charged by `compound`, never above `purchaseFeeBps`. On a legacy
   * (V1 / V2) account this reads as `purchaseFeeBps` — what those programs
   * charged on compound — and migration writes exactly that.
   */
  compoundFeeBps: number;
  /** Scheduled fee change; meaningful only while `feeChangeEffectiveAt > 0`. */
  pendingPurchaseFeeBps: number;
  pendingCompoundFeeBps: number;
  /** When the pending change may be applied (72 h after scheduling); `0n` = none. */
  feeChangeEffectiveAt: bigint;
  /** Referral commissions reserved in the fee vault, not yet claimed. */
  reservedCommissions: bigint;
  totalCommissionsReserved: bigint;
  totalCommissionsPaid: bigint;
  /** Early-claim fee treasury shares moved REWARD → FEE (60% of each fee). */
  totalRewardToFees: bigint;
};

/** A referrer's commission ledger (`["referral", referrer]`). */
export type ReferralLedger = {
  referrer: Address;
  accruedCum: bigint;
  claimedCum: bigint;
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
  /** Reward weight per rig: 100 (base) or 105 (referred). */
  weight: number;
  /** Immutable referrer; the all-ones-zero address when none. */
  referrer: Address;
  /** Chain time of the wallet's latest purchase; anchors the claim penalty. */
  lastPlantedAt: bigint;
};

const configFields = [
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
  ['founder', getAddressDecoder()],
  ['launchT0', getI64Decoder()],
  ['totalShares', getU64Decoder()],
  ['totalClaimPenalties', getU64Decoder()],
  ['genesisCapRigs', getU64Decoder()],
  ['compoundFeeBps', getU16Decoder()],
  ['pendingPurchaseFeeBps', getU16Decoder()],
  ['pendingCompoundFeeBps', getU16Decoder()],
  ['feeChangeEffectiveAt', getI64Decoder()],
  ['reservedCommissions', getU64Decoder()],
  ['totalCommissionsReserved', getU64Decoder()],
  ['totalCommissionsPaid', getU64Decoder()],
  ['totalRewardToFees', getU64Decoder()],
] as const;

const configDecoder = getStructDecoder(configFields);

/**
 * Each version appended fields: V2 added `genesis_cap_rigs` (321 → 329), V3
 * the §7a fee fields (329 → 375). The canonical Devnet program still runs V1
 * (`DEPLOY.md`), so every legacy layout stays readable from the app.
 */
const V1_CONFIG_SIZE = 321;
const V2_CONFIG_SIZE = 329;
const V2_FIELD_COUNT = configFields.findIndex(([name]) => name === 'genesisCapRigs') + 1;
const configV1Decoder = getStructDecoder(configFields.slice(0, V2_FIELD_COUNT - 1));
const configV2Decoder = getStructDecoder(configFields.slice(0, V2_FIELD_COUNT));

type LegacyFeeFields = Pick<
  Config,
  | 'compoundFeeBps'
  | 'pendingPurchaseFeeBps'
  | 'pendingCompoundFeeBps'
  | 'feeChangeEffectiveAt'
  | 'reservedCommissions'
  | 'totalCommissionsReserved'
  | 'totalCommissionsPaid'
  | 'totalRewardToFees'
>;

/** A legacy program compounds at the purchase fee and has no fee timelock. */
function legacyFees(purchaseFeeBps: number): LegacyFeeFields {
  return {
    compoundFeeBps: purchaseFeeBps,
    pendingPurchaseFeeBps: 0,
    pendingCompoundFeeBps: 0,
    feeChangeEffectiveAt: 0n,
    reservedCommissions: 0n,
    totalCommissionsReserved: 0n,
    totalCommissionsPaid: 0n,
    totalRewardToFees: 0n,
  };
}

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
  ['weight', getU16Decoder()],
  ['referrer', getAddressDecoder()],
  ['lastPlantedAt', getI64Decoder()],
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
  const payload = stripDiscriminator(data, ACCOUNT_DISCRIMINATOR.config, 'Config');
  if (payload.length === V1_CONFIG_SIZE - 8) {
    const v1 = configV1Decoder.decode(payload) as Omit<
      Config,
      'genesisCapRigs' | keyof LegacyFeeFields
    >;
    return { ...v1, genesisCapRigs: null, ...legacyFees(v1.purchaseFeeBps) };
  }
  if (payload.length === V2_CONFIG_SIZE - 8) {
    const v2 = configV2Decoder.decode(payload) as Omit<Config, keyof LegacyFeeFields>;
    return { ...v2, ...legacyFees(v2.purchaseFeeBps) };
  }
  if (payload.length !== ACCOUNT_SIZE.config - 8) {
    throw new AccountDecodeError(
      `Config: unexpected data length ${data.length} (V1 ${V1_CONFIG_SIZE}, V2 ${V2_CONFIG_SIZE} or V3 ${ACCOUNT_SIZE.config})`,
    );
  }
  return configDecoder.decode(payload) as Config;
}

const referralLedgerDecoder = getStructDecoder([
  ['referrer', getAddressDecoder()],
  ['accruedCum', getU64Decoder()],
  ['claimedCum', getU64Decoder()],
  ['bump', getU8Decoder()],
]);

export function decodeReferralLedger(data: Uint8Array): ReferralLedger {
  return referralLedgerDecoder.decode(
    stripDiscriminator(data, ACCOUNT_DISCRIMINATOR.referralLedger, 'ReferralLedger'),
  ) as ReferralLedger;
}

/** Unpaid commission: `accruedCum - claimedCum`. */
export function unpaidCommission(l: ReferralLedger): bigint {
  return l.accruedCum > l.claimedCum ? l.accruedCum - l.claimedCum : 0n;
}

export function decodeMiner(data: Uint8Array): Miner {
  return minerDecoder.decode(
    stripDiscriminator(data, ACCOUNT_DISCRIMINATOR.miner, 'Miner'),
  ) as Miner;
}
