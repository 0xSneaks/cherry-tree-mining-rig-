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
import { discoverIdentity } from './program/identity';
import { minerPda, programAddresses, skrBindingPda } from './program/pdas';
import type { RigsSigner } from './signer';

async function context(owner: Address) {
  const [pdas, miner] = await Promise.all([programAddresses(), minerPda(owner)]);
  return { pdas, miner };
}

/**
 * Plant `count` seeds. The signer's `.skr` / `.sns` proof is discovered and
 * passed along; the program re-verifies it. `referrer`, if given, is bound
 * only on a first purchase — the program rejects it otherwise.
 */
export async function buyRigs(
  signer: RigsSigner,
  count: bigint,
  opts: { referrer?: Address | null } = {},
): Promise<string> {
  const { pdas, miner } = await context(signer.address);
  const proof = await discoverIdentity(rpc(), signer.address);
  const identity = proof.accounts;
  // A `.skr` proof always carries its Day-1 binding PDA; the program only
  // consults it during [T0, T0 + 24h).
  const name = identity[0];
  const skrBinding = proof.kind === 'skr' && name ? await skrBindingPda(name) : null;
  const referrer = opts.referrer
    ? { wallet: opts.referrer, miner: await minerPda(opts.referrer) }
    : null;
  const sig = await signer.signAndSend(
    buyRigsInstruction(signer.address, miner, pdas, count, { identity, skrBinding, referrer }),
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
