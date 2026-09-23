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

export type IdentityProof = { kind: Identity; accounts: Address[] };

type Filter = { offset: number; bytes: string };

async function programAccounts(
  rpc: Rpc,
  program: Address,
  filters: Filter[],
  dataSize?: number,
): Promise<Array<{ pubkey: Address; data: Uint8Array }>> {
  const res = await rpc
    .getProgramAccounts(program, {
      encoding: 'base64',
      filters: [
        ...(dataSize ? [{ dataSize: BigInt(dataSize) }] : []),
        ...filters.map((f) => ({
          memcmp: { offset: BigInt(f.offset), bytes: f.bytes, encoding: 'base58' },
        })),
      ],
    } as never)
    .send();
  const list = res as unknown as Array<{ pubkey: Address; account: { data: [string, string] } }>;
  return list.map((a) => ({
    pubkey: a.pubkey,
    data: new Uint8Array(getBase64Encoder().encode(a.account.data[0])),
  }));
}

const keyAt = (d: Uint8Array, o: number) => getAddressDecoder().decode(d.subarray(o, o + 32));

/** NFTs (amount 1, SPL Token) the wallet holds: mint → token account. */
async function heldNfts(rpc: Rpc, wallet: Address): Promise<Array<{ mint: Address; token: Address }>> {
  const accounts = await programAccounts(
    rpc,
    SPL_TOKEN_PROGRAM_ID,
    [{ offset: 32, bytes: wallet }],
    165,
  );
  return accounts
    .filter((a) => new DataView(a.data.buffer, a.data.byteOffset + 64, 8).getBigUint64(0, true) === 1n)
    .slice(0, MAX_NFTS_CHECKED)
    .map((a) => ({ mint: keyAt(a.data, 0), token: a.pubkey }));
}

async function wrappedSkr(rpc: Rpc, nfts: Array<{ mint: Address; token: Address }>) {
  for (const { mint, token } of nfts) {
    // AllDomains NftRecord: disc(8) tag(1) bump(1) name(32) owner(32) mint(32).
    const [record] = await programAccounts(rpc, ANS_NAME_HOUSE_PROGRAM_ID, [
      { offset: 74, bytes: mint },
    ]);
    if (!record || record.data[8] !== 1) continue;
    const name = keyAt(record.data, 10);
    const [header] = await programAccounts(rpc, ANS_PROGRAM_ID, [
      { offset: 8, bytes: SKR_TLD_PARENT },
      { offset: 40, bytes: record.pubkey },
    ]);
    if (header?.pubkey === name) return [name, record.pubkey, token];
  }
  return null;
}

async function tokenizedSns(rpc: Rpc, nfts: Array<{ mint: Address; token: Address }>) {
  for (const { mint, token } of nfts) {
    // SNS tokenizer NftRecord: tag(1) nonce(1) name(32) owner(32) mint(32).
    const [record] = await programAccounts(rpc, SNS_NAME_TOKENIZER_ID, [{ offset: 66, bytes: mint }]);
    if (!record || record.data[0] !== 2) continue;
    const name = keyAt(record.data, 2);
    const [header] = await programAccounts(rpc, SNS_NAME_PROGRAM_ID, [
      { offset: 0, bytes: SNS_TLD_ROOT },
      { offset: 32, bytes: record.pubkey },
    ]);
    if (header?.pubkey === name) return [name, record.pubkey, token];
  }
  return null;
}

/**
 * The strongest identity `wallet` can present: `.skr` first (it opens every
 * phase `.sns` does, and more), then `.sns`; direct records before wrapped /
 * tokenized ones.
 */
export async function discoverIdentity(rpc: Rpc, wallet: Address): Promise<IdentityProof> {
  try {
    const [skr] = await programAccounts(rpc, ANS_PROGRAM_ID, [
      { offset: 8, bytes: SKR_TLD_PARENT },
      { offset: 40, bytes: wallet },
    ]);
    if (skr) return { kind: 'skr', accounts: [skr.pubkey] };

    const nfts = await heldNfts(rpc, wallet);
    const wrapped = await wrappedSkr(rpc, nfts);
    if (wrapped) return { kind: 'skr', accounts: wrapped };

    const [sns] = await programAccounts(rpc, SNS_NAME_PROGRAM_ID, [
      { offset: 0, bytes: SNS_TLD_ROOT },
      { offset: 32, bytes: wallet },
    ]);
    if (sns) return { kind: 'sns', accounts: [sns.pubkey] };

    const tokenized = await tokenizedSns(rpc, nfts);
    if (tokenized) return { kind: 'sns', accounts: tokenized };
  } catch {
    // RPCs may refuse getProgramAccounts. Fall through: the wallet is shown
    // as holding no domain, which only ever errs towards "not allowed".
  }
  return { kind: 'none', accounts: [] };
}
