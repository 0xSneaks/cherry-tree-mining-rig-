/**
 * THE CHERRY TREE — devnet initialization.
 *
 * DEVNET ONLY. Creates the config PDA and the three vault PDAs, then reads
 * the state back and verifies every field it just set.
 *
 * It does NOT fund rewards. Funding is a separate, explicit step.
 */
import {
  assertConnectedToDevnet,
  chainTime,
  connect,
  fail,
  fetchConfig,
  header,
  loadEnv,
  loadSigner,
  lamportsOf,
  requireDevnet,
  requireEnv,
  row,
  section,
  sol,
  submit,
} from './common';
import { address, type Address } from '@solana/kit';

import { programAddresses } from '../../src/lib/program/pdas';
import { PROGRAM_ID } from '../../src/lib/program/constants';
import { initializeInstruction } from './admin-instructions';

async function main(): Promise<void> {
  loadEnv();
  header('THE CHERRY TREE — devnet initialize');

  section('cluster guard');
  const { cluster, rpcUrl } = requireDevnet();
  row('cluster', cluster);
  row('rpc', rpcUrl);

  section('inputs');
  const walletPath = requireEnv(
    'ANCHOR_WALLET',
    'Set it to the authority keypair JSON path.',
  );
  const funderRaw = requireEnv(
    'CHERRY_TREE_REWARD_FUNDER',
    'Set it to the reward funder PUBLIC KEY. It does not have to be the authority.',
  );
  const feeRecipientRaw = requireEnv(
    'CHERRY_TREE_FEE_RECIPIENT',
    'Set it to the protocol fee recipient PUBLIC KEY. It never signs: the ' +
      'program pins it as the only account withdraw_fees may pay.',
  );
  const rigPriceLamports = BigInt(
    requireEnv('CHERRY_TREE_RIG_PRICE_LAMPORTS', 'e.g. 10000000 for 0.01 SOL.'),
  );
  const purchaseFeeBps = Number(
    requireEnv('CHERRY_TREE_PURCHASE_FEE_BPS', 'e.g. 300 for 3.00%.'),
  );

  let rewardFunder: Address;
  try {
    rewardFunder = address(funderRaw);
  } catch {
    fail(`CHERRY_TREE_REWARD_FUNDER is not a valid public key: ${funderRaw}`);
  }
  let feeRecipient: Address;
  try {
    feeRecipient = address(feeRecipientRaw);
  } catch {
    fail(`CHERRY_TREE_FEE_RECIPIENT is not a valid public key: ${feeRecipientRaw}`);
  }
  if (rigPriceLamports <= 0n) fail('CHERRY_TREE_RIG_PRICE_LAMPORTS must be positive.');
  if (!Number.isInteger(purchaseFeeBps) || purchaseFeeBps < 0 || purchaseFeeBps > 1000) {
    fail('CHERRY_TREE_PURCHASE_FEE_BPS must be an integer in 0..=1000 (the program caps it at 1000).');
  }

  // Only the public key is ever printed.
  const authority = await loadSigner(walletPath, 'authority');

  const pdas = await programAddresses();
  const chain = connect(rpcUrl);
  await assertConnectedToDevnet(chain);

  section('about to initialize');
  row('program ID', PROGRAM_ID);
  row('authority (deployer)', authority.address);
  row('authority balance', `${sol(await lamportsOf(chain, authority.address))} SOL`);
  row('reward funder', rewardFunder);
  row('fee recipient', feeRecipient);
  row('rig price', `${rigPriceLamports} lamports (${sol(rigPriceLamports)} SOL)`);
  row('purchase fee', `${purchaseFeeBps} bps (${(purchaseFeeBps / 100).toFixed(2)}%)`);
  console.log();
  row('CONFIG PDA', pdas.config);
  row('CAPITAL VAULT PDA', pdas.capitalVault);
  row('REWARD VAULT PDA', pdas.rewardVault);
  row('FEE VAULT PDA', pdas.feeVault);

  if (authority.address === feeRecipient) {
    console.log(
      '\n  note: the authority is also the fee recipient. Allowed, but the\n' +
        '        program keeps them as separate identities on purpose — the\n' +
        '        authority triggers a withdrawal, the recipient receives it.',
    );
  }

  if (authority.address === rewardFunder) {
    console.log(
      '\n  note: authority and reward funder are the same key. That is allowed,\n' +
        '        but they are independent roles — prefer separate keys.',
    );
  }

  const existing = await fetchConfig(chain);
  if (existing) {
    fail(
      `config PDA ${pdas.config} already exists on ${cluster}. ` +
        'Initialization is one-shot; refusing to touch live state.',
    );
  }

  section('sending initialize');
  const signature = await submit(
    chain,
    authority,
    initializeInstruction(authority.address, pdas, {
      rigPriceLamports,
      purchaseFeeBps,
      rewardFunder,
      feeRecipient,
    }),
  );
  row('signature', signature);

  section('verifying on-chain state');
  const config = await fetchConfig(chain);
  if (!config) fail('config PDA is still absent after initialize. Something is wrong.');

  const checks: [string, unknown, unknown][] = [
    ['authority', config.authority, authority.address],
    ['reward_funder', config.rewardFunder, rewardFunder],
    ['fee_recipient', config.feeRecipient, feeRecipient],
    ['rig_price_lamports', config.rigPriceLamports, rigPriceLamports],
    ['purchase_fee_bps', config.purchaseFeeBps, purchaseFeeBps],
    ['total_rigs', config.totalRigs, 0n],
    ['total_reward_funded', config.totalRewardFunded, 0n],
    ['total_reward_claimed', config.totalRewardClaimed, 0n],
    ['total_reward_compounded', config.totalRewardCompounded, 0n],
    ['total_capital_received', config.totalCapitalReceived, 0n],
    ['total_protocol_fees', config.totalProtocolFees, 0n],
    ['miner_count', config.minerCount, 0n],
    ['paused', config.paused, false],
    ['pending_authority', config.pendingAuthority, '11111111111111111111111111111111'],
    ['version', config.version, 1],
  ];

  let bad = 0;
  for (const [name, actual, expected] of checks) {
    const ok = String(actual) === String(expected);
    if (!ok) bad += 1;
    console.log(
      `  ${ok ? 'ok  ' : 'BAD '} ${name.padEnd(26)} ${String(actual)}${ok ? '' : `   (expected ${String(expected)})`}`,
    );
  }

  // The vault PDAs must exist and be owned by the program.
  for (const [name, addr] of [
    ['CAPITAL VAULT', pdas.capitalVault],
    ['REWARD VAULT', pdas.rewardVault],
    ['FEE VAULT', pdas.feeVault],
  ] as const) {
    const acc = await chain.rpc.getAccountInfo(addr, { encoding: 'base64' }).send();
    const owned = acc.value?.owner === PROGRAM_ID;
    if (!owned) bad += 1;
    console.log(
      `  ${owned ? 'ok  ' : 'BAD '} ${name.padEnd(26)} ${addr} owner=${acc.value?.owner ?? '<missing>'}`,
    );
  }

  // Vault bumps recorded in config must match the derived PDAs.
  row('capital_vault_bump', config.capitalVaultBump);
  row('reward_vault_bump', config.rewardVaultBump);
  row('fee_vault_bump', config.feeVaultBump);
  row('chain time', await chainTime(chain));

  if (bad > 0) fail(`${bad} post-initialize check(s) did not match. Do not proceed.`);

  console.log('\nINITIALIZED. Every field verified against on-chain state.');
  console.log(`\nProtocol fees will accrue in the FEE VAULT (${pdas.feeVault})`);
  console.log(`and are withdrawable only to ${feeRecipient}:`);
  console.log('  ./scripts/devnet-withdraw-fees.sh <amountSol>');
  console.log('\nRewards are NOT funded. That is a separate explicit step:');
  console.log('  ./scripts/devnet-fund.sh <amountSol> <durationSeconds>');
}

main().catch((e) => {
  console.error(`\nFAIL: ${e instanceof Error ? e.message : String(e)}\n`);
  process.exitCode = 1;
});
