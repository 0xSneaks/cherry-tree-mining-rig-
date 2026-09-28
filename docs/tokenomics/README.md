# Frozen tokenomics architecture (reference)

Source of truth for implementation. Copied verbatim from docs revision
`46433cc` on `claude/cherry-tree-tokenomics-review-12fa4b` (frozen
architecture `b6e91df` + founder single 5-day lock correction `3036392` +
founder-approved fee amendment §7a, tests 78–91). Do not edit here; changes
to the architecture happen on the docs branch and are re-imported.

- `CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md` — the frozen specification.
- `OPUS_CHERRY_TREE_TOKENOMICS_CLOSEOUT.md` — closeout, verdict, §12 fee addendum.
- `CHERRY_TREE_FEE_OPTIMIZATION_MODEL.md` — APPROVED / FINAL fee model (§7a source).

Implementation pass #1: permanent 250-seed wallet cap (§4 check 2), capped
compound, Genesis cap pinned to 250, Day-1 `.skr` one-domain → one-wallet
binding (§4); tests 1, 2, 3, 4 (engine part), 5, 8 (preflight), 9–15 of §14.3.

Implementation pass #2: on-chain Fruit ledger foundation (§10 / §10a) —
CORE / LOYALTY / LOCK ledgers and vaults, `V >= L + U + Z`, roots with
challenge window, Merkle claims, close, pause, versioning; tests 55–67, 73–76.

Implementation pass #3: protocol fee implementation (§7a) — independent
compound fee, 72-hour fee timelock, 30/20/10/0 early-claim fee split 60/40,
20/80 referral commission with the 8% retained-margin clamp, reserved in FEE
and claimed by `claim_commission`, `withdraw_fees` reservation guard, 80/20
lock-forfeit release at lock-pool close, Config V3 migration, TypeScript
mirrors and disclosures; tests 6 (fee half), 68–72, 78–87, 89–91.

Implementation pass #4: ownership, encumbrance and debt eligibility
foundation (§5) in the off-chain `crates/eligibility-engine` — global
finalized event timeline `(slot, tx, ix, inner)` with half-open intervals,
unit-level encumbrance (delegation, freeze, owner change, Token-2022 risk
extensions, collateral / vesting, pending unstake, duplicate receipts),
fixed debt registry with wallet-level netting `X = max(0, G - D)`, PENDING
on unreadable or unpinned state, whole-pool UNRESOLVED windows on missing
history, SKR (wallet + verified stake) and xSOL (owned-token NAV) adapter
foundations, and the non-registry-debt disclosure in the product; tests
16–29. Production adapter / registry inputs remain gate 14.2-D
configuration (see `crates/eligibility-engine/README.md`).
