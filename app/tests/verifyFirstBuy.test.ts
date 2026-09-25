/**
 * Post-first-buy verification: a correct activation passes; each way the
 * transaction or the resulting state could be wrong is caught.
 */
import { address, type Address } from '@solana/kit';
import { describe, expect, it } from 'vitest';

import type { Config, Miner } from '../src/lib/program/accounts';
import { PROGRAM_ID } from '../src/lib/program/constants';
import { buyRigsInstruction } from '../src/lib/program/instructions';
import { EXPECTED_FOUNDER } from '../src/lib/launch/expected';
import {
  COMPUTE_BUDGET_PROGRAM,
  evaluateFirstBuy,
  type FirstBuyState,
  type FirstBuyTx,
} from '../src/lib/launch/verify';

const PDAS = {
  config: address('11111111111111111111111111111112'),
  capitalVault: address('11111111111111111111111111111113'),
  rewardVault: address('11111111111111111111111111111114'),
  feeVault: address('11111111111111111111111111111115'),
};
const MINER = address('11111111111111111111111111111116');
const T0 = 1_790_000_000n;
const SEEDS = 10n;
// 10 seeds at 0.01 SOL + 3%: capital 100_000_000, fee 3_000_000.
const CAPITAL = 100_000_000n;
const FEE = 3_000_000n;
const NET = 5_000n;
const RENT = 2_025_000n;

const ix = buyRigsInstruction(EXPECTED_FOUNDER, MINER, PDAS, SEEDS);

function tx(over: Partial<FirstBuyTx> = {}): FirstBuyTx {
  const keys: Address[] = [EXPECTED_FOUNDER, PDAS.config, MINER, PDAS.capitalVault, PDAS.feeVault, PDAS.rewardVault];
  return {
    signature: 'sig',
    commitment: 'finalized',
    err: null,
    blockTime: T0,
    feeLamports: NET,
    accountKeys: keys,
    signers: [EXPECTED_FOUNDER],
    instructions: [
      { programId: PROGRAM_ID, accounts: ix.accounts!.map((a) => a.address), data: ix.data! as Uint8Array },
    ],
    preBalances: [10_000_000_000n, 1n, 0n, 1_000_000n, 1_000_000n, 30_000_000_000n],
    postBalances: [
      10_000_000_000n - CAPITAL - FEE - NET - RENT,
      1n,
      RENT,
      1_000_000n + CAPITAL,
      1_000_000n + FEE,
      30_000_000_000n,
    ],
    ...over,
  };
}

function state(over: Partial<FirstBuyState> = {}, c: Partial<Config> = {}, m: Partial<Miner> = {}): FirstBuyState {
  return {
    config: {
      rigPriceLamports: 10_000_000n,
      purchaseFeeBps: 300,
      version: 2,
      genesisCapRigs: 2_500n,
      founder: EXPECTED_FOUNDER,
      launchT0: T0,
      totalRigs: SEEDS,
      totalShares: SEEDS * 100n,
      rewardRate: 11_574n,
      rewardPeriodFinish: T0 + 30n * 86_400n,
      totalRewardFunded: 30_000_000_000n,
      totalRewardClaimed: 0n,
      totalRewardCompounded: 0n,
      ...c,
    } as Config,
    miner: {
      rigs: SEEDS,
      lifetimeRigsBought: SEEDS,
      weight: 100,
      referrer: '11111111111111111111111111111111',
      ...m,
    } as Miner,
    minerAccountOwner: PROGRAM_ID,
    rewardVaultDistributable: 30_000_000_000n,
    chainTime: T0 + 60n,
    pdas: PDAS,
    minerAddress: MINER,
    ...over,
  };
}

const expected = { founder: EXPECTED_FOUNDER, seeds: SEEDS };

describe('verify first buy', () => {
  it('passes a correct activation and reports ~24h to Public', () => {
    const r = evaluateFirstBuy(tx(), state(), expected);
    expect(r.checks.filter((c) => !c.pass)).toEqual([]);
    expect(r.countdown).toBe(86_400n - 60n);
  });

  it('allows compute-budget instructions only', () => {
    const cb = { programId: COMPUTE_BUDGET_PROGRAM, accounts: [], data: new Uint8Array([2]) };
    expect(evaluateFirstBuy(tx({ instructions: [cb, ...tx().instructions] }), state(), expected).ok).toBe(true);
    const foreign = { programId: address('11111111111111111111111111111111'), accounts: [], data: new Uint8Array([2]) };
    expect(evaluateFirstBuy(tx({ instructions: [foreign, ...tx().instructions] }), state(), expected).ok).toBe(false);
  });

  const breaks: Array<[string, FirstBuyTx, FirstBuyState]> = [
    ['failed tx', tx({ err: { InstructionError: [0, 'x'] } }), state()],
    ['only processed', tx({ commitment: 'processed' }), state()],
    ['wrong signer', tx({ signers: [address('9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM')] }), state()],
    ['T0 not the block time', tx(), state({}, { launchT0: T0 + 1n })],
    ['fee not routed exactly', tx({ postBalances: tx().postBalances.map((b, i) => (i === 4 ? b - 1n : b)) }), state()],
    ['reward vault touched', tx({ postBalances: tx().postBalances.map((b, i) => (i === 5 ? b - 1n : b)) }), state()],
    ['wrong seeds credited', tx(), state({}, {}, { rigs: 9n })],
    ['activation above cap', tx(), state({}, { genesisCapRigs: SEEDS - 1n })],
    ['miner not program-owned', tx(), state({ minerAccountOwner: address('11111111111111111111111111111111') })],
    ['referral on activation', tx(), state({}, {}, { weight: 105 })],
    ['insolvent', tx(), state({ rewardVaultDistributable: 1n })],
    ['phase already public', tx(), state({ chainTime: T0 + 3n * 86_400n })],
  ];
  for (const [label, t, s] of breaks) {
    it(`catches: ${label}`, () => {
      expect(evaluateFirstBuy(t, s, expected).ok).toBe(false);
    });
  }
});
