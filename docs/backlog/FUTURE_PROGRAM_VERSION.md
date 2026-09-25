# Category C — future program version (REQUIRES FRESH QUALIFICATION)

Nothing in this file is implemented. **No experimental on-chain code was written on this
branch**: none of these items would materially help before Part B, and each needs a design
decision first. Every item changes instructions, account layouts or economics, so each needs a
new program version, disclosure, audit and **fresh qualification** on a disposable Devnet ID.
None may enter the qualified Production V2 lane.

## 1. Items from this backlog that are Category C

| Item | Exact protocol impact | Recommendation |
|---|---|---|
| Direct referrer reward | New payout path from REWARD (or a new vault) + accounting; changes emission split | **Do not build.** Off-chain status + separately funded season prizes cover acquisition without deposit-funded referral economics. |
| On-chain seasons / season state | New accounts; risk of coupling season state to Miner | Not needed — seasons are fully off-chain. |
| Changing Genesis cap after init | New admin setter on Config | Not recommended: the cap only matters for 24h and is set before T0. |
| Split pause authority from config authority | New Config field + constraint; account layout change | Useful for custody (emergency pause without pricing power). Candidate for V3. |
| On-chain timelock for admin instructions | Queued-action accounts, new instructions | Prefer multisig-level timelock (Squads) — no program change. |
| CAPITAL release instruction | New instruction, timelock, caps | See `CAPITAL_FUTURE_POLICY.md`: default is never. |
| Referral binding via a signed Cherry param | None on chain (already `Option<Pubkey>`) | **Not C** — product-layer, pending Cherry answers. |
| 24h earning cap | Breaks O(1) accumulator; needs reward epochs | Remains rejected (README). |

## 2. Legacy / parked miner features (Backlog 19)

Source: README "Deferred — deliberately not built" (commit `0a9d997`, unchanged since) and
DEPLOY.md history (`814e755`: "the realized-yield / staking adapter is a separate phase").

| Item | Source | Original intent | Class | Fits current philosophy? | Decision |
|---|---|---|---|---|---|
| 24h earning cap | README Deferred | Limit per-day earnings per wallet | C | No — breaks O(1) accounting | Keep deferred / rejected. |
| Native SOL staking adapter ("realized-yield") | README Deferred; DEPLOY `814e755` | Put CAPITAL to work staking to create "realized" yield for REWARD | C | **No** — CAPITAL → REWARD is forbidden; exposes CAPITAL to slashing/custody risk | Rejected under current invariants. |
| Rig upgrades | README Deferred | Spend SOL to raise a position's output multiplier | C | Weakly — a purchase semantics change; any multiplier redistributes the same emission and advantages whales | Not recommended. If ever revisited: weight-based, capped, and fully requalified. |
| SPL token | README Deferred | A project token (rewards or governance) | C | Only if rewards stay pre-funded and no token emission substitutes for funded SOL | Not recommended now; would need its own legal/economic review. |
| Reward-vault recovery instruction | README Deferred / "V1 ships no recovery instruction" | Recover dust/over-collateralisation in REWARD | C | Only for provably unattributable dust, never owed funds | Low value (dust is tiny); defer. If built: sweep ≤ `undistributed_dust`, only when Dormant, to the funder, timelocked. |
| Native Android / dApp Store build | README Deferred | Ship outside Cherry on the Seeker dApp Store | B/A (no program change) | Yes | Deferred product work: a TWA/PWA wrapper of the standalone path is feasible without protocol change; needs Solana Mobile dApp Store publisher setup (founder). Not built this session. |
| Interactive blinks with callback buttons | README Deferred | Act (plant/claim) from inside a chat card | External | Blocked — Cherry SDK 0.1.21 blinks are read-only | Not buildable until Cherry supports interactive blinks. |
| 70/15/10/5 allocation | YIELD.md | A token/SOL split idea | — | Explicitly absent; purchase-funded variant rejected | NOT SUPPORTED BY CURRENT SOURCE as a requirement. |
| In-memory rate-limit / attribution stores | DEPLOY "Other pre-mainnet hardening" | Durable stores for production | A (ops) | Yes | Still open: the store interface exists (`AttributionStore`); a durable implementation needs the production host's datastore choice. |
| `script-src 'unsafe-inline'` | DEPLOY CSP section | Tighten CSP before mainnet | A | Yes | Still open: choose dynamic nonce rendering or hash-based script-src. |
| Upgrade-authority policy | DEPLOY hardening | Multisig or burn before mainnet | B | Yes | Done as `CUSTODY.md`. |

Named in the request but **NOT SUPPORTED BY CURRENT SOURCE** as distinct historical
requirements: "achievements", "leaderboard seasons", "anti-whale" beyond the Genesis cap
(implemented now as new off-chain work, not recovered requirements).
