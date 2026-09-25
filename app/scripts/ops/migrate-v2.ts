/**
 * QUALIFICATION-ONLY V1 -> V2 migration.
 *
 * This command cannot run from the canonical source tree: it requires an
 * explicit qualification flag and disposable program ID, rejects the
 * canonical ID, verifies Devnet from the chain, then migrates one exact
 * 321-byte Config to 329 bytes and checks the result. It never deploys,
 * funds, or activates anything.
 */
import { address, getAddressDecoder, getBase64Encoder } from '@solana/kit';

import { decodeConfig } from '../../src/lib/program/accounts';
import {
  ACCOUNT_DISCRIMINATOR,
  MAX_TOTAL_RIGS,
  PROGRAM_ID,
} from '../../src/lib/program/constants';
import { programAddresses } from '../../src/lib/program/pdas';
import { migrateV2Instruction } from './admin-instructions';
import {
  assertConnectedToDevnet,
  connect,
  fail,
  header,
  loadEnv,
  loadSigner,
  requireDevnet,
  requireEnv,
  row,
  section,
  submit,
} from './common';

const CANONICAL_PROGRAM_ID = 'C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt';
const V1_SIZE = 321;
const V2_SIZE = 329;

function bytes(encoded: readonly [string, string]): Uint8Array {
  return new Uint8Array(getBase64Encoder().encode(encoded[0]));
}

function same(a: Uint8Array, b: Uint8Array): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i]);
}

async function main(): Promise<void> {
  loadEnv();
  header('THE CHERRY TREE — qualification V1 -> V2 migration');
  const { rpcUrl } = requireDevnet();
  if (process.env.CHERRY_TREE_QUALIFICATION !== '1') {
    fail('CHERRY_TREE_QUALIFICATION=1 is required. Qualification mode is never implicit.');
  }
  const target = address(
    requireEnv('CHERRY_TREE_QUALIFICATION_PROGRAM_ID', 'Set the disposable Devnet program ID.'),
  );
  if (target === CANONICAL_PROGRAM_ID) fail('The canonical program ID is forbidden as a qualification target.');
  if (target !== PROGRAM_ID) {
    fail(`staged source declares ${PROGRAM_ID}, but qualification target is ${target}`);
  }

  const cap = BigInt(
    requireEnv('CHERRY_TREE_GENESIS_CAP_RIGS', 'Set an explicit qualification cap; there is no default.'),
  );
  if (cap <= 0n || cap > MAX_TOTAL_RIGS) fail(`cap must be in 1..=${MAX_TOTAL_RIGS}`);

  const authority = await loadSigner(
    requireEnv('ANCHOR_WALLET', 'Set the V1 Config authority keypair path.'),
    'qualification authority',
  );
  const chain = connect(rpcUrl);
  await assertConnectedToDevnet(chain);
  const { config } = await programAddresses();
  const beforeInfo = await chain.rpc.getAccountInfo(config, { encoding: 'base64' }).send();
  if (!beforeInfo.value) fail(`V1 Config ${config} is missing.`);
  const before = bytes(beforeInfo.value.data);
  if (before.length !== V1_SIZE) fail(`Config is ${before.length} bytes; expected exact V1 size ${V1_SIZE}.`);
  if (beforeInfo.value.owner !== PROGRAM_ID) fail(`Config owner is ${beforeInfo.value.owner}, not ${PROGRAM_ID}.`);
  if (!same(before.subarray(0, 8), ACCOUNT_DISCRIMINATOR.config)) fail('Config discriminator is wrong.');
  const storedAuthority = getAddressDecoder().decode(before.subarray(8, 40));
  if (storedAuthority !== authority.address) {
    fail(`signer ${authority.address} is not stored V1 authority ${storedAuthority}`);
  }
  if (new DataView(before.buffer, before.byteOffset + 262, 2).getUint16(0, true) !== 1) {
    fail('Config version is not V1. Replay or non-V1 migration refused.');
  }
  const beforeLamports = BigInt(beforeInfo.value.lamports);
  const v2Rent = BigInt(await chain.rpc.getMinimumBalanceForRentExemption(BigInt(V2_SIZE)).send());

  section('about to migrate disposable Devnet Config');
  row('program ID', PROGRAM_ID);
  row('Config PDA', config);
  row('authority', authority.address);
  row('V1 size', before.length);
  row('V2 cap', cap);
  row('rent shortfall', v2Rent > beforeLamports ? v2Rent - beforeLamports : 0n);

  const signature = await submit(chain, authority, migrateV2Instruction(authority.address, config, cap));
  row('signature', signature);

  const afterInfo = await chain.rpc.getAccountInfo(config, { encoding: 'base64' }).send();
  if (!afterInfo.value) fail('Config disappeared after migration.');
  const after = bytes(afterInfo.value.data);
  if (after.length !== V2_SIZE) fail(`post-migration Config is ${after.length}, expected ${V2_SIZE}`);
  const decoded = decodeConfig(after);
  if (decoded.version !== 2 || decoded.genesisCapRigs !== cap) {
    fail(`post-migration version/cap mismatch: ${decoded.version}/${decoded.genesisCapRigs}`);
  }
  if (!same(before.subarray(0, 262), after.subarray(0, 262)) ||
      !same(before.subarray(264), after.subarray(264, 321))) {
    fail('a V1 field other than version changed during migration.');
  }
  const expectedLamports = beforeLamports > v2Rent ? beforeLamports : v2Rent;
  if (BigInt(afterInfo.value.lamports) !== expectedLamports) {
    fail(`Config lamports ${afterInfo.value.lamports}; expected ${expectedLamports}`);
  }
  console.log('\nMIGRATED: disposable Devnet Config is exact V2. No funding or activation occurred.');
}

main().catch((e) => {
  console.error(`\nFAIL: ${e instanceof Error ? e.message : String(e)}\n`);
  process.exitCode = 1;
});
