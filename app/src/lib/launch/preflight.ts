/**
 * First-buy preflight — one pure evaluation shared by the CLI
 * (`scripts/ops/first-buy-preflight.ts`) and the in-app founder activation.
 *
 * It fails closed: every check must pass, and a missing input is a failure.
 * Passing it does not send anything. The activation is a separate, explicitly
 * confirmed step that re-runs this preflight (and its simulation) first.
 */
import type { Address, Instruction } from '@solana/kit';

import type { Config } from '../program/accounts';
import { IX_DISCRIMINATOR, PROGRAM_ID, SYSTEM_PROGRAM_ID } from '../program/constants';
import type { ProgramAddresses } from '../program/pdas';
import { quotePurchase, type PurchaseQuote } from '../program/pricing';
import { aprBps, yieldInputs } from '../program/yield';
import { EXPECTED_FEE_RECIPIENT, EXPECTED_FOUNDER, type Cluster } from './expected';

export type PreflightInput = {
  /** What the operator explicitly said they intend. */
  intendedCluster: Cluster | null;
  /** What the RPC's genesis hash says it actually is. */
  actualCluster: Cluster | 'unknown';
  programAccount: { executable: boolean } | null;
  config: Config | null;
  pdas: ProgramAddresses;
  founderWallet: Address;
  founderMinerExists: boolean;
  rewardVaultDistributable: bigint;
  chainTime: bigint;
  founderLamports: bigint;
  /** Explicit. Never defaulted. */
  rigCount: bigint | null;
  networkFeeLamports: bigint;
  minerRentLamports: bigint;
  instruction: Instruction | null;
  simulation: { ok: boolean; error: string | null; logs: string[] } | null;
};

export type Check = { name: string; pass: boolean; detail: string };

export type PreflightResult = {
  ok: boolean;
  checks: Check[];
  quote: PurchaseQuote | null;
  aprBpsNow: bigint | null;
  aprBpsAfter: bigint | null;
  totalCostLamports: bigint | null;
};

function outstanding(c: Config): bigint {
  return c.totalRewardFunded - c.totalRewardClaimed - c.totalRewardCompounded;
}

/**
 * The only instruction shape an activation may have: the seven declared
 * accounts and nothing else — the founder wallet needs no domain proof.
 */
function expectedAccounts(i: PreflightInput, miner: Address): Address[] {
  return [
    i.founderWallet,
    i.pdas.config,
    miner,
    i.pdas.capitalVault,
    i.pdas.feeVault,
    i.pdas.rewardVault,
    SYSTEM_PROGRAM_ID,
  ];
}

export function evaluatePreflight(i: PreflightInput, founderMiner: Address): PreflightResult {
  const checks: Check[] = [];
  const add = (name: string, pass: boolean, detail: string) => checks.push({ name, pass, detail });
  const c = i.config;

  add(
    'cluster explicitly intended',
    i.intendedCluster !== null && i.intendedCluster === i.actualCluster,
    `intended ${i.intendedCluster ?? '(not stated)'}, RPC genesis says ${i.actualCluster}`,
  );
  add(
    'production cluster',
    i.actualCluster === 'mainnet-beta',
    i.actualCluster === 'mainnet-beta'
      ? 'mainnet-beta'
      : `${i.actualCluster}: a rehearsal, not the live activation`,
  );
  add(
    'expected program id',
    i.programAccount?.executable === true,
    `${PROGRAM_ID} ${i.programAccount ? (i.programAccount.executable ? 'is executable' : 'is NOT executable') : 'not found'}`,
  );
  add(
    'expected founder wallet',
    i.founderWallet === EXPECTED_FOUNDER,
    `connected ${i.founderWallet}, expected ${EXPECTED_FOUNDER}`,
  );

  add('program initialised', c !== null, c ? 'config found' : 'config PDA missing');
  if (c) {
    add('config founder pinned', c.founder === i.founderWallet, `config.founder = ${c.founder}`);
    add(
      'expected payout / fee wallet',
      c.feeRecipient === EXPECTED_FEE_RECIPIENT,
      `config.fee_recipient = ${c.feeRecipient}`,
    );
    add('not paused', !c.paused, c.paused ? 'program is paused' : 'live');
    add('never activated', c.launchT0 === 0n, `launch_t0 = ${c.launchT0}`);
    add(
      'reward vault funded',
      c.totalRewardFunded > 0n && i.rewardVaultDistributable > 0n,
      `funded ${c.totalRewardFunded}, vault ${i.rewardVaultDistributable} lamports`,
    );
    add(
      'reward schedule active',
      c.rewardRate > 0n && i.chainTime < c.rewardPeriodFinish,
      `rate ${c.rewardRate}/s, finishes at ${c.rewardPeriodFinish}, chain time ${i.chainTime}`,
    );
    add(
      'vault solvent',
      i.rewardVaultDistributable >= outstanding(c),
      `distributable ${i.rewardVaultDistributable} ≥ outstanding ${outstanding(c)}`,
    );
  }

  const count = i.rigCount;
  add(
    'purchase amount supplied explicitly',
    count !== null && count > 0n,
    count === null ? 'no seed count given' : `${count} seeds`,
  );

  let quote: PurchaseQuote | null = null;
  let aprNow: bigint | null = null;
  let aprAfter: bigint | null = null;
  let total: bigint | null = null;
  if (c && count !== null && count > 0n) {
    try {
      quote = quotePurchase(count, c.rigPriceLamports, BigInt(c.purchaseFeeBps));
    } catch (e) {
      add('quote', false, e instanceof Error ? e.message : String(e));
    }
    const inputs = yieldInputs(c, i.chainTime);
    aprNow = aprBps(inputs);
    aprAfter = aprBps(inputs, count);
    add('APR derived from state', true, `now ${aprNow} bps · after planting ${aprAfter} bps`);
  }
  if (quote) {
    const rent = i.founderMinerExists ? 0n : i.minerRentLamports;
    total = quote.totalLamports + i.networkFeeLamports + rent;
    add(
      'founder balance covers purchase + fees',
      i.founderLamports >= total,
      `balance ${i.founderLamports} ≥ ${quote.totalLamports} purchase + ${i.networkFeeLamports} network + ${rent} rent`,
    );
    add('expected seeds credited', true, `${quote.count}`);
    add('expected purchase fee', true, `${quote.feeLamports} lamports to the fee vault`);
  }

  // The instruction must be exactly one buy_rigs, no referrer, no extras.
  if (i.instruction) {
    const ix = i.instruction;
    const data = ix.data ?? new Uint8Array();
    const accounts = (ix.accounts ?? []).map((a) => a.address);
    const want = expectedAccounts(i, founderMiner);
    const shapeOk =
      ix.programAddress === PROGRAM_ID &&
      IX_DISCRIMINATOR.buyRigs.every((b, k) => data[k] === b) &&
      data.length === 8 + 8 + 1 &&
      data[16] === 0 &&
      accounts.length === want.length &&
      accounts.every((a, k) => a === want[k]);
    add(
      'instruction contains only expected operations',
      shapeOk,
      shapeOk ? 'one buy_rigs, no referrer, no extra accounts' : 'unexpected instruction shape',
    );
  } else {
    add('instruction contains only expected operations', false, 'no instruction built');
  }

  add(
    'simulation succeeds',
    i.simulation?.ok === true,
    i.simulation ? (i.simulation.ok ? 'simulated cleanly' : (i.simulation.error ?? 'failed')) : 'not simulated',
  );

  return {
    ok: checks.every((k) => k.pass),
    checks,
    quote,
    aprBpsNow: aprNow,
    aprBpsAfter: aprAfter,
    totalCostLamports: total,
  };
}

/** The exact phrase the founder must type to confirm the one live purchase. */
export function confirmationPhrase(count: bigint): string {
  return `ACTIVATE ${count}`;
}
