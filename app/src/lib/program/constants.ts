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
  'C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt',
);

export const SYSTEM_PROGRAM_ID: Address = address(
  '11111111111111111111111111111111',
);

export const LAMPORTS_PER_SOL = 1_000_000_000n;

/** Matches `rigs_engine::pricing::BPS_DENOMINATOR`. */
export const BPS_DENOMINATOR = 10_000n;
/** Matches `rigs_engine::pricing::MAX_RIGS_PER_PURCHASE`. */
export const MAX_RIGS_PER_PURCHASE = 1_000_000n;
/** Matches `rigs_engine::pricing::MAX_WALLET_RIGS`: permanent per-wallet seed cap. */
export const MAX_WALLET_RIGS = 250n;
/** Matches `rigs_engine::launch::GENESIS_CAP_RIGS`: the only cap `initialize` accepts. */
export const GENESIS_CAP_RIGS = MAX_WALLET_RIGS;
/** Matches `rigs_engine::pricing::MAX_TOTAL_RIGS`. */
export const MAX_TOTAL_RIGS = 1_000_000_000_000n;
/** Matches `rigs_engine::accumulator::PRECISION`. */
export const PRECISION = 1_000_000_000_000n;

export const SEEDS = {
  config: new TextEncoder().encode('config'),
  miner: new TextEncoder().encode('miner'),
  capitalVault: new TextEncoder().encode('capital_vault'),
  rewardVault: new TextEncoder().encode('reward_vault'),
  feeVault: new TextEncoder().encode('fee_vault'),
  /** Day-1 `.skr` binding: `["skr_bind", name_account]`. */
  skrBind: new TextEncoder().encode('skr_bind'),
  /** Referral ledger (architecture §7): `["referral", referrer]`. */
  referral: new TextEncoder().encode('referral'),
  // Fruit ledger (§10a) and lock Season (§8, Pass #7).
  fruitConfig: new TextEncoder().encode('fruit_config'),
  fruitLedger: new TextEncoder().encode('fruit_ledger'),
  fruitVault: new TextEncoder().encode('fruit_vault'),
  fruitPool: new TextEncoder().encode('fruit_pool'),
  fruitRoot: new TextEncoder().encode('fruit_root'),
  fruitClaim: new TextEncoder().encode('fruit_claim'),
  lockSeason: new TextEncoder().encode('lock_season'),
  lockReg: new TextEncoder().encode('lock_reg'),
} as const;

// --- protocol fees (architecture §7a) — program constants, mirrored ---

/** Matches `rigs_engine::pricing::MAX_PURCHASE_FEE_BPS`. */
export const MAX_PURCHASE_FEE_BPS = 1_000;
/** Matches `rigs_engine::fees::FEE_CHANGE_TIMELOCK_SECONDS` (72 h). */
export const FEE_CHANGE_TIMELOCK_SECONDS = 259_200n;
/** Matches `rigs_engine::fees::REFERRAL_COMMISSION_BPS` (c = 20%). */
export const REFERRAL_COMMISSION_BPS = 2_000n;
/** Matches `rigs_engine::fees::REFERRAL_RETAINED_MARGIN_BPS` (m = 8%). */
export const REFERRAL_RETAINED_MARGIN_BPS = 800n;
/** Matches `rigs_engine::fees::EARLY_CLAIM_TREASURY_BPS` (60% of the fee). */
export const EARLY_CLAIM_TREASURY_BPS = 6_000n;

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
  migrateV2: fromHex('bbcb121fceedfe29'),
  buyRigs: fromHex('0cfa7948abf4e5a3'),
  claim: fromHex('3ec6d6c1d59f6cd2'),
  compound: fromHex('a5d0fb4ef2a08d2f'),
  // Operator-only; used by app/scripts/ops, never by the browser bundle.
  setFeeRecipient: fromHex('e312d72aedf69742'),
  withdrawFees: fromHex('c6d4ab6d90d7ae59'),
  // §7a protocol fees.
  claimCommission: fromHex('0c090faa9beb7cfe'),
  applyFeeChange: fromHex('e7b15c75f934c370'),
  // Operator-only.
  scheduleFeeChange: fromHex('4117d76e1094f993'),
  cancelFeeChange: fromHex('a52beb3f1f674984'),
  migrateV3: fromHex('f5aa677c90bb1566'),
  // Lock Season (§8, Pass #7). Wallet: register / cancel / claim.
  registerLock: fromHex('52880400d0cab4c7'),
  cancelLock: fromHex('139c6c7fbaef5f8a'),
  claimLock: fromHex('1b6cc228c82ef1d2'),
  // Permissionless tally / settlement steps.
  sealLockWeights: fromHex('81f2808f1292f69d'),
  awardLockRegistrations: fromHex('c9ed2b56f8bd4d2d'),
  sealLockAwards: fromHex('31ebece092c7b362'),
  finalizeLockAllocation: fromHex('cd059b8bf32a3cbe'),
  finalizeLockRoot: fromHex('b7f122446573734e'),
  closeLockPool: fromHex('72625957d9a931b6'),
  // Operator-only (admin / root poster / veto set).
  commitLockPool: fromHex('c1ba6df208b5e124'),
  postLockWeights: fromHex('2a99e134b1a77b54'),
  postLockRoot: fromHex('3be6c66b30087f4e'),
  vetoLockAllocation: fromHex('4c01759c354469be'),
  resolveLockPoolTerminal: fromHex('1784afc44d7773e8'),
} as const;

export const ACCOUNT_DISCRIMINATOR = {
  config: fromHex('9b0caae01efacc82'),
  miner: fromHex('df710f367b7a8c64'),
  vault: fromHex('d308e82b02987577'),
  referralLedger: fromHex('a30861cf910b5b80'),
  lockSeason: fromHex('e1527c8e3d57ca84'),
  lockRegistration: fromHex('55d00114b6413c41'),
} as const;

/** Byte length of each account, discriminator included. */
export const ACCOUNT_SIZE = {
  /** V3: V2 (329) plus the architecture §7a protocol-fee fields. */
  config: 375,
  miner: 171,
  referralLedger: 8 + 32 + 8 + 8 + 1,
} as const;
