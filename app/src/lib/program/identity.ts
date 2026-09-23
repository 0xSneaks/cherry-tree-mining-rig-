/**
 * Finding the name record a wallet can present as proof of `.skr` / SNS
 * ownership. Mirrors `programs/sol-rigs/src/identity.rs` and the official
 * SDK filters (`@onsol/tldparser` `findOwnedNameAccountsForUser`,
 * `@bonfida/spl-name-service` `getSnsDomainKeysForOwner`).
 *
 * Nothing here is trusted. The program re-checks the account's owner program,
 * parent and owner field against the signer; a client that passes the wrong
 * account simply gets `AccessDenied`.
 */
import { address, getAddressDecoder, type Address } from '@solana/kit';

import type { RigsRpc as Rpc } from '../chain';
import type { Identity } from './launch';

/** Pinned in `programs/sol-rigs/src/constants.rs`; equality is tested. */
export const ANS_PROGRAM_ID = address('ALTNSZ46uaAUU7XUV6awvdorLGqAsPwa9shm7h4uP2FK');
export const SKR_TLD_PARENT = address('F3A8kuikEiu6k2399oSJ1PWfcJYDHqpwoQ2e8psSDNuF');
export const FOUNDER_NAME_ACCOUNT = address('H4ZaQQaESb69EPFnMLGFde1stMPbb5s15YBnzyDEHxZH');
export const SNS_NAME_PROGRAM_ID = address('namesLPneVptA9Z5rqUDD9tMTWEJwofgaYwp8cawRkX');
export const SNS_ROOT = address('58PwtjSDuFHuUkYjH9BYnnQKHfwo9reZhC2zMJv9JPkx');

export type IdentityProof = { kind: Identity; account: Address | null };

async function firstOwned(
  rpc: Rpc,
  program: Address,
  parentOffset: number,
  parent: Address,
  ownerOffset: number,
  wallet: Address,
): Promise<Address | null> {
  const res = await rpc
    .getProgramAccounts(program, {
      encoding: 'base64',
      dataSlice: { offset: 0, length: 0 },
      filters: [
        { memcmp: { offset: BigInt(parentOffset), bytes: parent as string, encoding: 'base58' } },
        { memcmp: { offset: BigInt(ownerOffset), bytes: wallet as string, encoding: 'base58' } },
      ],
    } as never)
    .send();
  const list = res as unknown as Array<{ pubkey: Address }>;
  return list[0]?.pubkey ?? null;
}

/**
 * The strongest identity `wallet` can present: a `.skr` record first (it
 * opens every phase an SNS record does, and more), then SNS.
 */
export async function discoverIdentity(rpc: Rpc, wallet: Address): Promise<IdentityProof> {
  try {
    const skr = await firstOwned(rpc, ANS_PROGRAM_ID, 8, SKR_TLD_PARENT, 40, wallet);
    if (skr) return { kind: 'skr', account: skr };
    const sns = await firstOwned(rpc, SNS_NAME_PROGRAM_ID, 0, SNS_ROOT, 32, wallet);
    if (sns) return { kind: 'sns', account: sns };
  } catch {
    // RPCs may refuse getProgramAccounts. Fall through: the wallet is
    // treated as having no name, which only ever errs towards "not allowed".
  }
  return { kind: 'none', account: null };
}

/** ANS `NameRecordHeader` discriminator (`@onsol/tldparser`). */
const ANS_DISCRIMINATOR = [68, 72, 88, 44, 15, 167, 103, 243];

function keyAt(data: Uint8Array, offset: number): string | null {
  if (data.length < offset + 32) return null;
  return getAddressDecoder().decode(data.subarray(offset, offset + 32));
}

/**
 * Mirror of `identity::owns_founder_name`: the account at
 * FOUNDER_NAME_ACCOUNT is an ANS record under `.skr`, unexpired, whose owner
 * field is `wallet`. Used by the preflight to predict what the program will
 * decide — the program decides.
 */
export function founderNameOwnedBy(
  account: { address: Address; owner: Address; data: Uint8Array } | null,
  wallet: Address,
  now: bigint,
): { ok: boolean; reason: string; owner: string | null } {
  if (!account) return { ok: false, reason: 'sneaks.skr name record not found on chain', owner: null };
  if (account.address !== FOUNDER_NAME_ACCOUNT) {
    return { ok: false, reason: 'not the pinned sneaks.skr record', owner: null };
  }
  if (account.owner !== ANS_PROGRAM_ID) {
    return { ok: false, reason: `record not owned by ANS (owner ${account.owner})`, owner: null };
  }
  const d = account.data;
  if (d.length < 200 || ANS_DISCRIMINATOR.some((b, i) => d[i] !== b)) {
    return { ok: false, reason: 'not an ANS name record', owner: null };
  }
  const parent = keyAt(d, 8);
  const owner = keyAt(d, 40);
  const expires = new DataView(d.buffer, d.byteOffset + 104, 8).getBigUint64(0, true);
  if (parent !== SKR_TLD_PARENT) return { ok: false, reason: 'parent is not .skr', owner };
  if (expires !== 0n && now >= expires) return { ok: false, reason: 'name record expired', owner };
  if (owner !== wallet) {
    return {
      ok: false,
      reason:
        `sneaks.skr owner field is ${owner}, not ${wallet}` +
        (owner && owner !== wallet ? ' (wrapped as an NFT, or transferred)' : ''),
      owner,
    };
  }
  return { ok: true, reason: 'sneaks.skr is owned by this wallet on chain', owner };
}
