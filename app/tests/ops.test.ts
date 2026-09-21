/**
 * The devnet operations layer.
 *
 * These scripts hold the only code in the repository that loads a keypair
 * from disk and the only guards standing between an operator and mainnet, so
 * both are tested rather than trusted.
 */
import { unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { AccountRole, address, createKeyPairSignerFromBytes } from '@solana/kit';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import {
  IX_DISCRIMINATOR,
  PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
} from '../src/lib/program/constants';
import type { ProgramAddresses } from '../src/lib/program/pdas';
import {
  fundRewardsInstruction,
  initializeInstruction,
} from '../scripts/ops/admin-instructions';
import { loadSigner, requireDevnet, requireEnv, sol } from '../scripts/ops/common';

const PDAS: ProgramAddresses = {
  config: address('11111111111111111111111111111112'),
  capitalVault: address('11111111111111111111111111111113'),
  rewardVault: address('11111111111111111111111111111114'),
  feeVault: address('11111111111111111111111111111115'),
};
const AUTHORITY = address('7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU');
const FUNDER = address('So11111111111111111111111111111111111111112');

// ------------------------------------------------------------- cluster guard

describe('requireDevnet', () => {
  const saved = { ...process.env };
  afterEach(() => {
    process.env = { ...saved };
  });

  const set = (cluster?: string, rpc?: string) => {
    delete process.env.SOLANA_CLUSTER;
    delete process.env.SOLANA_RPC_URL;
    if (cluster !== undefined) process.env.SOLANA_CLUSTER = cluster;
    if (rpc !== undefined) process.env.SOLANA_RPC_URL = rpc;
  };

  it('refuses mainnet outright, however it is spelled', () => {
    for (const c of ['mainnet', 'mainnet-beta', 'mainnet_beta']) {
      set(c);
      expect(() => requireDevnet(), c).toThrow(/MAINNET IS REFUSED/);
    }
  });

  it('refuses a devnet label pointed at a mainnet RPC', () => {
    set('devnet', 'https://api.mainnet-beta.solana.com');
    expect(() => requireDevnet()).toThrow(/looks like mainnet/);
  });

  it('refuses any other cluster', () => {
    for (const c of ['testnet', 'localnet', 'prod', '']) {
      set(c);
      expect(() => requireDevnet(), c).toThrow();
    }
  });

  it('refuses to guess when the cluster is unset', () => {
    set(undefined);
    expect(() => requireDevnet()).toThrow(/not set/);
  });

  it('accepts devnet', () => {
    set('devnet', 'https://api.devnet.solana.com');
    expect(requireDevnet()).toEqual({
      cluster: 'devnet',
      rpcUrl: 'https://api.devnet.solana.com',
    });
  });

  it('accepts a local validator', () => {
    set('devnet', 'http://127.0.0.1:8899');
    expect(requireDevnet().rpcUrl).toBe('http://127.0.0.1:8899');
  });
});

describe('requireEnv', () => {
  it('names the missing variable and how to set it', () => {
    delete process.env.CHERRY_TREE_NOT_SET;
    expect(() => requireEnv('CHERRY_TREE_NOT_SET', 'Set it to something.')).toThrow(
      /CHERRY_TREE_NOT_SET is not set\. Set it to something\./,
    );
  });
});

// ------------------------------------------------------------------ wallets

describe('loadSigner', () => {
  const path = join(tmpdir(), `cherry-tree-ops-test-${process.pid}.json`);
  const badPath = join(tmpdir(), `cherry-tree-ops-bad-${process.pid}.json`);
  let bytes: number[];

  beforeEach(async () => {
    const kp = (await crypto.subtle.generateKey('Ed25519', true, [
      'sign',
      'verify',
    ])) as CryptoKeyPair;
    const pkcs8 = new Uint8Array(await crypto.subtle.exportKey('pkcs8', kp.privateKey));
    const raw = new Uint8Array(await crypto.subtle.exportKey('raw', kp.publicKey));
    bytes = [...pkcs8.slice(-32), ...raw];
    writeFileSync(path, JSON.stringify(bytes));
  });

  afterEach(() => {
    for (const p of [path, badPath]) {
      try {
        unlinkSync(p);
      } catch {
        /* already gone */
      }
    }
  });

  it('loads a Solana CLI keypair and resolves the right address', async () => {
    const fromFile = await loadSigner(path, 'test');
    const direct = await createKeyPairSignerFromBytes(Uint8Array.from(bytes));
    expect(fromFile.address).toBe(direct.address);
  });

  it('rejects a JSON file that is not a keypair', async () => {
    writeFileSync(badPath, JSON.stringify({ not: 'a keypair' }));
    await expect(loadSigner(badPath, 'test')).rejects.toThrow(/not a Solana keypair/);
  });

  it('rejects an array of the wrong length', async () => {
    writeFileSync(badPath, JSON.stringify([1, 2, 3]));
    await expect(loadSigner(badPath, 'test')).rejects.toThrow(/not a Solana keypair/);
  });

  it('reports a missing file by path rather than throwing something opaque', async () => {
    await expect(loadSigner('/tmp/definitely-not-here.json', 'authority')).rejects.toThrow(
      /could not read the authority keypair/,
    );
  });
});

// ------------------------------------------------------- admin instructions

describe('initialize instruction', () => {
  const ix = initializeInstruction(AUTHORITY, PDAS, {
    rigPriceLamports: 10_000_000n,
    purchaseFeeBps: 300,
    rewardFunder: FUNDER,
  });

  it('targets the program', () => {
    expect(ix.programAddress).toBe(PROGRAM_ID);
  });

  it('orders accounts as the Initialize struct declares them', () => {
    expect(ix.accounts!.map((a) => a.address)).toEqual([
      AUTHORITY,
      PDAS.config,
      PDAS.capitalVault,
      PDAS.rewardVault,
      PDAS.feeVault,
      SYSTEM_PROGRAM_ID,
    ]);
    expect(ix.accounts![0]!.role).toBe(AccountRole.WRITABLE_SIGNER);
  });

  it('encodes u64 price, u16 bps and a 32-byte pubkey after the discriminator', () => {
    const data = ix.data!;
    expect(Array.from(data.slice(0, 8))).toEqual(Array.from(IX_DISCRIMINATOR.initialize));
    // 10_000_000 little-endian u64
    expect(Array.from(data.slice(8, 16))).toEqual([128, 150, 152, 0, 0, 0, 0, 0]);
    // 300 little-endian u16
    expect(Array.from(data.slice(16, 18))).toEqual([44, 1]);
    expect(data.length).toBe(8 + 8 + 2 + 32);
  });
});

describe('fund_rewards instruction', () => {
  const ix = fundRewardsInstruction(FUNDER, PDAS, 500_000_000n, 3_600n);

  it('makes the funder the only signer and the payer', () => {
    const signers = ix.accounts!.filter(
      (a) =>
        a.role === AccountRole.WRITABLE_SIGNER || a.role === AccountRole.READONLY_SIGNER,
    );
    expect(signers.map((s) => s.address)).toEqual([FUNDER]);
  });

  it('writes to the reward vault and touches no other vault', () => {
    const addrs = ix.accounts!.map((a) => a.address);
    expect(addrs).toEqual([FUNDER, PDAS.config, PDAS.rewardVault, SYSTEM_PROGRAM_ID]);
    expect(addrs).not.toContain(PDAS.capitalVault);
    expect(addrs).not.toContain(PDAS.feeVault);
  });

  it('encodes amount and duration as little-endian u64s', () => {
    const data = ix.data!;
    expect(Array.from(data.slice(0, 8))).toEqual(Array.from(IX_DISCRIMINATOR.fundRewards));
    expect(Array.from(data.slice(8, 16))).toEqual([0, 101, 205, 29, 0, 0, 0, 0]);
    expect(Array.from(data.slice(16, 24))).toEqual([16, 14, 0, 0, 0, 0, 0, 0]);
    expect(data.length).toBe(24);
  });
});

// ---------------------------------------------------------------- reporting

describe('lamport rendering', () => {
  it('is exact integer arithmetic, not float', () => {
    expect(sol(0n)).toBe('0.000000000');
    expect(sol(1n)).toBe('0.000000001');
    expect(sol(1_000_000_000n)).toBe('1.000000000');
    expect(sol(18_291_000n)).toBe('0.018291000');
    // Past the point where Number would lose precision.
    expect(sol(9_007_199_254_740_993n)).toBe('9007199.254740993');
  });
});
