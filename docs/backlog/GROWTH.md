# Growth, engagement and transparency layer (Backlog 2–9)

**Category A — built.** Off-chain and frontend only. No instruction, account layout, weight,
emission or purchase semantics changed. Commit `fc75b3b` on `claude/cherry-postqual-backlog`.

## What was built

| Backlog | Feature | Code | Tests |
|---|---|---|---|
| 5 | Official Cherry referral fallback + explicit precedence | `lib/growth/referral.ts`, `hooks/usePlanterStatus.ts`, `Game.tsx`, `BuyPanel.tsx` | `tests/referralFallback.test.ts`, e2e referral suite |
| 9, 10 | Dormant / low-runway / unfunded state, accrued vs future emission | `lib/growth/runway.ts`, `GroveStatus.tsx`, `YieldPanel.tsx`, `BuyPanel.tsx`, `/api/runway` | `tests/growth.test.ts`, e2e "exhausted pool" |
| 8 | Public dilution panel + what-if calculator | `lib/growth/dilution.ts`, `DilutionPanel.tsx` | `tests/growth.test.ts` |
| 2 | Distribution / concentration analytics, per-wallet caveat | `lib/growth/concentration.ts`, `DistributionPanel.tsx`, `/api/distribution` | `tests/growth.test.ts` |
| 3 | Post-72h milestones, tiers, reminders, days tended | `lib/growth/milestones.ts`, `MilestonesPanel.tsx` | `tests/growth.test.ts` |
| 4, 6 | Referral stats, qualified referrals, tiers, anti-farming flags, leaderboard | `lib/growth/referralStats.ts`, `ReferralPanel.tsx`, `/api/referrals` | `tests/growth.test.ts` |
| 7 | Seasons registry, season boards, prize metadata validation | `lib/growth/seasons.ts`, `/api/referrals?season=` | `tests/growth.test.ts` |
| — | Event decoding + refill history | `lib/program/events.ts`, `lib/program/eventHistory.ts` | `tests/growth.test.ts` (discriminators recomputed from sha256) |

## Official Cherry referral fallback (Backlog 5)

The qualified protocol already supports it: `buy_rigs(count, Some(referrer))` binds any existing
planter at position creation. So the fallback is **entirely product-layer**:

1. Wallet already has a position → nothing sent; no replacement, ever.
2. `?ref=` supplied → it always wins. If it cannot bind (malformed, self, not a planter) nothing
   is sent and the fallback is **not** substituted.
3. No `?ref=`, no position → the official referrer, only if
   `NEXT_PUBLIC_CHERRY_TREE_OFFICIAL_REFERRER` is set **and** that wallet is verified on chain as
   a planter (else the purchase would fail with `InvalidReferrer`). Otherwise nothing binds.
4. The buy panel shows a highlighted disclosure before signing: official fallback in use, binds
   permanently, 1.05× on the same emission, referrer receives nothing.

**Not configured in this repo.** No official wallet was invented. **Founder action:** choose the
official wallet, make its first planting (it must be a planter), then set the variable.
If the founder wallet is chosen, note it is also the founder activation wallet (custody doc).

## Qualified referrals, tiers, leaderboard (Backlog 4, 6)

- Source: permanent Miner state (`referrer`, `createdAt`), the same facts `ReferralBound` emits.
- **Qualified** = referred wallet has `lifetimeRigsBought ≥ MIN_FRESH_SEEDS` (own SOL; replanting
  excluded) **and** held `≥ MIN_HOLD_HOURS` since first planting. Wallet creation never counts.
- Thresholds are **founder decisions**: `CHERRY_TREE_REFERRAL_MIN_FRESH_SEEDS`,
  `CHERRY_TREE_REFERRAL_MIN_HOLD_HOURS`. Unset → leaderboard **closed**.
- Tiers (cosmetic): Seed Sharer 1, Grove Guide 5, Orchard Keeper 25, Cherry Ambassador 100.
- Anti-farming: official referrer + `CHERRY_TREE_EXCLUDED_REFERRERS` never ranked; `burst`
  (≥5 qualified within 10 min) and `minimumOnly` (≥80% exactly at the minimum) are **review
  flags** — they never remove a referral or touch a position. Prize eligibility requires manual
  review of flagged rows.
- No payouts, no levels, no multi-hop counting.

## Seasons (Backlog 7)

`CHERRY_TREE_SEASONS_JSON` (server env), for example:

```json
[{"id":"season-1","name":"First Bloom","startsAt":"1790400105","endsAt":"1792992105",
  "competitions":["referrals"],"qualificationGraceSeconds":"259200",
  "prizes":[{"description":"Cosmetic badge set","fundingSource":"separately-funded",
             "funder":"<disclosed wallet or entity>","disclosureUrl":"https://…"}]}]
```

Validation rejects: bad slugs, duplicates, overlaps, inverted/over-long windows, prizes not
`separately-funded`, prize funder equal to any protocol vault/config PDA, and return-promising
prize text. An invalid registry yields **no seasons** (fail closed).

**Proof Miner state is never reset:** seasons are pure functions over immutable `createdAt`;
there is no write path to any account; `tests/growth.test.ts` "season leaderboard … never resets
Miner state" freezes the input positions, checks they are byte-identical afterwards, and that
all-time standings still include the pre-season referral after the season ends.

## Post-72h engagement (Backlog 3)

Built without touching economics: grove status tiers by seeds (seeds never decrease, so tiers
never drop), tenure milestones from `createdAt`, replant milestones, first harvest, Genesis
planter, referred planter, "Rooted · penalty-free" status, upcoming-event reminders (penalty
band drops, Public opening, funded window end — each worded without urging any action), and
"days tended" (distinct days, localStorage, only increases). Forbidden mechanics — cooldowns,
daily-click requirements, resets, deposit-funded incentives — are absent by construction.

**Remaining (optional):** push/notification delivery of reminders (needs Cherry notification
support — not in SDK 0.1.21), season cosmetics artwork.

## Dilution and Dormant copy

- Dilution explainer states that the schedule is fixed, shares fall as seeds are added, solvency
  is unaffected, and figures are arithmetic, "not a forecast or a promise". Hidden when there is
  no funded emission.
- Dormant copy: runway 0, accrued stays claimable and backed, planting does not restart
  emission, a new schedule would have to be separately funded and none is promised. The old
  "Awaiting new funding" label (which implied funding was coming) is replaced.

## Configuration summary

See `app/.env.example` (growth layer section). Every value fails closed when unset.
`app/scripts/ops/check-production-env.ts` reports them as `EXTERNAL` until the founder sets them.
