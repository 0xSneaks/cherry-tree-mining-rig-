# Launch: the founder's first planting activates THE CHERRY TREE

One event flips the grove from pre-launch to live: **the first successful
purchase signed by the founder activation wallet.** That purchase's chain time
is **T0**. Every access phase after it is a pure function of T0 and the chain
clock. There is no manual launch time, no Day-2 or Day-3 toggle, and no
redeploy between stages.

| When | Phase | Who may plant |
|---|---|---|
| before T0 | PRE-LAUNCH | nobody (the founder activation only) |
| T0 → T0 + 24h | .SKR GENESIS ACCESS | `.skr` domain holders |
| T0 + 24h → T0 + 48h | .SKR + .SNS EARLY ACCESS | `.skr` or `.sns` domain holders |
| T0 + 48h onward | PUBLIC | everyone, permanently |

## Founder activation wallet

`FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp` is the founder activation
authority.

- It is pinned on-chain as `config.founder` by `initialize(…, founder)`.
- There is no setter.
- The ops scripts refuse any other value.
- The preflight fails closed if `config.founder` differs.

Activation depends on the wallet only. No domain name is resolved at runtime.

## Activation (on-chain, atomic)

`buy_rigs` decides through `rigs_engine::gate_purchase` before anything moves.
While `launch_t0 == 0`, the only purchase allowed is the activation. It needs:

- a signer equal to `config.founder`, and
- an active, funded reward schedule (`reward_rate > 0 && now < reward_period_finish`).

The purchase then runs and the fully-funded check passes. Only after that is
`launch_t0 := Clock::unix_timestamp` written, in the same instruction, and
`LaunchActivated` emitted.

- **Nothing else activates.** A failed, reverted or simulated transaction
  writes nothing. Initialising, funding and connecting never touch T0.
- **T0 is immutable.** It is written only while unset. No instruction resets
  it, re-initialisation is impossible, and a second founder purchase is an
  ordinary purchase — after T0 the founder faces the access gate like any
  other wallet, so without a domain proof it is refused and writes nothing.
- **Concurrency and replay.** Writes to Config are serialised, so concurrent
  attempts produce exactly one T0. A byte-identical replay is de-duplicated by
  signature.
- **Restarts and redeploys.** T0 lives in the Config account, so neither can
  change it.

## Access gate

Enforced inside `buy_rigs`, not in the UI. The phase comes from `launch_t0`
and the chain clock, so browser time, direct RPC calls, the Vercel URL, and
standalone vs Cherry-embedded mode make no difference.

The buyer passes its domain proof as remaining accounts:

- `[name]` for a directly owned domain;
- `[name, nft_record, token_account]` for a wrapped or tokenized one.

### `.skr` — AllDomains

`.skr` domains (Seeker ID) are issued on AllDomains. Solana Mobile's docs
say "SKR domains are built on AllDomains", and resolution uses AllDomains'
official SDK, `@onsol/tldparser`.

| | |
|---|---|
| Registry program | ANS `ALTNSZ46uaAUU7XUV6awvdorLGqAsPwa9shm7h4uP2FK` |
| `.skr` TLD parent | `F3A8kuikEiu6k2399oSJ1PWfcJYDHqpwoQ2e8psSDNuF` |
| **Direct** | ANS name record with the ANS discriminator, parent = `.skr`, unexpired, header owner = wallet |
| **Wrapped (NFT)** | the name record's owner is its Name House `NftRecord` PDA (`["nft_record", .skr name house, name]`, program `NH3uX6…`) |
| | that record is Active and points at the name |
| | the wallet's SPL token account holds exactly 1 of the record's mint |

This is the SDK's own resolution path (`getNameOwner` → `getMintOwner`).
The `.skr` TLD parent and name house are re-derived from first principles
in the program tests.

### `.sns` — SNS

`.sns` domains are issued by SNS (sns.id) on the SPL Name Service. SNS's
developer docs (dev.sns.id) define:

- `SNS_ROOT_DOMAIN_ACCOUNT` — "The `.sns` TLD";
- `getSnsDomainKeysForOwner` — "top-level `.sns` domain accounts owned by a wallet";
- `getSnsNftsForOwner` — "tokenized `.sns` domains".

| | |
|---|---|
| Registry program | SPL Name Service `namesLPneVptA9Z5rqUDD9tMTWEJwofgaYwp8cawRkX` |
| `.sns` TLD root | `58PwtjSDuFHuUkYjH9BYnnQKHfwo9reZhC2zMJv9JPkx` |
| **Direct** | Name Service record with parent = `.sns` root and header owner = wallet (exactly `getSnsDomainKeysForOwner`) |
| **Tokenized (NFT)** | the name record's owner is its tokenizer `NftRecord` PDA (`["nft_record", name]`, program `nftD3v…`) |
| | that record is Active and points at the name |
| | the wallet's SPL token account holds exactly 1 of the record's mint (the `getSnsNftsForOwner` path) |

Subdomains are not top-level domains, so they are rejected by the parent
check.

### RPC requirements for discovery

The app finds a wallet's proof with:

- `getProgramAccounts` + `memcmp` on ANS, the AllDomains Name House, SPL Name
  Service and the SNS tokenizer;
- `getTokenAccountsByOwner` (SPL Token) for held NFTs — never
  `getProgramAccounts` on the Token program, which most RPCs refuse;
- `getAccountInfo` for the wrapped / tokenized name record.

Each path fails closed on its own: a refused or failed call establishes
nothing and never hides a proof found on another path. Candidates pass the
program's own checks client side before they are used. The mainnet RPC must
allow `getProgramAccounts` with filters on those four programs, or `.skr` /
`.sns` holders are treated as the public phase (`app/tests/identity.test.ts`).

## Referral V1

- **Weight.** A validly referred wallet plants at **1.05× reward weight**.
  Emission is split by `Σ rigs × weight`: 100 for a base rig, 105 for a
  referred rig. That redistributes the same funded emission, so
  `paid + owed ≤ funded` is untouched. It is not "+5% APR".
- **Binding.** One wallet has one immutable referrer, bound at its first
  purchase to an existing planter.
- **Rejected:** self-referral, a non-planter referrer, a forged referrer
  account, and any referrer on a later purchase.
- **No payouts.** There is no referrer payout and there are no levels.
- **Links.** `?ref=<wallet>` is read from the launch URL in both modes
  (`app/e2e/render.spec.ts`, "referral link"). Whether Cherry forwards a
  shared link's query string into the mini-app launch is Cherry host
  behaviour, not documented in the SDK; it needs a real-Cherry check before
  referral links are promoted inside Cherry.
- **Limit.** This does not prove distinct humans.

## First buy — the one live transaction

1. **Read-only preflight (no key):**

   ```
   CHERRY_TREE_RPC_URL=<mainnet RPC> CHERRY_TREE_INTENDED_CLUSTER=mainnet-beta \
   CHERRY_TREE_FIRST_BUY_SEEDS=<count> npx tsx app/scripts/ops/first-buy-preflight.ts
   ```

   Every check must PASS. The preflight covers:
   - cluster, program id, founder wallet and fee wallet;
   - initialised, not paused, never activated;
   - funded, schedule active, solvent;
   - explicit amount, balance, fee, seeds, APR now and after;
   - exact instruction shape (one `buy_rigs`, no extra accounts);
   - simulation.
2. **Activation in the app.** Open the app with the founder wallet. The
   founder activation panel appears only for `config.founder` while
   pre-launch.
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
   solvency, APR, no referral, phase = .SKR GENESIS and about 24h to `.sns`
   access. Then it stops.
