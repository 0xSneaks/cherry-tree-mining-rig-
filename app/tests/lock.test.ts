/**
 * Lock Season client (architecture §8; Pass #7): discriminators, account
 * order, argument encoding, PDAs, the award formula and the disclosures.
 * Account order is asserted against `programs/sol-rigs/src/instructions/
 * fruit_lock.rs`; award values against the Rust engine formula.
 */
import { createHash } from 'node:crypto';

import { AccountRole, address } from '@solana/kit';
import { describe, expect, it } from 'vitest';

import {
  ACCOUNT_DISCRIMINATOR,
  IX_DISCRIMINATOR,
  PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
} from '../src/lib/program/constants';
import {
  LOCK_DISCLOSURE,
  LOYALTY_INACTIVE_NOTICE,
  awardLockRegistrationsInstruction,
  cancelLockInstruction,
  claimLockInstruction,
  closeLockPoolInstruction,
  lockAddresses,
  lockAward,
  lockH,
  lockPoolId,
  lockRegistrationOpen,
  lockRegistrationPda,
  lockSeasonStart,
  lockVested,
  postLockRootInstruction,
  postLockWeightsInstruction,
  registerLockInstruction,
  sealLockWeightsInstruction,
  type LockAddresses,
} from '../src/lib/program/lock';

const WALLET = address('7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU');
const REG = address('So11111111111111111111111111111111111111112');
const A: LockAddresses = {
  fruitConfig: address('11111111111111111111111111111112'),
  ledger: address('11111111111111111111111111111113'),
  vault: address('11111111111111111111111111111114'),
  pool: address('11111111111111111111111111111115'),
  season: address('11111111111111111111111111111116'),
};
const FRUIT_T0 = 1_800_086_400n;

const anchor = (s: string) => createHash('sha256').update(s).digest().subarray(0, 8);
const snake = (s: string) => s.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);

describe('lock discriminators', () => {
  const lockIxs = Object.entries(IX_DISCRIMINATOR).filter(([k]) => /Lock/.test(k));

  it('are Anchor sha256("global:<name>") for every lock instruction', () => {
    expect(lockIxs.length).toBe(14);
    for (const [name, d] of lockIxs) {
      expect(Buffer.from(d).toString('hex'), name).toBe(
        anchor(`global:${snake(name)}`).toString('hex'),
      );
    }
  });

  it('match the lock account types', () => {
    expect(Buffer.from(ACCOUNT_DISCRIMINATOR.lockSeason)).toEqual(anchor('account:LockSeason'));
    expect(Buffer.from(ACCOUNT_DISCRIMINATOR.lockRegistration)).toEqual(
      anchor('account:LockRegistration'),
    );
  });
});

describe('timing and ids', () => {
  it('derives t_s and the half-open registration window', () => {
    const ts = lockSeasonStart(FRUIT_T0);
    expect(ts).toBe(FRUIT_T0 + 518_400n);
    expect(lockRegistrationOpen(FRUIT_T0, FRUIT_T0 - 1n)).toBe(false);
    expect(lockRegistrationOpen(FRUIT_T0, FRUIT_T0)).toBe(true);
    expect(lockRegistrationOpen(FRUIT_T0, ts - 1n)).toBe(true);
    expect(lockRegistrationOpen(FRUIT_T0, ts)).toBe(false);
  });

  it('binds the pool id to FRUIT_T0 and refuses an unset T0', () => {
    expect(lockPoolId(FRUIT_T0)).toBe(FRUIT_T0);
    expect(() => lockPoolId(0n)).toThrow();
  });

  it('derives deterministic, distinct PDAs', async () => {
    const a = await lockAddresses(FRUIT_T0);
    const b = await lockAddresses(FRUIT_T0);
    expect(a).toEqual(b);
    expect(new Set(Object.values(a)).size).toBe(5);
    const other = await lockAddresses(FRUIT_T0 + 1n);
    expect(other.pool).not.toBe(a.pool);
    const r1 = await lockRegistrationPda(a.pool, WALLET);
    const r2 = await lockRegistrationPda(other.pool, WALLET);
    expect(r1).not.toBe(r2);
  });
});

describe('award formula (parity with rigs_engine::fruit_lock)', () => {
  const B = 428_082_192n;
  it('matches engine-computed values below and above the floor', () => {
    const ells = [2_500_000_000n, 1_000_000_007n, 700_000_000n];
    const h = lockH(ells.reduce((x, y) => x + y));
    expect(h).toBe(62_500_000_000n);
    expect(ells.map((e) => lockAward(B, e, h))).toEqual([85_616_438n, 34_246_575n, 23_972_602n]);
    const h2 = lockH(20_000_000_000n);
    expect(h2).toBe(100_000_000_000n);
    expect(lockAward(B, 2_500_000_000n, h2)).toBe(53_510_274n);
    expect(lockAward(B, 0n, h)).toBe(0n);
    expect(() => lockAward(B, 2_500_000_001n, h)).toThrow();
  });

  it('vests the surviving share, floored', () => {
    expect(lockVested(1_000n, 1n, 3n)).toBe(333n);
    expect(() => lockVested(1_000n, 4n, 3n)).toThrow();
  });
});

describe('instruction encoding', () => {
  it('register_lock: accounts and u64 arg', () => {
    const ix = registerLockInstruction(WALLET, A, REG, 2_500_000_000n);
    expect(ix.programAddress).toBe(PROGRAM_ID);
    expect(ix.accounts).toEqual([
      { address: WALLET, role: AccountRole.WRITABLE_SIGNER },
      { address: A.fruitConfig, role: AccountRole.READONLY },
      { address: A.pool, role: AccountRole.WRITABLE },
      { address: A.season, role: AccountRole.WRITABLE },
      { address: REG, role: AccountRole.WRITABLE },
      { address: SYSTEM_PROGRAM_ID, role: AccountRole.READONLY },
    ]);
    expect(Buffer.from(ix.data!.subarray(8)).readBigUInt64LE()).toBe(2_500_000_000n);
    expect(() => registerLockInstruction(WALLET, A, REG, 0n)).toThrow();
    expect(() => registerLockInstruction(WALLET, A, REG, 2_500_000_001n)).toThrow();
  });

  it('cancel_lock: the wallet signs, nothing is paid', () => {
    const ix = cancelLockInstruction(WALLET, A, REG);
    expect(ix.accounts![0]).toEqual({ address: WALLET, role: AccountRole.READONLY_SIGNER });
    expect(ix.data!.length).toBe(8);
  });

  it('claim_lock: registration is read-only, proof is a Borsh Vec', () => {
    const node = new Uint8Array(32).fill(7);
    const ix = claimLockInstruction(WALLET, A, REG, REG, 5n, [node, node]);
    expect(ix.accounts!.map((a) => a.role)).toEqual([
      AccountRole.WRITABLE_SIGNER,
      AccountRole.WRITABLE,
      AccountRole.READONLY,
      AccountRole.READONLY,
      AccountRole.WRITABLE,
      AccountRole.WRITABLE,
      AccountRole.WRITABLE,
      AccountRole.READONLY,
    ]);
    const args = Buffer.from(ix.data!.subarray(8));
    expect(args.readBigUInt64LE(0)).toBe(5n);
    expect(args.readUInt32LE(8)).toBe(2);
    expect(args.length).toBe(8 + 4 + 64);
  });

  it('post_lock_weights: one writable registration per weight, batch <= 24', () => {
    const entries = [
      { registration: REG, ell: 1n },
      { registration: WALLET, ell: 2n },
    ];
    const ix = postLockWeightsInstruction(WALLET, A, entries);
    expect(ix.accounts!.slice(4)).toEqual([
      { address: REG, role: AccountRole.WRITABLE },
      { address: WALLET, role: AccountRole.WRITABLE },
    ]);
    const args = Buffer.from(ix.data!.subarray(8));
    expect(args.readUInt32LE(0)).toBe(2);
    expect(args.readBigUInt64LE(4)).toBe(1n);
    expect(args.readBigUInt64LE(12)).toBe(2n);
    const big = Array.from({ length: 25 }, () => ({ registration: REG, ell: 1n }));
    expect(() => postLockWeightsInstruction(WALLET, A, big)).toThrow();
    expect(() => postLockWeightsInstruction(WALLET, A, [])).toThrow();
  });

  it('tally steps and close carry no arguments', () => {
    expect(sealLockWeightsInstruction(A).data!.length).toBe(8);
    expect(awardLockRegistrationsInstruction(A, [REG]).accounts!.length).toBe(4);
    const close = closeLockPoolInstruction(A, REG, WALLET);
    expect(close.accounts!.at(-1)).toEqual({ address: WALLET, role: AccountRole.WRITABLE });
  });

  it('post_lock_root: Borsh args in program order', () => {
    const ix = postLockRootInstruction(WALLET, A, REG, {
      version: 3n,
      merkleRoot: new Uint8Array(32).fill(1),
      checkpointTime: -1n,
      total: 9n,
      complete: true,
      forfeited: 2n,
    });
    const args = Buffer.from(ix.data!.subarray(8));
    expect(args.length).toBe(8 + 32 + 8 + 8 + 1 + 8);
    expect(args.readBigUInt64LE(0)).toBe(3n);
    expect(args.readBigInt64LE(40)).toBe(-1n);
    expect(args.readBigUInt64LE(48)).toBe(9n);
    expect(args[56]).toBe(1);
    expect(args.readBigUInt64LE(57)).toBe(2n);
  });
});

describe('disclosures', () => {
  it('state the cliff, no fee, cancel window and 80/20 forfeit', () => {
    for (const s of ['5-day', 'cliff', 'no fee', '80%', '20%', 'before the Season starts']) {
      expect(LOCK_DISCLOSURE).toContain(s);
    }
    expect(LOYALTY_INACTIVE_NOTICE).toContain('not active');
  });
});

describe('planLockTally', () => {
  it('batches weights and awards by 24 between the two seals', async () => {
    const { planLockTally, lockRootPda } = await import('../src/lib/program/lock');
    // 50 distinct valid addresses (derived PDAs stand in for registrations).
    const regs = await Promise.all(
      Array.from({ length: 50 }, (_, i) => lockRootPda(A.pool, BigInt(i))),
    );
    const plan = planLockTally(WALLET, A, regs.map((registration) => ({ registration, ell: 1n })));
    const names = plan.map((ix) => {
      const hex = Buffer.from(ix.data!.subarray(0, 8)).toString('hex');
      return Object.entries(IX_DISCRIMINATOR).find(
        ([, d]) => Buffer.from(d).toString('hex') === hex,
      )![0];
    });
    expect(names).toEqual([
      'postLockWeights',
      'postLockWeights',
      'postLockWeights',
      'sealLockWeights',
      'awardLockRegistrations',
      'awardLockRegistrations',
      'awardLockRegistrations',
      'sealLockAwards',
    ]);
    expect(planLockTally(WALLET, A, []).length).toBe(2);
    expect(() =>
      planLockTally(WALLET, A, [
        { registration: REG, ell: 1n },
        { registration: REG, ell: 1n },
      ]),
    ).toThrow();
  });
});
