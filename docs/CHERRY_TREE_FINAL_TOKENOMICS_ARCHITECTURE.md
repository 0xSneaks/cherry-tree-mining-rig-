# THE CHERRY TREE — Final Tokenomics Architecture

Date: 2026-09-26. Status: reconciled economic specification; implementation is gated by section 14. No production changes, deployment, funding or launch authorization.

Authority: founder overrides in this request, then Fable's quantitative solve, then the existing Fruit baseline. This document defines one model. Unresolved inputs remain configuration variables; no illustrative budget is asserted to exist.

Evidence reviewed:

- `docs/FABLE_CHERRY_TREE_TOKENOMICS_SOLVE.md` on `claude/cherry-tree-tokenomics-cfoilj`, Git blob `fe1be454609c52943112f92eee33e8dc1f302e6a`, and its review checklist.
- Both supplied baseline copies, `CHERRY_TREE_FRUIT_TOKENOMICS.md` and `CHERRY_TREE_FRUIT_TOKENOMICS(1).md`: 562 lines each, identical content.
- V2 `mainnet/rc-hardening`: `state.rs` blob `f60b1f2a174ab4187f6237e28af6c0d3acdb45a2`, `buy_rigs.rs` blob `76c22acbd63674cee581ae935492f82d9a22f9b2`, `compound.rs` blob `91ade0dd81c0771d3f25a8ac2c144dba38a69849`.
- Current official Hylo address and equations pages, plus Jito and Solana Mobile documentation. Asset admission still requires chain/configuration and adapter verification; reading documentation is not production certification.

Pre-ops result: source confirms that V2's current cap applies only to cumulative fresh purchases during the first 24 hours; compounds are excluded, and the cap expires at public opening. Source also confirms separate capital, fee and reward vault flows, immutable referral binding, and base referral weights of 100/105. A permanent hard holding cap is therefore a required protocol change, not a claim about current behavior. Live deployed configuration, balances and account migration state were not audited here.

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

`X_i,total` is the full external position included in that user's stated capital denominator, not merely the clipped recognized amount. Disclose reference value versus acquisition cost. Seed nominal value is not redeemable principal; compounded seed nominal value is not additional cash invested. Report gross SOL, actual period return, fees and market P&L separately. External yield and leverage are never added to Cherry APR. With zero participants, show N/A and zero accrual.

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
| Production price | Must equal adopted P before this schedule is used; a mismatch requires reconciliation, not silent substitution |
| Base schedule and B_base | Additional funding needed after dedicated base balances, liabilities and remaining commitments are reconciled |
| Net core Fruit budget F | Actual cleared funds assigned exclusively to core Fruit; after base and operational obligations are protected |
| Separately funded bonus capital J | Actual cleared bonus donation/allocation; not counted in F or other commitments |
| B_loy,p and B_lock,s | Nonnegative explicit committed budgets; aggregate commitments cannot exceed available bonus reserve |
| Fee fraction φ | Actual configured purchase_fee_bps/10,000; rehearsal 3% is not adopted as production fact |
| Required retained margin m | Fraction of fee-bearing principal volume, incorporating operations and any approved replenishment obligations |
| Referral cut c | Unset until m and φ are known; 0 <= c <= 1-m/φ, if φ>0 and 0<=m<=φ |
| Asset adapters / prices | Verified identifiers, owner/position decoding, reference feed, finality and staleness rules |
| Borrow/encumbrance proof | Enforceable admission evidence; unknown financing provenance cannot be certified by balance alone |
| Risk overlays | Calibrated feed-confidence, quote/slippage and freshness bounds; no guessed thresholds |
| Base hard-cap migration | Version/account migration and treatment of any existing S>250 must preserve earned claims |
| Allocation authority | Named trust model, independent replay, correction/finality rules and on-chain budget enforcement |

Demand and eligible-wallet counts are scenario inputs, never constants used to guarantee APR. No LP depth threshold can be derived from seed count: capital-vault SOL is not an AMM LP position. Any future market-liquidity gate needs a specified market, executable-size requirement and measured depth.

## 3. Launch timeline

All intervals are half-open. `FRUIT_T0 = T0 + 86,400 seconds`. Public access opens then; no intervening .sns-only window.

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

Before activation, verify funding, cap enforcement and required admission infrastructure. Public opening is time-based; it does not waive funding/verification requirements. If an operational failure prevents accrual, do not spend or retroactively catch up unearned capacity. Resolve uncertain already-running intervals under section 10.

At `F=10 SOL`, `E_storm=8.373287670 SOL`, `Z_future=0`. At fixed H, the exact Storm APR numerator is `305624.999955`, so `APR=305624.999955/H`. The usual `305625/H` is rounded. At H=12.5 this is approximately 24,450%, corresponding to 66.98630136% for that one day on matched seed reference capital. At equal matched external capital, the combined-capital period return is 33.49315068%, before fees/losses. No 10 SOL funding is asserted to exist.

After the sequence, new 48-hour Harvest epochs require a separately committed budget and fresh eligibility checks. No funded renewal means zero new emissions. Rate and phase follow actual funding and denominator, not the name of the epoch.

## 4. Max-wallet and qualification model

Adopt a **hard current Cherry holding cap of 250 seeds** to implement the founder's Cherry max-wallet constraint. This supersedes Fable's recommendation to use only a reward-power cap.

Three independent checks apply:

1. During `[T0,T0+24h)`, cumulative fresh purchases, including activation, cannot exceed `C_day1=250`.
2. At all times, every purchase, compound, mint, migration or supported seed receipt must preserve `S_i<=250`.
3. Fruit recognition uses `P min(S_i,250)` defensively, even if an invalid legacy position is encountered.

The Day-1 250-seed allowance costs 2.5 SOL of seed principal; purchase fees, rent and transaction costs are additional. Qualification remains 150 current seeds, or 1.5 SOL nominal value. It is consistent with the cap: `150<=250`, and the qualifying ratio is exactly 60%, not strictly greater than 60%.

For compounding:

```text
n_affordable = whole seeds purchasable from actually available earned SOL,
               using the existing exact price and fee calculation
n_minted = min(n_affordable, max(0, 250-S_i))
```

Debit only the exact price and fee for `n_minted`. Remaining earned SOL stays an accrued claimable liability. At the cap, compound is a no-op with no fee; it must not discard rewards, reroute them automatically or create overflow seeds. Fresh purchases exceeding headroom fail atomically. Existing base reward/penalty rules are not changed by this model except for the required seed-cap integration; Fruit claims do not inherit base claim penalties.

Existing S>250 positions cannot simply be clipped or confiscated during migration. Resolving them is a deployment-version prerequisite if any exist. No such position was verified here.

External holdings have no wallet cap. A wallet may hold 100 SOL-equivalent of an admitted Fruit, but at most 2.5 SOL-equivalent can affect Cherry weight. External assets remain outside Cherry custody.

## 5. Fruit eligibility model

One selected asset/adapter per wallet per epoch. A selected asset can have multiple verified accounts, but each underlying claim can be counted only once. Switching is effective next epoch; exiting is always permitted and ends eligibility for the exited units. A later join accrues only from its finalized registration and verified ownership time.

For position j attributed to wallet i:

```text
G_ij(t) = attributed gross position value at fixed epoch reference prices
D_ij(t) = verified borrowed principal attributable to that position,
          valued in the same numeraire
A_ij(t) = 1 only if asset, ownership, exit, financing and encumbrance
          admission checks all pass
X_i(t) = sum_selected_positions A_ij(t) max(0, G_ij(t)-D_ij(t))
```

Unknown borrowing/encumbrance evidence makes the affected position ineligible pending resolution. Unsupported pledged collateral, collateral/debt wrappers, pending unstake balances, vesting-locked units and duplicated underlying receipts are excluded. V1 does not admit lending/debt wrappers. An owned xSOL token's intrinsic leveraged exposure is not wallet borrowed principal; value the holder's owned token equity, not the underlying leveraged notional. Borrowing to purchase it is a separate financing obligation and is excluded from X.

Use fixed timestamped epoch prices and verified share conversion. Rebase/share-rate changes follow the adapter's specified accounting. CR/NAV failure can trigger the prospective emergency reset in section 9. Never reprice past liabilities. Live market values remain separately visible.

**Required attribution mechanism:** replay complete finalized ownership-changing history, including transfers, authority changes, mint/burn, stake/unstake, wrapper conversion and relevant Token-2022 changes. Construct a global event timeline and integrate ownership over disjoint intervals. One underlying unit/claim may support only its actual owner at a given instant. A→B→A earns A only its actual ownership time and B only B's; a same-transaction round trip has no additional positive holding duration. Seed ownership/eligibility changes are included in the same settlement timeline.

Endpoint balances, endpoint minima, wallet signatures and Merkle membership alone do not prove this. Pool denominators must be reconstructed on the same timeline. Missing events can change every participant's share, not merely the wallet with missing data. Cumulative roots require unique epoch/version identifiers, replay protection, conserved sums, per-wallet limits and independent replay. The indexer's truthfulness remains an explicit trust assumption unless independently enforceable proofs are implemented.

Transfer attribution does **not** prove absence of loans from another wallet, exchange or off-chain lender. A strict universal borrowed-funds exclusion cannot currently be certified. This is an unresolved admission blocker; it must not be silently replaced by an attestation-only claim or Fable's acceptance of borrowed tokens.

| Asset/form | Design disposition | Required activation evidence |
|---|---|---|
| Wallet SKR | Candidate | Mint/account identity, unlocked ownership, approved SKR/SOL pricing, normal executable exit and financing checks |
| Staked SKR | Conditional candidate | All preceding checks plus live IDL, beneficiary ownership, cooldown/epoch rules, withdraw path and pending-unstake exclusion |
| xSOL | Candidate with Safe Mode | Mint/pool version mapping, equity/NAV, CR/feed data, operational routes and financing checks |
| JitoSOL | Candidate | Mint/pool identity, share accounting, documented normal exit within five days and financing checks |
| Spot JLP | Candidate | Unencumbered direct token ownership, pool valuation and SOL conversion, normal exit evidence and financing checks |
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

Let d_i be consecutive verified qualifying days. During Day 1 this means S>=150 plus registered, fully verified positive eligible external equity, although Fruit emits zero. Thereafter it means w_i>0. A voluntary loss of eligibility or a penalized base claim resets the tenure clock; an ordinary unpenalized claim does not. A system-data outage freezes its settlement pending replay; an involuntary Safe Mode interruption freezes tenure without erasing its recorded past. Permanent activity history and leaderboard are not reset.

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
commission_e = floor_lamport(c f_e)
retained_e = f_e - commission_e
retained_e >= required_margin_e
```

For constant fee fraction φ and required retained margin m on fee-bearing principal volume v:

```text
f = φ v
(1-c) φ >= m
0 <= c <= 1-m/φ                   [φ>0 and 0<=m<=φ]
```

The bound does not uniquely determine c. Leave c unset until m and the actual production fee are known and a value within the bound is selected. If m>φ, no referral cut is feasible, even at zero. If φ=0, no commission exists. Exact integer settlement must also preserve the per-event retained margin after fee rounding.

Reserve commissions atomically before any fee withdrawal; fee recipient withdrawals can use only the remaining unencumbered fee balance. Once a compound's fee has actually reached FEE, its eligible commission is funded there; do not deduct a second commission from the user's accrued rewards. No retroactive promise of commissions on fees already withdrawn.

Each fee event is consumed once. Immutable first-purchase binding to a pre-existing planter prevents ordinary referral cycles, but linked-wallet fee rebates remain possible. Never claim complete person-level abuse prevention. Referral revenues can be concentrated without violating reward-vault solvency. Retained fees fund future rewards only after a separate actual transfer and commitment.

## 8. Lock model

A lock is a voluntary, noncustodial **commitment of eligible Fruit weight for a conditional bonus**. It does not lock external principal or already-earned core/loyalty rewards.

For the five-day Season beginning at `t_s=FRUIT_T0+6 days`:

- Registration window is `[FRUIT_T0,t_s)`. The finalized chain ordering determines inclusion; registrations at or after t_s are excluded.
- One immutable lock election per wallet per Season; cancellation or early exit cannot reopen registration for that Season.
- Register duration `d_i in {1,2,3,5}` days, selected Season Fruit and requested eligible weight. At t_s, set `ell_i=min(requested_weight,w_i(t_s))`; reject zero/invalid registrations. The registered position must satisfy the Season price/ownership checks.
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

On a verified EXTREME event or actual route/protocol unavailability, offer immediate safety release without forfeiture of the earned-to-date conditional progress: vest that progress into L and cancel only the future unearned part. Compute relief using the last valid surviving weight immediately before the safety event, not a post-event NAV-zero weight. End the election permanently; no second election. HOT alone does not force exit or vesting.

Unknown history is held pending, not treated as voluntary exit. Everyone choosing five days reduces the allocation to `B_lock ell_i/max(sum ell,12.5)` at maturity. Nobody choosing a lock returns all B_lock to Z. Locks cannot overrun the five-day Season.

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

| Transition | Accounting |
|---|---|
| Actual cleared funding a | V+=a; Z+=a |
| Commit epoch/bonus b | Z-=b; U+=b |
| Settle verified accrual δ | U-=δ; L+=δ |
| Vest conditional bonus δ | U-=δ; L+=δ |
| Pay established claim c | V-=c; L-=c atomically |
| Release verified unused capacity z | U-=z; Z+=z |
| Compound earned SOL c | V-=c; L-=c, atomically matching actual capital+fee transfers; retain remainder |
| Authorized reserve withdrawal z | V-=z; Z-=z; never reduce L or protected U |

In an ownership/indexer gap affecting a shared denominator, retain the **entire unsettled scheduled capacity for the affected common pool interval**, minus only liabilities already conclusively established. Do not use `r Δt w_max/H_observed` with an unverified denominator. For a known fixed lock contract, reserve its unsettled remaining award. Never release disputed funds merely because the epoch ended. Release only after complete reconciliation; no expiry can erase valid accrued rewards.

Proof for core Fruit: `sum dR/dt=r Cw/H<=r`, and `dR_i/dt<=r*2.5/12.5=r/5`. The same proof applies to loyalty because `0<=beta<=1`. The lock proof uses its fixed denominator and maximum 12.5 weight-days per wallet. Therefore every pool has a finite prefunded liability ceiling. The per-wallet 20% bound applies separately to each funded pool; with bonuses, total receipts are not bounded by E/5 alone.

Use rational/fixed-point cumulative entitlement and carried remainders; never round each wallet upward. If total epoch time is T seconds, cumulative scheduled lamport capacity can be `floor(E_lamports * elapsed_seconds/T)`. Differences of this function bound interval spending and release the final schedule remainder correctly. Carry wallet fractions so checkpoint frequency does not destroy meaningful entitlement. Claims enforce cumulative paid amounts and unique commitments atomically. Allocation revisions cannot claw back settled liabilities or spend the same budget twice; a correction requiring extra liabilities needs separate backing first.

External asset value, future fees, seed principal and unrealized yield do not count in V. Solvency in SOL does not insure external principal or guarantee system availability.

## 11. Whale and Sybil protections

Hard Cherry holding cap, Fruit recognition cap, common denominator floor, one asset selection, finalized history, one lock election and one referral level constrain wallet behavior. They do not establish one wallet per person. A .skr entitlement/device is not proof of unique beneficial ownership.

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
| Seed price | 10,000,000 lamports; verify production configuration |
| Day-1 fresh max | 250 seeds / 2.5 SOL principal, fees additional |
| Permanent Cherry current holding max | 250 seeds; all seed creation/receipt paths enforce it |
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
| Referral payment | floor(c × actually received eligible fee); c unresolved |
| Referral margin bound | (1-c)φ>=m |
| Existing base referral weight | Preserve 105/100 funded base redistribution; no added Fruit weight |
| External normal exit | Documented normal-path <=432,000 seconds; no catastrophe guarantee |
| Borrowed/unsupported encumbered equity | Excluded; unknown provenance unresolved/ineligible |
| Ownership evidence | Complete finalized event attribution; endpoint-only checks prohibited |
| xBTC / xHYPE | Disabled |
| JLP | Direct spot only; no leveraged/queued/vault exposure |
| HOT | Entry allowed after required two-step consent when protocol/route is available |
| Safe Mode liabilities | Already-accrued L never erased |
| Solvency | V>=L+U+Z, including reserved unsettled obligations |
| No participants | No accrual; no infinite APR; unused capacity remains reserve |
| Claim expiry | None for valid accrued liabilities |
| Compound at cap | No new seed; earned SOL remains claimable |
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
| §15 tests | Replace superseded “251 earns capped weight” behavior with rejection/headroom tests; retain legacy S>250 as migration failure tests. Add all section 14 settlement gates. |

Explicitly reject Fable edits E2/E18 (C=150), E4 (endpoint minimum), E5 (automatic whitelist activation), and E11's HOT blocking. Correct E9's universal B/E boost maximum, E10's unspecified registration/denominator, E13's 12-remaining-days condition at public opening, and E17's blanket branch-state assertions beyond the inspected source snapshots.

## 14. Remaining blockers before implementation

1. **Enforceable external capital admission.** Specify how eligible owned equity is distinguished from borrowed/encumbered capital. Global absence of borrowing is not provable from SPL balances/history. Strict exclusion remains unsatisfied until an enforceable scoped proof/trust model is accepted; do not launch a permissive adapter under a false guarantee.
2. **Hard-cap protocol integration.** Inspect the exact implementation branch, deployed program/version and all seed-creation/receipt paths. Implement permanent S<=250 and partial compound spending without discarding rewards. Resolve any legacy over-cap positions before activation. This is a protocol behavior change, not a frontend validation.
3. **Actual budget and base ledger.** Read live balances and exact base schedule, verify production price/fee, reconcile L/U/Z, and establish F and any separately funded J. Confirm base coverage through T0+12 days. No example amount becomes a funding instruction.
4. **Referral economics.** Supply m, verify φ, then select c within the proved feasible range. Specify fee-receipt attribution and atomically reserved commissions before withdrawal. Until configured/funded, no monetary referral promise is active.
5. **Ownership settlement design.** Select finalized-history sources, full event coverage, time/finality convention, allocation authority, independent replay and correction policy. Merkle roots alone prove neither ownership nor non-borrowing. Missing shared-denominator data must reserve the common unsettled budget.
6. **Adapter and risk verification.** Pin asset/program versions, accounts, decimals, extensions, oracle/reference feeds, normal exit evidence and route health. Resolve staked-SKR IDL/cooldown mechanics and spot-JLP price conversion. xBTC/xHYPE remain disabled. Calibrate optional risk overlays before enabling them.
7. **Bonus configuration.** Commit explicit B_loy and B_lock, or leave them zero. Implement the fixed Season cohort, strict cutoff, one-election registry, surviving-weight tracking and safety release. No headline boost exists before its backing is committed.
8. **Accounting and adversarial verification.** Verify 149/150 and 249/250 boundaries; rejection/partial compounding at 251; exact public boundary; no Day-1 Fruit spending; ownership carousel and debt exclusion; denominator outage reserves; all lock durations/early exits/re-entry attempts; HOT acknowledgement and unavailable-route rejection; EXTREME accrued-progress preservation; fee commission replay/withdrawals; rounding/checkpoint independence; simultaneous/replayed claims; and funding exhaustion. Assert V>=L+U+Z after every transition, per-wallet/pool ceilings, and preservation of accrued claims.

Next action is a gated implementation specification against the inspected release branch once blockers 1–7 have concrete evidence/configuration. Work on isolated accounting/tests may proceed against explicit fixtures, but unresolved parameters or admission proof must not be represented as solved production behavior. No launch date is selected.

### Source links

- [Fable solve](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/claude/cherry-tree-tokenomics-cfoilj/docs/FABLE_CHERRY_TREE_TOKENOMICS_SOLVE.md)
- [V2 state](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/mainnet/rc-hardening/programs/sol-rigs/src/state.rs)
- [V2 purchases](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/mainnet/rc-hardening/programs/sol-rigs/src/instructions/buy_rigs.rs)
- [V2 compounding](https://github.com/PhantomCapAI/cherry-tree-mining-rig-/blob/mainnet/rc-hardening/programs/sol-rigs/src/instructions/compound.rs)
- [Hylo equations and zone boundaries](https://docs.hylo.so/technical-addendum/hylo-equations)
- [Hylo mint registry](https://docs.hylo.so/security/onchain-addresses)
- [Jito general FAQs](https://www.jito.network/docs/jitosol/faqs/general-faqs/)
- [Solana Mobile SKR overview](https://solanamobile.com/blog/skr-is-live)
