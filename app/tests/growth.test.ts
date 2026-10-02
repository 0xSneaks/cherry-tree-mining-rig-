/**
 * Off-chain growth layer: concentration, dilution, runway / Dormant,
 * milestones, referral qualification + leaderboard, seasons, events.
 */
import { createHash } from 'node:crypto';
import { getAddressEncoder, getBase64Decoder, getI64Encoder, getU16Encoder, getU64Encoder } from '@solana/kit';
import { describe, expect, it } from 'vitest';

import { distribution, PER_WALLET_CAVEAT } from '../src/lib/growth/concentration';
import { dilutionView, whatIfSeedsAdded, DILUTION_EXPLAINER } from '../src/lib/growth/dilution';
import {
  achievements,
  groveTier,
  isGenesisPlanter,
  isRooted,
  nextGroveTier,
  parseVisitLog,
  recordVisit,
  reminders,
} from '../src/lib/growth/milestones';
import {
  myReferrals,
  parseQualification,
  referralLeaderboard,
  referrerTier,
  seasonLeaderboard,
  type ReferredPosition,
} from '../src/lib/growth/referralStats';
import { projectionsAllowed, refillHistory, runwayReport, STATUS_COPY } from '../src/lib/growth/runway';
import { parseSeasons, seasonState, validateSeasons, type Season } from '../src/lib/growth/seasons';
import { decodeEvent, eventsFromLogs, EVENT_DISCRIMINATOR } from '../src/lib/program/events';
import { DAY, NOW, SYS, WALLET_1, WALLET_2, WALLET_3, WALLET_4, config, miner } from './growthFixtures';

describe('concentration', () => {
  it('handles an empty grove', () => {
    const d = distribution([]);
    expect(d).toMatchObject({ wallets: 0, largestBps: 0, hhi: 0, giniBps: 0, level: 'dispersed' });
  });

  it('one wallet holds everything', () => {
    const d = distribution([{ owner: 'a', rigs: 10n, weight: 100 }]);
    expect(d.largestBps).toBe(10_000);
    expect(d.hhi).toBe(10_000);
    expect(d.level).toBe('highlyConcentrated');
  });

  it('equal wallets are dispersed with zero Gini', () => {
    const d = distribution(Array.from({ length: 100 }, (_, i) => ({ owner: `w${i}`, rigs: 5n, weight: 100 })));
    expect(d.largestBps).toBe(100);
    expect(d.hhi).toBe(100);
    expect(d.giniBps).toBe(0);
    expect(d.level).toBe('dispersed');
    expect(d.whales).toEqual([]);
    expect(d.medianRigs).toBe(5n);
  });

  it('weights referred seeds at 105 and flags whales', () => {
    const rows = [
      { owner: 'whale', rigs: 900n, weight: 100 },
      ...Array.from({ length: 10 }, (_, i) => ({ owner: `r${i}`, rigs: 10n, weight: 105 })),
    ];
    const d = distribution(rows);
    expect(d.totalShares).toBe(90_000n + 10_500n);
    expect(d.largestBps).toBe(Number((90_000n * 10_000n) / 100_500n));
    expect(d.whales.map((w) => w.owner)).toEqual(['whale']);
    expect(d.level).toBe('highlyConcentrated');
    expect(d.giniBps).toBeGreaterThan(7_000);
  });

  it('ignores zero-seed rows', () => {
    expect(distribution([{ owner: 'z', rigs: 0n, weight: 100 }]).wallets).toBe(0);
  });

  it('carries a per-wallet, not per-person caveat', () => {
    expect(PER_WALLET_CAVEAT).toMatch(/per wallet, not per person/);
    expect(PER_WALLET_CAVEAT).toMatch(/does not prove one human per wallet/);
  });
});

describe('dilution', () => {
  const cfg = config({ totalShares: 100_000n, rewardRate: 1_000n, rewardPeriodFinish: NOW + 10n * DAY });
  const m = miner({ rigs: 100n, weight: 100 }); // 10_000 shares = 10%

  it('computes share, per-day and remaining funded slice', () => {
    const v = dilutionView(cfg, m, NOW);
    expect(v.shareBps).toBe(1_000);
    expect(v.perDayLamports).toBe((10_000n * 1_000n * DAY) / 100_000n);
    expect(v.remainingFundedLamports).toBe((10_000n * 1_000n * 10n * DAY) / 100_000n);
    expect(v.projectionsAllowed).toBe(true);
  });

  it('shows share falling as total shares grow', () => {
    const v = dilutionView(cfg, m, NOW, [0, 10_000, 30_000]);
    expect(v.rows.map((r) => r.shareBps)).toEqual([1_000, 500, 250]);
    expect(v.rows[1]!.addedSeeds).toBe(1_000n);
  });

  it('what-if N seeds', () => {
    const r = whatIfSeedsAdded(cfg, m, NOW, 1_000n);
    expect(r.totalShares).toBe(200_000n);
    expect(r.shareBps).toBe(500);
    expect(r.increaseBps).toBe(10_000);
    expect(() => whatIfSeedsAdded(cfg, m, NOW, -1n)).toThrow();
  });

  it('never projects when emission is zero', () => {
    const dormant = config({ rewardPeriodFinish: NOW - 1n });
    const v = dilutionView(dormant, m, NOW);
    expect(v.projectionsAllowed).toBe(false);
    expect(v.perDayLamports).toBe(0n);
    expect(v.remainingFundedLamports).toBe(0n);
    expect(v.rows.every((r) => r.perDayLamports === 0n && r.remainingFundedLamports === 0n)).toBe(true);
  });

  it('a viewer without a position holds 0%', () => {
    expect(dilutionView(cfg, null, NOW).shareBps).toBe(0);
  });

  it('explains dilution without promises', () => {
    expect(DILUTION_EXPLAINER).toMatch(/not a forecast or a promise/);
    expect(DILUTION_EXPLAINER).not.toMatch(/guarantee/i);
  });
});

describe('runway and Dormant', () => {
  it('live, healthy', () => {
    const r = runwayReport(config(), 3_500_000_000n, NOW);
    expect(r.status).toBe('live');
    expect(r.alert).toBe('healthy');
    expect(r.runwaySeconds).toBe(30n * DAY);
    expect(r.futureEmissionLamports).toBe(1_000n * 30n * DAY);
    expect(r.ledgerRemainingLamports).toBe(3_500_000_000n);
    expect(r.backed).toBe(true);
  });

  it('low and critical runway', () => {
    expect(runwayReport(config({ rewardPeriodFinish: NOW + 10n * DAY }), 3_500_000_000n, NOW).alert).toBe('low');
    const c = runwayReport(config({ rewardPeriodFinish: NOW + 2n * DAY }), 3_500_000_000n, NOW);
    expect(c.status).toBe('lowRunway');
    expect(c.alert).toBe('critical');
  });

  it('Dormant: runway 0, no future emission, accrued stays backed', () => {
    const r = runwayReport(config({ rewardPeriodFinish: NOW }), 3_500_000_000n, NOW);
    expect(r.status).toBe('dormant');
    expect(r.alert).toBe('dormant');
    expect(r.runwaySeconds).toBe(0n);
    expect(r.futureEmissionLamports).toBe(0n);
    expect(r.emissionPerDayLamports).toBe(0n);
    expect(r.accruedBackingLamports).toBe(3_500_000_000n);
    expect(projectionsAllowed(config({ rewardPeriodFinish: NOW }), NOW)).toBe(false);
  });

  it('unfunded', () => {
    const r = runwayReport(
      config({ totalRewardFunded: 0n, totalRewardClaimed: 0n, totalRewardCompounded: 0n, rewardRate: 0n, rewardPeriodFinish: 0n }),
      0n,
      NOW,
    );
    expect(r.status).toBe('unfunded');
  });

  it('flags a backing shortfall', () => {
    expect(runwayReport(config(), 3_499_999_999n, NOW).alert).toBe('backingShortfall');
  });

  it('Dormant copy never implies planting restarts emission or refill is guaranteed', () => {
    expect(STATUS_COPY.dormant.body).toMatch(/does not restart emission/);
    expect(STATUS_COPY.dormant.body).toMatch(/none is promised/);
    expect(STATUS_COPY.dormant.body).toMatch(/stays claimable/);
    for (const c of Object.values(STATUS_COPY)) expect(c.body).not.toMatch(/guaranteed (return|yield)|ROI/i);
  });

  it('refill history labels funding after dormancy', () => {
    const ev = (amount: bigint, finish: bigint) => ({
      name: 'RewardsFunded' as const,
      funder: WALLET_2,
      amount,
      duration: 10n,
      newRewardRate: 1n,
      periodFinish: finish,
      rewardVaultBalance: 0n,
      outstandingLiabilities: 0n,
    });
    const h = refillHistory([
      { signature: 's2', blockTime: 200n, event: ev(2n, 300n) },
      { signature: 's1', blockTime: 100n, event: ev(1n, 150n) },
      { signature: 's3', blockTime: 250n, event: ev(3n, 400n) },
    ]);
    expect(h.map((e) => e.signature)).toEqual(['s1', 's2', 's3']);
    expect(h.map((e) => e.afterDormancy)).toEqual([false, true, false]);
  });
});

describe('milestones (post-72h engagement)', () => {
  const cfg = config({ launchT0: NOW - 10n * DAY });

  it('tiers are monotone in seeds and seeds never decrease', () => {
    expect(groveTier(0n)).toBeNull();
    expect(groveTier(1n)!.id).toBe('seedling');
    expect(groveTier(150n)!.id).toBe('youngTree');
    expect(nextGroveTier(150n)).toEqual({ tier: expect.objectContaining({ id: 'blossoming' }), seedsToGo: 850n });
    expect(nextGroveTier(10_000n)).toBeNull();
  });

  it('Genesis planter is decided by createdAt in [T0, T0+24h)', () => {
    expect(isGenesisPlanter({ createdAt: cfg.launchT0 }, cfg.launchT0)).toBe(true);
    expect(isGenesisPlanter({ createdAt: cfg.launchT0 + DAY - 1n }, cfg.launchT0)).toBe(true);
    expect(isGenesisPlanter({ createdAt: cfg.launchT0 + DAY }, cfg.launchT0)).toBe(false);
    expect(isGenesisPlanter({ createdAt: 5n }, 0n)).toBe(false);
  });

  it('achievements come from permanent state only and never mutate it', () => {
    const m = Object.freeze(
      miner({ createdAt: cfg.launchT0 + 60n, weight: 105, lifetimeRigsCompounded: 12n, lifetimeSolClaimed: 1n }),
    );
    const got = Object.fromEntries(achievements(m, cfg, NOW).map((a) => [a.id, a.earned]));
    expect(got.firstSeed).toBe(true);
    expect(got.genesisPlanter).toBe(true);
    expect(got.referred).toBe(true);
    expect(got.firstHarvest).toBe(true);
    expect(got.replant1).toBe(true);
    expect(got.replant10).toBe(true);
    expect(got.replant100).toBe(false);
    expect(got.tenure7).toBe(true);
    expect(got.tenure30).toBe(false);
  });

  it('rooted after 72h since the latest fresh purchase; reminders say buying restarts the clock', () => {
    const fresh = miner({ lastPlantedAt: NOW - 50n * 3_600n });
    expect(isRooted(fresh, NOW)).toBe(false);
    const r = reminders(fresh, cfg, NOW);
    const p = r.find((x) => x.id === 'penalty')!;
    expect(p.atSeconds).toBe(fresh.lastPlantedAt + 3n * DAY);
    expect(p.text).toMatch(/free of the early-claim fee/);
    expect(p.text).toMatch(/replanting would not/);
    expect(isRooted(miner({ lastPlantedAt: NOW - 3n * DAY }), NOW)).toBe(true);
    const early = reminders(miner({ lastPlantedAt: NOW - 3_600n }), cfg, NOW).find((x) => x.id === 'penalty')!;
    expect(early.text).toBe('Early-claim fee drops to 20%.');
  });

  it('reminders are sorted and include the funded-window end', () => {
    const r = reminders(miner({ lastPlantedAt: NOW - 3n * DAY }), cfg, NOW);
    expect(r.map((x) => x.id)).toEqual(['windowEnd']);
    expect(r[0]!.text).toMatch(/unless a new schedule is separately funded/);
  });

  it('days tended only ever goes up; a gap never subtracts', () => {
    let log = recordVisit(null, 100);
    log = recordVisit(log, 100);
    expect(log.days).toBe(1);
    log = recordVisit(log, 101);
    log = recordVisit(log, 150); // missed 48 days
    expect(log).toEqual({ days: 3, lastDay: 150 });
    expect(recordVisit(log, 120)).toBe(log);
    expect(() => recordVisit(log, -1)).toThrow();
    expect(parseVisitLog(JSON.stringify(log))).toEqual(log);
    expect(parseVisitLog('{"days":0,"lastDay":1}')).toBeNull();
    expect(parseVisitLog('garbage')).toBeNull();
  });
});

describe('referral qualification and leaderboard', () => {
  const q = parseQualification('5', '72')!;
  const pos = (over: Partial<ReferredPosition>): ReferredPosition => ({
    owner: 'o',
    referrer: WALLET_2,
    createdAt: NOW - 10n * DAY,
    rigs: 5n,
    lifetimeRigsBought: 5n,
    lifetimeRigsCompounded: 0n,
    ...over,
  });

  it('qualification must be configured, and wallet creation alone never qualifies', () => {
    expect(parseQualification(undefined, undefined)).toBeNull();
    expect(parseQualification('0', '24')).toBeNull();
    expect(parseQualification('x', '24')).toBeNull();
    expect(q).toEqual({ minFreshSeeds: 5n, minHoldSeconds: 72n * 3_600n });
  });

  it('leaderboard is closed until qualification is configured', () => {
    expect(referralLeaderboard([pos({})], { qualification: null, at: NOW })).toEqual({
      status: 'closed',
      reason: 'qualificationNotConfigured',
    });
  });

  it('counts only meaningful fresh participation that has been held', () => {
    const rows = [
      pos({ owner: 'a' }),
      pos({ owner: 'b', lifetimeRigsBought: 4n, rigs: 40n, lifetimeRigsCompounded: 36n }), // compounding does not count
      pos({ owner: 'c', createdAt: NOW - 3_600n }), // not held long enough
      pos({ owner: 'd', rigs: 0n }),
    ];
    const lb = referralLeaderboard(rows, { qualification: q, at: NOW });
    expect(lb.status).toBe('open');
    if (lb.status !== 'open') return;
    expect(lb.rows).toHaveLength(1);
    expect(lb.rows[0]).toMatchObject({ referrer: WALLET_2, total: 4, qualified: 1, pending: 3, rank: 1 });
    expect(lb.rows[0]!.tier!.id).toBe('sharer');
  });

  it('excludes the official referrer, self rows and unreferred rows', () => {
    const rows = [
      pos({ owner: 'a', referrer: WALLET_3 }),
      pos({ owner: 'b', referrer: SYS }),
      pos({ owner: WALLET_4, referrer: WALLET_4 }),
      pos({ owner: 'c', referrer: WALLET_1 }),
    ];
    const lb = referralLeaderboard(rows, { qualification: q, at: NOW, excluded: [WALLET_3] });
    expect(lb.status === 'open' && lb.rows.map((r) => r.referrer)).toEqual([WALLET_1]);
  });

  it('ranks by qualified count, then earliest qualifying referral', () => {
    const rows = [
      pos({ owner: 'a1', referrer: WALLET_1, createdAt: NOW - 9n * DAY }),
      pos({ owner: 'b1', referrer: WALLET_2, createdAt: NOW - 8n * DAY }),
      pos({ owner: 'c1', referrer: WALLET_4, createdAt: NOW - 8n * DAY }),
      pos({ owner: 'c2', referrer: WALLET_4, createdAt: NOW - 7n * DAY }),
    ];
    const lb = referralLeaderboard(rows, { qualification: q, at: NOW });
    expect(lb.status === 'open' && lb.rows.map((r) => [r.referrer, r.rank])).toEqual([
      [WALLET_4, 1],
      [WALLET_1, 2],
      [WALLET_2, 3],
    ]);
  });

  it('flags bursts and minimum-only farming for review without removing anything', () => {
    const rows = Array.from({ length: 6 }, (_, i) =>
      pos({ owner: `f${i}`, referrer: WALLET_1, createdAt: NOW - 10n * DAY + BigInt(i * 30) }),
    );
    const lb = referralLeaderboard(rows, { qualification: q, at: NOW });
    expect(lb.status === 'open' && lb.rows[0]!.flags).toEqual(['burst', 'minimumOnly']);
    expect(lb.status === 'open' && lb.rows[0]!.qualified).toBe(6);
  });

  it('tiers', () => {
    expect(referrerTier(0)).toBeNull();
    expect(referrerTier(5)!.id).toBe('guide');
    expect(referrerTier(250)!.id).toBe('ambassador');
  });

  it('my referrals', () => {
    const rows = [pos({ owner: 'a', referrer: WALLET_1 }), pos({ owner: 'b', referrer: WALLET_1, createdAt: NOW })];
    expect(myReferrals(rows, WALLET_1, { qualification: q, at: NOW })).toMatchObject({
      total: 2,
      qualified: 1,
      pending: 1,
      configured: true,
    });
    expect(myReferrals(rows, WALLET_1, { qualification: null, at: NOW }).configured).toBe(false);
  });
});

describe('seasons', () => {
  const s1: Season = {
    id: 'season-1',
    name: 'First Bloom',
    startsAt: NOW - 20n * DAY,
    endsAt: NOW - 5n * DAY,
    competitions: ['referrals'],
    qualificationGraceSeconds: 3n * DAY,
    prizes: [
      {
        description: 'Cherry Tree cosmetic badge pack',
        fundingSource: 'separately-funded',
        funder: 'PhantomCapAI marketing wallet',
        disclosureUrl: 'https://example.org/season-1',
      },
    ],
  };

  it('states', () => {
    expect(seasonState(s1, NOW)).toBe('ended');
    expect(seasonState(s1, s1.startsAt)).toBe('active');
    expect(seasonState(s1, s1.startsAt - 1n)).toBe('upcoming');
  });

  it('validation rejects prizes from protocol vaults and promise language', () => {
    expect(validateSeasons([s1], [WALLET_3])).toEqual([]);
    const bad: Season = {
      ...s1,
      id: 'Bad Id',
      prizes: [{ ...s1.prizes[0]!, funder: WALLET_3, description: 'Guaranteed ROI' }],
    };
    const errs = validateSeasons([bad], [WALLET_3]);
    expect(errs.join('\n')).toMatch(/lowercase slug/);
    expect(errs.join('\n')).toMatch(/protocol vault/);
    expect(errs.join('\n')).toMatch(/must not promise/);
  });

  it('validation rejects overlaps and inverted windows', () => {
    const s2: Season = { ...s1, id: 'season-2', startsAt: s1.endsAt - 1n, endsAt: s1.endsAt + DAY };
    expect(validateSeasons([s1, s2], []).join()).toMatch(/overlaps/);
    expect(validateSeasons([{ ...s1, endsAt: s1.startsAt }], []).join()).toMatch(/after startsAt/);
  });

  it('parses JSON config and ships with no seasons', () => {
    expect(parseSeasons(undefined)).toEqual([]);
    const parsed = parseSeasons(
      JSON.stringify([{ id: 's', name: 'S', startsAt: '10', endsAt: '20', competitions: ['referrals'] }]),
    );
    expect(parsed[0]).toMatchObject({ startsAt: 10n, endsAt: 20n, qualificationGraceSeconds: 0n, prizes: [] });
    expect(() => parseSeasons('[{"id":"s","name":"S","startsAt":"1.5","endsAt":"2","competitions":[]}]')).toThrow();
  });

  it('season leaderboard counts only referrals bound inside the season and never resets Miner state', () => {
    const q = parseQualification('5', '24')!;
    const inside = Object.freeze({
      owner: 'a',
      referrer: WALLET_1,
      createdAt: s1.startsAt + DAY,
      rigs: 5n,
      lifetimeRigsBought: 5n,
      lifetimeRigsCompounded: 0n,
    });
    const outside = Object.freeze({ ...inside, owner: 'b', createdAt: s1.endsAt + 1n });
    const before = JSON.stringify([inside, outside], (_, v) => (typeof v === 'bigint' ? v.toString() : v));
    const lb = seasonLeaderboard([inside, outside], s1, NOW, { qualification: q });
    expect(lb.status === 'open' && lb.rows[0]).toMatchObject({ referrer: WALLET_1, total: 1, qualified: 1 });
    // All-time standings still include both — the season ending removed nothing.
    const all = referralLeaderboard([inside, outside], { qualification: q, at: NOW });
    expect(all.status === 'open' && all.rows[0]!.total).toBe(2);
    expect(JSON.stringify([inside, outside], (_, v) => (typeof v === 'bigint' ? v.toString() : v))).toBe(before);
  });

  it('a season without the referrals competition has no referral board', () => {
    expect(seasonLeaderboard([], { ...s1, competitions: ['newPlanters'] }, NOW, { qualification: null })).toEqual({
      status: 'notACompetition',
    });
  });
});

describe('event decoding', () => {
  const PROGRAM = 'C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt';

  it('discriminators are sha256("event:<Name>")[..8]', () => {
    for (const [name, disc] of Object.entries(EVENT_DISCRIMINATOR)) {
      const want = createHash('sha256').update(`event:${name}`).digest().subarray(0, 8);
      expect(Buffer.from(disc).equals(want)).toBe(true);
    }
  });

  function rewardsFundedBytes(): Uint8Array {
    const parts = [
      EVENT_DISCRIMINATOR.RewardsFunded,
      getAddressEncoder().encode(WALLET_2),
      getU64Encoder().encode(5_000_000_000n),
      getU64Encoder().encode(86_400n),
      getU64Encoder().encode(57_870n),
      getI64Encoder().encode(NOW + DAY),
      getU64Encoder().encode(5_000_000_000n),
      getU64Encoder().encode(5_000_000_000n),
    ];
    const out = new Uint8Array(parts.reduce((s, p) => s + p.length, 0));
    let o = 0;
    for (const p of parts) {
      out.set(p, o);
      o += p.length;
    }
    return out;
  }

  it('decodes RewardsFunded and ReferralBound', () => {
    expect(decodeEvent(rewardsFundedBytes())).toMatchObject({
      name: 'RewardsFunded',
      funder: WALLET_2,
      amount: 5_000_000_000n,
      periodFinish: NOW + DAY,
    });
    const rb = new Uint8Array(8 + 32 + 32 + 2);
    rb.set(EVENT_DISCRIMINATOR.ReferralBound, 0);
    rb.set(getAddressEncoder().encode(WALLET_1), 8);
    rb.set(getAddressEncoder().encode(WALLET_2), 40);
    rb.set(getU16Encoder().encode(105), 72);
    expect(decodeEvent(rb)).toEqual({ name: 'ReferralBound', miner: WALLET_1, referrer: WALLET_2, weight: 105 });
    expect(decodeEvent(new Uint8Array(16))).toBeNull();
    expect(decodeEvent(EVENT_DISCRIMINATOR.ReferralBound)).toBeNull(); // truncated body
  });

  it('reads only our program’s Program data lines', () => {
    const data = getBase64Decoder().decode(rewardsFundedBytes());
    const logs = [
      `Program ${PROGRAM} invoke [1]`,
      'Program log: Instruction: FundRewards',
      `Program data: ${data}`,
      `Program ${PROGRAM} consumed 1000 of 200000 compute units`,
      `Program ${PROGRAM} success`,
      'Program Other1111111111111111111111111111111111 invoke [1]',
      `Program data: ${data}`,
      'Program Other1111111111111111111111111111111111 success',
      'Program data: !!!notbase64',
    ];
    const evs = eventsFromLogs(logs, PROGRAM);
    expect(evs).toHaveLength(1);
    expect(evs[0]!.name).toBe('RewardsFunded');
  });
});
