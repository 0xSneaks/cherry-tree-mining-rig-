/**
 * The first-buy preflight fails closed: a fully valid input passes, and
 * breaking any single precondition fails it. Also pins the identity constants
 * the client uses to the ones the program enforces.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { address } from '@solana/kit';
import { describe, expect, it } from 'vitest';

import type { Config } from '../src/lib/program/accounts';
import { buyRigsInstruction } from '../src/lib/program/instructions';
import * as id from '../src/lib/program/identity';
import { EXPECTED_FEE_RECIPIENT, EXPECTED_FOUNDER } from '../src/lib/launch/expected';
import {
  confirmationPhrase,
  evaluatePreflight,
  type PreflightInput,
} from '../src/lib/launch/preflight';
import type { ProgramAddresses } from '../src/lib/program/pdas';

const PDAS: ProgramAddresses = {
  config: address('11111111111111111111111111111112'),
  capitalVault: address('11111111111111111111111111111113'),
  rewardVault: address('11111111111111111111111111111114'),
  feeVault: address('11111111111111111111111111111115'),
};
const MINER = address('11111111111111111111111111111116');
const NOW = 1_790_000_000n;
const SOL = 1_000_000_000n;

function config(over: Partial<Config> = {}): Config {
  return {
    authority: 'x' as never,
    pendingAuthority: 'x' as never,
    rewardFunder: 'x' as never,
    feeRecipient: EXPECTED_FEE_RECIPIENT,
    paused: false,
    rigPriceLamports: 10_000_000n,
    purchaseFeeBps: 300,
    totalRigs: 0n,
    rewardRate: 11_574n,
    rewardPeriodFinish: NOT_LATER(),
    rewardLastUpdate: NOW - 10n,
    rewardPerRigStored: 0n,
    undistributedDust: 0n,
    totalRewardFunded: 30n * SOL,
    totalRewardClaimed: 0n,
    totalRewardCompounded: 0n,
    totalCapitalReceived: 0n,
    totalProtocolFees: 0n,
    totalFeesWithdrawn: 0n,
    minerCount: 0n,
    capitalVaultBump: 1,
    rewardVaultBump: 1,
    feeVaultBump: 1,
    version: 2,
    bump: 1,
    founder: EXPECTED_FOUNDER,
    launchT0: 0n,
    totalShares: 0n,
    totalClaimPenalties: 0n,
    genesisCapRigs: 2_500n,
    ...over,
  };
}
function NOT_LATER() {
  return NOW + 30n * 86_400n;
}

function input(over: Partial<PreflightInput> = {}): PreflightInput {
  return {
    intendedCluster: 'mainnet-beta',
    actualCluster: 'mainnet-beta',
    programAccount: { executable: true },
    config: config(),
    pdas: PDAS,
    founderWallet: EXPECTED_FOUNDER,
    founderMinerExists: false,
    rewardVaultDistributable: 30n * SOL,
    chainTime: NOW,
    founderLamports: 2n * SOL,
    rigCount: 10n,
    networkFeeLamports: 5_000n,
    minerRentLamports: 2_000_000n,
    instruction: buyRigsInstruction(EXPECTED_FOUNDER, MINER, PDAS, 10n),
    simulation: { ok: true, error: null, logs: [] },
    ...over,
  };
}

describe('first-buy preflight', () => {
  it('passes when everything is exactly right', () => {
    const r = evaluatePreflight(input(), MINER);
    expect(r.checks.filter((c) => !c.pass)).toEqual([]);
    expect(r.ok).toBe(true);
    expect(r.quote!.feeLamports).toBe(3_000_000n);
    expect(r.aprBpsAfter! < r.aprBpsNow!).toBe(true);
  });

  const breaks: Array<[string, Partial<PreflightInput>]> = [
    ['cluster not stated', { intendedCluster: null }],
    ['cluster mismatch', { intendedCluster: 'mainnet-beta', actualCluster: 'devnet' }],
    ['devnet rehearsal', { intendedCluster: 'devnet', actualCluster: 'devnet' }],
    ['program missing', { programAccount: null }],
    ['wrong wallet', { founderWallet: address('9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM') }],
    ['not initialised', { config: null }],
    ['already activated', { config: config({ launchT0: NOW - 5n }) }],
    ['paused', { config: config({ paused: true }) }],
    ['wrong payout wallet', { config: config({ feeRecipient: address('11111111111111111111111111111117') }) }],
    ['different founder pinned', { config: config({ founder: address('11111111111111111111111111111117') }) }],
    ['unfunded', { config: config({ totalRewardFunded: 0n }), rewardVaultDistributable: 0n }],
    ['schedule ended', { config: config({ rewardPeriodFinish: NOW }) }],
    ['insolvent', { rewardVaultDistributable: 1n }],
    ['amount not supplied', { rigCount: null }],
    ['activation above cap', { rigCount: 2_501n, instruction: buyRigsInstruction(EXPECTED_FOUNDER, MINER, PDAS, 2_501n) }],
    ['insufficient SOL', { founderLamports: 1_000n }],
    ['no instruction', { instruction: null }],
    [
      'instruction carries a referrer',
      {
        instruction: buyRigsInstruction(EXPECTED_FOUNDER, MINER, PDAS, 10n, {
          referrer: { wallet: EXPECTED_FEE_RECIPIENT, miner: MINER },
        }),
      },
    ],
    [
      'instruction carries extra accounts',
      {
        instruction: buyRigsInstruction(EXPECTED_FOUNDER, MINER, PDAS, 10n, {
          identity: [address('11111111111111111111111111111117')],
        }),
      },
    ],
    ['simulation failed', { simulation: { ok: false, error: 'custom 6023', logs: [] } }],
    ['not simulated', { simulation: null }],
  ];
  for (const [label, over] of breaks) {
    it(`fails closed: ${label}`, () => {
      expect(evaluatePreflight(input(over), MINER).ok).toBe(false);
    });
  }

  it('requires the exact typed confirmation for the chosen amount', () => {
    expect(confirmationPhrase(10n)).toBe('ACTIVATE 10');
  });
});

describe('identity constants match the program', () => {
  const rust = readFileSync(
    join(import.meta.dirname, '..', '..', 'programs', 'sol-rigs', 'src', 'constants.rs'),
    'utf8',
  );
  const pinned = (name: string) =>
    rust.match(new RegExp(`${name}: Pubkey =\\s*pubkey!\\("(\\w+)"\\)`))?.[1];

  it('pins the same .skr (AllDomains) and .sns (SNS) registry accounts', () => {
    for (const name of [
      'ANS_PROGRAM_ID',
      'SKR_TLD_PARENT',
      'ANS_NAME_HOUSE_PROGRAM_ID',
      'SKR_NAME_HOUSE',
      'SNS_NAME_PROGRAM_ID',
      'SNS_TLD_ROOT',
      'SNS_NAME_TOKENIZER_ID',
      'SPL_TOKEN_PROGRAM_ID',
    ] as const) {
      expect(pinned(name), name).toBe(id[name]);
    }
  });

  it('carries no founder name anywhere in the program', () => {
    expect(rust).not.toMatch(/FOUNDER_NAME/);
  });
});

describe('read-only launch tooling', () => {
  it('can never sign or send', () => {
    const root = join(import.meta.dirname, '..');
    for (const f of [
      'scripts/ops/first-buy-preflight.ts',
      'scripts/ops/verify-first-buy.ts',
      'src/lib/launch/gather.ts',
      'src/lib/launch/preflight.ts',
      'src/lib/launch/verify.ts',
    ]) {
      const text = readFileSync(join(root, f), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
      for (const banned of [
        'sendTransaction',
        'signAndSend',
        'sendAndConfirm',
        'Keypair',
        'loadKeypair',
        'createKeyPair',
        'signTransaction',
      ]) {
        expect(text, `${f} uses ${banned}`).not.toContain(banned);
      }
    }
  });
});
