/**
 * PRODUCTION ENV CHECK — offline, read-only. Reads the process environment
 * (and the repo .env / app/.env.production.local if present, without printing
 * values that could be secret) and reports PASS / FAIL / WARN / EXTERNAL.
 *
 *   npx tsx scripts/ops/check-production-env.ts [path/to/env-file]
 *
 * EXTERNAL means the value can only come from outside this repository (Cherry
 * registration, a founder decision). Exit 1 on any FAIL.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { checkProductionEnv, summarize } from '../../src/lib/ops/envCheck';
import { header, row, section } from './common';

function readEnvFile(path: string): Record<string, string> {
  const out: Record<string, string> = {};
  let raw: string;
  try {
    raw = readFileSync(path, 'utf8');
  } catch {
    return out;
  }
  for (const line of raw.split('\n')) {
    const t = line.trim();
    if (!t || t.startsWith('#')) continue;
    const eq = t.indexOf('=');
    if (eq < 1) continue;
    out[t.slice(0, eq).trim()] = t.slice(eq + 1).trim().replace(/^["']|["']$/g, '');
  }
  return out;
}

const file = process.argv[2] ? resolve(process.argv[2]) : resolve(import.meta.dirname, '..', '..', '.env.production.local');
const env = { ...readEnvFile(file), ...process.env };

header('THE CHERRY TREE — production environment check (offline)');
row('env file', file);
const findings = checkProductionEnv(env);
section('findings');
for (const f of findings) row(`${f.level.padEnd(8)} ${f.key}`, f.detail);
const s = summarize(findings);
section('result');
row('FAIL', s.fail);
row('EXTERNAL (founder / Cherry action)', s.external);
if (!s.ok) process.exitCode = 1;
