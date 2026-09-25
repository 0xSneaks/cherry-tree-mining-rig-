/**
 * Product-facing branding, in one place.
 *
 * Product copy calls the game's production units "Seeds" — players plant
 * Seeds, not rigs. The on-chain program keeps its original technical
 * identifiers (`sol_rigs`, `rigs-engine`, the PDA seed strings and the
 * program ID) — renaming those would change instruction and account
 * discriminators and invalidate the deployed address for no user-visible
 * benefit.
 */

/** Full display name. Used for the wordmark and the document title. */
export const PRODUCT_NAME = 'THE CHERRY TREE';

/** Shorter reference, for running prose. */
export const PRODUCT_NAME_SHORT = 'The Cherry Tree';

/** Slug for packages, scripts and env-var prefixes. */
export const PRODUCT_SLUG = 'cherry-tree';

export const PRODUCT_DESCRIPTION =
  'A Solana-native mining simulation with SOL-denominated rewards, paid from a funded reward vault.';
