/**
 * Launch phases — a port of `rigs_engine::launch`, checked against the
 * engine's own vectors (`tests/launch.test.ts`).
 *
 * Production V2 access: `.skr` holders only for the first 24 hours after T0,
 * then PUBLIC FOREVER. There is no `.sns` gate and no second early-access
 * window; `'skrSnsEarly'` survives in the union only to keep the phase codes
 * (order matches `AccessPhase::code()`) stable with the engine.
 *
 * Every input is on-chain: `launch_t0` from the Config account and the chain
 * clock the app anchors to `getBlockTime`. Browser time never decides a
 * phase — and the phase shown here is only a display of the gate the program
 * enforces in `buy_rigs` regardless of what any client believes.
 */
export const ACCESS_WINDOW_SECONDS = 86_400n;

export type AccessPhase = 'preLaunch' | 'skrGenesis' | 'skrSnsEarly' | 'public';

/** Order matches `AccessPhase::code()`. */
export const ACCESS_PHASES: readonly AccessPhase[] = [
  'preLaunch',
  'skrGenesis',
  'skrSnsEarly',
  'public',
];

export function accessPhase(launchT0: bigint, now: bigint): AccessPhase {
  if (launchT0 <= 0n || now < launchT0) return 'preLaunch';
  const since = now - launchT0;
  if (since < ACCESS_WINDOW_SECONDS) return 'skrGenesis';
  return 'public';
}

export function secondsToNextPhase(launchT0: bigint, now: bigint): bigint | null {
  switch (accessPhase(launchT0, now)) {
    case 'skrGenesis':
      return launchT0 + ACCESS_WINDOW_SECONDS - now;
    // 'skrSnsEarly' is unreachable under V2; nothing follows it anyway.
    default:
      return null;
  }
}

export type Identity = 'skr' | 'sns' | 'none';

/** `launch::purchase_allowed` — display only; the program is the gate. */
export function purchaseAllowed(phase: AccessPhase, identity: Identity): boolean {
  switch (phase) {
    case 'preLaunch':
      return false;
    case 'skrGenesis':
      return identity === 'skr';
    case 'skrSnsEarly':
      return identity === 'skr' || identity === 'sns';
    case 'public':
      return true;
  }
}

/**
 * Fresh rigs this wallet may still buy under the Genesis cap, or `null` once
 * the launch is Public (T0 + 24h onward) and the cap no longer applies — and
 * for a V1 Config, which carries no cap at all. Display only — the program
 * enforces `rigs_engine::genesis_cap_check`.
 */
export function genesisAllowance(
  launchT0: bigint,
  now: bigint,
  lifetimeRigsBought: bigint,
  genesisCapRigs: bigint | null,
): bigint | null {
  if (accessPhase(launchT0, now) === 'public') return null;
  if (genesisCapRigs === null) return null;
  const remaining = genesisCapRigs - lifetimeRigsBought;
  return remaining < 0n ? 0n : remaining;
}

/** `HH:MM:SS`, or `Dd HH:MM:SS` beyond a day. */
export function formatCountdown(seconds: bigint): string {
  const s = seconds < 0n ? 0n : seconds;
  const d = s / 86_400n;
  const hh = ((s % 86_400n) / 3_600n).toString().padStart(2, '0');
  const mm = ((s % 3_600n) / 60n).toString().padStart(2, '0');
  const ss = (s % 60n).toString().padStart(2, '0');
  return d > 0n ? `${d}d ${hh}:${mm}:${ss}` : `${hh}:${mm}:${ss}`;
}
