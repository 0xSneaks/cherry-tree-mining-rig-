# Frozen tokenomics architecture (reference)

Source of truth for implementation. Copied verbatim from the frozen docs
revision `3036392` (freeze `b6e91df` + founder single-5-day-lock correction) on `claude/cherry-tree-tokenomics-review-12fa4b`
(verdict: PASS FOR IMPLEMENTATION). Do not edit here; changes to the
architecture happen on the docs branch and are re-imported.

- `CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md` — the frozen specification.
- `OPUS_CHERRY_TREE_TOKENOMICS_CLOSEOUT.md` — closeout and verdict.

Implementation pass #1 (this branch): permanent 250-seed wallet cap
(§4 check 2), capped compound, Genesis cap pinned to 250, Day-1 `.skr`
one-domain → one-wallet binding (§4); tests 1, 2, 3, 4 (engine part), 5, 8
(preflight), 9, 10, 11, 12, 13, 14, 15 of §14.3 (gate now 1–77).
