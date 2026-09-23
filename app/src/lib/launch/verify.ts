/**
 * Post-first-buy verification — pure, so every rule is unit-tested. The CLI
 * (`scripts/ops/verify-first-buy.ts`) fetches the confirmed transaction and
 * the resulting state, then hands both here. It reports and stops; nothing in
 * this path can send a transaction.
 */
import { address, type Address } from '@solana/kit';

import type { Config, Miner } from '../program/accounts';
import { IX_DISCRIMINATOR, PROGRAM_ID } from '../program/constants';
import { accessPhase, secondsToNextPhase } from '../program/launch';
import { quotePurchase } from '../program/pricing';
import { aprBps, yieldInputs } from '../program/yield';
import type { Check } from './preflight';

export const COMPUTE_BUDGET_PROGRAM = address('ComputeBudget111111111111111111111111111111');
const DEFAULT_KEY = '11111111111111111111111111111111';

export type FirstBuyTx = {
  signature: string;
  commitment: 'confirmed' | 'finalized' | 'processed';
  err: unknown;
  blockTime: bigint | null;
  feeLamports: bigint;
  accountKeys: Address[];
  signers: Address[];
  instructions: { programId: Address; accounts: Address[]; data: Uint8Array }[];
  preBalances: bigint[];
  postBalances: bigint[];
};

export type FirstBuyState = {
  config: Config;
  miner: Miner | null;
  minerAccountOwner: Address | null;
  rewardVaultDistributable: bigint;
  chainTime: bigint;
  pdas: { config: Address; capitalVault: Address; feeVault: Address; rewardVault: Address };
  minerAddress: Address;
};

function delta(tx: FirstBuyTx, key: Address): bigint | null {
  const i = tx.accountKeys.indexOf(key);
  if (i < 0) return null;
  return (tx.postBalances[i] ?? 0n) - (tx.preBalances[i] ?? 0n);
}

export function evaluateFirstBuy(
  tx: FirstBuyTx,
  s: FirstBuyState,
  expected: { founder: Address; seeds: bigint },
): { ok: boolean; checks: Check[]; t0: bigint; aprBps: bigint; countdown: bigint | null } {
  const checks: Check[] = [];
  const add = (name: string, pass: boolean, detail: string) => checks.push({ name, pass, detail });
  const c = s.config;

  add(
    'confirmed signature',
    tx.err === null && (tx.commitment === 'confirmed' || tx.commitment === 'finalized'),
    `${tx.signature} · ${tx.commitment} · err ${tx.err === null ? 'none' : JSON.stringify(tx.err)}`,
  );
  add(
    'founder signed and paid',
    tx.signers[0] === expected.founder && tx.signers.includes(expected.founder),
    `fee payer ${tx.signers[0]}`,
  );

  const ours = tx.instructions.filter((i) => i.programId === PROGRAM_ID);
  const others = tx.instructions.filter(
    (i) => i.programId !== PROGRAM_ID && i.programId !== COMPUTE_BUDGET_PROGRAM,
  );
  const buy = ours[0];
  const isBuy =
    ours.length === 1 &&
    buy !== undefined &&
    IX_DISCRIMINATOR.buyRigs.every((b, k) => buy.data[k] === b) &&
    buy.data.length === 17 &&
    buy.data[16] === 0;
  const count = isBuy
    ? new DataView(buy.data.buffer, buy.data.byteOffset + 8, 8).getBigUint64(0, true)
    : -1n;
  add(
    'no unexpected instructions',
    isBuy && others.length === 0,
    `${ours.length} program instruction(s), ${others.length} foreign (compute-budget allowed)`,
  );
  add(
    'no unexpected accounts',
    isBuy && buy.accounts.length === 7,
    isBuy ? `${buy.accounts.length} accounts` : 'no buy_rigs',
  );
  add('purchase amount', count === expected.seeds, `${count} seeds (expected ${expected.seeds})`);

  const t0 = c.launchT0;
  add(
    'activation T0 = transaction block time',
    tx.blockTime !== null && t0 === tx.blockTime && t0 > 0n,
    `launch_t0 ${t0}, block time ${tx.blockTime}`,
  );
  add('founder pinned', c.founder === expected.founder, `config.founder ${c.founder}`);

  const quote = quotePurchase(expected.seeds, c.rigPriceLamports, BigInt(c.purchaseFeeBps));
  const feeD = delta(tx, s.pdas.feeVault);
  const capD = delta(tx, s.pdas.capitalVault);
  const rewardD = delta(tx, s.pdas.rewardVault);
  const minerD = delta(tx, s.minerAddress) ?? 0n;
  const founderD = delta(tx, expected.founder);
  add('exact purchase fee routed', feeD === quote.feeLamports, `fee vault +${feeD} (quote ${quote.feeLamports})`);
  add('exact capital routed', capD === quote.capitalLamports, `capital vault +${capD} (quote ${quote.capitalLamports})`);
  add('reward vault untouched', rewardD === 0n || rewardD === null, `reward vault Δ ${rewardD}`);
  add(
    'exact SOL movement from founder',
    founderD === -(quote.totalLamports + tx.feeLamports + minerD),
    `founder Δ ${founderD} = −(${quote.totalLamports} purchase + ${tx.feeLamports} network + ${minerD} miner rent)`,
  );

  const m = s.miner;
  add(
    'expected seeds credited',
    m !== null && m.rigs === expected.seeds && c.totalRigs === expected.seeds,
    `miner ${m?.rigs ?? 'none'}, global ${c.totalRigs}`,
  );
  add('miner account owned by the program', s.minerAccountOwner === PROGRAM_ID, `owner ${s.minerAccountOwner}`);
  add(
    'no referral on the activation',
    m !== null && m.referrer === DEFAULT_KEY && (m.weight === 100 || m.weight === 0),
    `weight ${m?.weight}, referrer ${m?.referrer}`,
  );

  const owed = c.totalRewardFunded - c.totalRewardClaimed - c.totalRewardCompounded;
  add(
    'reward accounting',
    c.totalRewardClaimed === 0n && c.totalRewardCompounded === 0n && c.totalRewardFunded > 0n,
    `funded ${c.totalRewardFunded}, claimed ${c.totalRewardClaimed}, compounded ${c.totalRewardCompounded}`,
  );
  add('reward vault solvent', s.rewardVaultDistributable >= owed, `${s.rewardVaultDistributable} ≥ ${owed}`);

  const apr = aprBps(yieldInputs(c, s.chainTime));
  add('APR from state', apr > 0n, `${apr} bps`);

  const phase = accessPhase(t0, s.chainTime);
  const countdown = secondsToNextPhase(t0, s.chainTime);
  add('access phase = .SKR GENESIS', phase === 'skrGenesis', phase);
  add(
    'countdown ≈ 24h to .sns access',
    countdown !== null && countdown > 0n && countdown <= 86_400n,
    `${countdown} s`,
  );

  return { ok: checks.every((k) => k.pass), checks, t0, aprBps: apr, countdown };
}
