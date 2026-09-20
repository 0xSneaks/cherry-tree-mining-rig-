/**
 * Instruction builders.
 *
 * Account order and writability mirror the `#[derive(Accounts)]` structs in
 * `programs/sol-rigs/src/instructions/`. Getting either wrong is a runtime
 * failure with an opaque message, so the ordering is annotated against the
 * Rust struct it came from.
 */
import {
  AccountRole,
  getU64Encoder,
  type Address,
  type Instruction,
} from '@solana/kit';

import { IX_DISCRIMINATOR, PROGRAM_ID, SYSTEM_PROGRAM_ID } from './constants';
import type { ProgramAddresses } from './pdas';

function ix(
  discriminator: Uint8Array,
  accounts: Instruction['accounts'],
  args?: Uint8Array,
): Instruction {
  const data = new Uint8Array(discriminator.length + (args?.length ?? 0));
  data.set(discriminator, 0);
  if (args) data.set(args, discriminator.length);
  return { programAddress: PROGRAM_ID, accounts, data };
}

/** `buy_rigs(count)` — see `instructions/buy_rigs.rs`. */
export function buyRigsInstruction(
  owner: Address,
  miner: Address,
  pdas: ProgramAddresses,
  count: bigint,
): Instruction {
  return ix(
    IX_DISCRIMINATOR.buyRigs,
    [
      { address: owner, role: AccountRole.WRITABLE_SIGNER },
      { address: pdas.config, role: AccountRole.WRITABLE },
      { address: miner, role: AccountRole.WRITABLE },
      { address: pdas.capitalVault, role: AccountRole.WRITABLE },
      { address: pdas.feeVault, role: AccountRole.WRITABLE },
      // Read-only on purpose: spec §3. A purchase cannot reach reward backing.
      { address: pdas.rewardVault, role: AccountRole.READONLY },
      { address: SYSTEM_PROGRAM_ID, role: AccountRole.READONLY },
    ],
    new Uint8Array(getU64Encoder().encode(count)),
  );
}

/** `claim()` — see `instructions/claim.rs`. */
export function claimInstruction(
  owner: Address,
  miner: Address,
  pdas: ProgramAddresses,
): Instruction {
  return ix(IX_DISCRIMINATOR.claim, [
    { address: owner, role: AccountRole.WRITABLE_SIGNER },
    { address: pdas.config, role: AccountRole.WRITABLE },
    { address: miner, role: AccountRole.WRITABLE },
    { address: pdas.rewardVault, role: AccountRole.WRITABLE },
  ]);
}

/** `compound()` — see `instructions/compound.rs`. */
export function compoundInstruction(
  owner: Address,
  miner: Address,
  pdas: ProgramAddresses,
): Instruction {
  return ix(IX_DISCRIMINATOR.compound, [
    { address: owner, role: AccountRole.WRITABLE_SIGNER },
    { address: pdas.config, role: AccountRole.WRITABLE },
    { address: miner, role: AccountRole.WRITABLE },
    { address: pdas.rewardVault, role: AccountRole.WRITABLE },
    { address: pdas.capitalVault, role: AccountRole.WRITABLE },
    { address: pdas.feeVault, role: AccountRole.WRITABLE },
  ]);
}
