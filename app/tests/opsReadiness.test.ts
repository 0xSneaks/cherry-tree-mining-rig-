/**
 * Production readiness tooling: env check (CHERRY_APP_ID fail-closed and
 * friends) and the read-only custody audit.
 */
import { getAddressEncoder, getAddressDecoder, type Address } from '@solana/kit';
import { describe, expect, it } from 'vitest';

import { checkProductionEnv, summarize } from '../src/lib/ops/envCheck';
import {
  evaluateCustody,
  parsePolicyAddress,
  programDataAddressFrom,
  upgradeAuthorityFrom,
  type CustodyInput,
} from '../src/lib/ops/custody';
import { SYS, WALLET_1, WALLET_2, WALLET_3, WALLET_4 } from './growthFixtures';

const good = {
  NEXT_PUBLIC_SOLANA_CLUSTER: 'mainnet-beta',
  NEXT_PUBLIC_SOLANA_RPC_URL: 'https://rpc.example-provider.com/abc',
  NEXT_PUBLIC_EXPLORER_CLUSTER: 'mainnet-beta',
  CHERRY_APP_ID: 'app_7f3c9a',
  CHERRY_EXPECTED_ORIGIN: 'https://cherrytree.example',
  NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER: WALLET_3,
  CHERRY_TREE_REFERRAL_MIN_FRESH_SEEDS: '5',
  CHERRY_TREE_REFERRAL_MIN_HOLD_HOURS: '72',
};

const level = (env: Record<string, string | undefined>, key: string) =>
  checkProductionEnv(env).filter((f) => f.key.startsWith(key)).map((f) => f.level);

describe('production env check', () => {
  it('passes a complete configuration', () => {
    const s = summarize(checkProductionEnv(good));
    expect(s.ok).toBe(true);
    expect(s.external).toBe(0);
  });

  it('CHERRY_APP_ID absent is an EXTERNAL action, never invented', () => {
    expect(level({ ...good, CHERRY_APP_ID: undefined }, 'CHERRY_APP_ID')).toEqual(['EXTERNAL']);
    expect(level({ ...good, CHERRY_APP_ID: '' }, 'CHERRY_APP_ID')).toEqual(['EXTERNAL']);
  });

  it('rejects placeholder and test app ids', () => {
    for (const v of ['changeme', 'TODO', '<app-id>', 'sol-rigs-e2e', 'your-app-id', 'xxx']) {
      expect(level({ ...good, CHERRY_APP_ID: v }, 'CHERRY_APP_ID')).toEqual(['FAIL']);
    }
  });

  it('rejects devnet endpoints and cluster mismatch', () => {
    expect(level({ ...good, NEXT_PUBLIC_SOLANA_CLUSTER: 'devnet' }, 'NEXT_PUBLIC_SOLANA_CLUSTER')).toEqual(['FAIL']);
    expect(level({ ...good, NEXT_PUBLIC_SOLANA_RPC_URL: 'https://api.devnet.solana.com' }, 'NEXT_PUBLIC_SOLANA_RPC_URL')).toEqual(['FAIL']);
    expect(level({ ...good, NEXT_PUBLIC_SOLANA_RPC_URL: 'http://rpc.x' }, 'NEXT_PUBLIC_SOLANA_RPC_URL')).toEqual(['FAIL']);
    expect(level({ ...good, NEXT_PUBLIC_EXPLORER_CLUSTER: 'devnet' }, 'NEXT_PUBLIC_EXPLORER_CLUSTER')).toEqual(['FAIL']);
  });

  it('rejects the e2e JWKS stub and non-https origins', () => {
    expect(level({ ...good, CHERRY_JWKS_URL: 'http://127.0.0.1:3519/jwks.json' }, 'CHERRY_JWKS_URL')).toEqual(['FAIL']);
    expect(level({ ...good, CHERRY_EXPECTED_ORIGIN: 'http://x' }, 'CHERRY_EXPECTED_ORIGIN')).toEqual(['FAIL']);
  });

  it('refuses secret-looking NEXT_PUBLIC_ variables', () => {
    expect(level({ ...good, NEXT_PUBLIC_CHERRY_APP_SECRET: 'x' }, 'NEXT_PUBLIC_CHERRY_APP_SECRET')).toEqual(['FAIL']);
  });

  it('growth config: unset is EXTERNAL (fail closed), invalid is FAIL', () => {
    expect(level({ ...good, NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER: '' }, 'NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER')).toEqual(['EXTERNAL']);
    expect(level({ ...good, NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER: 'nope' }, 'NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER')).toEqual(['FAIL']);
    expect(level({ ...good, CHERRY_TREE_REFERRAL_MIN_FRESH_SEEDS: '0' }, 'CHERRY_TREE_REFERRAL_MIN')).toEqual(['EXTERNAL']);
    expect(level({ ...good, CHERRY_TREE_SEASONS_JSON: '[{"id":"BAD","name":"x","startsAt":"2","endsAt":"1","competitions":["referrals"]}]' }, 'CHERRY_TREE_SEASONS_JSON')).toContain('FAIL');
    expect(level({ ...good, CHERRY_TREE_SEASONS_JSON: 'not json' }, 'CHERRY_TREE_SEASONS_JSON')).toEqual(['FAIL']);
    expect(level({ ...good, CHERRY_TREE_EXCLUDED_REFERRERS: `${WALLET_1}, nope` }, 'CHERRY_TREE_EXCLUDED_REFERRERS')).toEqual(['FAIL']);
  });
});

describe('custody audit', () => {
  it('fixture keys: WALLET_4 is an on-curve key', async () => {
    const { isOffCurveAddress } = await import('@solana/kit');
    expect(isOffCurveAddress(WALLET_4)).toBe(false);
  });

  async function realPda(seed: string): Promise<Address> {
    const { getProgramDerivedAddress } = await import('@solana/kit');
    const [pda] = await getProgramDerivedAddress({ programAddress: WALLET_3, seeds: [seed] });
    return pda;
  }

  it('flags single-key upgrade and config authorities', () => {
    const input: CustodyInput = {
      upgradeAuthority: WALLET_4,
      configAuthority: WALLET_4,
      pendingAuthority: SYS,
      rewardFunder: WALLET_3,
      feeRecipient: WALLET_2,
      founder: WALLET_2,
    };
    const checks = evaluateCustody(input, {
      expectedUpgradeAuthority: null,
      expectedConfigAuthority: null,
      expectedRewardFunder: null,
      expectedFeeRecipient: null,
    });
    const fails = checks.filter((c) => c.level === 'FAIL').map((c) => c.name);
    expect(fails).toContain('upgrade authority is a single key');
    expect(fails).toContain('config authority is a single key');
    expect(checks.some((c) => c.name === 'fee recipient is the founder activation wallet')).toBe(true);
    expect(checks.some((c) => c.name === 'human separation (not checkable)')).toBe(true);
  });

  it('passes a separated multisig layout and enforces separation rules', async () => {
    const up = await realPda('upgrade');
    const cfg = await realPda('config');
    const fund = await realPda('funder');
    const fee = await realPda('fees');
    const policy = {
      expectedUpgradeAuthority: up,
      expectedConfigAuthority: cfg,
      expectedRewardFunder: fund,
      expectedFeeRecipient: fee,
    };
    const ok = evaluateCustody(
      { upgradeAuthority: up, configAuthority: cfg, pendingAuthority: SYS, rewardFunder: fund, feeRecipient: fee, founder: WALLET_2 },
      policy,
    );
    expect(ok.filter((c) => c.level === 'FAIL')).toEqual([]);

    const merged = evaluateCustody(
      { upgradeAuthority: up, configAuthority: cfg, pendingAuthority: WALLET_1, rewardFunder: cfg, feeRecipient: cfg, founder: WALLET_2 },
      policy,
    );
    const names = merged.map((c) => `${c.level}:${c.name}`);
    expect(names).toContain('FAIL:funder separation');
    expect(names).toContain('FAIL:reward funder');
    expect(names).toContain('WARN:pending authority');

    const feeIsFunder = evaluateCustody(
      { upgradeAuthority: up, configAuthority: cfg, pendingAuthority: SYS, rewardFunder: fund, feeRecipient: fund, founder: WALLET_2 },
      policy,
    );
    expect(feeIsFunder.some((c) => c.level === 'FAIL' && c.name === 'fee separation')).toBe(true);

    const founderFunds = evaluateCustody(
      { upgradeAuthority: up, configAuthority: cfg, pendingAuthority: SYS, rewardFunder: WALLET_2, feeRecipient: fee, founder: WALLET_2 },
      { ...policy, expectedRewardFunder: null },
    );
    expect(founderFunds.some((c) => c.level === 'FAIL' && c.name === 'founder separation')).toBe(true);
  });

  it('revoked upgrade authority', async () => {
    const cfg = await realPda('config');
    const base = { configAuthority: cfg, pendingAuthority: SYS, rewardFunder: WALLET_4, feeRecipient: WALLET_1, founder: WALLET_2 };
    const empty = { expectedConfigAuthority: null, expectedRewardFunder: null, expectedFeeRecipient: null };
    expect(evaluateCustody({ ...base, upgradeAuthority: null }, { ...empty, expectedUpgradeAuthority: 'revoked' })[0]!.level).toBe('PASS');
    expect(evaluateCustody({ ...base, upgradeAuthority: null }, { ...empty, expectedUpgradeAuthority: null })[0]!.level).toBe('WARN');
    expect(evaluateCustody({ ...base, upgradeAuthority: WALLET_1 }, { ...empty, expectedUpgradeAuthority: 'revoked' })[0]!.level).toBe('FAIL');
  });

  it('parses loader state', () => {
    const enc = (a: Address) => new Uint8Array(getAddressEncoder().encode(a));
    const dec = (b: Uint8Array) => getAddressDecoder().decode(b) as Address;
    const program = new Uint8Array(36);
    program.set([2, 0, 0, 0], 0);
    program.set(enc(WALLET_1), 4);
    expect(programDataAddressFrom(program, dec)).toBe(WALLET_1);
    expect(() => programDataAddressFrom(new Uint8Array(36), dec)).toThrow();

    const pd = new Uint8Array(45);
    pd.set([3, 0, 0, 0], 0);
    pd[12] = 1;
    pd.set(enc(WALLET_2), 13);
    expect(upgradeAuthorityFrom(pd, dec)).toBe(WALLET_2);
    pd[12] = 0;
    expect(upgradeAuthorityFrom(pd.subarray(0, 13), dec)).toBeNull();
    pd[12] = 7;
    expect(() => upgradeAuthorityFrom(pd, dec)).toThrow();
  });

  it('policy addresses parse strictly', () => {
    expect(parsePolicyAddress(undefined)).toBeNull();
    expect(parsePolicyAddress('nope')).toBeNull();
    expect(parsePolicyAddress(` ${WALLET_1} `)).toBe(WALLET_1);
  });
});
