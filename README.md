# THE CHERRY TREE

A Solana-native mining **simulation** with SOL-denominated rewards, built as a
[Cherry](https://chat.cherry.fun) mini-app.

> **Naming.** The product is **THE CHERRY TREE**; "Cherry Tree" in prose and
> `cherry-tree` as a slug. The crate and program keep their original technical
> names (`rigs-engine`, `sol-rigs`, the `sol_rigs` module), as do the PDA seed
> strings and the program ID — renaming those would change every instruction
> and account discriminator and invalidate the deployed address for no
> user-visible benefit. `app/src/lib/brand.ts` is the single source of the
> display name.

Buy rigs → rigs produce hashpower → SOL accrues → claim it or compound it into
more rigs. The classic ROI-miner loop with the payout engine replaced by a
fully-funded reward stream.

SOL is not proof-of-work mined. Rigs do not participate in Solana consensus and
do not cryptographically produce new SOL. The mining metaphor governs the game
loop and nothing else. Rig purchases buy game production capacity and are
**non-redeemable** — they are not a deposit.

---

## The core invariant

```
BUYING RIGS != FUNDING EXISTING MINER REWARDS
```

This is structural, not documentary. Three separate SOL vaults:

| Vault       | Receives                            | Pays out                       |
|-------------|-------------------------------------|--------------------------------|
| **CAPITAL** | non-fee portion of rig purchases    | nothing — locked in this version |
| **REWARD**  | realized revenue via `fund_rewards` | user claims and compounds only |
| **FEE**     | disclosed purchase fee              | authority withdrawal           |

`buy_rigs` takes the reward vault as a **read-only** account. There is no code
path by which a purchase lamport can reach a claim, and
`t13_purchase_lamports_never_reach_the_reward_vault` proves it by balance
accounting across a full buy → accrue → claim cycle.

---

## Layout

```
crates/rigs-engine/         all money logic — zero Solana dependencies
programs/sol-rigs/          the Anchor program: account validation + lamport moves
app/                        Next.js mini-app (Cherry embed + standalone fallback)
app/scripts/ops/            devnet operations: initialize, fund, smoke test
scripts/verify.sh           everything checkable without a live chain
scripts/verify-deploy-ready.sh  pre-deploy gate: IDs, branding, secrets, scripts
scripts/devnet-*.sh         devnet-only deploy / init / fund / smoke entrypoints
DEPLOY.md                   the runbook
YIELD.md                    genesis yield: APR model, phases, solvency simulations
LAUNCH.md                   founder activation, T0, access phases, referral, first buy
docs/backlog/               post-qualification backlog: growth layer, models, custody, runbooks
```

The split is the point. Every decision about how many lamports someone is owed
lives in `rigs-engine`, a crate that compiles for the host target with no
Solana types in sight, so the accounting is exercisable by an ordinary
`cargo test`. The program reads state out, runs the transition, writes it back,
and moves lamports according to what came back.

`#![deny(clippy::float_arithmetic)]` is on the engine. There is no floating
point in the money path, by construction.

---

## Decisions locked in V1

**`PRECISION = 1e12`, not `1e18`.** The hot expression is
`elapsed * reward_rate * PRECISION`, evaluated in `u128` (max ≈ 3.4e38).
Pessimistic bounds: `elapsed` 1e9 s (~31 years), `reward_rate` 1e12
lamports/s (1000 SOL/s). At 1e18 that product is ≈ 1e39 and overflows; at 1e12
it is ≈ 1e33, with five orders of magnitude to spare.

**Fee sits on top of capital.** `total = cost + fee`. The program and the UI
both call the same `quote_purchase`, and the TypeScript port is checked against
Rust-generated vectors, so they cannot drift apart.

**`total_rigs == 0` credits the elapsed slice to dust.** The schedule does not
stall and `reward_period_finish` never moves, so the runway shown in the
transparency panel stays honest. Those lamports sit in the reward vault
attributed to nobody, and V1 ships no recovery instruction — the vault ends up
over-collateralised, which is the safe direction. The rejected alternative,
stalling by extending `period_finish`, would make the advertised funded-until
date a moving target.

**Rounding always truncates in the vault's favour.** Never toward the user, and
never creating a lamport.

**The fully-funded invariant is O(1).** Everything that entered the reward vault
did so through `fund_rewards`; everything that left did so through `claim` or
`compound`. So `funded − claimed − compounded` is an upper bound on what is
owed, and it is asserted at the end of every instruction that touches reward
state.

**No admin path out of the reward or capital vaults.** The only admin money
path in this program version is `withdraw_fees`, and it is seed-bound to the
fee vault.

**Two-step authority handover,** so a typo cannot brick the program.

---

## Deploying

See **[DEPLOY.md](DEPLOY.md)** for the full runbook. In short:

```bash
cp .env.example .env && $EDITOR .env   # paths and public keys only, never key material
./scripts/verify.sh                    # full test pipeline
./scripts/verify-deploy-ready.sh       # pre-deploy gate
./scripts/devnet-deploy.sh             # real SBF build + deploy
./scripts/devnet-init.sh               # config + three vault PDAs, verified back
./scripts/devnet-fund.sh 0.5 3600      # explicit, never automatic
./scripts/devnet-smoke.sh 5 5 45       # two wallets, real elapsed time
```

Every one of those scripts refuses mainnet, in the shell guard and again in
the TypeScript. Deployer/authority and reward funder are independent roles,
supplied as a keypair **path** and a **public key** respectively; neither is
hardcoded anywhere.

## Building and testing

```bash
./scripts/verify.sh
```

### Testing without the SBF toolchain

`release.anza.xyz` and `github.com` are blocked by this environment's egress
policy, so there is no `anchor` CLI and no `cargo build-sbf`. The program is
still fully tested: integration tests run the **real Anchor entrypoint** under
`solana-program-test`'s native processor, which needs neither. Real account
validation, real PDAs, real lamport movement, no SBF compiler.

Pinned combination that builds on this toolchain: `anchor-lang 0.31.2` +
`solana-program-test 2.3.13`. Newer `solana-program-test` requires rustc 1.97.

`Anchor.toml` is present for deploying once the CLI is available.

### Cross-language fixtures

Three things could drift between Rust and TypeScript, so none of them is
trusted to stay in step by hand:

| What | Pinned by |
|---|---|
| Anchor discriminators | `programs/sol-rigs/tests/discriminators.rs` fails if a name changes without the client following |
| Account layouts | `app/tests/accounts.test.ts` decodes Borsh fixtures serialised by the program itself |
| Pricing + accrual math | `app/tests/{pricing,accrual}.test.ts` run against vectors generated by the engine |

`scripts/verify.sh` regenerates the vectors and fails if they move.

---

## Test coverage against the spec's acceptance list

**144 tests: 35 Rust, 82 vitest, 27 Playwright.**

| # | Requirement | Covered by |
|---|---|---|
| 1 | New rig earns nothing accrued before it existed | `t01_new_rig_earns_nothing_accrued_before_it_existed`, `t01_onchain_new_rig_earns_nothing_retroactively` |
| 2 | N:M rigs → N:M rewards | `t02_rewards_split_in_rig_ratio` |
| 3 | `claim` pays exactly `earned`, rigs unchanged | `t03_claim_transfers_exactly_earned`, `e2e_buy_accrue_claim_moves_real_lamports` |
| 4 | Double-claim pays nothing twice | `t04_double_claim_in_same_slot_pays_nothing` |
| 5 | `compound` buys `floor(pending/cost)`, keeps remainder | `t05_compound_worked_example_from_spec`, `t05b_compound_affordability_is_exact_with_fees` |
| 6 | Reward vault drops by exactly the compounded amount | `t06_compound_moves_real_lamports_between_vaults`, `e2e_compound_moves_reward_vault_into_capital_and_fee` |
| 7 | `earned + claimed ≤ funded` under fuzz | `t07_t08_...` (4 000 interleaved actions) + `fuzz.rs` (200 proptest cases) |
| 8 | Vault backs liabilities after every instruction | asserted by the harness after *every* action, plus on-chain in the e2e tests |
| 9 | Mid-schedule funding rolls leftover | `t09_funding_mid_schedule_rolls_leftover` |
| 10 | Funding reverts without backing | `t10_funding_without_backing_is_rejected` |
| 11 | No accrual past `period_finish` | `t11_accrual_stops_at_period_finish` |
| 12 | `total_rigs == 0` creates no unattributable liability | `t12_zero_rig_window_credits_dust_and_attributes_nothing` |
| 13 | No purchase lamport reaches the reward vault | `t13_purchase_lamports_never_reach_the_reward_vault` + e2e balance assertions |
| 14 | Authority cannot drain the reward vault | `t14_authority_cannot_withdraw_from_reward_vault` |
| 15 | Authority cannot drain the capital vault | `t15_authority_cannot_withdraw_from_capital_vault` |
| 16 | Only `reward_funder` may fund | `t16_only_reward_funder_may_fund` |
| 17 | `paused` blocks buy/claim/compound | `t17_paused_blocks_user_instructions` + a UI test |
| 18 | Wrong-owner position rejected, seeds validated | `t18`, `t18b_vault_substitution_rejected`, `t18c_config_substitution_rejected` |
| 19 | Checked math at realistic and extreme inputs | `t19_extreme_inputs_error_rather_than_wrap` |
| 20 | Strict detection false outside Cherry | `tests/environment.test.ts` — including the RN-WebView and iframe cases the non-strict fallbacks misread |
| 21 | Embedded: zero connect UI, rigs on first paint | `render.spec.ts` — two tests |
| 22 | Backend rejects forged/expired tokens | `tests/launchToken.test.ts` — real RS256 keypair, local JWKS |
| 23 | Ticker halts on suspend, re-anchors on resume | `accrual.test.ts` re-anchor test + `render.spec.ts` freeze test. **Partial:** the "matches chain within tolerance after a 5-minute suspend" half needs a live chain |
| 24 | `walletDisconnected` disables actions, keeps state | `render.spec.ts` — the event is pushed through the host stub |
| 25 | Standalone buy → compound → claim | **Not verified.** Needs a real wallet extension and a live cluster |
| 26 | Loads in an iframe with CSP set | `render.spec.ts` asserts `frame-ancestors`, the absence of `X-Frame-Options`, that `connect-src` is an allowlist rather than `https:`, that `script-src` is nonce-based and carries no `'unsafe-inline'`, and — in four separate tests with a `securitypolicyviolation` listener attached — that the app raises no violations under it |
| 27 | Blink renders from signed params; tampering is inert | `blink.spec.ts` — five tests against a real keypair, including override/append/reorder tamper attempts |
| 28 | Interpolation reconciles to chain | `accrual.test.ts` — parity with the engine at every fixture timestamp |
| 29 | Quoted total equals lamports debited | `t29_quote_matches_debit` + `pricing.test.ts` over 336 Rust-generated vectors + a UI test |

### What is not verified here, and why

This environment's egress policy blocks `api.devnet.solana.com`,
`api.mainnet-beta.solana.com` and `chat.cherry.fun`. So:

- **Nothing has been deployed to devnet.** Build-order step 15 — devnet
  end-to-end inside real Cherry on a real Seeker with a real funded schedule —
  has not been done and cannot be done from here.
- **Test 25** (standalone path through a real wallet adapter) is unverified.
- **Test 23's tolerance check** against a live chain is unverified; the logic
  it depends on is covered.
- The browser tests stub the Solana RPC at the network boundary and the Cherry
  host at the `window` boundary. The **app code under test is untouched**, and
  the account bytes the stub serves are the same Borsh fixtures the Rust
  program emits — but a stub is a stub.

Everything else in the table above is green.

---

## Mini-app notes

Full integration status, the Cherry registration packet and the hosting
constraint are in [`CHERRY.md`](./CHERRY.md).


- **Embedded is the primary path.** No connect-wallet step: the wallet arrives
  with the Cherry handshake, and a test asserts the embed renders no connect UI
  at all.
- **Signing** goes through `cherry.wallet.signAndSendTransaction`. Transactions
  are built with `@solana/kit` and handed over as wire bytes — no web3.js round
  trip. Signatures are confirmed against the RPC before any optimistic UI moves.
- **Standalone** is second-class but working: wallet-standard via
  `@solana/react`, behind the same `RigsSigner` interface, so `Game` is
  byte-identical in both modes.
- **Lifecycle is load-bearing.** The accrual ticker stops on `suspended` and
  re-anchors to on-chain state on `resumed` rather than resuming interpolation
  from a stale anchor. `visibilitychange` is the browser-level backstop.
- **The ore bin shows full lamport precision.** Six decimals would sit still for
  half a minute at a realistic emission rate and the mine would look stopped;
  the last three digits are de-emphasised but real.
- **Room context** drives the leaderboard and nothing else. Attribution comes
  from the *verified* `room_id` claim, is stored off-chain, and has zero effect
  on reward math.
- **Backend is read/index only.** It cannot move SOL, sign anything, or hold a
  key that could. Every user- or room-scoped endpoint verifies the launch token
  server-side; a wallet address in a request body is ignored. Rate limits key on
  the verified `sub`, not on IP.

### Configuration

Copy `app/.env.example` to `app/.env.local`. `CHERRY_APP_ID` comes from Cherry
app registration and is required before any launch token can be verified —
`/api/session` fails closed without it rather than accepting unverified tokens.
The `inline:render` permission is required for blink sharing.

---

## Deferred — deliberately not built

24h earning cap (breaks O(1) accumulator accounting; would need explicit reward
epochs), native SOL staking adapter, rig upgrades, an SPL token, a reward-vault
recovery instruction, a native Android / dApp Store build, and interactive
blinks with callback buttons.

---

## Licence

**Source-available, not open source.** Licensed under the
[Business Source License 1.1](LICENSE).

| | |
|---|---|
| **You may** | read it, audit it, learn from it, run it locally, study the reward accounting |
| **You may not** | deploy the program to a public network, or run any service on this work that takes value from or pays value to other people, without a commercial licence |
| **On 2030-09-23** | it converts automatically to Apache 2.0 |

"THE CHERRY TREE", the wordmark and the blossom palette are trademarks of
PhantomCapAI and are **not** covered by the code licence — a permitted use of
the code grants no right to the name or branding. See [NOTICE](NOTICE).

The licence is legal recourse, not a technical control. The thing that
actually governs the deployed program is its **upgrade authority**, and the
thing that governs the money is the vault separation — neither is something a
licence can do for you.
