/**
 * Anchor event decoding from transaction logs — read-only indexing.
 *
 * `emit!` writes `Program data: <base64(discriminator ‖ borsh)>`. The
 * discriminator is `sha256("event:<Name>")[..8]`; `tests/events.test.ts`
 * recomputes each one, so a renamed event fails the suite. Layouts mirror
 * `programs/sol-rigs/src/events.rs` field for field.
 *
 * Nothing here trusts the log for money: these rows feed history views
 * (refill history, referral binding times). Balances and positions always
 * come from account state.
 */
import {
  getAddressDecoder,
  getBase64Encoder,
  getI64Decoder,
  getStructDecoder,
  getU16Decoder,
  getU64Decoder,
  type Address,
} from '@solana/kit';

function fromHex(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

export const EVENT_DISCRIMINATOR = {
  RewardsFunded: fromHex('54e9f5cbe493a55c'),
  ReferralBound: fromHex('0c1c989437d266be'),
  RigsPurchased: fromHex('20b3aae7f7f3026c'),
  Claim: fromHex('856209ee85cfbf71'),
  Compound: fromHex('a884a027a408ced1'),
  LaunchActivated: fromHex('b8d0a32ced6518a5'),
} as const;

export type RewardsFundedEvent = {
  name: 'RewardsFunded';
  funder: Address;
  amount: bigint;
  duration: bigint;
  newRewardRate: bigint;
  periodFinish: bigint;
  rewardVaultBalance: bigint;
  outstandingLiabilities: bigint;
};

export type ReferralBoundEvent = {
  name: 'ReferralBound';
  miner: Address;
  referrer: Address;
  weight: number;
};

export type RigsPurchasedEvent = {
  name: 'RigsPurchased';
  owner: Address;
  count: bigint;
  capitalLamports: bigint;
  feeLamports: bigint;
  totalLamports: bigint;
  userRigs: bigint;
  totalRigs: bigint;
};

export type LaunchActivatedEvent = {
  name: 'LaunchActivated';
  founder: Address;
  launchT0: bigint;
  rigs: bigint;
  totalLamports: bigint;
};

export type ClaimEvent = {
  name: 'Claim';
  owner: Address;
  amount: bigint;
  gross: bigint;
  penalty: bigint;
  penaltyBps: number;
  userRigs: bigint;
  totalRewardClaimed: bigint;
};

export type CompoundEvent = {
  name: 'Compound';
  owner: Address;
  rigsMinted: bigint;
  capitalLamports: bigint;
  feeLamports: bigint;
  totalLamports: bigint;
  remainderLamports: bigint;
  userRigs: bigint;
  totalRigs: bigint;
};

export type ProgramEvent =
  | RewardsFundedEvent
  | ReferralBoundEvent
  | RigsPurchasedEvent
  | ClaimEvent
  | CompoundEvent
  | LaunchActivatedEvent;

const decoders = {
  RewardsFunded: getStructDecoder([
    ['funder', getAddressDecoder()],
    ['amount', getU64Decoder()],
    ['duration', getU64Decoder()],
    ['newRewardRate', getU64Decoder()],
    ['periodFinish', getI64Decoder()],
    ['rewardVaultBalance', getU64Decoder()],
    ['outstandingLiabilities', getU64Decoder()],
  ]),
  ReferralBound: getStructDecoder([
    ['miner', getAddressDecoder()],
    ['referrer', getAddressDecoder()],
    ['weight', getU16Decoder()],
  ]),
  RigsPurchased: getStructDecoder([
    ['owner', getAddressDecoder()],
    ['count', getU64Decoder()],
    ['capitalLamports', getU64Decoder()],
    ['feeLamports', getU64Decoder()],
    ['totalLamports', getU64Decoder()],
    ['userRigs', getU64Decoder()],
    ['totalRigs', getU64Decoder()],
  ]),
  Claim: getStructDecoder([
    ['owner', getAddressDecoder()],
    ['amount', getU64Decoder()],
    ['gross', getU64Decoder()],
    ['penalty', getU64Decoder()],
    ['penaltyBps', getU16Decoder()],
    ['userRigs', getU64Decoder()],
    ['totalRewardClaimed', getU64Decoder()],
  ]),
  Compound: getStructDecoder([
    ['owner', getAddressDecoder()],
    ['rigsMinted', getU64Decoder()],
    ['capitalLamports', getU64Decoder()],
    ['feeLamports', getU64Decoder()],
    ['totalLamports', getU64Decoder()],
    ['remainderLamports', getU64Decoder()],
    ['userRigs', getU64Decoder()],
    ['totalRigs', getU64Decoder()],
  ]),
  LaunchActivated: getStructDecoder([
    ['founder', getAddressDecoder()],
    ['launchT0', getI64Decoder()],
    ['rigs', getU64Decoder()],
    ['totalLamports', getU64Decoder()],
  ]),
} as const;

function startsWith(data: Uint8Array, prefix: Uint8Array): boolean {
  if (data.length < prefix.length) return false;
  for (let i = 0; i < prefix.length; i++) if (data[i] !== prefix[i]) return false;
  return true;
}

/** Decode one event payload (discriminator included). Unknown → null. */
export function decodeEvent(data: Uint8Array): ProgramEvent | null {
  for (const name of Object.keys(EVENT_DISCRIMINATOR) as (keyof typeof EVENT_DISCRIMINATOR)[]) {
    if (!startsWith(data, EVENT_DISCRIMINATOR[name])) continue;
    try {
      const body = decoders[name].decode(data.subarray(8));
      return { name, ...body } as ProgramEvent;
    } catch {
      return null;
    }
  }
  return null;
}

const PROGRAM_DATA = 'Program data: ';

/**
 * Every recognised event in a transaction's log messages, in order. Only
 * lines emitted while `programId` is the executing program are considered,
 * so a different program's `Program data:` line cannot be mistaken for ours.
 */
export function eventsFromLogs(logs: readonly string[], programId: string): ProgramEvent[] {
  const out: ProgramEvent[] = [];
  const stack: string[] = [];
  for (const line of logs) {
    const invoke = /^Program (\S+) invoke \[\d+\]$/.exec(line);
    if (invoke) {
      stack.push(invoke[1]!);
      continue;
    }
    if (/^Program (\S+) (success|failed)/.test(line)) {
      stack.pop();
      continue;
    }
    if (!line.startsWith(PROGRAM_DATA) || stack[stack.length - 1] !== programId) continue;
    let bytes: Uint8Array;
    try {
      bytes = new Uint8Array(getBase64Encoder().encode(line.slice(PROGRAM_DATA.length).trim()));
    } catch {
      continue;
    }
    const ev = decodeEvent(bytes);
    if (ev) out.push(ev);
  }
  return out;
}
