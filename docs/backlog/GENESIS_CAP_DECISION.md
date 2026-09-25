# Production Genesis cap — decision artifact (Backlog 1, 2, 13)

**Status:** model complete. **Final number: founder decision — not chosen here.**
Qualification used `cap = 10`. That value is **test-only** and is not a candidate.

## 1. What the qualified V2 rule actually does

During `[T0, T0 + 24h)` every wallet must satisfy
`lifetime_rigs_bought + requested <= genesis_cap_rigs` (`rigs_engine::genesis_cap_check`).

| Fact | Consequence for the decision |
|---|---|
| Cumulative lifetime fresh purchases, per **wallet** | Splitting buys does not help; splitting **wallets** does. |
| Founder activation counts (`A <= C`) | The founder's activation size is bounded by the same number. |
| Compounding never counts, no cooldown | Genesis replanting adds seeds above the cap (§6 of the tables). |
| Cap vanishes at exact T0+24h | The cap bounds the **head start**, never long-run concentration. |
| Set once at `initialize` / `migrate_v2`; **no setter** | The choice is **irreversible for this program version**. Changing it needs a new program version + requalification. |
| Valid range `1 ..= 1e12` (`validate_genesis_cap`) | Any sane value is accepted; the program will not stop a typo. The runbook requires a two-person readback. |

## 2. The simulator

- Model: `app/src/lib/model/genesisCap.ts` (deterministic Pareto-quantile demand, no randomness).
- Report: `cd app && npx tsx scripts/model/genesis-cap.ts --write` → `docs/models/genesis-cap-scenarios.{md,json}`.
- Tests: `app/tests/models.test.ts` (k-wallet actor gets exactly `k × C`, founder `A <= C`, monotonicity, compounding, Public dilution, target search, determinism, "10 is test-only" label).

Every population, participation and demand figure is an **ASSUMPTION**. There is no measured
count of `.skr`-eligible wallets in this repository; the grid (2k / 10k / 50k / 150k eligible ×
0.5% / 2% / 6% / 15% participation) brackets outcomes rather than predicting one. Replace the
baseline with real numbers when they exist and re-run.

## 3. Key results (baseline: 10,000 eligible, 2% participate = 200 wallets; full tables in `docs/models/genesis-cap-scenarios.md`)

| cap | 1-wallet actor | 3-wallet | 5-wallet | 10-wallet | cap binds for | largest ordinary wallet |
|---:|---:|---:|---:|---:|---:|---:|
| 25 | 1.0% | 3.1% | 5.0% | 9.6% | 15% of planters | 1.0% |
| 50 | 1.7% | 4.9% | 8.0% | 14.8% | 7% | 1.6% |
| 100 | 2.9% | 8.1% | 12.9% | 22.8% | 3% | 2.6% |
| 250 | 5.8% | 15.6% | 23.5% | 38.1% | 1% | 4.7% |
| 500 | 9.7% | 24.4% | 35.0% | 51.8% | 0.5% | 7.0% |
| 1,000 | 15.3% | 35.2% | 47.5% | 64.4% | 0% | 8.3% |
| none | — | — | 90.1% | — | 0% | 1.6% |

(share of Genesis fresh seeds held by one actor who wants 50,000 seeds)

Observations the founder should weigh:

1. **Participation dominates.** The same cap 100 gives a 5-wallet actor 65.9% of Genesis at
   2,000 × 0.5% (10 planters) and 0.4% at 50,000 × 15%. The cap must be chosen for the
   *low* end of believable participation, because that is where concentration is worst.
2. **Multi-wallet scaling is linear.** A k-wallet actor gets `k × C`. No cap value fixes this;
   it only prices it (each extra wallet needs its own `.skr` proof and SOL).
3. **Low caps defer demand, they do not destroy it.** Deferred seeds move to Public at T0+24h
   (§4 of the tables). A cap that binds for >15% of honest planters mostly moves them to hour 24.
4. **Genesis replanting is small unless Genesis emission is large.** At 5 SOL/day Genesis
   emission the actor gains ~1 point (cap 100); at 20 SOL/day ~3 points (§6).
5. **Public dilutes everyone, including the actor, unless the actor keeps buying.** With
   100,000 Public seeds the Genesis cohort falls to ~3.7% of all shares (cap 100). A whale that
   tops up in Public is uncapped by design (§7) — the Genesis cap is not an anti-whale control
   for Public.

## 4. Candidate production range

The modelling supports a **candidate range of 25–250 seeds per wallet**, with three bands:

| Band | Cap | 5-wallet actor at 10k×2% | 5-wallet actor at 50k×2% | Binds honest planters | Trade-off |
|---|---:|---:|---:|---:|---|
| Tight | 25–50 | 5–8% | 1–2% | 7–15% | Strongest head-start limit; noticeably defers real demand; founder activation ≤ 25–50. |
| Middle | 100 | 12.9% | 2.9% | ~3% | Rarely binds honest planters; a 5-wallet actor still ~1/8 of Genesis at modest participation. |
| Loose | 250 | 23.5% | 6.1% | ~1% | Almost never binds; only acceptable if participation is expected to be high (≥50k eligible, ≥2%). |

Above 250 the cap stops binding honest planters at all and a 5-wallet actor exceeds ~35% of
Genesis at baseline participation — i.e. the cap stops doing its job. Below 25 it binds a large
share of ordinary planters and the founder activation becomes token-sized.

## 5. Sybil limitations (must be stated, not solved)

- The cap is **per wallet**. It is **not** unique-human protection.
- `.skr` adds friction (a domain per wallet, a Seeker-issued name) but does **not** prove one
  human per wallet; one person can hold several `.skr` names.
- Nothing on chain links wallets to people. Off-chain analytics (`/api/distribution`,
  `DistributionPanel`) make concentration *visible*; they do not detect or prevent Sybils.
- Referral weight (105) is also per wallet: a person can refer their own second wallet
  (bounded to a 5% larger slice of the same emission — `yield_sim` s07).

## 6. Required UI / docs language

Use verbatim (already in `PER_WALLET_CAVEAT`, shown in the distribution panel):

> All limits and figures here are per wallet, not per person. One person can hold several
> wallets, and nothing on chain or in this app can tell. A .skr domain adds friction; it does
> not prove one human per wallet.

Never write "one per person", "fair launch guaranteed", "anti-bot", or "Sybil-proof".

## 7. Founder decision rubric

1. **Pick the participation you would bet against**, not the one you hope for: the low end of
   eligible `.skr` wallets you can defend × a low participation rate.
2. **Pick the largest multi-wallet actor you want to tolerate** (k = 3? 5? 10?).
3. **Pick a concentration ceiling** for that actor's share of Genesis (e.g. ≤10%).
4. Read §5 of `docs/models/genesis-cap-scenarios.md` for that row: the largest cap that meets
   the ceiling. That is the **upper bound**.
5. Check §4 of the tables at that cap: if it binds more than ~10–15% of honest planters,
   decide whether deferring them to hour 24 is acceptable.
6. Confirm the founder activation size `A <= C` you actually intend.
7. Record the choice with its assumptions in the mainnet evidence archive; it cannot be changed
   in this program version.

Example (not a recommendation): "≤10% for a 5-wallet actor at 10k × 2%" → cap **50**;
the same rule at 50k × 2% → **250**.
