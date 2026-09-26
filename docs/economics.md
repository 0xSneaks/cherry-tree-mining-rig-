# Economics

For `n` rigs: capital = `n × rig_price_lamports`; fee = `floor(capital × purchase_fee_bps / 10,000)`; total paid = capital + fee. The fee is capped at 10% and a single purchase at 1,000,000 rigs. Repository examples of 0.01 SOL and 3% are rehearsal values, not a live promise.

Rewards come only from `fund_rewards(amount, duration)`, which must back the schedule. When the window ends, APR is zero; existing liabilities remain accounted for. Mid-schedule funding rolls the remaining schedule into the new one.

APR uses the funded rate, 365-day annualization, fee-inclusive cost, and weighted total shares. More rigs dilute each rig’s share of the same emission. Phases are labels derived from state:

| Phase | Base APR |
|---|---:|
| Genesis | ≥ 1,200% |
| EarlyGrove | 600%–<1,200% |
| Growth | 250%–<600% |
| Mature | >0%–<250% |
| Dormant | no funded emission |

Claim penalties are 25% before 24h, 15% at 24–48h, 5% at 48–72h, and 0% thereafter. They stay in REWARD and are redistributed or recorded as dust. A valid first referrer changes weight from 100 to 105; it is not an extra payout or guaranteed 5% APR.

## Caps and launch windows

During the first 24 hours after T0, cumulative fresh purchases per wallet are capped; compounding never consumes the allowance and the cap stops applying exactly at T0 + 24h. The production value is 250 seeds (2.5 SOL of principal; fees additional). Rehearsal and qualification used smaller test-only numbers.

Separately, a **permanent per-wallet holding cap of 250 seeds** is adopted policy for the planned Fruit model. It is not yet implemented — it requires a new program version — and at that cap, compounding must keep unspent earned SOL claimable. See [What's next](whats-next.md).
