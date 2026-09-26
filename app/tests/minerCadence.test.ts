import { address, getAddressEncoder, getU16Encoder, getU64Encoder } from '@solana/kit';
import { describe, expect, it } from 'vitest';

import { buyResetDisclosure, penaltyCadence, replantState } from '../src/lib/minerCadence';
import { minerActivity, safeExplorerTxUrl } from '../src/lib/program/activity';
import type { EventRow } from '../src/lib/program/eventHistory';
import { decodeEvent, EVENT_DISCRIMINATOR } from '../src/lib/program/events';

const DAY = 86_400n;
const OWNER = address('7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU');
const OTHER = address('US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx');

describe('penalty cockpit cadence', () => {
  const planted = 1_000_000n;

  it.each([
    ['30% start', 0n, 3_000, 2_000, DAY, 3n * DAY],
    ['30% final second', DAY - 1n, 3_000, 2_000, 1n, 2n * DAY + 1n],
    ['20% exact boundary', DAY, 2_000, 1_000, DAY, 2n * DAY],
    ['20% final second', 2n * DAY - 1n, 2_000, 1_000, 1n, DAY + 1n],
    ['10% exact boundary', 2n * DAY, 1_000, 0, DAY, DAY],
    ['10% final second', 3n * DAY - 1n, 1_000, 0, 1n, 1n],
  ])('%s', (_label, age, bps, next, toNext, toRooted) => {
    expect(penaltyCadence(planted, planted + age)).toEqual({
      penaltyBps: bps,
      nextPenaltyBps: next,
      secondsToNextStep: toNext,
      secondsToRooted: toRooted,
      rooted: false,
    });
  });

  it('is Rooted at the exact 72-hour boundary', () => {
    expect(penaltyCadence(planted, planted + 3n * DAY)).toEqual({
      penaltyBps: 0,
      nextPenaltyBps: null,
      secondsToNextStep: null,
      secondsToRooted: 0n,
      rooted: true,
    });
  });

  it('treats a chain clock behind the planting anchor as age zero', () => {
    const state = penaltyCadence(planted, planted - 5n);
    expect(state).toMatchObject({ penaltyBps: 3_000, secondsToNextStep: DAY });
  });
});

describe('replant current-state arithmetic', () => {
  // Replant is `compound`, priced at the compound fee (architecture §7a).
  const config = { rigPriceLamports: 10_000_000n, compoundFeeBps: 300, totalShares: 100_000n };
  const miner = { rigs: 42n, weight: 105 };

  it('mints only whole seeds, includes the fee, and preserves dust', () => {
    expect(replantState(config, miner, 25_000_000n)).toEqual({
      availableLamports: 25_000_000n,
      wholeSeeds: 2n,
      costLamports: 20_600_000n,
      remainderLamports: 4_400_000n,
      resultingSeeds: 44n,
      resultingShares: 4_620n,
      totalSharesAfter: 100_210n,
    });
  });

  it('handles zero and sub-seed dust without inventing a seed', () => {
    expect(replantState(config, miner, 0n)).toMatchObject({ wholeSeeds: 0n, remainderLamports: 0n, resultingSeeds: 42n });
    expect(replantState(config, miner, 9_999_999n)).toMatchObject({ wholeSeeds: 0n, remainderLamports: 9_999_999n, resultingSeeds: 42n });
  });
});

describe('fresh-buy reset disclosure', () => {
  it('shows the §7a top-up warning, with the pending amount, when rewards are pending', () => {
    const copy = buyResetDisclosure(true, 1_500_000n);
    expect(copy).toBe(
      'Buying more seeds restarts your early-claim timer. Your unclaimed rewards ' +
        '(0.001500 SOL) would then carry the 30% early-claim fee for the next 24 hours. ' +
        'Claim them first to keep the full amount.',
    );
    expect(copy).not.toMatch(/should|profit|guarantee|ROI|return per day/i);
  });

  it('shows no top-up warning when nothing is pending, only the fee disclosure', () => {
    for (const [has, pending] of [
      [true, 0n],
      [false, 0n],
    ] as const) {
      const copy = buyResetDisclosure(has, pending);
      expect(copy).not.toMatch(/restarts your early-claim timer/i);
      expect(copy).toMatch(/30% under 24h, 20% from 24h to 48h, 10% from 48h to 72h, 0% after/);
      expect(copy).toMatch(/60% of the fee goes to the Protocol Treasury and 40% is redistributed/);
    }
  });
});

describe('personal activity', () => {
  const signature = '2'.repeat(88);
  const base = { signature, blockTime: 123n };
  const rows: EventRow[] = [
    { ...base, event: { name: 'RigsPurchased', owner: OWNER, count: 5n, capitalLamports: 50n, feeLamports: 2n, totalLamports: 52n, userRigs: 5n, totalRigs: 5n } },
    { ...base, event: { name: 'Compound', owner: OWNER, rigsMinted: 2n, capitalLamports: 20n, feeLamports: 1n, totalLamports: 21n, remainderLamports: 3n, userRigs: 7n, totalRigs: 7n } },
    { ...base, event: { name: 'Claim', owner: OWNER, amount: 75n, gross: 100n, penalty: 25n, penaltyBps: 2_500, userRigs: 7n, totalRewardClaimed: 75n } },
    { ...base, event: { name: 'ReferralBound', miner: OWNER, referrer: OTHER, weight: 105 } },
    { ...base, event: { name: 'LaunchActivated', founder: OWNER, launchT0: 100n, rigs: 1n, totalLamports: 10n } },
    { ...base, event: { name: 'RigsPurchased', owner: OTHER, count: 9n, capitalLamports: 90n, feeLamports: 3n, totalLamports: 93n, userRigs: 9n, totalRigs: 16n } },
  ];

  it('maps only the connected wallet’s actual decoded events', () => {
    const activity = minerActivity(rows, OWNER);
    expect(activity.map((row) => row.action)).toEqual([
      'Fresh plant',
      'Replant',
      'Harvest',
      'Referral bound',
      'Founder activation',
    ]);
    expect(activity[2]).toMatchObject({ amountLamports: 75n, detail: '25% early-claim fee' });
  });

  it('returns an empty history and rejects unsafe explorer signatures', () => {
    expect(minerActivity([], OWNER)).toEqual([]);
    expect(safeExplorerTxUrl(signature)).toMatch(/^https:\/\/explorer\.solana\.com\/tx\//);
    expect(safeExplorerTxUrl('../javascript:alert(1)')).toBeNull();
    expect(safeExplorerTxUrl('2'.repeat(63))).toBeNull();
  });

  it('decodes the existing Claim and Compound program events', () => {
    const bytes = (...parts: Uint8Array[]) => {
      const out = new Uint8Array(parts.reduce((total, part) => total + part.length, 0));
      let offset = 0;
      for (const part of parts) {
        out.set(part, offset);
        offset += part.length;
      }
      return out;
    };
    const u64 = (value: bigint) => Uint8Array.from(getU64Encoder().encode(value));
    expect(decodeEvent(bytes(
      EVENT_DISCRIMINATOR.Claim,
      new Uint8Array(getAddressEncoder().encode(OWNER)),
      u64(75n), u64(100n), u64(25n), Uint8Array.from(getU16Encoder().encode(2_500)), u64(7n), u64(500n),
    ))).toMatchObject({ name: 'Claim', owner: OWNER, amount: 75n, gross: 100n, penaltyBps: 2_500 });
    expect(decodeEvent(bytes(
      EVENT_DISCRIMINATOR.Compound,
      new Uint8Array(getAddressEncoder().encode(OWNER)),
      u64(2n), u64(20n), u64(1n), u64(21n), u64(3n), u64(7n), u64(100n),
    ))).toMatchObject({ name: 'Compound', owner: OWNER, rigsMinted: 2n, remainderLamports: 3n });
  });
});
