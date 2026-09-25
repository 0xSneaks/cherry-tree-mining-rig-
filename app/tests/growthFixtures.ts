import { address, type Address } from '@solana/kit';

import type { Config, Miner } from '../src/lib/program/accounts';

export const A = (s: string): Address => address(s);

export const SYS = A('11111111111111111111111111111111');
export const WALLET_1 = A('US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx');
export const WALLET_2 = A('FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp');
export const WALLET_3 = A('C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt');
export const WALLET_4 = A('ALTNSZ46uaAUU7XUV6awvdorLGqAsPwa9shm7h4uP2FK');

export const NOW = 1_800_000_000n;
export const DAY = 86_400n;

export function config(over: Partial<Config> = {}): Config {
  return {
    authority: SYS,
    pendingAuthority: SYS,
    rewardFunder: SYS,
    feeRecipient: SYS,
    paused: false,
    rigPriceLamports: 10_000_000n,
    purchaseFeeBps: 300,
    totalRigs: 1_000n,
    rewardRate: 1_000n,
    rewardPeriodFinish: NOW + 30n * DAY,
    rewardLastUpdate: NOW,
    rewardPerRigStored: 0n,
    undistributedDust: 0n,
    totalRewardFunded: 5_000_000_000n,
    totalRewardClaimed: 1_000_000_000n,
    totalRewardCompounded: 500_000_000n,
    totalCapitalReceived: 0n,
    totalProtocolFees: 0n,
    totalFeesWithdrawn: 0n,
    minerCount: 10n,
    capitalVaultBump: 255,
    rewardVaultBump: 255,
    feeVaultBump: 255,
    version: 2,
    bump: 255,
    founder: WALLET_2,
    launchT0: NOW - 10n * DAY,
    totalShares: 100_000n,
    totalClaimPenalties: 0n,
    genesisCapRigs: 10n,
    ...over,
  };
}

export function miner(over: Partial<Miner> = {}): Miner {
  return {
    owner: WALLET_1,
    rigs: 100n,
    rewardPerRigPaid: 0n,
    pendingRewards: 0n,
    lifetimeRigsBought: 100n,
    lifetimeRigsCompounded: 0n,
    lifetimeSolSpent: 1_030_000_000n,
    lifetimeSolCompounded: 0n,
    lifetimeSolClaimed: 0n,
    createdAt: NOW - 5n * DAY,
    lastActionAt: NOW - 5n * DAY,
    bump: 255,
    weight: 100,
    referrer: SYS,
    lastPlantedAt: NOW - 5n * DAY,
    ...over,
  };
}
