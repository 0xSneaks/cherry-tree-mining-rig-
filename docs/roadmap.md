# Roadmap and status

Last verified: 2026-09-26. Source: release lane `mainnet/rc-hardening` (`f0d5e96`) and the canonical documentation branch `z-ai/cherry-docs-roadmap`.

## Where the plan lives

The repository maintains the plan of record:

- [Implementation roadmap](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/z-ai/cherry-docs-roadmap/docs/CHERRY_TREE_IMPLEMENTATION_ROADMAP.md) — 33 dependency-ordered tasks with acceptance criteria and evidence rules.
- [Decision register](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/z-ai/cherry-docs-roadmap/docs/CHERRY_TREE_DECISIONS.md) — open decisions D01–D11; founder-set decisions C-01–C-20.
- [Status / handoff](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/z-ai/cherry-docs-roadmap/docs/CHERRY_TREE_STATUS.md) — exactly one next actionable task at any time.

## Phase

Documentation baseline complete. Implementation has not started. The single next task is **T30**: the gated implementation specification for the permanent 250-seed holding cap and capped compounding on the release lane.

## Open decisions (summary)

| ID | Question |
|---|---|
| D01 | How owned external equity is distinguished from borrowed/encumbered capital, enforceably |
| D02 | Ownership settlement / indexer trust and correction model |
| D03 | Actual base schedule, balances, and net Fruit budget F |
| D04 | Production seed price and purchase fee |
| D05 | Referral margin m and commission cut c |
| D06 | Bonus funding commitments (or explicit zero) |
| D07 | Legacy over-cap positions (expected none; verified during T30) |
| D08 | Asset identity and reference-price verification |
| D09 | Safe Mode overlay calibration |
| D10 | Existing mainnet readiness gates (custody, app registration, host, audit, qualification Part B) |
| D11 | "Stock baskets" teaser disposition |

Founder-set values (Day-1 maximum 250 seeds, qualification 150 seeds, `FRUIT_T0 = T0 + 24h`, zero Day-1 Fruit spend, and the others) are settled and listed in the register; they are not re-opened.
