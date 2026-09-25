/**
 * THE CHERRY TREE — devnet launch rehearsal, against the frozen program.
 *
 * DEVNET ONLY (`requireDevnet` + the genesis-hash check). Two stages, because
 * the access phases run on the real chain clock and devnet cannot be warped:
 *
 *   activate  run right after init + fund, with a devnet rehearsal founder
 *             pinned at init (CHERRY_TREE_ALLOW_OTHER_FOUNDER=1):
 *               - a non-founder purchase before T0 is refused (AccessDenied)
 *               - the founder purchase activates; T0 = its block time
 *               - a post-T0 founder purchase without `.skr` proof is refused
 *                 (AccessDenied) and leaves T0 unchanged — the immutability
 *                 proof. After T0 the founder is an ordinary wallet under the
 *                 access gate; the refusal writes nothing, so T0 cannot move.
 *               - a wallet with no domain is refused in the .skr phase
 *               - founder claim pays the penalty band its planting age earns;
 *                 a sole planter's penalty stays in the reward vault as dust;
 *                 fee/capital vaults untouched
 *               - founder replant pays no penalty and keeps the clock
 *               - reward-vault solvency
 *
 *   activate --resume --expect-t0 <unix>
 *             continue an interrupted activate run, but ONLY from exactly the
 *             state an interrupted run leaves behind (strict fingerprint in
 *             ./rehearsal-resume): T0 already written by the founder
 *             activation purchase, nothing else landed. Never re-funds, never
 *             re-activates, never replays the activation purchase; ordinary
 *             `activate` (without --resume) stays one-shot and refuses a
 *             non-zero T0.
 *
 *   public    run at or after T0 + 48h (the schedule must still be live):
 *               - wallet A plants (weight 100)
 *               - wallet B plants referred by A (weight 105, referrer bound)
 *               - B naming a referrer again is refused (ReferrerAlreadyBound)
 *               - B claims inside 24h: 25% penalty, only the net leaves the
 *                 reward vault, fee/capital vaults untouched
 *               - A replants with no penalty
 *               - reward-vault solvency
 *
 * `.skr` / `.sns` acceptance cannot be exercised here: AllDomains and SNS
 * name records exist on mainnet only. Their verification is covered by the
 * program tests (`programs/sol-rigs/tests/launch.rs`) and the discovery tests
 * (`app/tests/identity.test.ts`).
 *
 * Only public keys are ever printed.
 */
import type { Address, KeyPairSigner } from '@solana/kit';

import {
  assertConnectedToDevnet,
  chainTime,
  connect,
  fail,
  fetchConfig,
  fetchMiner,
  header,
  lamportsOf,
  loadEnv,
  loadSigner,
  requireDevnet,
  requireEnv,
  row,
  section,
  sleep,
  submit,
  vaultDistributable,
  type Chain,
} from './common';
import { ACCESS_WINDOW_SECONDS } from '../../src/lib/program/launch';
import {
  buyRigsInstruction,
  claimInstruction,
  compoundInstruction,
} from '../../src/lib/program/instructions';
import { minerPda, programAddresses } from '../../src/lib/program/pdas';
import { PROGRAM_ID } from '../../src/lib/program/constants';
import { projectedClaimable } from '../../src/lib/program/accrual';
import { quotePurchase } from '../../src/lib/program/pricing';
import { claimPenaltyBps } from '../../src/lib/program/penalty';
import { activateResumeMismatches } from './rehearsal-resume';
import type { Config } from '../../src/lib/program/accounts';

/** Anchor custom error codes: 6000 + position in `error.rs` (see the test). */
export const ERR = {
  ReferrerAlreadyBound: 6021,
  AccessDenied: 6023,
} as const;

type Pdas = Awaited<ReturnType<typeof programAddresses>>;
const results: Array<{ name: string; pass: boolean; detail: string }> = [];

function check(name: string, pass: boolean, detail: string): void {
  results.push({ name, pass, detail });
  console.log(`  ${pass ? 'PASS' : 'FAIL'}  ${name} — ${detail}`);
}

async function buy(
  chain: Chain,
  pdas: Pdas,
  who: KeyPairSigner,
  seeds: bigint,
  referrer: Address | null = null,
): Promise<string> {
  const ix = buyRigsInstruction(who.address, await minerPda(who.address), pdas, seeds, {
    identity: [],
    referrer: referrer ? { wallet: referrer, miner: await minerPda(referrer) } : null,
  });
  return submit(chain, who, ix);
}

/** True when `p` is rejected with Anchor error `code` (and nothing else). */
async function rejectedWith(p: Promise<unknown>, code: number): Promise<[boolean, string]> {
  try {
    await p;
    return [false, 'transaction succeeded'];
  } catch (e) {
    const text = describe(e);
    const hex = `0x${code.toString(16)}`;
    const hit = text.includes(hex) || new RegExp(`\\b${code}\\b`).test(text);
    return [
      hit,
      hit
        ? `rejected with ${code} (${hex})`
        : `rejected, but not with ${code}: ${text.slice(0, 300)}`,
    ];
  }
}

function describe(e: unknown): string {
  const parts: string[] = [];
  let cur: unknown = e;
  for (let i = 0; cur && i < 6; i++) {
    const o = cur as { message?: string; context?: unknown; cause?: unknown };
    parts.push(String(o.message ?? cur));
    try {
      parts.push(JSON.stringify(o.context, (_, v) => (typeof v === 'bigint' ? v.toString() : v)));
    } catch {
      /* context not serialisable */
    }
    cur = o.cause;
  }
  return parts.join(' | ');
}

async function blockTimeOf(chain: Chain, sig: string): Promise<bigint | null> {
  const tx = await chain.rpc
    .getTransaction(sig as never, {
      encoding: 'json',
      maxSupportedTransactionVersion: 0,
      commitment: 'confirmed',
    })
    .send();
  return tx?.blockTime == null ? null : BigInt(tx.blockTime);
}

/**
 * Claim for `who`; returns (net, penalty, bps, expectedBps) and checks vault
 * isolation. The expected band comes from the position's planting age — the
 * same pure function the program mirrors — so the assertion holds for any
 * claim time, not just the fresh-flow 25% case.
 */
async function claimChecked(chain: Chain, pdas: Pdas, who: KeyPairSigner, label: string) {
  const before = {
    cfg: (await fetchConfig(chain))!,
    miner: (await fetchMiner(chain, who.address))!,
    reward: await lamportsOf(chain, pdas.rewardVault),
    fee: await lamportsOf(chain, pdas.feeVault),
    capital: await lamportsOf(chain, pdas.capitalVault),
  };
  const claimNow = await chainTime(chain);
  const expectedBps = claimPenaltyBps(before.miner.lastPlantedAt, claimNow);
  await submit(chain, who, claimInstruction(who.address, await minerPda(who.address), pdas));
  const cfg = (await fetchConfig(chain))!;
  const miner = (await fetchMiner(chain, who.address))!;
  const net = miner.lifetimeSolClaimed - before.miner.lifetimeSolClaimed;
  const penalty = cfg.totalClaimPenalties - before.cfg.totalClaimPenalties;
  const gross = net + penalty;
  const bps = gross > 0n ? (penalty * 10_000n) / gross : 0n;
  check(
    `${label}: only the net leaves the reward vault`,
    before.reward - (await lamportsOf(chain, pdas.rewardVault)) === net,
    `net ${net}, penalty ${penalty}`,
  );
  check(
    `${label}: fee and capital vaults untouched`,
    (await lamportsOf(chain, pdas.feeVault)) === before.fee &&
      (await lamportsOf(chain, pdas.capitalVault)) === before.capital,
    'unchanged',
  );
  check(
    `${label}: claim does not move the planting clock`,
    miner.lastPlantedAt === before.miner.lastPlantedAt,
    `last_planted_at ${miner.lastPlantedAt}`,
  );
  return { net, penalty, bps, expectedBps, before, cfg };
}

/** Poll the chain until `who` can afford one seed from rewards, or give up. */
async function waitUntilReplantable(chain: Chain, pdas: Pdas, who: Address, maxSeconds: number) {
  const deadline = Date.now() + maxSeconds * 1000;
  for (;;) {
    const config = (await fetchConfig(chain))!;
    const miner = await fetchMiner(chain, who);
    const now = await chainTime(chain);
    const owed = projectedClaimable(
      {
        config,
        miner,
        rewardVaultDistributable: 0n,
        rewardVaultAddress: pdas.rewardVault,
        chainTimeSeconds: now,
        fetchedAtMs: 0,
      },
      now,
    );
    const seed = quotePurchase(
      1n,
      config.rigPriceLamports,
      BigInt(config.purchaseFeeBps),
    ).totalLamports;
    if (owed >= seed) return;
    if (Date.now() > deadline) {
      fail(
        `${who} accrued ${owed} lamports in ${maxSeconds}s; one seed costs ${seed}. ` +
          'Fund a faster schedule or use a devnet seed price that fits the rehearsal.',
      );
    }
    await sleep(15_000);
  }
}

async function replantChecked(chain: Chain, pdas: Pdas, who: KeyPairSigner, label: string) {
  const cfg0 = (await fetchConfig(chain))!;
  const m0 = (await fetchMiner(chain, who.address))!;
  await submit(chain, who, compoundInstruction(who.address, await minerPda(who.address), pdas));
  const cfg1 = (await fetchConfig(chain))!;
  const m1 = (await fetchMiner(chain, who.address))!;
  check(
    `${label}: replant pays no penalty`,
    cfg1.totalClaimPenalties === cfg0.totalClaimPenalties,
    `total_claim_penalties ${cfg1.totalClaimPenalties}`,
  );
  check(
    `${label}: replant keeps the planting clock`,
    m1.lastPlantedAt === m0.lastPlantedAt,
    `last_planted_at ${m1.lastPlantedAt}`,
  );
  check(`${label}: replant planted seeds`, m1.rigs > m0.rigs, `${m0.rigs} → ${m1.rigs}`);
}

async function solvency(chain: Chain, pdas: Pdas): Promise<void> {
  const c = (await fetchConfig(chain))!;
  const backing = await vaultDistributable(chain, pdas.rewardVault);
  const owedCap = c.totalRewardFunded - c.totalRewardClaimed - c.totalRewardCompounded;
  check(
    'reward vault backs funded − claimed − compounded',
    backing >= owedCap,
    `distributable ${backing} ≥ ${owedCap}`,
  );
  check(
    'paid ≤ funded',
    c.totalRewardClaimed + c.totalRewardCompounded <= c.totalRewardFunded,
    `claimed ${c.totalRewardClaimed} + compounded ${c.totalRewardCompounded} ≤ ${c.totalRewardFunded}`,
  );
}

/** What `activate --resume` was told to expect about the interrupted run. */
type ResumeOptions = { expectT0: bigint };

/**
 * Verify the chain is EXACTLY at the state an interrupted activate run leaves
 * behind, then hand control back to continue from the immutability probe.
 * Any mismatch is fatal: never guess where to resume.
 */
async function resumeActivate(
  chain: Chain,
  pdas: Pdas,
  cfg: Config,
  who: { founder: Address; walletA: Address },
  expectT0: bigint,
): Promise<bigint> {
  const walletB = await loadSigner(
    requireEnv('CHERRY_TREE_SMOKE_WALLET_B', 'Devnet wallet B keypair.'),
    'wallet B',
  );
  const authority = await loadSigner(
    requireEnv(
      'ANCHOR_WALLET',
      'Authority keypair path — its address must match config.authority.',
    ),
    'authority',
  );
  const rewardFunder = requireEnv(
    'CHERRY_TREE_REWARD_FUNDER',
    'PUBLIC KEY of the reward funder — must match config.reward_funder.',
  );

  const configOwner =
    (await chain.rpc.getAccountInfo(pdas.config, { encoding: 'base64' }).send()).value?.owner ??
    'account absent';
  const miner = await fetchMiner(chain, who.founder);
  const mismatches = activateResumeMismatches(
    {
      configOwner,
      founder: cfg.founder,
      authority: cfg.authority,
      rewardFunder: cfg.rewardFunder,
      launchT0: cfg.launchT0,
      paused: cfg.paused,
      totalRigs: cfg.totalRigs,
      totalShares: cfg.totalShares,
      minerCount: cfg.minerCount,
      totalCapitalReceived: cfg.totalCapitalReceived,
      totalProtocolFees: cfg.totalProtocolFees,
      totalRewardFunded: cfg.totalRewardFunded,
      totalRewardClaimed: cfg.totalRewardClaimed,
      totalRewardCompounded: cfg.totalRewardCompounded,
      rewardRate: cfg.rewardRate,
      rewardPeriodFinish: cfg.rewardPeriodFinish,
      founderMiner: miner && {
        rigs: miner.rigs,
        weight: miner.weight,
        createdAt: miner.createdAt,
        lastActionAt: miner.lastActionAt,
        lastPlantedAt: miner.lastPlantedAt,
        lifetimeSolClaimed: miner.lifetimeSolClaimed,
      },
      walletAMinerExists: (await fetchMiner(chain, who.walletA)) !== null,
      walletBMinerExists: (await fetchMiner(chain, walletB.address)) !== null,
      capitalVaultExcess: await vaultDistributable(chain, pdas.capitalVault),
      feeVaultExcess: await vaultDistributable(chain, pdas.feeVault),
      rewardVaultDistributable: await vaultDistributable(chain, pdas.rewardVault),
      chainNow: await chainTime(chain),
    },
    {
      programId: PROGRAM_ID,
      founder: who.founder,
      authority: authority.address,
      rewardFunder,
      expectT0,
      activationSeeds: 10n,
      activationCapital: cfg.rigPriceLamports * 10n,
      activationFees: (cfg.rigPriceLamports * 10n * BigInt(cfg.purchaseFeeBps)) / 10_000n,
      accessWindowSeconds: ACCESS_WINDOW_SECONDS,
    },
  );
  if (mismatches.length) {
    fail(
      `activate --resume: the chain is not the interrupted activate state. ` +
        `${mismatches.length} mismatch(es):\n    - ${mismatches.join('\n    - ')}`,
    );
  }
  check(
    'resume fingerprint: exactly the interrupted activate state',
    true,
    `T0 ${cfg.launchT0}, rigs ${cfg.totalRigs}, shares ${cfg.totalShares}, ` +
      `capital ${cfg.totalCapitalReceived}, fees ${cfg.totalProtocolFees}, ` +
      `claimed 0, compounded 0, schedule live`,
  );
  row('resuming', 'after the fingerprinted activation purchase, at the T0 probe');
  return cfg.launchT0;
}

async function stageActivate(
  chain: Chain,
  pdas: Pdas,
  waitSeconds: number,
  maxWaitSeconds: number,
  resume: ResumeOptions | null,
): Promise<void> {
  const founder = await loadSigner(
    requireEnv(
      'CHERRY_TREE_FOUNDER_KEYPAIR',
      'Devnet rehearsal founder keypair (never the mainnet founder).',
    ),
    'devnet rehearsal founder',
  );
  const walletA = await loadSigner(
    requireEnv('CHERRY_TREE_SMOKE_WALLET_A', 'Devnet wallet A keypair.'),
    'wallet A',
  );
  const cfg0 = (await fetchConfig(chain)) ?? fail('not initialised. Run devnet-init first.');
  if (cfg0.founder !== founder.address) {
    fail(`config.founder is ${cfg0.founder}, not the rehearsal founder ${founder.address}.`);
  }

  let t0: bigint;
  if (resume) {
    section('resume: strict state fingerprint');
    t0 = await resumeActivate(
      chain,
      pdas,
      cfg0,
      { founder: founder.address, walletA: walletA.address },
      resume.expectT0,
    );
  } else {
    if (cfg0.launchT0 !== 0n)
      fail(
        `already activated (T0 ${cfg0.launchT0}). Stage "activate" runs once. ` +
          'To continue an interrupted run: activate --resume --expect-t0 <unixSeconds>',
      );

    section('before T0');
    const [pre, preDetail] = await rejectedWith(buy(chain, pdas, walletA, 1n), ERR.AccessDenied);
    check('non-founder purchase before T0 is refused', pre, preDetail);

    section('founder activation');
    const sig = await buy(chain, pdas, founder, 10n);
    const cfg1 = (await fetchConfig(chain))!;
    const bt = await blockTimeOf(chain, sig);
    row('activation signature', sig);
    row('T0', cfg1.launchT0);
    check('T0 written by the founder purchase', cfg1.launchT0 > 0n, `launch_t0 ${cfg1.launchT0}`);
    check('T0 = activation block time', bt !== null && bt === cfg1.launchT0, `block time ${bt}`);
    t0 = cfg1.launchT0;
  }

  // The T0-immutability proof. After T0 the founder is an ordinary wallet:
  // without `.skr` proof the access gate refuses the purchase (AccessDenied)
  // and nothing lands, so T0 cannot move. A SUCCESSFUL second founder
  // purchase would prove the gate open, not that T0 is pinned.
  section('post-T0 founder probe (no .skr proof)');
  const cfgP0 = (await fetchConfig(chain))!;
  const [imm, immDetail] = await rejectedWith(buy(chain, pdas, founder, 1n), ERR.AccessDenied);
  const cfgP1 = (await fetchConfig(chain))!;
  check(
    'POST-T0 FOUNDER WITHOUT .SKR REFUSED + T0 UNCHANGED',
    imm && cfgP1.launchT0 === cfgP0.launchT0,
    `${immDetail}; launch_t0 ${cfgP1.launchT0} (was ${cfgP0.launchT0})`,
  );

  const [gen, genDetail] = await rejectedWith(buy(chain, pdas, walletA, 1n), ERR.AccessDenied);
  check('no-domain wallet refused in the .skr phase', gen, genDetail);

  section('accrue one seed, replant; accrue, claim');
  await waitUntilReplantable(chain, pdas, founder.address, maxWaitSeconds);
  await replantChecked(chain, pdas, founder, 'founder');
  await sleep(waitSeconds * 1000);
  const dust0 = (await fetchConfig(chain))!.undistributedDust;
  const c = await claimChecked(chain, pdas, founder, 'founder claim');
  check(
    'founder claim pays the band its planting age earns',
    c.bps === BigInt(c.expectedBps) || c.bps === BigInt(c.expectedBps) - 1n,
    `${c.bps} bps, band ${c.expectedBps} bps`,
  );
  check(
    'sole planter: penalty stays in the vault as dust',
    c.cfg.undistributedDust >= dust0 + c.penalty,
    `dust ${dust0} → ${c.cfg.undistributedDust}`,
  );
  await solvency(chain, pdas);

  row('.skr + .sns phase from', t0 + ACCESS_WINDOW_SECONDS);
  row('public phase from (run stage "public")', t0 + 2n * ACCESS_WINDOW_SECONDS);
}

async function stagePublic(
  chain: Chain,
  pdas: Pdas,
  waitSeconds: number,
  maxWaitSeconds: number,
): Promise<void> {
  const walletA = await loadSigner(
    requireEnv('CHERRY_TREE_SMOKE_WALLET_A', 'Devnet wallet A keypair.'),
    'wallet A',
  );
  const walletB = await loadSigner(
    requireEnv('CHERRY_TREE_SMOKE_WALLET_B', 'Devnet wallet B keypair.'),
    'wallet B',
  );
  if (walletA.address === walletB.address) fail('wallets A and B must be distinct.');
  const cfg0 = (await fetchConfig(chain)) ?? fail('not initialised.');
  const now = await chainTime(chain);
  if (cfg0.launchT0 === 0n) fail('not activated. Run stage "activate" first.');
  if (now < cfg0.launchT0 + 2n * ACCESS_WINDOW_SECONDS) {
    fail(
      `public phase opens at ${cfg0.launchT0 + 2n * ACCESS_WINDOW_SECONDS}; chain time is ${now}.`,
    );
  }
  if (now >= cfg0.rewardPeriodFinish) fail('the reward schedule has finished. Fund it first.');

  section('public planting and referral');
  if (await fetchMiner(chain, walletA.address))
    fail('wallet A already has a position; use fresh wallets.');
  if (await fetchMiner(chain, walletB.address))
    fail('wallet B already has a position; use fresh wallets.');
  await buy(chain, pdas, walletA, 10n);
  const a = (await fetchMiner(chain, walletA.address))!;
  check('A plants in the public phase at weight 100', a.weight === 100, `weight ${a.weight}`);

  await buy(chain, pdas, walletB, 10n, walletA.address);
  const b = (await fetchMiner(chain, walletB.address))!;
  check(
    'B plants referred by A at weight 105',
    b.weight === 105 && b.referrer === walletA.address,
    `weight ${b.weight}, referrer ${b.referrer}`,
  );
  const cfgB = (await fetchConfig(chain))!;
  check(
    'total shares = Σ seeds × weight includes the referral weight',
    cfgB.totalShares >= a.rigs * 100n + b.rigs * 105n,
    `total_shares ${cfgB.totalShares}`,
  );

  const [again, againDetail] = await rejectedWith(
    buy(chain, pdas, walletB, 1n, walletA.address),
    ERR.ReferrerAlreadyBound,
  );
  check('a second referrer is refused', again, againDetail);

  section('A accrues one seed and replants; B claims inside 24h');
  await waitUntilReplantable(chain, pdas, walletA.address, maxWaitSeconds);
  await replantChecked(chain, pdas, walletA, 'A');
  await sleep(waitSeconds * 1000);
  const c = await claimChecked(chain, pdas, walletB, 'B claim');
  check(
    'B claim pays the band its planting age earns',
    c.bps === BigInt(c.expectedBps) || c.bps === BigInt(c.expectedBps) - 1n,
    `${c.bps} bps, band ${c.expectedBps} bps`,
  );
  await solvency(chain, pdas);
}

function parseArgs(argv: string[]): {
  stage: string;
  waitSeconds: number;
  maxWaitSeconds: number;
  resume: ResumeOptions | null;
} {
  const stage = argv[0];
  const rest = argv.slice(1);
  let wantsResume = false;
  let expectT0 = 0n;
  const positional: string[] = [];
  for (let i = 0; i < rest.length; i++) {
    const arg = rest[i]!;
    if (arg === '--resume') {
      wantsResume = true;
    } else if (arg === '--expect-t0') {
      const value = rest[++i];
      if (value === undefined || !/^\d+$/.test(value)) {
        fail('--expect-t0 needs the T0 (unix seconds) already on chain, e.g. --expect-t0 1790293911');
      }
      expectT0 = BigInt(value);
    } else {
      positional.push(arg);
    }
  }
  if (stage !== 'activate' && stage !== 'public')
    fail(
      'usage: devnet-rehearsal.ts activate|public [waitSeconds] [maxReplantWaitSeconds] ' +
        '[activate only: --resume --expect-t0 <unixSeconds>]',
    );
  if (wantsResume && stage !== 'activate') fail('--resume applies to the activate stage only.');
  const resume = wantsResume ? { expectT0 } : null;
  if (resume && expectT0 <= 0n)
    fail('--resume needs --expect-t0 <unixSeconds>: the T0 the interrupted activation wrote.');
  const waitSeconds = Number(positional[0] ?? '45');
  const maxWaitSeconds = Number(positional[1] ?? '1800');
  if (!Number.isFinite(waitSeconds) || waitSeconds < 5) fail('waitSeconds must be at least 5');
  return { stage, waitSeconds, maxWaitSeconds, resume };
}

async function main(): Promise<void> {
  loadEnv();
  const { stage, waitSeconds, maxWaitSeconds, resume } = parseArgs(process.argv.slice(2));

  header(`THE CHERRY TREE — devnet rehearsal: ${stage}`);
  const { rpcUrl } = requireDevnet();
  const chain = connect(rpcUrl);
  await assertConnectedToDevnet(chain);
  row('program ID', PROGRAM_ID);
  const pdas = await programAddresses();

  if (stage === 'activate')
    await stageActivate(chain, pdas, waitSeconds, maxWaitSeconds, resume);
  else await stagePublic(chain, pdas, waitSeconds, maxWaitSeconds);

  const failed = results.filter((r) => !r.pass);
  section(`result: ${results.length - failed.length}/${results.length} PASS`);
  if (failed.length)
    fail(`${failed.length} check(s) failed: ${failed.map((f) => f.name).join('; ')}`);
}

main().catch((e) => {
  console.error(`\n  ERROR: ${e instanceof Error ? e.message : String(e)}`);
  process.exitCode = 1;
});
