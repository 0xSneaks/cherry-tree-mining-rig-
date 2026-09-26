/**
 * Expected protocol failures, said in product language.
 *
 * The raw error is never swallowed: `explainActionError` returns it alongside
 * the product line so the caller can keep it in the developer console while
 * the screen stays readable on a phone.
 *
 * Anchor custom errors carry both a name (`Error Code: AccessDenied`) and a
 * number (`Error Number: 6023`, `6000 + position in error.rs`). The order
 * below is pinned to the program source by `tests/errors.test.ts`, exactly
 * the way the rehearsal pins its own codes — a reordered enum fails the test
 * rather than mistranslating a refusal.
 */

/** Declaration order of `RigsError` in `programs/sol-rigs/src/error.rs`. */
export const ANCHOR_ERROR_NAMES = [
  'MathOverflow',
  'InvalidConfig',
  'InvalidRigCount',
  'RigSupplyExceeded',
  'InvalidDuration',
  'InvalidFundingAmount',
  'RewardRateZero',
  'NothingToClaim',
  'NothingToCompound',
  'InsufficientRewardBacking',
  'ClockWentBackwards',
  'ProgramPaused',
  'UnauthorizedFunder',
  'UnauthorizedPendingAuthority',
  'MinerOwnerMismatch',
  'VaultKindMismatch',
  'VaultRentReserve',
  'InsufficientFeeBalance',
  'ZeroAmount',
  'FeeRecipientMismatch',
  'SelfReferral',
  'ReferrerAlreadyBound',
  'InvalidReferrer',
  'AccessDenied',
  'ActivationUnfunded',
  'MissingReferrerAccount',
  'GenesisCapExceeded',
  'MigrationRejected',
  'UnauthorizedInitializer',
  'WalletCapExceeded',
  'SkrDomainBound',
  'SkrBindingRequired',
  'SkrBindingInvalid',
] as const;

export type AnchorErrorName = (typeof ANCHOR_ERROR_NAMES)[number];

/** What the screen says instead of the raw Anchor error. */
const PRODUCT_COPY: Partial<Record<AnchorErrorName, string>> = {
  NothingToClaim: 'No harvest is currently available.',
  NothingToCompound: 'Not enough accrued rewards to plant another rig yet.',
  ProgramPaused: 'Cherry Tree actions are temporarily paused.',
  SelfReferral: 'A wallet cannot refer itself.',
  ReferrerAlreadyBound: 'A referrer is already bound to this grove and is never replaced.',
  InvalidReferrer: 'That referrer is not an existing planter — the link cannot be used.',
  AccessDenied: 'This wallet is not eligible during the current access phase.',
  ActivationUnfunded: 'The grove cannot be activated yet — the reward schedule needs funding first.',
  GenesisCapExceeded: 'This purchase exceeds your remaining Genesis fresh-buy allowance.',
  WalletCapExceeded: 'A wallet can hold at most 250 seeds.',
  SkrDomainBound: 'This .skr domain has already opened Day-1 access for another wallet.',
};

export type ExplainedError = {
  /** Product language, for the screen. */
  message: string;
  /** The raw error, for the developer console. */
  raw: string;
};

/**
 * Anchor error detection that refuses to guess. Names and explicit error
 * numbers only — a bare `6021` inside a lamport figure must never translate
 * a balance into a refusal.
 */
export function detectAnchorError(text: string): AnchorErrorName | null {
  const names = ANCHOR_ERROR_NAMES as readonly string[];
  const byName = text.match(/Error Code: ([A-Za-z]+)/)?.[1];
  if (byName && names.includes(byName)) return byName as AnchorErrorName;

  for (const pattern of [/Error Number: (\d{4})/g, /"Custom":(\d{4})/g]) {
    for (const match of text.matchAll(pattern)) {
      const index = Number(match[1]) - 6000;
      const name = ANCHOR_ERROR_NAMES[index];
      if (name) return name;
    }
  }
  return null;
}

/**
 * Translate an action failure. Known protocol errors become product copy;
 * anything unknown passes through unchanged rather than being papered over.
 */
export function explainActionError(e: unknown): ExplainedError {
  const raw = e instanceof Error ? `${e.name}: ${e.message}` : String(e);
  const name = detectAnchorError(raw);
  return { message: name ? (PRODUCT_COPY[name] ?? raw) : raw, raw };
}
