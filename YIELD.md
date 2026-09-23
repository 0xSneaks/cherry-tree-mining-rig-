# Genesis yield

How THE CHERRY TREE's headline rate is computed, why it is always payable,
and what the target phase ranges cost.

## The model

```text
current_apr = base_reward_rate × phase_multiplier × utilization_multiplier × compound_multiplier
```

| term | what it is in this program |
|---|---|
| `base_reward_rate` | `reward_rate × SECONDS_PER_YEAR / cost_per_rig`. `reward_rate` is set only by `fund_rewards`, which refuses any schedule the reward vault cannot back. `cost_per_rig` is the fee-inclusive price. |
| `phase_multiplier` | The funded schedule itself. V1 applies no on-chain multiplier. A front-loaded genesis is done by the funder scheduling a higher rate early with ordinary `fund_rewards` windows (scenario s08). Nothing scales emission past what is funded. |
| `utilization_multiplier` | `1 / total_rigs`. Every rig planted dilutes every rig. This is the decay curve: APR falls hyperbolically as participation grows, and the earliest planters hold the largest share of emission while it is highest. |
| `compound_multiplier` | APY / APR, found by running the real `compound` transition forward (`project_compounding`), including fees, whole-rig truncation, self-dilution and the end of the funded window. It is never a `(1 + r/n)^n` formula. |

So, in integer basis points:

```text
apr_bps = reward_rate × 31_536_000 × 10_000 × 10_000 / (total_rigs × rig_price × (10_000 + fee_bps))
```

- The APR is 0 whenever there is no funded emission (the window has ended or never started).
- With no rigs planted, the quoted rate is the first rig's.
- The buy panel shows the APR after planting, which includes the dilution your own rigs cause.

The single implementation is `crates/rigs-engine/src/yield_curve.rs`. The
browser runs `app/src/lib/program/yield.ts`, a port that is checked
row-for-row against the engine (`app/tests/yield.test.ts`, 3,000+ vectors).
The e2e suite checks that the rendered APR equals the value computed from the
served program state.

## Phases

A phase is a label on the current APR, derived from global state only.

| phase | APR |
|---|---|
| Genesis | ≥ 1,200% |
| Early Grove | 600% – < 1,200% |
| Growth | 250% – < 600% |
| Mature Grove | > 0% – < 250% |
| Dormant | 0% (no funded emission) |

- **Next decay threshold:** the exact rig count at which the APR drops below the current phase's floor (`next_threshold`, tested to the rig).
- **Determinism:** rigs are non-redeemable, so between funding events the phase only moves forward. Split purchases, claim-and-rebuy and compounding cannot reset it. Only new funding raises it.
- **Exhaustion:** when the window ends, the APR is 0 and the phase is Dormant.
  - Nothing further accrues.
  - Everything accrued stays claimable.
  - Rigs planted after exhaustion earn nothing until new funding arrives.
  - The vault keeps only flooring residue (< 1 lamport per second of window).

## The 70 / 15 / 10 / 5 split, and what the targets cost

There is **no 70/15/10/5 allocation in this repository**: no constant, no
config field and no instruction, on any branch. The economics that exist are:

- Purchases go to **CAPITAL** (non-redeemable) plus a **FEE** (≤ 10%, currently 3%).
- Rewards come **only** from `fund_rewards`, SOL paid in by the reward funder.
- A purchase lamport can never reach a claim. `buy_rigs` takes the reward vault read-only.

So the target ranges depend entirely on how much the funder pays in, against
how much capital is planted:

```text
required emission per year = target_apr × planted_capital
```

```text
[s09] reward emission required to hold each target APR (fee-inclusive planted capital)
  target                    planted         per year      per 90 days     per 180 days
  Genesis 3,000%             10 SOL     300.0000 SOL      73.9726 SOL     147.9452 SOL
  Genesis 3,000%            100 SOL    3000.0000 SOL     739.7260 SOL    1479.4520 SOL
  Genesis 3,000%           1000 SOL   30000.0000 SOL    7397.2602 SOL   14794.5205 SOL
  Genesis 3,000%          10000 SOL  300000.0000 SOL   73972.6027 SOL  147945.2054 SOL
  Genesis floor 1,200%       10 SOL     120.0000 SOL      29.5890 SOL      59.1780 SOL
  Genesis floor 1,200%      100 SOL    1200.0000 SOL     295.8904 SOL     591.7808 SOL
  Genesis floor 1,200%     1000 SOL   12000.0000 SOL    2958.9041 SOL    5917.8082 SOL
  Genesis floor 1,200%    10000 SOL  120000.0000 SOL   29589.0410 SOL   59178.0821 SOL
  Early Grove floor 600%     10 SOL      60.0000 SOL      14.7945 SOL      29.5890 SOL
  Early Grove floor 600%    100 SOL     600.0000 SOL     147.9452 SOL     295.8904 SOL
  Early Grove floor 600%   1000 SOL    6000.0000 SOL    1479.4520 SOL    2958.9041 SOL
  Early Grove floor 600%  10000 SOL   60000.0000 SOL   14794.5205 SOL   29589.0410 SOL
  Growth floor 250%          10 SOL      25.0000 SOL       6.1643 SOL      12.3287 SOL
  Growth floor 250%         100 SOL     250.0000 SOL      61.6438 SOL     123.2876 SOL
  Growth floor 250%        1000 SOL    2500.0000 SOL     616.4383 SOL    1232.8767 SOL
  Growth floor 250%       10000 SOL   25000.0000 SOL    6164.3835 SOL   12328.7671 SOL
  Mature floor 60%           10 SOL       6.0000 SOL       1.4794 SOL       2.9589 SOL
  Mature floor 60%          100 SOL      60.0000 SOL      14.7945 SOL      29.5890 SOL
  Mature floor 60%         1000 SOL     600.0000 SOL     147.9452 SOL     295.8904 SOL
  Mature floor 60%        10000 SOL    6000.0000 SOL    1479.4520 SOL    2958.9041 SOL
```

Read the table this way:

- **Holding 1,200–3,000% costs 12–30× the planted capital per year.** With
  100 SOL planted, that is 296–740 SOL over 90 days.
- **Rig sales cannot fund it.** If a 70% share of purchase SOL were routed to
  rewards, holding APR `a` on capital `C` would need `a × C / 0.7` of new
  purchases every year: 17× the entire planted capital per year at 1,200%, and
  43× at 3,000%. That only works while new money keeps arriving faster than
  that, which is the definition of a scheme that ends insolvent.
  The program forbids it structurally, and it has not been changed.
- **If the "70% mining pool" is a token or SOL treasury** that the funder pays in
  through `fund_rewards`, then the targets are reachable exactly while planted
  capital stays under `pool_per_year / target_apr`:
  - a 100 SOL pool spent over 180 days holds Genesis (≥ 1,200%) up to ~17 SOL planted;
  - the same pool holds Mature (≥ 60%) up to ~338 SOL planted.

## Simulation results

`cargo test -p rigs-engine --test yield_sim -- --nocapture --test-threads=1`

Every action in every scenario asserts that the reward vault covers what it
owes, that its balance equals funded − claimed − compounded, and that
paid + owed ≤ funded.

Setup for all scenarios: rig price 0.01 SOL, fee 3%.

```text
[s01] 1 planter · 100 SOL over 180 days
  1 rigs                             rigs         1  planted       0.0103 SOL  APR  1,968,703.68%  Genesis
  SOLVENCY funded 100.0000 | claimed 99.9993 | compounded 0.0000 | owed 0.0000 | vault 0.0006 | slack 0.0006 SOL — OK
  100 rigs                           rigs       100  planted       1.0300 SOL  APR     19,687.03%  Genesis
  SOLVENCY funded 100.0000 | claimed 99.9993 | compounded 0.0000 | owed 0.0000 | vault 0.0006 | slack 0.0006 SOL — OK
  10000 rigs                         rigs     10000  planted     103.0000 SOL  APR        196.87%  Mature
  SOLVENCY funded 100.0000 | claimed 99.9993 | compounded 0.0000 | owed 0.0000 | vault 0.0006 | slack 0.0006 SOL — OK
[s02] 10 planters, mixed low/large deposits · 100 SOL over 180 days
  after planter 1                    rigs      5000  planted      51.5000 SOL  APR        393.74%  Growth
  after planter 10                   rigs      5033  planted      51.8399 SOL  APR        391.15%  Growth
  SOLVENCY funded 100.0000 | claimed 99.9993 | compounded 0.0000 | owed 0.0000 | vault 0.0006 | slack 0.0006 SOL — OK
[s02] 100 planters, mixed low/large deposits · 100 SOL over 180 days
  after planter 1                    rigs      5000  planted      51.5000 SOL  APR        393.74%  Growth
  after planter 10                   rigs      5033  planted      51.8399 SOL  APR        391.15%  Growth
  after planter 100                  rigs     50355  planted     518.6565 SOL  APR         39.09%  Mature
  SOLVENCY funded 100.0000 | claimed 99.9993 | compounded 0.0000 | owed 0.0000 | vault 0.0006 | slack 0.0006 SOL — OK
[s02] 1000 planters, mixed low/large deposits · 100 SOL over 180 days
  after planter 1                    rigs      5000  planted      51.5000 SOL  APR        393.74%  Growth
  after planter 100                  rigs     50355  planted     518.6565 SOL  APR         39.09%  Mature
  after planter 1000                 rigs    503600  planted    5187.0800 SOL  APR          3.90%  Mature
  SOLVENCY funded 100.0000 | claimed 99.9993 | compounded 0.0000 | owed 0.0000 | vault 0.0006 | slack 0.0006 SOL — OK
[s03] 10 planters compounding every 10 minutes for 7 days · 20 SOL over 7 days
  start                              rigs      1000  planted      10.3000 SOL  APR     10,124.58%  Genesis
  day 0                              rigs      1000  planted      10.3000 SOL  APR     10,124.58%  Genesis
  day 1                              rigs      1270  planted      13.0810 SOL  APR      7,972.11%  Genesis
  day 2                              rigs      1550  planted      15.9650 SOL  APR      6,531.99%  Genesis
  day 3                              rigs      1830  planted      18.8490 SOL  APR      5,532.56%  Genesis
  day 4                              rigs      2110  planted      21.7330 SOL  APR      4,798.38%  Genesis
  day 5                              rigs      2380  planted      24.5140 SOL  APR      4,254.02%  Genesis
  day 6                              rigs      2660  planted      27.3980 SOL  APR      3,806.23%  Genesis
  SOLVENCY funded 20.0000 | claimed 0.0000 | compounded 19.9820 | owed 0.0175 | vault 0.0180 | slack 0.0004 SOL — OK
[s04] 100 planters claiming every hour for 30 days · 50 SOL over 30 days
  start                              rigs      5950  planted      61.2850 SOL  APR        992.62%  EarlyGrove
  window over                        rigs      5950  planted      61.2850 SOL  APR          0.00%  Dormant
  SOLVENCY funded 50.0000 | claimed 49.9996 | compounded 0.0000 | owed 0.0000 | vault 0.0003 | slack 0.0003 SOL — OK
[s05] exhaustion: 5 SOL over 10 days, run to day 40
  day 0                              rigs       500  planted       5.1500 SOL  APR      3,543.66%  Genesis
  day 10 (window closes)             rigs       500  planted       5.1500 SOL  APR          0.00%  Dormant
  day 40                             rigs      1000  planted      10.3000 SOL  APR          0.00%  Dormant
  SOLVENCY funded 5.0000 | claimed 4.9999 | compounded 0.0000 | owed 0.0000 | vault 0.0000 | slack 0.0000 SOL — OK
  refunded 5 SOL / 10 days           rigs      1000  planted      10.3000 SOL  APR      1,771.83%  Genesis
[s06] 10 years: yearly 20 SOL top-ups, 50 planters compounding monthly
  year 0                             rigs      1000  planted      10.3000 SOL  APR        194.11%  Mature
  year 1                             rigs      2900  planted      29.8700 SOL  APR         66.93%  Mature
  year 2                             rigs      4850  planted      49.9550 SOL  APR         40.02%  Mature
  year 3                             rigs      6750  planted      69.5250 SOL  APR         28.75%  Mature
  year 4                             rigs      8700  planted      89.6100 SOL  APR         22.31%  Mature
  year 5                             rigs     10650  planted     109.6950 SOL  APR         18.22%  Mature
  year 6                             rigs     12600  planted     129.7800 SOL  APR         15.40%  Mature
  year 7                             rigs     14550  planted     149.8650 SOL  APR         13.34%  Mature
  year 8                             rigs     16500  planted     169.9500 SOL  APR         11.76%  Mature
  year 9                             rigs     18400  planted     189.5200 SOL  APR         10.54%  Mature
  year 10                            rigs     20350  planted     209.6050 SOL  APR          0.00%  Dormant
  SOLVENCY funded 200.0000 | claimed 0.0000 | compounded 199.3050 | owed 0.6332 | vault 0.6950 | slack 0.0617 SOL — OK
[s07] sybil/referral ring: 200 × 1 rig vs 1 × 200 rigs
  ring earned 4.9999 SOL, single account 4.9999 SOL
  SOLVENCY funded 10.0000 | claimed 0.0000 | compounded 0.0000 | owed 9.9999 | vault 10.0000 | slack 0.0000 SOL — OK
[s08] genesis curve: 100 SOL front-loaded 40/30/20/10 over 7/23/60/90 days, 1,000 arrivals
  epoch 1 opens                      rigs         0  planted       0.0000 SOL  APR 20,249,479.92%  Genesis
  epoch 1 closes                     rigs      1250  planted      12.8750 SOL  APR          0.00%  Dormant
  epoch 2 opens                      rigs      1250  planted      12.8750 SOL  APR      3,697.61%  Genesis
  epoch 2 closes                     rigs      2500  planted      25.7500 SOL  APR          0.00%  Dormant
  epoch 3 opens                      rigs      2500  planted      25.7500 SOL  APR        472.48%  Growth
  epoch 3 closes                     rigs      3750  planted      38.6250 SOL  APR          0.00%  Dormant
  epoch 4 opens                      rigs      3750  planted      38.6250 SOL  APR        104.99%  Mature
  epoch 4 closes                     rigs      5000  planted      51.5000 SOL  APR          0.00%  Dormant
  first 10 planters earned 7.9438 SOL, last 10 earned 0.0022 SOL
  SOLVENCY funded 100.0000 | claimed 0.0000 | compounded 0.0000 | owed 99.9982 | vault 100.0000 | slack 0.0017 SOL — OK
  target                    planted         per year      per 90 days     per 180 days
[s10] user examples · 30 SOL over 30 days, 0.01 SOL rigs + 3% fee
  A plants 10 rigs at genesis — entry APR 354,366.66%
  B plants 1,000 rigs an hour later — entry APR 3,508.58%
  after A+B                          rigs      1010  planted      10.4030 SOL  APR      3,508.58%  Genesis
  C plants 10 rigs on day 5 — entry APR 3,474.18%
  D plants 5,000 rigs on day 5 — entry APR 588.64%
  after C+D                          rigs      6020  planted      62.0060 SOL  APR        588.64%  Growth
  day 30 (window closes)             rigs      7123  planted      73.3669 SOL  APR          0.00%  Dormant
  A · 10 rigs, genesis, held               paid   0.1030 SOL  received   0.1281 SOL  rigs now     10  return   124.38%
  B · 1,000 rigs, hour 1, compounded daily paid  10.3000 SOL  received  11.3609 SOL  rigs now   2103  return   110.30%
  C · 10 rigs, day 5, held                 paid   0.1030 SOL  received   0.0369 SOL  rigs now     10  return    35.87%
  D · 5,000 rigs, day 5, held              paid  51.5000 SOL  received  18.4738 SOL  rigs now   5000  return    35.87%
  SOLVENCY funded 30.0000 | claimed 0.0000 | compounded 11.3609 | owed 18.6389 | vault 18.6391 | slack 0.0001 SOL — OK
```
