# PASS #6 — Architecture Decision Freeze (PROPOSED, not committed)

Scope: Pass #6 = canonical tests 40–46 (epochs / timeline / Fruit settlement).
Baseline: Pass #5 VERIFIED at `209bd6711b858ae0a69d12252acbe0a24cfd4fae`
(branch `z-ai/cherry-tree-pass5-pricing-safe-mode`).
Canonical source: `docs/tokenomics/CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md`
at that commit (below: "Arch"), plus `OPUS_CHERRY_TREE_TOKENOMICS_CLOSEOUT.md`
and the original adversarial review (`claude/cherry-tree-tokenomics-review-12fa4b`).

Out of scope, and not touched here: Loyalty, Locks, deployment, Cherry
submission, mainnet. Pass #5 invariants (FIFO lots, equal-stamp-only merging,
`min(ref, live)` addition pricing, the `ts <= epoch_start` reference rule, and
the Pending/Ineligible/Verified classification) are consumed as-is and not
reopened.

Status legend: **FROZEN** (already stated in canonical text) · **DERIVED**
(follows from canonical invariants; no new economics) · **FOUNDER DECISION
REQUIRED** · **DEFERRED OUTSIDE PASS #6**.

---

## D2 — Meaning of Test 41 — **DERIVED → answer B; test DEFERRED OUTSIDE PASS #6 (Lock package)**

Test 41: "Registration finalized before t_s is included; at t_s it is excluded."

Evidence:
- Arch §1: subscripts `i, p, s` denote wallet, **epoch** and **Season**. `t_s` is a Season symbol, not an epoch symbol.
- `t_s` is defined only in Arch §8 (Lock model): "For the five-day Season beginning at `t_s = FRUIT_T0 + 6 days`: Registration window is `[FRUIT_T0, t_s)` … registrations at or after t_s are excluded."
- Arch §12: "Lock election/cutoff: … finalize strictly before Season start."
- Original review test 31 (the ancestor of test 41) sits beside H-11 (time convention) and AC-9 (lock registration stores intent until `t_s`).
- A generic epoch-registration cutoff would contradict Arch §5: "A later join accrues only from its finalized registration" (joins are allowed mid-epoch) and "Switching is effective next epoch". Core Fruit has no registration cutoff that excludes anyone.

Decision: Test 41 is **B, the Lock registration cutoff** (`[FRUIT_T0, t_s)`, half-open, finalized order). It moves to the Lock package, next to tests 50–54 and 77. It is listed under "Epochs and timeline" only because it tests a timeline boundary.

Pass #6 effect: implement tests **40 and 42–46**. The only piece of test 41 that belongs to the timeline, the Season start `t_s = FRUIT_T0 + 518,400` as a half-open boundary, is already asserted by test 42. No lock registration state is built in Pass #6.

## D3 — Source of registration/selection — **DERIVED**

Evidence: Arch §3 ("if the **on-chain registration/selection program** … was unavailable"); Arch §5 ("accrues only from its **finalized** registration", "One selected asset per wallet per epoch", "Switching is effective next epoch; exiting is always permitted"); Arch §3 time convention (finalized only; order `(slot, tx, ix, inner ix)`; boundary equality belongs to the later interval); AC-10 (`Cn` counts wallets with a *registered* selection).

Decision:
1. **The only source of truth** is a finalized, wallet-signed on-chain registration instruction, ordered in the same global event timeline as ownership. App state, off-chain signatures, indexer caches, failed or dropped transactions and operator records are never registrations.
2. Semantics, all taken from Arch §5:
   - **Join** (wallet has no selection this epoch): effective from the join's finalized event. No backdating.
   - **Switch** (a different asset while registered): effective at the next epoch's `t_start`. The wallet keeps its current asset until then.
   - **Exit**: effective from its finalized event.
   - A re-join in the same epoch after an exit may only use the asset already selected in that epoch ("one per wallet per epoch"). A different asset waits for the next `t_start`.
   - Registration pricing is unchanged from Pass #5: at or before `epoch_start` it gets the reference price, after it gets `min(ref, live)` once per holder. D7 covers the next epoch.
3. Engine contract: the replay consumes `Register / Switch / Exit` events that carry `(slot, tx, ix, inner)` keys and come from that instruction. Pass #5's `EventKind::Register/Exit` stays. `Switch` is added as a pending selection applied at the boundary.

**DEFERRED OUTSIDE PASS #6:** building the on-chain registration instruction and its account. It is absent from the program today (`grep` finds no registration instruction in `programs/sol-rigs`), and Arch §10a's minimum state does not list it. Pass #6 tests 40–46 run on fixture events that carry this contract.

## D5 — Settlement rounding — **DERIVED**

Evidence: Arch §10 ("rational/fixed-point cumulative entitlement and carried remainders; never round each wallet upward"; capacity `floor(E·elapsed/T)`; "carry wallet fractions so checkpoint frequency does not destroy meaningful entitlement"); §10a invariants 3, 6, 7 and 8; §11 ("not per-wallet upward settlement rounding"); the existing `rigs-engine::fruit::scheduled_capacity`; tests 63 and 64.

Decision (integer only, `u128`, checked, no floats, matching `#![deny(clippy::float_arithmetic)]`):
- Units: weights are in lamports. `K_i = P_ref · min(S_i, 250)` with `P_ref = 10,000,000`. `w_i = min(K_i, X_i)` over Verified segments. `H_seg = max(Σ w_i, 12,500,000,000)`.
- Never compute `r = E/D` as a truncated integer.
- Per pool, per settled segment `[a, b)`: `acc_seg = floor(E · (b−a) · SCALE / (D · H_seg))` with `SCALE = 10^18`.
- Wallet leaf (cumulative): `leaf_i = floor( Σ_seg w_i,seg · acc_seg / SCALE )`. It is recomputed over the full settled history on every root, so it is order-independent and non-decreasing as more segments settle. No per-segment or per-checkpoint floor is applied to wallets.
- Proof sketch: `Σ_i w_i ≤ H_seg` gives `Σ_i leaf_i ≤ E·(settled elapsed)/D`, so the root total ≤ `scheduled_capacity(E, t_start, t_end, checkpoint)`. `w_i ≤ 2.5e9 = H_floor/5` gives `leaf_i ≤ floor(E/5) = wallet_cap`, which is also asserted.
- Headroom: the `acc` numerator is ≤ 8.6e9 · 9.5e5 · 1e18 ≈ 8e33 and the leaf numerator ≤ ~2e27. Both fit `u128` (3.4e38). Per-wallet drift from the floors in `acc` is ≤ `w/SCALE` per segment, below 1 lamport over any realistic segment count (test 63).
- Remainder: `E − final_total` (floor dust plus unallocated floor shortfall) stays in U until close, then U→Z exactly once (tests 44, 45, 64; invariant 8).

## D6 — Wallet-level PENDING and the shared denominator H — **DERIVED** (one sub-item DEFERRED)

Evidence: Arch §5 ("unreadable registry state … that wallet's position is **pending (section 10 reserve)**"; "Pool denominators must be reconstructed on the same timeline. Missing events can change every participant's share"); Arch §10 ("In an ownership/indexer gap affecting a shared denominator, retain the **entire unsettled scheduled capacity for the affected common pool interval**, minus only liabilities already conclusively established. Do not use `r Δt w_max/H_observed` with an unverified denominator"; terminal resolution); test 24; Pass #5 `Eligibility::Pending` = "unknown: later settlement must reserve".

Why canonical determines it: a Pending wallet with `K_i > 0` has an unknown `w_i ∈ [0, K_i]`, so `H_seg` is unverified and every other wallet's share in that segment is unverified too. That is exactly the §10 case.

Decision:
1. A segment is **settleable** only when it is `IntervalStatus::Verified` **and** no wallet in it is `Pending` with `K_i > 0` (seed-qualified `S_i ≥ 150` **and** registered in that segment). A Pending wallet with `K_i = 0` cannot change H and does not block the segment.
2. For a non-settleable segment, its **entire** scheduled capacity stays in U. No leaf includes that segment for anyone, and nothing is released at epoch end (invariant 8).
3. It settles only when a later replay classifies it with no blocking Pending. The formula then uses the true `H_seg`, and every wallet, the formerly pending one included, is paid by the D5 rule. The final amounts are the same whenever settlement happens. Only timing differs.
4. If it never resolves: Arch §10 terminal resolution (≥2 independent archive providers, veto-set signature). That can only add L or release to Z, never reduce L.
5. Pass #6 adds no new resolution semantics for specific Pass #5 Pending reasons (`RouteHalted`, `AdapterProgramUnknown`, `NavUnverified`). They stay reserved until the eligibility engine resolves them.

Consequence, recorded so it isn't hidden: one seed-qualified registered wallet whose debt registry is unreadable holds back **everyone's** settlement for that segment until it resolves. Solvency is safe and payment is delayed, not reduced.

**DEFERRED OUTSIDE PASS #6:**
- (a) Optional early settlement of provable lower bounds, `w_j · acc` computed with `H_upper = max(Cw_known + Σ K_pending, 12.5e9)`. Canonical allows it as "conclusively established", but no Pass #6 test needs it.
- (b) Settlement mapping for the Safe Mode / adapter Pending reasons in item 5. It belongs with adapter re-verification and Safe Mode recovery.

## D7 — Pricing across epoch boundaries — **DERIVED**

Evidence: Arch §5 ("reference price is committed on-chain **before the epoch starts** … **Units present at the reference timestamp use the reference price.** Units added or registered after it are valued at `min(reference, verified price at the addition event)`"; missing reference = inactive for that epoch, weight 0). Arch §9 ("never reprice past liabilities"; recovery requires a published prospective segment). Arch §3 (boundary equality belongs to the later interval). Pass #5 `ReferenceCommit` (`committed_at < epoch_start`; `ts <= epoch_start` is reference-priced). NEW-4 note (`rebase_mint` reprices every lot to one price, so age order is value-neutral).

Decision:
1. Each epoch `p` has its own `EpochReferences_p`, committed before `t_start(p)`. Segments are split at every `t_start`, and each segment is valued with its own epoch's references only.
2. At `t_start(p+1)`, every lot of an asset that has a `p+1` reference, including lots acquired in `p` at `min(ref_p, live)` and lots that were unpriced in `p`, re-bases to `ref_{p+1}` using the existing `rebase_mint` semantics (one price, age-neutral FIFO). Units acquired at exactly `t_start(p+1)` are present at the reference timestamp and get `ref_{p+1}`, matching Pass #5 `ts <= epoch_start`. After it, the `min(ref_{p+1}, live)` rule applies. Registration anchors are per epoch: a wallet registered at `t_start(p+1)` is reference-priced for `p+1`.
3. Asset without a `p+1` commit: Ineligible (`InactiveNoReference`) for all of `p+1`, known before it starts. Other assets settle normally (test 32 carried forward).
4. The boundary never touches settled liabilities or prior-epoch segments. Values in epoch `p` never use `ref_{p+1}`.
5. Safe Mode state crosses the boundary unchanged. A new epoch reference does **not** by itself clear `RouteHalted`, an active EXTREME reset or `AdapterProgramUnknown`. Only the existing Pass #5 recovery and verification events clear them. A still-active emergency NAV stays authoritative over the new reference until a published recovery.
6. A selection switch (D3) takes effect at `t_start(p+1)` and is priced under item 2.

Within an epoch, Pass #5 FIFO and pricing behavior is unchanged.

## D10 — Evidence for a registration-program outage — **DERIVED**

Evidence: Arch §3 ("if the on-chain registration/selection program or the asset's committed reference was unavailable, the interval is **not earned** …; if on-chain state existed but only indexer/data observation failed, the interval **is earned** and settles on replay, with the common-pool reserve held meanwhile"; "do not spend or retroactively catch up unearned capacity"); AC-6; Arch §5 (accrual only from finalized registration); test 46.

Decision:
1. **No separate outage attestation exists or is accepted.** The classification follows from finalized chain state alone:
   - *Not earned (program outage):* no finalized registration exists for the wallet in `[a, b)` because none could land. The wallet accrues from its first later finalized registration only. Failed transactions, app logs, signed intents and operator statements are never evidence of registration, and nothing is backdated. During `[a, b)` the wallet is simply absent from `Cw`. Other wallets settle by the normal formula, floor shortfall goes U→Z at close, and nothing is caught up later.
   - *Earned on replay (indexer outage):* the finalized registration exists on chain at `t1` but was not observed. The interval stays Pending/NotReconciled (Pass #5) with the whole common-pool reserve held (D6). Once the event is found, the wallet earns from `t1`.
2. **Guard:** a segment may be treated as "not registered" only after its history is reconciled (a Verified checkpoint in Pass #5 terms). Absence of an observed registration inside an unreconciled window is Pending, never "not earned". Terminal resolution (§10) applies if history cannot be recovered.
3. Wallets already registered keep accruing during a registration-program outage. Their registration is existing on-chain state, and accrual needs no per-interval program execution. A missing asset reference remains the per-epoch Ineligible rule (D7 item 3).

Test 46 fixture: wallet A's registration lands at `t2` after an outage. A earns only from `t2`, with no catch-up for `[t1, t2)`. Wallet B's registration finalized at `t1` but the indexer missed it. B is reserved, then earns from `t1` on replay, and the reserve releases correctly.

---

## Summary

| ID | Topic | Status |
|---|---|---|
| D2 | Test 41 meaning | DERIVED: B (Lock cutoff). Test 41 DEFERRED OUTSIDE PASS #6. Pass #6 = tests 40, 42–46 |
| D3 | Registration/selection source | DERIVED. On-chain instruction build DEFERRED OUTSIDE PASS #6 |
| D5 | Settlement rounding | DERIVED |
| D6 | Wallet PENDING vs shared H | DERIVED. Lower-bound early settlement and Safe-Mode Pending mapping DEFERRED OUTSIDE PASS #6 |
| D7 | Pricing across epoch boundaries | DERIVED |
| D10 | Registration-outage evidence | DERIVED |

No new economic parameter, rate, cap, threshold or split is introduced.

PASS #6 DECISIONS READY — NO FOUNDER DECISIONS REQUIRED
