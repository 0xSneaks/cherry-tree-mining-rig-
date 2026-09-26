# THE CHERRY TREE — Fable Tokenomics Solve

Date: 2026-09-26. Inputs: `docs/FABLE_CHERRY_TREE_TOKENOMICS_CHECKLIST.md`, `CHERRY_TREE_FRUIT_TOKENOMICS.md` (the Fruit Pool Economic Solve, 562 lines), the engine and program sources on `main` (V1) and `mainnet/rc-hardening` (V2: launch gate, genesis cap, claim penalty, referral weight, yield curve), `docs/backlog/GENESIS_CAP_DECISION.md`, `docs/models/*`, and the official Hylo, Jito, Jupiter and Solana Mobile documentation fetched today. Design review only. No code, no deployment, no funding.

Notation follows the Fruit doc: `P` seed price, `M` cap, `T` threshold, `K_i = P·min(S_i, M)`, `X_i` external equity at epoch reference price, `w_i = min(K_i, X_i)` once `S_i ≥ T`, `Cw = Σw_i`, `H = max(Cw, H_floor)`, `E` epoch tranche, `D` epoch days, `r = E/D`. All SOL figures are SOL-equivalent at `P = 0.01`.

---

## 1. Verified existing math

Every figure below was recomputed independently in exact rational arithmetic. No arithmetic error was found in the Fruit doc.

| Claim | Recomputed | Status |
|---|---|---|
| `K_max = P·M = 2.5` | 0.01 × 250 = 2.5 | ✓ |
| `T = ceil(3M/5) = 150 = 60% of M` | 150/250 = 0.6 | ✓ |
| `H_floor = K_max / q_max = 12.5` | 2.5 / 0.2 = 12.5 | ✓ |
| Harvest tranche `ceil_lamport(10·12.5·2/365)` | 250/365 = 0.684931506849… → 0.684931507 | ✓ |
| Superfruit `ceil_lamport(5·12.5·3/365)` | 187.5/365 = 0.513698630137… → 0.513698631 | ✓ |
| Season `ceil_lamport(2.5·12.5·5/365)` | 156.25/365 = 0.428082191781… → 0.428082192 | ✓ |
| Continuation total `R` | 1.626712330 | ✓ |
| Opening Storm `F − R` | 10 − 1.626712330 = 8.373287670 | ✓ |
| Storm 24 h return at floor | 8.373287670 / 12.5 = 66.98630136% | ✓ |
| Annualized | × 365 = 24,449.99995% ≈ 24,450% (the 5·10⁻⁵ gap is the lamport ceil on the tranches) | ✓ |
| Return on combined capital | 66.9863% / 2 = 33.4932% | ✓ |
| Prudent Storm ceiling `ceil_lamport(250·12.5/365)` | 3125/365 → 8.561643836 | ✓ |
| `APR_unit,max = 2,920·E` % for a 1-day epoch | 36,500/12.5 = 2,920 | ✓ |
| Whale share vs n−1 threshold wallets `1/[1+0.6(n−1)]` | n=5: 29.4118%, n=25: 6.4935%, n=100: 1.6556% | ✓ |
| 50% whale case: 24×1.5 + whale capped 2.5 → H = 38.5 | 8.37328767 × 2.5/38.5 = 0.543720, 6.4935% of E | ✓ |
| User examples (Small/Medium/Large) | 0.334932 / 0.543720 / 0.543720 SOL; 22.3288% / 21.7488% / 2.1749% | ✓ |
| Cw doubles after hour 1 / grows 10× | (1 + 23/2)/24 = 52.0833% ; (1 + 23/10)/24 = 13.75% | ✓ |
| §2 sensitivity table (M = 250…2,500) | 637.5/225, 600/325, 525/337.5, 525/400, 375/375, 375/375 | ✓ |
| §2 "25 threshold wallets, 1 SOL/day" APRs | 973.33 / 486.67 / 324.44 / 243.33 / 162.22 / 97.33% | ✓ |
| §6 funding matrix `E = APR·C·D/365` | spot-checked 8 cells incl. 3,424.657534 | ✓ |
| §7 multiplier shares [1.25,1.35,1.5,1.5,1.75]/7.35 | 17.0068 / 18.3673 / 20.4082 / 20.4082 / 23.8095% | ✓ |
| §9 decay table | every row = 24,450 × 12.5/H, 1,000 × 12.5/H, … | ✓ |

Equation checks:

- `Σ_i dR_i/dt = r·Cw/H ≤ r` ⇒ total accrual ≤ E. ✓
- `dR_i/dt ≤ r·K_max/H_floor = r/5` ⇒ `R_i ≤ E/5`. ✓ (holds only while every bonus pool also uses a floored denominator — see §7.)
- Undistributed capacity when `Cw < 12.5` is `E·(1 − Cw/12.5)`; it stays in `Z`. ✓
- The doc's floored integer allocation matches the engine's per-second floor (`accumulator.rs::sync`): rounding favours the vault.

**Closed form that the doc does not state and should:** with the F = 10 Storm, `APR_open(H) = 365 × 8.373287670 × 100 / H = 305,625 / H` percent (H in SOL). Everything in §12 follows from it.

---

## 2. Conflicts / outdated assumptions

| # | Fruit doc says | Repo / checklist says | Resolution in this solve |
|---|---|---|---|
| C1 | Storm starts at `T0`; offsets 0 / 1 / 3 / 6 days | Checklist §2: Day 1 is the `.skr` window, **public open = `FRUIT_T0` = T0 + 86,400 s**; `launch.rs` makes Public exactly at T0 + 24 h | Shift every Fruit offset by +86,400 s. Storm = [T0+1d, T0+2d), Harvest = [T0+2d, T0+4d), Superfruit = [T0+4d, T0+7d), Season = [T0+7d, T0+12d). |
| C2 | Genesis fresh-purchase cap = M = 250 | `GENESIS_CAP_DECISION.md`: 250 is the "loose" band, set once with no setter; checklist asks for the Day-1 cap separately | Day-1 cap `C = T = 150` (§4). Fruit reward-power cap stays `M = 250`. |
| C3 | "Referral weight 1.00; no Fruit referral emission" | V2 code: referred wallet plants at 105/100 base weight, no payout. Checklist §6: referrer gets a continuing cut `c` of eligible fees | Keep the on-chain 1.05× base weight (it is a redistribution, already funded). Add the fee cut from the FEE vault. Fruit weight unaffected. |
| C4 | "External exit ceiling 432,000 s including liquidity failure, no exceptions" ⇒ whitelist empty | Checklist §4 asks to validate *normal* exit/cooldown mechanics and lists explicit exclusions | No asset, including SOL, satisfies a guaranteed exit under liquidity failure. Replace with a normal-path rule (§11). This is a founder requirement change; the doc's Gate 2 says so and this solve recommends making it. |
| C5 | No lock, no boost, no Safe Mode | Checklist §5, §7, §8 require them | Added (§7, §9, §10) as separately funded pools; base equations untouched. |
| C6 | "Base price, Genesis mechanism and phase labels accepted from the brief" | `main` is V1 (no genesis gate, no penalty, no referral weight). Those exist only on `mainnet/rc-hardening` | The Fruit model presupposes V2. Nothing on `main` implements a Fruit vault, epochs or external weight. |
| C7 | "Genesis M … pending pre-launch adoption" | Genesis cap and Fruit cap are different parameters | Name them differently: `C` (on-chain fresh-purchase cap, 24 h) vs `M` (Fruit reward-power cap, permanent). |
| C8 | JitoSOL listed with the SPL stake-pool *program* only | Jito docs also publish the pool account `Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb`, 0.1% delayed-unstake fee, ≤ 1 epoch | Add to the registry (§11). |
| C9 | 24,450% is presented as "the" Storm rate | It is the `Cw ≤ 12.5` case only: ≤ 5 max wallets or ≤ 8 threshold wallets. With the genesis model's *baseline* Day-1 cohort (200 wallets) the opening rate is 611–1,019% | Never display the floor-case APR; display `305,625/H` from live `H` (§18 edit E3). |
| C10 | "11-day sequence" | 83.7% of F pays out in the first 24 h; at H = 250 the continuation epochs pay 50 / 25 / 12.5% APR | Keep the shape (it is the doc's stated policy frontier) but state the front-loading and size `Future_reserve` for repeat Harvest epochs, which are the cheapest retention instrument in the model (0.684931507 SOL each at floor reference). |

---

## 3. Recommended Cherry max-wallet equation

Compared structures:

| Structure | Splitting incentive below M | Splitting incentive above M | Compounding (core loop) | On-chain change | Explainable |
|---|---|---|---|---|---|
| A. Hard holding cap (wallet cannot exceed M seeds) | none | n/a (blocked) | **breaks** `compound` above M; genesis cap already excludes compounds by design | new program version + requalification | yes |
| B. Reward-power cap `K = P·min(S, M)` | none | yes, but only by deploying real capital | preserved | none (off-chain Fruit layer) | yes |
| C. Diminishing curve (sqrt, log, `1−e^{−S/M}`) | **yes, everywhere** (concave ⇒ Jensen: splitting always raises Σweight) | yes | preserved | none | no |

Per-SOL return under B is uniform: a wallet at any `S ∈ [T, M]` with full coverage earns `r/H` per unit weight and needs `K + X = 2w` of capital, so return on deployed capital is `E/(2H)` per epoch for everyone. A whale gains no yield per SOL by splitting; it only gains *share*. Concave curves make splitting yield-positive, which is strictly worse. A hard cap adds a program version for no economic gain.

**Decision: B.** `w_i = 1[S_i ≥ T] · min(P·min(S_i, M), X_i)`, `M = 250`, permanent. Whale/Sybil tables are in §14.

---

## 4. Day-1 `.skr` early-access model

The 24-hour `.skr` window is a Tree-only day. Fruit reserve spend on Day 1 = **0**.

| Question | Answer | Derivation |
|---|---|---|
| Day-1 fresh-purchase cap `C` relative to the public Fruit cap `M` | `C = T = 150` (60% of M) | Smallest `C` that makes every Day-1 wallet Fruit-eligible at `FRUIT_T0` with no public top-up (needs `S ≥ T`). Between the genesis model's 100 and 250 rows (5-wallet actor 12.9% → 23.5% of Genesis seeds at baseline). Anyone can top up to M at `FRUIT_T0`, so Day-1 holds no Fruit-weight head start, only eligibility. |
| Fruit reserve spendable during Day 1 | 0 | Day-1 `Cw` is bounded by `N_skr × 1.5` and the cohort is gated; spending the Storm into a gated cohort is the maximum-concentration case (§14). Undistributed capacity returns to `Z` anyway, so nothing is gained by opening early. |
| Reserve protected for public `FRUIT_T0` | 100% of `F` (`R + Storm + Future_reserve`) | `U` after epoch creation = `R + Storm`; `Z = Future_reserve`. |
| What Day-1 users earn | (a) the Tree's own genesis emission, which is hyperbolic in total seeds and already gives the earliest cohort the largest share (`YIELD.md`, s08); (b) qualification history: Fruit weight > 0 from the first second of the Storm if `X` is registered; (c) the loyalty tenure clock (§7) starts on Day 1 for seed-qualified wallets | No new Fruit-funded Day-1 incentive. Quantifying (a) needs the Tree schedule (`fund_rewards` amount/duration), which is an unresolved input. |
| Transition condition to full public Fruit | Time: exactly `T0 + 86,400 s` (on-chain). Economic preconditions, all checked before the Storm tranche is committed: `F ≥ R`; Tree base funded runway ≥ 12 days remaining (Fruit must never outlive base); `V ≥ L + U + Z` verified on the Fruit vault; `Cw(FRUIT_T0)` read and the live opening APR `305,625/H` published instead of the floor figure | The doc's own rule "do not raid committed base rewards", made operational. |

---

## 5. Public `FRUIT_T0` reserve model

Unchanged from the Fruit doc §8 except for the +1 day offset (C1):

```
R      = 1.626712330 SOL   (Harvest 0.684931507 + Superfruit 0.513698631 + Season 0.428082192)
Storm  = min(max(F − R, 0), 8.561643836)
Future = F − R − Storm
```

Reserve sizing for a *target* opening rate `A` against *expected* qualified weight `H_exp` (1-day Storm):

```
E_storm(A, H_exp) = A/100 × H_exp / 365
```

| `H_exp` (SOL) | A = 10,000% | A = 2,500% | A = 1,200% | A = 600% |
|---:|---:|---:|---:|---:|
| 30.5625 | 8.373 | 2.093 | 1.005 | 0.502 |
| 100 | 27.397 | 6.849 | 3.288 | 1.644 |
| 254.6875 | 69.777 | 17.444 | 8.373 | 4.187 |
| 500 | 136.986 | 34.247 | 16.438 | 8.219 |

Reading: F = 10 buys 10,000% only if ≤ 30.6 SOL of matched weight shows up. If the Day-1 cohort alone brings 200 wallets at threshold (`Cw = 300`), the same tranche opens at 1,019%. The reserve does not scale the rate; participation does. Size `Future_reserve` in whole Harvest tranches (`floor(Future / 0.684931507)` repeat 48-hour epochs at floor reference).

---

## 6. Qualification threshold analysis

`T` does three things: sets the minimum Fruit position (1.5 SOL seeds + up to 1.5 SOL external = 3 SOL at full coverage), sets the linear range `[0.6, 1.0]·K_max`, and sets the unit of Sybil concentration (one threshold wallet = 1.545 SOL with the 3% rehearsal fee).

It does **not** change per-SOL return (uniform under §3) and does not protect against beneficial-owner splitting (a whale's cost per split wallet is real capital, not a fee). Lowering `T` widens participation and dilutes faster; raising it narrows participation and slows dilution. There is no quantitative optimum without the seed-size distribution at enrolment (`N_q(T) = N × Pr(S ≥ T ∧ external eligible)`).

**Decision: keep `T = ceil(3M/5) = 150`**, defined as a function of `M` so the ratio survives any future `M` change. Reasons: it is above dust, it keeps the Day-1 cap `C = T` meaningful, and the doc's sensitivity table shows qualified count falling monotonically with `T` while capped weight does not rise monotonically with `M`.

---

## 7. Boost / loyalty equation

**Constraint that decides the design:** a boost that multiplies base weight is zero-sum inside `E` and taxes newcomers.

| max boost `b` | share at max `f` | unboosted wallet gets | max-boosted wallet gets |
|---:|---:|---:|---:|
| 0.25 | 10% | 97.6% of no-boost payout | 122.0% |
| 0.25 | 50% | 88.9% | 111.1% |
| 0.25 | 80% | 83.3% | 104.2% |
| 0.50 | 80% | 71.4% | 107.1% |
| 1.00 | 80% | 55.6% | 111.1% |

At 80% adoption a multiplicative boost is a 17–44% tax on new users for a 4–11% gain to veterans. That violates "new users must still have a reason to join". Therefore:

**Recommended equation (separate funded bonus pool, base untouched):**

```
base_i     = E · w_i / H                                   (unchanged)
β_i        = min(1, d_i / d_cap)                           d_i = consecutive days with w_i > 0 and no penalised claim
bonus_i    = B_loy · w_i·β_i / max(Σ_j w_j·β_j , 12.5)      B_loy committed from Z at epoch creation
realised boost_i = bonus_i / base_i
```

- Curve comparison: fixed tiers → cliff gaming; capped linear → simple, reaches max, no permanent veteran edge; diminishing/log → asymptotic or uncapped, harder to explain, no economic gain over capped linear; percentile/rank → zero-sum by construction, newcomers always bottom (rejected); hybrid with chat/GM/GN multipliers → farmable at zero capital cost (rejected as a multiplier, allowed only as an eligibility gate with rate limits).
- `d_cap` recommendation: **11 days**, the length of the first Fruit sequence, so a wallet that joins at the Storm and stays through the Season reaches max by the Season's end. This is derived from the schedule, not tuned.
- Maximum boost: not a design constant. It is `B_loy / E` per epoch, bounded by `B_loy ≤ Future_reserve`. The per-wallet cap `bonus_i ≤ B_loy/5` follows from the same 12.5 floor. With F = 10, `Future_reserve = 0` ⇒ `B_loy = 0` ⇒ no boost is fundable; boosts begin at `F > 10.188356` SOL.
- If 10% / 50% / 80% of wallets reach max: newcomers lose nothing; the realised boost of a max wallet is `(B_loy/E)/f_eff` where `f_eff = Σw·β / Σw`. Boost is a share of a fixed pool, not a budget multiplier.

Funding requirement for a *target* realised boost `b` on base with fraction `f` at max (equal wallets): `B_loy = b·f·E`.

| `f` | b = 0.10 | b = 0.25 | b = 0.50 |
|---:|---:|---:|---:|
| 10% | 0.010 E | 0.025 E | 0.050 E |
| 50% | 0.050 E | 0.125 E | 0.250 E |
| 80% | 0.080 E | 0.200 E | 0.400 E |

The wallet count (25 / 100 / 500 / 1,000) does not change `B_loy` for a given `(b, f)`; it changes the per-wallet amount (`base` at Storm: 0.334932 / 0.083733 / 0.016747 / 0.008373 SOL for max wallets). The `b` values are sweep points, not recommendations; `B_loy` is a founder budget input.

---

## 8. Referral economics equation

Eligible fee `f` = the only protocol-generated fee in the program: `purchase_fee_bps` on fresh purchases **and** on compounds of the referred wallet (both land in the FEE vault; claim penalties are not fees, they are redistributed to other planters).

```
referral_i   = c · φ · (fresh_capital_i + compound_capital_i)        paid from FEE, never from REWARD or CAPITAL
retained     = (1 − c) · φ · volume
c_max        = 1 − m/φ            m = required protocol margin on purchase volume (unresolved input)
```

| φ | c = 0.10 | c = 0.20 | c = 0.30 | c = 0.50 |
|---|---|---|---|---|
| 300 bps (rehearsal) | 30 / 270 bps | 60 / 240 | 90 / 210 | 150 / 150 |
| 1,000 bps (program max) | 100 / 900 bps | 200 / 800 | 300 / 700 | 500 / 500 |

(referral / retained per 10,000 of volume.)

Stress results:
- Low, normal, high volume: burden is linear in volume, `c·φ`; retained margin `(1−c)·φ` is volume-independent. No solvency exposure at any volume because REWARD is untouched.
- 100% referred users: retained = `(1−c)·φ` on everything. `c` is a revenue split, nothing else.
- Few very large referrers: concentration of `c·φ·volume` in few hands; no protocol risk; cap per-referrer payouts only if `m` requires it.
- Self / circular / multi-wallet referral: economically a fee rebate of `c·φ` on the abuser's own purchases (0.30–1.50% at φ = 3%). Cannot create SOL. Mitigation: the existing V2 rules (one immutable referrer bound at first purchase, no self-referral, referrer must be a planter) plus paying `c` only on fees actually received.
- `c` fixed, not tenure-scaled: the referred wallet's fee stream already grows with the referrer's real activity; a tenure ladder adds gaming and has no revenue basis.

`c` cannot be solved without `m`. Unresolved.

---

## 9. Lock / vesting equation

Existing soft lock: the V2 claim penalty (25/15/5% inside 72 h of a fresh purchase, redistributed to other planters). The Fruit lock is a separate, voluntary commitment on Fruit weight.

```
lock pool     B_lock, committed from Z at Season open (locks span 1–5 days; commit up front, never from future epochs)
lock weight   u_i = w_i · d_i            d_i ∈ {1, 2, 3, 5} days   (weight-days is the protocol benefit)
lock bonus_i  = B_lock · u_i / max(Σ_j u_j , 12.5 × 5)
vesting       cliff at lock end; accrues as U, moves to L only at d_i
early exit    unvested lock bonus → Z; base accrual and already-vested bonus untouched
```

| Option | Everyone longest lock | Nobody locks | Early exit |
|---|---|---|---|
| Single 5-day lock | pool splits by `w`; realised boost `B_lock/(5·Cw)` per weight-day | `B_lock → Z` | forfeits whole unvested bonus |
| Ladder 24/48/72 h/5 d, weight-days | identical to single lock when all choose 5 d; otherwise longer locks get proportionally more | `B_lock → Z` | same rule |

**Decision: the ladder**, because allocation by weight-days is the one non-arbitrary rule (bonus ∝ what the protocol gains) and the single lock is its degenerate case. Maximum supported boost per weight-day = `B_lock / (Σu)`, bounded per wallet at `B_lock/5`.

Cases where another treatment is necessary:
1. **Cross-epoch locks must be committed at creation** (`U += B_lock` at Season open), otherwise a lock could promise a bonus a later unfunded epoch cannot pay.
2. **Weight drops below the locked amount** (external transfer-out, `X` falls): treat as early exit of the shortfall.
3. **Safe Mode EXTREME on the locked Fruit** (§10): waive forfeiture and vest the accrued bonus. The protocol must not pay users to keep holding a destabilised asset. Cost is bounded by `B_lock`.

---

## 10. Leveraged Fruit Safe-Mode framework

Primary feed: the Hylo pool's collateral ratio, which Hylo itself maps to six zones (docs.hylo.so, Hylo Equations). Effective leverage is `L = CR/(CR − 1)`.

| Hylo zone (official) | CR | L | Cherry state |
|---|---|---|---|
| Buy Zone 2 / Buy Zone 1 / Neutral | ≥ 135% | ≤ 3.86× | NORMAL |
| Sell Zone 1 | 120–135% | 3.86–6× | WARM |
| Sell Zone 2 | 100–120% | 6× → ∞ | HOT |
| Destabilized | < 100% (xASSET NAV → 0, mint/redeem halted) | — | EXTREME |

Overlays (any one escalates by one state; EXTREME if the route is blocked): oracle/reference-price staleness beyond the oracle's own published staleness bound; exit-route condition from Hylo's zone fee table (docs show 100 / 400 / 800 bps redeem in normal / sell 1 / sell 2, labelled "example" by Hylo, and blocked in Destabilized); protocol paused or program upgraded. Realised volatility, drawdown and recent price move are recorded but **not thresholded** until calibrated.

Data required to calibrate: per-pool CR time series; xSOL / xBTC NAV history; oracle update-interval distribution; Hylo's live fee-table parameters; xBTC borrow-rate history (paid via NAV reduction).

Actions by state:

| State | Warning | New entry / increase | Recognised weight | Bonus pools | User control |
|---|---|---|---|---|---|
| NORMAL | none | open | `min(K, X)` at epoch reference | normal | full |
| WARM | risk screen + explicit confirmation (two-step) | open after confirmation | unchanged | unchanged | full |
| HOT | persistent | **blocked** for that Fruit | unchanged this epoch; next epoch reference price will be lower by construction | unchanged | exit any time; base accrual keeps |
| EXTREME | persistent | blocked | `X` re-referenced to NAV (→ 0 in Destabilized) at next checkpoint; nothing accrued is touched | lock forfeiture waived (§9) | exit any time |

No liquidation, no leverage change: Cherry never holds the asset. The NAV mechanism already reduces recognised weight without any invented haircut.

---

## 11. Fruit candidate validation table

Exit rule applied: normal-path exit ≤ 432,000 s per official documentation, no custody, on-chain verifiability. (The literal "guaranteed even under liquidity failure" rule is unsatisfiable for any asset and is recommended to be dropped, C4.)

| Asset | Identity (official) | Wallet verification | Normal exit | Leverage / borrow | Yield figure usable? | Encumbrance rule | Verdict |
|---|---|---|---|---|---|---|---|
| **xSOL** | mint `4sWNB8zGWHkh6UnmwiEtzNxL4XrN7uK9tosbESbJFfVs` (docs.hylo.so/security/onchain-addresses) | SPL balance in wallet | mint/redeem at NAV "without slippage regardless of size" (Hylo); instant | dynamic, target 3× at CR 150%; no borrow cost ("free leverage"); volatility decay | no cash yield; never add leverage to APR | balance in wallet only; deposited-as-collateral leaves the wallet ⇒ not counted | **Valid** with Safe Mode |
| **xBTC** | official address page: `[TBD]` | — | same mechanism | configurable borrow rate paid via NAV | — | — | **Blocked** until Hylo publishes the mint |
| **xHYPE** | product page: "Coming soon"; no address | — | — | — | — | — | **Blocked** |
| **JitoSOL** | mint `J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn`, pool `Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb` (Jito Technical FAQ) | SPL balance; value = pool `total_lamports / pool_token_supply` (no oracle) | delayed unstake ≤ 1 epoch (≤ 2 days), 0.1% fee; instant via Jupiter | none | 4% of rewards management fee; live APY not needed for weight | wallet balance only | **Valid** |
| **Spot JLP** | mint `27G8MtK7VtTcCHkpASjSDdkWWYfoqT6ggEuKidVJidD4`, pool `5BUwFW4nRbftYTDMbgxykoFWqWHPzahFSNAaaaJtVKsq` (docs.jup.ag) | SPL balance; value = virtual price (AUM/supply) in USD ⇒ needs SOL/USD reference | burn at virtual price, weightage fee (10 bps base non-stable); swap instant; AUM cap blocks mint only | none; holders bear trader PnL | 75% of perps fees accrue to price; do not call price change "fee yield" | wallet balance only | **Valid**; USD→SOL reference is an added dependency |
| Leveraged / Multiply / Delta-Neutral / queued JLP | — | — | 3-day redemption on the deprecated vault | yes | — | — | **Excluded** (checklist) |
| **SKR (wallet)** | mint `SKRbvo6Gf7GondiT3BbTfuRDPqLWei4j2Qy2NPGZhW3` (docs.solanamobile.com) | SPL balance | swap; liquidity risk | none | — | locked/vesting allocations excluded ("only unlocked SKR can be staked") | **Valid**; SKR/SOL price source unresolved |
| **SKR (staked)** | staking program `SKRskrmtL83pcL4YqLWt6iPefDqwXQWHSw9S9vz94BZ`, vault `8isViKbwhuhFhsv2t8vaFL74pKCqaFPQXo1KkeQwZbB8` | tokens leave the wallet into the vault; needs the per-user stake account layout from the program IDL | 2-day epoch + 48 h cooldown, then separate withdraw | none | 26.5 / 25.8% shown on stake.solanamobile.com; not used | pending-unstake excluded | **Valid only after IDL-based stake-account verification** |

Double-counting prevention across wallets and time (§12 below): interval weight = `min(balance at interval start, balance at interval end)` with finalized checkpoints at every accrual sync. A token moved A→B inside an interval counts for neither; A→B→A counts once for A. Same external capital cannot back two wallets in one interval. Borrowed tokens held across a full interval are indistinguishable from owned ones; the borrower pays the borrow cost and bears the leverage — no Cherry solvency exposure.

---

## 12. APR dilution table

Opening Storm `E = 8.373287670 SOL / 24 h`. `APR = 305,625 / H` %.

| Opening APR falls below | `H` (SOL matched weight) | max wallets (2.5) | threshold wallets (1.5) |
|---:|---:|---:|---:|
| 24,450% (floor) | 12.5 | 5 | 8.3 |
| 10,000% | 30.5625 | 12.2 | 20.4 |
| 5,000% | 61.125 | 24.5 | 40.8 |
| 2,500% | 122.25 | 48.9 | 81.5 |
| 1,200% (Genesis floor) | 254.6875 | 101.9 | 169.8 |
| 600% (Early Grove floor) | 509.375 | 203.8 | 339.6 |
| 250% (Growth floor) | 1,222.5 | 489 | 815 |

Storm → mature transition: the sequence ends at `T0 + 12 d` by schedule. Label the Fruit weight-unit APR with the existing `YieldPhase` floors (code: 1,200 / 600 / 250%). Once `H ≥ 254.69` the Storm itself opens below Genesis; once `H ≥ 1,222.5` every tranche in the model is Mature. From then on the only honest product is repeat 48-hour Harvest epochs funded from `Future_reserve` and, later, from realised revenue `Y` (`APR ≤ 36,500·Y_daily/H`).

---

## 13. 5 / 25 / 100 / 500 / 1,000 wallet stress test

Storm epoch, `E = 8.373287670`, 24 h. Two bounds per row: all wallets at threshold (`w = 1.5`, full coverage) and all at max (`w = 2.5`). Real cohorts lie between.

| n | mix | Cw = matched external = recognised Cherry | H | period return | APR | threshold wallet payout | max wallet payout | dilution vs previous n | undistributed |
|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 5 | thr | 7.5 | 12.5 | 66.9863% | 24,450% | 1.004795 | 1.674658 | — | 3.349315 |
| 5 | max | 12.5 | 12.5 | 66.9863% | 24,450% | 1.004795 | 1.674658 | — | 0 |
| 25 | thr | 37.5 | 37.5 | 22.3288% | 8,150% | 0.334932 | 0.558219 | ×0.333 | 0 |
| 25 | max | 62.5 | 62.5 | 13.3973% | 4,890% | 0.200959 | 0.334932 | ×0.200 | 0 |
| 100 | thr | 150 | 150 | 5.5822% | 2,037.5% | 0.083733 | 0.139555 | ×0.250 | 0 |
| 100 | max | 250 | 250 | 3.3493% | 1,222.5% | 0.050240 | 0.083733 | ×0.250 | 0 |
| 500 | thr | 750 | 750 | 1.1164% | 407.5% | 0.016747 | 0.027911 | ×0.200 | 0 |
| 500 | max | 1,250 | 1,250 | 0.6699% | 244.5% | 0.010048 | 0.016747 | ×0.200 | 0 |
| 1,000 | thr | 1,500 | 1,500 | 0.5582% | 203.75% | 0.008373 | 0.013955 | ×0.500 | 0 |
| 1,000 | max | 2,500 | 2,500 | 0.3349% | 122.25% | 0.005024 | 0.008373 | ×0.500 | 0 |

Loyalty / lock reserve per row: `B_loy = b·f·E` and `B_lock` are independent of n (§7, §9); with F = 10 both are 0. Referral burden per row: `c·φ·volume`; at φ = 3% on fresh capital of n × 1.5 (thr) or n × 2.5 (max) SOL ⇒ 0.045·c·n or 0.075·c·n SOL, from FEE, unresolved `c`.

Mid-epoch participation shock (from the doc, verified): if `Cw` doubles after hour 1 an incumbent receives 52.0833% of its unchanged-share projection; 10× ⇒ 13.75%. Past accrual is unchanged; total stays ≤ E.

---

## 14. Whale + Sybil stress test

Whale with `k` max wallets (each 250 seeds + 2.5 SOL external; capital `k × 5.075` SOL at the 3% rehearsal fee) against `N` honest max wallets, Storm epoch:

| N honest | k | whale share of E | whale 24 h take (SOL) | return on whale capital |
|---:|---:|---:|---:|---:|
| 0 | 1 | 20.00% | 1.6747 | 33.0% |
| 0 | 5 | 100.00% | 8.3733 | 33.0% |
| 0 | 20 | 100.00% | 8.3733 | 8.25% |
| 5 | 1 | 16.67% | 1.3955 | 27.5% |
| 5 | 5 | 50.00% | 4.1866 | 16.5% |
| 25 | 5 | 16.67% | 1.3955 | 5.5% |
| 25 | 20 | 44.44% | 3.7215 | 3.67% |
| 100 | 5 | 4.76% | 0.3987 | 1.57% |
| 100 | 20 | 16.67% | 1.3955 | 1.37% |
| 500 | 20 | 3.85% | 0.3220 | 0.32% |
| 1,000 | 20 | 1.96% | 0.1642 | 0.16% |

Whale with 50% of nominal Cherry capital in one wallet (doc case): `w = 2.5`, 6.4935% of E. Split into 24 threshold wallets with 36 SOL of matching external: 50% of E (doc §11), at the same per-SOL return as everyone else.

Per attack:

| Attack | Solvent? | Unfair allocation? | Mitigating rule | Not solvable without identity / history |
|---|---|---|---|---|
| One large wallet | yes | no; share ≤ 2.5/H, ≤ 20% | cap + floor | — |
| Owner splits into many wallets | yes | share grows linearly in k; per-SOL return unchanged | each wallet needs T seeds + matching X; floor caps the ≤ 5-wallet case at 20% each | beneficial-owner linkage |
| Same external capital across wallets over time | yes | yes, if snapshots are point-in-time | `min(start, end)` per interval, finalized checkpoints at every sync | full transfer history for exact intra-interval min |
| Referral farming across linked wallets | yes (FEE only) | fee rebate `c·φ` | one immutable referrer, no self-referral, pay only on received fees | — |
| Chat / GM / GN spam | yes | none if activity is a gate, not a multiplier | rate limits; never weight | — |
| Late entry before a checkpoint | yes | no: accrual is continuous from entry (`∫ r·w/H dt`) | time-weighted accrual | — |
| Enter/exit carousel | yes | no extra yield; loses lock bonus | cliff-vested lock; base accrual only while weight > 0 | — |
| Borrowed external capital | yes | none per SOL; borrower bears cost | interval-min rule; Safe Mode | cannot distinguish owned from borrowed |
| Fake mint / wrong program / decimals | yes if verification fails closed | — | pinned mints, program IDs, decimals read from chain | — |
| Stale price | yes (fixed epoch reference) | weight can over-reference a collapsed asset for one epoch | fixed timestamped reference per epoch; Safe Mode EXTREME re-references | — |
| Fruit liquidity disappears mid-epoch | yes | none | Safe Mode; no Cherry custody | user's principal risk only |
| Oracle / indexer outage | yes: reserve the interval's maximum liability in U | none; new eligibility pauses, old L claimable | disputed-interval reserve = `r · Δt · w_max/H` per affected wallet | — |

---

## 15. Solvency / failure conditions

Invariant `V ≥ L + U + Z`, verified against the flows:

- Funding: `V += a`, `Z += a`. No duplicate spendable accounting. ✓
- Epoch creation: `Z −= E`, `U += E`. No SOL moves. Bonus pools (`B_loy`, `B_lock`) are committed the same way. ✓
- Accrual: `L += δ`, `U −= δ`, `δ ≤ r·Δt·Cw/H`. Elapsed unused capacity `r·Δt·(1 − Cw/H)`: `U → Z`, never to other wallets. ✓
- Claim: `V −= c`, `L −= c` atomically; `c ≤` wallet's accrued-unclaimed. ✓
- Epoch end: only unaccrued `U → Z`. `L` is never swept into a new campaign. ✓
- Rounding: floor every wallet's interval entitlement; Σ integer amounts ≤ interval budget. Matches `accumulator.rs`. ✓
- The V1/V2 on-chain ledger (`funded − claimed − compounded`) is `L + U + Z + dust`, an upper bound; the Fruit vault needs its own three counters.

Failure conditions (each breaks solvency or the promise):

1. Storm tranche committed while `F < R` or while Tree base runway < 12 days.
2. Any bonus pool computed as a multiplier on `E` rather than committed from `Z`.
3. Lock bonuses vested from a future epoch's budget.
4. Fruit weight taken from wallet snapshots without an interval-min rule (double-count ⇒ unfair, still solvent only because Σ ≤ E).
5. Displaying `24,450%` or any floor-case APR as a live rate.
6. Adding external yield or leverage to a Cherry APR.
7. Referral paid from REWARD or CAPITAL.
8. Reference price not fixed per epoch (weight manipulable intra-epoch).
9. Continuation epochs "announced" without `R` already in `U`.
10. Genesis cap `C` chosen > `M` or < `T` (breaks C2/§4 logic; irreversible for the program version).

---

## 16. Inputs that cannot yet be solved

| Variable | Needed for | Equation waiting on it |
|---|---|---|
| Tree base schedule (`fund_rewards` amount, duration) and `B_base` | Day-1 incentive size, `F = B − B_base`, runway ≥ 12 d check | `APR_tree = reward_rate·31,536,000·10⁴·10⁴ / (total_shares·price·(10⁴+fee))` |
| Production `rig_price_lamports`, `purchase_fee_bps` | every SOL figure (0.01 / 3% are rehearsal values) | all |
| Net Fruit budget `F` | Storm, Future_reserve, whether any boost is fundable | `Storm = min(max(F−R,0), 8.561643836)` |
| `.skr` eligible count and participation rate | `Cw(FRUIT_T0)`, opening APR, concentration | `APR_open = 305,625/H` |
| Seed-size distribution at enrolment | `N_q(T)`, expected `Cw` | `N × Pr(S ≥ T ∧ X > 0)` |
| Required protocol margin `m` | referral `c` | `c_max = 1 − m/φ` |
| Bonus budgets `B_loy`, `B_lock` | boost and lock magnitudes | `B ≤ Future_reserve` |
| SKR/SOL and USD/SOL reference-price source | SKR and JLP weight | `X_i` in SOL |
| SKR staking per-user account layout (IDL) | staked-SKR verification | — |
| xBTC mint (Hylo lists TBD); xHYPE status (Coming soon) | admission | — |
| Hylo live fee-table parameters, oracle staleness bounds, CR/NAV history | Safe-Mode overlay thresholds | — |
| Indexer trust model / finalized history availability | exact intra-interval min vs endpoint min | — |
| Epoch cadence after day 12 and realised revenue `Y` | mature rate | `APR ≤ 36,500·Y_daily/H` |

---

## 17. Final recommended tokenomics table

| Parameter | Value / rule | Source |
|---|---|---|
| `P` | 0.01 SOL/seed (rehearsal; production undecided) | code config |
| Fruit cap `M` | 250 seeds, reward-power cap (structure B), permanent | §3 |
| Threshold `T` | `ceil(3M/5)` = 150 | §6 |
| Day-1 `.skr` fresh-purchase cap `C` | 150 (= T), on-chain, 24 h, compounds excluded | §4 |
| `FRUIT_T0` | `T0 + 86,400 s` (public open) | C1 |
| Fruit offsets from `FRUIT_T0` | Storm 0–1 d, Harvest 1–3 d, Superfruit 3–6 d, Season 6–11 d | doc §8 + 1 d |
| Tranches | Harvest 684,931,507; Superfruit 513,698,631; Season 428,082,192 lamports; Storm `min(max(F−R,0), 8,561,643,836)` | verified |
| Day-1 Fruit spend | 0 | §4 |
| Weight | `w = 1[S≥T]·min(P·min(S,M), X)`; one Fruit per wallet per epoch; multipliers 1.00 | doc §5 |
| Denominator | `H = max(Σw, 12.5)`; per-wallet ≤ E/5; total ≤ E | verified |
| Displayed APR | `36,500·r/H` from live H; floor-case figures never shown | §12 |
| Loyalty | `β = min(1, d/11)`; separate pool `B_loy ≤ Future_reserve`; floored denominator | §7 |
| Lock | ladder 1/2/3/5 d; bonus ∝ weight-days from `B_lock`; cliff vest; early exit forfeits unvested only; EXTREME waives | §9 |
| Referral | on-chain 1.05× base weight kept; plus `c·φ·(fresh+compound)` from FEE; `c` fixed; `c ≤ 1 − m/φ` | §8 |
| Safe Mode | NORMAL ≥ 135% CR; WARM 120–135%; HOT 100–120%; EXTREME < 100% or route blocked; overlays for staleness and exit route | §10 |
| Fruit universe now | xSOL, JitoSOL, spot JLP, wallet SKR (staked SKR after IDL verification); xBTC and xHYPE blocked; leveraged/queued JLP excluded | §11 |
| Ownership rule | interval weight = `min(start, end)` at finalized checkpoints; disputed intervals reserved in U | §11, §14 |
| Solvency | `V ≥ L + U + Z`; all pools committed from Z; L never swept | §15 |
| Storm → mature | phase labels by existing floors on the Fruit rate; after day 12 only repeat Harvest epochs from Future_reserve or realised revenue | §12 |

---

## 18. Exact proposed edits to `CHERRY_TREE_FRUIT_TOKENOMICS.md`

Line numbers refer to the uploaded 562-line file.

| # | Location | Edit |
|---|---|---|
| E1 | §8 lines 292–296 and §14 line 476 | Replace offsets `0; 86,400; 259,200; 518,400` with `86,400; 172,800; 345,600; 604,800` seconds from `T0`, and "End T0+11 days" with "End T0+12 days". Add: "`FRUIT_T0 = T0 + 86,400 s`, the public opening. Day 1 is `.skr` Tree-only; Fruit reserve spend on Day 1 is 0." |
| E2 | §2 line 29 and §14 lines 461–464 | Split the two caps: "Genesis fresh-purchase cap `C = 150` (on-chain, 24 h, compounds excluded). Fruit reward-power cap `M = 250` (permanent)." Remove "Fresh purchase cap … from actual T0" from the `M` row. |
| E3 | §1 line 12, §9 table, §10 header | After every 24,450% figure add "(holds only while `Cw ≤ 12.5`, i.e. ≤ 5 max or ≤ 8 threshold wallets)". Add the closed form `APR_open = 305,625 / H` and the §12 crossing table. Add: "User-facing displays show the live-H rate only." |
| E4 | §5 after line 158 | Add the ownership rule: "Interval weight uses `min(X at interval start, X at interval end)` from finalized checkpoints at every accrual sync; a token moved between wallets inside an interval counts for neither. Disputed intervals reserve `r·Δt·w_max/H` in U." |
| E5 | §3 lines 65–67 and §14 lines 483–484 | Replace the literal 432,000 s "including liquidity failure, no exceptions" rule with "normal-path exit ≤ 432,000 s per official documentation, no custody, on-chain verifiable, Safe Mode for leveraged assets". Replace "Current approved external whitelist: Empty" with the §11 verdicts: xSOL, JitoSOL, spot JLP, wallet SKR valid; staked SKR pending IDL verification; xBTC and xHYPE blocked. Mark this as a founder requirement change. |
| E6 | §3 line 76 | Add Jito pool account `Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb`, delayed-unstake fee 0.1%, instant path via Jupiter, valuation `total_lamports / pool_token_supply`. |
| E7 | §3 lines 97–100 | Keep xBTC/xHYPE disabled; cite that the official address page lists xBTC as `[TBD]` and the product page lists xHYPE as "Coming soon" as of 2026-09-26. |
| E8 | §14 line 470 | Replace "Referral weight 1.00; no incremental Fruit referral emission" with "Fruit referral weight 1.00. Base referral weight 1.05× (on-chain, redistribution). Referrer cut `c·φ` on the referred wallet's fresh-purchase and compound fees, paid from FEE only; `c ≤ 1 − m/φ`, `m` unresolved." |
| E9 | new §5a after line 197 | Insert §7 of this solve (loyalty equation, `d_cap = 11`, `B_loy ≤ Future_reserve`, per-wallet ≤ `B_loy/5`, funding table `B = b·f·E`). |
| E10 | new §5b | Insert §9 (lock ladder by weight-days, `B_lock` committed at Season open, cliff vesting, forfeiture to Z, three necessary-treatment cases). |
| E11 | new §13a | Insert §10 (Safe Mode states mapped to Hylo zones, `L = CR/(CR−1)`, overlays, actions table, calibration data list). |
| E12 | §8 line 297 | Replace "No Cherry Fruit claim lock" with "No mandatory Fruit lock. A voluntary 1/2/3/5-day lock exists (§5b) on Fruit weight; it never affects base accrual or principal." |
| E13 | §8 after line 335 | Add: "Storm tranche is committed only if `F ≥ R`, Tree base funded runway ≥ 12 days remains, and `V ≥ L+U+Z` is verified on the Fruit vault. State the front-loading: 83.7% of F = 10 pays in the Storm; `Future_reserve` funds `floor(Future/0.684931507)` repeat Harvest epochs." |
| E14 | §12 line 413 | Add: "Bonus pools (`B_loy`, `B_lock`) are committed from Z exactly like E. A lock bonus is never funded from a later epoch." |
| E15 | §13 table | Add rows: "Owner splits into k wallets" (§14 table), "Chat/GM/GN spam" (gate only), "Late entry before checkpoint" (continuous accrual), "Oracle/indexer outage" (reserve in U). |
| E16 | §14 line 492 | Add: "Storm→mature: after `FRUIT_T0 + 11 d` only repeat Harvest epochs from `Future_reserve` or realised revenue `Y`; label the Fruit weight-unit rate with the existing phase floors." |
| E17 | §15 line 500 | Add to PRE-OPS: "`main` is V1; the genesis gate, claim penalty and referral weight this document assumes exist only on `mainnet/rc-hardening`. No Fruit vault, epoch or external-weight code exists on any branch." |
| E18 | §15 GATES | Add gate 8: "Choose `C` (Day-1 cap) explicitly, `T ≤ C ≤ M`; recommended `C = T = 150`. Irreversible per program version." |
