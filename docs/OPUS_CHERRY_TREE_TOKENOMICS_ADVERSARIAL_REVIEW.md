# THE CHERRY TREE — Final Adversarial Tokenomics Review

Date: 2026-09-26. Review only. No production code, no deployment.

Reviewed: `docs/CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md` (the "Architecture", cited §n) and `docs/FABLE_CHERRY_TREE_TOKENOMICS_SOLVE.md` on `claude/cherry-tree-tokenomics-cfoilj`, cross-checked against V2 source on `mainnet/rc-hardening` (`state.rs`, `buy_rigs.rs`, `compound.rs`, `claim.rs`, `admin.rs`, `fund_rewards.rs`, `migrate_v2.rs`, `identity.rs`, `rigs-engine/{position,launch,penalty}.rs`).

The architecture is treated as frozen. Arithmetic re-verified with no errors: tranche lamports, R, Storm ceiling, offsets, 305624.999955 numerator, the §11 table, APR crossings, loyalty ratio `b=(B/E)βH/H_loy`, `B=bfE`, lock bound `A_i≤B/5`, referral bound `c≤1−m/φ`.

Source facts this review relies on (verified in V2):

- Seeds are non-transferable and non-redeemable. `Miner` is PDA `["miner", owner]`; no transfer, sell, or close instruction exists. `S_i` only increases.
- `set_pricing` lets the authority change `rig_price_lamports` and `purchase_fee_bps` at any time.
- `genesis_cap_rigs` is pinned at `initialize`/`migrate_v2` and has no setter.
- `withdraw_fees` can withdraw the whole distributable fee-vault balance. There is no reservation ledger.
- The engine's `compound` returns `NothingToCompound` when count is 0, which reverts the transaction; it is not a silent no-op.
- `identity::prove` checks current domain ownership only. It records no domain-to-wallet consumption.
- The base reward vault's `assert_fully_funded` compares vault lamports against base-ledger outstanding only.

---

## 1. CRITICAL BLOCKERS

### CB-1 — The borrowed-capital exclusion, as written, admits no position at all
- **Rule:** §5 `A_ij` ("financing ... checks all pass"); "Unknown borrowing/encumbrance evidence makes the affected position ineligible"; §14.1.
- **Failure:** Nobody can prove that funds were not borrowed off-chain, on a CEX, or from a second wallet (§5 says so). Read strictly, every position has unknown provenance, so `A_ij=0` for everyone, `X=0`, `w=0` and `Cw=0`. Fruit then emits to nobody. Read loosely, an implementer quietly swaps in a weaker check, which is the "false guarantee" §14.1 forbids.
- A second, separate problem: `D_ij`, "borrowed principal attributable to that position", cannot be computed. Lending protocols record debt per obligation, fungible across collateral and not per token lot.
- **Severity:** CRITICAL. **Breaks:** implementation feasibility (and fairness if weakened silently).
- **Correction:** The founder must adopt an enforceable scoped definition before any adapter goes live. Smallest version consistent with §5:
  - `D_i` = the wallet's total outstanding debt, valued in SOL at epoch reference, across a pinned registry of lending/perp/margin programs, read from finalized chain state.
  - `X_i = max(0, Σ_j A_ij G_ij − D_i)`, netted at wallet level instead of per position.
  - Plus the §5 exclusions: wrappers, pledged collateral, pending unstake, vesting.
  - Disclosure text must say that off-registry and off-chain borrowing cannot be detected.
  - Without this decision, §5 cannot be implemented.

### CB-2 — The settlement and allocation layer is undefined, so §10's invariants and per-wallet caps cannot be enforced
- **Rule:** §2 "Allocation authority"; §5 "cumulative roots"; §10; §14.5.
- **Failure:** Weight depends on external history, so every `R_i`, `Q_i` and `A_i` is computed off-chain and posted as a root. §10 transitions (`U→L`, `U→Z`) and the `E/5` bound hold only if an on-chain program enforces them. With just a root-posting key, a faulty or compromised authority can:
  - create 5 synthetic qualifying wallets and take up to 100% of each tranche. This stays inside `E`, so it passes a solvency check but is theft.
  - publish an over-allocating root that gets claimed before correction. §10 forbids clawback, so the honest wallets' correct amounts then need "separate backing first". That is an unfunded liability created by an error.
- **Severity:** CRITICAL. **Breaks:** security, solvency of promises, implementation feasibility.
- **Correction:** Specify the on-chain Fruit ledger program before implementation, with at minimum:
  1. Per-pool on-chain counters for `V`, `L`, `U`, `Z` (separate vault PDA per ledger, see H-8).
  2. Each root carries `(program_id, pool_id, version, checkpoint_time, Σleaf)`. Enforce `Σleaf ≤ floor(E·elapsed/T_pool)` and monotone `version` on-chain.
  3. Enforce leaf `cumulative_amount ≤ E_pool/5` at claim.
  4. A challenge/timelock window between posting and claimability, plus an independent-replay veto (a signer set distinct from the poster).
  5. `claimed_i` counters so cumulative claims are idempotent. A lower corrected leaf pays 0 and never reverts other claims.

### CB-3 — The hard cap needs a base-program upgrade against unverified deployed state
- **Rule:** §4 checks 1–3; §2 "Base hard-cap migration"; §14.2.
- **Failure:**
  - V2 has no holding cap. `genesis_cap_rigs` has no setter; if the deployed value is not 250, check 1 needs a new migration.
  - Adding `M_hold` means changing the Config layout (V2→V3, 329→N bytes). That brings a second inert migration window in which buy, claim and compound all fail.
  - If `launch_t0` is already set on mainnet, V2 purchases after T0+24h are uncapped, so wallets above 250 may exist today.
  - None of this was audited (§0 "Live deployed configuration ... not audited").
- **Severity:** CRITICAL. **Breaks:** implementation feasibility.
- **Correction:**
  - Pre-ops read of the deployed program (programdata hash, Config bytes, `launch_t0`, `genesis_cap_rigs`, `rig_price_lamports`, `purchase_fee_bps`, and the maximum `Miner.rigs` via getProgramAccounts).
  - Put the cap as a pure check in `rigs_engine::buy_rigs` and `rigs_engine::compound`, not only in handlers, so the existing fuzz and acceptance suites cover it.
  - Pin `M_hold` as a constant or a no-setter Config field.
  - Complete the migration before T0 if T0 is unset. If T0 is set, apply H-13.

---

## 2. HIGH-RISK ISSUES

### H-1 — A fixed epoch reference price lets entries after a shock buy weight cheaply (every asset, not just xSOL)
- **Rule:** §5 "Use fixed timestamped epoch prices"; §9 applies the emergency reset to xSOL only; the table lists JitoSOL, spot JLP and SKR with no emergency path.
- **Failure:**
  - (a) JLP exploit or SKR crash mid-epoch (Season lasts 5 days). An attacker buys crashed units cheaply, registers ("a later join accrues from its finalized registration") and is valued at the pre-crash reference, reaching full `w=2.5` for a fraction of the capital. That defeats the 1:1 coverage rule.
  - (b) Normal volatility does the same thing on a smaller scale: buy after an intra-epoch dip, get valued at the higher reference.
  - (c) xSOL in HOT, where roughly 6× leverage makes the gap large. Entry is allowed after consent (§9), and the reset only applies from the "authenticated event/checkpoint", which leaves a lag window.
- **Severity:** HIGH. **Breaks:** fairness (the reward share is captured without matched capital; `ΣR≤E` still holds).
- **Correction:**
  1. Value any units added or registered after the epoch reference timestamp at `min(reference_price, verified_price_at_add)`.
  2. Extend the §9 EXTREME/prospective re-reference to every admitted adapter, triggered by verified depeg, exploit, pause or program-upgrade signals.
  3. Make a reset effective from the first finalized slot where the trigger is observable on-chain, for all still-unsettled intervals, not from the human checkpoint.

### H-2 — The reference price can be manipulated at the snapshot (SKR, and JLP's USD→SOL conversion)
- **Rule:** §5 reference prices; §2 "Asset adapters/prices"; §14.6.
- **Failure:** SKR/SOL is thin. Pushing spot up at the reference timestamp inflates `X` for the whole epoch across many Sybil wallets. Because `min(K,X)` caps each wallet at 2.5, the attack does not raise any wallet's maximum; it removes most of the external capital needed to reach it.
- **Severity:** HIGH. **Breaks:** fairness.
- **Correction:** Require manipulation-resistant references: an oracle with a confidence bound, or a TWAP over a window, with the rejection rule defined in the adapter spec. A single spot sample is prohibited. Leave thresholds unset until calibrated (§9 rule), but make "no single-sample reference" normative.

### H-3 — Mutable `rig_price_lamports` breaks the 20% per-wallet proof
- **Rule:** §1 `Kmax=P·M`, `Hfloor=Kmax/qmax=12.5`; §10 proof `dR_i/dt ≤ r·2.5/12.5`; §2 "Production price must equal adopted P".
- **Failure:** `set_pricing` can change the price after launch. If Fruit's `K_i` uses the live on-chain price and the price doubles, `Kmax=5` while `Hfloor` stays 12.5, so a single wallet's share reaches 40%. The `R_i≤E/5` guarantee fails; `ΣR≤E` still holds.
- **Severity:** HIGH. **Breaks:** fairness and a published guarantee.
- **Correction:** Normatively, Fruit uses a frozen constant `P_ref = 10,000,000` lamports in `K_i`, independent of `Config.rig_price_lamports`. Alternatively, `Hfloor` is recomputed as `P_live·M/qmax` at each epoch commit and frozen for that epoch. Pick one; the first is smaller.

### H-4 — Mutable `purchase_fee_bps` breaks the referral margin bound after `c` is set
- **Rule:** §7 `(1−c)φ ≥ m`; "Exact integer settlement must also preserve the per-event retained margin".
- **Failure:** The authority lowers φ after `c` is fixed, so `(1−c)φ < m`. §7 gives no rule for what the commission becomes when the per-event check fails.
- **Severity:** HIGH. **Breaks:** fee-ledger solvency against the margin.
- **Correction:** Define `commission_e = min(floor(c·f_e), max(0, f_e − ceil(m·v_e)))`, computed in the same instruction that receives the fee. Pin the rounding direction of `required_margin_e` (ceil).

### H-5 — Referral commissions are unimplementable without base-program changes
- **Rule:** §7 "Reserve commissions atomically before any fee withdrawal".
- **Failure:** V2 `withdraw_fees` checks only `distributable(fee_vault) ≥ amount`. Any commission tracked off-chain is an unfunded promise the authority can drain in the same slot. The V2 migration notes say the `Miner` layout is unchanged, and adding a field would need a realloc migration of every Miner.
- **Severity:** HIGH. **Breaks:** solvency of referral promises, feasibility.
- **Correction:** Program changes in `buy_rigs` and `compound`: increment `Config.reserved_commissions` and a per-referrer `ReferralLedger` PDA (a separate PDA, so no Miner realloc) in the same instruction that moves the fee. Change `withdraw_fees` to require `distributable − reserved_commissions ≥ amount`. Add a referrer `claim_commission` instruction that decrements both. Until this ships, `c = 0`, as §14.4 already requires.

### H-6 — Delegation, freeze and Token-2022 authorities are not in the encumbrance exclusion
- **Rule:** §5 exclusion list; §5 attribution ("relevant Token-2022 changes").
- **Failure:**
  - An SPL `Approve` lets a delegate move `delegated_amount` at any time. Protocols use this in place of custody, so the tokens are effectively pledged while ownership never changes.
  - A frozen account cannot exit.
  - A Token-2022 mint with a permanent delegate or a transfer hook can seize or block units.
  - None of these show up as ownership changes.
- **Severity:** HIGH. **Breaks:** fairness (encumbered capital earns weight) and the normal-exit rule.
- **Correction:** Add these to `A_ij` failure conditions:
  - Count `amount − delegated_amount` only.
  - Exclude accounts with state Frozen.
  - Reject mints with permanent-delegate, transfer-hook or confidential-transfer extensions unless the adapter explicitly clears them.
  - Track `Approve`/`Revoke`/`SetAuthority`/`FreezeAccount`/`ThawAccount` in the replay timeline.

### H-7 — Loyalty tenure can be farmed with dust
- **Rule:** §6 `d_i` = consecutive days with `w_i>0`; `v_i = w_i β_i`.
- **Failure:** Keep 150 seeds (needed anyway) plus one lamport's worth of an admitted asset registered for 11 days, so `β=1`. Then move real capital in only when wanted. Rotate one 2.5 SOL block across N such wallets and each one earns full-β loyalty on the full weight while it holds the block. Loyalty ends up rewarding seed tenure plus dust, not sustained capital.
- **Severity:** HIGH. **Breaks:** fairness of the loyalty pool (bounded by `B_loy`; not solvency).
- **Correction:** Founder must define a qualifying day for tenure using an existing constant. Smallest version: a day counts only while `w_i ≥ P·T` (the 1.5 SOL-equivalent 60% threshold already used for seeds). Alternatively, apply `β` to the low-water mark of `w` over the tenure window. Until chosen, set `B_loy = 0`.

### H-8 — Fruit vault separation is required but not pinned
- **Rule:** §10 "For each independently funded reward ledger"; "Base and Fruit balances cannot back each other twice".
- **Failure:** If Fruit SOL is deposited into the V2 REWARD vault, the base `assert_fully_funded` (vault lamports vs base outstanding) passes with Fruit lamports and stops detecting a base shortfall. Fruit claims from the same vault then spend base backing.
- **Severity:** HIGH. **Breaks:** solvency (cross-backing).
- **Correction:** Normatively, use distinct program-owned vault PDAs for: base rewards (existing), Fruit core, loyalty, lock and referral commissions (or one Fruit vault with on-chain per-ledger counters and a per-ledger `V_k ≥ L_k+U_k+Z_k` assertion on every instruction). No Fruit lamport ever enters the base REWARD vault.

### H-9 — An external protocol upgrade can silently corrupt adapters
- **Rule:** §9 "verifying the deployed version"; §14.6 "Pin asset/program versions".
- **Failure:** Hylo, SKR staking or Jupiter upgrades a program and account layouts shift. The adapter misreads CR, NAV or stake beneficiary and assigns wrong states or weights. Fable's "program upgraded" overlay was dropped in the final text.
- **Severity:** HIGH. **Breaks:** security and fairness.
- **Correction:** Each adapter pins the `programdata` last-deploy slot or hash for every program it decodes. Any change sets that asset to UNKNOWN (pause new recognition, reserve per §10) until the adapter is re-verified.

### H-10 — One asset's data outage freezes the whole common pool
- **Rule:** §9 "Stale or missing data is UNKNOWN ... reserve existing unsettled capacity"; §10 "retain the entire unsettled scheduled capacity for the affected common pool interval".
- **Failure:** An SKR/SOL feed outage makes the weight of every SKR wallet unknown. `H` becomes unknown, so settlement stops for all participants on every asset. A weak adapter becomes a pool-wide denial-of-service.
- **Severity:** HIGH. **Breaks:** availability and implementation feasibility (a solvent failure).
- **Correction:** The per-asset epoch reference must be committed on-chain before the epoch starts. An asset with no committed reference is inactive for that epoch, deterministically, known before `t_start`, weight 0, not UNKNOWN. UNKNOWN reserve treatment then applies only to mid-epoch ownership gaps, where it is unavoidable.

### H-11 — Time and ordering convention unspecified
- **Rule:** §3 half-open intervals; §5 "integrate ownership over disjoint intervals"; §14.5.
- **Failure:** Several slots share one `unix_timestamp`. Clock is stake-weighted and can drift from wall time. `getBlockTime` can return null. If the replay runs on per-second snapshots, two owners in the same second double-count, or a transfer at exactly `t_s`/FRUIT_T0 is classified inconsistently between on-chain and off-chain code.
- **Severity:** HIGH. **Breaks:** double-counting and fairness.
- **Correction:** Normatively:
  - Event order is `(slot, tx_index, ix_index, inner_ix_index)`.
  - Event time is the bank `Clock.unix_timestamp` of that slot, the same value the base program reads for T0.
  - Intervals are event-ordered, not per-second samples.
  - Boundary equality is resolved half-open against the same Clock value.
  - Commitment level is `finalized` only.

### H-12 — `.skr` domain hopping multiplies Day-1 access
- **Rule:** §3 Day-1 `.skr` early access; §4 check 1; §6 Day-1 tenure.
- **Failure:** `identity::prove` checks only current ownership. Wallet A buys 250, transfers the domain (or its NFT wrapper) to B, and B buys 250, and so on. One domain gives unlimited Day-1 wallets. Fruit spends zero on Day 1, but base genesis emission (hyperbolic, favoring the earliest cohort) and a loyalty tenure head start are multiplied.
- **Severity:** HIGH (fairness). §11 acknowledges .skr is not unique ownership but never states that one domain serves unlimited wallets.
- **Correction:** Founder choice, stated explicitly: either (a) accept and disclose that the Day-1 gate is per domain at purchase time and reusable, or (b) add a `["skr_used", name_account]` PDA written on the first SkrGenesis purchase that binds the domain to one Miner. (b) is a base-program change within CB-3's upgrade.

### H-13 — Legacy S>250 is labelled unresolved, but §4's rules already determine it
- **Rule:** §4 "Existing S>250 positions cannot simply be clipped ... Resolving them is a deployment-version prerequisite".
- **Failure:** Checks 2 and 3 already produce one outcome: base shares untouched, no growth (`n_minted = min(n, max(0, 250−S)) = 0`, purchases reject), Fruit clipped to 250. Leaving it "unresolved" invites an implementer to write a clipping migration, which is confiscation.
- **Severity:** HIGH (implementation risk).
- **Correction:** State it as the rule: legacy positions are frozen and grandfathered. Base emissions continue on existing seeds; no purchase or compound mints; Fruit recognizes `min(S,250)`. No migration mutates `Miner.rigs`. Legacy wallets can still claim base rewards.

### H-14 — EXTREME trigger griefing and flapping
- **Rule:** §8 safety release "on verified EXTREME event or actual route/protocol unavailability"; §9 EXTREME row.
- **Failure:**
  - "Route unavailability" is undefined. A transient DEX quote failure, or a pool briefly drained by an attacker, can trigger safety release and let locked users take pro-rata vesting instead of the cliff forfeit (cost bounded by `B_lock`).
  - CR oscillating around 1.00 causes repeated prospective resets.
- **Severity:** HIGH→MEDIUM. **Breaks:** fairness in the lock pool.
- **Correction:** Define route unavailability per adapter as the documented protocol mint/redeem/unstake path being halted per on-chain state, not DEX quote depth. Add a minimum-dwell/hysteresis parameter, left unset until calibrated, and forbid enabling EXTREME-triggered release until it is set.

### H-15 — Disputed reserves can be stranded forever
- **Rule:** §10 "Never release disputed funds merely because the epoch ended. Release only after complete reconciliation".
- **Failure:** History becomes permanently unrecoverable (a pruned RPC with no archive, or an ambiguous event). `U` is locked forever with no terminal procedure.
- **Severity:** MEDIUM. **Breaks:** reserve liveness (not solvency).
- **Correction:** Add a terminal resolution procedure: after an archive search of at least two independent providers fails, a published final settlement is signed by the CB-2 signer set and computed from the best available finalized data. Excess reserve returns to `Z`; existing `L` is never reduced.

---

## 3. ARCHITECTURE CONTRADICTIONS

| # | Sections | Contradiction | Severity / breaks | Correction |
|---|---|---|---|---|
| AC-1 | §1 `P` constant vs §2 "production price must equal P" vs V2 `set_pricing` | `P` is used as an immutable proof constant but is mutable on-chain | HIGH / fairness | See H-3: freeze `P_ref` for Fruit |
| AC-2 | §5 "fixed epoch reference prices" vs §9 emergency reset (xSOL only) vs §5 table (other candidates) | Only xSOL has any intra-epoch correction path; other admitted assets have none | HIGH / fairness | See H-1: generic adapter EXTREME and `min(ref, live)` for mid-epoch adds |
| AC-3 | §4 compound rule and §10 "Compound earned SOL" vs Fruit ledger | Unclear whether Fruit, loyalty or lock SOL can be compounded into seeds. If yes, it is a new seed-creation path (needs a cap check, fee/commission event, penalty-anchor rule, and a cross-vault transfer from a Fruit vault into base capital/fee vaults) that V2 `compound` cannot express | MEDIUM / feasibility | State explicitly: V1 of Fruit has no compounding of Fruit/bonus SOL; the §10 compound row applies to the base ledger only. Or specify the new instruction in full |
| AC-4 | §4 "At the cap, compound is a no-op with no fee" vs V2 engine | The engine reverts with `NothingToCompound` when count is 0 | LOW / feasibility | Accept a revert as the at-cap behavior (no state change, no fee, rewards retained) and write the test on that |
| AC-5 | §4 check 1 vs check 2 | Seeds are non-redeemable, so `S ≥ lifetime_rigs_bought` and check 2 strictly dominates check 1 while both are 250 | LOW / none | Keep check 1 only as a defence against misconfiguration; document the redundancy; require `genesis_cap_rigs == M_hold` in pre-ops |
| AC-6 | §3 "do not spend or retroactively catch up unearned capacity" vs §6/§10 "outage freezes settlement pending replay" | Undefined split between "accrual prevented" (no catch-up) and "accrual occurred but unobserved" (replay pays) | MEDIUM / fairness, disputes | Define: if the on-chain registration or verification program was unavailable, the interval is not earned. If only the indexer or data was unavailable while on-chain state existed, the interval is earned and settled on replay |
| AC-7 | §6 "voluntary loss resets" / "involuntary freezes" | Unclassified causes: user exits during WARM/HOT, an asset is inactive because its reference is missing (H-10), an adapter goes UNKNOWN (H-9), the delegate moves tokens (H-6) | MEDIUM / fairness | Publish a cause table: user-signed tx → voluntary. Protocol, data or adapter state → involuntary freeze. Third-party authority movement (delegate, permanent delegate) → voluntary (the user granted it) |
| AC-8 | §5 "one selected asset/adapter per wallet" vs wallet SKR / staked SKR rows | Unclear whether SKR in the wallet and SKR in a staking account are one asset (multiple accounts) or two adapters | LOW / feasibility | Define "asset" at the underlying-mint level: one selection covers the wallet ATA(s) and the verified stake beneficiary account, counting each unit once, with pending-unstake excluded |
| AC-9 | §8 "registered position must satisfy the Season price/ownership checks" vs registration window `[FRUIT_T0, t_s)` | The Season reference price exists only at `t_s`, so it cannot validate a registration made before `t_s` | LOW / feasibility | Registration stores only intent `(d_i, asset, requested_weight)`. All price/ownership validation happens at `t_s`, as `ell_i = min(requested, w_i(t_s))` already implies |
| AC-10 | §1 `APR_pool_Cherry` uses `Cn = P Σ_active S_i` | "Active" is undefined (all seed holders? qualified? registered?) | LOW / display | Define `Cn` over all wallets with `S ≥ T` and a registered selection |

---

## 4. MISSING TESTS

All tests assert `V_k ≥ L_k + U_k + Z_k` per ledger and in aggregate after every transition, plus `ΣR ≤ E` and `R_i ≤ E/5` per pool.

**Cap / base program**
1. Fresh purchase when `S=249, count=2` reverts atomically with no lamport movement. `S=248, count=2` succeeds.
2. Compound with `S=240` and pending that buys 50 mints exactly 10, debits exactly `quote(10)` and leaves the remainder claimable.
3. Compound at `S=250` reverts: no fee, pending unchanged, penalty anchor unchanged.
4. Legacy fixture `S=400`: purchase reverts, compound reverts, base claim works, base shares unchanged, Fruit `K=2.5`.
5. Referred wallet (weight 105) at the cap: `total_shares` includes `250·105` exactly.
6. After `set_pricing(price×2)`, Fruit `K` still uses `P_ref` (H-3). After `set_pricing(fee↓)`, commission is clamped (H-4).
7. V2→V3 migration: byte and lamport identity on failure, one-shot, and all instructions fail cleanly during the inert window.
8. `genesis_cap_rigs ≠ 250` on the deployed fixture is detected in pre-ops.
9. The activation purchase is counted against both check 1 and check 2.

**Ownership / attribution**
10. A→B→A across slots: A earns only A's intervals and B only B's.
11. A→B→C inside one transaction: no one earns extra duration.
12. Transfers in different slots with the same `unix_timestamp`: zero overlap.
13. Token-account `SetAuthority(AccountOwner)` is treated as a transfer.
14. `Approve` a delegate for the full balance: `X` drops by `delegated_amount`. `Revoke` restores it prospectively only.
15. Frozen account: excluded.
16. Token-2022 mint with permanent delegate or transfer hook: rejected.
17. A flash-loan-sized intra-transaction balance: zero weight.
18. One missing transfer event: the whole common-pool interval is reserved, not only the affected wallet's `r·Δt·w_max/H`.
19. Staked SKR with pending unstake: excluded. Same units in wallet and stake views: counted once.
20. Debt in a pinned lending program: `X` is netted at wallet level (CB-1).

**Pricing / Safe Mode**
21. Units added after the reference timestamp are valued at `min(ref, live)` (H-1).
22. Reference sample manipulated in one block: rejected by the confidence/TWAP rule (H-2).
23. Asset has no committed reference at epoch start: inactive with weight 0, and other assets settle normally (H-10).
24. Adapter `programdata` changes mid-epoch: asset becomes UNKNOWN and is reserved (H-9).
25. HOT entry: rejected without a consent signature. Rejected with an expired, wrong-epoch, wrong-risk-version or replayed nonce. Accepted with a valid one.
26. HOT entry to an unavailable route: rejected even with a signature.
27. EXTREME at slot s: reset effective from s for unsettled intervals, and settled `R`/`Q` unchanged.
28. CR oscillating across 1.00: hysteresis prevents repeated safety releases.
29. The CR = 1 display path: no finite leverage shown.

**Epochs / timeline**
30. A purchase at `T0+86,399` gets the Day-1 cap and no Fruit. A purchase at `T0+86,400` gets public access and Storm accrual.
31. Registration finalized at `t_s−1` is included. Registration at `t_s` is excluded.
32. Every boundary at 86,400 / 259,200 / 518,400 / 950,400 from FRUIT_T0 is half-open.
33. `F = R`: Storm is 0. `F = R−1` lamport: a single 48-hour Harvest of F with no Season or lock. `F = 0`: inactive.
34. Zero participants for a whole epoch: all capacity goes `U→Z`, APR shows N/A.
35. `Cw < 12.5`: the unallocated share goes `U→Z` and never to other wallets.

**Loyalty / lock**
36. Dust-tenure wallet: behavior under the H-7 rule.
37. Penalized base claim resets tenure (sourced from instruction/account data, not only logs). An unpenalized claim does not.
38. Lock with every election at 5 days, with nobody locking, and with a partial shortfall (low-water mark, no resurrection on re-top-up).
39. Early exit followed by a second election attempt: rejected.
40. EXTREME release: vested progress uses the pre-event weight; the future part goes to `Z` only after ownership resolves.
41. Indexer gap at `t_s`: all of `B_lock` stays reserved until `H_lock` is final.

**Settlement / claims**
42. Root with `Σleaf > floor(E·elapsed/T)`: rejected on-chain.
43. Leaf `> E/5`: rejected.
44. Root replayed across pools or versions: rejected.
45. Corrected root that lowers a wallet already claimed: that wallet's claim pays 0 and other wallets are unaffected.
46. Two claims for the same wallet in one slot: the second pays 0. N wallets claiming concurrently: `V` never drops below `L`.
47. Checkpoint-frequency independence: 1 checkpoint vs 86,400 checkpoints produce totals within the carried-remainder bound.
48. The final lamport remainder is released exactly once.

**Referral**
49. `withdraw_fees` for more than `distributable − reserved_commissions`: rejected.
50. Commission on a compound fee is reserved in the compound instruction.
51. Fee event replay: consumed once.
52. `c·f_e` rounding keeps the retained margin per event.
53. Self-referral rejected. A referrer that is not a planter is rejected (existing test; extend it to the hard-cap build).

**Funding**
54. Committing a bonus pool when `Z < B`: rejected.
55. Committing the same lamport to renewal and loyalty: rejected.
56. Reserve withdrawal attempting to touch `L` or protected `U`: rejected.
57. Funding exhaustion mid-sequence: already committed tranches still finish.

---

## 5. REQUIRED CORRECTIONS BEFORE IMPLEMENTATION

1. **CB-1:** Founder adopts the scoped, enforceable debt definition: wallet-level netting against a pinned lending registry, plus the disclosure. Replace per-position `D_ij`.
2. **CB-2:** Write the on-chain Fruit ledger spec: per-ledger vault/counters, root schema, on-chain `Σleaf ≤ scheduled capacity` and `leaf ≤ E/5`, challenge window with an independent veto, idempotent cumulative claims.
3. **CB-3:** Run deployed-state pre-ops. Put the hard cap in the engine's pure `buy_rigs`/`compound`. Pin `M_hold`. Plan the V3 migration. Require `genesis_cap_rigs == 250`.
4. **H-13:** Replace "legacy S>250 unresolved" with the grandfather-frozen rule. No `Miner.rigs` mutation.
5. **H-3 / AC-1:** Freeze `P_ref = 10,000,000` lamports inside Fruit weight math.
6. **H-1 / AC-2:** Value mid-epoch adds at `min(ref, live)`. Add a generic adapter EXTREME trigger with on-chain-observable effective time.
7. **H-2:** Require manipulation-resistant references (no single spot sample).
8. **H-10:** Commit per-asset references before the epoch starts. A missing reference makes the asset inactive for the epoch, not UNKNOWN.
9. **H-11:** Pin the event ordering, time source, commitment level and half-open boundary rule.
10. **H-6:** Add delegate, frozen and Token-2022 extension exclusions to `A_ij`.
11. **H-9:** Pin adapter program versions. An upgrade sets the asset to UNKNOWN.
12. **H-8:** Mandate separate Fruit vault PDAs, or per-ledger on-chain counters with per-ledger assertions. No Fruit SOL in the base REWARD vault.
13. **H-4 / H-5:** Clamp commissions against the per-event margin. Specify base-program changes (reservation counter, ReferralLedger PDA, gated `withdraw_fees`, `claim_commission`). Keep `c = 0` until shipped.
14. **H-7:** Founder defines a tenure qualifying day with a non-dust threshold. Keep `B_loy = 0` until then.
15. **H-12:** Founder states whether Day-1 `.skr` access is reusable per domain or bound to one wallet.
16. **H-14:** Define "route unavailability" from protocol on-chain state and add a hysteresis parameter (unset until calibrated).
17. **H-15:** Add the terminal disputed-reserve resolution procedure.
18. **AC-3 … AC-10:** Apply the one-line clarifications in the §3 table above.
19. Add all §4 tests to the §14.8 gate.

---

## 6. IMPLEMENTATION-READINESS VERDICT

**NEEDS CORRECTION**

No insolvency path was found in the equations. `ΣR ≤ E`, the prefunded U/Z accounting and the non-reuse of lamports hold as written.

Implementation cannot start for three reasons:

- **CB-1:** the borrowed-capital rule as written admits nothing.
- **CB-2:** the settlement layer that must enforce the solvency and per-wallet guarantees is unspecified.
- **CB-3:** the hard cap depends on a base-program upgrade against unaudited deployed state.

H-1 through H-8 are fairness or cross-ledger failures that must be closed in the spec before code. Isolated accounting and test work against fixtures (§14 last paragraph) can proceed now.
