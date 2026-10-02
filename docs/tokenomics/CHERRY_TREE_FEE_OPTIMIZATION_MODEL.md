# THE CHERRY TREE — Protocol Fee Optimization Model

Date: 2026-09-26. Status: **APPROVED / FINAL** (founder approval 2026-09-26). Adopted into the frozen architecture as §7a (`docs/CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md`, tests 78–91) and recorded in closeout §12. **Nothing is implemented yet**; the fee implementation is its own pass after the Fruit-ledger foundation. The model was built on the V2 code at `claude/cherry-tree-pass1-cap-skr` (`c7241df`) and the frozen architecture (`3036392`, which includes the single 5-day lock correction).

Objective: the highest protocol revenue that still leaves users a clear reason to participate. The model never touches principal, already-earned rewards or vested rewards, never compromises `V >= L + U + Z`, and adds no hidden fees.

**Founder decisions (final).**
- Purchase fee 10%; ceiling stays 10%.
- Compound fee 10%, as its own setting, with `compound_fee <= purchase_fee`.
- 72-hour fee-change timelock.
- Referral: 20% referrer / 80% Protocol Treasury, one level, retained margin 8%.
- Early-claim fee 30 / 20 / 10 / 0, split 60% treasury / 40% grove, disclosed before purchase as an early-claim fee.
- Mandatory top-up warning; the anchor-reset behaviour itself is kept.
- Single 5-day lock with no additional exit fee.
- Released lock forfeits: 80% treasury / 20% next-Season reserve.
- CAPITAL unchanged.
- Protocol fees never fund rewards automatically.
- `.skr` binding rent ≈ 0.00145 SOL.

Where this document says "recommended", read "approved". Where it names a founder gate, the gate is resolved as stated in §7 and §9.

Revenue destination: the **Protocol Treasury / Protocol Sustainability Fund**. On-chain this is `config.fee_recipient`, which must be a treasury multisig (see §12).

---

## 1. Complete money-flow map

| # | Flow | From → To | Class | Treasury revenue? |
|---|---|---|---|---|
| 1 | Seed principal on `buy_rigs` (`count × price`) | user → CAPITAL | **User principal**: non-redeemable and locked in this program version. No instruction pays out of CAPITAL | No |
| 2 | Purchase fee (`capital × fee_bps / 10,000`, charged on top of principal) | user → FEE | **Protocol revenue** (gross) | Yes, net of referral reservations |
| 3 | Compound, principal portion | REWARD → CAPITAL | A reward liability becomes locked user principal | No |
| 4 | Compound fee | REWARD → FEE | **Protocol revenue** (gross) | Yes, net of referral reservations |
| 5 | `fund_rewards` | reward funder → REWARD | **Reward reserve**. It becomes a **reward liability** (L) as it accrues | No |
| 6 | Claim, net amount | REWARD → user | Settlement of a reward liability | No |
| 7 | Early-claim penalty (current rule) | stays in REWARD and is re-credited to other planters, or to `undistributed_dust` if there are none | **Forfeiture**, redistributed as others' reward liability | No, today |
| 8 | `undistributed_dust` | stays in REWARD, attributed to no one, no withdrawal path | Reward reserve (stranded) | No |
| 9 | `withdraw_fees` | FEE → `config.fee_recipient` | Treasury transfer of net revenue | Already counted in 2 and 4 |
| 10 | Referral commission (architecture §7; **not implemented**, so `c = 0` today) | reserved in FEE → referrer | **Referral liability**, paid out of flows 2 and 4 | Deducted from gross |
| 11 | Referral weight 1.05× | redistribution inside emission | Neither revenue nor cost; reshares funded emission | No |
| 12 | Account rent: Miner PDA ≈ 0.00208 SOL and `.skr` binding ≈ 0.00145 SOL (user-paid); Config and vault rent (authority-paid); transaction fees | user or authority → the accounts / validators | **Network and rent cost**. No close path, so not recoverable | No |
| 13 | Fruit core, loyalty and lock budgets (F, J, B_lock) | treasury or sponsor → separate Fruit ledger vaults (Z → U → L) | Reward reserve and reward liability | No |
| 14 | Lock early-exit forfeit (unvested conditional bonus) | U → Z at pool close | **Forfeiture**, returning to uncommitted reserve | Only through a Z release (§8) |
| 15 | Z, the uncommitted Fruit/bonus reserve | Fruit ledger vault | **Uncommitted treasury capital**. Withdrawable only if L and U are untouched (test 75) | It is the treasury's own capital coming back, not new revenue |
| 16 | External assets (xSOL, JitoSOL, JLP, …) | never in Cherry custody | Not a flow | No |

This ties out with no double counting. Every lamport a user spends is split into principal (1), fee (2) and rent (12). Every reward lamport ends in exactly one of: claimed (6), compounded (3 + 4), still owed (L), dust (8), or reserve (5 or 15).

Two important facts:

- **User principal is non-redeemable.** For a user, the whole purchase — principal plus fee — is sunk cost. Users profit only if the rewards they receive exceed that cost.
- **Rewards are funded from outside.** The fee is a small markup on top of the sunk principal; it is not what decides whether users profit. Funding is.

## 2. True protocol-revenue sources

1. Purchase fee.
2. Compound fee.
3. Early-claim penalty, **only if** the founder approves Model B (§7). Today it is not revenue.
4. Lock forfeits, only as a release of the treasury's own Z capital. This recovers unspent bonus budget; it is not user money.

Not revenue: principal (CAPITAL), any reward reserve or liability, reserved referral commissions, dust, and rent.

## 3. Current code limits

| Item | Current code | Consequence |
|---|---|---|
| Fee ceiling | `MAX_PURCHASE_FEE_BPS = 1000` (10%) in `pricing.rs` | Anything above 10% needs a code change plus requalification |
| Fee applied | One `purchase_fee_bps`, used by both `buy_rigs` and `compound` | No separate compound fee today |
| Fee changes | `set_pricing` by the authority, effective immediately, no timelock | A trust gap at higher fee levels (§12) |
| Penalty schedule | Hard-coded 25% / 15% / 5% / 0% at `<24h` / `<48h` / `<72h` after the last purchase | Changing it is a code change |
| Penalty destination | Hard-coded redistribution (`redistribute_excluding`) | Routing any of it to the treasury is a code change |
| Penalty anchor | Reset by every fresh purchase, applied to **all** pending rewards | A top-up re-exposes old accrued rewards to the penalty. Higher penalties make this harsher (§7) |
| Referral commission | Not implemented; no `reserved_commissions` | `c = 0` until implemented |
| Fee withdrawal | FEE vault → `config.fee_recipient` only | That recipient must be the treasury multisig |
| CAPITAL | No withdrawal instruction exists | Locked in this version (§6) |
| Lock | Not implemented | Design only |
| Deployment state | Not deployed (initial-deployment case) | Config layout changes cost nothing now: no migration needed |

## 4. Maximum sustainable purchase fee → **10%**

Effect on the user (price `P_ref = 0.01 SOL`):

| Fee | All-in cost per seed | Fee share of spend | Headline APR factor | Rewards needed to break even (× principal) | Break-even at 3,000% gross APR | Return on all-in cost after 1 / 3 / 5 / 12 days at 3,000% |
|---:|---:|---:|---:|---:|---:|---|
| 5% | 0.01050 | 4.76% | 0.952 | 1.050 | 12.78 d | −92.2 / −76.5 / −60.9 / −6.1% |
| 7.5% | 0.01075 | 6.98% | 0.930 | 1.075 | 13.08 d | −92.4 / −77.1 / −61.8 / −8.3% |
| **10%** | **0.01100** | **9.09%** | **0.909** | **1.100** | **13.38 d** | −92.5 / −77.6 / −62.6 / −10.3% |
| 12.5% | 0.01125 | 11.11% | 0.889 | 1.125 | 13.69 d | −92.7 / −78.1 / −63.5 / −12.3% |
| 15% | 0.01150 | 13.04% | 0.870 | 1.150 | 13.99 d | −92.9 / −78.6 / −64.3 / −14.2% |

The general break-even time is `t* = 365 × (1 + fee) / r`, where r is the gross APR on principal. Each fee point adds about 1% to break-even time.

The return column is deliberately base-only and assumes seeds are never redeemed. Even at 3,000% APR, base emissions alone do not repay a seed within the 12-day program. User upside depends on the funded emission level and on Fruit, not on the fee.

**Demand model.** Users respond to expected profit, not to price. The model sets participation proportional to `((G/(1+fee) − 1)/(G − 1))^k`, where G is the gross reward multiple on principal over the user's horizon and k is how sensitive participation is to profit. It then finds the revenue-maximizing fee:

| G (reward multiple) | k = 0.5 | k = 1 | k = 2 | Volume lost going from 10% to 15% (k = 1) |
|---:|---:|---:|---:|---:|
| 1.25 (marginal economics) | 16.3% | **11.8%** | **7.5%** | 36% |
| 1.5 | 31.9% | 22.5% | 13.7% | 16% |
| 2.0 | ~40% | ~40% | 23.6% | 10% |
| 3.0 | ~40% | ~40% | 37.2% | 7% |

How to read this:

- Very high fees are revenue-optimal only while economics are extraordinary (G ≥ 1.5), which is the Genesis window.
- A fee is fixed for the whole lifecycle. In the marginal-economics regime (G ≈ 1.25), which is where the protocol sits once APR decays, the optimum is 7.5–12%.
- At 1,000 wallets × 150 seeds with G = 1.25, net revenue is 155.7 / 188.9 / **204.4** / 203.7 / 188.2 SOL at 5 / 7.5 / **10** / 12.5 / 15%. Revenue is flat from 10% to 12.5% and falls after that.
- 10% is the highest fee that is at or near the optimum in the fragile regime and fits the current code ceiling. It is also where a "dev fee" stops reading as normal to crypto users.
- 12.5–15% gains revenue only if economics stay strong, and loses it when they don't. The upside is not worth the downside.

**Recommendation:** 10% (1,000 bps), at the existing ceiling. Do **not** raise the ceiling; keeping it at 10% is a trust feature. Compared with the current 3% devnet config, this is 3.3× the revenue per unit of volume.

## 5. Maximum sustainable compound fee → **10%, the same as the purchase fee**

A user compounds only if the new seed pays back its all-in cost within the remaining funded runway, net of what claiming would give instead:

```text
compound preferred  ⇔  payback_rem / (1 + fee_c)  >  (1 − π(age)) / (1 + fee_p)   [when the user would re-buy]
                    ⇔  payback_rem > 1 + fee_c                                     [when the user would otherwise hold SOL]
payback_rem = r × T_remaining / 365
```

**Arbitrage bound.** Once the penalty window is over (π = 0), a compound fee above the purchase fee makes "claim, then buy fresh" strictly cheaper than compounding:

- The only friction is that the fresh buy resets the penalty anchor. That costs a patient claimer almost nothing.
- So `fee_c > fee_p` does not raise revenue from sophisticated users. They re-route through `buy_rigs` and pay `fee_p` anyway.
- It only charges more to users who don't notice. That is a trap, and it violates the no-predatory requirement.

| Compound fee (purchase fee 10%) | Compound value per SOL of reward | Claim-and-rebuy value (π = 0) | Verdict |
|---:|---:|---:|---|
| 5% | 0.952 | 0.909 | Leaves revenue on the table |
| 7.5% | 0.930 | 0.909 | Leaves revenue on the table |
| **10%** | **0.909** | **0.909** | **Arbitrage-free, maximum honest rate** |
| 12.5% | 0.889 | 0.909 | Claim-and-rebuy wins |
| 15% | 0.870 | 0.909 | Claim-and-rebuy wins |
| 20% | 0.833 | 0.909 | Claim-and-rebuy wins by 9% |

Compounding remains attractive because:

- it avoids the claim penalty inside the window (compound never moves the anchor);
- it is how sub-150-seed wallets reach the Fruit qualification threshold.

Revenue is 9.09% of every compounded lamport. Zero-seed compounds (at the cap) charge nothing, as already implemented.

**Separate constants?** Add a separate `compound_fee_bps` Config field for flexibility. It costs nothing before the initial deployment. Enforce `compound_fee_bps <= purchase_fee_bps` in code and set both to 1,000 bps.

## 6. CAPITAL vault

- **What the code permits:** receiving lamports only. No instruction pays out of CAPITAL.
- **Size:** the largest pool of SOL in the protocol, about 90.9% of user spend at a 10% fee.
- **Classification:** user principal for non-redeemable seeds. It is **not** protocol revenue and not treasury capital.
- **Disclosure:** "locked in this program version". It is not "permanently locked" while an upgrade authority exists.
- **Must stay protected:** all of it. Any path out of CAPITAL — to REWARD, to operations, to salaries, to market-making — turns the product into deposit-funded payouts or insider extraction (`docs/backlog/CAPITAL_FUTURE_POLICY.md`, forbidden-use matrix).

Recommendation:

- No change, and no withdrawal path.
- The fee is the only legitimate lever that shifts part of user spend to the treasury, and it is fully disclosed at purchase. Raising it from 3% to 10% moves the treasury share of spend from 2.9% to 9.1%.
- Eventually revoke the upgrade authority, which makes "locked" provable.

## 7. Early-claim penalty

The penalty applies only to claims within 72 hours of the wallet's last fresh purchase. Compounding and waiting both avoid it entirely, so the rational user rarely pays it. It mostly lands on users who need liquidity early.

**Schedule.**

| Schedule | 24h | 48h | 72h | Assessment |
|---|---:|---:|---:|---|
| Current | 25% | 15% | 5% | The 48–72h step is too weak to matter |
| **Recommended** | **30%** | **20%** | **10%** | Stronger staying incentive across all 3 days; no single step is punitive |
| 40 / 25 / 10 | 40% | 25% | 10% | 40% on the first day reads as punitive |
| 50 / 30 / 15 | 50% | 30% | 15% | Predatory optics; heavy exposure through the anchor reset (below) |

The absolute cost is bounded: within 24 hours of a purchase, a wallet has accrued at most one day of emission. At 3,000% APR that is 8.2% of principal, so a 30% penalty costs at most about 2.5% of principal.

**Anchor-reset risk.** A top-up puts **all** pending rewards, including older ones, back under the penalty. At harsher schedules this is the main predatory risk. Mitigation needed with any increase: the buy panel must warn "claim before topping up". The alternative fix (penalize only rewards accrued since the last purchase) is a model change and is not proposed here.

**Destination.**

| Model | Revenue | Staying incentive (for the claimer) | Perception | Grove economics |
|---|---|---|---|---|
| A. 100% redistributed to the grove | 0 | The same in both models: the claimer loses π either way | "Your patience pays the grove" | Stayers receive the penalty |
| **B. 60% treasury / 40% grove** | 60% of penalties | Same | A disclosed "early-claim fee", majority to treasury, with a visible grove share | Stayers keep a meaningful bonus |
| B. 70 / 30 | 70% | Same | Weaker grove story | Smaller bonus |
| B. 80 / 20 | 80% | Same | Reads as an exit tax | Grove share is token |

**Recommendation:** Model B, 60% treasury / 40% grove, on the 30 / 20 / 10 / 0 schedule.

**Founder gate — RESOLVED:** Model B approved; the treasury share is classified and disclosed before purchase as an **early-claim fee** (architecture §7a item 5). Original note: Model B turns part of a user's earned-but-unclaimed base rewards into protocol revenue. That is compatible with "never confiscate earned rewards" only if the penalty is classified and disclosed, before purchase, as an early-claim fee.

- It also amends the frozen architecture, where base penalty rules are unchanged.
- If you do not accept that classification, keep Model A. The stronger schedule alone still improves the staying incentive.

**Funding note.** Penalty lamports are funded emission. If a third-party sponsor funds the rewards, routing penalties to the treasury diverts sponsor money, so sponsors must be told in advance.

## 8. Five-day lock early exit

Under the frozen rule, the lock is noncustodial with **cliff vesting**. A voluntary exit before `t_s + 5 days` forfeits **100%** of the unvested conditional bonus for the lost units. Everything else stays protected: external principal, seed principal, core Fruit rewards, vested bonuses, and all of L.

**No time-decaying penalty.** Under the cliff, the forfeit is the entire unvested award at every moment before maturity. The staying incentive at time t is the full award A_i, and no decaying schedule gives more. A decaying schedule would require partial vesting, which the founder rules keep out (cliff vesting is preserved). An extra exit fee could only be taken from core rewards (forbidden: earned) or from external principal (impossible: noncustodial). So the answer is no extra fee and no decay.

**Destination of forfeits.** The exiting user loses the same amount whatever the destination, so the split does not affect incentives. Forfeits cannot go to the remaining lockers: the frozen fixed-denominator rule forbids renormalizing awards, which blocks collusive forfeiture into another wallet. The only real choice is treasury versus the reserve for future Seasons. Both are protocol-owned; B_lock came from treasury or sponsor funds in the first place.

**Recommendation:** at pool close, forfeits return to Z (unchanged frozen rule). Then **80%** is released from Z to the treasury (a permitted reserve withdrawal, test 75) and **20%** stays in Z to seed the next lock Season.

This is a recovery of unspent budget, not new revenue from users: the lock program's actual cost is `B_lock × (1 − forfeit rate)`.

## 9. Treasury accounting and splits

```text
GROSS PROTOCOL FEES            = purchase fees + compound fees                  (FEE vault receipts)
 − REFERRAL OBLIGATIONS         = reserved_commissions (unpaid + paid)
 − OTHER RESERVED FEE LIABILITIES (none today)
 = NET PROTOCOL FEE REVENUE     → withdrawable to the Protocol Treasury
 + early-claim fee, treasury share (Model B only; moved REWARD → FEE and counted separately)
 = NET PROTOCOL REVENUE → PROTOCOL TREASURY

Shown separately, never as revenue: lock forfeit releases from Z (the treasury's own capital coming back).
Never counted: CAPITAL, REWARD balance, L, U (including protected U), Z before release, dust, unpaid referral balances.
```

`withdraw_fees` must enforce `amount <= distributable(FEE) − reserved_commissions` (architecture §7; not yet implemented).

| Flow | Treasury | Other |
|---|---:|---:|
| Purchase fee | 80% (net of referral) | 20% to the referrer, **only on referred volume** |
| Compound fee | 80% (net of referral) | 20% to the referrer, **only on referred volume** |
| Early-claim penalty (Model B) | 60% | 40% to the grove |
| Lock forfeit (via Z) | 80% | 20% stays in Z for the next Season |
| CAPITAL | 0% | 100% protected |

**Reward funding from the treasury (founder decision).** The ops policy (`docs/backlog/REWARD_RUNWAY_OPS.md` §2) currently forbids funding REWARD from FEE-vault proceeds, including round trips. The reason: new buyers' fees would then pay earlier buyers, which is deposit-funded economics. The brief's "future reward funding where explicitly allocated" conflicts with that rule.

- **Founder decision (final):** the prohibition is kept. Protocol fees never fund rewards automatically; any treasury-to-reward allocation must be a separate, explicit funded transfer and commitment (architecture §7a item 10).
- Recommendation: keep the prohibition for base rewards.
- The Sustainability Fund pays operations, security, infrastructure and growth.
- Rewards stay funded by disclosed external sources.

## 10. Revenue scenarios (SOL)

Assumptions (explicit; change any of them and the tables scale linearly):

- Fees 10% / 10%.
- 40% of fee volume is referred, with a 20% commission (so 8% of gross fees go to referrers).
- Base emission funded at `e ×` planted principal over the program.
- Wallets compound 50% of their rewards at 50 seeds and 40% at 150 seeds, limited by cap headroom. Wallets at 250 seeds compound 0% (capped).
- 15% of claimed rewards are claimed inside the window, at an average rate of 20%; 60% of that goes to the treasury.
- Lock forfeits are excluded (budget recovery, §8).

**e = 1.0** (users in aggregate receive 1.0× principal, below the 1.10 break-even):

| Wallets | Avg seeds | Principal → CAPITAL | Purchase fees | Compounded | Compound fees | Gross fees | Referral (−) | Penalty → treasury | **Net treasury** | Funded emission |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 100 | 50 | 50.00 | 5.00 | 25.00 | 2.27 | 7.27 | 0.58 | 0.45 | **7.14** | 50 |
| 100 | 150 | 150.00 | 15.00 | 60.00 | 5.45 | 20.45 | 1.64 | 1.62 | **20.44** | 150 |
| 100 | 250 | 250.00 | 25.00 | 0 | 0 | 25.00 | 2.00 | 4.50 | **27.50** | 250 |
| 500 | 50 | 250.00 | 25.00 | 125.00 | 11.36 | 36.36 | 2.91 | 2.25 | **35.70** | 250 |
| 500 | 150 | 750.00 | 75.00 | 300.00 | 27.27 | 102.27 | 8.18 | 8.10 | **102.19** | 750 |
| 500 | 250 | 1,250.00 | 125.00 | 0 | 0 | 125.00 | 10.00 | 22.50 | **137.50** | 1,250 |
| 1,000 | 50 | 500.00 | 50.00 | 250.00 | 22.73 | 72.73 | 5.82 | 4.50 | **71.41** | 500 |
| 1,000 | 150 | 1,500.00 | 150.00 | 600.00 | 54.55 | 204.55 | 16.36 | 16.20 | **204.38** | 1,500 |
| 1,000 | 250 | 2,500.00 | 250.00 | 0 | 0 | 250.00 | 20.00 | 45.00 | **275.00** | 2,500 |
| 5,000 | 50 | 2,500.00 | 250.00 | 1,250.00 | 113.64 | 363.64 | 29.09 | 22.50 | **357.05** | 2,500 |
| 5,000 | 150 | 7,500.00 | 750.00 | 3,000.00 | 272.73 | 1,022.73 | 81.82 | 81.00 | **1,021.91** | 7,500 |
| 5,000 | 250 | 12,500.00 | 1,250.00 | 0 | 0 | 1,250.00 | 100.00 | 225.00 | **1,375.00** | 12,500 |

**e = 2.0** (users in aggregate receive 2× principal; the healthy case):

| Wallets | Avg seeds | Purchase fees | Compound fees | Referral (−) | Penalty → treasury | **Net treasury** | Funded emission |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 100 | 50 / 150 / 250 | 5 / 15 / 25 | 4.55 / 10.00 / 0 | 0.76 / 2.00 / 2.00 | 0.90 / 3.42 / 9.00 | **9.68 / 26.42 / 32.00** | 100 / 300 / 500 |
| 500 | 50 / 150 / 250 | 25 / 75 / 125 | 22.73 / 50.00 / 0 | 3.82 / 10.00 / 10.00 | 4.50 / 17.10 / 45.00 | **48.41 / 132.10 / 160.00** | 500 / 1,500 / 2,500 |
| 1,000 | 50 / 150 / 250 | 50 / 150 / 250 | 45.45 / 100.00 / 0 | 7.64 / 20.00 / 20.00 | 9.00 / 34.20 / 90.00 | **96.82 / 264.20 / 320.00** | 1,000 / 3,000 / 5,000 |
| 5,000 | 50 / 150 / 250 | 250 / 750 / 1,250 | 227.27 / 500.00 / 0 | 38.18 / 100.00 / 100.00 | 45.00 / 171.00 / 450.00 | **484.09 / 1,321.00 / 1,600.00** | 5,000 / 15,000 / 25,000 |

USD, as an illustration only at an assumed $150/SOL: 1,000 × 150 seeds at e = 1 gives about 204 SOL, roughly $30.7k; 5,000 × 250 at e = 2 gives about 1,600 SOL, roughly $240k.

Sustainability note: at e = 1, treasury revenue is about 14% of the emission the funders must supply. **Fees cannot fund rewards, and policy forbids it** (§9). Reward budgets must come from disclosed external sources.

## 11. User break-even analysis

- **Aggregate:** users as a group profit only if funded emission (base plus Fruit) exceeds `(1 + fee) ×` planted principal, i.e. 1.10× at a 10% fee. At `e = 1.0` users lose 9.1% in aggregate; at `e = 2.0` they gain 81.8%. The fee moves the break-even by 10 points. Funding moves it by 100+.
- **Individual:** `t* = 365 × (1 + fee) / r_eff`, where `r_eff` is the wallet's gross APR on principal from base plus Fruit. At a 10% fee: 13.4 days at 3,000%, 16.4 days at 2,445%, 33.5 days at 1,200%.
- **Qualified wallets (≥150 seeds with matched external capital):** Fruit Storm adds about 66.98% of seed reference capital in one day at H = 12.5 (architecture §3). That is the single largest term, and it is independent of the fee.
- **Compounding:** worthwhile iff `r × T_remaining / 365 > 1.10`. Inside the penalty window, the threshold drops to `(1 − π) × 1.10`.
- **Claim penalty:** zero cost to a user who waits 72 hours after their last purchase, or who compounds.
- **Lock:** zero cost to a user who holds for 5 days; an early exit loses only the unvested bonus.

## 12. Required code changes (approved; for the dedicated fee implementation pass)

1. **Config:** `purchase_fee_bps = 1000` at `initialize`. Add `compound_fee_bps` (enforced `<= purchase_fee_bps`), used by `compound`, with a matching TypeScript quote. The ceiling stays at 1,000.
2. **Fee-change timelock:** `set_pricing` queues a change and takes effect only after a delay (for example 72h), with events. This protects the trust gap at a 10% fee.
3. **Referral commission** (architecture §7):
   - Config `reserved_commissions`, `referral_commission_bps = 2000`, `retained_margin_bps = 800`;
   - a per-referrer ledger PDA;
   - reservation inside `buy_rigs` and `compound`;
   - a `claim_commission` instruction;
   - `withdraw_fees` enforces `amount <= distributable − reserved_commissions`;
   - the per-event clamp `commission = min(floor(c·f), max(0, f − ceil(m·v)))`.
4. **Penalty:**
   - engine constants 3000 / 2000 / 1000 / 0 bps (TypeScript `penalty.ts` and vectors updated);
   - Model B: `PENALTY_TREASURY_BPS = 6000`. The claim moves the treasury share REWARD → FEE and redistributes the rest;
   - new counters `total_penalty_to_treasury` / `total_penalty_to_grove`;
   - `assert_fully_funded` ledger includes the treasury transfer;
   - `Claim` event gains the split (appended schema).
5. **Lock (when built):** forfeits go to Z at close, unchanged. Add a treasury Z-release instruction that can never touch L or U (test 75).
6. **Treasury accounting:** read-only aggregation (API, TransparencyPanel, runway report) of gross fees, reserved and paid commissions, net fee revenue, penalty treasury share, withdrawn, and the Z releases shown separately.
7. **Custody:** `fee_recipient` = the Protocol Treasury multisig. Rename user-facing copy to "Protocol Treasury".
8. **Buy panel:** disclose the fee, the early-claim fee schedule and destination, and the warning "claim before topping up".

## 13. Required test changes (approved; codified as architecture tests 78–91)

- Quotes at 1,000 bps; compound at `compound_fee_bps`; `compound_fee_bps > purchase_fee_bps` rejected; zero-seed compound charges nothing (already covered).
- A timelocked `set_pricing` cannot take effect early; its event is emitted.
- Referral tests 68–72 at c = 20%, m = 8%, including the clamp when the fee is lowered, commissions on compound fees, and replay.
- Penalty boundaries at 30 / 20 / 10 / 0 (exact second edges). Treasury/grove split sums exactly to the penalty with rounding in the vault's favour. `V >= L` after every split. No penalty after 72h. Compounding never triggers it. The no-other-planter case sends the grove share to dust.
- Solvency property/fuzz tests extended with treasury penalty transfers.
- TypeScript penalty, quote and accounting mirrors, with cross-language vectors regenerated.
- Lock: early exit forfeits exactly the unvested award; the Z release never touches L or U; the 80/20 release is exact.
- Accounting: net revenue excludes reserved commissions, L, U, Z and CAPITAL.

## 14. Final approved production fee table

| Parameter | Current | **FINAL (approved)** | Code change |
|---|---|---|---|
| Purchase fee | 3% (devnet config) | **10%** (1,000 bps, at the ceiling) | Config only |
| Fee ceiling | 10% | **10%** (keep) | None |
| Compound fee | = purchase fee | **10%**, separate field, enforced ≤ purchase fee | Yes (small) |
| Fee-change delay | none | **72h timelock** | Yes |
| Referral split | none (`c = 0`) | **20% referrer / 80% treasury** of eligible fees; margin `m = 8%` of volume | Yes (architecture §7) |
| Early-claim schedule | 25 / 15 / 5 / 0 | **30 / 20 / 10 / 0** | Yes |
| Early-claim destination | 100% grove | **60% treasury / 40% grove**, approved as a disclosed early-claim fee | Yes |
| 5-day lock early exit | cliff forfeit of the unvested bonus | **Unchanged cliff**; no extra fee; no decay | None |
| Lock forfeit destination | Z | **Z, then 80% released to treasury / 20% kept for the next Season** | With the lock build |
| CAPITAL | locked in this version | **Unchanged; never revenue** | None |
| Treasury | `fee_recipient` wallet | **Protocol Treasury multisig**; published gross → net accounting | Ops + read-only UI |

These founder decisions are adopted: the frozen architecture is amended (§7a, with edits to §2, §7, §8, §10a, §12–§14). No other part of the frozen model changes. Nothing is implemented until the fee implementation pass.
