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
  '9b0caae01efacc82010101010101010101010101010101010101010101010101010101010101010102020202020202020202020202020202020202020202020202020202020202020303030303030303030303030303030303030303030303030303030303030303306259c2b26d5fc306ded332272d32d11bd21d06f6631acd02845213487a2aa60180969800000000002c01c937000000000000b288840100000000803bb16a0000000000a5186a00000000d20a3f4eeee073c3f60fe98e010000009210000000000000c007857303000000c08c6c050000000040ed9604000000000000943577000000000087930300000040420f0000000000db03000000000000fafbfc0300fddd4fd8b645aa70d07045a4fc0adf6fc5c9d235e334252c8d5da4b989a625d961601e176a00000000f5cc150000000000a025260000000000c409000000000000fa008403200380991c6a00000000d5dc3200000000001cd143000000000047f410000000000063c5540000000000';

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
    expect(c.feeRecipient).toBe('4FsYyXTbF1iftkUTtZVmzmvisxUaGvoYfZDj4DuGLvwF');
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
    expect(c.version).toBe(3);
    expect(c.bump).toBe(253);
    expect(c.founder).toBe('FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp');
    expect(c.launchT0).toBe(1_779_900_000n);
    expect(c.totalShares).toBe(14_156n * 100n + 125n * 105n);
    expect(c.totalClaimPenalties).toBe(2_500_000n);
    expect(c.genesisCapRigs).toBe(2_500n);
    // V3 §7a fee fields.
    expect(c.compoundFeeBps).toBe(250);
    expect(c.pendingPurchaseFeeBps).toBe(900);
    expect(c.pendingCompoundFeeBps).toBe(800);
    expect(c.feeChangeEffectiveAt).toBe(1_780_259_200n);
    expect(c.reservedCommissions).toBe(3_333_333n);
    expect(c.totalCommissionsReserved).toBe(4_444_444n);
    expect(c.totalCommissionsPaid).toBe(1_111_111n);
    expect(c.totalRewardToFees).toBe(5_555_555n);
  });

  it('rejects data carrying another account type', () => {
    const wrong = bytes(CONFIG_FIXTURE);
    wrong.set(ACCOUNT_DISCRIMINATOR.miner, 0);
    expect(() => decodeConfig(wrong)).toThrow(/discriminator/);
  });

  it('rejects data of none of the V1, V2 or V3 lengths', () => {
    expect(() => decodeConfig(bytes(CONFIG_FIXTURE.slice(0, 642 - 16)))).toThrow(/length/);
    expect(() => decodeConfig(bytes(CONFIG_FIXTURE.slice(0, 700)))).toThrow(/length/);
  });
});

describe('Config decoding (V2, pre-§7a)', () => {
  // A V2 Config: the V3 fixture with version 2 and the 46 fee bytes dropped
  // — the exact inverse of what `migrate_v3` writes on chain.
  const v2 = bytes(CONFIG_FIXTURE);
  v2.set([0x02, 0x00], 262);
  const data = v2.subarray(0, 329);

  it('reads the legacy layout with the compound fee equal to the purchase fee', () => {
    const c = decodeConfig(data);
    expect(c.version).toBe(2);
    expect(c.genesisCapRigs).toBe(2_500n);
    expect(c.compoundFeeBps).toBe(c.purchaseFeeBps);
    expect(c.feeChangeEffectiveAt).toBe(0n);
    expect(c.reservedCommissions).toBe(0n);
    expect(c.totalRewardToFees).toBe(0n);
  });
});

describe('Config decoding (V1 Devnet)', () => {
  // The canonical Devnet Config predates V2's appended `genesis_cap_rigs`:
  // 321 bytes, version 1, no cap. The fixture is the Rust V2 fixture with
  // the version reset to 1 and the final `u64` dropped — the exact inverse
  // of what `migrate_v2` writes on chain.
  const v1 = bytes(CONFIG_FIXTURE);
  v1.set([0x01, 0x00], 262); // version, offset 8 (discriminator) + 254
  const data = v1.subarray(0, 321);

  it('is exactly the size the V1 program allocates', () => {
    expect(data.length).toBe(321);
  });

  it('round-trips every V1 field and reports no genesis cap', () => {
    const c = decodeConfig(data);
    expect(c.version).toBe(1);
    expect(c.genesisCapRigs).toBeNull();
    expect(c.feeRecipient).toBe('4FsYyXTbF1iftkUTtZVmzmvisxUaGvoYfZDj4DuGLvwF');
    expect(c.rigPriceLamports).toBe(10_000_000n);
    expect(c.purchaseFeeBps).toBe(300);
    expect(c.totalRigs).toBe(14_281n);
    expect(c.founder).toBe('FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp');
    expect(c.launchT0).toBe(1_779_900_000n);
    expect(c.totalShares).toBe(14_156n * 100n + 125n * 105n);
    expect(c.totalClaimPenalties).toBe(2_500_000n);
  });

  it('agrees with the V2 decode on every shared field', () => {
    // `version` and the cap are exactly the two fields V2 changes.
    const v2 = bytes(CONFIG_FIXTURE);
    v2.set([0x02, 0x00], 262);
    const { genesisCapRigs: v1Cap, version: v1Ver, ...v1Rest } = decodeConfig(data);
    const { genesisCapRigs: v2Cap, version: v2Ver, ...v2Rest } = decodeConfig(
      v2.subarray(0, 329),
    );
    expect(v1Cap).toBeNull();
    expect(v1Ver).toBe(1);
    expect(v2Cap).toBe(2_500n);
    expect(v2Ver).toBe(2);
    expect(v1Rest).toEqual(v2Rest);
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
