/**
 * Gather every preflight input from the chain. Read-only: nothing here signs
 * or sends. The simulation runs the exact activation instruction unsigned
 * (`sigVerify: false`) so neither the CLI nor the page needs the founder key.
 */
import {
  appendTransactionMessageInstruction,
  compileTransaction,
  createTransactionMessage,
  getBase64Decoder,
  getTransactionEncoder,
  pipe,
  setTransactionMessageFeePayer,
  setTransactionMessageLifetimeUsingBlockhash,
  type Address,
  type Base64EncodedWireTransaction,
  type Instruction,
  type TransactionMessageBytesBase64,
} from '@solana/kit';

import { fetchSnapshot, type RigsRpc } from '../chain';
import { ACCOUNT_SIZE, PROGRAM_ID } from '../program/constants';
import { buyRigsInstruction } from '../program/instructions';
import { minerPda, programAddresses } from '../program/pdas';
import { clusterFromGenesis, type Cluster } from './expected';
import type { PreflightInput } from './preflight';

async function unsignedWire(rpc: RigsRpc, payer: Address, ix: Instruction) {
  const { value: bh } = await rpc.getLatestBlockhash().send();
  const message = pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayer(payer, m),
    (m) => setTransactionMessageLifetimeUsingBlockhash(bh, m),
    (m) => appendTransactionMessageInstruction(ix, m),
  );
  const tx = compileTransaction(message);
  const wire = getBase64Decoder().decode(
    getTransactionEncoder().encode(tx),
  ) as Base64EncodedWireTransaction;
  const messageB64 = getBase64Decoder().decode(
    new Uint8Array(tx.messageBytes),
  ) as TransactionMessageBytesBase64;
  return { wire, messageB64 };
}

/** Simulate the activation exactly as it would be sent, minus signatures. */
export async function simulateActivation(rpc: RigsRpc, founder: Address, ix: Instruction) {
  const { wire } = await unsignedWire(rpc, founder, ix);
  const sim = await rpc
    .simulateTransaction(wire, {
      encoding: 'base64',
      sigVerify: false,
      replaceRecentBlockhash: true,
      commitment: 'confirmed',
    })
    .send();
  const err = sim.value.err;
  return {
    ok: err === null,
    error: err === null ? null : JSON.stringify(err, (_k, v) => (typeof v === 'bigint' ? v.toString() : v)),
    logs: [...(sim.value.logs ?? [])],
  };
}

export async function gatherPreflight(
  rpc: RigsRpc,
  founder: Address,
  rigCount: bigint | null,
  intendedCluster: Cluster | null,
): Promise<{ input: PreflightInput; founderMiner: Address }> {
  const [genesis, programInfo, snapshot, pdas, founderMiner, balance, rent] =
    await Promise.all([
      rpc.getGenesisHash().send(),
      rpc.getAccountInfo(PROGRAM_ID, { encoding: 'base64' }).send(),
      fetchSnapshot(founder, rpc),
      programAddresses(),
      minerPda(founder),
      rpc.getBalance(founder).send(),
      rpc.getMinimumBalanceForRentExemption(BigInt(ACCOUNT_SIZE.miner)).send(),
    ]);

  const ix =
    rigCount !== null && rigCount > 0n
      ? buyRigsInstruction(founder, founderMiner, pdas, rigCount)
      : null;

  let networkFee = 5_000n;
  let simulation: PreflightInput['simulation'] = null;
  if (ix && snapshot) {
    const { messageB64 } = await unsignedWire(rpc, founder, ix);
    const fee = await rpc.getFeeForMessage(messageB64).send();
    if (fee.value !== null) networkFee = BigInt(fee.value);
    simulation = await simulateActivation(rpc, founder, ix);
  }

  return {
    founderMiner,
    input: {
      intendedCluster,
      actualCluster: clusterFromGenesis(genesis),
      programAccount: programInfo.value ? { executable: programInfo.value.executable } : null,
      config: snapshot?.config ?? null,
      pdas,
      founderWallet: founder,
      founderMinerExists: snapshot?.miner != null,
      rewardVaultDistributable: snapshot?.rewardVaultDistributable ?? 0n,
      chainTime: snapshot?.chainTimeSeconds ?? 0n,
      founderLamports: BigInt(balance.value),
      rigCount,
      networkFeeLamports: networkFee,
      minerRentLamports: BigInt(rent),
      instruction: ix,
      simulation,
    },
  };
}
