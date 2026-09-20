/** Program-derived addresses. Seeds mirror `programs/sol-rigs/src/constants.rs`. */
import {
  getAddressEncoder,
  getProgramDerivedAddress,
  type Address,
} from '@solana/kit';

import { PROGRAM_ID, SEEDS } from './constants';

export async function configPda(): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: [SEEDS.config],
  });
  return pda;
}

export async function minerPda(owner: Address): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: [SEEDS.miner, getAddressEncoder().encode(owner)],
  });
  return pda;
}

export async function capitalVaultPda(): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: [SEEDS.capitalVault],
  });
  return pda;
}

export async function rewardVaultPda(): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: [SEEDS.rewardVault],
  });
  return pda;
}

export async function feeVaultPda(): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: [SEEDS.feeVault],
  });
  return pda;
}

export type ProgramAddresses = {
  config: Address;
  capitalVault: Address;
  rewardVault: Address;
  feeVault: Address;
};

let cached: ProgramAddresses | null = null;

/** The four singleton PDAs. Derivation is deterministic, so it is cached. */
export async function programAddresses(): Promise<ProgramAddresses> {
  if (!cached) {
    const [config, capitalVault, rewardVault, feeVault] = await Promise.all([
      configPda(),
      capitalVaultPda(),
      rewardVaultPda(),
      feeVaultPda(),
    ]);
    cached = { config, capitalVault, rewardVault, feeVault };
  }
  return cached;
}
