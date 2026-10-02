# Reward runway, refill operations and the production funding model (Backlog 10, 14)

**Status:** policy, tooling and model complete. **Nothing here has been executed.**
`fund_rewards` has not been called; no funds were sent.

## 1. Invariants (enforced by the program, restated as policy)

- REWARD is funded **only** by `fund_rewards`, signed by `config.reward_funder`.
- Purchase capital (CAPITAL) and purchase fees (FEE) **never** reach REWARD — `buy_rigs` takes
  the reward vault read-only (`t13`). No policy may route them there indirectly either.
- No automatic refill exists or may be built. Refill never follows purchase volume.
- At runway 0 future emission stops (Dormant); accrued liabilities stay backed
  (`distributable >= funded − claimed − compounded`, asserted every instruction).

## 2. Funding sources

| Eligible | Forbidden |
|---|---|
| Founder / company treasury SOL that did not originate from this program's CAPITAL or FEE vaults | CAPITAL vault (no path exists; none may be added) |
| Separately raised, disclosed marketing/community budgets | FEE vault withdrawals, directly or via a round trip through another wallet |
| Grants or sponsorships disclosed by source | Any amount computed from recent purchase volume |
| | Borrowed or leveraged SOL; SOL owed to users elsewhere |

Every refill discloses: source category, amount, duration, resulting rate, and the funder transaction.

## 3. Roles

- **Refill authority:** the reward funder, a **separate** multisig from the config authority
  and the upgrade authority (see `CUSTODY.md`). The founder activation wallet is never the funder.
- The config authority may rotate the funder (`set_reward_funder`) — only via the custody flow.

## 4. Cadence and alerts

- Monitor with `app/scripts/ops/runway-report.ts` (read-only) and `/api/runway`.
- Alert thresholds (env, defaults): **low < 14 days**, **critical < 3 days**
  (`CHERRY_TREE_RUNWAY_LOW_DAYS`, `CHERRY_TREE_RUNWAY_CRITICAL_DAYS`).
- `backingShortfall` (vault < ledger) is an emergency: the program should make it impossible,
  so it means an RPC fault or a bug — stop and escalate (§8).
- No fixed refill cadence is promised. Decide a refill (or a deliberate Dormant) **before**
  the low threshold, and announce either way.

## 5. Refill procedure (when a refill is decided)

1. `runway-report.ts` → record status, rate, runway, ledger, backing.
2. **Preview:** `runway-report.ts --preview <sol> <days>`. `fund_rewards` re-spreads
   `leftover + amount` over the NEW duration, so **a small top-up with a long duration LOWERS
   the current rate** (model scenario D: 5 SOL / 365 d at day 30 → −56%; scenario B: even a
   25 SOL / 90 d refill lowers 0.5555 → 0.4629 SOL/day). Pick the duration from the rate you
   will announce.
3. Announce: source, amount, duration, resulting rate and end date. No return language.
4. Funder multisig signs `fund_rewards(amount, duration)` (`scripts/devnet-fund.sh` is devnet-only;
   production uses the multisig's own proposal flow — see `CUSTODY.md`).
5. Verify: re-run `runway-report.ts` — new rate/finish match the preview; `BACKED yes`;
   `--history` shows the `RewardsFunded` event.
6. Archive evidence: preview output, announcement link, proposal + approvals, tx signature,
   post-state report.

## 6. Dormant transition and recovery

- **Transition:** at `reward_period_finish` the app shows Dormant (`GroveStatus`): runway 0,
  accrued claimable, "planting does not restart emission", "none is promised". Post an
  announcement that the funded schedule ended. Claims keep working.
- **Recovery (optional, never promised):** a new schedule is a fresh `fund_rewards` after
  Dormant (no leftover). The refill history labels it `afterDormancy` and the UI must say it was
  separately funded. Seeds planted during Dormancy earned nothing for that period.

## 7. Dashboard

Built: `/api/runway` + `GroveStatus` (runway, funded until / final schedule ended, future
emission, accrued-to-you) + `TransparencyPanel` (funded, claimed, replanted) +
`runway-report.ts` (ledger, backing, projected exhaustion = `reward_period_finish`,
refill history with restart labelling).

## 8. Emergency

| Situation | Action |
|---|---|
| `BACKED no` reported | Re-check against a second RPC. If real: config authority multisig pauses (stops buy/claim/compound; funding stays open), freeze further refills, publish incident, root-cause, forward-fix via the upgrade process. |
| Funder key compromised | Config multisig `set_reward_funder` to a fresh funder vault. A compromised funder can only *add* SOL — it cannot remove any. |
| Mistaken refill (wrong duration) | Cannot be undone. A corrective second funding can only re-spread; announce the actual rate. |

## 9. Production funding model

- Model: `app/src/lib/model/rewardFunding.ts` (exact port of `RewardGlobals::fund`).
- Report: `cd app && npx tsx scripts/model/reward-funding.ts --write` →
  `docs/models/reward-funding-scenarios.{md,json}`. Tests: `app/tests/models.test.ts`.
- Assumed pricing 0.01 SOL + 3% fee (current devnet config; production pricing undecided).

Conclusions:

1. **Rate is funding ÷ planted capital.** 100 SOL over 180 days is 1,969% at 1,000 seeds and
   1.96% at 1,000,000 seeds. Any headline rate is a statement about participation, not a yield.
2. **Holding a phase floor costs proportional to planted capital** (§2 of the tables): Genesis
   floor (1,200%) at 10,000 seeds for 90 days needs ~305 SOL of external funding.
3. **Longer windows lower the rate linearly** for the same amount; they extend runway.
4. **Under-participation** makes early rates very high and cheap; **over-participation** drives
   per-seed emission toward zero. Neither affects solvency.
5. **Refills interact with the live rate** (§5 above).

**Founder decisions remaining:** initial amount, duration, whether to front-load with several
sequential `fund_rewards` windows (YIELD.md s08), production seed price/fee, and the eligible
funding source for each refill.
