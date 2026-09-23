/**
 * POST-FIRST-BUY VERIFICATION — read-only. Verifies the founder activation,
 * prints the report, and STOPS. It cannot send a transaction.
 *
 *   CHERRY_TREE_RPC_URL=<mainnet RPC> \
 *   CHERRY_TREE_FIRST_BUY_SEEDS=<the count that was planted> \
 *   npx tsx scripts/ops/verify-first-buy.ts <signature>
 */
import {
  address,
  createSolanaRpc,
  getBase58Encoder,
  signature as asSignature,
  type Address,
} from '@solana/kit';

import { currentChainTime, fetchSnapshot } from '../../src/lib/chain';
import { EXPECTED_FOUNDER } from '../../src/lib/launch/expected';
import { evaluateFirstBuy, type FirstBuyTx } from '../../src/lib/launch/verify';
import { minerPda, programAddresses } from '../../src/lib/program/pdas';
import { fail, header, loadEnv, OpsError, requireEnv, row, section } from './common';

async function main() {
  loadEnv();
  header('THE CHERRY TREE — verify first buy (read-only)');
  const sig = process.argv[2];
  if (!sig) fail('Usage: verify-first-buy.ts <signature>');
  const rpc = createSolanaRpc(requireEnv('CHERRY_TREE_RPC_URL', 'The RPC to read from.'));
  const seeds = BigInt(requireEnv('CHERRY_TREE_FIRST_BUY_SEEDS', 'The count that was planted.'));
  const founder = address(process.env.CHERRY_TREE_FOUNDER ?? EXPECTED_FOUNDER);

  const fetched = await rpc
    .getTransaction(asSignature(sig), {
      encoding: 'json',
      maxSupportedTransactionVersion: 0,
      commitment: 'confirmed',
    })
    .send();
  if (!fetched) fail(`Transaction ${sig} not found (not confirmed yet?).`);
  const status = await rpc.getSignatureStatuses([asSignature(sig)]).send();
  const commitment = (status.value[0]?.confirmationStatus ?? 'processed') as FirstBuyTx['commitment'];

  const msg = fetched.transaction.message;
  const keys: Address[] = [
    ...msg.accountKeys,
    ...(fetched.meta?.loadedAddresses?.writable ?? []),
    ...(fetched.meta?.loadedAddresses?.readonly ?? []),
  ].map((k) => address(k));
  const b58 = getBase58Encoder();
  const tx: FirstBuyTx = {
    signature: sig,
    commitment,
    err: fetched.meta?.err ?? null,
    blockTime: fetched.blockTime === null ? null : BigInt(fetched.blockTime),
    feeLamports: BigInt(fetched.meta?.fee ?? 0),
    accountKeys: keys,
    signers: keys.slice(0, msg.header.numRequiredSignatures),
    instructions: msg.instructions.map((i) => ({
      programId: keys[i.programIdIndex]!,
      accounts: i.accounts.map((a) => keys[a]!),
      data: new Uint8Array(b58.encode(i.data)),
    })),
    preBalances: (fetched.meta?.preBalances ?? []).map(BigInt),
    postBalances: (fetched.meta?.postBalances ?? []).map(BigInt),
  };

  const [snapshot, pdas, miner, now] = await Promise.all([
    fetchSnapshot(founder, rpc),
    programAddresses(),
    minerPda(founder),
    currentChainTime(rpc),
  ]);
  if (!snapshot) fail('Config not found on this cluster.');
  const minerInfo = await rpc.getAccountInfo(miner, { encoding: 'base64' }).send();

  const r = evaluateFirstBuy(
    tx,
    {
      config: snapshot.config,
      miner: snapshot.miner,
      minerAccountOwner: minerInfo.value?.owner ?? null,
      rewardVaultDistributable: snapshot.rewardVaultDistributable,
      chainTime: now,
      pdas,
      minerAddress: miner,
    },
    { founder, seeds },
  );

  section('checks');
  for (const c of r.checks) row(`${c.pass ? 'PASS' : 'FAIL'}  ${c.name}`, c.detail);
  section('result');
  row('T0', `${r.t0} (${new Date(Number(r.t0) * 1000).toISOString()})`);
  row('APR now (bps)', r.aprBps);
  row('.sns early access in', r.countdown === null ? '—' : `${r.countdown} s`);
  row('VERIFY', r.ok ? 'PASS' : 'FAIL');
  console.log('\n  STOP. No further transaction is sent by this tool.\n');
  if (!r.ok) process.exitCode = 1;
}

main().catch((e) => {
  console.error(e instanceof OpsError ? `\n!! ${e.message}\n` : e);
  process.exitCode = 1;
});
