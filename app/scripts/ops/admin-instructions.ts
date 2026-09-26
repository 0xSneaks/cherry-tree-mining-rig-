/**
 * Instruction builders for the operator-only instructions.
 *
 * Deliberately NOT in `src/lib/program/instructions.ts`: `initialize`,
 * `fund_rewards`, `set_fee_recipient` and `withdraw_fees` have no business
 * being reachable from the browser bundle.
 *
 * Account order and writability mirror the `#[derive(Accounts)]` structs in
 * `programs/sol-rigs/src/instructions/`.
 */
import {
  AccountRole,
  getAddressEncoder,
  getU16Encoder,
  getU64Encoder,
  type Address,
  type Instruction,
} from '@solana/kit';

import {
  IX_DISCRIMINATOR,
  PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
} from '../../src/lib/program/constants';
import type { ProgramAddresses } from '../../src/lib/program/pdas';

function concat(...parts: Uint8Array[]): Uint8Array {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

/**
 * `initialize(rig_price_lamports, purchase_fee_bps, reward_funder,
 * fee_recipient, founder, genesis_cap_rigs)`.
 *
 * `founder` is pinned for this program version: the only wallet whose purchase
 * can activate launch. There is no setter.
 *
 * `feeRecipient` is a payout destination pinned into config, not a signer:
 * `withdraw_fees` may pay it and nothing else, and it never has to sign.
 *
 * `authority` must be the program's upgrade authority: the program reads it
 * from `programData` (see `programDataPda`) and refuses any other signer.
 */
export function initializeInstruction(
  authority: Address,
  pdas: ProgramAddresses,
  programData: Address,
  args: {
    rigPriceLamports: bigint;
    purchaseFeeBps: number;
    rewardFunder: Address;
    feeRecipient: Address;
    founder: Address;
    genesisCapRigs: bigint;
  },
): Instruction {
  return {
    programAddress: PROGRAM_ID,
    accounts: [
      { address: authority, role: AccountRole.WRITABLE_SIGNER },
      { address: pdas.config, role: AccountRole.WRITABLE },
      { address: pdas.capitalVault, role: AccountRole.WRITABLE },
      { address: pdas.rewardVault, role: AccountRole.WRITABLE },
      { address: pdas.feeVault, role: AccountRole.WRITABLE },
      { address: SYSTEM_PROGRAM_ID, role: AccountRole.READONLY },
      { address: programData, role: AccountRole.READONLY },
    ],
    data: concat(
      IX_DISCRIMINATOR.initialize,
      new Uint8Array(getU64Encoder().encode(args.rigPriceLamports)),
      new Uint8Array(getU16Encoder().encode(args.purchaseFeeBps)),
      new Uint8Array(getAddressEncoder().encode(args.rewardFunder)),
      new Uint8Array(getAddressEncoder().encode(args.feeRecipient)),
      new Uint8Array(getAddressEncoder().encode(args.founder)),
      new Uint8Array(getU64Encoder().encode(args.genesisCapRigs)),
    ),
  };
}

/** Raw-account `migrate_v2(genesis_cap_rigs)`, authority-only and one-shot. */
export function migrateV2Instruction(
  authority: Address,
  config: Address,
  genesisCapRigs: bigint,
): Instruction {
  return {
    programAddress: PROGRAM_ID,
    accounts: [
      { address: authority, role: AccountRole.WRITABLE_SIGNER },
      { address: config, role: AccountRole.WRITABLE },
      { address: SYSTEM_PROGRAM_ID, role: AccountRole.READONLY },
    ],
    data: concat(
      IX_DISCRIMINATOR.migrateV2,
      new Uint8Array(getU64Encoder().encode(genesisCapRigs)),
    ),
  };
}

/**
 * `set_fee_recipient(fee_recipient)`. Authority only.
 *
 * The incoming recipient is an argument, not an account — it does not sign and
 * is not even passed in, so moving the payout address never requires the new
 * recipient's key to be online.
 */
export function setFeeRecipientInstruction(
  authority: Address,
  pdas: ProgramAddresses,
  feeRecipient: Address,
): Instruction {
  return {
    programAddress: PROGRAM_ID,
    accounts: [
      { address: authority, role: AccountRole.READONLY_SIGNER },
      { address: pdas.config, role: AccountRole.WRITABLE },
    ],
    data: concat(
      IX_DISCRIMINATOR.setFeeRecipient,
      new Uint8Array(getAddressEncoder().encode(feeRecipient)),
    ),
  };
}

/**
 * `withdraw_fees(amount)`. Authority signs; the destination is constrained
 * on chain to `config.fee_recipient`, so passing anything else fails.
 */
export function withdrawFeesInstruction(
  authority: Address,
  pdas: ProgramAddresses,
  feeRecipient: Address,
  amountLamports: bigint,
): Instruction {
  return {
    programAddress: PROGRAM_ID,
    accounts: [
      { address: authority, role: AccountRole.READONLY_SIGNER },
      { address: pdas.config, role: AccountRole.WRITABLE },
      { address: pdas.feeVault, role: AccountRole.WRITABLE },
      { address: feeRecipient, role: AccountRole.WRITABLE },
    ],
    data: concat(
      IX_DISCRIMINATOR.withdrawFees,
      new Uint8Array(getU64Encoder().encode(amountLamports)),
    ),
  };
}

/**
 * `fund_rewards(amount, duration)`.
 *
 * The funder signs and pays: lamports move from their wallet into the reward
 * vault inside the instruction, before the schedule is written.
 */
export function fundRewardsInstruction(
  funder: Address,
  pdas: ProgramAddresses,
  amountLamports: bigint,
  durationSeconds: bigint,
): Instruction {
  return {
    programAddress: PROGRAM_ID,
    accounts: [
      { address: funder, role: AccountRole.WRITABLE_SIGNER },
      { address: pdas.config, role: AccountRole.WRITABLE },
      { address: pdas.rewardVault, role: AccountRole.WRITABLE },
      { address: SYSTEM_PROGRAM_ID, role: AccountRole.READONLY },
    ],
    data: concat(
      IX_DISCRIMINATOR.fundRewards,
      new Uint8Array(getU64Encoder().encode(amountLamports)),
      new Uint8Array(getU64Encoder().encode(durationSeconds)),
    ),
  };
}
