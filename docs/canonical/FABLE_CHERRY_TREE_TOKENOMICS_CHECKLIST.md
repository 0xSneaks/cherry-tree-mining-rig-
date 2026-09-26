# THE CHERRY TREE — Fable Tokenomics Review Checklist

**Purpose:** Give Fable a clean, quantitative checklist for the next tokenomics pass. This is a design/configuration review only. No production code, deployment, funding transaction, or launch authorization is requested.

## 0. Operating rules for Fable

- [ ] Recalculate the existing math independently before recommending changes.
- [ ] Do not invent percentages, boosts, referral cuts, lock bonuses, or thresholds because they “sound good.”
- [ ] Where a number cannot be solved from current data, state the missing input and give the equation/range needed.
- [ ] Keep **Cherry-funded rewards** separate from **external asset yield / price performance**.
- [ ] Preserve the fully-funded invariant: no user reward can be promised unless the SOL/reward budget backing it is actually available.
- [ ] Do not use new deposits to pay old liabilities.
- [ ] Do not treat annualized APR as the user’s actual period return.
- [ ] Do not assume wallet caps solve beneficial-owner / Sybil concentration.
- [ ] Use primary/official platform documentation when validating SKR, Hylo xAssets, JitoSOL, and spot JLP mechanics.
- [ ] Do not write production code or deploy anything.

---

# 1. Existing base math — verify first

Current working constants from the existing economic solve:

- [ ] Seed reference price: **P = 0.01 SOL / seed**.
- [ ] Current proposed Cherry benchmark: **M = 250 seeds**.
- [ ] Current qualification threshold: **T = 150 seeds = 60% of M**.
- [ ] Current maximum recognized Cherry/Fruit weight: **2.5 SOL-equivalent**.
- [ ] Current per-wallet Fruit payout ceiling: **20% of a funded epoch**.
- [ ] Current denominator floor: **12.5 SOL-equivalent**.
- [ ] One active Fruit per wallet per epoch in the current model.
- [ ] External Fruit holdings remain in the user’s wallet; Cherry does not take custody.

Verify these equations:

- [ ] `K_i = P × min(S_i, M)`
- [ ] `w_i = min(K_i, X_i)` once the wallet is seed-qualified and the external position is valid.
- [ ] `Cw = Σ w_i`
- [ ] `H = max(Cw, 12.5)`
- [ ] `r = E / D`
- [ ] Wallet accrual rate: `r × w_i / H`
- [ ] Total accrual over an epoch never exceeds `E`.
- [ ] One wallet never accrues more than `E/5` under the current denominator-floor design.

Recheck the worked 10 SOL Fruit example:

- [ ] Net Fruit budget `F = 10 SOL`.
- [ ] Continuation allocations total `1.626712330 SOL`.
- [ ] Opening Storm allocation `8.373287670 SOL`.
- [ ] At the 12.5 SOL floor, 24h actual reference return ≈ **66.9863%**.
- [ ] Annualized reference APR ≈ **24,450%**.
- [ ] Confirm the displayed APR is an annualization of a short funded period, not a promise of 24,450% for a year.

If any of the above is wrong, show the arithmetic and identify the exact source of the error before proceeding.

---

# 2. Launch structure — use the updated plan

Current launch direction:

- [ ] **Day 1 = 24-hour `.skr` early-access window.**
- [ ] **`.sns` has been removed from the launch equation.**
- [ ] **Public launch begins immediately after the `.skr` window.**
- [ ] **Public launch is the real `FRUIT_T0`.**
- [ ] Do not consume the main public Fruit Storm during the `.skr` early-access period unless the math proves a better structure.
- [ ] Day 1 should be tightly controlled because the Tree needs a stronger liquidity foundation before the wider Fruit system opens.

Fable must answer:

- [ ] What should the Day-1 Cherry cap be relative to the public cap?
- [ ] How much of the reward reserve, if any, should be spendable during Day 1?
- [ ] How much reserve should be protected for public `FRUIT_T0`?
- [ ] Should Day-1 users earn only base Tree economics / qualification history, or a small separate early-access incentive?
- [ ] At what liquidity/participation condition should the system safely transition from restricted early access to full public Fruit operation?

---

# 3. Cherry max-wallet math — solve, do not assume

Founder direction:

- [ ] **Cherry wallets are capped.**
- [ ] External Fruit assets are **not** capped at the user-wallet level because they have much larger outside markets/liquidity.
- [ ] Cherry only recognizes external Fruit value up to the wallet’s eligible Cherry reward weight.

Example:

A user may own 100 SOL-equivalent of xSOL, but if the wallet’s recognized Cherry Fruit ceiling is 2.5 SOL-equivalent, only 2.5 SOL-equivalent may affect Cherry rewards.

Fable must compare:

- [ ] **A. Hard Cherry holding cap:** wallet cannot exceed M seeds at all.
- [ ] **B. Fresh-purchase / reward-power cap:** wallet may compound above M, but weight above M is zero.
- [ ] **C. Alternative capped/diminishing structure:** only if mathematically superior and still easy to explain.

For each structure, model:

- [ ] 5 wallets.
- [ ] 25 wallets.
- [ ] 100 wallets.
- [ ] 500 wallets.
- [ ] 1,000 wallets.
- [ ] One maximum-size whale.
- [ ] One whale split across many wallets.
- [ ] Low-liquidity launch conditions.
- [ ] Mature/high-liquidity conditions.

Required decision:

- [ ] Should **250 seeds** remain the wallet cap?
- [ ] Should **150 seeds / 60%** remain the Fruit qualification threshold?
- [ ] If not, show the exact better cap/threshold relationship and why.

---

# 4. Fruit universe and external-asset rules

Current research shortlist:

- [ ] SKR
- [ ] xSOL
- [ ] xBTC — technical mapping still requires verification before use
- [ ] xHYPE — technical mapping still requires verification before use
- [ ] JitoSOL
- [ ] **Spot JLP only**

Explicit exclusions for this design pass:

- [ ] Leveraged JLP
- [ ] Kamino Multiply JLP
- [ ] Queued/vault JLP strategies
- [ ] Other JLP derivatives whose exit mechanics are materially longer or uncertain

Fable should validate per candidate:

- [ ] Exact asset/program/mint identity from primary docs.
- [ ] Whether direct wallet ownership/stake can be verified on-chain.
- [ ] Normal exit/cooldown mechanics.
- [ ] Liquidity/slippage risks.
- [ ] Whether the asset has dynamic leverage or borrow costs.
- [ ] Whether any current APY/yield figure is live, stale, variable, or not usable.
- [ ] Whether borrowed/encumbered capital should be excluded from recognized Fruit equity.
- [ ] Whether the same external capital can be double-counted across wallets or pools; if yes, specify the prevention rule.

Important:

- [ ] External asset yield does **not** fund Cherry liabilities.
- [ ] External asset leverage must never be added to Cherry APR as if it were yield.

---

# 5. Boost / loyalty economics — user-facing value, not “XP for XP’s sake”

There will be a permanent internal activity/loyalty ledger and lifetime leaderboard, but the user-facing benefits are:

- [ ] Higher boost.
- [ ] Higher **funded** yield when reserves/equity permit.
- [ ] Referral economics.
- [ ] Better access / higher tiers where appropriate.

Potential verified activity inputs include:

- [ ] Longevity / active time.
- [ ] Maintaining qualification.
- [ ] Successful harvest history.
- [ ] Verified referrals.
- [ ] Verified Cherry community/chat participation.
- [ ] GM/GN activity if Cherry provides an authorized, reliable message/event feed.
- [ ] Anti-spam/rate-limit rules for community activity.

Fable must solve the **boost curve**, comparing at least:

- [ ] Fixed tiers.
- [ ] Capped linear boost.
- [ ] Diminishing-return curve.
- [ ] Logarithmic curve.
- [ ] Percentile/rank-based boost.
- [ ] Hybrid tenure/activity + economic participation model.

Constraints:

- [ ] No uncapped veteran advantage.
- [ ] New users must still have a reason to join.
- [ ] Boost cannot create an unfunded liability.
- [ ] If boosted SOL rewards exist, they must come from an identified bonus/surplus reserve.
- [ ] Model what happens if 10%, 50%, and 80% of qualified users reach the maximum boost.
- [ ] Determine whether boosted users should receive a larger **share of a fixed funded bonus pool** versus causing the required reward budget itself to grow.

Required outputs:

- [ ] Recommended boost equation.
- [ ] Recommended maximum boost.
- [ ] Funding requirement at 25 / 100 / 500 / 1,000 qualified wallets.

---

# 6. Referral economics

Locked direction:

- [ ] **One referral level only.**
- [ ] Referrer receives a continuing cut of the referred user’s **eligible protocol-generated fees**.
- [ ] Referral commission does not come from the referred user’s principal.
- [ ] Referral commission does not confiscate the referred user’s already-earned rewards.
- [ ] Referral relationship persists unless there is a fraud/abuse rule that invalidates it.

Fable must solve:

- [ ] Safe referral percentage `c` rather than inventing it.
- [ ] Whether `c` should be fixed or increase with verified long-term participation.
- [ ] Maximum referral burden the Tree can tolerate.
- [ ] Treasury / reward-replenishment / operating-margin impact at different fee margins.

Use:

`gross eligible protocol fee = f`

`referral payout = c × f`

`protocol retained revenue = f × (1 - c)`

Stress-test referral economics under:

- [ ] Low fees / low volume.
- [ ] Normal fees / normal volume.
- [ ] High volume.
- [ ] High percentage of referred users.
- [ ] A small number of very large referrers.
- [ ] Self-referral, circular-referral, and multiwallet abuse attempts.

---

# 7. Lockups, vesting, and penalties

This is **not finalized** and Fable must decide from the math.

Candidate maximum duration: **no longer than 5 days**.

Compare:

- [ ] One standard short lock.
- [ ] A voluntary ladder such as 24h / 48h / 72h / 5d.

For each option:

- [ ] Identify the funded source of the extra boost.
- [ ] Calculate the maximum supported yield boost.
- [ ] Model if everyone selects the longest lock.
- [ ] Model if nobody locks.
- [ ] Model early exit.

Preferred principle to test:

- [ ] Early exit reduces/forfeits **unvested bonus** rather than confiscating principal or already-accrued funded rewards.

Fable should identify any case where another treatment is mathematically necessary and explain why.

---

# 8. Leveraged Fruit Safe Mode

Higher-risk leveraged candidates include:

- [ ] xSOL
- [ ] xBTC
- [ ] xHYPE

Desired user-protection model:

- [ ] Risk states: **NORMAL / WARM / HOT / EXTREME**.
- [ ] New user in a high-risk state must pass a two-step acknowledgement similar to 2FA:
  1. Risk screen.
  2. Explicit confirmation/signature.
- [ ] No automatic liquidation.
- [ ] No automatic leverage increase.
- [ ] Existing earned Cherry rewards are not retroactively confiscated because market risk increased.

Fable should determine what should feed the risk state:

- [ ] Effective leverage.
- [ ] Recent price move.
- [ ] Realized volatility.
- [ ] Drawdown.
- [ ] Available liquidity.
- [ ] Expected slippage.
- [ ] Oracle/reference-price freshness.
- [ ] Protocol collateral / solvency state.
- [ ] Exit-route condition.

Do **not** invent numerical thresholds without evidence.

Required output:

- [ ] Equations/score framework.
- [ ] Historical data required to calibrate each threshold.
- [ ] What Safe Mode should change: warning only, new-entry restriction, recognized-weight reduction, bonus reduction, or another action.
- [ ] Which actions remain user-controlled.

---

# 9. Liquidity-first public-launch economics

Founder direction: build a solid Tree/liquidity base before branching aggressively into Fruits.

Fable must model the relationship between:

- [ ] Cherry wallet cap.
- [ ] Number of wallets.
- [ ] Total Cherry participation.
- [ ] Tree liquidity / available market depth.
- [ ] Fruit qualification rate.
- [ ] External matching capital.
- [ ] Reward reserve.
- [ ] Public APR.
- [ ] Dilution as new users arrive.

Answer:

- [ ] Does the existing `M=250 / T=150 / Hfloor=12.5` structure protect the launch well enough?
- [ ] If Day 1 is tightly capped, how much public demand can arrive before the opening Storm APR compresses through key levels?
- [ ] How should public reserve size scale with expected qualified capital?
- [ ] At what point should the system transition from a “Storm” launch experience into ordinary/mature funded yield?

Calculate participation levels at which the opening APR crosses below:

- [ ] 10,000%
- [ ] 5,000%
- [ ] 2,500%
- [ ] 1,200%
- [ ] 600%
- [ ] 250%

---

# 10. Solvency and reserve accounting

Preserve and independently verify the accounting invariant:

- [ ] `V >= L + U + Z`

Where:

- `V` = liquid reward-vault SOL available for reward accounting.
- `L` = accrued but unclaimed liabilities.
- `U` = unaccrued committed rewards.
- `Z` = uncommitted reserve.

Check:

- [ ] Funding moves value into reserve without creating duplicate spendable accounting.
- [ ] Epoch creation moves committed budget from reserve to commitment accounting.
- [ ] Accrual moves committed capacity into user liability.
- [ ] Claims reduce both vault assets and matching liability.
- [ ] Unused elapsed capacity does not become somebody else’s retroactive windfall.
- [ ] Already-accrued liabilities cannot be swept into a new campaign.
- [ ] Integer/lamport rounding cannot cause total allocation above the funded epoch amount.

---

# 11. Sybil, whale, and abuse stress tests

Fable must explicitly test:

- [ ] One large wallet versus many small wallets.
- [ ] One beneficial owner splitting into many wallets.
- [ ] Multiple wallets using the same external Fruit capital over time.
- [ ] Referral farming across linked wallets.
- [ ] Chat/GM/GN spam farming.
- [ ] Late entry immediately before an epoch checkpoint.
- [ ] Rapid enter/exit balance carousel.
- [ ] Borrowed external capital presented as owned equity.
- [ ] Fake mint / wrong program / wrong decimals.
- [ ] Stale price/reference data.
- [ ] Fruit liquidity disappearing mid-epoch.
- [ ] Oracle/indexer outage.

For each attack, state:

- [ ] Does the current math remain solvent?
- [ ] Can the user gain unfair allocation?
- [ ] What exact rule mitigates it?
- [ ] What cannot be solved without stronger identity or historical ownership data?

---

# 12. Continuous external ownership verification

This is a known technical/economic dependency and must not be hand-waved.

Fable should specify the economic requirements for a later implementation to prove that the same external asset is not reused across wallets or intervals.

Cover:

- [ ] Point-in-time snapshots versus continuous/finalized ownership history.
- [ ] Registration model versus indexer model.
- [ ] Transfers during an epoch.
- [ ] Missing history / indexer downtime.
- [ ] How much reward capacity must remain reserved while an interval is disputed/unverifiable.
- [ ] How previous valid accrual remains claimable if new eligibility is paused.

Do not claim this is trustless unless the proposed mechanism actually is.

---

# 13. Historical incentive lessons to use — without copying insolvency

Compare the useful mechanics of long-running systems such as:

- [ ] GMX-style loyalty / multiplier systems.
- [ ] Curve-style commitment/boost logic.
- [ ] DRIP Garden-style “care for the system and increase production” behavior.
- [ ] PancakeSwap-style farm allocation / progression concepts.
- [ ] Fantom/SpookySwap-style partner-funded reward layers and vesting.

Extract only mechanics that can fit Cherry’s funded model:

- [ ] Loyalty/tenure boost.
- [ ] Short vesting/commitment bonus.
- [ ] Referral cut from real fee generation.
- [ ] Partner/founder-funded bonus epochs.
- [ ] Dynamic dilution as participation grows.
- [ ] Transparent countdowns and finite reward tranches.

Reject:

- [ ] New-deposit-dependent payout loops.
- [ ] Unfunded perpetual ROI promises.
- [ ] Unlimited emissions sold as sustainable yield.
- [ ] Illiquid token values treated as guaranteed realizable value.

---

# 14. Required stress-test matrix

At minimum, produce a table for:

- [ ] 5 qualified wallets.
- [ ] 25 qualified wallets.
- [ ] 100 qualified wallets.
- [ ] 500 qualified wallets.
- [ ] 1,000 qualified wallets.

For each show:

- [ ] Total recognized Cherry weight.
- [ ] Total matched external Fruit weight.
- [ ] Effective denominator.
- [ ] Opening funded tranche.
- [ ] Period return.
- [ ] Annualized APR.
- [ ] Per-wallet payout for threshold wallet.
- [ ] Per-wallet payout for max wallet.
- [ ] Reward dilution relative to the previous scenario.
- [ ] Required reserve for any loyalty/lock bonus assumptions.
- [ ] Referral burden under modeled fee assumptions.

Also include:

- [ ] One whale with 50% of nominal Cherry capital.
- [ ] Same whale split across multiple wallets.
- [ ] 80% of users at maximum loyalty boost.
- [ ] 100% of users selecting maximum lock.
- [ ] Zero lock participation.
- [ ] Sudden 2× and 10× participation increase during an epoch.

---

# 15. Final questions Fable must answer

- [ ] 1. Should 250 seeds be a true hard Cherry-wallet cap, or only the maximum recognized reward weight?
- [ ] 2. What should the Day-1 `.skr` cap be?
- [ ] 3. What portion of the reward reserve, if any, is allowed to be used before public `FRUIT_T0`?
- [ ] 4. Should the public qualification threshold remain 150 seeds / 60%?
- [ ] 5. What boost equation best rewards long-term healthy users without creating permanent veteran dominance?
- [ ] 6. What is the safe maximum boost?
- [ ] 7. How much dedicated SOL is required to fund that boost under 25 / 100 / 500 / 1,000-wallet scenarios?
- [ ] 8. What referral cut is sustainable at different fee margins?
- [ ] 9. Should long-term participation increase referral cut, and if so by how much?
- [ ] 10. One short lock or a 24h/48h/72h/5d ladder?
- [ ] 11. What funded boost can each lock support?
- [ ] 12. What is the early-exit treatment?
- [ ] 13. How should leveraged Fruit Safe Mode be calibrated?
- [ ] 14. What should Safe Mode actually restrict or alter?
- [ ] 15. Which Fruit candidates remain economically valid after official-doc verification?
- [ ] 16. How much reserve should be protected for the public Storm?
- [ ] 17. At what qualified-capital levels does the opening APR fall through 10k / 5k / 2.5k / 1.2k / 600 / 250%?
- [ ] 18. When should the system transition from Storm to mature funded yield?
- [ ] 19. Which variables remain impossible to solve without live Tree balances, fee rates, demand estimates, or historical market data?
- [ ] 20. What exact edits should be made to the existing Fruit Tokenomics document after the math pass?

---

# 16. Required Fable deliverable

Return the review in this order:

1. **Verified Existing Math**
2. **Conflicts / Outdated Assumptions**
3. **Recommended Cherry Max-Wallet Equation**
4. **Day-1 `.skr` Early-Access Model**
5. **Public `FRUIT_T0` Reserve Model**
6. **Qualification Threshold Analysis**
7. **Boost / Loyalty Equation**
8. **Referral Economics Equation**
9. **Lock / Vesting Equation**
10. **Leveraged Fruit Safe-Mode Framework**
11. **Fruit Candidate Validation Table**
12. **APR Dilution Table**
13. **5 / 25 / 100 / 500 / 1,000 Wallet Stress Test**
14. **Whale + Sybil Stress Test**
15. **Solvency / Failure Conditions**
16. **Inputs That Cannot Yet Be Solved**
17. **Final Recommended Tokenomics Table**
18. **Exact Proposed Edits to `CHERRY_TREE_FRUIT_TOKENOMICS.md`**

## Final constraint

The goal is not to maximize the displayed APR at any cost. The goal is to find the strongest combination of **rapid, attractive, fully-funded rewards + liquidity protection + retention + referral economics + risk controls** that can actually be implemented without breaking Cherry’s solvency.
