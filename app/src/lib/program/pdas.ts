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

/**
 * Day-1 `.skr` binding PDA for a domain's ANS name account:
 * `["skr_bind", name_account]`. One domain opens Day-1 access for one wallet.
 */
export async function skrBindingPda(nameAccount: Address): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: [SEEDS.skrBind, getAddressEncoder().encode(nameAccount)],
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

/** BPF upgradeable loader. Owns every upgradeable program's ProgramData. */
export const BPF_LOADER_UPGRADEABLE_ID =
  'BPFLoaderUpgradeab1e11111111111111111111111' as Address;

/**
 * The program's ProgramData account: PDA([program id], upgradeable loader).
 * `initialize` reads the upgrade authority from it and refuses any other
 * signer.
 */
export async function programDataPda(): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: BPF_LOADER_UPGRADEABLE_ID,
    seeds: [getAddressEncoder().encode(PROGRAM_ID)],
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
