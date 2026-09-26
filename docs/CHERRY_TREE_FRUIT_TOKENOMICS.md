# THE CHERRY TREE — Fruit Tokenomics (Canonical)

Date: 2026-09-26. Branch: `z-ai/cherry-docs-roadmap` (created from `claude/cherry-tree-tokenomics-cfoilj` @ `e807b5573e14997469897055e7c833309f5ebfc3`).

**Status: this is the single normative tokenomics document.** Where any other document disagrees, this one governs, subject to the precedence in §0.2. Nothing in this document asserts that any Fruit-layer feature is implemented, deployed, funded, or scheduled to launch.

## 0. Provenance, authority, and how to read

### 0.1 Provenance of this file

The historical baseline `CHERRY_TREE_FRUIT_TOKENOMICS.md` (562 lines, supplied out-of-band as two identical copies to the earlier design sessions) **does not exist anywhere in this repository** — not on any branch, not in any commit, not in any local workspace. This was verified on 2026-09-26 against all remote refs of `PhantomCapAI/cherry-tree-mining-rig-` and the full object history. Its content was therefore **not** available for section-anchored editing, and nothing has been reconstructed from memory.

This canonical document is instead constructed directly from the reconciled final architecture (§0.2), which self-describes as "the canonical replacement specification." The architecture's §13 edit plan against the missing baseline is thereby discharged by replacement rather than by in-place edits. The out-of-band baseline is superseded by this file wherever a copy of it still exists outside the repository.

### 0.2 Authority and precedence

1. Explicit founder instructions (most recent session, 2026-09-26).
2. [`docs/CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md`](CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md) — final reconciled architecture (kept unchanged as the full reference; commit `e807b55`).
3. [`docs/FABLE_CHERRY_TREE_TOKENOMICS_SOLVE.md`](FABLE_CHERRY_TREE_TOKENOMICS_SOLVE.md) — quantitative support, where consistent with the final architecture. The architecture explicitly rejects Fable edits E2/E18 (Day-1 cap 150), E4 (endpoint-minimum ownership), E5 (automatic whitelist activation), and E11 (HOT blocking), and corrects E9, E10, E13, E17.
4. Historical baseline and older documentation (superseded).

**Code establishes current behavior; it does not override intended requirements.** Current-behavior claims in this document cite the verified source in §2.

### 0.3 Label legend

| Label | Meaning |
|---|---|
| **ADOPTED** | Approved design. Binding on implementation. Not necessarily implemented. |
| **CURRENT** | Behavior verified in source at the cited commit. |
| **UNRESOLVED** | Configuration or decision still open; see the [decision register](CHERRY_TREE_DECISIONS.md). |
| **BLOCKED** | Adopted but cannot activate until dependencies (decisions, funding, verification) are complete. |

---

## 1. System overview

THE CHERRY TREE has two economic layers:

1. **The Tree (base layer).** Seeds (on-chain "rigs") are purchased with SOL; a separately funded REWARD vault pays a production share; earned SOL can be harvested or compounded into more Seeds. Vaults: REWARD, CAPITAL, FEE, kept separate so purchases never fund rewards. **CURRENT** on the release lane (§2).
2. **The Fruit layer (external-weight layer).** Seed-qualified wallets register externally held assets (never custodied by Cherry). Verified eligible external equity backs additional reward weight in separately funded Fruit epochs. **ADOPTED, NOT IMPLEMENTED** — no Fruit vault, epoch, weight, adapter, or settlement code exists on any branch (verified 2026-09-26).

---

## 2. Current implementation baseline (verified)

All CURRENT claims below were verified against source on 2026-09-26. "V1" = `main` @ `85ee955` (identical program/engine code on this docs branch); "V2" = `mainnet/rc-hardening` @ `f0d5e96` with engine blobs `state.rs` `f60b1f2a`, `buy_rigs.rs` `76c22acb`, `compound.rs` (engine `position.rs`) `91ade0dd` — the exact blobs cited by the final architecture.

| Feature | V1 (`main`) | V2 (`mainnet/rc-hardening`) | Note |
|---|---|---|---|
| Seed purchase, REWARD/CAPITAL/FEE vault separation | CURRENT | CURRENT | Unchanged by Fruit design. |
| Compounding: whole affordable seeds, exact remainder stays claimable | CURRENT | CURRENT | `rigs_engine::position::compound`; `NothingToCompound` if zero affordable. |
| Launch gate (founder activation sets T0; `[T0, T0+24h)` `.skr`-only; Public at `T0+24h` forever; **no `.sns` window**) | absent | CURRENT | `rigs_engine::launch::access_phase`; `SkrSnsEarly` variant retained but unreachable under V2. |
| Day-1 genesis cap: cumulative per-wallet fresh purchases ≤ `genesis_cap_rigs` during `[T0, T0+24h)` only; compounds excluded; set once at `initialize`/`migrate_v2`, no setter | absent | CURRENT (mechanism) | **Value is a deployment parameter, not a constant.** Devnet rehearsal env suggests 0.01 SOL price / 3% fee; qualification Part A used cap = 10, test-only. No production initialization has occurred. |
| Claim penalty 2,500/1,500/500 bps over the 24 h/48 h/72 h after last fresh plant | absent | CURRENT | Redistributed to other planters; not fee revenue. |
| Referral: one immutable referrer bound at first purchase; referrer must be a planter; no self-referral; weight 100 base / 105 referred; no payout | absent | CURRENT | Pure redistribution of funded emission; adds none. |
| Fee ceiling | `MAX_PURCHASE_FEE_BPS = 1_000` (10%) | same | Production fee UNRESOLVED (D04). |
| **Permanent per-wallet holding cap `S ≤ 250`** | **absent** | **absent** | No per-wallet lifetime/holding cap exists on any branch; only per-instruction (10^6) and global (10^12) bounds. **ADOPTED as a required protocol change (§5).** |
| Fruit vaults, epochs, external weight, adapters, Safe Mode, loyalty, locks, fee-cut referral | absent | absent | No code exists anywhere. |

Additional verified release-lane state (see [status](CHERRY_TREE_STATUS.md)):

- V2 qualification Part A (devnet) PASSED 2026-09-25 at source `9a5e746`; the release lane has since advanced 14 commits to `f0d5e96`, so qualification evidence lags HEAD. Part B (mainnet lane) NOT EXECUTED. Evidence is kept in the private local archive (`/home/bronzesneaks/cherry-qualification-evidence/`, not published); reference it, do not copy key material.
- Mainnet runbook Gates 0–9 (`docs/backlog/MAINNET_RUNBOOK.md` on `mainnet/rc-hardening`) are PREPARED, NOT EXECUTED. No mainnet deployment, initialization, funding, or T0 exists.

---

## 3. Adopted constants and core equations

**ADOPTED** (founder-fixed values are final and are not re-opened as decisions). SOL-equivalent for weights; SOL for explanatory budgets; integer lamports/seconds for settlement; one day = 86,400 seconds.

```text
P      = 0.01 SOL/seed = 10,000,000 lamports/seed     [reference; production value must match — D04]
C_day1 = 250 fresh-purchased seeds, Day-1 maximum     [2.5 SOL principal; fees additional]
M_hold = 250 current seeds, permanent holding ceiling [REQUIRES protocol change — §5]
M      = 250 seeds, permanent Fruit benchmark
T      = ceil(3M/5) = 150 current seeds               [qualification]
Kmax   = P·M = 2.5 SOL-equivalent
qmax   = 1/5
Hfloor = Kmax/qmax = 12.5 SOL-equivalent

K_i(t) = P · min(S_i(t), M)
X_i(t) = verified eligible external equity at epoch reference prices
w_i(t) = 1[S_i(t) ≥ T and eligibility verified] · min(K_i(t), X_i(t))
Cw(t)  = Σ_i w_i(t)
H(t)   = max(Cw(t), Hfloor)
r_p    = E_p / D_p                                  [SOL/day, funded epoch p only]
dR_i/dt = r_p · w_i(t) / H(t)
R_i,p   = ∫_epoch r_p · w_i(t)/H(t) dt
```

`X` is defined by attributed position ownership and admissibility (§6), never by a user-entered balance. With `K > 0`, coverage is `g = min(1, X/K)`. Seeds qualify at 150 (exactly 60% of M); positive Fruit accrual additionally requires positive eligible external equity; a dust position receives dust-proportional weight.

Ceilings and rates:

```text
Σ_i R_i,p ≤ E_p            R_i,p ≤ E_p/5           [per-wallet 20% ceiling]
APR_weight(t)      = 36,500 · r_p / H(t)                    [percent/year]
APR_pool_Cherry(t) = 36,500 · r_p · Cw(t)/(H(t)·Cn(t)),  Cn(t) = P·Σ_active S_i(t)
APR_i_Cherry(t)    = 36,500 · r_p · w_i(t)/(H(t)·P·S_i(t))
APR_i_combined(t)  = 36,500 · r_p · w_i(t)/(H(t)·(P·S_i(t)+X_i,total(t)))
```

Reporting rules (ADOPTED): `X_i,total` is the user's full stated external position, not the clipped recognized amount; disclose reference value vs. acquisition cost; seed nominal value is not redeemable principal; compounded seed nominal value is not additional cash invested; report gross SOL, actual period return, fees and market P&L separately; external yield and leverage are never added to Cherry APR; with zero participants show N/A and zero accrual.

Static projection: `R_remaining = r_p · remaining_days · w_i/H`, clipped at epoch end; entrant-included vs. new-entrant denominator adjustment per the architecture §1; never project beyond funded commitments.

Funding equation for a target weight-unit APR `A` over `D` days (a funding requirement, not a promise):

```text
E_required = ceil_lamport((A/100) · H_expected · D/365)
```

Inherited continuation budgets (baseline policy constraints; determine these exact lamport amounts):

| Budget | Equation | Lamports |
|---|---|---:|
| Harvest (2-day) | ceil(10 × 12.5 × 2/365 SOL) | 684,931,507 |
| Superfruit (3-day) | ceil(5 × 12.5 × 3/365 SOL) | 513,698,631 |
| Season (5-day) | ceil(2.5 × 12.5 × 5/365 SOL) | 428,082,192 |
| Continuation R (sum) | — | 1,626,712,330 |
| Storm ceiling | ceil(250 × 12.5/365 SOL) | 8,561,643,836 |

These are funding equations. No funding balance (including the architecture's F = 10 SOL worked example) is asserted to exist; F and J are UNRESOLVED (D03/D06).

---

## 4. Launch timeline

**ADOPTED.** All intervals half-open. `T0` = founder activation chain timestamp (first successful purchase by the pinned founder wallet — CURRENT V2 behavior). **`FRUIT_T0 = T0 + 86,400 seconds` is the public opening; there is no intervening `.sns`-only window** (matches verified V2 `access_phase`). No calendar launch date exists.

| Stage | Start (from T0) | End (from T0) | Fruit budget |
|---|---:|---:|---|
| `.skr` early access | 0 | 1 day | **0** — base Tree emissions only |
| Storm | 1 day | 2 days | `E_storm` (below) |
| Harvest | 2 days | 4 days | 684,931,507 lamports |
| Superfruit | 4 days | 7 days | 513,698,631 lamports |
| Season | 7 days | 12 days | 428,082,192 lamports |

The sequence spans 11 days from public `FRUIT_T0` and ends at `T0+12 days` (= `FRUIT_T0 + 950,400 s`; core start offsets from `FRUIT_T0`: 0 / 86,400 / 259,200 / 518,400 s; durations 86,400 / 172,800 / 259,200 / 432,000 s). The Tree's base funding horizon must reach `T0+12 days` at activation (11 days remaining at public opening).

Funding boundary states, for net cleared core Fruit budget `F ≥ R`:

```text
E_storm   = min(F − R, 8.561643836 SOL)
Z_future  = F − R − E_storm
```

- `F = R`: Storm emits zero; continuation keeps published offsets.
- `0 < F < R`: the sole deterministic fallback is one 48-hour Harvest beginning at `FRUIT_T0` funded with `F`; no four-stage sequence, no lock Season.
- `F = 0`: Fruit remains inactive.
These are funding boundary states, not alternative product menus.

Commitment rule: commit every announced core tranche before its schedule begins; public opening is time-based and does not waive funding/verification requirements; operational failures must not spend or retroactively "catch up" unearned capacity; uncertain already-running intervals resolve under §11. After the sequence, new 48-hour Harvest epochs need a separately committed budget and fresh eligibility; no funded renewal means zero new emissions.

Worked example (NOT a funding claim): at F = 10 SOL, `E_storm = 8.373287670 SOL`, `Z_future = 0`; exact Storm numerator at fixed H is `305624.999955`, so `APR = 305624.999955/H` (the usual `305625/H` is rounded); at H = 12.5 this is ≈ 24,450% (≈ 66.98630136% for that one day on matched seed reference capital; ≈ 33.49315068% combined-capital at equal matched external capital, before fees/losses).

---

## 5. Wallet caps and qualification

**ADOPTED.** A hard **permanent** Cherry holding cap of 250 current seeds implements the founder's Cherry max-wallet constraint. Three independent checks:

1. During `[T0, T0+24h)`, cumulative fresh purchases (including the activation itself) cannot exceed `C_day1 = 250` seeds = 2.5 SOL principal; **purchase fees, rent and transaction costs are additional.** (Mechanism CURRENT in V2 as `genesis_cap_check`; production must initialize `genesis_cap_rigs = 250` — the value itself is UNRESOLVED as configuration until production initialize, D04/D10 lane.)
2. At all times, every purchase, compound, mint, migration or supported seed receipt must preserve `S_i ≤ 250`. **NOT IMPLEMENTED — required protocol change.**
3. Fruit recognition uses `P·min(S_i, 250)` defensively, even against an invalid legacy position.

Qualification: 150 current seeds (1.5 SOL nominal), consistent with the cap (`150 ≤ 250`; exactly 60%).

Capped compounding:

```text
n_affordable = whole seeds purchasable from actually available earned SOL,
               using the existing exact price and fee calculation
n_minted     = min(n_affordable, max(0, 250 − S_i))
```

Debit only the exact price and fee for `n_minted`; **remaining earned SOL stays an accrued, claimable liability.** At the cap, compound is a no-op with no fee; it must not discard rewards, reroute them, or create overflow seeds. Fresh purchases exceeding headroom fail atomically. Base reward/penalty rules are otherwise unchanged; Fruit claims do not inherit base claim penalties. (CURRENT V2 already preserves the unspent remainder; the headroom clamp is part of the required change.)

Legacy positions with `S > 250` cannot be clipped or confiscated during migration; resolving them is a deployment-version prerequisite **if any exist**. No production deployment exists (verified: runbook PREPARED/NOT EXECUTED; no mainnet program state); the formal no-over-cap-positions verification is roadmap task T30 / decision D07.

External holdings are **uncapped**: a wallet may hold 100 SOL-equivalent of an admitted Fruit, but at most 2.5 SOL-equivalent affects Cherry weight. External assets remain outside Cherry custody at all times.

---

## 6. Fruit eligibility and external admission

**ADOPTED — BLOCKED ACTIVATION** (admission enforcement and verification incomplete; D01/D02/D08).

One selected asset/adapter per wallet per epoch; a selected asset may have multiple verified accounts, but each underlying claim counts once. Switching is effective next epoch; exit is always permitted and ends eligibility for the exited units; later joins accrue only from finalized registration and verified ownership time.

Per attributed position j of wallet i:

```text
G_ij(t) = attributed gross position value at fixed epoch reference prices
D_ij(t) = verified borrowed principal attributable to that position, same numeraire
A_ij(t) = 1 only if asset, ownership, exit, financing and encumbrance checks all pass
X_i(t)  = Σ_selected A_ij(t) · max(0, G_ij(t) − D_ij(t))
```

- **Borrowed principal and unsupported encumbered capital do not count toward X.** Unknown borrowing/encumbrance evidence makes the affected position ineligible pending resolution. Excluded: unsupported pledged collateral, collateral/debt wrappers, pending unstake balances, vesting-locked units, duplicated underlying receipts. V1 admits no lending/debt wrappers. An owned xSOL token's intrinsic leverage is not wallet borrowed principal (value owned token equity, not underlying notional); borrowing to purchase it is a separate excluded financing obligation.
- **Enforceability is UNRESOLVED (D01).** Transfer attribution does **not** prove absence of loans from another wallet, exchange, or off-chain lender; a strict universal borrowed-funds exclusion cannot currently be certified. This must not be silently replaced by an attestation-only claim.

**Required attribution mechanism (D02 governs the final trust/correction model):** replay complete finalized ownership-changing history — transfers, authority changes, mint/burn, stake/unstake, wrapper conversion, relevant Token-2022 changes — into a global event timeline; integrate ownership over disjoint intervals; one underlying unit supports only its actual owner at a given instant (A→B→A earns A only its actual holding time and B only B's; same-transaction round trips add no positive duration). Seed ownership/eligibility changes settle on the same timeline. **Endpoint-only balance checks, endpoint minima, wallet signatures, and Merkle membership alone are insufficient.** Pool denominators are reconstructed on the same timeline; missing events can change every participant's share. Cumulative roots require unique epoch/version identifiers, replay protection, conserved sums, per-wallet limits, independent replay; indexer truthfulness remains an explicit trust assumption unless independently enforceable proofs exist.

Pricing: fixed timestamped epoch prices and verified share conversion; rebase/share-rate changes follow the adapter's specified accounting; never reprice past liabilities; live market values stay separately visible; CR/NAV failure triggers only the prospective §10 emergency reset.

Asset dispositions (no candidate is live; candidate addresses from the Fable solve remain unverified-chain records, not deployment constants):

| Asset/form | Disposition | Required activation evidence |
|---|---|---|
| Wallet SKR | Candidate | Mint/account identity, unlocked ownership, approved SKR/SOL pricing, normal executable exit, financing checks |
| Staked SKR | Conditional candidate | All preceding plus live IDL, beneficiary ownership, cooldown/epoch rules, withdraw path, pending-unstake exclusion |
| xSOL | Candidate with Safe Mode | Mint/pool version mapping, equity/NAV, CR/feed data, operational routes, financing checks |
| JitoSOL | Candidate | Mint/pool identity, share accounting, documented normal exit ≤ 5 days, financing checks |
| Spot JLP | Candidate | Unencumbered direct token ownership, pool valuation and SOL conversion, normal exit evidence, financing checks |
| Leveraged / Multiply / queued / vault JLP | **Excluded** | No admission in this model |
| xBTC, xHYPE | **Disabled** | Independent official mint/program reconciliation and full adapter validation before any later approval |

External normal-path exit test: documented time from initiating a supported exit through required cooldown/queue/withdraw steps to a normally executable sale/redemption, at the admitted position size, **no more than 432,000 seconds (5 days)**. It is documentation of the normal path — **not a guarantee during total liquidity or market failure.** Cherry's voluntary lock never prevents external exit; leaving early affects only the conditional lock bonus.

---

## 7. Loyalty (boost)

**ADOPTED — BLOCKED ACTIVATION** until separately funded (D06). Base Fruit weight and E are unchanged by loyalty; bonuses come only from a separately committed bonus reserve:

```text
Z_bonus_available = J + explicitly earmarked uncommitted Z_future
Σ_committed B_loy,p + Σ_committed B_lock,s ≤ available bonus funds
```

The same SOL cannot fund renewals, loyalty and locks simultaneously; earmarking decreases the usable future reserve. With F = 10 and J = 0, opening bonus budgets are zero (the architecture's example; Fable's claimed universal > 10.188356 SOL funding threshold is not a rule).

Tenure: `d_i` = consecutive verified qualifying days (Day 1: `S ≥ 150` plus registered, fully verified positive eligible external equity — Fruit emits zero that day; thereafter `w_i > 0`). Voluntary loss of eligibility or a penalized base claim resets the clock; an ordinary unpenalized claim does not; a system-data outage freezes settlement pending replay; an involuntary Safe Mode interruption freezes tenure without erasing recorded history; permanent activity history and leaderboard are never reset.

```text
d_cap = 11 days        [length of the core Fruit sequence]
β_i(t) = min(1, d_i(t)/11)
v_i(t) = w_i(t)·β_i(t)
H_loy(t) = max(Σ_i v_i(t), 12.5)
dQ_i/dt = (B_loy,p/D_p) · v_i(t)/H_loy(t)
Σ Q_i ≤ B_loy,p ;  Q_i ≤ B_loy,p/5
```

Instantaneous bonus-to-core ratio where the base rate is positive: `b_i(t) = (B_loy,p/E_p)·β_i(t)·H(t)/H_loy(t)` — so `B_loy/E` is **not** a universal maximum boost (sparse participation can raise a veteran's bonus/base ratio while the absolute `B_loy/5` limit holds; floors matter). Realized lifetime/epoch boost is `Q_i/R_i` (undefined if `R_i = 0`). GM/GN/chat/referral activity may be recorded with verified events and anti-spam limits but has **no monetary conversion and no weight multiplier** in this model. New users lose no core allocation to this bonus.

---

## 8. Referral

**ADOPTED.** One immutable direct referrer per wallet, bound at its first purchase; no self-referral; no ancestor commissions; referrer must already be a valid planter (all CURRENT V2 behavior). The existing funded base redistribution — weights 100/105 — is preserved and is **not** an extra Fruit multiplier, cash commission, or new emission. Fruit referral multiplier remains 1.

Eligible fee events are **actual finalized fees received in the fee vault** from the referred wallet's fresh purchase or completed compound. Not eligible: base claim penalties, seed principal, promised future fees, reward-vault balances. **NOT IMPLEMENTED — fee-backed commissions are new program/off-chain work; activation UNRESOLVED (D05).**

```text
f_e          = actual received eligible fee for unique finalized event e
commission_e = floor_lamport(c · f_e)
retained_e   = f_e − commission_e
retained_e   ≥ required_margin_e
```

With fee fraction φ on fee-bearing principal volume and required retained margin m: `(1−c)·φ ≥ m`, i.e. `0 ≤ c ≤ 1−m/φ` (φ > 0, 0 ≤ m ≤ φ). **c remains UNRESOLVED until m and the actual production fee configuration are known** (D05); if `m > φ` no referral cut is feasible even at zero; if φ = 0 no commission exists. Integer settlement must preserve the per-event retained margin after fee rounding.

Settlement rules: reserve commissions atomically before any fee withdrawal; fee-recipient withdrawals can use only the remaining unencumbered fee balance; once a compound's fee has actually reached the FEE vault its commission is funded there and is never deducted a second time from accrued rewards; no retroactive commissions on fees already withdrawn; each fee event is consumed once. Linked-wallet fee rebates remain possible — never claim complete person-level abuse prevention. Retained fees fund future rewards only after a separate actual transfer and commitment.

---

## 9. Locks

**ADOPTED — BLOCKED ACTIVATION** until funded (D06). A lock is a voluntary, noncustodial **commitment of eligible Fruit weight for a conditional bonus**. It locks neither external principal nor already-earned core/loyalty rewards.

For the five-day Season beginning at `t_s = FRUIT_T0 + 6 days` (registration window `[FRUIT_T0, t_s)`; the finalized chain ordering determines inclusion; registrations at/after `t_s` are excluded):

- **One immutable lock election per wallet per Season**; cancellation or early exit never reopens registration for that Season; no second election after a safety release.
- Register duration `d_i ∈ {1,2,3,5}` days, selected Season Fruit, and requested eligible weight; at `t_s`, `ℓ_i = min(requested_weight, w_i(t_s))`; zero/invalid registrations rejected; registered positions must satisfy Season price/ownership checks; all locks start at `t_s`, maturing at `t_s + d_i` (a one-day election does not get five consecutive one-day claims).
- Commit `B_lock` before the cutoff becomes effective; never fund awards from future epochs.

```text
u_i     = ℓ_i · d_i
H_lock  = max(Σ_registered u_i, 62.5)          [Hfloor × 5 days; frozen for the Season]
A_i     = B_lock · u_i / H_lock
Σ A_i ≤ B_lock ;  A_i ≤ B_lock/5               [since ℓ_i ≤ 2.5 and d_i ≤ 5]
```

Later exits, one-day maturities, and forfeitures never renormalize awards. Reserve the unallocated balance until registration finalizes, then release to Z.

Surviving weight `ℓ_surv_i(t)` is a nonincreasing low-water mark of verified eligible weight supporting the election, bounded by `ℓ_i`; restored/replaced weight never resurrects forfeited units. Voluntary shortfall cancels the proportional unvested award for the lost units. For surviving units:

```text
conditional_progress_i(t) = A_i · ℓ_surv_i(t)/ℓ_i · min((t−t_s)/d_i, 1)
```

This is conditional vesting progress held in U, **not** claimable accrued L. At ordinary maturity, vest `A_i · ℓ_surv_i/ℓ_i` from U to L. Cancelled unvested awards return to Z only after ownership is resolved; no other wallet receives them retroactively. Core Fruit accrual and vested bonuses are untouched.

On a **verified EXTREME event or actual route/protocol unavailability**: offer immediate safety release without forfeiture of earned-to-date progress — vest that progress into L, cancel only the future unearned part, computed on the last valid surviving weight immediately before the event (never a post-event NAV-zero weight); end the election permanently. HOT alone does not force exit or vesting. Unknown history is held pending, never treated as voluntary exit. Everyone choosing five days reduces the allocation to `B_lock·ℓ_i/max(Σℓ_i, 12.5)` at maturity; nobody locking returns all `B_lock` to Z. Locks cannot overrun the five-day Season.

---

## 10. Safe Mode

**ADOPTED — NOT IMPLEMENTED.** Maps xSOL-family risk to the verified pool collateral state. CR is a ratio; `leverage = CR/(CR−1)` is valid only for CR > 1; CR = 1 is singular — never display a finite leverage there.

| State | CR range | Entry / increase | Reward behavior |
|---|---|---|---|
| NORMAL | CR ≥ 1.35 | Permitted when admission checks pass | Normal prospective accrual |
| WARM | 1.20 ≤ CR < 1.35 | Two-step risk acknowledgement/signature | No automatic haircut |
| HOT | 1.00 ≤ CR < 1.20 | **Permitted after two-step acknowledgement/signature when the required protocol/route is actually available** | No automatic haircut solely for HOT |
| EXTREME | CR < 1.00, or verified severe protocol/route failure | Acknowledge risk only if an operation genuinely remains available; never submit an unavailable operation | Prospective verified NAV/eligibility reset; safety-release lock treatment (§9) |

The two steps are (1) a specific risk screen and (2) an explicit wallet signature binding wallet, asset, action/amount, chain, epoch, risk/config version, nonce and expiry — risk consent, not 2FA, not permission to transfer funds. Reconfirm when action scope or risk version materially changes. Risk severity and technical availability are separate: a label alone never blocks HOT entry, and a signature never bypasses an unavailable operation; a blocked mint route is not proof that swap/exit routes are blocked — check the requested route and the required normal exit path.

Use official zone boundaries only after verifying the deployed version; the Hylo zone fee table is labeled "example" — never hardcode those fees. Volatility/drawdown/slippage overlays remain UNSET until calibrated (D09). Stale/missing data is UNKNOWN, never proof of zero NAV: pause new recognition where data cannot be verified and reserve existing unsettled capacity. Verified NAV collapse: `X_new = eligible_units · verified_emergency_NAV`, prospectively from the authenticated checkpoint with a signed/logged reference reset. Verified loss of a required operational route: stop affected future recognition until recovery (fresh route/data checks plus a published prospective reference segment). Never retroactively change settled R or Q; do not claim next-epoch prices must be lower — markets may recover.

**Safe Mode never erases accrued liabilities (L):** no deletion, no forced liquidation, no leverage increase, no custody. Claims already established remain payable whenever claim infrastructure itself is operational; outages can delay payment but never cancel the debt. The base program's global pause (`set_paused`, CURRENT V2) is not the Fruit Safe Mode controller.

---

## 11. Solvency and accounting

**ADOPTED — Fruit ledger NOT IMPLEMENTED.** For each independently funded reward ledger, and in aggregate:

```text
V ≥ L + U + Z
V = liquid spendable reward-vault SOL (excluding rent and operating balances)
L = established accrued/vested unclaimed liabilities
U = remaining commitments, including conditional locks and reserved unsettled capacity
Z = uncommitted available reserve
```

For uncertain past intervals, U holds settlement reserves even if economic accrual may already have occurred; U is not available for reuse. Base and Fruit balances never back each other twice; fee commissions carry their own funded ledger/reserve constraint. (CURRENT base program tracks `funded − claimed − compounded`, an upper bound; the Fruit layer needs its own counters.)

| Transition | Accounting |
|---|---|
| Actual cleared funding a | `V += a; Z += a` |
| Commit epoch/bonus b | `Z −= b; U += b` |
| Settle verified accrual δ | `U −= δ; L += δ` |
| Vest conditional bonus δ | `U −= δ; L += δ` |
| Pay established claim c | `V −= c; L −= c` atomically |
| Release verified unused capacity z | `U −= z; Z += z` |
| Compound earned SOL c | `V −= c; L −= c` atomically, matching actual capital+fee transfers; retain remainder |
| Authorized reserve withdrawal z | `V −= z; Z −= z`; never reduce L or protected U |

In an ownership/indexer gap affecting a shared denominator: retain the **entire unsettled scheduled capacity for the affected common pool interval**, minus only liabilities already conclusively established — never `r·Δt·w_max/H_observed` with an unverified denominator. A known fixed lock contract reserves its unsettled remaining award. Never release disputed funds merely because the epoch ended; release only after complete reconciliation; no expiry erases valid accrued rewards.

Ceiling proofs: `Σ dR/dt = r·Cw/H ≤ r` and `dR_i/dt ≤ r·2.5/12.5 = r/5`; loyalty inherits this via `0 ≤ β ≤ 1`; locks via the fixed denominator and ≤ 12.5 weight-days per wallet. Every pool has a finite prefunded liability ceiling; the 20% per-wallet bound applies per funded pool, so total receipts with bonuses are not bounded by `E/5` alone.

Precision: rational/fixed-point cumulative entitlement with carried remainders; never round each wallet upward; cumulative scheduled capacity `floor(E_lamports · elapsed_seconds/T)` bounds interval spending; carry wallet fractions so checkpoint frequency does not destroy entitlement; claims enforce cumulative paid amounts and unique commitments atomically; allocation revisions never claw back settled liabilities or double-spend a budget; corrections needing extra liabilities require separate backing first.

External asset value, future fees, seed principal, and unrealized yield never count in V. Solvency in SOL neither insures external principal nor guarantees availability.

---

## 12. Whale and Sybil protections

**ADOPTED.** The hard holding cap, recognition cap, denominator floor, one-asset selection, finalized history, one lock election and one referral level constrain wallet behavior but do **not** establish one wallet per person; a `.skr` entitlement/device is not proof of unique beneficial ownership. Every split wallet needs its own 150+ seeds and simultaneously attributed external equity; sequentially reused positions earn only sequential ownership time. Five fully covered 250-seed wallets controlled by one person can receive the entire core tranche when nobody else qualifies; the per-wallet ceiling still holds. Do not label this model Sybil-proof; person-level protection requires a real enforceable identity/ownership mechanism. Single-wallet 2,500-seed examples from the historical baseline are invalid under the hard cap and survive only as legacy/migration or rejected-transaction tests; wallet splitting remains a valid adversarial case. Scenario tables: architecture §11.

---

## 13. APR display rules

**ADOPTED.** Display the live-denominator rate `36,500·r/H`; never present floor-case APRs (e.g. 24,450%) as live rates. Distinguish weight-unit, Cherry-capital and combined-capital rates. Phase labels (matching the base engine's `YieldPhase`, CURRENT V2): Genesis ≥ 1,200%; Early Grove 600–<1,200%; Growth 250–<600%; Mature >0–<250%; Dormant when no funded active rate. APR crossing points and dilution arithmetic: architecture §11. No APR figure is a promise; annualization of a short funded period must be labeled as such.

---

## 14. Canonical parameter table

| Parameter | Final rule |
|---|---|
| Seed price | 10,000,000 lamports reference; production configuration must match before schedule use (D04) |
| Day-1 fresh max | 250 seeds / 2.5 SOL principal, fees additional; `[T0, T0+24h)` cumulative fresh purchases |
| Permanent Cherry current holding max | 250 seeds; all seed creation/receipt paths enforce `S ≤ 250` (required protocol change) |
| Permanent Fruit benchmark | 250 seeds |
| Qualification | 150 current seeds; positive verified X required for rewards |
| External holdings cap | None |
| Recognized external contribution | `min(K, X)`, maximum 2.5 SOL-equivalent per wallet |
| Asset/referral Fruit multipliers | 1.00 |
| Active Fruit | One per wallet per epoch; next-epoch switching |
| Denominator | `max(Σw, 12.5)` |
| Per-wallet core ceiling | `E/5` per epoch |
| Day-1 Fruit reserve spend | Zero |
| Public opening | `FRUIT_T0 = T0 + 86,400 s`; no `.sns` window |
| Core durations | 86,400 / 172,800 / 259,200 / 432,000 s |
| Core start offsets from `FRUIT_T0` | 0 / 86,400 / 259,200 / 518,400 s |
| Core end | `FRUIT_T0 + 950,400 s = T0 + 1,036,800 s` |
| Harvest / Superfruit / Season | 684,931,507 / 513,698,631 / 428,082,192 lamports |
| Continuation R | 1,626,712,330 lamports |
| Storm, when `F ≥ R` | `min(F−R, 8,561,643,836 lamports)` |
| Underfunded sequence | If `0 < F < R`: one 172,800-second Harvest funded with F |
| Loyalty tenure saturation | 11 verified qualifying days |
| Loyalty payout | Separate `B_loy`; `∫ (B_loy/D)·w·β/max(Σw·β, 12.5) dt` |
| Lock durations | 1 / 2 / 3 / 5 days |
| Lock election/cutoff | One immutable election per wallet/Season; finalize strictly before Season start |
| Lock award | `B_lock·ℓ·d/max(Σℓ·d, 62.5)`; frozen denominator |
| Lock vesting | Cliff; conditional progress reserved in U; safety release vests earned-to-date progress |
| Bonus per-wallet ceilings | `B_loy/5` and `B_lock/5` for their funded pools |
| Referral level | One direct level |
| Referral payment | `floor(c × actually received eligible fee)`; c UNRESOLVED (D05) |
| Referral margin bound | `(1−c)·φ ≥ m` |
| Existing base referral weight | Preserve 105/100 funded base redistribution; no added Fruit weight |
| External normal exit | Documented normal path ≤ 432,000 s; no catastrophe guarantee |
| Borrowed/unsupported encumbered equity | Excluded; unknown provenance unresolved/ineligible (D01) |
| Ownership evidence | Complete finalized event attribution; endpoint-only checks prohibited (D02) |
| xBTC / xHYPE | Disabled |
| JLP | Direct spot only; no leveraged/queued/vault exposure |
| HOT | Entry allowed after required two-step consent when protocol/route is available |
| Safe Mode liabilities | Accrued L never erased |
| Solvency | `V ≥ L + U + Z`, including reserved unsettled obligations |
| No participants | No accrual; no infinite APR; unused capacity remains reserve |
| Claim expiry | None for valid accrued liabilities |
| Compound at cap | No new seed; earned SOL remains claimable |
| Funding ends | Finish valid commitments, then zero new emission |

---

## 15. Implementation status map

| # | Feature | Status | Where |
|---|---|---|---|
| 1 | Base Tree: purchases, vault separation, funded schedule, compounding with claimable remainder | CURRENT (V1 `main`, V2 `mainnet/rc-hardening`) | `programs/sol-rigs`, `crates/rigs-engine` |
| 2 | Launch gate: founder activation → T0; 24 h `.skr` window; Public at T0+24 h; no `.sns` phase | CURRENT (V2) | `crates/rigs-engine/src/launch.rs`, `programs/sol-rigs/src/instructions/buy_rigs.rs` |
| 3 | Day-1 genesis cap mechanism (window-limited, compounds excluded, no setter) | CURRENT (V2); production value 250 to be set at initialize | `state.rs`, `buy_rigs.rs`, `launch.rs` |
| 4 | Claim penalty 25/15/5% over 72 h | CURRENT (V2) | `crates/rigs-engine/src/penalty.rs` |
| 5 | Base referral binding + 100/105 weights | CURRENT (V2) | `crates/rigs-engine/src/referral.rs` |
| 6 | Permanent holding cap `S ≤ 250` on all seed paths | **ADOPTED — NOT IMPLEMENTED** (required program change) | roadmap T30–T33 |
| 7 | Capped compounding (`n_minted = min(n_affordable, headroom)`) | ADOPTED — NOT IMPLEMENTED (remainder preservation alone is CURRENT) | roadmap T30–T33 |
| 8 | Fruit vaults, epochs, weights, settlement | ADOPTED — NOT IMPLEMENTED (fixture-based engine work permitted) | roadmap T40–T42 |
| 9 | Ownership attribution / indexer settlement | ADOPTED — design open (D02) | roadmap T11, T50 |
| 10 | External adapters (SKR/xSOL/JitoSOL/spot JLP; staked SKR conditional) | ADOPTED — verification incomplete (D08); none live | roadmap T12, T51 |
| 11 | Borrowed/encumbered capital exclusion enforcement | ADOPTED — enforceability UNRESOLVED (D01) | roadmap T10 |
| 12 | Loyalty bonus pool | ADOPTED — BLOCKED on funding (D06) | roadmap T60 |
| 13 | Lock registry / Season cohort / safety release | ADOPTED — BLOCKED on funding (D06) | roadmap T61 |
| 14 | Fee-backed referral commissions | ADOPTED — BLOCKED on D05 (m, φ) | roadmap T62 |
| 15 | Safe Mode states + two-step consent | ADOPTED — NOT IMPLEMENTED; overlays UNRESOLVED (D09) | roadmap T52 |
| 16 | Fruit solvency ledger `V ≥ L+U+Z` | ADOPTED — NOT IMPLEMENTED | roadmap T40–T42 |
| 17 | Estimator/frontend Fruit surfaces | ADOPTED — NOT IMPLEMENTED | roadmap T70 |
| 18 | User-facing Fruit documentation/disclosures | ADOPTED — this document is the source; user copy pending | roadmap T71 |
| 19 | xBTC / xHYPE | Disabled by design | — |

---

## 16. Unresolved configuration summary

All unresolved items live in the [decision register](CHERRY_TREE_DECISIONS.md) with governing equations and blocked tasks: D01 borrowed/encumbered-capital enforcement; D02 ownership/indexer trust and correction; D03 base schedule, balances and Fruit budget F; D04 production price P and fee φ; D05 referral margin m and cut c; D06 bonus funding J/B_loy/B_lock; D07 legacy over-cap positions (conditional; expected none); D08 asset verification and price sources; D09 risk overlay calibration; D10 existing release/security gates. Demand and eligible-wallet counts are scenario inputs, never constants guaranteeing APR; no LP-depth threshold is derivable from seed count (capital-vault SOL is not an AMM LP position).

## 17. Document map

- Normative (this set): [Fruit tokenomics (this file)](CHERRY_TREE_FRUIT_TOKENOMICS.md) · [Implementation roadmap](CHERRY_TREE_IMPLEMENTATION_ROADMAP.md) · [Decision register](CHERRY_TREE_DECISIONS.md) · [Status](CHERRY_TREE_STATUS.md)
- Traceable references (unchanged): [Final tokenomics architecture](CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md) (`e807b55`) — full derivation and scenario tables · [Fable solve](FABLE_CHERRY_TREE_TOKENOMICS_SOLVE.md) — quantitative support, partially superseded per §0.2 · [Fable checklist](FABLE_CHERRY_TREE_TOKENOMICS_CHECKLIST.md) — review brief
- Release lane (`mainnet/rc-hardening`): [mainnet runbook](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/mainnet/rc-hardening/docs/backlog/MAINNET_RUNBOOK.md) and the `docs/backlog/` decision artifacts (custody, genesis cap rubric, runway ops, capital policy, Cherry integration readiness)
- Published user-facing docs (base product only, maintained in GitBook): <https://the-cherry-tree.gitbook.io/the-cherry-tree-docs> — verified 2026-09-26 to contain no Fruit-layer content; this canonical set is not yet mirrored there.
- Private (not published): qualification evidence archive, local path `/home/bronzesneaks/cherry-qualification-evidence/` — reference by manifest only.
