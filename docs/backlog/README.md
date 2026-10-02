# Post-qualification miner backlog

Branch: `claude/cherry-postqual-backlog` (isolated). Started from `1586bca` (= protected
`claude/cherry-protocol-v2` HEAD). **No file under `programs/`, `crates/` or `scripts/` was
changed; the qualified Part A → Part B lane is untouched.** Nothing was deployed, funded,
initialised or signed.

## Classification

A = product/off-chain, built now · B = model / policy / runbook, completed now ·
C = future program version, requires fresh qualification.

| # | Backlog item | Class | Deliverable |
|---|---|---|---|
| 1 | Genesis cap concentration / Sybil modelling | B | `lib/model/genesisCap.ts`, `scripts/model/genesis-cap.ts`, `docs/models/genesis-cap-scenarios.*`, [GENESIS_CAP_DECISION](GENESIS_CAP_DECISION.md) |
| 2 | Anti-whale product defences | A | Distribution analytics + per-wallet caveat — [GROWTH](GROWTH.md) |
| 3 | Post-72h engagement | A | Milestones / tiers / reminders / days tended — [GROWTH](GROWTH.md) |
| 4 | Referrer acquisition loop | A | Qualified referrals, tiers, anti-farming, season boards — [GROWTH](GROWTH.md) |
| 5 | Official Cherry referral fallback | A | Product-layer fallback, explicit precedence, disclosure — [GROWTH](GROWTH.md) |
| 6 | Referral leaderboard / badges | A | `/api/referrals`, `ReferralPanel` — [GROWTH](GROWTH.md) |
| 7 | Seasons / competitions | A | Off-chain season registry — [GROWTH](GROWTH.md) |
| 8 | Public dilution UX | A | `DilutionPanel` + calculator — [GROWTH](GROWTH.md) |
| 9 | Dormant state UX | A | `GroveStatus`, Dormant copy, no projections — [GROWTH](GROWTH.md) |
| 10 | Runway / refill operations | A + B | `/api/runway`, `runway-report.ts` (+ funding preview) — [REWARD_RUNWAY_OPS](REWARD_RUNWAY_OPS.md) |
| 11 | CAPITAL future policy | B (C to implement) | [CAPITAL_FUTURE_POLICY](CAPITAL_FUTURE_POLICY.md) |
| 12 | Multisig / timelock / immutability | B (+ A tool) | [CUSTODY](CUSTODY.md), `authority-audit.ts` |
| 13 | Genesis cap decision tool | B | [GENESIS_CAP_DECISION](GENESIS_CAP_DECISION.md) §7 rubric |
| 14 | Production reward funding model | B | `lib/model/rewardFunding.ts`, `docs/models/reward-funding-scenarios.*` — [REWARD_RUNWAY_OPS](REWARD_RUNWAY_OPS.md) §9 |
| 15 | CHERRY_APP_ID | A up to external boundary | `check-production-env.ts` — [CHERRY_INTEGRATION_READINESS](CHERRY_INTEGRATION_READINESS.md) |
| 16 | Real Cherry host / messenger | A up to external boundary | [CHERRY_INTEGRATION_READINESS](CHERRY_INTEGRATION_READINESS.md) §4 |
| 17 | Physical Seeker validation | B (plan; NOT EXECUTED) | [SEEKER_DEVICE_TEST_PLAN](SEEKER_DEVICE_TEST_PLAN.md) |
| 18 | Production / mainnet runbook | B (NOT EXECUTED) | [MAINNET_RUNBOOK](MAINNET_RUNBOOK.md) |
| 19 | Legacy / parked features | mixed | [FUTURE_PROGRAM_VERSION](FUTURE_PROGRAM_VERSION.md) §2 |
| C | Referrer payout, pause split, CAPITAL release, on-chain seasons, recovery, upgrades, staking, SPL | C | [FUTURE_PROGRAM_VERSION](FUTURE_PROGRAM_VERSION.md) |

## Founder decisions still open

1. Production Genesis cap (candidate range 25–250; rubric in GENESIS_CAP_DECISION §7).
2. Reward amount, duration, eligible source; production seed price and fee.
3. Official referrer wallet (`NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER`) — must be a planter.
4. Referral qualification thresholds (`CHERRY_TREE_REFERRAL_MIN_FRESH_SEEDS`, `_MIN_HOLD_HOURS`).
5. Seasons and any separately funded prizes.
6. Custody: multisig members, thresholds, timelocks, fee-recipient destination, revocation date.
7. CAPITAL future use (recommended: none).
8. Cherry registration (`CHERRY_APP_ID`) — production host chosen: Vercel ([CHERRY_INTEGRATION_READINESS](CHERRY_INTEGRATION_READINESS.md) §3).

## External actions (cannot be done from this repository)

Cherry mini-app registration and the §4 host questions; physical Seeker test execution;
multisig creation; audit engagement; mainnet deployment.

## Regenerating the model tables

```bash
cd app
npx tsx scripts/model/genesis-cap.ts --write
npx tsx scripts/model/reward-funding.ts --write
```
