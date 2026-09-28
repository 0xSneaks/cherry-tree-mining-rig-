# eligibility-engine

Off-chain ownership, encumbrance and debt eligibility for the Fruit model
(architecture §5, canonical tests 16–29). Pure integer Rust, no Solana
dependencies, not linked into the on-chain program. The indexer feeds it
finalized events; later passes (pricing, Safe Mode, epochs, Fruit roots)
consume its per-wallet `VERIFIED / INELIGIBLE / PENDING` output.

```text
X_i(t) = max(0, sum_j A_ij(t) G_ij(t) - D_i(t))
```

## What it guarantees

- **Timeline.** Events are totally ordered by
  `(slot, tx index, ix index, inner index)`; each slot has one
  `unix_timestamp`; state holds on half-open `[t_i, t_{i+1})`. Same-timestamp
  states (one transaction, or distinct slots on one clock second) have zero
  duration: no overlap, no carousel or flash-loan weight.
- **Verification.** An interval is VERIFIED only when a checkpoint of
  observed finalized state matches the replay. Divergence (a mismatch, an
  event the token program would refuse, a declared gap) marks the **whole
  common-pool window** since the last verified checkpoint UNRESOLVED:
  every participant PENDING. Intervals after the last checkpoint are PENDING.
- **Encumbrance.** Recognized units = `amount - delegated_amount -
  encumbered`, 0 if frozen. `SetAuthority(AccountOwner)` is a transfer and
  clears the delegate. Token-2022 permanent delegate / transfer hook /
  confidential transfer make the asset INELIGIBLE unless the adapter clears
  the extension. Pending unstake and duplicate receipts never count.
- **Debt.** Registry readings are netted once per wallet across the fixed
  registry, from their event onward. Unread, unreadable or unpinned registry
  state makes the wallet PENDING. Debt in programs outside the registry is
  **not detected**; `NON_REGISTRY_DEBT_DISCLOSURE` says so and the app shows
  it verbatim (`app/src/lib/eligibility.ts`).
- **Pins.** Every pinned external program (adapter or registry) is UNKNOWN
  until a finalized version observation matches its pin, and again from any
  later mismatching observation.
- **xSOL** is valued at owned-token NAV (`PriceBasis::OwnedTokenNav`);
  underlying leveraged notional and plain reference prices are refused.

## Launch scope

SKR and xSOL only. JitoSOL, spot JLP, xBTC and xHYPE are framework-supported
but refused by `AdapterConfig::validate` (`AssetNotEnabled`); leveraged /
Multiply / queued / vault JLP is `AssetExcluded`.

## Deferred configuration (gate 14.2-D)

Nothing below is embedded; tests use `Addr::fixture` values only.

- SKR mint address and token program (SPL vs Token-2022, extension set).
- SKR staking: program ID, live IDL / account layout, beneficiary rules,
  cooldown / epoch rules, withdraw path and custody owner(s). Until verified
  the SKR adapter must use `StakeSupport::Unverified` (stake → PENDING); no
  staking layout exists in this repository.
- xSOL mint, Hylo program ID(s) and deployed version pins; owned-token NAV
  source.
- Debt registry: the approved lending programs, their pinned versions, and
  decoders producing per-wallet debt readings.
- Price / NAV sources, confidence and window thresholds (Pass #5).
- Indexer: archive providers and checkpoint cadence.
