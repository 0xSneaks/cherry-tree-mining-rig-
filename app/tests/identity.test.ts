/**
 * `.skr` / `.sns` discovery against an in-memory RPC that implements exactly
 * the calls the app makes (`getProgramAccounts` with memcmp / dataSize
 * filters, `getTokenAccountsByOwner`, `getAccountInfo`). Every path — direct
 * and wrapped `.skr`, direct and tokenized `.sns` — and every way ownership
 * can fail to be established, which must yield `none` (fail closed).
 */
import {
  address,
  getAddressDecoder,
  getAddressEncoder,
  getBase58Encoder,
  type Address,
} from '@solana/kit';
import { describe, expect, it } from 'vitest';

import type { RigsRpc } from '../src/lib/chain';
import {
  ANS_NAME_HOUSE_PROGRAM_ID,
  ANS_PROGRAM_ID,
  SKR_TLD_PARENT,
  SNS_NAME_PROGRAM_ID,
  SNS_NAME_TOKENIZER_ID,
  SNS_TLD_ROOT,
  SPL_TOKEN_PROGRAM_ID,
  discoverIdentity,
} from '../src/lib/program/identity';

const enc = getAddressEncoder();
const NOW = 1_790_000_000n;
// Obvious fakes: valid base58 32-byte keys, no real accounts.
const WALLET = address('7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU');
const OTHER = address('US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx');
/** A distinct fake 32-byte key per `n`. */
const k = (n: number): Address => {
  const bytes = new Uint8Array(32).fill(n);
  bytes[0] = 200;
  return getAddressDecoder().decode(bytes);
};

type Acc = { pubkey: Address; owner: Address; data: Uint8Array };

function put(d: Uint8Array, off: number, key: Address) {
  d.set(enc.encode(key), off);
}
function putU64(d: Uint8Array, off: number, v: bigint) {
  new DataView(d.buffer).setBigUint64(off, v, true);
}

const ANS_DISC = [68, 72, 88, 44, 15, 167, 103, 243];

function skrRecord(
  pubkey: Address,
  owner: Address,
  expires = 0n,
  parent: Address = SKR_TLD_PARENT,
): Acc {
  const d = new Uint8Array(200);
  d.set(ANS_DISC, 0);
  put(d, 8, parent);
  put(d, 40, owner);
  putU64(d, 104, expires);
  return { pubkey, owner: ANS_PROGRAM_ID, data: d };
}
function skrNftRecord(pubkey: Address, name: Address, mint: Address, tag = 1): Acc {
  const d = new Uint8Array(138);
  d[8] = tag;
  put(d, 10, name);
  put(d, 74, mint);
  return { pubkey, owner: ANS_NAME_HOUSE_PROGRAM_ID, data: d };
}
function snsRecord(pubkey: Address, owner: Address, parent: Address = SNS_TLD_ROOT): Acc {
  const d = new Uint8Array(96);
  put(d, 0, parent);
  put(d, 32, owner);
  return { pubkey, owner: SNS_NAME_PROGRAM_ID, data: d };
}
function snsNftRecord(pubkey: Address, name: Address, mint: Address, tag = 2): Acc {
  const d = new Uint8Array(98);
  d[0] = tag;
  put(d, 2, name);
  put(d, 66, mint);
  return { pubkey, owner: SNS_NAME_TOKENIZER_ID, data: d };
}
function tokenAccount(pubkey: Address, mint: Address, owner: Address, amount = 1n, state = 1): Acc {
  const d = new Uint8Array(165);
  put(d, 0, mint);
  put(d, 32, owner);
  putU64(d, 64, amount);
  d[108] = state;
  return { pubkey, owner: SPL_TOKEN_PROGRAM_ID, data: d };
}

type Refuse = Partial<Record<'gpa' | 'tokens' | 'info', boolean>> & {
  gpaPrograms?: Address[];
};

function fakeRpc(accounts: Acc[], refuse: Refuse = {}) {
  const calls: string[] = [];
  const b64 = (d: Uint8Array) => Buffer.from(d).toString('base64');
  const wire = (a: Acc) => ({
    pubkey: a.pubkey,
    account: {
      owner: a.owner,
      data: [b64(a.data), 'base64'],
      lamports: 1n,
      executable: false,
    },
  });
  const rpc = {
    getProgramAccounts(program: Address, cfg: { filters: Array<Record<string, never>> }) {
      return {
        send: async () => {
          calls.push(`getProgramAccounts:${program}`);
          if (refuse.gpa || refuse.gpaPrograms?.includes(program))
            throw new Error('method disabled');
          return accounts
            .filter((a) => a.owner === program)
            .filter((a) =>
              cfg.filters.every((f: Record<string, unknown>) => {
                if ('dataSize' in f) return BigInt(a.data.length) === f.dataSize;
                const m = f.memcmp as { offset: bigint; bytes: string };
                const want = getBase58Encoder().encode(m.bytes);
                const at = Number(m.offset);
                return want.every((x, i) => a.data[at + i] === x);
              }),
            )
            .map(wire);
        },
      };
    },
    getTokenAccountsByOwner(owner: Address, f: { programId: Address }) {
      return {
        send: async () => {
          calls.push('getTokenAccountsByOwner');
          if (refuse.tokens) throw new Error('method disabled');
          return {
            value: accounts
              .filter((a) => a.owner === f.programId)
              .filter(
                (a) =>
                  a.data.length >= 64 && enc.encode(owner).every((x, i) => a.data[32 + i] === x),
              )
              .map(wire),
          };
        },
      };
    },
    getAccountInfo(key: Address) {
      return {
        send: async () => {
          calls.push('getAccountInfo');
          if (refuse.info) throw new Error('down');
          const a = accounts.find((x) => x.pubkey === key);
          return {
            value: a ? { owner: a.owner, data: [b64(a.data), 'base64'] } : null,
          };
        },
      };
    },
  };
  return { rpc: rpc as unknown as RigsRpc, calls };
}

const NAME = k(11);
const RECORD = k(12);
const MINT = k(13);
const TOKEN = k(14);

const discover = (accts: Acc[], refuse?: Refuse) =>
  discoverIdentity(fakeRpc(accts, refuse).rpc, WALLET, NOW);

describe('.skr discovery (AllDomains)', () => {
  it('direct: a live .skr name record owned by the wallet', async () => {
    expect(await discover([skrRecord(NAME, WALLET)])).toEqual({
      kind: 'skr',
      accounts: [NAME],
    });
    expect(await discover([skrRecord(NAME, WALLET, NOW + 1n)])).toEqual({
      kind: 'skr',
      accounts: [NAME],
    });
  });

  it('wrapped: name → Name House record → the NFT the wallet holds', async () => {
    const accts = [
      skrRecord(NAME, RECORD),
      skrNftRecord(RECORD, NAME, MINT),
      tokenAccount(TOKEN, MINT, WALLET),
    ];
    expect(await discover(accts)).toEqual({
      kind: 'skr',
      accounts: [NAME, RECORD, TOKEN],
    });
  });

  it('rejects expired, wrong-parent, wrong-program and non-wallet records', async () => {
    expect((await discover([skrRecord(NAME, WALLET, NOW)])).kind).toBe('none');
    expect((await discover([skrRecord(NAME, WALLET, 0n, OTHER)])).kind).toBe('none');
    const forged = { ...skrRecord(NAME, WALLET), owner: SNS_NAME_PROGRAM_ID };
    expect((await discover([forged])).kind).toBe('none');
    expect((await discover([skrRecord(NAME, OTHER)])).kind).toBe('none');
    const badDisc = skrRecord(NAME, WALLET);
    badDisc.data[0] = 0;
    expect((await discover([badDisc])).kind).toBe('none');
  });

  it('wrapped: rejects an inactive record, a record pointing elsewhere, or a non-held NFT', async () => {
    const base = [skrRecord(NAME, RECORD), tokenAccount(TOKEN, MINT, WALLET)];
    expect((await discover([...base, skrNftRecord(RECORD, NAME, MINT, 0)])).kind).toBe('none');
    expect(
      (
        await discover([
          skrRecord(NAME, OTHER),
          skrNftRecord(RECORD, NAME, MINT),
          tokenAccount(TOKEN, MINT, WALLET),
        ])
      ).kind,
    ).toBe('none');
    for (const tok of [
      tokenAccount(TOKEN, MINT, WALLET, 0n),
      tokenAccount(TOKEN, MINT, WALLET, 2n),
      tokenAccount(TOKEN, MINT, WALLET, 1n, 0),
    ]) {
      expect(
        (await discover([skrRecord(NAME, RECORD), skrNftRecord(RECORD, NAME, MINT), tok])).kind,
      ).toBe('none');
    }
  });
});

describe('.sns discovery (SNS)', () => {
  it('direct: a top-level .sns record owned by the wallet', async () => {
    expect(await discover([snsRecord(NAME, WALLET)])).toEqual({
      kind: 'sns',
      accounts: [NAME],
    });
  });

  it('tokenized: name → tokenizer record → the NFT the wallet holds', async () => {
    const accts = [
      snsRecord(NAME, RECORD),
      snsNftRecord(RECORD, NAME, MINT),
      tokenAccount(TOKEN, MINT, WALLET),
    ];
    expect(await discover(accts)).toEqual({
      kind: 'sns',
      accounts: [NAME, RECORD, TOKEN],
    });
  });

  it('rejects subdomains (other parent), inactive records and other owners', async () => {
    expect((await discover([snsRecord(NAME, WALLET, OTHER)])).kind).toBe('none');
    expect((await discover([snsRecord(NAME, OTHER)])).kind).toBe('none');
    expect(
      (
        await discover([
          snsRecord(NAME, RECORD),
          snsNftRecord(RECORD, NAME, MINT, 1),
          tokenAccount(TOKEN, MINT, WALLET),
        ])
      ).kind,
    ).toBe('none');
  });
});

describe('preference and fail-closed behaviour', () => {
  it('prefers .skr over .sns', async () => {
    const r = await discover([snsRecord(k(21), WALLET), skrRecord(NAME, WALLET)]);
    expect(r).toEqual({ kind: 'skr', accounts: [NAME] });
  });

  it('an RPC that refuses getProgramAccounts establishes nothing', async () => {
    expect(
      await discover([skrRecord(NAME, WALLET), snsRecord(k(21), WALLET)], {
        gpa: true,
      }),
    ).toEqual({
      kind: 'none',
      accounts: [],
    });
  });

  it('a failure on one path never hides a proof on another', async () => {
    // Token lookups refused: direct .sns is still found.
    expect((await discover([snsRecord(NAME, WALLET)], { tokens: true })).kind).toBe('sns');
    // ANS refused: direct .sns is still found, and never upgraded to .skr.
    expect(
      (
        await discover([skrRecord(k(22), WALLET), snsRecord(NAME, WALLET)], {
          gpaPrograms: [ANS_PROGRAM_ID],
        })
      ).kind,
    ).toBe('sns');
    // Name lookups down: wrapped paths establish nothing.
    const wrapped = [
      skrRecord(NAME, RECORD),
      skrNftRecord(RECORD, NAME, MINT),
      tokenAccount(TOKEN, MINT, WALLET),
    ];
    expect((await discover(wrapped, { info: true })).kind).toBe('none');
  });

  it('uses exactly the documented RPC methods, and no getProgramAccounts on the Token program', async () => {
    const { rpc, calls } = fakeRpc([]);
    await discoverIdentity(rpc, WALLET, NOW);
    expect(new Set(calls)).toEqual(
      new Set([
        `getProgramAccounts:${ANS_PROGRAM_ID}`,
        `getProgramAccounts:${SNS_NAME_PROGRAM_ID}`,
        'getTokenAccountsByOwner',
      ]),
    );
    expect(calls).not.toContain(`getProgramAccounts:${SPL_TOKEN_PROGRAM_ID}`);
  });
});

describe('client layouts equal the program', () => {
  it('reads the same offsets identity.rs enforces', async () => {
    const { readFileSync } = await import('node:fs');
    const { join } = await import('node:path');
    const rs = readFileSync(
      join(import.meta.dirname, '..', '..', 'programs', 'sol-rigs', 'src', 'identity.rs'),
      'utf8',
    );
    const ts = readFileSync(
      join(import.meta.dirname, '..', 'src', 'lib', 'program', 'identity.ts'),
      'utf8',
    );
    for (const name of [
      'ANS_PARENT',
      'ANS_OWNER',
      'ANS_EXPIRES',
      'ANS_HEADER_LEN',
      'SNS_PARENT',
      'SNS_OWNER',
      'SNS_HEADER_LEN',
    ]) {
      const r = rs.match(new RegExp(`const ${name}: usize = (\\d+);`))?.[1];
      const t = ts.match(new RegExp(`const ${name} = (\\d+);`))?.[1];
      expect(r, name).toBeDefined();
      expect(t, name).toBe(r);
    }
    const disc =
      rs.includes('[68, 72, 88, 44, 15, 167, 103, 243]') ||
      readFileSync(
        join(import.meta.dirname, '..', '..', 'programs', 'sol-rigs', 'src', 'constants.rs'),
        'utf8',
      ).includes('[68, 72, 88, 44, 15, 167, 103, 243]');
    expect(disc).toBe(true);
    expect(ts.replace(/\s+/g, '')).toContain('[68,72,88,44,15,167,103,243]');
  });
});
