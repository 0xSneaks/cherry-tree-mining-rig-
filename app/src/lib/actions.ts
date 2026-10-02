/**
 * The user actions, identical in both modes.
 *
 * Each returns once the chain has *confirmed* the signature. Nothing in the UI
 * advances on an unconfirmed send (spec §8).
 */
import type { Address } from '@solana/kit';

import { confirmSignature, rpc } from './chain';
import {
  buyRigsInstruction,
  claimCommissionInstruction,
  claimInstruction,
  compoundInstruction,
} from './program/instructions';
import { discoverIdentity } from './program/identity';
import { minerPda, programAddresses, referralPda, skrBindingPda } from './program/pdas';
import type { RigsSigner } from './signer';

async function context(owner: Address) {
  const [pdas, miner] = await Promise.all([programAddresses(), minerPda(owner)]);
  return { pdas, miner };
}

/** The all-ones "no referrer" address a Miner stores when unreferred. */
const NO_REFERRER = '11111111111111111111111111111111';

/**
 * The referral ledger a referred position must carry (architecture §7): the
 * bound referrer's, or — on a first purchase — the referrer being bound.
 */
async function ledgerFor(
  boundReferrer: Address | null | undefined,
  newReferrer: Address | null | undefined,
): Promise<Address | null> {
  const r = boundReferrer && boundReferrer !== NO_REFERRER ? boundReferrer : newReferrer;
  return r ? referralPda(r) : null;
}

/**
 * Plant `count` seeds. The signer's `.skr` / `.sns` proof is discovered and
 * passed along; the program re-verifies it. `referrer`, if given, is bound
 * only on a first purchase — the program rejects it otherwise.
 */
export async function buyRigs(
  signer: RigsSigner,
  count: bigint,
  opts: {
    referrer?: Address | null;
    /** The position's already-bound referrer (`Miner.referrer`), if any. */
    boundReferrer?: Address | null;
  } = {},
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
  const referralLedger = await ledgerFor(opts.boundReferrer, opts.referrer);
  const sig = await signer.signAndSend(
    buyRigsInstruction(signer.address, miner, pdas, count, {
      identity,
      skrBinding,
      referrer,
      referralLedger,
    }),
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

export async function compound(
  signer: RigsSigner,
  opts: { boundReferrer?: Address | null } = {},
): Promise<string> {
  const { pdas, miner } = await context(signer.address);
  const referralLedger = await ledgerFor(opts.boundReferrer, null);
  const sig = await signer.signAndSend(
    compoundInstruction(signer.address, miner, pdas, { referralLedger }),
  );
  await confirmSignature(sig, rpc());
  return sig;
}

/**
 * A referrer claims its whole unpaid referral commission from the fee vault
 * (architecture §7a item 4). Refused on chain when nothing is unpaid.
 */
export async function claimCommission(signer: RigsSigner): Promise<string> {
  const pdas = await programAddresses();
  const ledger = await referralPda(signer.address);
  const sig = await signer.signAndSend(claimCommissionInstruction(signer.address, ledger, pdas));
  await confirmSignature(sig, rpc());
  return sig;
}
