/**
 * The three user actions, identical in both modes.
 *
 * Each returns once the chain has *confirmed* the signature. Nothing in the UI
 * advances on an unconfirmed send (spec §8).
 */
import type { Address } from '@solana/kit';

import { confirmSignature, rpc } from './chain';
import {
  buyRigsInstruction,
  claimInstruction,
  compoundInstruction,
} from './program/instructions';
import { minerPda, programAddresses } from './program/pdas';
import type { RigsSigner } from './signer';

async function context(owner: Address) {
  const [pdas, miner] = await Promise.all([programAddresses(), minerPda(owner)]);
  return { pdas, miner };
}

export async function buyRigs(signer: RigsSigner, count: bigint): Promise<string> {
  const { pdas, miner } = await context(signer.address);
  const sig = await signer.signAndSend(
    buyRigsInstruction(signer.address, miner, pdas, count),
  );
  await confirmSignature(sig, rpc());
  return sig;
}

export async function claim(signer: RigsSigner): Promise<string> {
  const { pdas, miner } = await context(signer.address);
  const sig = await signer.signAndSend(claimInstruction(signer.address, miner, pdas));
  await confirmSignature(sig, rpc());
  return sig;
}

export async function compound(signer: RigsSigner): Promise<string> {
  const { pdas, miner } = await context(signer.address);
  const sig = await signer.signAndSend(compoundInstruction(signer.address, miner, pdas));
  await confirmSignature(sig, rpc());
  return sig;
}
