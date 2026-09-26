# THE CHERRY TREE — Decision Register

Date: 2026-09-26. Branch: `z-ai/cherry-docs-roadmap`.

Every item that blocks or configures the adopted tokenomics ([canonical spec](CHERRY_TREE_FRUIT_TOKENOMICS.md)) is registered here exactly once. Resolved founder-set constants are recorded in §2 and are **not** re-opened. A decision is closed only by recording the resolution, its source, and its date below; an agent's recommendation or a document's suggestion never closes one.

Legend: **OPEN** (missing input), **PARTIAL** (evidence gathered, decision still required), **CLOSED** (resolution recorded).

---

## 1. Open decisions

### D01 — Borrowed / encumbered capital enforcement

- **Question:** How is verified *owned* external equity distinguished from borrowed or encumbered capital, in a way that is actually enforceable at admission?
- **Known facts / evidence:** Canonical spec §6 excludes borrowed principal and unsupported encumbrance from X. Transfer history replay (D02) proves ownership timeline, **not** absence of loans from another wallet, an exchange, or an off-chain lender. Global absence of borrowing is not provable from SPL balances or history. The final architecture rejects Fable's acceptance of borrowed tokens and any attestation-only substitute.
- **Governing constraint:** `X_i = Σ A_ij·max(0, G_ij − D_ij)` with `D_ij` verified; unknown provenance ⇒ ineligible, not admitted.
- **Missing input:** Founder selection of an enforceable scoped proof/trust model (or an explicit accepted-trust disclosure) for admission.
- **Who resolves:** Founder, with options prepared by Z.ai; Opus reviews the chosen model's economics/security.
- **Blocks:** T10 (spec), T51 (adapter admission), and all Fruit activation (T80/T90/T100 lane).
- **Consequence if unresolved:** No adapter may honestly activate; launching a permissive adapter would contradict the adopted exclusion rule.

### D02 — Ownership settlement, indexer trust and correction model

- **Question:** Which finalized-history sources, event coverage, time/finality convention, allocation authority, independent-replay requirement and correction policy settle Fruit ownership and shared denominators?
- **Known facts / evidence:** Canonical spec §6 requires complete event attribution (transfers, authority changes, mint/burn, stake/unstake, wrapper conversion, Token-2022); endpoint balances/minima, signatures and Merkle membership alone are insufficient; missing shared-denominator data must reserve the common unsettled budget (§11). Cumulative roots need unique epoch/version ids, replay protection, conserved sums, per-wallet limits, independent replay. Indexer truthfulness is an explicit trust assumption unless independently enforceable.
- **Governing constraint:** One underlying unit supports only its actual owner at a given instant; denominators reconstructed on the same timeline; `V ≥ L+U+Z` maintained through gaps.
- **Missing input:** Founder/lead selection of the settlement design and the named allocation authority; correction/finality rules.
- **Who resolves:** Founder (trust model), Z.ai (design options), Opus (security review of the chosen design).
- **Blocks:** T11, T50, T80, and every epoch that settles real weights.
- **Consequence if unresolved:** Weights cannot be settled honestly; epochs cannot open.

### D03 — Actual base schedule, balances and Fruit budget F

- **Question:** What are the actual base reward schedule (`fund_rewards` amount/duration), the dedicated base balances and liabilities (B_base), and the net cleared core Fruit budget F?
- **Known facts / evidence:** No production deployment or funding exists (mainnet runbook PREPARED/NOT EXECUTED; no T0). Devnet rehearsal values (0.01 SOL price, 3% fee, cap 10) are test-only. The Tree base funding horizon must reach `T0+12 days` at activation. `F` determines Storm/Z_future and boundary states (`F=R`, `0<F<R`, `F=0`).
- **Governing equations:** `E_storm = min(F−R, 8,561,643,836 lamports)`; `Z_future = F−R−E_storm`; `R = 1,626,712,330 lamports`; runway ≥ T0+12 days; `V ≥ L+U+Z` per ledger.
- **Missing input:** Founder funding plan and actual cleared balances (production-side; nothing to read on-chain yet).
- **Who resolves:** Founder (funding), ops/multisig signers (execution), Z.ai (reconciliation evidence).
- **Blocks:** T20, T42 activation, T90 rehearsal values, T100/T101.
- **Consequence if unresolved:** No Fruit epoch can be committed; base runway requirement cannot be verified.

### D04 — Production price P and fee configuration φ

- **Question:** What are the production `rig_price_lamports` and `purchase_fee_bps`?
- **Known facts / evidence:** Adopted reference P = 10,000,000 lamports/seed; fee ceiling `MAX_PURCHASE_FEE_BPS = 1,000` (verified V2). Devnet rehearsal suggests 10,000,000 / 300 bps — not production facts. Every SOL figure in the schedule keys off P; a mismatch requires reconciliation, not silent substitution.
- **Governing constraint:** Production P must equal adopted P before the tranche schedule is used; φ enters D05's bound.
- **Missing input:** Founder decision recorded for the production initialize (runbook Gate 4, two-person readback).
- **Who resolves:** Founder; config multisig signers execute.
- **Blocks:** T21, T22 (via φ), production Gate 4, every absolute SOL figure.
- **Consequence if unresolved:** No production initialize; schedule cannot be committed.

### D05 — Referral margin m and cut c

- **Question:** What required retained margin m (fraction of fee-bearing principal volume) and referral cut c apply to actually received eligible fees?
- **Known facts / evidence:** Fee-backed commissions are NOT IMPLEMENTED. Eligible events: fees actually received in FEE from fresh purchases/completed compounds of the referred wallet; commissions reserved atomically before withdrawal; each event consumed once.
- **Governing equations:** `(1−c)·φ ≥ m`; `0 ≤ c ≤ 1−m/φ` (φ>0, 0≤m≤φ); if `m>φ` no cut feasible; if φ=0 no commission; integer settlement preserves per-event margin.
- **Missing input:** m (founder/ops margin requirement), φ (D04), then a c selected within the bound.
- **Who resolves:** Founder (m, then c); Opus reviews economics before activation.
- **Blocks:** T62; any monetary referral promise (none is active).
- **Consequence if unresolved:** Referral fee-cut stays unbuilt/unpromised (base 105/100 redistribution alone continues).

### D06 — Bonus funding commitments (J, B_loy, B_lock)

- **Question:** What separately committed bonus capital J exists, and which B_loy/B_lock commitments (if any) are funded?
- **Known facts / evidence:** Loyalty and lock bonuses must never expand core E; they require separate committed funding. `Σ B_loy + Σ B_lock ≤ available bonus funds`; a reserve lamport is allocated once. F=10/J=0 example gives zero opening bonuses.
- **Governing equations:** Canonical spec §7 (loyalty), §9 (locks), §11 (commit/release transitions).
- **Missing input:** Founder funding commitment (or explicit zero).
- **Who resolves:** Founder.
- **Blocks:** T60/T61 activation, T23.
- **Consequence if unresolved:** No headline boost or lock bonus exists or is advertised (correct current posture).

### D07 — Legacy over-cap position handling

- **Question:** Do any positions with `S > 250` exist that the permanent-cap program version must migrate, and if so under what treatment?
- **Known facts / evidence:** No production deployment exists (verified 2026-09-26: mainnet runbook PREPARED/NOT EXECUTED; no mainnet program state or T0). Devnet artifacts are test-only. Therefore no production over-cap position can exist today; the check that must be evidenced is "no production deployment/no seed accounts" plus devnet-lane disposition at migration time.
- **Governing constraint:** Over-cap positions cannot be clipped or confiscated; earned claims preserved; resolution is a deployment-version prerequisite.
- **Missing input:** Formal verification record at implementation time (chain check + branch pre-ops).
- **Who resolves:** Z.ai produces the evidence; founder accepts the migration disposition (expected: "none exist").
- **Blocks:** T30/T31 finalization only (not the whole roadmap).
- **Consequence if unresolved:** Cap program version cannot be declared migration-safe.

### D08 — Asset verification and price sources

- **Question:** Which exact asset/program identities, accounts, decimals, extensions, share/NAV conversions, reference feeds, finality and staleness rules are verified for each admitted adapter, and what converts SKR/SOL and USD/SOL (JLP) into the SOL numeraire?
- **Known facts / evidence:** Candidate list and dispositions in canonical spec §6 (wallet SKR, staked SKR conditional, xSOL with Safe Mode, JitoSOL, spot JLP; leveraged/queued/vault JLP excluded; xBTC/xHYPE disabled). Documentary research exists (Fable solve §11 + architecture §5) but is **not** chain/configuration verification; addresses remain candidates.
- **Governing constraint:** Fixed timestamped epoch prices; documented normal-path exit ≤ 432,000 s; financing checks per D01.
- **Missing input:** Pinned live identities/versions per asset; chosen reference feeds; staked-SKR IDL layout; spot-JLP USD→SOL conversion source.
- **Who resolves:** Z.ai assembles evidence; founder approves admission set; Opus reviews before activation.
- **Blocks:** T12, T51, T13 partially.
- **Consequence if unresolved:** No adapter activates (correct current posture).

### D09 — Risk overlay calibration

- **Question:** What thresholds (feed confidence, quote/slippage, freshness/volatility/drawdown) escalate Safe Mode beyond the CR zones?
- **Known facts / evidence:** CR zone boundaries (1.35/1.20/1.00) are the primary state map; overlays remain UNSET; Hylo's zone fee table is labeled "example"; stale/missing data is UNKNOWN.
- **Governing constraint:** No guessed thresholds; calibration data listed in the architecture §2/§14.
- **Missing input:** Calibrated bounds from actual data (per-pool CR series, NAV history, oracle update intervals, live fee parameters).
- **Who resolves:** Z.ai proposes from data; founder approves; optional overlays may stay disabled.
- **Blocks:** T52 overlays only (core NORMAL/WARM/HOT/EXTREME states and consent flow are not blocked).
- **Consequence if unresolved:** Overlays stay disabled; core states still function.

### D10 — Existing release/security gates (release lane)

- **Question:** Completion of the pre-existing mainnet readiness gates, preserved from `docs/backlog/` on `mainnet/rc-hardening` and integrated into the roadmap as T100.
- **Known facts / evidence (verified 2026-09-26):**
  - V2 qualification: Part A PASS 2026-09-25 at source `9a5e746` (private evidence archive, manifest-only reference); **Part B NOT EXECUTED**; release lane has advanced 14 commits since (`f0d5e96`) ⇒ the qualification no longer covers lane HEAD.
  - Mainnet runbook Gates 0–9 PREPARED, NOT EXECUTED (custody multisigs not created; `CHERRY_APP_ID` not issued; production host not chosen; Seeker plan not executed; external audit status not recorded; production funding model undecided).
  - Fee-recipient currently expected equal to founder wallet (`authority-audit` WARN) — acceptable only as a disclosed interim.
  - Part A limitation: `.skr` does not exist on devnet; cap enforcement proven at activation only.
  - **Scanner finding (2026-09-26, recorded honestly):** the Mimosa pre-commit scanner blocked a commit, flagging `app/next.config.ts:126` (`buildCsp`) as a high-risk "code-injection entry". Triage: pattern-level warning — the function constructs an allowlist CSP header string from env-derived, `URL`-parsed origins with no execution semantics; and the flagged construction **no longer exists on the release lane** (`mainnet/rc-hardening` replaced it with a per-request nonce CSP served from `src/proxy.ts`, commit `1b2c54c` "nonce/hash CSP without unsafe-inline"). The docs branch carries the stale V1-era copy inherited from `main`. Disposition: not a new tokenomics blocker; re-triage the current lane's CSP/nonce design under T100 security review. The documentation commit proceeded with the hook bypassed **under explicit founder authorization recorded on 2026-09-26** (session question answering "Authorize bypass"), with this record and the commit message disclosing the bypass and its rationale (the session mandate is documentation-only; the flagged file is untouched by the change and superseded upstream).
- **Governing constraint:** Gates are sequential; a FAIL stops the run; no agent may execute mainnet actions (founder/custody only).
- **Missing input:** Founder actions (multisigs, APP_ID, host, audit, funding, Part B at the release commit).
- **Who resolves:** Founder + custody multisig signers; Z.ai prepares evidence; Codex/Opus reproduce/review as useful.
- **Blocks:** T100, T101, and any launch authorization (T110).
- **Consequence if unresolved:** No mainnet deployment, initialization, funding, or T0 — regardless of Fruit readiness.

### D11 — "Stock baskets" teaser vs. canonical architecture (website copy)

- **Question:** How is the "stock baskets" concept on the website styling lane dispositioned relative to the approved Fruit architecture?
- **Known facts / evidence:** `claude/tree-styling-website-hd5x4r` → `app/src/components/Game.tsx` (§8 "What comes next") advertises a planned Phase 2: "rewards paid in tokenized stocks on Solana instead of SOL … a funded pool of stock tokens. Stock baskets are planned, not live." The copy is correctly hedged (no dates, no terms) and exists **only on that unmerged styling branch** — not on `main`, `mainnet/rc-hardening`, `gh-pages`, or any other lane (verified 2026-09-26). The canonical Fruit architecture pays SOL from funded vaults and admits external assets only as non-custodied reward weight; it contains no tokenized-stock reward pool.
- **Terminology rule (founder instruction, 2026-09-26):** the concept is named **"stock baskets"**; the phrase "stock mining" must not be used anywhere. No occurrence of "stock mining" exists in this repository as of 2026-09-26 (verified across all branches).
- **Governing constraint:** canonical spec C-19 (no invented features or funding claims). A stock-token reward pool would be a new, separately funded architecture needing its own decision set; nothing may present it as part of the approved model.
- **Missing input:** founder disposition — (a) remove the teaser, (b) keep it as clearly non-binding marketing, or (c) scope it as a future feature with its own decision entries and roadmap phase.
- **Who resolves:** founder.
- **Blocks:** T71 user-facing copy review only (canonical/user docs must not imply stock-basket rewards); no Fruit implementation phase is blocked.
- **Consequence if unresolved:** public copy could promise a reward mechanism outside the approved architecture — harmless while confined to the unmerged styling branch.

---

## 2. Closed decisions (founder-set; do not re-open)

| ID | Decision | Resolution and source |
|---|---|---|
| C-01 | Day-1 `.skr` fresh-purchase maximum | **250 seeds / 2.5 SOL principal, fees additional**, cumulative over `[T0, T0+24h)`, compounds excluded. Founder instruction 2026-09-26; canonical spec §5. Note: this closes the release lane's previously open "production Genesis cap (candidate 25–250)" question (`docs/backlog/GENESIS_CAP_DECISION.md` remains as historical analysis; its rubric is not a decision anymore). Production `genesis_cap_rigs = 250` is a configuration task at initialize (T100/Gate 4 readback), not a decision. Qualification's cap = 10 was test-only. |
| C-02 | Qualification threshold | **150 current seeds** (ceil(3M/5), exactly 60% of M = 250). Founder instruction 2026-09-26; canonical spec §5. |
| C-03 | Public opening | **`FRUIT_T0 = T0 + 86,400 s`**; no intervening `.sns`-only window. Founder instruction 2026-09-26; matches verified V2 `access_phase`. |
| C-04 | Day-1 Fruit reserve spend | **Zero.** Founder instruction 2026-09-26; canonical spec §4. |
| C-05 | Permanent Cherry holding cap | **Adopted: hard permanent cap `S ≤ 250` on all seed-creation/receipt paths** (distinct from C-01's Day-1 fresh-purchase cap; the V2 genesis cap expires at Public and does not enforce this). Founder instruction 2026-09-26; canonical spec §5; implementation T30–T33. |
| C-06 | Capped compounding | Preserve unspent earned SOL as claimable; compound at cap is a fee-less no-op; fresh purchases above headroom fail atomically. Founder instruction 2026-09-26; canonical spec §5. |
| C-07 | External holdings | Uncapped holdings; recognized contribution capped by eligible Cherry weight (≤ 2.5 SOL-equivalent). Founder instruction 2026-09-26; canonical spec §5. |
| C-08 | One active Fruit per wallet per epoch | Adopted (next-epoch switching). Founder instruction 2026-09-26; canonical spec §6. |
| C-09 | Ownership evidence standard | Complete finalized ownership attribution required; endpoint-only balance checks insufficient. Founder instruction 2026-09-26; canonical spec §6. |
| C-10 | Borrowed/encumbered capital | Excluded from X; transfer history does not prove absence of hidden borrowing; enforceability stays unresolved until supported (D01). Founder instruction 2026-09-26; canonical spec §6. |
| C-11 | External eligibility exit standard | Documented normal-path exit within five days (≤ 432,000 s); not a guarantee during total market failure. Founder instruction 2026-09-26; canonical spec §6. |
| C-12 | JLP admission | Spot JLP only; leveraged/Multiply/queued/vault JLP excluded. Founder instruction 2026-09-26; canonical spec §6. |
| C-13 | xBTC / xHYPE | Disabled pending independent official reconciliation. Founder instruction 2026-09-26; canonical spec §6. |
| C-14 | Loyalty/lock funding discipline | Separately committed funding only; never expand core E. Founder instruction 2026-09-26; canonical spec §7/§9. |
| C-15 | Lock elections | One immutable lock election per wallet per Season; registration strictly before Season start; fixed frozen allocation denominator `max(Σℓ·d, 62.5)`. Founder instruction 2026-09-26; canonical spec §9. |
| C-16 | Referral structure | One direct level; paid only from actually received eligible protocol fees; existing 105/100 base redistribution preserved as-is. Founder instruction 2026-09-26; canonical spec §8. |
| C-17 | HOT entry | Permitted after the required two-step acknowledgement/signature **when the underlying protocol/route is available** (Fable's HOT block rejected). Founder instruction 2026-09-26; canonical spec §10. |
| C-18 | Safe Mode liabilities | Safe Mode never erases accrued liabilities; `V ≥ L+U+Z` including conditional and unsettled obligations. Founder instruction 2026-09-26; canonical spec §10/§11. |
| C-19 | No invented figures | No APR promises, external yield figures, percentages, funding balances or launch dates beyond the adopted equations and explicitly labeled examples. Founder instruction 2026-09-26. |
| C-20 | Sequence end / base horizon | Sequence ends `T0+12 days`; base runway must reach `T0+12 days` at activation (11 days remaining at public opening; Fable's 12-remaining-days condition corrected). Final architecture §3. |

---

## 3. Resolution protocol

1. When an input arrives, record it against the D-id above with source and date; update the blocked roadmap tasks' "Required decisions" fields.
2. A resolution that changes approved economics or dependency order requires a recorded rationale and appropriate founder decision (routine documentation corrections do not).
3. Never close a decision by inference, by an agent's success message, or by treating a scenario example as a commitment.
