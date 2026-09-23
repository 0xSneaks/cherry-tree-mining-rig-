/**
 * The hand-written Borsh decoders are checked against fixtures serialised by
 * the Rust program itself (`cargo test -p sol-rigs --test discriminators`), so
 * a layout change in `state.rs` fails here instead of silently shifting every
 * number the UI shows.
 */
import { describe, expect, it } from 'vitest';

import {
  ACCOUNT_DISCRIMINATOR,
  ACCOUNT_SIZE,
} from '../src/lib/program/constants';
import { decodeConfig, decodeMiner } from '../src/lib/program/accounts';

const CONFIG_FIXTURE =
  '9b0caae01efacc82010101010101010101010101010101010101010101010101010101010101010102020202020202020202020202020202020202020202020202020202020202020303030303030303030303030303030303030303030303030303030303030303dd4fd8b645aa70d07045a4fc0adf6fc5c9d235e334252c8d5da4b989a625d9610180969800000000002c01c937000000000000b288840100000000803bb16a0000000000a5186a00000000d20a3f4eeee073c3f60fe98e010000009210000000000000c007857303000000c08c6c050000000040ed9604000000000000943577000000000087930300000040420f0000000000db03000000000000fafbfc0100fddd4fd8b645aa70d07045a4fc0adf6fc5c9d235e334252c8d5da4b989a625d961601e176a00000000f5cc150000000000a025260000000000';

const MINER_FIXTURE =
  'df710f367b7a8c6409090909090909090909090909090909090909090909090909090909090909092a00000000000000ea7e26e5384da55a050000000000000038191701000000001e000000000000000c0000000000000040f76a1200000000000e270700000000e0dc4e0300000000800e80690000000040f0646a00000000fe6900070707070707070707070707070707070707070707070707070707070707070700ae556a00000000';

function bytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

describe('Config decoding', () => {
  const data = bytes(CONFIG_FIXTURE);

  it('is exactly the size the program allocates', () => {
    expect(data.length).toBe(ACCOUNT_SIZE.config);
  });

  it('round-trips every field from the Rust fixture', () => {
    const c = decodeConfig(data);
    // The Rust fixture carries the real production fee recipient, so a layout
    // shift that silently moved this field would be caught here rather than by
    // paying protocol revenue to the wrong address.
    expect(c.feeRecipient).toBe('FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp');
    expect(c.paused).toBe(true);
    expect(c.rigPriceLamports).toBe(10_000_000n);
    expect(c.purchaseFeeBps).toBe(300);
    expect(c.totalRigs).toBe(14_281n);
    expect(c.rewardRate).toBe(25_462_962n);
    expect(c.rewardPeriodFinish).toBe(1_790_000_000n);
    expect(c.rewardLastUpdate).toBe(1_780_000_000n);
    expect(c.rewardPerRigStored).toBe(123_456_789_012_345_678_901_234_567_890n);
    expect(c.undistributedDust).toBe(4_242n);
    expect(c.totalRewardFunded).toBe(14_823_000_000n);
    expect(c.totalRewardClaimed).toBe(91_000_000n);
    expect(c.totalRewardCompounded).toBe(77_000_000n);
    expect(c.totalCapitalReceived).toBe(512_000_000_000n);
    expect(c.totalProtocolFees).toBe(15_360_000_000n);
    expect(c.totalFeesWithdrawn).toBe(1_000_000n);
    expect(c.minerCount).toBe(987n);
    expect(c.capitalVaultBump).toBe(250);
    expect(c.rewardVaultBump).toBe(251);
    expect(c.feeVaultBump).toBe(252);
    expect(c.version).toBe(1);
    expect(c.bump).toBe(253);
    expect(c.founder).toBe('FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp');
    expect(c.launchT0).toBe(1_779_900_000n);
    expect(c.totalShares).toBe(14_156n * 100n + 125n * 105n);
    expect(c.totalClaimPenalties).toBe(2_500_000n);
  });

  it('rejects data carrying another account type', () => {
    const wrong = bytes(CONFIG_FIXTURE);
    wrong.set(ACCOUNT_DISCRIMINATOR.miner, 0);
    expect(() => decodeConfig(wrong)).toThrow(/discriminator/);
  });
});

describe('Miner decoding', () => {
  const data = bytes(MINER_FIXTURE);

  it('is exactly the size the program allocates', () => {
    expect(data.length).toBe(ACCOUNT_SIZE.miner);
  });

  it('round-trips every field from the Rust fixture', () => {
    const m = decodeMiner(data);
    expect(m.rigs).toBe(42n);
    expect(m.rewardPerRigPaid).toBe(98_765_432_109_876_543_210n);
    expect(m.pendingRewards).toBe(18_291_000n);
    expect(m.lifetimeRigsBought).toBe(30n);
    expect(m.lifetimeRigsCompounded).toBe(12n);
    expect(m.lifetimeSolSpent).toBe(309_000_000n);
    expect(m.lifetimeSolCompounded).toBe(120_000_000n);
    expect(m.lifetimeSolClaimed).toBe(55_500_000n);
    expect(m.createdAt).toBe(1_770_000_000n);
    expect(m.lastActionAt).toBe(1_785_000_000n);
    expect(m.bump).toBe(254);
    expect(m.weight).toBe(105);
    expect(m.referrer).toBe('US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx');
    expect(m.lastPlantedAt).toBe(1_784_000_000n);
  });
});
