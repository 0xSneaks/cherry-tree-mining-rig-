/**
 * Launch phases — a port of `rigs_engine::launch`, checked against the
 * engine's own vectors (`tests/launch.test.ts`).
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
  if (since < 2n * ACCESS_WINDOW_SECONDS) return 'skrSnsEarly';
  return 'public';
}

export function secondsToNextPhase(launchT0: bigint, now: bigint): bigint | null {
  switch (accessPhase(launchT0, now)) {
    case 'skrGenesis':
      return launchT0 + ACCESS_WINDOW_SECONDS - now;
    case 'skrSnsEarly':
      return launchT0 + 2n * ACCESS_WINDOW_SECONDS - now;
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

/** `HH:MM:SS`, or `Dd HH:MM:SS` beyond a day. */
export function formatCountdown(seconds: bigint): string {
  const s = seconds < 0n ? 0n : seconds;
  const d = s / 86_400n;
  const hh = ((s % 86_400n) / 3_600n).toString().padStart(2, '0');
  const mm = ((s % 3_600n) / 60n).toString().padStart(2, '0');
  const ss = (s % 60n).toString().padStart(2, '0');
  return d > 0n ? `${d}d ${hh}:${mm}:${ss}` : `${hh}:${mm}:${ss}`;
}
