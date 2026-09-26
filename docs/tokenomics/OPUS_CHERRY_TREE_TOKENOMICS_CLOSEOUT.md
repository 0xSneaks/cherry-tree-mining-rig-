# THE CHERRY TREE — Final Correction and Implementation-Readiness Closeout

Date: 2026-09-26. Documentation only. No production code, no deployment.

Applies the founder's final decisions to `docs/CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md` (the "Architecture", cited §n) and reconciles `docs/OPUS_CHERRY_TREE_TOKENOMICS_ADVERSARIAL_REVIEW.md` (the "Review"). The corrected Architecture is committed on this branch.

## 1. PRE-OPS / SOURCES VERIFIED

| Source | Result |
|---|---|
| Architecture (`claude/cherry-tree-tokenomics-cfoilj`, commit `e807b55`) | Merged into this branch and corrected in place |
| Fable solve (same branch) | Arithmetic unchanged; no correction needed |
| Review (this branch, commit `5ffac43`) | All CB, H and AC items reconciled below |
| V2 `mainnet/rc-hardening` source | Re-confirmed: `declare_id!` = `C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt`; `Anchor.toml` lists localnet/devnet only; the mainnet runbook header says "PREPARED, NOT EXECUTED". Earlier source facts stand: no holding cap; `genesis_cap_rigs` has no setter; mutable price/fee via `set_pricing`; `withdraw_fees` has no reservation; `NothingToCompound` revert; `identity::prove` records no binding; base `assert_fully_funded` counts base ledger only |
| Deployed mainnet state | **Verified by founder-supplied read-only evidence.** `solana account C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt --url mainnet-beta` → `AccountNotFound`. The declared production program does not exist on mainnet: no ProgramData, Config or Miner state. Initial-deployment case (Architecture §14.2-A). (RPC from this environment returns proxy 403, so the check was run by the founder, not reproduced here.) |

## 2. CRITICAL BLOCKERS

**CLOSED**

| # | Closed by | Resulting rule |
|---|---|---|
| CB-1 Borrowed-capital exclusion admits nothing | Founder decision 1 | `X_i = max(0, Σ A_ij G_ij − D_i)`, with `D_i` = wallet debt across a fixed registry of on-chain lending/perp/margin programs. Off-registry and off-chain borrowing are undetectable and disclosed. No proof of non-borrowing is required (Architecture §5) |
| CB-2 Settlement layer undefined | Founder decision 2 | On-chain Fruit ledger with minimum state and 11 invariants (Architecture §10a) |
| CB-3 (rule part) Hard cap and legacy treatment | Founder decision 3 | Permanent `S≤250` enforced in engine `buy_rigs`/`compound`. Legacy `S>250` wallets are grandfathered and frozen (Architecture §4) |
| CB-3 (evidence part) Deployed state | Founder-supplied mainnet check: `AccountNotFound` | Initial-deployment case. Cap and `.skr` binding ship in the initial deployment; no migration; no legacy `S>250` wallet can exist under this deployment (Architecture §14.2-A) |

**STILL OPEN**

None.

## 3. HIGH-RISK ITEMS

**Resolved by rule (Architecture corrected)**

| Item | Resolution | § |
|---|---|---|
| H-1 Stale reference exploited after a shock | Units added after the reference use `min(ref, live)`. Every admitted asset has an EXTREME path, effective from the first observable on-chain slot | §5, §9 |
| H-2 Reference manipulation | Committed, manipulation-resistant references; no single spot sample | §5 |
| H-3 Mutable price breaks the E/5 bound | Frozen `P_ref = 10,000,000` in all Fruit math | §1 |
| H-4 Mutable fee breaks the margin | `commission_e = min(floor(c f_e), max(0, f_e − ceil(m v_e)))` | §7 |
| H-6 Delegate, freeze and Token-2022 encumbrance | Added to exclusions and to the ownership timeline | §5 |
| H-8 Vault cross-backing | Separate Fruit vaults; no Fruit lamport enters a base vault | §10, §10a-9 |
| H-9 Adapter program upgrades | Pin programdata per decoded program; an upgrade sets the asset to UNKNOWN | §5 |
| H-10 One asset's outage freezes the pool | A missing committed reference makes the asset inactive, not UNKNOWN | §5 |
| H-11 Time and ordering | (slot, tx, ix, inner ix); slot `Clock.unix_timestamp`; finalized data; later-interval boundaries | §3 |
| H-12 `.skr` domain hopping | One domain = one Day-1 wallet via a binding PDA (founder decision 5) | §4 |
| H-13 Legacy S>250 | Grandfather-freeze rule (founder decision 3) | §4 |
| H-15 Stranded disputed reserves | Terminal resolution after two archive searches; never reduces L | §10 |

**Implementation / config dependent**

| Item | Dependency |
|---|---|
| H-5 Referral reservation | Base-program changes specified (reservation counter, referrer ledger PDA, gated `withdraw_fees`, `claim_commission`). `c = 0` until shipped and until m and φ are set |
| H-7 Loyalty dust tenure | The rule is fixed: a tenure day requires `w ≥ w_tenure_min`. The threshold is a founder parameter, unset; `B_loy = 0` until set. This does not block code |
| H-14 EXTREME griefing and flapping | Route unavailability is defined by on-chain protocol halt state. The dwell period is a calibrated parameter, unset; EXTREME-triggered safety release stays disabled until it is set |

**Still unresolved**

None at the rule level.

## 4. ARCHITECTURE CORRECTIONS REQUIRED

All corrections below are applied in the Architecture on this branch.

| Item | Correction applied | § |
|---|---|---|
| AC-1 | `P_ref` frozen for Fruit | §1 |
| AC-2 | Generic EXTREME path plus `min(ref, live)` | §5, §9 |
| AC-3 | Fruit, loyalty, lock and commission SOL are claimable only. Only base rewards compound, subject to the cap (founder decision 4) | §4, §10 |
| AC-4 | Engine revert accepted as the at-cap no-op: no seed, no fee, rewards retained | §4 |
| AC-5 | Check 1 kept as a guard; pre-ops requires `genesis_cap_rigs == 250`; activation purchase counts against both checks | §4 |
| AC-6 | "Not earned" (on-chain program or reference unavailable) vs "earned, pay on replay" (indexer-only gap) | §3 |
| AC-7 | Voluntary/involuntary cause table | §6 |
| AC-8 | Asset = underlying mint; wallet and staked SKR are one selection | §5 |
| AC-9 | Lock registration stores intent only; validation happens at t_s | §8 |
| AC-10 | `Cn` = wallets with `S ≥ 150` and a registered selection; N/A if 0 | §1 |

The following sections were also updated: §2 unresolved-configuration table (new rows: debt registry, `w_tenure_min`, ledger governance, EXTREME dwell), §12 final table, §13 supersession note, and §14 rewritten as gates.

No tokenomics parameter, tranche, rate, cap or threshold was changed.

## 5. FINAL ON-CHAIN FRUIT LEDGER REQUIREMENTS

(Architecture §10a, normative.)

**Accounts**
- `FruitConfig`: version, admin, poster, veto set and threshold (the poster is excluded), challenge window, `P_ref`, `M`, `qmax`, pause flags.
- `Ledger` per core/loyalty/lock: vault, `L`, `U`, `Z`, `paid_cum`.
- `Pool` per epoch/Season: `E`, `t_start`, `t_end`, `wallet_cap = floor(E/5)`, reference-commit hash, `final_total`, `paid_cum`, status, frozen `H_lock`.
- `Root` per version: root, `checkpoint_time`, `total`, `final_after`, status.
- `ClaimRecord` per pool and wallet: `claimed_cum`.
- Leaves are domain-separated by program, ledger, pool and wallet.

**Invariants**
1. `vault − rent ≥ L+U+Z` per ledger and in aggregate, after every instruction.
2. Commit only from Z, once per lamport, and before `t_start`.
3. `root.total ≤ floor(E·elapsed/D)`. `checkpoint_time ≤ now`. Versions strictly increase. At most one Pending root per pool.
4. `total ≥ final_total`: finalized liabilities are never reduced.
5. Finality only after the challenge window with no veto. On finality, U→L by the delta.
6. Leaf `≤ wallet_cap`.
7. Cumulative claims against the latest Final root only: payment = `max(0, leaf − claimed_cum)`. Duplicate or concurrent claims pay 0. Hard guard `paid_cum ≤ final_total`.
8. Unallocated capacity stays in U until reconciled close. Elapsed time alone releases nothing.
9. Outflows only by claim (against L) or reserve withdrawal (against Z). Nothing goes to base vaults or into seeds.
10. Lock progress sits in U and vests U→L. Forfeits go to Z at close. `H_lock` is written once.
11. Versioned, one-shot, fail-closed migrations that preserve lamports, L, U, `claimed_cum` and history. Pause never reduces L.

The governance values (poster, veto set, threshold, window length, upgrade authority) are required inputs, not chosen here.

## 6. FINAL 250-SEED CAP / LEGACY MIGRATION RULE

(Architecture §4.)

- **Cap.** `S_i ≤ 250` at all times, for every seed-creating path: purchase, compound, mint, migration, receipt. It is enforced in engine `buy_rigs`/`compound`. `M_hold` is a constant or a no-setter field.
- **Day 1.** The 250 fresh-purchase check is retained, and `genesis_cap_rigs == 250` is a pre-ops requirement.
- **Qualification.** 150 seeds.
- **Compound.** Mints `min(n_affordable, 250 − S)` and debits only that quote. At the cap: no seed, no fee, earned SOL stays claimable. A revert is an acceptable implementation.
- **Fruit and bonus SOL** never compound.
- **Legacy S>250:**
  - no confiscation or clipping; no write to `Miner.rigs`;
  - frozen: no growth, purchases revert;
  - base emissions and claims continue, and earned liabilities are preserved;
  - Fruit, loyalty and lock recognize 250.
- **Migration** is versioned, one-shot and fail-closed.
- **Deployed-state read** decides whether this is an initial deploy, a pre-T0 migration or a post-T0 migration with legacy freeze.

## 7. FINAL OWNERSHIP + DEBT ADMISSION RULE

(Architecture §3, §5.)

**Ownership**
- Complete finalized event replay: transfers, authority/owner changes, `Approve`/`Revoke`, freeze/thaw, mint/burn, stake/unstake, wrapper conversion, Token-2022 changes and registry debt events.
- Ordered by (slot, tx, ix, inner ix), using slot `Clock.unix_timestamp`.
- One unit supports one owner at any instant. Same-transaction round trips and flash balances have zero duration.
- Endpoint-only proofs are prohibited.

**Equity**
- `X_i = max(0, Σ_selected A_ij G_ij − D_i)`.
- `G` counts only unencumbered units: `amount − delegated_amount`. Frozen accounts, pledged collateral, wrappers, pending unstake, vesting, duplicate receipts, and unapproved Token-2022 permanent-delegate/hook/confidential mints are excluded.
- Valuation: committed manipulation-resistant reference; later additions at `min(ref, live)`.

**Debt**
- `D_i` = total debt across the fixed registry, from finalized state, valued at reference.
- An increase applies from its event. Unreadable registry state makes the wallet pending, not zero and not admitted.
- Off-registry and off-chain borrowing cannot be detected. This is disclosed and does not make positions ineligible.

**xAssets**
- xSOL is a candidate, valued at owned-token NAV, not underlying notional, subject to adapter validation.
- xBTC and xHYPE stay disabled.
- Excluded: leveraged, queued, Multiply or vault JLP; recursive lending; borrowed capital counted as equity. Only spot JLP is admitted.

## 8. FINAL .SKR FAIR-LAUNCH BINDING RULE

(Architecture §4.)

- The Day-1 window is `[T0, T0+86,400)`. One qualifying `.skr` domain opens Day-1 access for exactly one wallet.
- The first successful Day-1 purchase using domain D creates PDA `["skr_bind", name_account(D)]`, recording the wallet.
- Any other wallet proving D during Day 1 is rejected, including after D or its wrapper is transferred.
- The binding cannot be closed or reassigned during Day 1 and has no effect after public opening.
- This is a base-program change shipped with the cap, since V2 `identity::prove` records no binding.
- Day-1 Fruit spend stays 0. Public FRUIT_T0 = T0 + 86,400.
- This is not a unique-person guarantee.

## 9. FINAL TEST GATE

Architecture §14.3 lists 76 tests. It contains all 57 Review tests, some updated for the founder decisions (tests 3, 4, 36/47), plus 19 new ones:

| New tests | Covers |
|---|---|
| 10–11 | Engine-level cap fuzz; no path converts Fruit SOL into seeds or base vaults |
| 12–15 | `.skr` binding |
| 27–29 | Mid-epoch debt and unreadable registry, non-registry disclosure, xSOL NAV valuation (test 26 extends Review test 20 with the X≥0 floor) |
| 39 | DEX quote failure is not route unavailability |
| 46 | Not-earned vs replay-earned split |
| 49 | Cause table |
| 54 | Lock intent-only registration |
| 56–57, 60, 65–67 | Root ordering and monotonicity, veto and poster separation, close rules, terminal resolution, ledger migration |
| 71 (extended) | The margin clamp gives 0 when `m·v ≥ f` |
| 73 (extended) | No commit after `t_start` |

Every test asserts per-ledger and aggregate `V ≥ L+U+Z`, `ΣR ≤ E`, `R_i ≤ E/5`, `paid_cum ≤ final_total`, and that L never decreases except by payment.

## 10. IMPLEMENTATION-READINESS VERDICT

**PASS FOR IMPLEMENTATION**

The single remaining blocker (CB-3 evidence part, Architecture §14.2-A) is closed. Founder-supplied read-only evidence shows program `C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt` does not exist on mainnet (`AccountNotFound`). That is the initial-deployment case: the 250 cap and `.skr` binding ship in the initial deployment, no Config/Miner migration is required, and no legacy `S>250` wallet can exist under this deployment. All three critical blockers are closed, H-1 to H-15 and AC-1 to AC-10 are reconciled, and no rule-level item is open.

**Architecture frozen.** `docs/CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md` at this revision is the implementation baseline. No tokenomics parameter was changed by this closeout.

**Deploy-time conditions (not blockers to code):**
- initialize with `genesis_cap_rigs == 250` (test 8);
- if deployment uses a different program ID, or this ID exists before deployment, re-run §14.2-A against it before initialization;
- record the release branch/commit (§14.2-B) before code changes.

**Next step:** the architecture → code gap map and implementation sequence.

**What can proceed now**

- All code work in §14.3 scope: base-program cap/binding/referral-reservation changes, Fruit ledger, adapters and tests 1–76.

**Activation-only inputs**

These gate feature activation, not code (Architecture §14.2-C/D):
- budget: F, J, B_lock;
- debt registry and adapter pins/thresholds;
- EXTREME dwell;
- `w_tenure_min`;
- m and φ, then c;
- ledger governance keys and window.
