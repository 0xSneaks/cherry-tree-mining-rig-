/**
 * AUTHORITY AUDIT — read-only. Sends nothing, signs nothing, holds no key.
 *
 *   CHERRY_TREE_RPC_URL=<rpc> \
 *   CHERRY_TREE_EXPECTED_UPGRADE_AUTHORITY=<multisig vault | revoked> \
 *   CHERRY_TREE_EXPECTED_CONFIG_AUTHORITY=<multisig vault> \
 *   CHERRY_TREE_EXPECTED_REWARD_FUNDER=<funder multisig vault> \
 *   CHERRY_TREE_EXPECTED_FEE_RECIPIENT=<cold / multisig destination> \
 *   npx tsx scripts/ops/authority-audit.ts
 *
 * Reports the program's upgrade authority (from ProgramData), Config
 * authorities and their separation against docs/backlog/CUSTODY.md. It never
 * transfers, proposes or accepts any authority. Exit 1 on any FAIL.
 */
import { createSolanaRpc, getAddressDecoder, getBase64Encoder, type Address } from '@solana/kit';

import { decodeConfig } from '../../src/lib/program/accounts';
import { PROGRAM_ID } from '../../src/lib/program/constants';
import { programAddresses } from '../../src/lib/program/pdas';
import { clusterFromGenesis } from '../../src/lib/launch/expected';
import {
  evaluateCustody,
  parsePolicyAddress,
  programDataAddressFrom,
  upgradeAuthorityFrom,
} from '../../src/lib/ops/custody';
import { fail, header, loadEnv, OpsError, requireEnv, row, section } from './common';

const b64 = (s: string) => new Uint8Array(getBase64Encoder().encode(s));
const encode = (b: Uint8Array) => getAddressDecoder().decode(b) as Address;

async function main() {
  loadEnv();
  header('THE CHERRY TREE — authority audit (read-only)');
  const rpc = createSolanaRpc(requireEnv('CHERRY_TREE_RPC_URL', 'The RPC to read from.'));
  const cluster = clusterFromGenesis(await rpc.getGenesisHash().send());
  row('cluster (from genesis hash)', cluster);
  row('program', PROGRAM_ID);

  const program = await rpc.getAccountInfo(PROGRAM_ID, { encoding: 'base64' }).send();
  if (!program.value) fail('program account not found on this cluster');
  const programData = programDataAddressFrom(b64(program.value.data[0]), encode);
  const pd = await rpc
    .getAccountInfo(programData, { encoding: 'base64', dataSlice: { offset: 0, length: 45 } })
    .send();
  if (!pd.value) fail('ProgramData account not found');
  const upgradeAuthority = upgradeAuthorityFrom(b64(pd.value.data[0]), encode);

  const { config: configPda } = await programAddresses();
  const cfgAcc = await rpc.getAccountInfo(configPda, { encoding: 'base64' }).send();
  if (!cfgAcc.value) fail('Config not initialised');
  const config = decodeConfig(b64(cfgAcc.value.data[0]));

  section('on chain');
  row('programdata', programData);
  row('upgrade authority', upgradeAuthority ?? 'REVOKED (immutable)');
  row('config authority', config.authority);
  row('pending authority', config.pendingAuthority);
  row('reward funder', config.rewardFunder);
  row('fee recipient', config.feeRecipient);
  row('founder', config.founder);
  row('paused', config.paused);

  const expectedUpgrade = (process.env.CHERRY_TREE_EXPECTED_UPGRADE_AUTHORITY ?? '').trim();
  const checks = evaluateCustody(
    {
      upgradeAuthority,
      configAuthority: config.authority,
      pendingAuthority: config.pendingAuthority,
      rewardFunder: config.rewardFunder,
      feeRecipient: config.feeRecipient,
      founder: config.founder,
    },
    {
      expectedUpgradeAuthority: expectedUpgrade === 'revoked' ? 'revoked' : parsePolicyAddress(expectedUpgrade),
      expectedConfigAuthority: parsePolicyAddress(process.env.CHERRY_TREE_EXPECTED_CONFIG_AUTHORITY),
      expectedRewardFunder: parsePolicyAddress(process.env.CHERRY_TREE_EXPECTED_REWARD_FUNDER),
      expectedFeeRecipient: parsePolicyAddress(process.env.CHERRY_TREE_EXPECTED_FEE_RECIPIENT),
    },
  );
  section('custody policy');
  for (const c of checks) row(`${c.level.padEnd(4)} ${c.name}`, c.detail);
  if (checks.some((c) => c.level === 'FAIL')) process.exitCode = 1;
}

main().catch((e) => {
  console.error(e instanceof OpsError ? `\n!! ${e.message}\n` : e);
  process.exitCode = 1;
});
