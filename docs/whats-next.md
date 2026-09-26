# What's next

## Current (verified 2026-09-26)

The V2 release lane (`mainnet/rc-hardening`) implements: founder activation setting T0; a 24-hour `.skr`-only Genesis window; public access from exactly T0 + 24 hours with no `.sns` window; a cumulative per-wallet Genesis fresh-purchase cap during that window (compounds never consume it); 25/15/5% claim penalties over the first 72 hours; immutable first-purchase referral binding with 100/105 reward weights; and compounding that spends only whole affordable rigs while the remainder stays claimable.

Devnet qualification (Part A) passed on 2026-09-25. Mainnet activation is deliberately gated: qualification Part B runs at the final release commit, and T0 is set only by the founder's activation transaction through the runbook.

## Planned — the Fruit layer

Fruit lets seed-qualified wallets register externally held assets as additional reward weight. The design is approved and fully specified; implementation begins with task T30, and activation follows committed funding.

- Qualification: 150 current seeds plus registered, verified external equity.
- Day-1 `.skr` fresh-purchase maximum: 250 seeds — 2.5 SOL of principal, fees additional.
- Permanent Cherry holding cap of 250 current seeds — arriving with the next program version; compounding at the cap keeps unspent earned SOL fully claimable.
- Public opening `FRUIT_T0 = T0 + 24 hours`; no intervening window; Day-1 Fruit reserve spend is zero.
- External holdings are uncapped and never custodied; recognized reward weight is capped at 2.5 SOL-equivalent per wallet; one active Fruit per wallet per epoch.
- Asset candidates, activating as verification completes: wallet SKR, staked SKR, xSOL (with Safe Mode), JitoSOL, spot JLP. Leveraged/queued/vault JLP forms are excluded; xBTC and xHYPE stay disabled.
- Loyalty and lock bonuses draw on separately committed bonus funding.
- Referral stays one direct level; the fee-based commission cut is selected once the production fee configuration is final.
- Safe Mode (NORMAL / WARM / HOT / EXTREME) never erases accrued rewards.
- Every reward ledger stays fully solvent: V ≥ L + U + Z.

Full specification: [canonical Fruit tokenomics](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/z-ai/cherry-docs-roadmap/docs/CHERRY_TREE_FRUIT_TOKENOMICS.md).

## Stock baskets

A "stock baskets" phase (rewards in tokenized stocks) appears in early website copy as a future possibility; whether and how it is scoped is a founder decision.
