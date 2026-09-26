# What's next

## Current (verified 2026-09-26)

The V2 release lane (`mainnet/rc-hardening`) implements: founder activation setting T0; a 24-hour `.skr`-only Genesis window; public access from exactly T0 + 24 hours with no `.sns` window; a cumulative per-wallet Genesis fresh-purchase cap during that window (compounds never consume it); 25/15/5% claim penalties over the first 72 hours; immutable first-purchase referral binding with 100/105 reward weights; and compounding that spends only whole affordable rigs while the remainder stays claimable.

Devnet qualification (Part A) passed on 2026-09-25. Mainnet qualification (Part B) has not run. Nothing is deployed, initialized, funded, or scheduled on mainnet; T0 is not set.

## Planned — the Fruit layer (adopted design, not implemented)

Fruit lets seed-qualified wallets register externally held assets as additional reward weight. The design is approved; no Fruit code exists on any branch yet, no Fruit funding exists, and no launch date has been chosen.

- Qualification: 150 current seeds plus registered, verified external equity.
- Day-1 `.skr` fresh-purchase maximum: 250 seeds — 2.5 SOL of principal, fees additional.
- Permanent Cherry holding cap of 250 current seeds — adopted policy that still requires a new program version; compounding at the cap must keep unspent earned SOL claimable.
- Public opening `FRUIT_T0 = T0 + 24 hours`; no intervening window; Day-1 Fruit reserve spend is zero.
- External holdings are uncapped and never custodied; recognized reward weight is capped at 2.5 SOL-equivalent per wallet; one active Fruit per wallet per epoch.
- Assets under consideration (none live): wallet SKR, staked SKR (conditional), xSOL (with Safe Mode), JitoSOL, spot JLP. Leveraged/queued/vault JLP forms are excluded; xBTC and xHYPE stay disabled.
- Loyalty and lock bonuses exist only with separately committed funding; none is committed.
- Referral stays one direct level; a fee-based commission cut is undecided until fee margins and production fees are fixed.
- Safe Mode (NORMAL / WARM / HOT / EXTREME) never erases accrued rewards.
- Every reward ledger must stay solvent: V ≥ L + U + Z.

Full specification: [canonical Fruit tokenomics](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/z-ai/cherry-docs-roadmap/docs/CHERRY_TREE_FRUIT_TOKENOMICS.md).

## Stock baskets

Early website copy mentions a "stock baskets" phase (rewards in tokenized stocks) as planned, not live. It is not part of the approved Fruit architecture; whether and how it is scoped is an open founder decision.
