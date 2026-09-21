/**
 * Instruction builders for the two operator-only instructions.
 *
 * Deliberately NOT in `src/lib/program/instructions.ts`: `initialize` and
 * `fund_rewards` have no business being reachable from the browser bundle.
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

/** `initialize(rig_price_lamports, purchase_fee_bps, reward_funder)`. */
export function initializeInstruction(
  authority: Address,
  pdas: ProgramAddresses,
  args: {
    rigPriceLamports: bigint;
    purchaseFeeBps: number;
    rewardFunder: Address;
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
    ],
    data: concat(
      IX_DISCRIMINATOR.initialize,
      new Uint8Array(getU64Encoder().encode(args.rigPriceLamports)),
      new Uint8Array(getU16Encoder().encode(args.purchaseFeeBps)),
      new Uint8Array(getAddressEncoder().encode(args.rewardFunder)),
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
