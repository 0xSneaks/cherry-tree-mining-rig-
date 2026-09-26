# THE CHERRY TREE — Implementation Roadmap (Master)

Date: 2026-09-26. Branch: `z-ai/cherry-docs-roadmap`.

This is the single dependency-ordered plan from this point onward. It integrates the tokenomics program ([canonical spec](CHERRY_TREE_FRUIT_TOKENOMICS.md)), the decision register ([decisions](CHERRY_TREE_DECISIONS.md)) and the pre-existing release/mainnet readiness gates (`docs/backlog/MAINNET_RUNBOOK.md` and siblings on `mainnet/rc-hardening`). It replaces no runbook: runbook gates are preserved and referenced, not duplicated.

Source basis (verified 2026-09-26): docs branch base `claude/cherry-tree-tokenomics-cfoilj` @ `e807b55`; release lane `mainnet/rc-hardening` @ `f0d5e96` (engine blobs cited by the architecture match exactly); `main` @ `85ee955` (V1, code-identical to the docs branch); qualification Part A PASS at `9a5e746` (14 commits behind lane HEAD), Part B NOT EXECUTED. No Fruit code exists on any branch. No production deployment, initialization, funding, or T0 exists.

**Status legend:** `NOT STARTED` · `READY` (dependencies VERIFIED, work may begin) · `IN PROGRESS` · `BLOCKED` (a decision/input gates even starting) · `VERIFIED` (acceptance criteria have recorded evidence — a plan, merged code, or an agent's success message is insufficient).

---

## 1. Operating rules for all future sessions (strict workflow)

1. Read applicable repository instructions, [STATUS](CHERRY_TREE_STATUS.md), this roadmap, and relevant entries in the [decision register](CHERRY_TREE_DECISIONS.md).
2. Verify current branch/HEAD and preserve unrelated work; never reset, overwrite, or switch away from another agent's unfinished branch/worktree.
3. Select the next `READY` task whose dependencies are all `VERIFIED` (or the exact task named as NEXT ACTIONABLE TASK in STATUS).
4. Work only within that task's scope.
5. Run its required checks.
6. Record evidence and update the task status (`VERIFIED` only with evidence).
7. Update STATUS with the next actionable task.
8. Commit scoped changes and report the result.

Hard rules:

- Do not skip gates, silently change economics, mark blocked work complete, or drift into unrelated features.
- Independent tasks may proceed only when this roadmap explicitly permits their parallelism (§2).
- Blocked production activation does not prohibit isolated research or fixture-based tests — but those are never production verification and must never be presented as such.
- Changes to approved economics or dependency order require a recorded rationale and the appropriate founder decision. Routine documentation corrections need no repeated permission.
- Agent routing: **Z.ai** for routine implementation and verification; **Codex** for independent QA/test reproduction where useful; **Opus** reserved for consequential economics/security review. Not all three are needed on every task.
- No production deployment, funding, initialization, or launch action is ever inside agent scope; those are founder/custody actions under the mainnet runbook.

---

## 2. Phase overview and dependency order

```
Phase 0  Documentation baseline .................. T00.1–T00.3  [VERIFIED]
Phase 1  Enforcement proofs & evidence .......... T10 T11 T12 T13   (parallel-permitted)
Phase 2  Budget & configuration ................. T20 T21 T22 T23   (founder-input gated)
Phase 3  Permanent cap protocol change .......... T30 → T31 → T32 → T33
Phase 4  Fruit accounting core .................. T40 → T41 → T42   (fixture-based start)
Phase 5  Attribution & adapters ................. T50 (needs T11) · T51 (needs T10,T12) · T52 (needs T13)
Phase 6  Bonus & referral ....................... T60 T61 (need T41; T61 also T52) · T62 (needs T22)
Phase 7  Frontend & user docs ................... T70 (needs T41) · T71 (needs T00.1)
Phase 8  Integrated verification ................ T80 (needs T31,T41,T50,T51,T52,T60,T61,T62) → T81
Phase 9  Devnet rehearsal & ops ................. T90 (needs T80) → T91
Phase 10 Mainnet readiness ...................... T100 (founder gates) · T101 (needs T33,T81,T91,T100)
Phase 11 Launch authorization ................... T110 (founder only; needs T101)
```

Dependency graph (no cycles; arrows mean "must be VERIFIED before"):

```
T00.1 ─┬─→ T30 ─→ T31 ─→ T32 ─→ T33 ──────────────────────────┐
       ├─→ T40 ─→ T41 ─┬─→ T42 · T70 · T60 ─┐                 │
       ├─→ T71         │                    ├─→ T80 ─→ T81 ─┐ │
       ├─→ T10 ────────┼─→ T51 ←─ T12       │                │ │
       ├─→ T11 ─→ T50 ─┤                    │                │ │
       └─→ T13 ─→ T52 ─┴─→ T61 ←─ T52       │                │ │
T21(D04) ─→ T22(D05) ─→ T62 ────────────────┘                │ │
T20(D03) ─→ T42 activation · T90 · T100                      │ │
T23(D06) ─→ T60/T61 activation                               │ │
                                   T33 ──────────────────────┤─┴─→ T101 ─→ T110
                                   T91 (← T90 ← T80) ────────┤
                                   T100 (founder gates D10) ─┘
```

Explicitly permitted parallelism after Phase 0: {T10, T11, T12, T13, T30, T40, T71} may proceed independently of each other. Everything else requires its listed dependencies VERIFIED. T42/T60/T61/T70 may be **built** against fixtures before Phase 2 decisions land, but their **activation** acceptance additionally requires the named decisions closed.

---

## 3. Task index

| ID | Title | Status | Depends on (VERIFIED) | Decision gates |
|---|---|---|---|---|
| T00.1 | Canonical Fruit tokenomics document | VERIFIED | — | — |
| T00.2 | Roadmap, decision register, status documents | VERIFIED | T00.1 | — |
| T00.3 | Documentation entry point (README) | VERIFIED | T00.2 | — |
| T10 | Borrowed/encumbered capital admission model | READY | T00.1 | D01 gates closure |
| T11 | Ownership settlement & allocation authority design | READY | T00.1 | D02 gates closure |
| T12 | Adapter verification evidence packs | READY | T00.1 | D08 gates closure |
| T13 | Reference-price framework + risk-overlay calibration plan | READY | T00.1 | D08/D09 partial |
| T20 | Base schedule, balances, runway and F/J reconciliation | BLOCKED | — | D03, D06 |
| T21 | Production price/fee configuration package | READY | T00.1 | D04 gates closure |
| T22 | Referral margin m and cut c selection package | BLOCKED | T21 completion | D05 |
| T23 | Bonus funding commitment package | BLOCKED | — | D06 |
| T30 | **Gated implementation spec: permanent cap + capped compounding** | **READY (NEXT)** | T00.1 | D07 evidence inside |
| T31 | Program implementation of permanent cap | NOT STARTED | T30 | — |
| T32 | Cap/compounding adversarial tests | NOT STARTED | T31 (co-developable) | — |
| T33 | Requalification of cap program version | NOT STARTED | T31, T32 | — |
| T40 | Fruit ledger engine specification | READY | T00.1 | — |
| T41 | Fruit ledger engine implementation + fixture tests | NOT STARTED | T40 | — |
| T42 | Epoch settlement/funding integration & activation | NOT STARTED | T41; activation also T20, T33 | D03 |
| T50 | Attribution/indexer implementation | NOT STARTED | T11 | D02 |
| T51 | External adapter implementations | NOT STARTED | T10, T12 | D01, D08 |
| T52 | Safe Mode core + consent + EXTREME handling | NOT STARTED | T13 | D09 (overlays only) |
| T60 | Loyalty accounting | NOT STARTED | T41; activation also T23 | D06 |
| T61 | Lock registry, Season cohort, safety release | NOT STARTED | T41, T52; activation also T23 | D06 |
| T62 | Fee-backed referral commissions | NOT STARTED | T22 | D05 |
| T70 | Estimator/calculator frontend | NOT STARTED | T41 (fixture mode allowed) | — |
| T71 | User-facing documentation & disclosures | READY | T00.1 | — |
| T80 | Integrated adversarial test campaign | NOT STARTED | T31, T41, T50, T51, T52, T60, T61, T62 | — |
| T81 | Independent economics/security review | NOT STARTED | T80, D01–D06 closed | — |
| T90 | Devnet Fruit rehearsal | NOT STARTED | T80 | D03 (rehearsal values) |
| T91 | Operational runbook extension (Fruit) | NOT STARTED | T90 | — |
| T100 | Existing release gates (custody, APP_ID, host, Seeker, audit, Part B) | NOT STARTED (founder-gated) | — | D10 |
| T101 | Mainnet readiness review | BLOCKED | T33, T81, T91, T100 | D10 |
| T110 | Founder launch authorization + activation | BLOCKED (founder-only) | T101 + founder | — |

---

## 4. Task cards

### Phase 0 — Documentation baseline

#### T00.1 — Canonical Fruit tokenomics document — VERIFIED
- **Purpose/scope:** Single normative tokenomics spec reconciling formulas, constants, timeline, caps, qualification, external ownership, boosts, referrals, locks, Safe Mode, solvency; labeling adopted vs. current vs. unresolved vs. blocked.
- **Dependencies:** none. **Files:** `docs/CHERRY_TREE_FRUIT_TOKENOMICS.md`.
- **Role:** Z.ai (author), founder (ratification by non-objection in review). **Decisions consumed:** C-01…C-20.
- **Acceptance:** document exists; every constant cross-checks against the final architecture §12; every CURRENT claim cites verified source; baseline absence recorded honestly.
- **Evidence:** commit introducing this file; link/constant cross-check output in the commit message trail; §5 of the doc (implementation map) matches `git grep` evidence gathered 2026-09-26.
- **Completion record:** VERIFIED 2026-09-26 — commit `fb50239` on `z-ai/cherry-docs-roadmap`.
- **Rollback:** git revert (documentation only).
- **Next permitted:** T00.2, and every Phase 1/3/4/7 READY task.

#### T00.2 — Roadmap, decision register, status documents — VERIFIED
- **Purpose/scope:** This file + `docs/CHERRY_TREE_DECISIONS.md` + `docs/CHERRY_TREE_STATUS.md`; one register entry per unresolved item; closed founder decisions recorded once (not re-opened).
- **Dependencies:** T00.1. **Role:** Z.ai; founder reviews D-register.
- **Acceptance:** every architecture §14 blocker maps to a D-id or task; no circular dependencies; exactly one NEXT actionable task in STATUS; statuses evidence-backed.
- **Evidence:** link/dependency/consistency checks in the commit trail. **Completion record:** VERIFIED 2026-09-26 — same commit as T00.1. **Rollback:** git revert.
- **Next permitted:** T00.3.

#### T00.3 — Documentation entry point — VERIFIED
- **Purpose/scope:** README documentation section links the canonical set; distinguishes shipped product behavior from planned Fruit features; notes release-lane-only files.
- **Dependencies:** T00.2. **Files:** `README.md`.
- **Role:** Z.ai.
- **Acceptance:** all links resolve on this branch; no claim that Fruit features are live.
- **Completion record:** VERIFIED 2026-09-26 — same commit as T00.1. **Rollback:** git revert.
- **Next permitted:** every Phase 1/3/4/7 READY task (roadmap §2).

---

### Phase 1 — Enforcement proofs and verification evidence

#### T10 — Borrowed/encumbered capital admission model — READY
- **Purpose/scope:** Produce enforceable options for distinguishing owned vs. borrowed/encumbered external equity (scoped proof sets, admitted-trust disclosure, or explicit per-asset acceptance bounds); take one to founder selection. Analysis may start immediately; **closure is gated by D01**.
- **Dependencies:** T00.1. **Files:** new `docs/` analysis note + D01 register entry; later enforced in admission checks (T51).
- **Role:** Z.ai drafts; founder selects; Opus reviews the selected model.
- **Required decisions:** D01. **Status rationale:** no missing input to begin analysis.
- **Acceptance:** options with exact evidence requirements, failure modes, and what each cannot prove; founder selection recorded in D01; selected model integrated into canonical spec §6 wording if needed.
- **Tests/evidence:** for the selected model, a worked example per asset class (owned, margin-borrowed, flash-loaned, pledged, wrapper-held) showing admit/exclude outcomes.
- **Completion record:** (open). **Rollback:** documentation-level.
- **Next permitted:** T51 (admission implementation).

#### T11 — Ownership settlement & allocation authority design — READY
- **Purpose/scope:** Design the finalized-history settlement: event sources and coverage, time/finality convention, global timeline construction, denominator reconstruction, allocation authority, independent replay, correction policy, cumulative-root invariants (unique epoch/version ids, replay protection, conserved sums, per-wallet limits). **Closure gated by D02** (founder selects trust model/authority).
- **Dependencies:** T00.1. **Files:** design note under `docs/`; later indexer/settlement components (T50).
- **Role:** Z.ai designs; Opus reviews; founder approves trust model.
- **Acceptance:** design covers every §6 attribution requirement and the §11 outage-reserve rule; known impossibilities stated (indexer truthfulness as explicit trust assumption unless independently enforceable).
- **Tests/evidence:** replay test plan on synthetic event streams (transfers, A→B→A, authority changes, mint/burn, stake/unstake, Token-2022) with expected interval integrations.
- **Completion record:** (open). **Rollback:** documentation-level.
- **Next permitted:** T50.

#### T12 — Adapter verification evidence packs — READY
- **Purpose/scope:** Per candidate (wallet SKR, staked SKR conditional, xSOL, JitoSOL, spot JLP): pinned mint/program/account identities, decimals/extensions, owner/position decoding, share/NAV conversion, normal-exit documentation (≤ 432,000 s normal path), financing/encumbrance checks. Explicitly excluded: leveraged/Multiply/queued/vault JLP; xBTC/xHYPE stay disabled. **No asset becomes live by completing this task** (D08 admission approval is separate).
- **Dependencies:** T00.1. **Files:** evidence pack(s) under `docs/` (or `docs/adapters/`), referencing the Fable solve §11 records as candidates only.
- **Role:** Z.ai assembles; Codex independently reproduces identity/exit checks where useful; founder approves admission set (D08).
- **Acceptance:** every identity cross-checked against official live sources with dates; chain reads verified read-only (no deployment claims); unresolved items listed back into D08.
- **Tests/evidence:** per-asset verification transcript; discrepancies logged.
- **Completion record:** (open). **Rollback:** documentation-level.
- **Next permitted:** T51.

#### T13 — Reference-price framework + risk-overlay calibration plan — READY
- **Purpose/scope:** Specify epoch reference-price construction (fixed timestamped prices, share conversion, staleness/finality rules), the SKR/SOL and USD/SOL (JLP) numeraire sources, and the data/collection plan for overlay calibration (per-pool CR series, NAV history, oracle update intervals, live fee parameters). Overlay thresholds themselves remain UNSET (D09).
- **Dependencies:** T00.1. **Files:** framework note under `docs/`; feeds consumed by T51/T52.
- **Role:** Z.ai; founder approves reference sources (part of D08).
- **Acceptance:** framework distinguishes reference (settlement) vs live (display) values; never reprices past liabilities; UNKNOWN handling specified.
- **Tests/evidence:** worked price-fixing example per asset with staleness outcomes.
- **Completion record:** (open). **Rollback:** documentation-level.
- **Next permitted:** T52.

---

### Phase 2 — Budget and configuration

#### T20 — Base schedule, balances, runway and F/J reconciliation — BLOCKED (D03, D06)
- **Purpose/scope:** Read/record the actual base schedule and vault balances once they exist; reconcile L/U/Z per ledger; determine net core Fruit budget F and bonus capital J; verify base runway reaches T0+12 days at activation.
- **Dependencies:** founder funding plan (D03) and bonus commitment (D06). **Files:** reconciliation report under `docs/`; runway tooling `app/scripts/ops/` (release lane).
- **Role:** founder (plan), ops multisig (execution), Z.ai (evidence).
- **Acceptance:** every lamport in the schedule traced to a cleared funding action; `V ≥ L+U+Z` verified per ledger; boundary state (F≥R / 0<F<R / F=0) determined and recorded.
- **Tests/evidence:** runway-report output, funding transaction signatures (once real), reconciliation table.
- **Completion record:** (open).
- **Completion record:** (open). **Rollback:** funding actions are not reversible; mistaken refills only re-spread (per runbook) — hence two-person readbacks before execution.
- **Next permitted:** T42 activation, T90 with real values, T100 Gate 5.

#### T21 — Production price/fee configuration package — READY
- **Purpose/scope:** Prepare the production initialize configuration package: `rig_price_lamports`, `purchase_fee_bps`, `genesis_cap_rigs = 250` (C-01), founder/funder/fee-recipient fields, two-person readback checklist (extends runbook Gate 4). **Closure gated by D04** (founder records the values).
- **Dependencies:** T00.1. **Files:** package note under `docs/`; executed later via T100 (never by an agent).
- **Role:** Z.ai prepares; founder decides; custody multisig executes.
- **Acceptance:** package states adopted P reference and the reconciliation rule on mismatch; fee within the verified 10% ceiling; value provenance recorded in D04.
- **Completion record:** (open). **Rollback:** configuration has no setter for the cap — correction requires a new program version; hence readback requirement.
- **Next permitted:** T22 (needs D04 closed), production Gate 4 under T100.

#### T22 — Referral margin m and cut c selection package — BLOCKED (D05; needs D04)
- **Purpose/scope:** Once φ (D04) is fixed, compute the feasible band `0 ≤ c ≤ 1−m/φ` for founder-supplied m and present options with burden/revenue tables (reusing the Fable §8 arithmetic where consistent); record the selected c in D05.
- **Dependencies:** T21 completion. **Files:** package note; register entry D05.
- **Role:** founder (m, c); Opus reviews before activation.
- **Acceptance:** selected c satisfies the bound with integer-settlement margin preservation demonstrated per-event.
- **Completion record:** (open). **Rollback:** parameter-level if implemented configurable; otherwise program-version.
- **Next permitted:** T62.

#### T23 — Bonus funding commitment package — BLOCKED (D06)
- **Purpose/scope:** Record J, per-epoch B_loy and per-Season B_lock commitments (or explicit zero), satisfying `Σ B_loy + Σ B_lock ≤ available bonus funds`; earmarking effects on Z_future documented.
- **Dependencies:** founder (D06). **Files:** register entry D06; reconciliation ties into T20.
- **Role:** founder (commitments); Z.ai records and reconciles against T20.
- **Acceptance:** commitments backed by cleared funds; no overlap with core commitments; zero-option recorded if chosen.
- **Completion record:** (open). **Rollback:** commitments release only verified-uncommitted amounts back to Z.
- **Next permitted:** T60/T61 activation.

---

### Phase 3 — Permanent Cherry cap (protocol change)

#### T30 — Gated implementation spec: permanent cap + capped compounding — **READY — NEXT ACTIONABLE TASK**
- **Purpose/scope:** Produce the implementation specification against the **current** release lane after fresh pre-ops (lane HEAD may have moved past `f0d5e96`): (a) enforce `S ≤ 250` on every seed-creation/receipt path — fresh purchase, compound, mint, migration, any supported receipt; (b) capped compounding `n_minted = min(n_affordable, max(0, 250−S))` with only exact price+fee debited and the remainder staying a claimable liability; compound at cap = fee-less no-op that discards nothing; fresh over-headroom purchases fail atomically; (c) account/state layout change (version bump), migration design, and the D07 evidence step: verify no production deployment/seed accounts exist (expected none) and disposition any devnet-lane over-cap artifacts.
- **Dependencies:** T00.1 (VERIFIED). **Files:** spec under `docs/`; touches later `programs/sol-rigs/src/{state.rs,instructions/*}`, `crates/rigs-engine/src/{position.rs,launch.rs,pricing.rs}` (specification only in this task).
- **Role:** Z.ai authors; Opus reviews the spec (protocol behavior change with economic effect); founder notes the version boundary.
- **Required decisions:** D07 evidence produced here (none expected to exist).
- **Acceptance:** spec enumerates every seed-creation path found in source with file/line references; defines exact rejection errors; preserves base reward/penalty rules otherwise; Fruit claims exempt from base claim penalties; migration/no-op-at-cap/remainder behaviors specified to lamport precision; D07 evidence recorded.
- **Tests/evidence (spec-level):** boundary case list handed to T32 (149/150, 249/250/251, compound-at-cap with partial spend, overflow attempts, legacy S>250 fixtures).
- **Completion record:** (open). **Rollback:** documentation-level.
- **Next permitted:** T31.

#### T31 — Program implementation of permanent cap — NOT STARTED
- **Purpose/scope:** Implement the T30 spec as a new program version on an isolated branch off the current release lane; no deployment.
- **Dependencies:** T30 VERIFIED. **Files:** `programs/sol-rigs/src/**`, `crates/rigs-engine/src/**`, migration instruction, Anchor IDL/types, TypeScript port parity (`app/src/lib/`).
- **Role:** Z.ai implements; Codex reproduces boundary tests independently.
- **Acceptance:** all T30 behaviors implemented; existing launch-gate/genesis-cap/penalty/referral semantics unchanged; `cargo test` + anchor tests green; TypeScript parity tests green.
- **Tests/evidence:** full test-suite output at the implementation commit.
- **Completion record:** (open).
- **Rollback/recovery:** pre-deploy, branch abandon/reset is safe; post-deploy, forward-fix only per `docs/backlog/CUSTODY.md` (never roll back past a state migration).
- **Next permitted:** T32 (co-developable), T33.

#### T32 — Cap/compounding adversarial tests — NOT STARTED
- **Purpose/scope:** Boundary and adversarial suite: 149/150 and 249/250 qualification/headroom boundaries; 251 rejection (fresh and compound); compound-at-cap no-op with zero fee and full reward preservation; partial compound spending only exact price+fee; remainder claimable; overflow/large-number attempts; legacy S>250 fixtures as migration-failure tests (not permitted-user examples).
- **Dependencies:** T31 (co-developable within its branch). **Files:** `programs/sol-rigs/tests/`, `app/tests/`.
- **Role:** Z.ai; Codex independent reproduction.
- **Acceptance:** every listed case has an automated test with expected exact lamport outcomes; suite runs in CI.
- **Completion record:** (open). **Rollback:** tests-only.
- **Next permitted:** T33.

#### T33 — Requalification of the cap program version — NOT STARTED
- **Purpose/scope:** Rerun the qualification lane (Part A devnet; Part B per runbook timing) for the new program version at its exact release commit, archiving evidence to the private archive; updates the Part-A-lags-HEAD gap recorded in D10.
- **Dependencies:** T31, T32. **Files:** qualification tooling on the release lane; private evidence archive (manifest-referenced).
- **Role:** Z.ai executes devnet lane; founder/custody schedule Part B.
- **Acceptance:** PASS record with artifact hashes, config readback, boundary observations (cap on/off at T0+24h for the Day-1 cap; permanent cap always on), superseding the `9a5e746` evidence for this lane.
- **Completion record:** (open). **Rollback:** n/a (evidence lane).
- **Next permitted:** T101 (and T42 activation hardening).

---

### Phase 4 — Fruit accounting core

#### T40 — Fruit ledger engine specification — READY
- **Purpose/scope:** Specify the Fruit settlement engine exactly: per-ledger V/L/U/Z and the eight canonical transitions; cumulative entitlement with carried remainders (`floor(E·elapsed/T)` capacity bound); per-wallet/pool ceilings (`E/5`, `B_loy/5`, `B_lock/5`); funding boundary states (F≥R / 0<F<R fallback single 48-hour Harvest / F=0 inactive); commit-before-schedule; outage/denominator-gap reserves (whole unsettled common-pool interval, minus only conclusively established liabilities); no premature release at epoch end; no claim expiry.
- **Dependencies:** T00.1. **Files:** spec under `docs/`; future `crates/` module (fixture-based development permitted — never presented as production verification).
- **Role:** Z.ai authors; Opus reviews the accounting spec.
- **Acceptance:** spec realizes every §11 rule to lamport precision; every §4 boundary state executable on paper with worked lamport examples.
- **Completion record:** (open). **Rollback:** documentation-level.
- **Next permitted:** T41.

#### T41 — Fruit ledger engine implementation + fixture tests — NOT STARTED
- **Purpose/scope:** Implement the T40 engine (pure accounting core, no chain integration required yet) with a fixture test suite: rounding/checkpoint independence, ceiling enforcement, transition atomicity, simultaneous/replayed claims, budget exhaustion, `V ≥ L+U+Z` asserted after every transition.
- **Dependencies:** T40. **Files:** new engine module under `crates/` (isolated from deployed code paths) + tests.
- **Role:** Z.ai implements; Codex reproduces the fixture suite.
- **Acceptance:** 100% of fixture scenarios pass with exact lamport equality; property-style checks (conservation, no double-spend) included.
- **Completion record:** (open). **Rollback:** isolated module; safe to drop.
- **Next permitted:** T42, T60, T70 (fixture mode).

#### T42 — Epoch settlement/funding integration & activation — NOT STARTED (activation additionally needs T20, T33)
- **Purpose/scope:** Wire the engine to real funding sources and epoch schedule: tranche commitments before start, boundary-state selection from actual F, Day-1 zero-spend enforcement, defensive `P·min(S,250)` recognition, epoch reference pricing hookup (from T13). **Activation requires D03 closed and the cap version qualified (T33).**
- **Dependencies:** T41; activation: T20 (VERIFIED), T33.
- **Files:** engine integration, epoch scheduler, ops tooling.
- **Role:** Z.ai; Opus reviews activation checklist.
- **Acceptance:** given recorded funding inputs, the exact sequence (tranches, offsets, durations) commits and settles correctly on devnet fixtures; zero Day-1 Fruit spend proven; no spend without prior commitment.
- **Completion record:** (open). **Rollback:** epoch commitments release only verified-uncommitted capacity; funding actions irreversible (readbacks first).
- **Next permitted:** contributes to T80.

---

### Phase 5 — Attribution and adapters

#### T50 — Attribution/indexer implementation — NOT STARTED
- **Purpose/scope:** Implement the D02-selected settlement: event ingestion/replay, global timeline, interval ownership integration, denominator reconstruction, correction/replay protection, outage reservation hooks. Fixture streams first; real sources once D02 names them.
- **Dependencies:** T11 VERIFIED (design closed via D02). **Role:** Z.ai; Opus reviews the settlement security model.
- **Acceptance:** synthetic adversarial streams (A→B→A, same-tx carousel, authority change, missing-denominator-history) settle exactly per spec; gap handling reserves the whole unsettled common-pool interval.
- **Completion record:** (open). **Rollback:** isolated component.
- **Next permitted:** contributes to T80.

#### T51 — External adapter implementations — NOT STARTED
- **Purpose/scope:** Adapters for the approved admission set only (D08; dispositions per canonical §6), enforcing D01 financing/encumbrance checks and T12 evidence; one asset per wallet per epoch; next-epoch switching; join-time accrual start.
- **Dependencies:** T10, T12 (both VERIFIED incl. founder selections D01/D08).
- **Role:** Z.ai; Codex reproduces identity/decoding checks.
- **Acceptance:** each adapter passes its evidence-pack checks against pinned identities; fail-closed on unknown provenance; no adapter enabled without founder admission approval.
- **Completion record:** (open). **Rollback:** feature-flag off; no custody involved.
- **Next permitted:** contributes to T80.

#### T52 — Safe Mode core + consent + EXTREME handling — NOT STARTED
- **Purpose/scope:** NORMAL/WARM/HOT/EXTREME state mapping from verified CR (and route availability), the two-step risk screen + scoped signature (wallet, asset, action/amount, chain, epoch, risk/config version, nonce, expiry), reconfirmation rules, prospective-only NAV resets with signed reference checkpoints, safety-release lock treatment hook (feeds T61), UNKNOWN handling for stale/missing data. Overlay thresholds remain disabled until D09.
- **Dependencies:** T13 framework VERIFIED (thresholds themselves not needed for core states).
- **Role:** Z.ai; Opus reviews the consent/reset security model.
- **Acceptance:** state transitions unit-tested against CR boundaries (1.35/1.20/1.00) and route-availability matrix; no signature ever bypasses an unavailable operation; no retroactive settlement change.
- **Completion record:** (open). **Rollback:** Safe Mode is prospective; disabling overlays is safe; never erases L by construction.
- **Next permitted:** T61 interplay; contributes to T80.

---

### Phase 6 — Bonus and referral

#### T60 — Loyalty accounting — NOT STARTED (activation needs T23)
- **Purpose/scope:** Implement β clock (Day-1 definition; reset rules: voluntary eligibility loss or penalized base claim; freezes on outage/involuntary Safe Mode without erasing history), `H_loy` floor, `B_loy` commitment/settlement in the bonus ledger, `B_loy/5` ceiling, activity event recording with anti-spam limits and **no monetary conversion**.
- **Dependencies:** T41; activation: T23. **Role:** Z.ai.
- **Acceptance:** fixture scenarios for all reset/freeze rules; ceiling and floor-arithmetic exact; new-user core allocation unaffected.
- **Completion record:** (open). **Rollback:** bonus pool only; core untouched.
- **Next permitted:** contributes to T80.

#### T61 — Lock registry, Season cohort, safety release — NOT STARTED (activation needs T23)
- **Purpose/scope:** One immutable election per wallet/Season; `[FRUIT_T0, t_s)` registration cutoff with finalized-ordering inclusion; `ℓ_i = min(requested, w_i(t_s))`; frozen `H_lock = max(Σℓ·d, 62.5)`; `A_i = B_lock·u_i/H_lock`; surviving-weight low-water mark; proportional shortfall cancellation; cliff vest U→L; safety release on verified EXTREME/route failure vesting earned-to-date progress computed on last valid weight; no second election; unallocated balance reservation then release to Z.
- **Dependencies:** T41, T52; activation: T23. **Role:** Z.ai; Opus reviews.
- **Acceptance:** fixtures: all durations, everyone-five-days case, nobody-locks case, early exit, partial shortfall, restored weight not resurrecting forfeited units, safety release, re-entry rejection, unknown-history pending.
- **Completion record:** (open). **Rollback:** conditional U only; cancelled awards return to Z after ownership resolution.
- **Next permitted:** contributes to T80.

#### T62 — Fee-backed referral commissions — NOT STARTED
- **Purpose/scope:** Per-event commission ledger on actually received eligible fees (fresh purchase + completed compound), atomic reservation before any fee withdrawal, single consumption per event, `floor_lamport(c·f_e)` with per-event retained-margin preservation, no second deduction from accrued rewards, no retroactive commissions.
- **Dependencies:** T22 VERIFIED (c selected). **Role:** Z.ai; Opus reviews margin preservation.
- **Acceptance:** fee-withdrawal race fixtures (commission reserved first); replayed-event rejection; margin bound holds under integer rounding.
- **Completion record:** (open). **Rollback:** commissions are reserved funds; disabling stops new reservations without clawing back established ones.
- **Next permitted:** contributes to T80.

---

### Phase 7 — Frontend and user documentation

#### T70 — Estimator/calculator — NOT STARTED
- **Purpose/scope:** Projection UI per the architecture §15-calculator requirements: cap headroom; separate core/loyalty/conditional-lock/vested/referral balances; pending-verification state; all budget sources; risk state/route status; full capital denominators (weight, Cherry, combined); live-H rates only (no floor-case APR as live); N/A at zero participants; no bonus ratio when the denominator is zero.
- **Dependencies:** T41 (fixture mode acceptable for build; live data later). **Role:** Z.ai.
- **Acceptance:** estimator outputs match engine fixtures exactly; disclosure copy reviewed.
- **Completion record:** (open). **Rollback:** UI-only.
- **Next permitted:** contributes to T80/T91 disclosures.

#### T71 — User-facing documentation & disclosures — READY
- **Purpose/scope:** Turn the canonical spec into user-facing copy: how Fruit works, what is/isn't custodied, normal-path exit standard (no catastrophe guarantee), borrowed-capital exclusion, lock conditionality, Safe Mode consent, APR labeling rules, phase labels, non-redeemable seeds, no claim expiry. **No claim that Fruit is live until T101 passes.**
- **Dependencies:** T00.1. **Files:** user docs / app content areas.
- **Required decisions:** D11 — the website's "stock baskets" teaser (styling branch only) must not be implied to be part of the canonical Fruit model in any user-facing copy until the founder dispositions it.
- **Role:** Z.ai drafts; founder approves public copy.
- **Acceptance:** copy cross-checked against canonical spec; zero invented figures; planned/live distinction explicit.
- **Completion record:** (open). **Rollback:** documentation-level.
- **Next permitted:** T91 inclusion.

---

### Phase 8 — Integrated verification and independent review

#### T80 — Integrated adversarial test campaign — NOT STARTED
- **Purpose/scope:** End-to-end adversarial integration of every component: the architecture §14.8 list — 149/150 and 249/250 boundaries; 251 rejection/partial compounding; exact public boundary `FRUIT_T0`; zero Day-1 Fruit spend; ownership carousel and debt exclusion; denominator outage reserves; all lock durations/early exits/re-entry; HOT acknowledgement and unavailable-route rejection; EXTREME accrued-progress preservation; fee commission replay/withdrawal races; rounding/checkpoint independence; simultaneous/replayed claims; funding exhaustion — asserting `V ≥ L+U+Z` after every transition, per-wallet/pool ceilings, and accrued-claim preservation.
- **Dependencies:** T31, T41, T50, T51, T52, T60, T61, T62 all VERIFIED.
- **Role:** Z.ai builds; **Codex independently reproduces the campaign from the task description** (not from Z.ai's code).
- **Acceptance:** both runs pass with identical results; failures documented and fixed before T81.
- **Completion record:** (open). **Rollback:** test campaign only.
- **Next permitted:** T81, T90.

#### T81 — Independent economics/security review — NOT STARTED
- **Purpose/scope:** Opus-led review of the integrated system against the canonical spec: economics (weights, ceilings, floors, bonus ratios, referral bounds), settlement security (attribution, roots, corrections), Safe Mode/consent, solvency transitions, and the honesty of all user-facing claims.
- **Dependencies:** T80 VERIFIED; D01–D06 closed (no unresolved parameter presented as solved).
- **Role:** Opus leads the review; founder receives and disposes findings.
- **Acceptance:** written review artifact with verdict; every finding either fixed (with re-verification) or explicitly accepted by the founder with rationale.
- **Completion record:** (open).
- **Completion record:** (open). **Rollback:** n/a (review).
- **Next permitted:** T101.

---

### Phase 9 — Devnet rehearsal and operational readiness

#### T90 — Devnet Fruit rehearsal — NOT STARTED
- **Purpose/scope:** Full-flow devnet rehearsal of the integrated system (launch gate → Day-1 zero Fruit spend → public opening → sequence → claims/compounds/locks/referrals/Safe Mode drills), launch-gate aware and resumable along the existing rehearsal lane pattern (`claude/cherry-devnet-rehearsal-resume` lineage). Rehearsal funding values are explicitly test-only stand-ins, never D03 facts.
- **Dependencies:** T80; rehearsal budget stand-ins recorded. **Role:** Z.ai runs; Codex observes/reproduces spot checks.
- **Acceptance:** rehearsal log covering the full timeline with boundary observations; deviations documented; no mainnet action.
- **Completion record:** (open). **Rollback:** devnet-only.
- **Next permitted:** T91.

#### T91 — Operational runbook extension — NOT STARTED
- **Purpose/scope:** Extend `docs/backlog/MAINNET_RUNBOOK.md` (release lane) with Fruit gates: funding/readback, epoch commitment checks, admission/verification status checks, monitoring (denominators, reserves, indexer health, risk states), disclosure checks, and Fruit-specific pause/containment notes (Safe Mode ≠ base pause).
- **Dependencies:** T90. **Role:** Z.ai; founder/ops review.
- **Acceptance:** runbook diff reviewed; gates sequential and FAIL-stops-run like existing gates.
- **Completion record:** (open). **Rollback:** documentation-level.
- **Next permitted:** T101.

---

### Phase 10 — Mainnet readiness (existing gates preserved)

#### T100 — Existing release gates — NOT STARTED (founder-gated; D10)
- **Purpose/scope:** Completion of the pre-existing gates from `docs/backlog/MAINNET_RUNBOOK.md` (Gates 0–7) and its sibling artifacts: custody multisigs created/tested; production funding model decided and executed (Gate 5, ties to T20/T21); `CHERRY_APP_ID` issued + host chosen; Seeker plan executed; external audit status recorded; fee-recipient ≠ founder separation; Part B qualification at the **final** release commit (must include the T33 cap version — superseding the `9a5e746` Part A evidence); `genesis_cap_rigs = 250` readback at Gate 4. **All execution is founder/custody action; agents only prepare and verify evidence.**
- **Dependencies:** D10 items; Part B requires T33's release commit. **Role:** founder + custody signers; Z.ai evidence; Codex/Opus as useful.
- **Acceptance:** each runbook gate checked with archived evidence per its own evidence-archive requirements.
- **Completion record:** (open). **Rollback:** per runbook (forward-fix only; no rollback past migrations).
- **Next permitted:** T101.

#### T101 — Mainnet readiness review — BLOCKED (until T33, T81, T91, T100 VERIFIED)
- **Purpose/scope:** Single consolidated readiness review: runbook gates + Fruit activation checklist (funding committed, cap enforced and qualified, admission infrastructure live, solvency verified, disclosures accurate). Output: a GO/NO-GO recommendation with evidence index. **Not a launch authorization.**
- **Dependencies:** as listed. **Role:** Opus leads; founder receives.
- **Acceptance:** written GO/NO-GO with complete evidence index; every NO-GO item mapped to an owner.
- **Completion record:** (open). **Rollback:** n/a.
- **Next permitted:** T110 (founder only).

---

### Phase 11 — Launch authorization

#### T110 — Founder launch authorization + activation — BLOCKED (founder-only)
- **Purpose/scope:** Runbook Gates 8–9: founder preflight, the single activation transaction recording T0, post-activation verification and monitoring. **Explicitly and permanently outside agent scope; requires explicit founder authorization after T101.** No launch date exists; none may be implied by this roadmap.
- **Dependencies:** T101 GO + explicit founder decision.
- **Role:** founder (authorization and the activation transaction); custody as applicable; agents only verify evidence afterward.
- **Acceptance:** per runbook Gates 8–9 evidence archive.
- **Completion record:** (open; never closed by an agent).
- **Rollback:** containment via pause + forward-fix only (runbook); activation itself is irreversible — hence every preceding gate.
- **Next permitted:** none — T110 is terminal; post-launch operation follows the runbook, not this roadmap.

---

## 5. Release-gate integration map

| Existing artifact (release lane) | Roadmap anchoring |
|---|---|
| `docs/backlog/MAINNET_RUNBOOK.md` Gates 0–7 | T100 (founder-gated, D10) |
| `docs/backlog/MAINNET_RUNBOOK.md` Gates 8–9 | T110 (founder-only) — nothing in this roadmap authorizes them |
| `docs/backlog/CUSTODY.md` (multisigs, forward-fix) | D10; T100; rollback policies of T31/T100 |
| `docs/backlog/GENESIS_CAP_DECISION.md` | Closed by C-01 (250); rubric retained as historical analysis |
| `docs/backlog/REWARD_RUNWAY_OPS.md` | T20/T21; Gate 5 under T100 |
| `docs/backlog/CHERRY_INTEGRATION_READINESS.md` | D10 (APP_ID/host) |
| `docs/backlog/CAPITAL_FUTURE_POLICY.md`, `FUTURE_PROGRAM_VERSION.md`, `GROWTH.md`, `SEEKER_DEVICE_TEST_PLAN.md` | D10 scope; unchanged by tokenomics roadmap |
| Qualification evidence (private archive) | T33; manifest-only references in public docs |

## 6. Changing this roadmap

- Status transitions require evidence in the task's completion record; `VERIFIED` never without it.
- Dependency or economics changes: recorded rationale + appropriate founder decision (decision register §3).
- Documentation-only corrections: proceed without repeated permission; keep links and STATUS's single NEXT ACTIONABLE TASK consistent.
