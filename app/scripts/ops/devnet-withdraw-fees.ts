/**
 * THE CHERRY TREE — withdraw accrued protocol fees.
 *
 * DEVNET ONLY.
 *
 * This is the only admin money path in V1, and it is deliberately narrow: the
 * authority signs, the FEE vault pays, and the destination is whatever
 * `config.fee_recipient` says — the program rejects any other account. The
 * recipient never signs, so collecting revenue never requires its private key
 * to be present on this machine or anywhere else.
 *
 * Neither the reward vault nor the capital vault is reachable from here.
 *
 *   ./scripts/devnet-withdraw-fees.sh <amountSol>
 *   ./scripts/devnet-withdraw-fees.sh all
 */
import {
  assertConnectedToDevnet,
  connect,
  fail,
  fetchConfig,
  header,
  lamportsOf,
  loadEnv,
  loadSigner,
  requireDevnet,
  requireEnv,
  row,
  section,
  sol,
  submit,
  vaultDistributable,
} from './common';

import { programAddresses } from '../../src/lib/program/pdas';
import { PROGRAM_ID } from '../../src/lib/program/constants';
import { withdrawFeesInstruction } from './admin-instructions';
import { feeWithdrawable, resolveWithdrawAmount } from './fee-math';

const LAMPORTS_PER_SOL = 1_000_000_000n;

function parseAmount(raw: string | undefined): 'all' | bigint {
  if (!raw) fail('usage: ./scripts/devnet-withdraw-fees.sh <amountSol>|all');
  if (raw === 'all') return 'all';
  if (!/^\d+(\.\d{1,9})?$/.test(raw)) {
    fail(`amount must be a decimal number of SOL (max 9 dp), got: ${raw}`);
  }
  const [whole = '0', frac = ''] = raw.split('.');
  return BigInt(whole) * LAMPORTS_PER_SOL + BigInt(frac.padEnd(9, '0'));
}

async function main(): Promise<void> {
  loadEnv();
  header('THE CHERRY TREE — withdraw protocol fees');

  section('cluster guard');
  const { cluster, rpcUrl } = requireDevnet();
  row('cluster', cluster);
  row('rpc', rpcUrl);

  const requested = parseAmount(process.argv[2]);

  const walletPath = requireEnv(
    'ANCHOR_WALLET',
    'Set it to the authority keypair JSON path. The authority signs the withdrawal.',
  );
  const authority = await loadSigner(walletPath, 'authority');

  const pdas = await programAddresses();
  const chain = connect(rpcUrl);
  await assertConnectedToDevnet(chain);

  const config = await fetchConfig(chain);
  if (!config) fail(`config PDA ${pdas.config} does not exist on ${cluster}.`);

  if (config.authority !== authority.address) {
    fail(
      `the loaded wallet ${authority.address} is not the program authority ` +
        `(${config.authority}). Only the authority may trigger a withdrawal.`,
    );
  }

  // Rent stays put: only the surplus above the rent-exempt reserve can move,
  // and reserved referral commissions never move (§7a item 11):
  // withdrawable = distributable(FEE) - reserved_commissions.
  const distributable = await vaultDistributable(chain, pdas.feeVault);
  let limits;
  try {
    limits = feeWithdrawable(distributable, config.reservedCommissions);
  } catch (e) {
    fail(e instanceof Error ? e.message : String(e));
  }
  const available = limits.withdrawable;

  section('about to withdraw');
  row('program ID', PROGRAM_ID);
  row('FEE VAULT PDA', pdas.feeVault);
  row('gross distributable FEE', `${sol(limits.distributable)} SOL`);
  row('reserved commissions', `${sol(limits.reserved)} SOL (never withdrawable)`);
  row('withdrawable now', `${sol(available)} SOL`);
  row('destination', config.feeRecipient);
  row('  (pinned in config)', 'the program rejects any other destination');
  row('signed by', authority.address);
  row('total fees to date', `${sol(config.totalProtocolFees)} SOL`);
  row('already withdrawn', `${sol(config.totalFeesWithdrawn)} SOL`);

  let amount: bigint;
  try {
    amount = resolveWithdrawAmount(requested, limits);
  } catch (e) {
    fail(e instanceof Error ? e.message : String(e));
  }
  row('amount', `${amount} lamports (${sol(amount)} SOL)`);

  // Proof that the other two vaults are untouched, measured rather than claimed.
  const rewardBefore = await lamportsOf(chain, pdas.rewardVault);
  const capitalBefore = await lamportsOf(chain, pdas.capitalVault);
  const recipientBefore = await lamportsOf(chain, config.feeRecipient);

  section('sending withdraw_fees');
  const signature = await submit(
    chain,
    authority,
    withdrawFeesInstruction(authority.address, pdas, config.feeRecipient, amount),
  );
  row('signature', signature);

  section('verifying');
  const after = await fetchConfig(chain);
  if (!after) fail('config PDA vanished after the withdrawal. Something is wrong.');

  const recipientAfter = await lamportsOf(chain, config.feeRecipient);
  const rewardAfter = await lamportsOf(chain, pdas.rewardVault);
  const capitalAfter = await lamportsOf(chain, pdas.capitalVault);

  const checks: [string, unknown, unknown][] = [
    ['recipient credited', recipientAfter - recipientBefore, amount],
    ['reward vault unchanged', rewardAfter, rewardBefore],
    ['capital vault unchanged', capitalAfter, capitalBefore],
    [
      'total_fees_withdrawn',
      after.totalFeesWithdrawn,
      config.totalFeesWithdrawn + amount,
    ],
    ['fee_recipient unchanged', after.feeRecipient, config.feeRecipient],
    ['reserved commissions unchanged', after.reservedCommissions, config.reservedCommissions],
    [
      'FEE still backs the reservation',
      (await vaultDistributable(chain, pdas.feeVault)) >= after.reservedCommissions,
      true,
    ],
  ];

  let bad = 0;
  for (const [name, actual, expected] of checks) {
    const ok = String(actual) === String(expected);
    if (!ok) bad += 1;
    console.log(
      `  ${ok ? 'ok  ' : 'BAD '} ${name.padEnd(26)} ${String(actual)}${ok ? '' : `   (expected ${String(expected)})`}`,
    );
  }
  if (bad > 0) fail(`${bad} post-withdrawal check(s) did not match.`);

  console.log(`\nWITHDREW ${sol(amount)} SOL to ${config.feeRecipient}.`);
}

main().catch((e) => {
  console.error(`\nFAIL: ${e instanceof Error ? e.message : String(e)}\n`);
  process.exitCode = 1;
});
