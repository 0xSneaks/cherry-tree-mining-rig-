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
import {
  AccountRole,
  address,
  createKeyPairSignerFromBytes,
  getAddressEncoder,
} from '@solana/kit';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import {
  IX_DISCRIMINATOR,
  PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
} from '../src/lib/program/constants';
import { programDataPda, type ProgramAddresses } from '../src/lib/program/pdas';
import {
  fundRewardsInstruction,
  initializeInstruction,
  migrateV2Instruction,
  setFeeRecipientInstruction,
  withdrawFeesInstruction,
} from '../scripts/ops/admin-instructions';
import {
  GENESIS_HASH,
  assertGenesisIsDevnet,
  loadSigner,
  requireDevnet,
  requireEnv,
  sol,
} from '../scripts/ops/common';

const PDAS: ProgramAddresses = {
  config: address('11111111111111111111111111111112'),
  capitalVault: address('11111111111111111111111111111113'),
  rewardVault: address('11111111111111111111111111111114'),
  feeVault: address('11111111111111111111111111111115'),
};
const AUTHORITY = address('7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU');
const FUNDER = address('So11111111111111111111111111111111111111112');
/** THE CHERRY TREE protocol fee recipient. Public configuration; never signs. */
const FEE_RECIPIENT = address('4FsYyXTbF1iftkUTtZVmzmvisxUaGvoYfZDj4DuGLvwF');
const FOUNDER = address('FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp');
const PROGRAM_DATA = address('11111111111111111111111111111116');

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

describe('migrate_v2 instruction', () => {
  const ix = migrateV2Instruction(AUTHORITY, PDAS.config, 2_500n);

  it('uses raw migration account order and appends the cap', () => {
    expect(ix.accounts!.map((a) => a.address)).toEqual([
      AUTHORITY,
      PDAS.config,
      SYSTEM_PROGRAM_ID,
    ]);
    expect(ix.accounts![0]!.role).toBe(AccountRole.WRITABLE_SIGNER);
    expect(ix.data!.length).toBe(16);
    expect(Array.from(ix.data!.slice(0, 8))).toEqual(Array.from(IX_DISCRIMINATOR.migrateV2));
    expect(new DataView(ix.data!.buffer, ix.data!.byteOffset + 8, 8).getBigUint64(0, true)).toBe(2_500n);
  });
});

/**
 * `requireDevnet` trusts the operator's label. This is the check that does
 * not: the chain states its own identity and anything but devnet is refused.
 */
describe('assertGenesisIsDevnet', () => {
  const RPC = 'https://rpc.example.com';

  it('accepts the real devnet genesis hash', () => {
    expect(() => assertGenesisIsDevnet(GENESIS_HASH.devnet, RPC)).not.toThrow();
  });

  it('refuses mainnet even when the URL says nothing about it', () => {
    expect(() => assertGenesisIsDevnet(GENESIS_HASH.mainnet, RPC)).toThrow(
      /MAINNET IS REFUSED/,
    );
  });

  it('refuses testnet', () => {
    expect(() => assertGenesisIsDevnet(GENESIS_HASH.testnet, RPC)).toThrow(/testnet/);
  });

  it('refuses a cluster it cannot identify', () => {
    expect(() => assertGenesisIsDevnet('SomeOtherGenesisHash11111111111111111111111', RPC))
      .toThrow(/not devnet/);
  });

  it('allows a local validator, which has a fresh genesis every run', () => {
    for (const url of ['http://127.0.0.1:8899', 'http://localhost:8899']) {
      expect(() =>
        assertGenesisIsDevnet('FreshLocalGenesisHash1111111111111111111111', url),
      ).not.toThrow();
    }
  });

  it('refuses mainnet even from a local URL — value beats convenience', () => {
    expect(() => assertGenesisIsDevnet(GENESIS_HASH.mainnet, 'http://127.0.0.1:8899'))
      .toThrow(/MAINNET IS REFUSED/);
  });

  it('pins the three well-known genesis hashes', () => {
    expect(GENESIS_HASH.devnet).toBe('EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG');
    expect(GENESIS_HASH.mainnet).toBe('5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d');
    expect(GENESIS_HASH.testnet).toBe('4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY');
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

describe('program data address', () => {
  it('is the loader PDA the program checks for initialize', async () => {
    // Same literal is pinned in programs/sol-rigs/tests/initialize_security.rs.
    expect(await programDataPda()).toBe('3W8FkxmT9RmES9Duj8jjp2BR9htCNqCZtrvnYLFsETVT');
  });
});

describe('initialize instruction', () => {
  const ix = initializeInstruction(AUTHORITY, PDAS, PROGRAM_DATA, {
    rigPriceLamports: 10_000_000n,
    purchaseFeeBps: 300,
    rewardFunder: FUNDER,
    feeRecipient: FEE_RECIPIENT,
    founder: FOUNDER,
    genesisCapRigs: 2_500n,
    compoundFeeBps: 250,
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
      PROGRAM_DATA,
    ]);
    expect(ix.accounts![0]!.role).toBe(AccountRole.WRITABLE_SIGNER);
    // The upgrade-authority binding account is read, never written.
    expect(ix.accounts![6]!.role).toBe(AccountRole.READONLY);
  });

  it('encodes initialize args with the cap and then the §7a compound fee appended', () => {
    const data = ix.data!;
    expect(Array.from(data.slice(0, 8))).toEqual(Array.from(IX_DISCRIMINATOR.initialize));
    // 10_000_000 little-endian u64
    expect(Array.from(data.slice(8, 16))).toEqual([128, 150, 152, 0, 0, 0, 0, 0]);
    // 300 little-endian u16
    expect(Array.from(data.slice(16, 18))).toEqual([44, 1]);
    // reward_funder, fee_recipient, founder, genesis_cap_rigs, compound_fee_bps
    expect(data.length).toBe(8 + 8 + 2 + 32 + 32 + 32 + 8 + 2);
    expect(new DataView(data.buffer, data.byteOffset + data.length - 10, 8).getBigUint64(0, true)).toBe(2_500n);
    expect(new DataView(data.buffer, data.byteOffset + data.length - 2, 2).getUint16(0, true)).toBe(250);
  });

  it('carries the fee recipient after the reward funder', () => {
    const data = ix.data!;
    const encode = (a: string) =>
      Array.from(new Uint8Array(getAddressEncoder().encode(address(a))));
    expect(Array.from(data.slice(18, 50))).toEqual(encode(FUNDER));
    expect(Array.from(data.slice(50, 82))).toEqual(encode(FEE_RECIPIENT));
  });

  it('does not pass the fee recipient as an account — it never signs', () => {
    expect(ix.accounts!.map((a) => a.address)).not.toContain(FEE_RECIPIENT);
  });
});

// --------------------------------------------------------- fee recipient

describe('set_fee_recipient instruction', () => {
  const ix = setFeeRecipientInstruction(AUTHORITY, PDAS, FEE_RECIPIENT);

  it('makes the authority the only signer', () => {
    const signers = ix.accounts!.filter(
      (a) =>
        a.role === AccountRole.WRITABLE_SIGNER || a.role === AccountRole.READONLY_SIGNER,
    );
    expect(signers.map((s) => s.address)).toEqual([AUTHORITY]);
  });

  it('touches config and nothing else — no vault is reachable from here', () => {
    const addrs = ix.accounts!.map((a) => a.address);
    expect(addrs).toEqual([AUTHORITY, PDAS.config]);
    expect(addrs).not.toContain(PDAS.capitalVault);
    expect(addrs).not.toContain(PDAS.rewardVault);
    expect(addrs).not.toContain(PDAS.feeVault);
  });

  it('passes the new recipient as an argument, so its key is never needed', () => {
    const data = ix.data!;
    expect(Array.from(data.slice(0, 8))).toEqual(
      Array.from(IX_DISCRIMINATOR.setFeeRecipient),
    );
    expect(Array.from(data.slice(8, 40))).toEqual(
      Array.from(new Uint8Array(getAddressEncoder().encode(FEE_RECIPIENT))),
    );
    expect(data.length).toBe(40);
    expect(ix.accounts!.map((a) => a.address)).not.toContain(FEE_RECIPIENT);
  });
});

describe('withdraw_fees instruction', () => {
  const ix = withdrawFeesInstruction(AUTHORITY, PDAS, FEE_RECIPIENT, 1_000_000n);

  it('moves money out of the FEE vault only', () => {
    const addrs = ix.accounts!.map((a) => a.address);
    expect(addrs).toEqual([AUTHORITY, PDAS.config, PDAS.feeVault, FEE_RECIPIENT]);
    expect(addrs).not.toContain(PDAS.rewardVault);
    expect(addrs).not.toContain(PDAS.capitalVault);
  });

  it('pays the configured recipient, and the recipient does not sign', () => {
    const recipient = ix.accounts!.find((a) => a.address === FEE_RECIPIENT)!;
    expect(recipient.role).toBe(AccountRole.WRITABLE);
    const signers = ix.accounts!.filter(
      (a) =>
        a.role === AccountRole.WRITABLE_SIGNER || a.role === AccountRole.READONLY_SIGNER,
    );
    expect(signers.map((s) => s.address)).toEqual([AUTHORITY]);
  });

  it('encodes the amount as a little-endian u64', () => {
    const data = ix.data!;
    expect(Array.from(data.slice(0, 8))).toEqual(Array.from(IX_DISCRIMINATOR.withdrawFees));
    expect(Array.from(data.slice(8, 16))).toEqual([64, 66, 15, 0, 0, 0, 0, 0]);
    expect(data.length).toBe(16);
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
