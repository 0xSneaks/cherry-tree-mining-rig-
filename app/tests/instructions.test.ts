/**
 * Instruction encoding.
 *
 * Account order and writability have to match the `#[derive(Accounts)]`
 * structs byte for byte. The reward vault's read-only role in `buy_rigs` is
 * the core invariant made structural (spec §3), so it is asserted here as
 * well as in the program's own tests.
 */
import { AccountRole, address } from '@solana/kit';
import { describe, expect, it } from 'vitest';

import {
  IX_DISCRIMINATOR,
  PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
} from '../src/lib/program/constants';
import {
  buyRigsInstruction,
  claimInstruction,
  compoundInstruction,
} from '../src/lib/program/instructions';
import { skrBindingPda, type ProgramAddresses } from '../src/lib/program/pdas';

const OWNER = address('7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU');
const MINER = address('So11111111111111111111111111111111111111112');

const PDAS: ProgramAddresses = {
  config: address('11111111111111111111111111111112'),
  capitalVault: address('11111111111111111111111111111113'),
  rewardVault: address('11111111111111111111111111111114'),
  feeVault: address('11111111111111111111111111111115'),
};

describe('buy_rigs', () => {
  const ix = buyRigsInstruction(OWNER, MINER, PDAS, 10n);

  it('targets the program', () => {
    expect(ix.programAddress).toBe(PROGRAM_ID);
  });

  it('carries the discriminator, a little-endian u64 count and Option::None', () => {
    expect(Array.from(ix.data!.slice(0, 8))).toEqual(
      Array.from(IX_DISCRIMINATOR.buyRigs),
    );
    expect(Array.from(ix.data!.slice(8))).toEqual([10, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect(ix.data!.length).toBe(17);
  });

  it('encodes a referrer as Option::Some and appends its miner account', () => {
    const REF = address('9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM');
    const REF_MINER = address('11111111111111111111111111111116');
    // A wrapped / tokenized domain proof is three accounts; the referrer's
    // miner always comes last.
    const ID = [
      address('11111111111111111111111111111117'),
      address('11111111111111111111111111111118'),
      address('11111111111111111111111111111119'),
    ];
    const r = buyRigsInstruction(OWNER, MINER, PDAS, 1n, {
      identity: ID,
      referrer: { wallet: REF, miner: REF_MINER },
    });
    expect(r.data!.length).toBe(8 + 8 + 1 + 32);
    expect(r.data![16]).toBe(1);
    expect(r.accounts!.slice(7).map((a) => a.address)).toEqual([...ID, REF_MINER]);
    expect(r.accounts!.slice(7).every((a) => a.role === AccountRole.READONLY)).toBe(true);
  });

  it('puts a writable .skr binding between identity and referrer', () => {
    const REF = address('9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM');
    const REF_MINER = address('11111111111111111111111111111116');
    const NAME = address('11111111111111111111111111111117');
    const BIND = address('3X5vjVDFZJNy9Yc9jMrbWNpXN3U6crbTwSzfyHeihrVH');
    const r = buyRigsInstruction(OWNER, MINER, PDAS, 1n, {
      identity: [NAME],
      skrBinding: BIND,
      referrer: { wallet: REF, miner: REF_MINER },
    });
    expect(r.accounts!.slice(7).map((a) => a.address)).toEqual([NAME, BIND, REF_MINER]);
    expect(r.accounts!.slice(7).map((a) => a.role)).toEqual([
      AccountRole.READONLY,
      AccountRole.WRITABLE,
      AccountRole.READONLY,
    ]);
  });

  it('derives the binding PDA exactly as the program does', async () => {
    // Pinned in programs/sol-rigs/tests/skr_binding.rs for name [7u8; 32].
    const name = address('US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx');
    expect(await skrBindingPda(name)).toBe('3X5vjVDFZJNy9Yc9jMrbWNpXN3U6crbTwSzfyHeihrVH');
  });

  it('orders accounts exactly as the Accounts struct declares them', () => {
    expect(ix.accounts!.map((a) => a.address)).toEqual([
      OWNER,
      PDAS.config,
      MINER,
      PDAS.capitalVault,
      PDAS.feeVault,
      PDAS.rewardVault,
      SYSTEM_PROGRAM_ID,
    ]);
  });

  it('passes the reward vault READ-ONLY so a purchase cannot reach it', () => {
    const rewardVault = ix.accounts!.find((a) => a.address === PDAS.rewardVault);
    expect(rewardVault?.role).toBe(AccountRole.READONLY);
  });

  it('makes only the payer a signer', () => {
    const signers = ix.accounts!.filter(
      (a) =>
        a.role === AccountRole.WRITABLE_SIGNER || a.role === AccountRole.READONLY_SIGNER,
    );
    expect(signers.map((s) => s.address)).toEqual([OWNER]);
  });
});

describe('claim', () => {
  const ix = claimInstruction(OWNER, MINER, PDAS);

  it('carries only the discriminator — it takes no arguments', () => {
    expect(Array.from(ix.data!)).toEqual(Array.from(IX_DISCRIMINATOR.claim));
  });

  it('touches the reward vault and nothing else that holds money', () => {
    expect(ix.accounts!.map((a) => a.address)).toEqual([
      OWNER,
      PDAS.config,
      MINER,
      PDAS.rewardVault,
    ]);
    expect(ix.accounts!.map((a) => a.address)).not.toContain(PDAS.capitalVault);
  });
});

describe('compound', () => {
  const ix = compoundInstruction(OWNER, MINER, PDAS);

  it('carries only the discriminator', () => {
    expect(Array.from(ix.data!)).toEqual(Array.from(IX_DISCRIMINATOR.compound));
  });

  it('writes to all three vaults, because lamports genuinely move between them', () => {
    const writable = ix
      .accounts!.filter((a) => a.role === AccountRole.WRITABLE)
      .map((a) => a.address);
    expect(writable).toContain(PDAS.rewardVault);
    expect(writable).toContain(PDAS.capitalVault);
    expect(writable).toContain(PDAS.feeVault);
  });
});

describe('discriminators', () => {
  it('are eight bytes each and mutually distinct', () => {
    const all = Object.values(IX_DISCRIMINATOR);
    for (const d of all) expect(d.length).toBe(8);
    const hex = all.map((d) => Buffer.from(d).toString('hex'));
    expect(new Set(hex).size).toBe(all.length);
  });
});
