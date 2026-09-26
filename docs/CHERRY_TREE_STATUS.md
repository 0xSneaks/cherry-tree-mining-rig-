# THE CHERRY TREE — Status / Handoff

**Keep this file short and operational.** Update it at the end of every working session (roadmap §1, step 7). Last updated: 2026-09-26.

## Snapshot

| Field | Value |
|---|---|
| Docs branch | `z-ai/cherry-docs-roadmap` (this file's introducing commit; base `e807b55` = `claude/cherry-tree-tokenomics-cfoilj`) |
| Source verification basis | Release lane `mainnet/rc-hardening` @ `f0d5e96` and `main` @ `85ee955`, verified 2026-09-26; engine blobs match the architecture's citations |
| Current phase | Phase 0 — documentation baseline: **COMPLETE**. No production code changed anywhere. |
| Last VERIFIED task(s) | T00.1 canonical tokenomics · T00.2 roadmap/decisions/status · T00.3 README entry point (evidence: the commit adding these files) |
| Active task | None in progress |

## Blockers (see [decision register](CHERRY_TREE_DECISIONS.md))

- **D01** borrowed/encumbered-capital enforcement — gates all adapter admission.
- **D02** ownership/indexer trust & correction model — gates settlement.
- **D03** base schedule/balances/net Fruit budget F — gates epoch commitment.
- **D04** production price/fee — gates production initialize.
- **D05** referral m and c — gates fee-backed commissions.
- **D06** bonus funding (J, B_loy, B_lock) — gates loyalty/lock activation.
- **D07** legacy over-cap positions — expected none; evidence produced inside T30.
- **D08/D09** asset verification & risk-overlay calibration — gates adapters/overlays.
- **D10** existing release gates (custody, APP_ID, host, Seeker, audit, Part B qualification; Part A evidence lags lane HEAD by 14 commits) — gates all mainnet readiness.

No founder-set constant is listed as unresolved (closed decisions C-01…C-20).

## NEXT ACTIONABLE TASK — exactly one

**T30 — Gated implementation spec: permanent 250-seed Cherry holding cap + capped compounding.**

- Why this one: it is `READY` (only dependency T00.1 is VERIFIED), it needs no founder input to start, and it sits on the critical path of every later mainnet gate (the current V2 genesis cap expires at Public and does **not** enforce the permanent cap; no holding-cap code exists on any branch).
- Scope and acceptance: [roadmap task T30](CHERRY_TREE_IMPLEMENTATION_ROADMAP.md#t30--gated-implementation-spec-permanent-cap--capped-compounding--ready--next-actionable-task). Begin with fresh pre-ops on the release lane (HEAD may have moved past `f0d5e96`) and the D07 no-over-cap-positions verification.
- Owner: Z.ai (author), Opus (spec review), founder (version-boundary note).
- Permitted in parallel (roadmap §2): T10, T11, T12, T13, T40, T71.

## Required inputs for the next phase after T30

- Founder: D01 trust-model selection, D03 funding plan, D04 price/fee values, D06 bonus commitments (or explicit zero), D10 custody/APP_ID/host/audit actions.
- None of these block starting T30.

## Evidence locations

- Canonical docs: this directory (`CHERRY_TREE_FRUIT_TOKENOMICS.md`, `CHERRY_TREE_IMPLEMENTATION_ROADMAP.md`, `CHERRY_TREE_DECISIONS.md`, this file).
- Full derivation/scenarios: [`CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md`](CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md), [`FABLE_CHERRY_TREE_TOKENOMICS_SOLVE.md`](FABLE_CHERRY_TREE_TOKENOMICS_SOLVE.md) (traceable references).
- Release-lane runbooks/decision artifacts: `docs/backlog/` on `mainnet/rc-hardening`.
- Qualification evidence (private, not published): local archive `/home/bronzesneaks/cherry-qualification-evidence/` — Part A PASS 2026-09-25 @ `9a5e746`, Part B NOT EXECUTED. Reference by manifest only; do not copy key material into the repository.
