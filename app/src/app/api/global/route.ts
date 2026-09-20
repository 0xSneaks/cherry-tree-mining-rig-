/**
 * `GET /api/global` — cached global stats, all chain-derived.
 *
 * Public: nothing here is user-scoped, so no launch token is required.
 */
import { json } from '../../../lib/server/launchToken';
import { globalStats } from '../../../lib/server/indexer';

export const runtime = 'nodejs';
export const revalidate = 10;

export async function GET(): Promise<Response> {
  try {
    const { config, rewardVaultLamports, capitalVaultLamports, feeVaultLamports } =
      await globalStats();

    return json(
      {
        totalRigs: config.totalRigs.toString(),
        minerCount: config.minerCount.toString(),
        rigPriceLamports: config.rigPriceLamports.toString(),
        purchaseFeeBps: config.purchaseFeeBps,
        paused: config.paused,
        rewardRatePerSecond: config.rewardRate.toString(),
        rewardPeriodFinish: config.rewardPeriodFinish.toString(),
        rewardVaultLamports: rewardVaultLamports.toString(),
        capitalVaultLamports: capitalVaultLamports.toString(),
        feeVaultLamports: feeVaultLamports.toString(),
        totalRewardFunded: config.totalRewardFunded.toString(),
        totalRewardClaimed: config.totalRewardClaimed.toString(),
        totalRewardCompounded: config.totalRewardCompounded.toString(),
        undistributedDust: config.undistributedDust.toString(),
      },
      200,
      { 'cache-control': 'public, max-age=10, stale-while-revalidate=30' },
    );
  } catch (e) {
    return json({ error: e instanceof Error ? e.message : 'unavailable' }, 503);
  }
}
