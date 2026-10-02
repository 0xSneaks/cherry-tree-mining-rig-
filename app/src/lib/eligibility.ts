/**
 * Fruit eligibility disclosures (architecture section 5).
 *
 * Mirrors `NON_REGISTRY_DEBT_DISCLOSURE` in
 * `crates/eligibility-engine/src/lib.rs` verbatim; the Rust test t28 fails
 * if the two drift.
 */
export const NON_REGISTRY_DEBT_DISCLOSURE =
  'Borrowed-funds check: Cherry nets only debt it can read in a fixed list of approved lending programs. It cannot detect borrowing in other programs, loans from other wallets, exchange credit or off-chain loans, so it does not prove that an asset was not bought with borrowed money.';
