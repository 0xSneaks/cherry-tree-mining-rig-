/**
 * Production environment readiness — a pure check over an env map.
 *
 * Used by `scripts/ops/check-production-env.ts` and by tests. It validates
 * shape and consistency only. It cannot know whether an identifier was
 * really issued by Cherry: CHERRY_APP_ID comes from Cherry mini-app
 * registration at portal.cherry.fun, and this check never invents or accepts
 * a placeholder for it.
 */
import { isAddress } from '@solana/kit';

import { parseOfficialReferrer } from '../growth/referral';
import { parseQualification } from '../growth/referralStats';
import { parseSeasons, validateSeasons } from '../growth/seasons';

export type Level = 'PASS' | 'FAIL' | 'WARN' | 'EXTERNAL';

export type EnvFinding = { level: Level; key: string; detail: string };

type Env = Record<string, string | undefined>;

const PLACEHOLDER = /^(|x+|todo|tbd|changeme|placeholder|your[-_ ].*|<.*>|test|example.*|sol-rigs-e2e|sol-rigs-test-app)$/i;
const DEVNET_RPC = /devnet|testnet|localhost|127\.0\.0\.1/i;

function httpsUrl(v: string | undefined): boolean {
  if (!v) return false;
  try {
    return new URL(v).protocol === 'https:';
  } catch {
    return false;
  }
}

export function checkProductionEnv(env: Env, opts: { protocolVaults?: string[] } = {}): EnvFinding[] {
  const out: EnvFinding[] = [];
  const add = (level: Level, key: string, detail: string) => out.push({ level, key, detail });

  // ---- cluster consistency
  const cluster = env.NEXT_PUBLIC_SOLANA_CLUSTER ?? '';
  const rpc = env.NEXT_PUBLIC_SOLANA_RPC_URL ?? '';
  if (cluster !== 'mainnet-beta') {
    add('FAIL', 'NEXT_PUBLIC_SOLANA_CLUSTER', `is '${cluster || '(unset)'}', production requires mainnet-beta`);
  } else add('PASS', 'NEXT_PUBLIC_SOLANA_CLUSTER', 'mainnet-beta');
  if (!httpsUrl(rpc)) add('FAIL', 'NEXT_PUBLIC_SOLANA_RPC_URL', 'must be an https URL');
  else if (DEVNET_RPC.test(rpc)) add('FAIL', 'NEXT_PUBLIC_SOLANA_RPC_URL', 'points at devnet/testnet/local');
  else add('PASS', 'NEXT_PUBLIC_SOLANA_RPC_URL', 'https, not devnet');
  if ((env.NEXT_PUBLIC_EXPLORER_CLUSTER ?? cluster) !== cluster) {
    add('FAIL', 'NEXT_PUBLIC_EXPLORER_CLUSTER', 'differs from NEXT_PUBLIC_SOLANA_CLUSTER');
  }
  if (env.SOLANA_RPC_URL && DEVNET_RPC.test(env.SOLANA_RPC_URL)) {
    add('FAIL', 'SOLANA_RPC_URL', 'server-side indexer RPC points at devnet/testnet/local');
  }

  // ---- Cherry mini-app (launch token)
  const appId = (env.CHERRY_APP_ID ?? '').trim();
  if (appId === '') {
    add('EXTERNAL', 'CHERRY_APP_ID', 'not set — obtain from Cherry mini-app registration (portal.cherry.fun). /api/session fails closed until then.');
  } else if (PLACEHOLDER.test(appId)) {
    add('FAIL', 'CHERRY_APP_ID', 'looks like a placeholder or test value');
  } else add('PASS', 'CHERRY_APP_ID', 'set (authenticity can only be confirmed by a real Cherry launch)');
  if (!env.CHERRY_EXPECTED_ORIGIN) {
    add('WARN', 'CHERRY_EXPECTED_ORIGIN', 'unset — launch tokens are not pinned to the deployed origin');
  } else if (!httpsUrl(env.CHERRY_EXPECTED_ORIGIN)) {
    add('FAIL', 'CHERRY_EXPECTED_ORIGIN', 'must be an https origin');
  } else add('PASS', 'CHERRY_EXPECTED_ORIGIN', env.CHERRY_EXPECTED_ORIGIN);
  if (env.CHERRY_JWKS_URL && !httpsUrl(env.CHERRY_JWKS_URL)) {
    add('FAIL', 'CHERRY_JWKS_URL', 'must be https (the e2e JWKS stub must never reach production)');
  }

  // ---- secrets must never be public
  for (const k of Object.keys(env)) {
    if (k.startsWith('NEXT_PUBLIC_') && /SECRET|PRIVATE|KEYPAIR|MNEMONIC|SEED_PHRASE/i.test(k)) {
      add('FAIL', k, 'a secret-looking variable is NEXT_PUBLIC_ and would ship in the browser bundle');
    }
  }
  const chatId = env.NEXT_PUBLIC_CHERRY_CHAT_APP_ID;
  if (chatId && !env.CHERRY_APP_SECRET) {
    add('WARN', 'CHERRY_APP_SECRET', 'chat embed configured without its server secret — chat stays read-only');
  }
  if (env.STATIC_EXPORT === '1' && (chatId || env.NEXT_PUBLIC_CHERRY_CHAT_ROOM_ID)) {
    add('FAIL', 'STATIC_EXPORT', 'static export cannot serve the chat embed token routes');
  }

  // ---- growth layer
  const official = parseOfficialReferrer(env.NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER);
  if (official.status === 'unconfigured') {
    add('EXTERNAL', 'NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER', 'unset — the official fallback is off (fails closed). Founder decision.');
  } else if (official.status === 'invalid') {
    add('FAIL', 'NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER', 'not a valid wallet address');
  } else {
    add('PASS', 'NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER', `${official.wallet} (must be an existing planter before it can bind)`);
  }
  const q = parseQualification(env.CHERRY_TREE_REFERRAL_MIN_FRESH_SEEDS, env.CHERRY_TREE_REFERRAL_MIN_HOLD_HOURS);
  if (!q) {
    add('EXTERNAL', 'CHERRY_TREE_REFERRAL_MIN_FRESH_SEEDS / _MIN_HOLD_HOURS', 'unset — qualified referrals and the leaderboard stay closed. Founder decision.');
  } else add('PASS', 'CHERRY_TREE_REFERRAL_MIN_*', `≥ ${q.minFreshSeeds} fresh seeds, held ≥ ${q.minHoldSeconds / 3_600n}h`);
  try {
    const seasons = parseSeasons(env.CHERRY_TREE_SEASONS_JSON);
    const errs = validateSeasons(seasons, opts.protocolVaults ?? []);
    if (errs.length) for (const e of errs) add('FAIL', 'CHERRY_TREE_SEASONS_JSON', e);
    else add(seasons.length ? 'PASS' : 'WARN', 'CHERRY_TREE_SEASONS_JSON', `${seasons.length} season(s)`);
  } catch (e) {
    add('FAIL', 'CHERRY_TREE_SEASONS_JSON', e instanceof Error ? e.message : 'unparseable');
  }
  for (const k of ['CHERRY_TREE_RUNWAY_LOW_DAYS', 'CHERRY_TREE_RUNWAY_CRITICAL_DAYS']) {
    const v = env[k];
    if (v !== undefined && v !== '' && !/^\d+$/.test(v)) add('FAIL', k, 'must be a whole number of days');
  }
  for (const k of (env.CHERRY_TREE_EXCLUDED_REFERRERS ?? '').split(/[\s,]+/).filter(Boolean)) {
    if (!isAddress(k)) add('FAIL', 'CHERRY_TREE_EXCLUDED_REFERRERS', `${k} is not an address`);
  }

  // ---- server-side stores (rate limiting, room attribution)
  if (env.CHERRY_ALLOW_INPROCESS_STORE === '1') {
    add('WARN', 'CHERRY_ALLOW_INPROCESS_STORE', 'in-process stores acknowledged — correct only for a single-process deployment');
  } else {
    add('EXTERNAL', 'CHERRY_SERVER_STORE', 'no durable server store chosen — rate-limited endpoints and room attribution fail closed in production. Founder decision.');
  }
  return out;
}

export function summarize(findings: readonly EnvFinding[]): { ok: boolean; external: number; fail: number } {
  const fail = findings.filter((f) => f.level === 'FAIL').length;
  return { ok: fail === 0, fail, external: findings.filter((f) => f.level === 'EXTERNAL').length };
}
