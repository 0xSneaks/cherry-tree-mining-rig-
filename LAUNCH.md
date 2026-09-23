# Launch: sneaks.skr's first planting activates THE CHERRY TREE

One event flips the grove from pre-launch to live: **the first successful,
confirmed purchase by the verified `sneaks.skr` wallet.** That purchase's
chain time is **T0**. Every access phase after it is a pure function of T0
and the chain clock. There is no manual launch time, no toggle, and no
redeploy between stages.

| When | Phase | Who may plant |
|---|---|---|
| before T0 | PRE-LAUNCH | nobody (the founder activation only) |
| T0 → T0 + 24h | .SKR GENESIS ACCESS | `.skr` holders |
| T0 + 24h → T0 + 48h | .SKR + SNS EARLY ACCESS | `.skr` or SNS holders |
| T0 + 48h onward | PUBLIC | everyone, permanently |

## Founder identity

- **Registry.** `.skr` names (Seeker ID) are AllDomains names. Solana Mobile's
  docs: "SKR domains are built on AllDomains". The program is ANS
  `ALTNSZ46uaAUU7XUV6awvdorLGqAsPwa9shm7h4uP2FK` (`@onsol/tldparser`).
- **Derived accounts.** Both are derived with the official scheme and
  re-derived from first principles in `programs/sol-rigs/tests/launch.rs`:
  - `.skr` TLD parent: `F3A8kuikEiu6k2399oSJ1PWfcJYDHqpwoQ2e8psSDNuF`
  - `sneaks.skr` name record: `H4ZaQQaESb69EPFnMLGFde1stMPbb5s15YBnzyDEHxZH`
- **Resolved wallet.** `FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp`.
  - Source 1: AllDomains' own resolver (`alldomains.id/api/domain-owner/sneaks.skr`).
  - Source 2: the Seeker fee-receiving wallet already pinned as `fee_recipient`.
  - **Neither is an on-chain read from this environment**, which had no
    mainnet RPC access. So the text "sneaks.skr" is never trusted. Instead:
    1. `initialize(…, founder)` pins the wallet in `config.founder`. There is
       no setter.
    2. The activation purchase must be **signed by `config.founder`**, and it
       must pass the `sneaks.skr` record, which the program checks in the same
       instruction. The record must be owned by ANS, carry the ANS
       discriminator, have the `.skr` parent, be unexpired, and have
       **owner == signer**.
    3. The first-buy preflight reads that record from chain and fails closed
       if the owner differs.
- **Not accepted (fails closed).** An NFT-wrapped `sneaks.skr`: its header
  owner is an escrow, not the wallet. If the preflight reports that, unwrap it
  first.

## Activation (on-chain, atomic)

`buy_rigs` runs `rigs_engine::gate_purchase` before anything moves. While
`launch_t0 == 0` it permits only the activation. All of the following must
hold:

1. The signer equals `config.founder`.
2. The signer proves it owns the `sneaks.skr` record at that moment.
3. The reward schedule is active: `reward_rate > 0 && now < reward_period_finish`.

The purchase then runs and the fully-funded check passes. Only after that is
`launch_t0 := Clock::unix_timestamp` written, in the same instruction, and
`LaunchActivated` is emitted. A failed, reverted or simulated transaction
writes nothing. Initialising, funding and connecting never touch T0.

- **T0 is immutable.** `t0_after_success` writes only while T0 is unset. No
  instruction resets it, re-initialisation is impossible, and a second
  founder purchase is an ordinary purchase.
- **Concurrency and replay.** The runtime serialises writes to Config, so
  concurrent attempts produce exactly one T0. A byte-identical replay is
  de-duplicated by signature.
- **Restarts and redeploys.** T0 lives in the Config account. A server
  restart, frontend redeploy or program upgrade cannot change it.

## Access gate

This is enforced inside `buy_rigs`, not in the UI. The phase comes from
`launch_t0` and the chain clock, so browser time, a direct RPC call, the
Vercel URL or Cherry-embedded vs standalone mode make no difference.

Buyers prove identity by passing their name record as the first remaining
account:

- **`.skr`:** an ANS record under `F3A8…`, unexpired, with owner == signer.
- **SNS:** a Name Service (`namesLPne…`) record with parent == SNS root
  `58Pwt…` and owner == signer. This is the same test as the official SDK's
  `getSnsDomainKeysForOwner`. Since SNS's 2026 migration these names display
  as **`.sns`** (formerly `.sol`).

These are not verifiable yet, so they are not accepted (they fail closed):

- tokenized (NFT) SNS names;
- the **new SRS-based `.sol`** names. No official SDK resolves them yet; SNS
  targets Q4 2026–Q1 2027.

## Referral V1

- **Weight.** A validly referred wallet plants at **1.05× reward weight**.
  Emission is split by `Σ rigs × weight`: 100 for a base rig, 105 for a
  referred rig. That redistributes the **same funded emission**, so
  `paid + owed ≤ funded` is untouched. It is not "+5% APR". The referred APR
  shown is computed from state, and exactly 1.05× the base rate.
- **Binding.** One wallet has one immutable referrer, bound at its first
  purchase to an existing planter.
- **Rejected:** self-referral, a non-planter referrer, a forged referrer
  account, and any referrer on a later purchase (replacement, stacking).
- **No payouts.** There is no referrer payout and there are no levels.
- **Cycles.** Cycles are impossible: a referrer must exist before the referred
  position does.
- **Limit.** This does not prove distinct humans. One person with two wallets
  can refer themselves. The gain is bounded at 5% extra share for that
  wallet, paid out of other planters' shares, never by new emission.

## First buy — the one live transaction

1. **Read-only preflight (no key):**

   ```
   CHERRY_TREE_RPC_URL=<mainnet RPC> CHERRY_TREE_INTENDED_CLUSTER=mainnet-beta \
   CHERRY_TREE_FIRST_BUY_SEEDS=<count> npx tsx app/scripts/ops/first-buy-preflight.ts
   ```

   Every check must PASS. The preflight covers:
   - cluster, program id, founder wallet, `sneaks.skr` on chain, and the
     fee wallet;
   - initialised, not paused, never activated;
   - funded, schedule active, solvent;
   - explicit amount, balance, fee, seeds, APR now and after;
   - exact instruction shape;
   - simulation.
2. **Activation in the app.** Open the app with the sneaks.skr Seed Vault
   wallet. The Founder activation panel appears only for `config.founder`
   while pre-launch.
   - Enter the count (there is no default) and run the preflight.
   - Type `ACTIVATE <count>`.
   - The app re-runs the preflight and simulation on fresh state and sends
     **one** transaction, only if they pass.
3. **Verify, then stop:**

   ```
   CHERRY_TREE_RPC_URL=<mainnet RPC> CHERRY_TREE_FIRST_BUY_SEEDS=<count> \
   npx tsx app/scripts/ops/verify-first-buy.ts <signature>
   ```

   It checks the founder signer, T0 against the block time, exact SOL
   movement and fee, seeds credited, account ownership, reward accounting and
   solvency, APR, no referral, phase = .SKR GENESIS and about 24h to SNS.
   Then it stops.
