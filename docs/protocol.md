# Protocol

`initialize` creates configuration/vaults; `migrate_v2` appends the Genesis cap; `fund_rewards` deposits and schedules emissions; `buy_rigs` purchases whole rigs and may bind a referrer; `claim` pays settled net rewards; `compound` converts affordable whole rigs and preserves the remainder. Authority instructions pause, change configuration, hand over authority, and withdraw fees.

PDAs, defined in `programs/sol-rigs/src/constants.rs`:

| Account | Seeds |
|---|---|
| Config | `config` |
| Miner | `miner`, owner |
| Capital vault | `capital_vault` |
| Reward vault | `reward_vault` |
| Fee vault | `fee_vault` |

`Config` stores authority, funder, fee recipient, pause state, pricing, accumulator, launch, referral, penalty, and cap state. `Miner` stores rigs, reward watermark, pending rewards, lifetime counters, weight, referrer, and planting time. The engine uses integer lamports and checked `u128` intermediates. Purchase lamports cannot reach REWARD because `buy_rigs` treats it as read-only.
