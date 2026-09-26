# Operations

Operations mean deliberately funding rewards, monitoring runway and vault balances, pausing user actions when necessary, and verifying state after administration. Funding always precedes emission: every reward is backed before it exists.

Scripts under `scripts/` and `app/scripts/ops/` cover initialization, funding, smoke/rehearsal, first-buy preflight/verification, fee withdrawal, and authority audits. `fund_rewards` is restricted to `reward_funder`; `withdraw_fees` is authority-triggered but destination-pinned. The devnet qualification lane passed end to end on 2026-09-25, covering deploy, initialize, V2 migration, funding, founder activation and the launch boundary; the mini-app ID and production registration are launch-time configuration per the runbook.
