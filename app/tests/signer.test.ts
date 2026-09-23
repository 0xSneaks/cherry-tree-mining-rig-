/**
 * The transaction the app hands to a wallet — Cherry's bridge on Seeker, or
 * a Mobile Wallet Adapter / Seed Vault wallet standalone — must be one a
 * hardware-backed single-key signer can sign as is: a v0 message, fee payer =
 * the connected wallet and the only required signer, a fresh RPC blockhash,
 * no pre-filled signatures, and within the 1232-byte packet limit even for the
 * largest purchase (wrapped-domain proof + referrer).
 */
import {
  address,
  getBase58Encoder,
  getCompiledTransactionMessageDecoder,
  getTransactionDecoder,
  type Address,
} from '@solana/kit';
import { describe, expect, it } from 'vitest';

import type { RigsRpc } from '../src/lib/chain';
import { buyRigsInstruction, claimInstruction } from '../src/lib/program/instructions';
import type { ProgramAddresses } from '../src/lib/program/pdas';
import { buildAndSend } from '../src/lib/signer';

const OWNER = address('7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU');
const BLOCKHASH = 'EkSnNWid2cvwEVnVx9aBqawnmiCNiDgp3gUdkDPTKN1N';
const PDAS: ProgramAddresses = {
  config: address('11111111111111111111111111111112'),
  capitalVault: address('11111111111111111111111111111113'),
  rewardVault: address('11111111111111111111111111111114'),
  feeVault: address('11111111111111111111111111111115'),
};
const MINER = address('So11111111111111111111111111111111111111112');
const k = (s: string) => address(s);
const IDENTITY: Address[] = [
  k('11111111111111111111111111111116'),
  k('11111111111111111111111111111117'),
  k('11111111111111111111111111111118'),
];
const REFERRER = {
  wallet: k('US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx'),
  miner: k('11111111111111111111111111111119'),
};

const fakeRpc = {
  getLatestBlockhash: () => ({
    send: async () => ({ value: { blockhash: BLOCKHASH, lastValidBlockHeight: 100n } }),
  }),
} as unknown as RigsRpc;

async function wireFor(ix: Parameters<typeof buildAndSend>[1]) {
  let wire: Uint8Array | null = null;
  await buildAndSend(
    OWNER,
    ix,
    async (w) => {
      wire = w;
      return 'sig';
    },
    fakeRpc,
  );
  return wire!;
}

describe('wallet-facing transaction shape (Cherry bridge, MWA / Seed Vault)', () => {
  for (const [name, ix] of [
    [
      'plant (largest: wrapped proof + referrer)',
      buyRigsInstruction(OWNER, MINER, PDAS, 10n, { identity: IDENTITY, referrer: REFERRER }),
    ],
    ['claim', claimInstruction(OWNER, MINER, PDAS)],
  ] as const) {
    it(`${name}: v0, single signer = fee payer, fresh blockhash, unsigned, fits a packet`, async () => {
      const wire = await wireFor(ix);
      expect(wire.length).toBeLessThanOrEqual(1232);

      const tx = getTransactionDecoder().decode(wire);
      expect(Object.keys(tx.signatures)).toEqual([OWNER]);
      expect(tx.signatures[OWNER]).toBeNull();

      const msg = getCompiledTransactionMessageDecoder().decode(tx.messageBytes);
      expect(msg.version).toBe(0);
      if (msg.version !== 0) return;
      expect(msg.header.numSignerAccounts).toBe(1);
      expect(msg.staticAccounts[0]).toBe(OWNER);
      expect(msg.lifetimeToken).toBe(BLOCKHASH);
      expect(msg.instructions).toHaveLength(1);
      expect(getBase58Encoder().encode(BLOCKHASH)).toHaveLength(32);
    });
  }
});
