# Economics

For `n` Seeds: capital = `n × rig_price_lamports`; fee = `floor(capital × purchase_fee_bps / 10,000)`; total paid = capital + fee. The fee is capped at 10% and a single purchase at 1,000,000 Seeds. Repository examples of 0.01 SOL and 3% are rehearsal values, not a live promise.

Rewards come only from `fund_rewards(amount, duration)`, which must back the schedule. When the window ends, APR is zero; existing liabilities remain accounted for. Mid-schedule funding rolls the remaining schedule into the new one.

APR uses the funded rate, 365-day annualization, fee-inclusive cost, and weighted total shares. More Seeds dilute each Seed’s share of the same emission. Phases are labels derived from state:

| Phase | Base APR |
|---|---:|
| Genesis | ≥ 1,200% |
| EarlyGrove | 600%–<1,200% |
| Growth | 250%–<600% |
| Mature | >0%–<250% |
| Dormant | no funded emission |

Claim penalties are 25% before 24h, 15% at 24–48h, 5% at 48–72h, and 0% thereafter. They stay in REWARD and are redistributed or recorded as dust. A valid first referrer changes weight from 100 to 105; it is not an extra payout or guaranteed 5% APR.
