/**
 * The one place the two modes differ.
 *
 * Spec §8: the game component is identical embedded and standalone; only the
 * wallet layer branches. Everything downstream depends on this interface and
 * never on which mode it is running in.
 */
import {
  appendTransactionMessageInstruction,
  compileTransaction,
  createTransactionMessage,
  getBase58Decoder,
  getTransactionEncoder,
  pipe,
  setTransactionMessageFeePayer,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signAndSendTransactionMessageWithSigners,
  type Address,
  type Instruction,
  type TransactionSendingSigner,
} from '@solana/kit';

import { rpc, type RigsRpc } from './chain';

export type RigsSigner = {
  address: Address;
  /** Returns the transaction signature. */
  signAndSend(instruction: Instruction): Promise<string>;
};

/**
 * Build a single-instruction transaction and hand its wire bytes to `send`.
 *
 * V1 flows are all single-transaction, so `signAllTransactions` — which
 * batches into one user confirmation — is deliberately unused.
 */
export async function buildAndSend(
  feePayer: Address,
  instruction: Instruction,
  send: (wireBytes: Uint8Array) => Promise<string>,
  client: RigsRpc = rpc(),
): Promise<string> {
  const { value: latestBlockhash } = await client.getLatestBlockhash().send();

  const message = pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayer(feePayer, m),
    (m) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, m),
    (m) => appendTransactionMessageInstruction(instruction, m),
  );

  const transaction = compileTransaction(message);
  const wire = new Uint8Array(getTransactionEncoder().encode(transaction));
  return send(wire);
}

/** Minimal shape of the Cherry wallet facade this app uses. */
export type CherryWalletLike = {
  publicKey: string | null;
  signAndSendTransaction(transaction: unknown): Promise<string>;
};

/**
 * Embedded signer: signing goes through the Cherry bridge. No Mobile Wallet
 * Adapter, no Seed Vault, no deeplinks, and no connect step — the wallet
 * arrives already connected via the handshake.
 *
 * `signAndSendTransaction` accepts raw wire bytes, so the kit-built
 * transaction is handed over without a web3.js round trip.
 */
export function cherrySigner(
  wallet: CherryWalletLike,
  address: Address,
): RigsSigner {
  return {
    address,
    signAndSend: (instruction) =>
      buildAndSend(address, instruction, (wire) =>
        wallet.signAndSendTransaction(wire),
      ),
  };
}

/**
 * Standalone signer: a wallet-standard account, via kit's signer plumbing.
 *
 * `useWalletAccountTransactionSendingSigner` returns a
 * `TransactionSendingSigner`; attaching it as the fee-payer signer lets kit
 * sign and send in one step. Kept behind the same `RigsSigner` interface so
 * the game never learns which mode it is in.
 */
export function walletStandardSigner(
  signer: TransactionSendingSigner,
): RigsSigner {
  return {
    address: signer.address,
    async signAndSend(instruction) {
      const client = rpc();
      const { value: latestBlockhash } = await client.getLatestBlockhash().send();

      const message = pipe(
        createTransactionMessage({ version: 0 }),
        (m) => setTransactionMessageFeePayerSigner(signer, m),
        (m) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, m),
        (m) => appendTransactionMessageInstruction(instruction, m),
      );

      const signature = await signAndSendTransactionMessageWithSigners(message);
      return getBase58Decoder().decode(signature);
    },
  };
}
