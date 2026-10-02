/**
 * The devnet rehearsal recognises refusals by Anchor error code. Those codes
 * are positional (6000 + index in `error.rs`), so a reordered enum would make
 * the rehearsal accept the wrong refusal. Pin them to the source.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { PROGRAM_ID } from '../src/lib/program/constants';
import {
  activateResumeMismatches,
  type ActivateResumeObserved,
} from '../scripts/ops/rehearsal-resume';

const root = join(import.meta.dirname, '..', '..');
const scriptPath = join(root, 'app', 'scripts', 'ops', 'devnet-rehearsal.ts');
const script = () => readFileSync(scriptPath, 'utf8');

describe('devnet rehearsal error codes', () => {
  it('match the program error enum', () => {
    const rs = readFileSync(join(root, 'programs', 'sol-rigs', 'src', 'error.rs'), 'utf8');
    const body = rs.slice(rs.indexOf('#[error_code]'));
    const enumBody = body.slice(body.indexOf('{') + 1, body.indexOf('\n}'));
    const variants = enumBody
      .split('\n')
      .map((l) => l.trim())
      .filter((l) => /^[A-Z][A-Za-z]+,?$/.test(l))
      .map((l) => l.replace(',', ''));
    const code = (name: string) => 6000 + variants.indexOf(name);

    const ts = readFileSync(join(root, 'app', 'scripts', 'ops', 'devnet-rehearsal.ts'), 'utf8');
    for (const name of ['ReferrerAlreadyBound', 'AccessDenied']) {
      expect(variants).toContain(name);
      const m = ts.match(new RegExp(`${name}: (\\d+)`));
      expect(Number(m?.[1]), name).toBe(code(name));
    }
  });

  it('is devnet-guarded and never loads the mainnet founder', () => {
    const ts = script();
    expect(ts).toContain('requireDevnet()');
    expect(ts).toContain('assertConnectedToDevnet(chain)');
    expect(ts).not.toMatch(/EXPECTED_FOUNDER|FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp/);
  });
});

// ------------------------------------- the post-T0 founder probe (6023 fix)
//
// After T0 the founder is an ordinary wallet: without `.skr` proof the
// access gate refuses the purchase and nothing lands. The rehearsal must
// EXPECT that refusal — a plain second founder buy "succeeding" was the
// devnet-blocking defect.

describe('devnet rehearsal post-T0 founder probe', () => {
  it('runs the second founder purchase through the controlled refusal path', () => {
    const ts = script();
    expect(ts).toContain('rejectedWith(buy(chain, pdas, founder, 1n), ERR.AccessDenied)');
    // No unguarded founder purchase may remain anywhere in the stage.
    expect(ts).not.toMatch(/await buy\(chain, pdas, founder, 1n\)/);
  });

  it('proves T0 immutability as refusal + unchanged T0', () => {
    const ts = script();
    expect(ts).toContain('POST-T0 FOUNDER WITHOUT .SKR REFUSED + T0 UNCHANGED');
    // The T0 compared across the probe is re-read from chain after it.
    expect(ts).toMatch(/cfgP1\.launchT0 === cfgP0\.launchT0/);
  });

  it('does not weaken the gate: no founder exemption is expected or asserted', () => {
    const ts = script();
    expect(ts).not.toMatch(/is_founder|isFounder|founderExempt|exempt/i);
  });
});

// -------------------------------------------------------- the resume path

describe('devnet rehearsal resume CLI', () => {
  it('keeps ordinary activate one-shot and points interrupted runs at --resume', () => {
    const ts = script();
    expect(ts).toContain("cfg0.launchT0 !== 0n");
    expect(ts).toContain('--resume needs --expect-t0');
    expect(ts).toContain("'--resume applies to the activate stage only.'");
  });

  it('checks the fingerprint through the pure module, never ad-hoc', () => {
    const ts = script();
    expect(ts).toContain("from './rehearsal-resume'");
    expect(ts).toContain('activateResumeMismatches(');
  });

  it('computes the expected claim band from planting age, not a hardcoded 25%', () => {
    const ts = script();
    expect(ts).toContain('claimPenaltyBps(');
    expect(ts).not.toMatch(/c\.bps === 2_500n/);
  });
});

describe('activate resume fingerprint', () => {
  const FOUNDER = '4XLyo1zKkRsNcpv2pW1uXj7EwNFGTxVBB7GES9T3mD1H';
  const AUTHORITY = 'B1o2B2zcvYntZCrAmnz4uPzzXgNs4LnLs4evsRUSST31';
  const FUNDER = 'HoC5J7No7w4Wqmt281Di7yxLGzZUQNSpozri2ryU3ABm';
  const T0 = 1_790_293_911n;

  /** The exact state the interrupted devnet activate run left behind. */
  const interrupted = (): ActivateResumeObserved => ({
    configOwner: PROGRAM_ID,
    founder: FOUNDER,
    authority: AUTHORITY,
    rewardFunder: FUNDER,
    launchT0: T0,
    paused: false,
    totalRigs: 10n,
    totalShares: 1000n,
    minerCount: 1n,
    totalCapitalReceived: 10_000_000n,
    totalProtocolFees: 300_000n,
    totalRewardFunded: 1_000_000_000n,
    totalRewardClaimed: 0n,
    totalRewardCompounded: 0n,
    rewardRate: 3858n,
    rewardPeriodFinish: T0 + 259_200n,
    founderMiner: {
      rigs: 10n,
      weight: 100,
      createdAt: T0,
      lastActionAt: T0,
      lastPlantedAt: T0,
      lifetimeSolClaimed: 0n,
    },
    walletAMinerExists: false,
    walletBMinerExists: false,
    capitalVaultExcess: 10_000_000n,
    feeVaultExcess: 300_000n,
    rewardVaultDistributable: 1_000_000_000n,
    chainNow: T0 + 600n,
  });

  const expectation = () => ({
    programId: PROGRAM_ID,
    founder: FOUNDER,
    authority: AUTHORITY,
    rewardFunder: FUNDER,
    expectT0: T0,
    activationSeeds: 10n,
    activationCapital: 10_000_000n,
    activationFees: 300_000n,
    accessWindowSeconds: 86_400n,
  });

  it('accepts exactly the interrupted activate state', () => {
    expect(activateResumeMismatches(interrupted(), expectation())).toEqual([]);
  });

  it('names every broken field rather than the first one', () => {
    const broken = interrupted();
    broken.totalRigs = 11n;
    broken.paused = true;
    const mismatches = activateResumeMismatches(broken, expectation());
    expect(mismatches).toHaveLength(2);
  });

  const refuses = (label: string, mutate: (o: ActivateResumeObserved) => void, needle: RegExp) =>
    it(`refuses when ${label}`, () => {
      const observed = interrupted();
      mutate(observed);
      const mismatches = activateResumeMismatches(observed, expectation());
      expect(mismatches.length).toBeGreaterThan(0);
      expect(mismatches.join('\n')).toMatch(needle);
    });

  refuses('the T0 differs from the expected one', (o) => (o.launchT0 = T0 + 1n), /launch_t0/);
  refuses('T0 is unset', (o) => (o.launchT0 = 0n), /launch_t0/);
  refuses('another program owns the config', (o) => (o.configOwner = 'SomeOtherProgram1111111111111111111'), /program ID/);
  refuses('the pinned founder changed', (o) => (o.founder = 'AnotherWallet1111111111111111111111111111111'), /config\.founder/);
  refuses('the authority changed', (o) => (o.authority = 'AnotherWallet1111111111111111111111111111111'), /config\.authority/);
  refuses('the reward funder changed', (o) => (o.rewardFunder = 'AnotherWallet1111111111111111111111111111111'), /reward_funder/);
  refuses('the program is paused', (o) => (o.paused = true), /paused/);
  refuses('rigs are not the activation 10', (o) => (o.totalRigs = 11n), /total_rigs/);
  refuses('shares are not seeds × 100', (o) => (o.totalShares = 900n), /total_shares/);
  refuses('more than the founder planted', (o) => (o.minerCount = 2n), /miner_count/);
  refuses('capital accounting moved', (o) => (o.totalCapitalReceived = 9_000_000n), /total_capital_received/);
  refuses('fee accounting moved', (o) => (o.totalProtocolFees = 0n), /total_protocol_fees/);
  refuses('something was already claimed', (o) => (o.totalRewardClaimed = 1n), /total_reward_claimed/);
  refuses('something was already compounded', (o) => (o.totalRewardCompounded = 1n), /total_reward_compounded/);
  refuses('the reward schedule finished', (o) => (o.rewardPeriodFinish = o.chainNow), /schedule funded and live/);
  refuses('the founder miner is absent', (o) => (o.founderMiner = null), /founder miner: got absent/);
  refuses('the founder miner holds other rigs', (o) => (o.founderMiner!.rigs = 11n), /founder miner rigs/);
  refuses('anything landed after the activation', (o) => (o.founderMiner!.lastActionAt = T0 + 10n), /last_action_at/);
  refuses('the founder already claimed', (o) => (o.founderMiner!.lifetimeSolClaimed = 1n), /lifetime_sol_claimed/);
  refuses('wallet A already has a position', (o) => (o.walletAMinerExists = true), /wallet A miner/);
  refuses('wallet B already has a position', (o) => (o.walletBMinerExists = true), /wallet B miner/);
  refuses('the capital vault was drained', (o) => (o.capitalVaultExcess = 5_000_000n), /capital vault excess/);
  refuses('the fee vault was drained', (o) => (o.feeVaultExcess = 0n), /fee vault excess/);
  refuses('the reward vault no longer backs its liabilities', (o) => (o.rewardVaultDistributable = 1n), /reward vault backs/);
  refuses('the chain left the .skr window the stage reasons about', (o) => (o.chainNow = T0 + 86_400n), /\.skr window/);
});
