# THE CHERRY TREE — Final Tokenomics Architecture

Date: 2026-09-26. Status: reconciled economic specification, **closeout revision** (founder final decisions and adversarial-review corrections applied; see `docs/OPUS_CHERRY_TREE_TOKENOMICS_CLOSEOUT.md`). **Architecture FROZEN for implementation** (gate 14.2-A closed by founder-supplied mainnet evidence: initial-deployment case). Implementation is gated by section 14. No production changes, deployment, funding or launch authorization.

Authority: founder final decisions of the closeout (external debt netting, on-chain Fruit ledger, permanent 250 max wallet with legacy grandfathering, no Fruit/bonus compounding, one .skr domain = one Day-1 wallet), then founder overrides in the original request, then Fable's quantitative solve, then the existing Fruit baseline. This document defines one model. Unresolved inputs remain configuration variables; no illustrative budget is asserted to exist.

Evidence reviewed:

- `docs/FABLE_CHERRY_TREE_TOKENOMICS_SOLVE.md` on `claude/cherry-tree-tokenomics-cfoilj`, Git blob `fe1be454609c52943112f92eee33e8dc1f302e6a`, and its review checklist.
- Both supplied baseline copies, `CHERRY_TREE_FRUIT_TOKENOMICS.md` and `CHERRY_TREE_FRUIT_TOKENOMICS(1).md`: 562 lines each, identical content.
- V2 `mainnet/rc-hardening`: `state.rs` blob `f60b1f2a174ab4187f6237e28af6c0d3acdb45a2`, `buy_rigs.rs` blob `76c22acbd63674cee581ae935492f82d9a22f9b2`, `compound.rs` blob `91ade0dd81c0771d3f25a8ac2c144dba38a69849`.
- Current official Hylo address and equations pages, plus Jito and Solana Mobile documentation. Asset admission still requires chain/configuration and adapter verification; reading documentation is not production certification.

Pre-ops result: source confirms that V2's current cap applies only to cumulative fresh purchases during the first 24 hours; compounds are excluded, and the cap expires at public opening. Source also confirms separate capital, fee and reward vault flows, immutable referral binding, and base referral weights of 100/105. A permanent hard holding cap is therefore a required protocol change, not a claim about current behavior. Live deployed configuration, balances and account migration state were not audited here. Closeout re-check: the `mainnet/rc-hardening` mainnet runbook is marked "PREPARED, NOT EXECUTED" and `Anchor.toml` has only localnet/devnet program entries, but repository documents are not chain evidence; mainnet and devnet RPC were unreachable from the review environment (proxy 403), so deployed state was not verifiable from the review environment. Founder-supplied read-only evidence (`solana account C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt --url mainnet-beta` → `AccountNotFound`) establishes that the declared production program does not exist on mainnet: initial-deployment case, gate 14.2-A closed.

## 1. Final equations

Use SOL-equivalent for weights, SOL for explanatory budgets, and integer lamports/seconds for settlement. One day is 86,400 seconds. Subscripts `i`, `p`, `s` denote wallet, epoch and Season.

```text
P = 0.01 SOL/seed = 10,000,000 lamports/seed
C_day1 = 250 fresh-purchased seeds
M_hold = 250 current seeds, permanent holding ceiling
M = 250 seeds, permanent Fruit benchmark
T = ceil(3M/5) = 150 current seeds
Kmax = P M = 2.5 SOL-equivalent
qmax = 1/5
Hfloor = Kmax/qmax = 12.5 SOL-equivalent

K_i(t) = P min(S_i(t), M)
X_i(t) = verified eligible external equity at epoch reference prices
w_i(t) = 1[S_i(t) >= T and eligibility verified] min(K_i(t), X_i(t))
Cw(t) = sum_i w_i(t)
H(t) = max(Cw(t), Hfloor)
r_p = E_p/D_p                         [SOL/day]
dR_i/dt = r_p w_i(t)/H(t)             [inside funded epoch only]
R_i,p = integral_epoch r_p w_i(t)/H(t) dt
```

`P` in every Fruit, loyalty and lock formula is the frozen constant `P_ref = 10,000,000` lamports. It is never read from `Config.rig_price_lamports`, which V2 `set_pricing` can change; a production price change therefore cannot raise `Kmax` above 2.5 SOL-equivalent or break the `E/5` per-wallet bound. `S_i` is the wallet's `Miner.rigs`; `K_i` uses `min(S_i,250)` including for grandfathered legacy wallets (section 4).

`X` is defined by attributed position ownership and admissibility, not by a user-entered balance. With `K>0`, coverage is `g=min(1,X/K)`. Seeds qualify at 150; positive Fruit accrual additionally requires positive eligible external equity. A dust position receives only dust-proportional weight.

```text
sum_i R_i,p <= E_p
R_i,p <= E_p/5
APR_weight(t) = 36,500 r_p/H(t)                   [percent/year]
Cn(t) = P sum_active S_i(t)
APR_pool_Cherry(t) = 36,500 r_p Cw(t)/(H(t) Cn(t))
APR_i_Cherry(t) = 36,500 r_p w_i(t)/(H(t) P S_i(t))
APR_i_combined(t) = 36,500 r_p w_i(t)/(H(t)(P S_i(t)+X_i,total(t)))
```

`Cn` sums over wallets with `S_i>=T` and a registered Fruit selection for that epoch; APR_pool is N/A when `Cn=0`. `X_i,total` is the full external position included in that user's stated capital denominator, not merely the clipped recognized amount. Disclose reference value versus acquisition cost. Seed nominal value is not redeemable principal; compounded seed nominal value is not additional cash invested. Report gross SOL, actual period return, fees and market P&L separately. External yield and leverage are never added to Cherry APR. With zero participants, show N/A and zero accrual.

Static projection: `R_remaining = r_p * remaining_days * w_i/H`, clipped at epoch end. For an entrant already included in the denominator, `Cw_new=Cw-w_old+w_new+delta_other`; otherwise `Cw_new=Cw+w_new+delta_other`. Never project beyond funded commitments.

Funding a target weight-unit APR `A` for `D` days requires:

```text
E_required = ceil_lamport((A/100) H_expected D/365)
```

This is a funding equation, not a promise about actual participation.

## 2. Final constants versus unresolved configuration

Fixed policy constants are the seed price reference, three 250-seed caps/benchmarks above, threshold 150, 1:1 coverage, 20% per-wallet budget ceiling, 12.5 denominator floor, one Fruit per wallet per epoch, 24-hour early access, epoch durations 1/2/3/5 days, and lock choices 1/2/3/5 days.

The inherited continuation targets of 1,000%/500%/250% at the denominator floor and the approximately 25,000% Storm ceiling are retained baseline policy constraints. They are not mathematically optimal rates or newly invented percentages. They determine these exact budgets:

| Budget | Equation | Lamports |
|---|---|---:|
| Harvest | ceil(10 × 12.5 × 2/365 SOL) | 684,931,507 |
| Superfruit | ceil(5 × 12.5 × 3/365 SOL) | 513,698,631 |
| Season | ceil(2.5 × 12.5 × 5/365 SOL) | 428,082,192 |
| Continuation R | Sum of preceding three | 1,626,712,330 |
| Storm ceiling | ceil(250 × 12.5/365 SOL) | 8,561,643,836 |

Unresolved configuration:

| Variable | Constraint / governing equation |
|---|---|
| Actual T0 | Successful founder activation chain timestamp; no calendar launch date set |
| Production price | Must equal adopted P before this schedule is used; a mismatch requires reconciliation, not silent substitution. Fruit math always uses frozen `P_ref` (section 1) |
| Base schedule and B_base | Additional funding needed after dedicated base balances, liabilities and remaining commitments are reconciled |
| Net core Fruit budget F | Actual cleared funds assigned exclusively to core Fruit; after base and operational obligations are protected |
| Separately funded bonus capital J | Actual cleared bonus donation/allocation; not counted in F or other commitments |
| B_loy,p and B_lock,s | Nonnegative explicit committed budgets; aggregate commitments cannot exceed available bonus reserve |
| Fee fraction φ | Actual configured purchase_fee_bps/10,000; rehearsal 3% is not adopted as production fact |
| Required retained margin m | Fraction of fee-bearing principal volume, incorporating operations and any approved replenishment obligations |
| Referral cut c | Unset until m and φ are known; 0 <= c <= 1-m/φ, if φ>0 and 0<=m<=φ |
| Asset adapters / prices | Verified identifiers, owner/position decoding, pinned program versions, manipulation-resistant reference feed, finality and staleness rules (section 5) |
| Debt-protocol registry | Fixed whitelist of supported on-chain lending/perp/margin programs and their debt-account decoders used for debt netting (section 5). Each entry pinned to a verified program version. No adapter goes live until the registry exists; its contents are not selected here |
| Risk overlays | Calibrated feed-confidence, quote/slippage and freshness bounds, and the EXTREME minimum-dwell/hysteresis period; no guessed thresholds |
| Loyalty tenure threshold `w_tenure_min` | Founder-set non-dust minimum recognized weight for a tenure day (section 6). Unset; `B_loy=0` until set |
| Fruit ledger governance | Challenge-window length, veto signer set and threshold, root-poster key, program upgrade authority (section 10a). Required values, not selected here |
| Base hard-cap migration | Rule fixed in section 4 (grandfather-freeze). Remaining input is deployed-state evidence only |
| Allocation authority | On-chain Fruit ledger of section 10a; off-chain replay computes roots, the program enforces budgets |

Demand and eligible-wallet counts are scenario inputs, never constants used to guarantee APR. No LP depth threshold can be derived from seed count: capital-vault SOL is not an AMM LP position. Any future market-liquidity gate needs a specified market, executable-size requirement and measured depth.

## 3. Launch timeline

All intervals are half-open. `FRUIT_T0 = T0 + 86,400 seconds`. Public access opens then; no intervening .sns-only window.

Time and ordering convention (normative for on-chain and off-chain code alike): event order is `(slot, transaction index, instruction index, inner-instruction index)`; event time is that slot's `Clock.unix_timestamp`, the same clock the base program uses for T0; only `finalized` commitment data is used; intervals are built from ordered events, never per-second samples; a boundary equality `t = boundary` belongs to the later interval. Several slots can share one timestamp; ownership within a shared timestamp is resolved by event order, so two owners never overlap.

| Stage | Start from T0 | End from T0 | Fruit budget |
|---|---:|---:|---|
| .skr early access | 0 | 1 day | 0; base Tree emissions only |
| Storm | 1 day | 2 days | E_storm |
| Harvest | 2 days | 4 days | 0.684931507 SOL |
| Superfruit | 4 days | 7 days | 0.513698631 SOL |
| Season | 7 days | 12 days | 0.428082192 SOL |

The sequence lasts 11 days from public FRUIT_T0. The Tree's base funding horizon must reach `T0+12 days`: 12 days at activation, 11 days remaining at public opening. Fable's requirement for 12 *remaining* days at public opening is an unnecessary extra day and is corrected.

For `F >= R`:

```text
E_storm = min(F-R, 8.561643836)
Z_future = F-R-E_storm
```

Commit all announced core tranches before their schedule begins. Bonus allocations do not enlarge or consume these commitments. If `F=R`, Storm emits zero and the continuation retains its published offsets. If `0<F<R`, the sole deterministic fallback is one 48-hour Harvest beginning at FRUIT_T0 with budget F; there is no full four-stage sequence or lock Season. If `F=0`, Fruit remains inactive. These are funding boundary states, not alternative product menus.

Before activation, verify funding, cap enforcement and required admission infrastructure. Public opening is time-based; it does not waive funding/verification requirements. If an operational failure prevents accrual, do not spend or retroactively catch up unearned capacity. Resolve uncertain already-running intervals under section 10. The split is: if the on-chain registration/selection program or the asset's committed reference was unavailable, the interval is **not earned** and its capacity is released to Z; if on-chain state existed but only indexer/data observation failed, the interval **is earned** and settles on replay, with the common-pool reserve held meanwhile.

At `F=10 SOL`, `E_storm=8.373287670 SOL`, `Z_future=0`. At fixed H, the exact Storm APR numerator is `305624.999955`, so `APR=305624.999955/H`. The usual `305625/H` is rounded. At H=12.5 this is approximately 24,450%, corresponding to 66.98630136% for that one day on matched seed reference capital. At equal matched external capital, the combined-capital period return is 33.49315068%, before fees/losses. No 10 SOL funding is asserted to exist.

After the sequence, new 48-hour Harvest epochs require a separately committed budget and fresh eligibility checks. No funded renewal means zero new emissions. Rate and phase follow actual funding and denominator, not the name of the epoch.

## 4. Max-wallet and qualification model

Adopt a **hard current Cherry holding cap of 250 seeds** to implement the founder's Cherry max-wallet constraint. This supersedes Fable's recommendation to use only a reward-power cap.

Three independent checks apply:

1. During `[T0,T0+24h)`, cumulative fresh purchases, including activation, cannot exceed `C_day1=250`.
2. At all times, every purchase, compound, mint, migration or supported seed receipt must preserve `S_i<=250`.
3. Fruit recognition uses `P min(S_i,250)` defensively, even if an invalid legacy position is encountered.

Check 2 is enforced inside the base engine's pure `buy_rigs` and `compound` functions (not only in instruction handlers or the frontend), so every seed-creation path, and the existing engine fuzz/acceptance suites, inherit it. `M_hold=250` is a program constant or a no-setter Config field. Because seeds are non-transferable and non-redeemable, check 2 dominates check 1 while both equal 250; check 1 is kept as a misconfiguration guard, and pre-ops must confirm `genesis_cap_rigs == 250`. The activation purchase counts against both checks.

**Day-1 .skr binding.** Day-1 access is one qualifying .skr domain = one wallet. On the first successful Day-1 purchase that relies on a domain, the base program creates a binding PDA keyed by the domain's name account (seeds `["skr_bind", name_account]`) recording the Miner/wallet. Any later Day-1 purchase proving the same domain from a different wallet is rejected, including after the domain or its wrapper is transferred. The binding cannot be closed or reassigned during `[T0,T0+24h)` and has no effect after public opening. A wallet proving a domain it already bound is unaffected. This is a base-program change delivered in the same upgrade as check 2; V2 `identity::prove` checks current ownership only and records no binding.

The Day-1 250-seed allowance costs 2.5 SOL of seed principal; purchase fees, rent and transaction costs are additional. Qualification remains 150 current seeds, or 1.5 SOL nominal value. It is consistent with the cap: `150<=250`, and the qualifying ratio is exactly 60%, not strictly greater than 60%.

For compounding:

```text
n_affordable = whole seeds purchasable from actually available earned SOL,
               using the existing exact price and fee calculation
n_minted = min(n_affordable, max(0, 250-S_i))
```

Debit only the exact price and fee for `n_minted`. Remaining earned SOL stays an accrued claimable liability. At the cap, compound creates no seed and charges no fee; it must not discard rewards, reroute them automatically or create overflow seeds. The V2 engine returns `NothingToCompound` when the count is zero, reverting the transaction; that revert is an accepted implementation of the at-cap no-op because it leaves every account, fee and penalty anchor unchanged. Fresh purchases exceeding headroom fail atomically. Existing base reward/penalty rules are not changed by this model except for the required seed-cap integration; Fruit claims do not inherit base claim penalties.

**Only base rewards compound.** Fruit core SOL, loyalty SOL, lock SOL and referral commissions are claimable only. No instruction converts them into seeds, and no Fruit-ledger lamport can be transferred into the base capital, fee or reward vaults. The only seed-creating reward path is the existing base-reward compound, subject to check 2.

**Legacy wallets above the cap (final rule).** If any wallet already holds `S_i>250` when the cap ships:

- its seeds are not confiscated, clipped or reduced; no migration writes `Miner.rigs`;
- its balance is frozen: every purchase and compound mints zero (headroom `max(0,250-S_i)=0`) and a purchase reverts;
- base emissions and base claims on its existing seeds continue unchanged, and all already-earned base liabilities are preserved across the migration;
- Fruit, loyalty and lock math recognize at most 250 seeds (`K_i=2.5`).

The Config layout change is a versioned migration that preserves every existing account's lamports and bytes except the added fields, is one-shot, and fails closed. Whether any such wallet exists, and whether T0 has already been set on mainnet, is deployed-state evidence required before implementation (section 14).

External holdings have no wallet cap. A wallet may hold 100 SOL-equivalent of an admitted Fruit, but at most 2.5 SOL-equivalent can affect Cherry weight. External assets remain outside Cherry custody.

## 5. Fruit eligibility model

One selected asset per wallet per epoch, where an asset is defined by its underlying mint (for SKR, one selection covers wallet token accounts and the verified stake beneficiary account; pending-unstake units are excluded). A selected asset can have multiple verified accounts, but each underlying claim can be counted only once. Switching is effective next epoch; exiting is always permitted and ends eligibility for the exited units. A later join accrues only from its finalized registration and verified ownership time.

For position j attributed to wallet i:

```text
G_ij(t) = attributed gross position value of unencumbered units,
          at the unit's valuation price (below)
A_ij(t) = 1 only if asset, ownership, exit and encumbrance
          admission checks all pass
D_i(t)  = wallet i's total outstanding debt across the fixed
          debt-protocol registry, from finalized chain state,
          valued in SOL at the same reference prices
X_i(t)  = max(0, sum_selected_positions A_ij(t) G_ij(t) - D_i(t))
```

**Debt admission rule (final).** Cherry is not a lender and takes on no user debt; debt detection exists only to stop obvious borrowed capital being counted as owned equity. Debt is netted at wallet level against the registry, because lending protocols record debt per obligation, not per token lot. Debt-changing events in registry programs join the same finalized event timeline as ownership; an increase reduces X from its event. If a registry program's state for a registered wallet cannot be read, that wallet's position is pending (section 10 reserve), not zero and not admitted. Off-chain borrowing, exchange credit, loans from other wallets and borrowing in programs outside the registry **cannot be detected**; this limitation is disclosed in the product and in this document. It does not make positions ineligible, and no proof that an asset was never bought with borrowed money is required.

Encumbrance exclusions (unit-level, `A_ij=0` or reduced units): collateral pledged in any protocol, collateral/debt wrappers, pending unstake balances, vesting-locked units, duplicated underlying receipts, the `delegated_amount` of any token account with an active SPL delegate (count `amount - delegated_amount`), frozen token accounts, and Token-2022 mints carrying a permanent delegate, transfer hook or confidential-transfer extension unless the adapter explicitly clears that extension. `Approve`, `Revoke`, `SetAuthority`, `FreezeAccount` and `ThawAccount` are ownership-timeline events. Flash-loan or intra-transaction balances have zero holding duration.

**Protocol-native xAssets.** xSOL is not rejected because it provides leveraged exposure. The holder's owned xSOL is valued as owned token equity at verified NAV/price, not at the underlying leveraged notional, and it remains a candidate subject to identity, pricing/NAV, ownership attribution, risk-state data, exit route and adapter validation. Borrowing to buy it is wallet debt and is netted if it sits in a registry program. Still excluded: borrowed capital counted as owned equity, leveraged/queued/Multiply/vault JLP, and recursive lending structures. xBTC and xHYPE stay disabled until independently verified and approved.

**Valuation price.** Each admitted asset's epoch reference price is committed on-chain before the epoch starts, from a manipulation-resistant source (oracle with confidence bound, or time-weighted average over a window; a single spot sample is prohibited; confidence/window thresholds calibrated per adapter, not set here). An asset with no committed reference at epoch start is **inactive** for that epoch: weight 0, known before the epoch begins, and not treated as UNKNOWN, so it cannot freeze other assets' settlement. Units present at the reference timestamp use the reference price. Units added or registered after it are valued at `min(reference price, verified price at the addition event)`, so buying after a mid-epoch drop cannot purchase weight at the pre-drop price.

**Adapter version pinning.** Each adapter pins the deployed version (programdata hash or last-deploy slot) of every external program it decodes, including registry lending programs. An upgrade of any pinned program sets that asset or registry entry to UNKNOWN from the upgrade slot (pause new recognition; reserve under section 10) until the adapter is re-verified.

Use verified share conversion. Rebase/share-rate changes follow the adapter's specified accounting. CR/NAV failure, depeg, exploit, pause or pinned-program upgrade of any admitted asset can trigger the prospective emergency reset in section 9. Never reprice past liabilities. Live market values remain separately visible.

**Required attribution mechanism:** replay complete finalized ownership-changing history, including transfers, authority changes, mint/burn, stake/unstake, wrapper conversion and relevant Token-2022 changes. Construct a global event timeline and integrate ownership over disjoint intervals. One underlying unit/claim may support only its actual owner at a given instant. A→B→A earns A only its actual ownership time and B only B's; a same-transaction round trip has no additional positive holding duration. Seed ownership/eligibility changes are included in the same settlement timeline.

Endpoint balances, endpoint minima, wallet signatures and Merkle membership alone do not prove this. Pool denominators must be reconstructed on the same timeline. Missing events can change every participant's share, not merely the wallet with missing data. Cumulative roots are posted to and bounded by the on-chain Fruit ledger (section 10a). The indexer's truthfulness is an explicit trust assumption, mitigated by independent replay and the section 10a challenge window, not eliminated.

Transfer attribution does **not** prove absence of loans from another wallet, exchange or off-chain lender. The admission rule is the registry debt netting above plus disclosure; it must not be described as a universal borrowed-funds exclusion.

| Asset/form | Design disposition | Required activation evidence |
|---|---|---|
| Wallet SKR | Candidate | Mint/account identity, unlocked ownership, manipulation-resistant SKR/SOL reference, normal executable exit, encumbrance checks and registry debt netting |
| Staked SKR | Conditional candidate (same asset as wallet SKR) | All preceding checks plus live IDL, beneficiary ownership, cooldown/epoch rules, withdraw path and pending-unstake exclusion |
| xSOL | Candidate with Safe Mode | Mint/pool version mapping, owned-token equity/NAV, CR/feed data, operational routes, encumbrance checks and registry debt netting |
| JitoSOL | Candidate | Mint/pool identity, share accounting, documented normal exit within five days, encumbrance checks and registry debt netting |
| Spot JLP | Candidate | Unencumbered direct token ownership, pool valuation and manipulation-resistant SOL conversion, normal exit evidence, encumbrance checks and registry debt netting |
| Leveraged/Multiply/queued/vault JLP | Excluded | No admission in this model |
| xBTC, xHYPE | Disabled | Independent official mint/program reconciliation and complete adapter validation before a later approval |

The normal-path exit test is documented time from initiating a supported exit through required cooldown/queue/withdraw steps to a normally executable sale/redemption route, at the admitted position size, no more than 432,000 seconds. It is not a guarantee during total liquidity or market failure. Cherry's voluntary bonus lock never prevents that external exit; leaving early affects only the conditional lock bonus.

No candidate is automatically live. Current production adapter approval remains gated, now because verification is incomplete rather than because an impossible absolute exit guarantee is required. Use baseline/Fable address records as candidates, not unchecked deployment constants. The independently reread Hylo address page confirms xSOL and still lists xBTC as TBD.

## 6. Boost model

Base Fruit weight and E are unchanged by loyalty. Allocate separate funded SOL from bonus reserve:

```text
Z_bonus_available = J + explicitly earmarked uncommitted Z_future
sum_committed B_loy,p + sum_committed B_lock,s <= available bonus funds
```

Earmarking decreases the otherwise usable future reserve. The same SOL cannot fund renewals, loyalty and locks simultaneously. At F=10 and J=0, opening bonus budgets are zero; F=10 with separately supplied J can fund bonuses. Fable's claimed universal funding threshold above 10.188356 SOL is therefore not a rule.

Let d_i be consecutive verified qualifying days. A tenure day qualifies only while the wallet's recognized weight is at least `w_tenure_min` throughout (during Day 1: S>=150 and registered, fully verified eligible external equity at or above that threshold, although Fruit emits zero; thereafter `w_i >= w_tenure_min`). `w_tenure_min` is a founder-set non-dust threshold, currently unset; until it is set, `B_loy=0` and loyalty does not activate. Without it, a dust position keeps `beta=1` and loyalty rewards seed tenure rather than sustained capital. A voluntary loss of eligibility or a penalized base claim resets the tenure clock; an ordinary unpenalized claim does not. Penalized-claim detection reads base instruction/account data, not logs alone. A system-data outage freezes its settlement pending replay; an involuntary Safe Mode interruption freezes tenure without erasing its recorded past. Permanent activity history and leaderboard are not reset.

| Cause of eligibility loss | Classification |
|---|---|
| User-signed exit, transfer, selection switch or deregistration (in any Safe Mode state) | Voluntary: reset |
| Penalized base claim | Voluntary: reset |
| Movement by an SPL delegate or authority the user granted | Voluntary: reset |
| Registry debt increase that drops recognized weight below threshold | Voluntary: reset |
| Asset inactive for missing committed reference, adapter UNKNOWN, pinned-program upgrade, EXTREME reset, route halt | Involuntary: freeze |
| Indexer/data observation gap | Settlement pending replay; no reset |

```text
d_cap = 11 days                     [length of the core Fruit sequence]
beta_i(t) = min(1, d_i(t)/11)
v_i(t) = w_i(t) beta_i(t)
H_loy(t) = max(sum_i v_i(t), 12.5)
dQ_i/dt = (B_loy,p/D_p) v_i(t)/H_loy(t)
```

`sum Q_i<=B_loy,p` and `Q_i<=B_loy,p/5`. New users lose no core Fruit allocation to this bonus. GM/GN/chat/referral activity can be recorded with verified events and anti-spam limits; it has no invented monetary conversion or weight multiplier in this final model.

Correct instantaneous bonus-to-core ratio where base rate is positive:

```text
b_i(t) = (B_loy,p/E_p) beta_i(t) H(t)/H_loy(t)
```

Thus `B_loy/E` is not a universal maximum boost. Sparse bonus participation can raise a veteran's bonus relative to its base while preserving the absolute `B_loy/5` limit. Lifetime/epoch realized boost is `Q_i/R_i`, not a promised multiplier. If R_i=0, the ratio is undefined.

For equal-weight static participants where both denominators are above their floors and a fraction f has beta=1 while the rest have beta=0, a target bonus/base ratio b requires `B_loy=b f E`. When a floor binds, use the exact ratio above. This correction applies to Fable's 10%/50%/80% tables; wallet count can matter through the floors.

## 7. Referral model

One immutable direct referrer per wallet, bound at its first purchase; no self-referral, no ancestor commissions. Referrer must already be a valid planter, matching the checked V2 model. Source confirms base weights 100 or 105: preserve that existing funded redistribution; it is not an extra Fruit multiplier or cash commission. Fruit referral multiplier remains 1.

Eligible fee events are actual finalized fees received in the fee vault from the referred wallet's fresh purchase or completed compound. Base claim penalties, seed principal, promised future fees and reward-vault balances are not eligible fees.

```text
f_e = actual received eligible fee for unique finalized event e
v_e = fee-bearing principal volume of event e
required_margin_e = ceil_lamport(m v_e)
commission_e = min(floor_lamport(c f_e), max(0, f_e - required_margin_e))
retained_e = f_e - commission_e >= min(f_e, required_margin_e)
```

The clamp is computed in the same instruction that receives the fee. It keeps the per-event margin even if `purchase_fee_bps` is later lowered by `set_pricing` below the value used to choose c; in that case commissions shrink or reach zero rather than cutting into margin.

For constant fee fraction φ and required retained margin m on fee-bearing principal volume v:

```text
f = φ v
(1-c) φ >= m
0 <= c <= 1-m/φ                   [φ>0 and 0<=m<=φ]
```

The bound does not uniquely determine c. Leave c unset until m and the actual production fee are known and a value within the bound is selected. If m>φ, no referral cut is feasible, even at zero. If φ=0, no commission exists. Exact integer settlement must also preserve the per-event retained margin after fee rounding.

Reserve commissions atomically before any fee withdrawal; fee recipient withdrawals can use only the remaining unencumbered fee balance. V2 cannot do this: `withdraw_fees` checks only the distributable fee-vault balance. Required base-program changes: in `buy_rigs` and `compound`, in the instruction that moves the fee, increment a Config `reserved_commissions` counter and a per-referrer ledger PDA (separate from `Miner`, so no Miner realloc); `withdraw_fees` requires `distributable - reserved_commissions >= amount`; a referrer `claim_commission` instruction pays from the fee vault and decrements both. `c=0` until these changes ship and m and φ are set. Once a compound's fee has actually reached FEE, its eligible commission is funded there; do not deduct a second commission from the user's accrued rewards. No retroactive promise of commissions on fees already withdrawn.

Each fee event is consumed once. Immutable first-purchase binding to a pre-existing planter prevents ordinary referral cycles, but linked-wallet fee rebates remain possible. Never claim complete person-level abuse prevention. Referral revenues can be concentrated without violating reward-vault solvency. Retained fees fund future rewards only after a separate actual transfer and commitment.

## 8. Lock model

A lock is a voluntary, noncustodial **commitment of eligible Fruit weight for a conditional bonus**. It does not lock external principal or already-earned core/loyalty rewards.

For the five-day Season beginning at `t_s=FRUIT_T0+6 days`:

- Registration window is `[FRUIT_T0,t_s)`. The finalized chain ordering determines inclusion; registrations at or after t_s are excluded.
- One immutable lock election per wallet per Season; cancellation or early exit cannot reopen registration for that Season.
- Register duration `d_i in {1,2,3,5}` days, selected Season Fruit and requested eligible weight. Registration stores intent only; no price/ownership validation happens before t_s because the Season reference does not exist yet. At t_s, set `ell_i=min(requested_weight,w_i(t_s))` using the Season reference and ownership at t_s; reject zero/invalid registrations.
- All locks start at t_s and mature at `t_s+d_i`. A one-day election does not get five consecutive one-day claims.
- Commit B_lock before the cutoff becomes effective. Do not fund awards from future epochs.

```text
u_i = ell_i d_i
H_lock = max(sum_registered u_i, Hfloor * 5 days) = max(sum u_i, 62.5)
A_i = B_lock u_i/H_lock
```

Freeze H_lock for the entire Season. Later exits, one-day maturities or forfeitures never renormalize awards. `sum A_i<=B_lock`, and since `ell_i<=2.5` and `d_i<=5`, `A_i<=B_lock/5`. Reserve the unallocated balance until registration is finalized, then release it to Z.

Maintain surviving committed weight `ell_surv_i(t)` as a nonincreasing low-water mark of verified eligible weight supporting the original election, bounded by ell_i. Restored/replaced weight does not resurrect forfeited units. A voluntary shortfall cancels the proportional unvested award for the lost units. For surviving units:

```text
conditional_progress_i(t) = A_i * ell_surv_i(t)/ell_i
                            * min((t-t_s)/d_i, 1)
```

This is explicitly conditional vesting progress in U, not claimable accrued L. At ordinary maturity, vest `A_i * ell_surv_i/ell_i` from U to L. Cancelled unvested awards return to Z only after ownership is resolved; no other wallet receives them retroactively. Core Fruit accrual and vested bonuses remain untouched.

On a verified EXTREME event or route unavailability as defined in section 9, offer immediate safety release without forfeiture of the earned-to-date conditional progress: vest that progress into L and cancel only the future unearned part. Compute relief using the last valid surviving weight immediately before the safety event, not a post-event NAV-zero weight. End the election permanently; no second election. HOT alone does not force exit or vesting.

Unknown history is held pending, not treated as voluntary exit; an indexer gap at t_s keeps all of B_lock reserved until H_lock is final. Everyone choosing five days reduces the allocation to `B_lock ell_i/max(sum ell,12.5)` at maturity. Nobody choosing a lock returns all B_lock to Z. Locks cannot overrun the five-day Season.

## 9. Safe Mode

Map xSOL risk to its verified pool collateral state. CR is a ratio, and `leverage=CR/(CR-1)` is valid only for CR>1 under the documented healthy-liability assumptions. CR=1 is singular; do not display a finite leverage there.

| State | CR range | Entry / increase | Reward behavior |
|---|---|---|---|
| NORMAL | CR>=1.35 | Permitted when admission checks pass | Normal prospective accrual |
| WARM | 1.20<=CR<1.35 | Two-step risk acknowledgement/signature | No automatic haircut |
| HOT | 1.00<=CR<1.20 | **Permitted after two-step acknowledgement/signature if required protocol/route is available** | No automatic haircut solely for HOT |
| EXTREME | CR<1.00, or verified severe protocol/route failure | Acknowledge risk if an operation remains genuinely available; never submit an unavailable operation | Prospective verified NAV/eligibility reset; safety-release lock treatment |

The two steps are a specific risk screen and an explicit wallet signature bound to wallet, asset, action/amount, chain, epoch, risk/config version, nonce and expiry. It is risk consent, not literal two-factor authentication and not permission to transfer funds. Reconfirmation is required when action scope or risk version materially changes.

Risk severity and technical availability are separate. A label alone does not block HOT entry; an unavailable operation cannot be bypassed with a signature. Valid ordinary eligibility, pricing and ownership checks still apply. A blocked mint route is not proof that every swap/exit route is blocked; check the actual requested route and required normal exit path.

**Every admitted asset has an EXTREME path**, not only xSOL. For non-xSOL adapters the trigger is a verified depeg, exploit, protocol pause, or pinned-program upgrade (section 5). A reset is effective from the first finalized slot at which the trigger is observable on-chain, for all still-unsettled intervals, not from the later human/operator checkpoint.

**Route unavailability** means the asset's documented protocol mint/redeem/unstake/withdraw path is halted according to that protocol's on-chain state. DEX quote failure or thin pool depth alone is not route unavailability. **Hysteresis:** leaving EXTREME, and re-entering it, requires the condition to persist for a minimum dwell period; that period is a calibrated parameter, currently unset, and EXTREME-triggered lock safety release cannot be enabled until it is set.

Use official zone boundaries after verifying the deployed version. Current docs label the zone fee table as examples: do not hardcode those example fees. Extra volatility/drawdown/slippage triggers remain unset until calibrated. Stale or missing data is UNKNOWN, not proof of zero NAV: pause new recognition where data cannot be verified and reserve existing unsettled capacity.

For verified NAV collapse, apply `X_new=eligible_units * verified_emergency_NAV` prospectively from the authenticated event/checkpoint, with a signed/logged reference reset. For verified loss of the required operational eligibility route, stop affected future recognition until recovery. Recovery requires fresh route/data checks and a published prospective reference segment. No retroactive change to settled R or Q. Do not claim next epoch prices must be lower; markets may recover.

Safe Mode never deletes L, forces liquidation, increases leverage or takes custody. Core/loyalty claims already established remain available whenever the claim infrastructure itself is operational. Operational outages can delay payment; they do not cancel the debt. The base program's existing global pause is not the Fruit Safe Mode controller.

## 10. Solvency model

For each independently funded reward ledger, and also in aggregate:

```text
V >= L + U + Z
V = liquid, spendable reward-vault SOL, excluding rent and operating balances
L = established accrued/vested unclaimed liabilities
U = remaining commitments, including conditional locks and reserved unsettled capacity
Z = uncommitted available reserve
```

For uncertain past intervals, U holds settlement reserves even though some economic accrual may already have occurred; it is not available for reuse. This extends the shorthand “unaccrued commitments” without dropping unresolved obligations. Base and Fruit balances cannot back each other twice. Fee commissions have their own corresponding funded ledger/reserve constraint.

**Vault separation (normative).** Fruit core, loyalty and lock SOL are held in program-owned vaults separate from the base REWARD, CAPITAL and FEE vaults. No Fruit-ledger lamport ever enters a base vault; otherwise the base `assert_fully_funded` check (base vault lamports versus base outstanding) would count Fruit SOL and stop detecting a base shortfall. Referral commissions stay in the base FEE vault under the section 7 reservation counter.

| Transition | Accounting |
|---|---|
| Actual cleared funding a | V+=a; Z+=a |
| Commit epoch/bonus b | Z-=b; U+=b |
| Settle verified accrual δ | U-=δ; L+=δ |
| Vest conditional bonus δ | U-=δ; L+=δ |
| Pay established claim c | V-=c; L-=c atomically |
| Release verified unused capacity z | U-=z; Z+=z |
| Compound earned SOL c (base ledger only) | V-=c; L-=c, atomically matching actual capital+fee transfers; retain remainder. Not available on Fruit/loyalty/lock ledgers |
| Authorized reserve withdrawal z | V-=z; Z-=z; never reduce L or protected U |

In an ownership/indexer gap affecting a shared denominator, retain the **entire unsettled scheduled capacity for the affected common pool interval**, minus only liabilities already conclusively established. Do not use `r Δt w_max/H_observed` with an unverified denominator. For a known fixed lock contract, reserve its unsettled remaining award. Never release disputed funds merely because the epoch ended. Release only after complete reconciliation; no expiry can erase valid accrued rewards. **Terminal resolution:** if the missing history cannot be recovered after archive searches of at least two independent providers, the section 10a veto signer set may sign a final settlement computed from the best available finalized data. It can only add liabilities or release excess reserve to Z; it never reduces established L.

Proof for core Fruit: `sum dR/dt=r Cw/H<=r`, and `dR_i/dt<=r*2.5/12.5=r/5`. The same proof applies to loyalty because `0<=beta<=1`. The lock proof uses its fixed denominator and maximum 12.5 weight-days per wallet. Therefore every pool has a finite prefunded liability ceiling. The per-wallet 20% bound applies separately to each funded pool; with bonuses, total receipts are not bounded by E/5 alone.

Use rational/fixed-point cumulative entitlement and carried remainders; never round each wallet upward. If total epoch time is T seconds, cumulative scheduled lamport capacity can be `floor(E_lamports * elapsed_seconds/T)`. Differences of this function bound interval spending and release the final schedule remainder correctly. Carry wallet fractions so checkpoint frequency does not destroy meaningful entitlement. Claims enforce cumulative paid amounts and unique commitments atomically. Allocation revisions cannot claw back settled liabilities or spend the same budget twice; a correction requiring extra liabilities needs separate backing first.

External asset value, future fees, seed principal and unrealized yield do not count in V. Solvency in SOL does not insure external principal or guarantee system availability.

## 10a. On-chain Fruit ledger (minimum state and invariants)

Weights depend on external history, so allocations are computed off-chain by replay and posted as cumulative Merkle roots. The program, not the poster, enforces every budget. A compromised or faulty poster can at most propose a root inside the funded, capped, not-yet-final envelope, which the challenge window exists to veto.

Minimum state:

| Account | Fields (minimum) |
|---|---|
| `FruitConfig` (singleton) | state version; admin; root-poster key; veto signer set and threshold (poster is not a member); challenge-window seconds; frozen `P_ref`, `M=250`, `qmax=1/5`; per-ledger pause flags |
| `Ledger` (one per ledger k ∈ {core, loyalty, lock}) | vault PDA; counters `L_k`, `U_k`, `Z_k`; `paid_cum_k` |
| `Pool` (per ledger and epoch/Season) | pool id; budget `E`; `t_start`, `t_end`; `wallet_cap = floor(E/5)`; hash of committed per-asset references; `final_total`; `paid_cum`; status (Committed, Active, Ended, Reconciling, Closed); for lock pools the frozen `H_lock` |
| `Root` (per pool and version) | Merkle root; version; `checkpoint_time`; `total`; `posted_at`; `final_after = posted_at + challenge window`; status (Pending, Vetoed, Final) |
| `ClaimRecord` (per pool and wallet) | `claimed_cum` |

Leaf encoding is domain-separated: `hash(program_id, ledger_id, pool_id, wallet, cumulative_amount)`, so a leaf cannot be replayed across programs, ledgers, pools or wallets.

Invariants enforced on-chain:

1. **Solvency:** after every instruction, `vault_k lamports - rent_exempt_min >= L_k + U_k + Z_k` for each ledger, and in aggregate.
2. **Funding:** deposits do `V+=a, Z+=a`. Committing a pool requires `Z_k >= E` and moves `E` from Z to U. A lamport can be committed once; a pool cannot be committed after its `t_start`.
3. **Scheduled capacity:** a root is accepted only if `checkpoint_time <= now`, `version` is strictly greater than every earlier version for that pool, at most one root per pool is Pending, and `total <= floor(E * (min(checkpoint_time, t_end) - t_start) / (t_end - t_start))`. For lock pools, capacity is the fixed `sum A_i` from `H_lock`, vested by the section 8 schedule.
4. **Monotone finality:** `total` of a new root is at least the pool's `final_total`. Finalized liabilities cannot be reduced or clawed back.
5. **Challenge window:** a root becomes Final only when `now >= final_after` and it has not been vetoed. During the window the veto signer set, at its threshold, can mark it Vetoed; a vetoed root never finalizes. On finalization, `U -= (total - final_total)`, `L += (total - final_total)`, `final_total = total`.
6. **Per-wallet ceiling:** a claim is rejected if the leaf's `cumulative_amount > wallet_cap`.
7. **Replay-protected cumulative claims:** claims are verified only against the pool's latest Final root. Payment is `max(0, cumulative_amount - claimed_cum)`; then `claimed_cum = max(claimed_cum, cumulative_amount)`, `V -= payment`, `L -= payment`, `paid_cum += payment`, atomically. A duplicate or concurrent claim pays 0. A leaf lower than `claimed_cum` pays 0 and affects no other wallet. Hard guard: `paid_cum <= final_total`.
8. **Reserve until reconciliation:** unallocated pool capacity stays in U until the pool is Closed. Closing requires `now >= t_end`, no Pending root, and a Final root marked complete by the poster and not vetoed; then `U -= (E - final_total)`, `Z += (E - final_total)`. Unresolved shared-pool capacity is never released by elapsed time alone; terminal resolution follows section 10.
9. **Outflows:** lamports leave a Fruit vault only by a claim (against L) or an authorized reserve withdrawal (against Z only). No instruction moves Fruit lamports into a base vault or converts them into seeds.
10. **Lock pools:** conditional progress sits in U. Vesting moves U→L through finalized roots. Forfeited units return to Z only at pool close. `H_lock` is written once, at registration finalization, and is immutable.
11. **Versioning:** accounts carry a state version. Migrations preserve lamports, `L`, `U`, `claimed_cum` and pool/root history, and are one-shot and fail-closed. Pausing may stop new roots or registrations; it never reduces L. Claims are paused only if the claim path itself is unsafe.

Program upgrade authority, poster key, veto set, threshold and challenge-window length are required governance inputs (section 2), disclosed publicly, and not selected here.

## 11. Whale and Sybil protections

Hard Cherry holding cap, Fruit recognition cap, common denominator floor, one asset selection, finalized history, one lock election and one referral level constrain wallet behavior. They do not establish one wallet per person. A .skr entitlement/device is not proof of unique beneficial ownership; the Day-1 binding (section 4) only stops one domain from opening Day-1 access for more than one wallet.

Every split wallet needs its own qualifying seeds and simultaneously attributed external equity. Reusing a position sequentially earns only sequential ownership time, never overlapping historical rewards. Acquiring truly separate capital across wallets still increases aggregate share. Five fully covered 250-seed wallets controlled by one person can receive the whole core tranche when no one else qualifies; the individual-wallet ceiling still holds.

The following recomputed F=10 Storm scenarios assume identical fully covered wallets and unchanged weights for the full day. Payouts are mathematical SOL amounts rounded to 9 decimals for reporting, not per-wallet upward settlement rounding.

| Wallets | Seeds each | H | SOL per wallet | Weight-unit APR | Unallocated SOL |
|---:|---:|---:|---:|---:|---:|
| 5 | 150 | 12.5 | 1.004794520 | ~24,450% | 3.349315068 |
| 5 | 250 | 12.5 | 1.674657534 | ~24,450% | 0 |
| 25 | 150 | 37.5 | 0.334931507 | ~8,150% | 0 |
| 25 | 250 | 62.5 | 0.334931507 | ~4,890% | 0 |
| 100 | 150 | 150 | 0.083732877 | ~2,037.5% | 0 |
| 100 | 250 | 250 | 0.083732877 | ~1,222.5% | 0 |
| 500 | 150 | 750 | 0.016746575 | ~407.5% | 0 |
| 500 | 250 | 1,250 | 0.016746575 | ~244.5% | 0 |
| 1,000 | 150 | 1,500 | 0.008373288 | ~203.75% | 0 |
| 1,000 | 250 | 2,500 | 0.008373288 | ~122.25% | 0 |

APR crosses 10,000/5,000/2,500/1,200/600/250% at H approximately 30.5625/61.125/122.25/254.6875/509.375/1,222.5 SOL respectively. Above-floor doubling after hour one reduces an unchanged incumbent's full-day projection to 52.0833%; tenfold growth reduces it to 13.75%. Past accrual remains unchanged.

Fable's single-wallet 2,500-seed examples are invalid under the final hard cap and must be removed from ordinary permitted-user examples. Keep them only as legacy/migration or rejected-transaction tests. Wallet splitting remains a valid adversarial case. Person-level protection requires an actual enforceable identity/ownership mechanism; do not label this model Sybil-proof.

## 12. Exact final tokenomics table

| Parameter | Final rule |
|---|---|
| Seed price | 10,000,000 lamports; verify production configuration. Fruit math uses frozen P_ref=10,000,000 |
| Day-1 fresh max | 250 seeds / 2.5 SOL principal, fees additional |
| Day-1 .skr access | One domain = one wallet; binding PDA per name account for [T0,T0+24h) |
| Permanent Cherry current holding max | 250 seeds; enforced in engine buy_rigs/compound for every seed creation/receipt path |
| Legacy S>250 | Grandfathered and frozen: no confiscation, no growth, base claims preserved, Fruit recognizes 250 |
| Permanent Fruit benchmark | 250 seeds |
| Qualification | 150 current seeds; positive verified X needed for rewards |
| External holdings cap | None |
| Recognized external contribution | min(K,X), maximum 2.5 SOL-equivalent per wallet |
| Asset/referral Fruit multipliers | 1.00 |
| Active Fruit | One per wallet per epoch; next-epoch switching |
| Denominator | max(sum w,12.5) |
| Per-wallet core ceiling | E/5 per epoch |
| Day-1 Fruit reserve spend | Zero |
| Public opening | FRUIT_T0=T0+86,400 seconds |
| Core durations | 86,400 / 172,800 / 259,200 / 432,000 seconds |
| Core start offsets from FRUIT_T0 | 0 / 86,400 / 259,200 / 518,400 seconds |
| Core start offsets from T0 | 86,400 / 172,800 / 345,600 / 604,800 seconds |
| Core end | FRUIT_T0+950,400 seconds = T0+1,036,800 seconds |
| Harvest / Superfruit / Season | 684,931,507 / 513,698,631 / 428,082,192 lamports |
| Continuation R | 1,626,712,330 lamports |
| Storm, when F>=R | min(F-R,8,561,643,836 lamports) |
| Underfunded sequence | If 0<F<R, one 172,800-second Harvest funded with F |
| Loyalty tenure saturation | 11 verified qualifying days |
| Loyalty payout | Separate B_loy; integral of (B_loy/D) w beta/max(sum w beta,12.5) |
| Lock durations | 1 / 2 / 3 / 5 days |
| Lock election/cutoff | One immutable election per wallet/Season; finalize strictly before Season start |
| Lock award | B_lock ell d/max(sum ell d,62.5); fixed denominator |
| Lock vesting | Cliff; conditional progress reserved; safety release vests earned-to-date progress |
| Bonus per-wallet ceilings | B_loy/5 and B_lock/5 for respective funded pools |
| Referral level | One direct level |
| Referral payment | min(floor(c f_e), max(0, f_e - ceil(m v_e))); c unresolved; c=0 until reservation program changes ship |
| Referral margin bound | (1-c)φ>=m |
| Existing base referral weight | Preserve 105/100 funded base redistribution; no added Fruit weight |
| External normal exit | Documented normal-path <=432,000 seconds; no catastrophe guarantee |
| Borrowed/encumbered equity | X = max(0, unencumbered admitted value - registry debt), wallet-level; off-registry/off-chain borrowing undetectable and disclosed |
| Reference prices | Committed on-chain before epoch; manipulation-resistant; missing = asset inactive; later additions at min(ref, live) |
| Ownership evidence | Complete finalized event attribution; endpoint-only checks prohibited |
| xBTC / xHYPE | Disabled |
| JLP | Direct spot only; no leveraged/queued/vault exposure |
| HOT | Entry allowed after required two-step consent when protocol/route is available |
| Safe Mode liabilities | Already-accrued L never erased |
| Solvency | V>=L+U+Z per ledger and aggregate, separate Fruit vaults, including reserved unsettled obligations |
| No participants | No accrual; no infinite APR; unused capacity remains reserve |
| Claim expiry | None for valid accrued liabilities |
| Compound at cap | No new seed, no fee (engine revert accepted); earned SOL remains claimable |
| Fruit / bonus SOL | Claimable only; never compounds into seeds |
| Fruit settlement | On-chain ledger (section 10a): capacity-bounded roots, E/5 leaf cap, challenge window with independent veto, cumulative replay-protected claims |
| Loyalty tenure day | Requires w >= w_tenure_min (unset); B_loy=0 until set |
| Phase labels | Genesis >=1,200%; Early Grove 600–<1,200%; Growth 250–<600%; Mature >0–<250%; Dormant zero funded active rate |
| Rate display | Live denominator; distinguish weight, Cherry and combined-capital rates |
| Funding ends | Finish valid commitments, then zero new emission |

## 13. Exact changes required to CHERRY_TREE_FRUIT_TOKENOMICS.md

Apply this as a section-anchored replacement plan to the verified 562-line baseline. These are required edits, not a claim that the original file or GitHub has been modified. This final document is the canonical replacement specification.

| Baseline location | Exact required change |
|---|---|
| §1 Executive answer | Replace proposed-cap language with C_day1=M_hold=M=250 and T=150. Add permanent hard cap plus capped compounding. Replace strict-empty-whitelist rationale with normal-path eligibility and pending adapter/provenance gates. Retain F=10 only as an example; add live-H rate and corrected timeline. |
| §2 Genesis maximum | Replace policy alternatives/sensitivity recommendations with section 4's three separate checks. Keep sensitivity tables only as explicitly historical analysis, never current selectable configurations. State fees are additional to 2.5 SOL. |
| §3 Strict production whitelist | Delete the absolute exit guarantee. Insert section 5 admission rules/table; no asset becomes enabled merely from documentary research. Spot JLP only; xBTC/xHYPE stay disabled. |
| §3 address/rate registry | Retain candidate mints with source date and unverified-chain status; require live mint/program/decimals/extension checks. Add Fable's Jito pool candidate after validation. Do not carry stale APYs, guaranteed epoch wall times, or example Hylo fees as constants. |
| §4 historical lessons | Retain as non-normative background; no extra economic promises or unfunded incentives. |
| §5 mathematical model | Replace with section 1 plus section 5's X definition, event attribution and emergency prospective reference rule. Include missing-history common-denominator reserve treatment from section 10. |
| New §5a loyalty | Insert section 6, including integral accrual, separate committed funding, floor-aware boost ratio and explicit absence of a monetary chat multiplier. |
| New §5b referrals | Insert section 7; separate existing 1.05 base weight from c on actual fees. Leave c and m unresolved. |
| New §5c locks | Insert section 8 in full: registration cutoff, one election, fixed cohort denominator, partial shortfall, cliff, safety release and no second election. |
| §6 target funding matrix | Retain as hypothetical arithmetic; use H for weight-unit targets and actual nominal denominator for aggregate returns. Do not label hypothetical table entries funded. |
| §7 multipliers | Keep all Fruit asset multipliers at 1.00. Explain loyalty/locks as separate funds. Remove leveraged-JLP carry as an enabled-product implication. |
| §8 schedule | Replace with section 3. Public FRUIT_T0 is T0+1 day; sequence ends T0+12 days. Day-1 spend zero. Keep exact tranches/ceiling. Replace “no claim lock” with voluntary conditional-bonus commitment; accrued core rewards remain unlocked. |
| §8 reserves | Separate F, J, core commitments, bonus commitments and uncommitted future funds. Base runway ends no earlier than T0+12 days. A reserve lamport can be allocated only once. |
| §9 APR decay | Keep valid live-H arithmetic and label scenario assumptions. Use exact numerator 305624.999955 for F=10 or rounded 305625 with approximation sign. No floor-case APR as a guaranteed live rate. |
| §10 user examples | Remove the ordinary 2,500-seed wallet example. Replace with permitted 150/250-seed examples and a separate multiwallet attack case. Add compound-at-cap claimable-remainder behavior. |
| §11 anti-whale | Retain per-wallet proof but include permanent hard cap, finalized ownership and fixed lock denominator. Preserve the beneficial-owner impossibility warning. |
| §12 solvency | Replace with section 10. Add conditional/unsettled U subledgers, bonus funding, fee commission reservations, cumulative precision and no premature release at epoch end. |
| §13 failures | Add A→B→A, same-transaction carousel, authority change, missing denominator history, hidden borrowing, repeated lock election, partial lock shortfall, HOT consent, EXTREME relief, capped compound and fee-withdrawal races. |
| New §13a Safe Mode | Insert section 9; do not import Fable's HOT block or “next price must be lower” claim. |
| §14 canonical table | Replace entirely with section 12 of this architecture. |
| §15 handoff | Replace obsolete constants/gates with sections 2 and 14. Source-verified V2 cap expires at public opening and does not enforce the new hard holding cap. Require fresh branch/deployed-state pre-ops before implementation. |
| §15 calculator | Add headroom, separate core/loyalty/conditional-lock/vested/referral balances, pending verification, all budget sources, risk state/route status and full capital denominators. No bonus ratio when denominator is zero. |
| §15 tests | Replace superseded “251 earns capped weight” behavior with rejection/headroom tests; test legacy S>250 as grandfather-frozen (section 4). Add all section 14 settlement gates. |

Explicitly reject Fable edits E2/E18 (C=150), E4 (endpoint minimum), E5 (automatic whitelist activation), and E11's HOT blocking. Correct E9's universal B/E boost maximum, E10's unspecified registration/denominator, E13's 12-remaining-days condition at public opening, and E17's blanket branch-state assertions beyond the inspected source snapshots.

The closeout sections 1, 4, 5, 6, 7, 8, 9, 10 and 10a of this document supersede any baseline text on the same subjects, including baseline §3 financing language, §8 compounding, §12 solvency and §15 handoff.

## 14. Implementation gates

### 14.1 Status of original blockers

| # | Blocker | Status |
|---|---|---|
| 1 | External capital admission | **Closed by founder decision:** registry debt netting plus disclosure (section 5). Remaining input: registry contents (configuration, gate 14.2-D) |
| 2 | Hard-cap protocol integration | **Rule closed by founder decision** (section 4, including legacy grandfather-freeze). **Evidence closed** (gate 14.2-A): program account not found on mainnet → initial-deployment case; cap and `.skr` binding ship in the initial deployment; no migration and no legacy `S>250` wallet can exist under this deployment |
| 3 | Actual budget and base ledger | Configuration/evidence (gate 14.2-C); does not block code |
| 4 | Referral economics | Rule closed (section 7 clamp and reservation); `c=0` until m, φ and program changes exist |
| 5 | Ownership settlement design | **Closed by founder decision:** on-chain Fruit ledger (section 10a) and time convention (section 3). Governance values are configuration (gate 14.2-D) |
| 6 | Adapter and risk verification | Rule closed (section 5 pricing, pinning, encumbrance; section 9 EXTREME/hysteresis). Per-adapter verification is configuration; no adapter goes live unverified |
| 7 | Bonus configuration | Rule closed. `B_loy=0` until `w_tenure_min` is set; `B_lock` as committed |

### 14.2 Evidence and configuration required

A. **Deployed-state inspection (required before base-program implementation).** Read-only, from finalized mainnet state for program `C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt` or the actual production program ID if different: whether the program account and ProgramData exist; programdata hash and last-deploy slot versus the release artifact; upgrade authority; Config account bytes and version; `launch_t0`; `genesis_cap_rigs`; `rig_price_lamports`; `purchase_fee_bps`; vault PDA balances; and the maximum `Miner.rigs` over all Miner accounts. Outcomes: if the program is not initialized, the cap and binding ship in the initial deployment and no legacy wallet can exist; if it is initialized with `launch_t0 == 0`, the cap migration completes before T0; if T0 is set, the section 4 legacy rule applies to every `S>250` found. `genesis_cap_rigs != 250` requires the migration to correct it.

   **Result (closed):** founder-supplied read-only check `solana account C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt --url mainnet-beta` returned `AccountNotFound`. The declared program does not exist on mainnet, so no ProgramData, Config or Miner state exists at this ID. Outcome: **initial-deployment case**. The cap and `.skr` binding ship in the initial deployment; the section 4 legacy grandfather-freeze rule and cap-migration path are retained in code and tests but have no mainnet population under this deployment. Deploy-time conditions (not reopened design): initialize with `genesis_cap_rigs == 250` (test 8); if deployment uses a different program ID, or this ID is found to exist before deployment, re-run this gate against that ID before initialization.

B. **Release branch pin.** The exact implementation branch/commit and its test suite are recorded before code changes.

C. **Budget.** Live balances, base schedule, F, J, B_lock and base runway through `T0+12 days`, reconciled into L/U/Z.

D. **Configuration inputs (block activation of the feature, not code):** debt-protocol registry; per-adapter program pins, reference sources and calibrated thresholds; EXTREME dwell period; `w_tenure_min`; m and φ (then c); Fruit ledger poster key, veto set, threshold, challenge window and upgrade authority.

### 14.3 Test gate

Every test asserts, after every transition: `V_k >= L_k + U_k + Z_k` per ledger and in aggregate; `sum R <= E` and `R_i <= E/5` per pool; `paid_cum <= final_total`; established L never decreases except by payment.

**Cap, legacy and base program**
1. Fresh purchase with `S=249, count=2` reverts atomically with no lamport movement; `S=248, count=2` succeeds.
2. Compound with `S=240` and pending rewards that would buy 50 mints exactly 10, debits exactly `quote(10)`, and leaves the remainder claimable.
3. Compound at `S=250` creates no seed and charges no fee: revert or no-op, pending rewards and penalty anchor unchanged.
4. Legacy fixture `S=400`: purchase reverts; compound mints nothing and charges no fee; base claim works; base shares and `Miner.rigs` unchanged through the migration; Fruit `K=2.5`.
5. Referred wallet (weight 105) at the cap: `total_shares` includes exactly `250*105`.
6. After `set_pricing(price*2)`, Fruit `K` still uses `P_ref`. After `set_pricing` lowers the fee, the commission is clamped to preserve the margin.
7. Config migration: lamport and byte identity on failure; one-shot; every instruction fails cleanly during the inert window; existing liabilities preserved.
8. Pre-ops detects `genesis_cap_rigs != 250` on the deployed fixture.
9. The activation purchase counts against both check 1 and check 2.
10. The engine-level cap is covered by the existing engine fuzz/acceptance suites: no sequence of buy/compound reaches `S>250` from `S<=250`.
11. No instruction path converts Fruit core, loyalty, lock or commission SOL into seeds or into a base vault.

**Day-1 .skr binding**
12. Domain D used by wallet A on Day 1; wallet B proving D on Day 1 is rejected.
13. D transferred from A to B during Day 1; B's Day-1 purchase with D is still rejected. A can still buy with D up to its cap.
14. Binding has no effect at `T0+86,400` and later (public access).
15. A binding cannot be closed or reassigned during Day 1.

**Ownership, encumbrance and debt**
16. A→B→A across slots: A earns only A's intervals, B only B's.
17. A→B→C inside one transaction: no one earns extra duration.
18. Transfers in different slots sharing one `unix_timestamp`: zero overlap.
19. Token-account `SetAuthority(AccountOwner)` is treated as a transfer.
20. `Approve` of the full balance: X drops by `delegated_amount`; `Revoke` restores it prospectively only.
21. Frozen account excluded.
22. Token-2022 mint with permanent delegate or transfer hook rejected unless cleared by the adapter.
23. Flash-loan-sized intra-transaction balance: zero weight.
24. One missing transfer event reserves the whole common-pool interval, not only `r*dt*w_max/H` for one wallet.
25. Staked SKR with pending unstake excluded; the same units in wallet and stake views counted once.
26. Debt in a registry program is netted at wallet level; debt exceeding holdings gives `X=0`, never negative.
27. A debt increase mid-epoch reduces X from its event; unreadable registry state makes the wallet pending, not zero.
28. Debt in a non-registry program is not detected, and the disclosure is present in the product.
29. Owned xSOL is valued at owned-token NAV, not underlying notional.

**Pricing and Safe Mode**
30. Units added after the reference timestamp are valued at `min(ref, live)`.
31. A reference sample manipulated in one block is rejected by the confidence/TWAP rule.
32. An asset with no committed reference at epoch start is inactive with weight 0, and other assets settle normally.
33. A pinned-program upgrade mid-epoch makes the asset UNKNOWN from the upgrade slot and reserves it.
34. HOT entry: rejected without a consent signature; rejected with an expired, wrong-epoch, wrong-risk-version or replayed nonce; accepted with a valid one.
35. HOT entry to an unavailable route is rejected even with a signature.
36. EXTREME at slot s: the reset applies from s to unsettled intervals; settled R and Q unchanged. Covered for xSOL and at least one non-xSOL adapter.
37. CR oscillating across 1.00: hysteresis prevents repeated resets and safety releases; release cannot be enabled while the dwell period is unset.
38. CR = 1: no finite leverage displayed.
39. DEX quote failure alone does not count as route unavailability.

**Epochs and timeline**
40. Purchase at `T0+86,399`: Day-1 cap, no Fruit. At `T0+86,400`: public access and Storm accrual.
41. Registration finalized before t_s is included; at t_s it is excluded.
42. Every boundary at 86,400 / 259,200 / 518,400 / 950,400 from FRUIT_T0 is half-open.
43. `F=R`: Storm 0. `F=R-1` lamport: one 48-hour Harvest of F, no Season or lock. `F=0`: inactive.
44. Zero participants for a whole epoch: all capacity goes U→Z at close; APR N/A.
45. `Cw < 12.5`: the unallocated share goes U→Z, never to other wallets.
46. Registration-program outage: interval not earned. Indexer-only outage: earned and paid on replay.

**Loyalty and lock**
47. Dust-weight wallet below `w_tenure_min` accrues no tenure days; loyalty cannot activate with the threshold unset.
48. Penalized base claim resets tenure (from instruction/account data); unpenalized claim does not.
49. Each cause-table row produces its classified reset/freeze.
50. Locks with everyone at 5 days, nobody locking, and a partial shortfall (low-water mark, no resurrection on re-top-up).
51. Early exit then a second election attempt: rejected.
52. EXTREME release: vested progress uses the pre-event weight; the future part returns to Z only after ownership resolves.
53. Indexer gap at t_s: all of B_lock stays reserved until H_lock is final.
54. Registration stores intent only; validation occurs at t_s.

**On-chain Fruit ledger**
55. Root with `total` above scheduled capacity: rejected.
56. Root with `checkpoint_time > now`, non-increasing version, or a second Pending root: rejected.
57. Root with `total < final_total`: rejected.
58. Leaf above `wallet_cap`: claim rejected.
59. Leaf replayed across program, ledger, pool or wallet: rejected.
60. Claim against a Pending or Vetoed root: rejected. Vetoed root never finalizes. The poster cannot veto.
61. Corrected root lowering a wallet that already claimed: that wallet's claim pays 0; others unaffected; `paid_cum <= final_total` holds.
62. Two claims for one wallet in one slot: the second pays 0. N concurrent claimants: V never below L.
63. Checkpoint-frequency independence: 1 versus 86,400 checkpoints produce totals within the carried-remainder bound.
64. Final lamport remainder released exactly once at pool close.
65. Pool close without reconciliation, or with a Pending root, rejected; elapsed time alone releases nothing.
66. Terminal resolution cannot reduce L.
67. Migration of ledger accounts preserves lamports, L, U, `claimed_cum` and root history.

**Referral**
68. `withdraw_fees` above `distributable - reserved_commissions`: rejected.
69. Commission on a compound fee is reserved in the compound instruction.
70. Fee event replay: consumed once.
71. Rounding of `c*f_e` keeps the per-event retained margin; the clamp yields 0 when `m*v_e >= f_e`.
72. Self-referral rejected; non-planter referrer rejected, on the hard-cap build.

**Funding**
73. Committing a pool when `Z < E`: rejected. Committing after `t_start`: rejected.
74. Committing the same lamport to renewal and loyalty: rejected.
75. Reserve withdrawal touching L or U: rejected.
76. Funding exhaustion mid-sequence: already committed tranches finish.

Tests 1–76 are the implementation gate. Isolated accounting and tests may proceed now against explicit fixtures. Gate 14.2-A is closed (initial-deployment case), so base-program changes may proceed. Feature activation additionally requires 14.2-C and the relevant 14.2-D inputs. No launch date is selected.

### Source links

- [Fable solve](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/claude/cherry-tree-tokenomics-cfoilj/docs/FABLE_CHERRY_TREE_TOKENOMICS_SOLVE.md)
- [V2 state](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/mainnet/rc-hardening/programs/sol-rigs/src/state.rs)
- [V2 purchases](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/mainnet/rc-hardening/programs/sol-rigs/src/instructions/buy_rigs.rs)
- [V2 compounding](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/mainnet/rc-hardening/programs/sol-rigs/src/instructions/compound.rs)
- [Hylo equations and zone boundaries](https://docs.hylo.so/technical-addendum/hylo-equations)
- [Hylo mint registry](https://docs.hylo.so/security/onchain-addresses)
- [Jito general FAQs](https://www.jito.network/docs/jitosol/faqs/general-faqs/)
- [Solana Mobile SKR overview](https://solanamobile.com/blog/skr-is-live)
