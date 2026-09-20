/**
 * Display formatting. Every function takes lamports as `bigint` and produces a
 * string — no lamport value is ever converted to `number` for arithmetic.
 */
import { LAMPORTS_PER_SOL } from './program/constants';

/** `0.018291` — fixed decimals, no rounding surprises, no float math. */
export function formatSol(lamports: bigint, decimals = 6): string {
  const negative = lamports < 0n;
  const abs = negative ? -lamports : lamports;
  const whole = abs / LAMPORTS_PER_SOL;
  const frac = abs % LAMPORTS_PER_SOL;
  const fracStr = frac.toString().padStart(9, '0').slice(0, decimals);
  const sign = negative ? '-' : '';
  return decimals > 0 ? `${sign}${whole}.${fracStr}` : `${sign}${whole}`;
}

/** Thousands separators for rig counts. */
export function formatCount(n: bigint | number): string {
  return n.toLocaleString('en-US');
}

/** `0.294%` — a share of the global mine, to two decimal places. */
export function formatSharePercent(part: bigint, total: bigint): string {
  if (total === 0n) return '0.000%';
  // Scale before dividing so the integer division keeps three decimals.
  const scaled = (part * 100_000n) / total;
  const whole = scaled / 1000n;
  const frac = (scaled % 1000n).toString().padStart(3, '0');
  return `${whole}.${frac}%`;
}

/** `Sep 27 · 18:42 UTC` */
export function formatUtcInstant(unixSeconds: bigint): string {
  const d = new Date(Number(unixSeconds) * 1000);
  if (Number.isNaN(d.getTime())) return '—';
  const month = d.toLocaleString('en-US', { month: 'short', timeZone: 'UTC' });
  const day = d.getUTCDate();
  const hh = d.getUTCHours().toString().padStart(2, '0');
  const mm = d.getUTCMinutes().toString().padStart(2, '0');
  return `${month} ${day} · ${hh}:${mm} UTC`;
}

/** `6d 04h` — the funded runway. */
export function formatDuration(seconds: bigint): string {
  if (seconds <= 0n) return 'ended';
  const days = seconds / 86_400n;
  const hours = (seconds % 86_400n) / 3_600n;
  const mins = (seconds % 3_600n) / 60n;
  if (days > 0n) return `${days}d ${hours.toString().padStart(2, '0')}h`;
  if (hours > 0n) return `${hours}h ${mins.toString().padStart(2, '0')}m`;
  return `${mins}m`;
}

/** `3.00%` from basis points. */
export function formatBps(bps: number): string {
  return `${(bps / 100).toFixed(2)}%`;
}

/** `7xKX…gAsU` */
export function shortAddress(a: string): string {
  return a.length <= 10 ? a : `${a.slice(0, 4)}…${a.slice(-4)}`;
}
