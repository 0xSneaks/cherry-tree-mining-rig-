# Operations

Operations mean deliberately funding rewards, monitoring runway and vault balances, pausing user actions when necessary, and verifying state after administration. Funding is not automatic; empty runway stops future emission rather than creating an uncovered promise.

Scripts under `scripts/` and `app/scripts/ops/` cover initialization, funding, smoke/rehearsal, first-buy preflight/verification, fee withdrawal, and authority audits. `fund_rewards` is restricted to `reward_funder`; `withdraw_fees` is authority-triggered but destination-pinned. The primary source reports no real devnet rehearsal from this environment, no issued/configured mini-app ID, and unverified live wallet/ticker checks.
