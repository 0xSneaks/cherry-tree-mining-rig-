/**
 * Finding the accounts a wallet can present as proof of `.skr` / `.sns`
 * domain ownership. Mirrors `programs/sol-rigs/src/identity.rs` and each
 * registry's official SDK:
 *
 *   .skr — AllDomains (`@onsol/tldparser`): name records under the `.skr`
 *          TLD owned by the wallet, or wrapped as a Name House NFT the
 *          wallet holds.
 *   .sns — SNS (`@bonfida/spl-name-service` v4, dev.sns.id): top-level
 *          records under the `.sns` TLD root owned by the wallet
 *          (`getSnsDomainKeysForOwner`), or tokenized by the SNS name
 *          tokenizer (`getSnsNftsForOwner`).
 *
 * RPC requirements: `getProgramAccounts` with `memcmp` filters on the ANS,
 * AllDomains Name House, SPL Name Service and SNS tokenizer programs;
 * `getTokenAccountsByOwner` (SPL Token) for held NFTs; `getAccountInfo`.
 *
 * Nothing here is trusted. The program re-checks owner programs, PDAs,
 * parents, owner fields and token balances; a client that passes the wrong
 * accounts simply gets `AccessDenied`.
 */
import { address, getAddressDecoder, getBase64Encoder, type Address } from '@solana/kit';

import type { RigsRpc as Rpc } from '../chain';
import type { Identity } from './launch';

/** Pinned in `programs/sol-rigs/src/constants.rs`; equality is tested. */
export const ANS_PROGRAM_ID = address('ALTNSZ46uaAUU7XUV6awvdorLGqAsPwa9shm7h4uP2FK');
export const SKR_TLD_PARENT = address('F3A8kuikEiu6k2399oSJ1PWfcJYDHqpwoQ2e8psSDNuF');
export const ANS_NAME_HOUSE_PROGRAM_ID = address('NH3uX6FtVE2fNREAioP7hm5RaozotZxeL6khU1EHx51');
export const SKR_NAME_HOUSE = address('CPD3hKJE51VuiDE5u5Nosx1u7MXWwKGPff8HFJTtpX4V');
export const SNS_NAME_PROGRAM_ID = address('namesLPneVptA9Z5rqUDD9tMTWEJwofgaYwp8cawRkX');
export const SNS_TLD_ROOT = address('58PwtjSDuFHuUkYjH9BYnnQKHfwo9reZhC2zMJv9JPkx');
export const SNS_NAME_TOKENIZER_ID = address('nftD3vbNkNqfj2Sd3HZwbpw4BxxKWr4AjGb9X38JeZk');
export const SPL_TOKEN_PROGRAM_ID = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

/** How many held NFTs to inspect for a wrapped / tokenized domain. */
const MAX_NFTS_CHECKED = 50;

/** `NameRecordHeader` discriminator (pinned in `constants.rs`). */
const ANS_NAME_RECORD_DISCRIMINATOR = new Uint8Array([68, 72, 88, 44, 15, 167, 103, 243]);
// Layouts mirror `programs/sol-rigs/src/identity.rs`.
const ANS_PARENT = 8;
const ANS_OWNER = 40;
const ANS_EXPIRES = 104;
const ANS_HEADER_LEN = 200;
const SNS_PARENT = 0;
const SNS_OWNER = 32;
const SNS_HEADER_LEN = 96;
const TOKEN_ACCOUNT_LEN = 165;

export type IdentityProof = { kind: Identity; accounts: Address[] };

type Filter = { offset: number; bytes: string };
type Raw = { pubkey: Address; owner: Address; data: Uint8Array };

const bytesOf = (b64: string) => new Uint8Array(getBase64Encoder().encode(b64));
const keyAt = (d: Uint8Array, o: number) => getAddressDecoder().decode(d.subarray(o, o + 32));
const u64At = (d: Uint8Array, o: number) =>
  new DataView(d.buffer, d.byteOffset + o, 8).getBigUint64(0, true);

async function programAccounts(
  rpc: Rpc,
  program: Address,
  filters: Filter[],
  dataSize?: number,
): Promise<Raw[]> {
  const res = await rpc
    .getProgramAccounts(program, {
      encoding: 'base64',
      filters: [
        ...(dataSize ? [{ dataSize: BigInt(dataSize) }] : []),
        ...filters.map((f) => ({
          memcmp: {
            offset: BigInt(f.offset),
            bytes: f.bytes,
            encoding: 'base58',
          },
        })),
      ],
    } as never)
    .send();
  const list = res as unknown as Array<{
    pubkey: Address;
    account: { owner: Address; data: [string, string] };
  }>;
  return list.map((a) => ({
    pubkey: a.pubkey,
    owner: a.account.owner,
    data: bytesOf(a.account.data[0]),
  }));
}

async function account(rpc: Rpc, key: Address): Promise<Raw | null> {
  const { value } = await rpc.getAccountInfo(key, { encoding: 'base64' }).send();
  if (!value) return null;
  return { pubkey: key, owner: value.owner, data: bytesOf(value.data[0]) };
}

// ---- the program's own ownership predicates, client side -------------------

/** A live `.skr` ANS name record; returns its header owner. */
function skrOwner(a: Raw, now: bigint): Address | null {
  if (a.owner !== ANS_PROGRAM_ID || a.data.length < ANS_HEADER_LEN) return null;
  for (let i = 0; i < 8; i++) if (a.data[i] !== ANS_NAME_RECORD_DISCRIMINATOR[i]) return null;
  const expires = u64At(a.data, ANS_EXPIRES);
  const live = expires === 0n || (now >= 0n && now < expires);
  if (!live || keyAt(a.data, ANS_PARENT) !== SKR_TLD_PARENT) return null;
  return keyAt(a.data, ANS_OWNER);
}

/** A top-level `.sns` name record; returns its header owner. */
function snsOwner(a: Raw): Address | null {
  if (a.owner !== SNS_NAME_PROGRAM_ID || a.data.length < SNS_HEADER_LEN) return null;
  if (keyAt(a.data, SNS_PARENT) !== SNS_TLD_ROOT) return null;
  return keyAt(a.data, SNS_OWNER);
}

/**
 * NFTs (amount exactly 1, initialised or frozen, SPL Token) the wallet holds.
 * `getTokenAccountsByOwner` rather than `getProgramAccounts` on the Token
 * program: public and most paid RPCs refuse the latter.
 */
async function heldNfts(
  rpc: Rpc,
  wallet: Address,
): Promise<Array<{ mint: Address; token: Address }>> {
  const { value } = await rpc
    .getTokenAccountsByOwner(wallet, { programId: SPL_TOKEN_PROGRAM_ID }, { encoding: 'base64' })
    .send();
  const out: Array<{ mint: Address; token: Address }> = [];
  for (const a of value as unknown as Array<{
    pubkey: Address;
    account: { owner: Address; data: [string, string] };
  }>) {
    const d = bytesOf(a.account.data[0]);
    if (a.account.owner !== SPL_TOKEN_PROGRAM_ID || d.length !== TOKEN_ACCOUNT_LEN) continue;
    if (keyAt(d, 32) !== wallet || u64At(d, 64) !== 1n) continue;
    if (d[108] !== 1 && d[108] !== 2) continue;
    out.push({ mint: keyAt(d, 0), token: a.pubkey });
    if (out.length >= MAX_NFTS_CHECKED) break;
  }
  return out;
}

async function directSkr(rpc: Rpc, wallet: Address, now: bigint): Promise<Address[] | null> {
  const found = await programAccounts(rpc, ANS_PROGRAM_ID, [
    { offset: ANS_PARENT, bytes: SKR_TLD_PARENT },
    { offset: ANS_OWNER, bytes: wallet },
  ]);
  const hit = found.find((a) => skrOwner(a, now) === wallet);
  return hit ? [hit.pubkey] : null;
}

async function wrappedSkr(
  rpc: Rpc,
  nfts: Array<{ mint: Address; token: Address }>,
  now: bigint,
): Promise<Address[] | null> {
  for (const { mint, token } of nfts) {
    // AllDomains NftRecord: disc(8) tag(1) bump(1) name(32) owner(32) mint(32).
    const [record] = await programAccounts(rpc, ANS_NAME_HOUSE_PROGRAM_ID, [
      { offset: 74, bytes: mint },
    ]);
    if (!record || record.data[8] !== 1) continue;
    const name = await account(rpc, keyAt(record.data, 10));
    if (name && skrOwner(name, now) === record.pubkey) return [name.pubkey, record.pubkey, token];
  }
  return null;
}

async function directSns(rpc: Rpc, wallet: Address): Promise<Address[] | null> {
  const found = await programAccounts(rpc, SNS_NAME_PROGRAM_ID, [
    { offset: SNS_PARENT, bytes: SNS_TLD_ROOT },
    { offset: SNS_OWNER, bytes: wallet },
  ]);
  const hit = found.find((a) => snsOwner(a) === wallet);
  return hit ? [hit.pubkey] : null;
}

async function tokenizedSns(
  rpc: Rpc,
  nfts: Array<{ mint: Address; token: Address }>,
): Promise<Address[] | null> {
  for (const { mint, token } of nfts) {
    // SNS tokenizer NftRecord: tag(1) nonce(1) name(32) owner(32) mint(32).
    const [record] = await programAccounts(rpc, SNS_NAME_TOKENIZER_ID, [
      { offset: 66, bytes: mint },
    ]);
    if (!record || record.data[0] !== 2) continue;
    const name = await account(rpc, keyAt(record.data, 2));
    if (name && snsOwner(name) === record.pubkey) return [name.pubkey, record.pubkey, token];
  }
  return null;
}

/** One lookup; an RPC refusal or error means "not established", never "owned". */
async function attempt<T>(f: () => Promise<T | null>): Promise<T | null> {
  try {
    return await f();
  } catch {
    return null;
  }
}

/**
 * The strongest identity `wallet` can present: `.skr` first (it opens every
 * phase `.sns` does, and more), then `.sns`; direct records before wrapped /
 * tokenized ones.
 *
 * Fails closed per path: a refused or failed lookup establishes nothing, and
 * a candidate is returned only if it passes the same checks the program
 * applies. A failure on one path never hides a proof on another. The program
 * re-verifies everything regardless.
 */
export async function discoverIdentity(
  rpc: Rpc,
  wallet: Address,
  nowSeconds: bigint = BigInt(Math.floor(Date.now() / 1000)),
): Promise<IdentityProof> {
  const skr = await attempt(() => directSkr(rpc, wallet, nowSeconds));
  if (skr) return { kind: 'skr', accounts: skr };

  const nfts = (await attempt(() => heldNfts(rpc, wallet))) ?? [];
  const wrapped = await attempt(() => wrappedSkr(rpc, nfts, nowSeconds));
  if (wrapped) return { kind: 'skr', accounts: wrapped };

  const sns = await attempt(() => directSns(rpc, wallet));
  if (sns) return { kind: 'sns', accounts: sns };

  const tokenized = await attempt(() => tokenizedSns(rpc, nfts));
  if (tokenized) return { kind: 'sns', accounts: tokenized };

  return { kind: 'none', accounts: [] };
}
