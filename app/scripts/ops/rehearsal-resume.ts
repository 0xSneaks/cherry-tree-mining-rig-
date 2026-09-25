/**
 * The strict resume fingerprint for the devnet rehearsal "activate" stage.
 *
 * The stage is one-shot by design: it activates the launch and then proves
 * the post-T0 gate. If its process dies mid-way, the on-chain state is left
 * exactly at "the founder activation purchase landed, nothing since". A
 * `--resume` run may continue ONLY from precisely that state — anything else
 * and the operator must investigate, not resume. Never guess where to resume.
 *
 * Pure functions, no chain access, so the fingerprint is unit-testable.
 */

/** The founder's position as the interrupted flow leaves it. */
export type ResumeMinerSnapshot = {
  rigs: bigint;
  weight: number;
  createdAt: bigint;
  lastActionAt: bigint;
  lastPlantedAt: bigint;
  lifetimeSolClaimed: bigint;
};

/** Everything the chain is asked to prove before a resume may continue. */
export type ActivateResumeObserved = {
  /** Owner of the on-chain Config account — the program that wrote it. */
  configOwner: string;
  founder: string;
  authority: string;
  rewardFunder: string;
  launchT0: bigint;
  paused: boolean;
  totalRigs: bigint;
  totalShares: bigint;
  minerCount: bigint;
  totalCapitalReceived: bigint;
  totalProtocolFees: bigint;
  totalRewardFunded: bigint;
  totalRewardClaimed: bigint;
  totalRewardCompounded: bigint;
  rewardRate: bigint;
  rewardPeriodFinish: bigint;
  /** Null when the founder has no miner account at all. */
  founderMiner: ResumeMinerSnapshot | null;
  walletAMinerExists: boolean;
  walletBMinerExists: boolean;
  capitalVaultExcess: bigint;
  feeVaultExcess: bigint;
  rewardVaultDistributable: bigint;
  chainNow: bigint;
};

/** The identities and activation figures the resume was told to expect. */
export type ActivateResumeExpected = {
  /** Canonical program ID (`EXPECTED_PROGRAM_ID` in the shell guard). */
  programId: string;
  founder: string;
  authority: string;
  rewardFunder: string;
  /** The T0 the interrupted run wrote; non-zero, re-observed on chain. */
  expectT0: bigint;
  /** Seeds the activation purchase planted (10 in the rehearsal). */
  activationSeeds: bigint;
  /** Capital the activation purchase moved (seeds × rig price). */
  activationCapital: bigint;
  /** Fees the activation purchase moved (capital × fee bps / 10 000). */
  activationFees: bigint;
  /** One gated access window; the refusal checks are labelled `.skr phase`. */
  accessWindowSeconds: bigint;
};

/**
 * Every field that must hold before a resume may continue.
 * Empty array = exact match; anything else = STOP.
 */
export function activateResumeMismatches(
  observed: ActivateResumeObserved,
  expected: ActivateResumeExpected,
): string[] {
  const mismatches: string[] = [];
  const expect = (ok: boolean, label: string, got: unknown, want: unknown) => {
    if (!ok) mismatches.push(`${label}: got ${String(got)}, want ${String(want)}`);
  };

  expect(
    observed.configOwner === expected.programId,
    'config account owner (canonical program ID)',
    observed.configOwner,
    expected.programId,
  );
  expect(observed.founder === expected.founder, 'config.founder', observed.founder, expected.founder);
  expect(
    observed.authority === expected.authority,
    'config.authority',
    observed.authority,
    expected.authority,
  );
  expect(
    observed.rewardFunder === expected.rewardFunder,
    'config.reward_funder',
    observed.rewardFunder,
    expected.rewardFunder,
  );

  expect(
    observed.launchT0 > 0n && observed.launchT0 === expected.expectT0,
    'launch_t0',
    observed.launchT0,
    expected.expectT0,
  );
  expect(observed.paused === false, 'paused', observed.paused, false);

  expect(
    observed.totalRigs === expected.activationSeeds,
    'total_rigs',
    observed.totalRigs,
    expected.activationSeeds,
  );
  expect(
    observed.totalShares === expected.activationSeeds * 100n,
    'total_shares (activation seeds × weight 100)',
    observed.totalShares,
    expected.activationSeeds * 100n,
  );
  expect(observed.minerCount === 1n, 'miner_count', observed.minerCount, 1n);
  expect(
    observed.totalCapitalReceived === expected.activationCapital,
    'total_capital_received',
    observed.totalCapitalReceived,
    expected.activationCapital,
  );
  expect(
    observed.totalProtocolFees === expected.activationFees,
    'total_protocol_fees',
    observed.totalProtocolFees,
    expected.activationFees,
  );

  // The interrupted flow claims and compounds nothing.
  expect(observed.totalRewardClaimed === 0n, 'total_reward_claimed', observed.totalRewardClaimed, 0n);
  expect(
    observed.totalRewardCompounded === 0n,
    'total_reward_compounded',
    observed.totalRewardCompounded,
    0n,
  );

  // The reward schedule must be funded and still live.
  expect(
    observed.totalRewardFunded > 0n &&
      observed.rewardRate > 0n &&
      observed.chainNow < observed.rewardPeriodFinish,
    'reward schedule funded and live',
    `rate ${observed.rewardRate}, finishes ${observed.rewardPeriodFinish}, now ${observed.chainNow}`,
    'rate > 0 and now < finish',
  );

  const miner = observed.founderMiner;
  if (miner === null) {
    mismatches.push('founder miner: got absent, want the activation position');
  } else {
    expect(miner.rigs === expected.activationSeeds, 'founder miner rigs', miner.rigs, expected.activationSeeds);
    expect(miner.weight === 100, 'founder miner weight', miner.weight, 100);
    expect(miner.createdAt === observed.launchT0, 'founder miner created_at', miner.createdAt, observed.launchT0);
    expect(
      miner.lastActionAt === observed.launchT0,
      'founder miner last_action_at (nothing may have landed after the activation)',
      miner.lastActionAt,
      observed.launchT0,
    );
    expect(
      miner.lastPlantedAt === observed.launchT0,
      'founder miner last_planted_at',
      miner.lastPlantedAt,
      observed.launchT0,
    );
    expect(
      miner.lifetimeSolClaimed === 0n,
      'founder miner lifetime_sol_claimed',
      miner.lifetimeSolClaimed,
      0n,
    );
  }

  expect(
    !observed.walletAMinerExists,
    'wallet A miner',
    observed.walletAMinerExists ? 'present' : 'absent',
    'absent',
  );
  expect(
    !observed.walletBMinerExists,
    'wallet B miner',
    observed.walletBMinerExists ? 'present' : 'absent',
    'absent',
  );

  expect(
    observed.capitalVaultExcess === expected.activationCapital,
    'capital vault excess over rent',
    observed.capitalVaultExcess,
    expected.activationCapital,
  );
  expect(
    observed.feeVaultExcess === expected.activationFees,
    'fee vault excess over rent',
    observed.feeVaultExcess,
    expected.activationFees,
  );
  const owed =
    observed.totalRewardFunded - observed.totalRewardClaimed - observed.totalRewardCompounded;
  expect(
    observed.rewardVaultDistributable >= owed,
    'reward vault backs funded − claimed − compounded',
    observed.rewardVaultDistributable,
    `≥ ${owed}`,
  );

  // The stage's refusal checks are labelled for the .skr phase; past that
  // window the labels would lie, so a resume must not run there.
  expect(
    observed.chainNow < observed.launchT0 + expected.accessWindowSeconds,
    'chain still inside the .skr window',
    observed.chainNow,
    `< ${observed.launchT0 + expected.accessWindowSeconds}`,
  );

  return mismatches;
}
