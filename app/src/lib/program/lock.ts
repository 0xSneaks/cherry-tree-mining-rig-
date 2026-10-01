/**
 * Lock Season client (architecture §8; Pass #7 P7-B / P7-D).
 *
 * PDAs, instruction builders and the award formula for the single 5-day
 * lock. Account order and writability mirror the `#[derive(Accounts)]`
 * structs in `programs/sol-rigs/src/instructions/fruit_lock.rs`; every
 * builder is annotated with the struct it came from. Discriminators are
 * pinned by `programs/sol-rigs/tests/discriminators.rs`.
 */
import {
  AccountRole,
  getAddressEncoder,
  getI64Encoder,
  getProgramDerivedAddress,
  getU32Encoder,
  getU64Encoder,
  type Address,
  type Instruction,
} from '@solana/kit';

import { IX_DISCRIMINATOR, PROGRAM_ID, SEEDS, SYSTEM_PROGRAM_ID } from './constants';

/** `LedgerKind::Lock`. */
export const LOCK_KIND = 2;
/** The only lock duration: 5 days (§8). */
export const LOCK_DURATION_SECONDS = 432_000n;
/** `t_s = FRUIT_T0 + 6 days`. */
export const LOCK_SEASON_OFFSET_SECONDS = 518_400n;
/** `Kmax`: the largest lock weight, 2.5 SOL-equivalent. */
export const LOCK_KMAX_LAMPORTS = 2_500_000_000n;
/** `Hfloor * 5 days`, lamport-days. */
export const LOCK_H_FLOOR_LAMPORT_DAYS = 62_500_000_000n;
/** Instruction batch size for weights / awards (`MAX_LOCK_BATCH`). */
export const MAX_LOCK_BATCH = 24;
/** Protocol Treasury share of forfeited lock bonus at close (80%). */
export const LOCK_FORFEIT_TREASURY_BPS = 8_000n;

/** Product disclosure for the lock (§8, §7a item 7–8). */
export const LOCK_DISCLOSURE =
  'Lock: one 5-day lock per Season, registered before the Season starts. It commits your eligible Fruit weight for a conditional bonus; it never locks your seeds, your external assets or rewards you have already earned. The bonus vests only at the end of the 5 days (cliff). If your eligible weight drops during the lock, the bonus for the lost part is forfeited — there is no fee and no decay, and nothing else is touched. Forfeited bonus is split 80% to the Protocol Treasury and 20% kept for the next Season. You can cancel only before the Season starts, and you cannot register again after cancelling.';

/** Shown while the loyalty tenure threshold is unset (§6). */
export const LOYALTY_INACTIVE_NOTICE =
  'Loyalty bonus: not active yet. It starts only after the minimum qualifying weight is set; until then no loyalty bonus is funded or paid.';

/** `t_s` for a Season whose public opening is `fruitT0`. */
export function lockSeasonStart(fruitT0: bigint): bigint {
  return fruitT0 + LOCK_SEASON_OFFSET_SECONDS;
}

/** Registration and cancellation are open on `[FRUIT_T0, t_s)`. */
export function lockRegistrationOpen(fruitT0: bigint, now: bigint): boolean {
  return fruitT0 <= now && now < lockSeasonStart(fruitT0);
}

/** The Season's lock pool id is its `FRUIT_T0` (one lock pool per Season). */
export function lockPoolId(fruitT0: bigint): bigint {
  if (fruitT0 <= 0n) throw new Error('FRUIT_T0 is not set');
  return fruitT0;
}

/** `H_lock = max(5 sum ell, 62.5 SOL-days)`. */
export function lockH(sumEll: bigint): bigint {
  const u = sumEll * 5n;
  return u > LOCK_H_FLOOR_LAMPORT_DAYS ? u : LOCK_H_FLOOR_LAMPORT_DAYS;
}

/** `A_i = floor(B * 5 ell_i / H_lock)` — the on-chain award. */
export function lockAward(budget: bigint, ell: bigint, hLock: bigint): bigint {
  if (ell === 0n) return 0n;
  if (ell < 0n || ell > LOCK_KMAX_LAMPORTS || hLock < LOCK_H_FLOOR_LAMPORT_DAYS) {
    throw new RangeError('lock award input out of range');
  }
  return (budget * ell * 5n) / hLock;
}

/** Vested at maturity: `floor(A_i * ell_surv / ell_i)`. */
export function lockVested(award: bigint, ellSurv: bigint, ell: bigint): bigint {
  if (ell <= 0n || ellSurv < 0n || ellSurv > ell) throw new RangeError('invalid lock weights');
  return (award * ellSurv) / ell;
}

// ---------------------------------------------------------------- PDAs

const u64le = (v: bigint) => getU64Encoder().encode(v);

async function pda(seeds: Uint8Array[]): Promise<Address> {
  const [a] = await getProgramDerivedAddress({ programAddress: PROGRAM_ID, seeds });
  return a;
}

export interface LockAddresses {
  fruitConfig: Address;
  ledger: Address;
  vault: Address;
  pool: Address;
  season: Address;
}

/** Every fixed address of the Season's lock pool. */
export async function lockAddresses(fruitT0: bigint): Promise<LockAddresses> {
  const kind = new Uint8Array([LOCK_KIND]);
  const pool = await pda([SEEDS.fruitPool, kind, u64le(lockPoolId(fruitT0)) as Uint8Array]);
  return {
    fruitConfig: await pda([SEEDS.fruitConfig]),
    ledger: await pda([SEEDS.fruitLedger, kind]),
    vault: await pda([SEEDS.fruitVault, kind]),
    pool,
    season: await pda([SEEDS.lockSeason, getAddressEncoder().encode(pool) as Uint8Array]),
  };
}

/** `["lock_reg", pool, wallet]`. */
export async function lockRegistrationPda(pool: Address, wallet: Address): Promise<Address> {
  const enc = getAddressEncoder();
  return pda([SEEDS.lockReg, enc.encode(pool) as Uint8Array, enc.encode(wallet) as Uint8Array]);
}

/** `["fruit_claim", pool, wallet]`. */
export async function lockClaimRecordPda(pool: Address, wallet: Address): Promise<Address> {
  const enc = getAddressEncoder();
  return pda([SEEDS.fruitClaim, enc.encode(pool) as Uint8Array, enc.encode(wallet) as Uint8Array]);
}

/** `["fruit_root", pool, version]`. */
export async function lockRootPda(pool: Address, version: bigint): Promise<Address> {
  return pda([SEEDS.fruitRoot, getAddressEncoder().encode(pool) as Uint8Array, u64le(version) as Uint8Array]);
}

// ---------------------------------------------------------------- builders

function ix(discriminator: Uint8Array, accounts: Instruction['accounts'], args?: Uint8Array): Instruction {
  const data = new Uint8Array(discriminator.length + (args?.length ?? 0));
  data.set(discriminator, 0);
  if (args) data.set(args, discriminator.length);
  return { programAddress: PROGRAM_ID, accounts, data };
}

function concat(parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

const W = AccountRole.WRITABLE;
const R = AccountRole.READONLY;

/** `register_lock(requested)` — `RegisterLock`. */
export function registerLockInstruction(
  wallet: Address,
  a: LockAddresses,
  registration: Address,
  requested: bigint,
): Instruction {
  if (requested <= 0n || requested > LOCK_KMAX_LAMPORTS) {
    throw new RangeError('requested lock weight must be in (0, 2.5 SOL]');
  }
  return ix(
    IX_DISCRIMINATOR.registerLock,
    [
      { address: wallet, role: AccountRole.WRITABLE_SIGNER },
      { address: a.fruitConfig, role: R },
      { address: a.pool, role: W },
      { address: a.season, role: W },
      { address: registration, role: W },
      { address: SYSTEM_PROGRAM_ID, role: R },
    ],
    u64le(requested) as Uint8Array,
  );
}

/** `cancel_lock()` — `CancelLock`. Only before `t_s`; no re-registration. */
export function cancelLockInstruction(
  wallet: Address,
  a: LockAddresses,
  registration: Address,
): Instruction {
  return ix(IX_DISCRIMINATOR.cancelLock, [
    { address: wallet, role: AccountRole.READONLY_SIGNER },
    { address: a.fruitConfig, role: R },
    { address: a.pool, role: W },
    { address: a.season, role: W },
    { address: registration, role: W },
  ]);
}

/** `claim_lock(cumulative, proof)` — `ClaimLock`. */
export function claimLockInstruction(
  wallet: Address,
  a: LockAddresses,
  registration: Address,
  claimRecord: Address,
  cumulative: bigint,
  proof: readonly Uint8Array[],
): Instruction {
  for (const p of proof) if (p.length !== 32) throw new RangeError('proof nodes are 32 bytes');
  return ix(
    IX_DISCRIMINATOR.claimLock,
    [
      { address: wallet, role: AccountRole.WRITABLE_SIGNER },
      { address: a.pool, role: W },
      { address: a.season, role: R },
      { address: registration, role: R },
      { address: a.ledger, role: W },
      { address: a.vault, role: W },
      { address: claimRecord, role: W },
      { address: SYSTEM_PROGRAM_ID, role: R },
    ],
    concat([
      u64le(cumulative) as Uint8Array,
      getU32Encoder().encode(proof.length) as Uint8Array,
      ...proof,
    ]),
  );
}

function tallyStep(
  d: Uint8Array,
  a: LockAddresses,
  registrations: readonly Address[] = [],
): Instruction {
  if (registrations.length > MAX_LOCK_BATCH) throw new RangeError('lock batch too large');
  return ix(d, [
    { address: a.fruitConfig, role: R },
    { address: a.pool, role: W },
    { address: a.season, role: W },
    ...registrations.map((address) => ({ address, role: W })),
  ]);
}

/** `seal_lock_weights()` — permissionless (`LockTallyStep`). */
export const sealLockWeightsInstruction = (a: LockAddresses) =>
  tallyStep(IX_DISCRIMINATOR.sealLockWeights, a);
/** `award_lock_registrations()` — permissionless, batched. */
export function awardLockRegistrationsInstruction(
  a: LockAddresses,
  registrations: readonly Address[],
): Instruction {
  if (registrations.length === 0) throw new RangeError('empty lock batch');
  return tallyStep(IX_DISCRIMINATOR.awardLockRegistrations, a, registrations);
}
/** `seal_lock_awards()` — permissionless. */
export const sealLockAwardsInstruction = (a: LockAddresses) =>
  tallyStep(IX_DISCRIMINATOR.sealLockAwards, a);
/** `finalize_lock_allocation()` — permissionless after the window. */
export const finalizeLockAllocationInstruction = (a: LockAddresses) =>
  tallyStep(IX_DISCRIMINATOR.finalizeLockAllocation, a);

/** `post_lock_weights(weights)` — root poster only (`PostLockWeights`). */
export function postLockWeightsInstruction(
  poster: Address,
  a: LockAddresses,
  entries: readonly { registration: Address; ell: bigint }[],
): Instruction {
  if (entries.length === 0 || entries.length > MAX_LOCK_BATCH) {
    throw new RangeError('lock batch must hold 1..24 registrations');
  }
  return ix(
    IX_DISCRIMINATOR.postLockWeights,
    [
      { address: poster, role: AccountRole.READONLY_SIGNER },
      { address: a.fruitConfig, role: R },
      { address: a.pool, role: W },
      { address: a.season, role: W },
      ...entries.map((e) => ({ address: e.registration, role: W })),
    ],
    concat([
      getU32Encoder().encode(entries.length) as Uint8Array,
      ...entries.map((e) => u64le(e.ell) as Uint8Array),
    ]),
  );
}

/** `post_lock_root(...)` — root poster only (`PostLockRoot`). */
export function postLockRootInstruction(
  poster: Address,
  a: LockAddresses,
  root: Address,
  args: {
    version: bigint;
    merkleRoot: Uint8Array;
    checkpointTime: bigint;
    total: bigint;
    complete: boolean;
    forfeited: bigint;
  },
): Instruction {
  if (args.merkleRoot.length !== 32) throw new RangeError('merkle root is 32 bytes');
  return ix(
    IX_DISCRIMINATOR.postLockRoot,
    [
      { address: poster, role: AccountRole.WRITABLE_SIGNER },
      { address: a.fruitConfig, role: W },
      { address: a.pool, role: W },
      { address: a.season, role: R },
      { address: root, role: W },
      { address: SYSTEM_PROGRAM_ID, role: R },
    ],
    concat([
      u64le(args.version) as Uint8Array,
      args.merkleRoot,
      getI64Encoder().encode(args.checkpointTime) as Uint8Array,
      u64le(args.total) as Uint8Array,
      new Uint8Array([args.complete ? 1 : 0]),
      u64le(args.forfeited) as Uint8Array,
    ]),
  );
}

/** `finalize_lock_root()` — permissionless (`FinalizeLockRoot`). */
export function finalizeLockRootInstruction(a: LockAddresses, root: Address): Instruction {
  return ix(IX_DISCRIMINATOR.finalizeLockRoot, [
    { address: a.fruitConfig, role: W },
    { address: a.pool, role: W },
    { address: a.season, role: R },
    { address: root, role: W },
    { address: a.ledger, role: W },
    { address: a.vault, role: R },
  ]);
}

/** `close_lock_pool()` — permissionless (`CloseLockPool`). */
export function closeLockPoolInstruction(
  a: LockAddresses,
  config: Address,
  treasury: Address,
): Instruction {
  return ix(IX_DISCRIMINATOR.closeLockPool, [
    { address: a.fruitConfig, role: R },
    { address: a.pool, role: W },
    { address: a.season, role: R },
    { address: a.ledger, role: W },
    { address: a.vault, role: W },
    { address: config, role: R },
    { address: treasury, role: W },
  ]);
}

/**
 * The complete tally of one generation as ordered instructions: weights in
 * batches of at most 24 (root poster), then the permissionless seal, award
 * batches and award seal. `entries` must hold every active registration
 * exactly once with its `t_s` weight `ell = min(requested, w(t_s))`
 * (`eligibility-engine::lock_snapshot`); the program refuses anything else.
 * With no active registration it is the documented zero-active path: two
 * seals only.
 */
export function planLockTally(
  poster: Address,
  a: LockAddresses,
  entries: readonly { registration: Address; ell: bigint }[],
): Instruction[] {
  const seen = new Set<string>();
  for (const e of entries) {
    if (seen.has(e.registration)) throw new Error('duplicate lock registration');
    seen.add(e.registration);
    if (e.ell < 0n || e.ell > LOCK_KMAX_LAMPORTS) throw new RangeError('ell out of range');
  }
  const chunks = <T>(xs: readonly T[]) =>
    Array.from({ length: Math.ceil(xs.length / MAX_LOCK_BATCH) }, (_, i) =>
      xs.slice(i * MAX_LOCK_BATCH, (i + 1) * MAX_LOCK_BATCH),
    );
  return [
    ...chunks(entries).map((c) => postLockWeightsInstruction(poster, a, c)),
    sealLockWeightsInstruction(a),
    ...chunks(entries.map((e) => e.registration)).map((c) =>
      awardLockRegistrationsInstruction(a, c),
    ),
    sealLockAwardsInstruction(a),
  ];
}
