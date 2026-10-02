# THE CHERRY TREE — devnet runbook

Devnet only, enforced three ways:

1. The shell guard in `scripts/lib/guard.sh` refuses any `SOLANA_CLUSTER`
   that is not devnet, and any RPC URL that looks like mainnet.
2. The same check runs again inside `app/scripts/ops/common.ts`, so going
   around the shell does not bypass it.
3. Before the first transaction, every script asks the **chain** for its
   genesis hash and refuses anything but devnet's. A label in an env file is
   a claim; the genesis hash is the fact — a private or proxied RPC that says
   nothing about itself still cannot be mainnet.

No script in this repository funds rewards on its own. Deploy does not,
initialize does not. Funding is always a separate, deliberate command.

---

## Roles

Three independent identities. They do **not** have to be the same key, and
the authority is never hardcoded.

Canonical public addresses (public keys only; no key material is kept here):

| Role | Address |
|---|---|
| Reward / funding wallet (`reward_funder`) | `Eu866FnHkUHmB6HYiPNz7vNVWbegPmFvmtQRdD9GNBsz` |
| Fee / treasury wallet (`fee_recipient`) | `4FsYyXTbF1iftkUTtZVmzmvisxUaGvoYfZDj4DuGLvwF` |

The fee / treasury address is pinned in `.env.example`, the launch preflight
(`app/src/lib/launch/expected.ts`), the devnet workflow and the program tests.

| Role | Supplied as | Used by |
|---|---|---|
| **Deployer / program authority** | `ANCHOR_WALLET` — a path to a keypair JSON kept outside this repo | deploy, initialize, `set_paused`, `set_pricing`, `set_reward_funder`, `set_fee_recipient`, `withdraw_fees` |
| **Reward funder** | `CHERRY_TREE_REWARD_FUNDER` — a **public key**; the keypair path goes in `CHERRY_TREE_REWARD_FUNDER_KEYPAIR` | `fund_rewards` only |
| **Fee recipient** | `CHERRY_TREE_FEE_RECIPIENT` — a **public key**, and nothing else. There is no keypair variable for it, by design. | receives `withdraw_fees`; **signs nothing** |

The authority cannot fund rewards unless it is also the reward funder, and the
reward funder cannot change config. `set_reward_funder` lets the authority
rotate the funder later; authority handover is two-step
(`transfer_authority` then `accept_authority`), so a typo cannot brick the
program.

### The fee recipient is a destination, not a permission

Protocol fees accrue in the FEE vault PDA and are withdrawn by an explicit
`withdraw_fees` call. The destination is **not** an argument: it is pinned to
`config.fee_recipient`, and the program rejects a withdrawal to any other
account.

That splits two powers that are usually one:

- the **authority** decides *whether and when* fees move, and signs — but
  cannot choose where they land;
- the **recipient** is where they land — but never signs, is never passed as a
  signer, and its private key is therefore never needed to collect revenue,
  on any machine or in CI.

Moving the payout address is `set_fee_recipient`, authority-only, and it emits
`FeeRecipientUpdated { previous, current }`. It refuses `Pubkey::default()`,
which would otherwise turn every withdrawal into a burn.

To collect:

```bash
./scripts/devnet-withdraw-fees.sh 0.25   # or: all
```

It prints the gross distributable FEE balance, the reserved referral
commissions and the withdrawable amount
(`withdrawable = distributable(FEE) - reserved_commissions`); `all` withdraws
exactly the withdrawable amount and never the reservation, and the script
refuses to run if the reservation is not backed. It verifies afterwards that
the recipient was credited by exactly the amount, that the reservation is
unchanged and still backed, and that the reward and capital vaults are
byte-for-byte unchanged.

**Nothing here ever holds key material.** Wallets are referenced by file path
or public key. The scripts print public keys only — never secret bytes, and
never the contents of a keypair file.

---

## 0. Prerequisites

```bash
# Solana CLI + Anchor. Agave v4.1.0 is pinned: scripts/lib/sbf.sh refuses
# cargo-build-sbf < 4.1.0 (older runners cannot parse edition-2024 crates)
# and always builds with --tools-version v1.52.
sh -c "$(curl -sSfL https://release.anza.xyz/v4.1.0/install)"
cargo install --git https://github.com/coral-xyz/anchor avm --locked
avm install 0.31.2 && avm use 0.31.2

# App dependencies
cd app && npm ci && cd ..
```

### Or run the whole thing on a GitHub runner

Every step from here to the smoke test is wired into
`.github/workflows/devnet.yml`, dispatched manually. It installs the
toolchain, builds SBF, deploys, initialises, funds a tiny schedule and runs
the two-wallet smoke test, then uploads a run report with the program ID,
signatures and vault addresses. Use it when the machine you are on has no
Solana toolchain or no route to devnet.

It needs three repository secrets, each the **contents** of a keypair JSON:

| Secret | Role |
|---|---|
| `DEVNET_PROGRAM_KEYPAIR` | the program's own keypair. Must be the keypair for `C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt` — the workflow refuses to deploy under any other address rather than rewriting `declare_id!`. |
| `DEVNET_AUTHORITY_KEYPAIR` | pays for the deploy and becomes the program authority. |
| `DEVNET_REWARD_FUNDER_KEYPAIR` | the only signer permitted to call `fund_rewards`. The workflow refuses to run if this equals the authority. |

The two smoke wallets are generated per run and funded from the devnet
faucet — they are throwaway test users, so there is nothing to store. Devnet
SOL only; the reward amount is capped at 1.0 SOL by the workflow itself.

Create the wallets, **outside this repository**:

```bash
solana-keygen new -o ~/.config/solana/cherry-tree-authority.json
solana-keygen new -o ~/.config/solana/cherry-tree-funder.json
solana-keygen new -o ~/.config/solana/cherry-tree-smoke-a.json
solana-keygen new -o ~/.config/solana/cherry-tree-smoke-b.json
```

Fund them with **devnet** SOL:

```bash
for w in cherry-tree-authority cherry-tree-funder cherry-tree-smoke-a cherry-tree-smoke-b; do
  solana airdrop 2 "$(solana-keygen pubkey ~/.config/solana/$w.json)" \
    --url https://api.devnet.solana.com
done
```

---

## 1. Configure

```bash
cp .env.example .env
$EDITOR .env
```

`.env` is git-ignored. Fill in:

```
SOLANA_CLUSTER=devnet
SOLANA_RPC_URL=https://api.devnet.solana.com
ANCHOR_WALLET=/home/you/.config/solana/cherry-tree-authority.json
CHERRY_TREE_REWARD_FUNDER=<public key of the funder>
CHERRY_TREE_REWARD_FUNDER_KEYPAIR=/home/you/.config/solana/cherry-tree-funder.json
CHERRY_TREE_RIG_PRICE_LAMPORTS=10000000
CHERRY_TREE_PURCHASE_FEE_BPS=300
# Explicit rehearsal value only; no production cap is chosen in this repo.
CHERRY_TREE_GENESIS_CAP_RIGS=10000
CHERRY_TREE_SMOKE_WALLET_A=/home/you/.config/solana/cherry-tree-smoke-a.json
CHERRY_TREE_SMOKE_WALLET_B=/home/you/.config/solana/cherry-tree-smoke-b.json
```

Never put private-key bytes or a seed phrase in `.env`.

---

## 2. Verify before touching a cluster

```bash
./scripts/verify.sh              # full test pipeline
./scripts/verify-deploy-ready.sh # program IDs, branding, secrets, scripts
```

`verify-deploy-ready.sh` fails on anything wrong and prints `WARN` lines for
things that are merely unavailable (no SBF toolchain, no devnet, no
`CHERRY_APP_ID`).

---

## 3. Deploy

```bash
./scripts/devnet-deploy.sh
```

Builds the real SBF artifact with the pinned toolchain (`scripts/build-sbf.sh`
does the build alone: cargo-build-sbf >= 4.1.0, `--tools-version v1.52`),
writes the IDL to `target/idl/sol_rigs.json` and `target/types/sol_rigs.ts`
with `anchor idl build` (what `anchor build` did after compiling), and
deploys it. It refuses to
continue if the program ID differs between `declare_id!`, `Anchor.toml`
(both profiles), the frontend constant, and the build's program keypair — and
it will **not** rewrite any of them to make them agree. A mismatch is yours to
resolve.

Expected program ID:

```
C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt
```

If `target/deploy/sol_rigs-keypair.json` is not that key, the deploy stops. Put
the correct program keypair there.

---

## 4. Initialize

```bash
./scripts/devnet-init.sh
```

Prints the cluster, program ID, authority, reward funder, rig price, fee,
explicit `genesis_cap_rigs` and all four PDAs **before** sending anything.
V2 `initialize` takes the cap as its final argument. The script then reads the
state back and verifies every field it set — authority, reward funder, price, fee, cap, version 2,
`total_rigs == 0`, `total_reward_funded == 0`, and that each vault PDA exists
and is owned by the program. Any mismatch fails loudly.

Initialization is one-shot: if the config PDA already exists, the script
refuses rather than touching live state.

## 4A. V1 → V2 migration (qualification and eventual controlled upgrade)

V2 appended one `u64 genesis_cap_rigs` field to Config (321 → 329 bytes);
V3 appends the architecture §7a protocol-fee fields (329 → 375 bytes).
`migrate_v2` now takes a V1 account straight to the current 375-byte layout,
and `migrate_v3` takes a V2 account to it (compound fee = legacy purchase
fee, nothing pending, commission counters zero). Miner and Vault layouts do
not change. The authority calls:

```
migrate_v2(genesis_cap_rigs: u64)
```

The instruction receives Config as a raw unchecked account because a V2
`Account<Config>` cannot deserialize the short V1 data. Before mutation it
checks the canonical `config` PDA, program owner, exact size 321,
discriminator, exact V1 decode with no trailing bytes, version 1, stored bump,
stored authority signer, and cap bounds. It then transfers only the exact rent
shortfall, reallocates to 375 with zero initialization, serializes all V1
fields unchanged except version 1 → 3, appends the cap and the §7a fee
fields, preserves the discriminator, and emits `GenesisCapConfigured` and
`CompoundFeeConfigured`.

There is an intentional migration dead window: after the V2 upgrade and
before successful migration, every typed Config instruction rejects the
321-byte account. If migration fails, data and lamports remain unchanged.
Recovery is a forward fix to V2 and a retried migration after diagnosing the
failure. **Never roll back to V1 after migration:** V1 can read the trailing
bytes but does not enforce the Genesis cap.

The qualification-only command must be run from the staged V2 source tree so
the wrapper can prove that all four staged identity locks equal the disposable
target:

```bash
cd target/qualification/<disposable-devnet-id>/v2/source
CHERRY_TREE_QUALIFICATION=1 \
CHERRY_TREE_QUALIFICATION_PROGRAM_ID=<disposable-devnet-id> \
CHERRY_TREE_GENESIS_CAP_RIGS=<explicit-test-cap> \
./scripts/devnet-migrate-v2.sh
```

It rejects the canonical program ID, refuses canonical or mismatched staged
source, and verifies Devnet from the chain before sending. It is not a
canonical migration command.

---

## 5. Fund rewards

```bash
./scripts/devnet-fund.sh 0.5 3600     # 0.5 SOL over 1 hour
```

Explicit, separate, and never automatic. Amount is parsed as integer lamports
via string arithmetic — there is no float in the money path. Devnet funding is
capped at 100 SOL as a typo guard.

Before submitting it prints the funder, amount, duration, current vault
balance, current rate and current period finish. Afterwards it re-reads from
chain and prints the new vault balance, rate, period finish and total funded,
then asserts the fully-funded invariant.

---

## 6. Launch rehearsal on real devnet

Since launch gating, a non-founder purchase before T0 is refused, so the old
two-wallet smoke (`devnet-smoke.sh`) can no longer run on a fresh deployment.
The rehearsal replaces it:

```bash
# Init with a THROWAWAY devnet founder (never the mainnet founder):
CHERRY_TREE_FOUNDER=<devnet founder pubkey> CHERRY_TREE_ALLOW_OTHER_FOUNDER=1 \
CHERRY_TREE_RIG_PRICE_LAMPORTS=1000000 ./scripts/devnet-init.sh
./scripts/devnet-fund.sh 1.0 172800          # must outlive T0 + 24h

CHERRY_TREE_FOUNDER_KEYPAIR=<devnet founder keypair> \
./scripts/devnet-rehearsal.sh activate 60     # right away
./scripts/devnet-rehearsal.sh public 60       # at or after exact T0 + 24h, fresh wallets A/B
```

`activate` proves:

- a non-founder purchase before T0 is refused (`AccessDenied`);
- the founder purchase writes T0 = its block time;
- a post-T0 founder purchase without `.skr` proof is refused
  (`AccessDenied`) and leaves T0 unchanged — the immutability proof: after
  T0 the founder is an ordinary wallet under the access gate, and a refused
  purchase writes nothing;
- a wallet with no domain is refused in the `.skr` phase;
- a founder replant pays no early-claim fee and keeps the planting clock;
- a founder claim pays the early-claim fee band its planting age earns (30%
  inside 24h of the latest purchase): 60% of the fee moves to the fee vault,
  and a sole planter's 40% grove share stays in the reward vault as dust;
- the capital vault is untouched, and the reward vault is solvent.

If `activate` dies mid-way it is continued, never repeated. The resume path
is guarded by a strict fingerprint of the interrupted state:

```bash
CHERRY_TREE_FOUNDER_KEYPAIR=<devnet founder keypair> \
CHERRY_TREE_SMOKE_WALLET_A=<wallet A keypair> CHERRY_TREE_SMOKE_WALLET_B=<wallet B keypair> \
ANCHOR_WALLET=<authority keypair> CHERRY_TREE_REWARD_FUNDER=<funder pubkey> \
./scripts/devnet-rehearsal.sh activate 60 --resume --expect-t0 <T0 on chain>
```

`--resume` refuses to run unless the chain is exactly at the state an
interrupted `activate` leaves behind — T0 already written by the founder
activation purchase, every counter still at its post-activation value, no
claim or compound, wallets A/B untouched, schedule still live — and then
continues from the T0-immutability probe. It never re-funds, never
re-activates and never replays the activation purchase; ordinary `activate`
without `--resume` stays one-shot.

`public` proves:

- public planting at weight 100, and a referred first planting at weight
  105 with the referrer bound;
- a second referrer is refused (`ReferrerAlreadyBound`);
- a replant pays no early-claim fee;
- a claim inside 24h pays the 30% early-claim fee: the net goes to the
  claimant and 60% of the fee to the fee vault;
- the reward vault is solvent.

`.skr` acceptance cannot run on devnet because the registry is mainnet-only;
it is covered by `programs/sol-rigs/tests/launch.rs` and
`app/tests/identity.test.ts`. `.sns` discovery remains tested for future use,
but it does not participate in the V2 launch gate.

The `devnet.yml` workflow runs the same thing (`stage: activate`, then
`stage: public` at or after T0 + 24h).

## Disposable V2 qualification lane (do not run against canonical Devnet)

Qualification uses a fresh disposable Devnet program keypair and is explicit
at every step. The canonical program
`C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt` is rejected as a target.

```bash
export SOLANA_CLUSTER=devnet
export SOLANA_RPC_URL=https://api.devnet.solana.com
export CHERRY_TREE_QUALIFICATION=1
export CHERRY_TREE_QUALIFICATION_PROGRAM_ID=<disposable-id>
export CHERRY_TREE_QUALIFICATION_PROGRAM_KEYPAIR=<outside-repo-keypair>

./scripts/qualification-build.sh v1
./scripts/qualification-build.sh v2
```

The build script stages source beneath
`target/qualification/<disposable-id>/{v1,v2}`. V1 is pinned to checkpoint
`faf95124010c995504621b92882b0922926558b5`; V2 is pinned to committed HEAD.
It changes program identity only inside those generated copies, requires the
disposable keypair to match, reruns identity locks there, uses the pinned SBF
toolchain, and leaves the canonical worktree untouched. The stage name and
disposable ID make qualification artifacts distinguishable in CI.

The reviewed artifacts are 398,168 bytes for V1 and 426,920 bytes for V2, a
28,752-byte increase. Before the V2 upgrade, explicitly add 32,768 bytes to
the disposable program's ProgramData account. That covers the increase with
4,016 bytes to spare; do not rely on `solana program deploy` to auto-extend.
Run the guarded upgrade owner from the staged V2 source tree:

```bash
cd target/qualification/$CHERRY_TREE_QUALIFICATION_PROGRAM_ID/v2/source

SOLANA_CLUSTER=devnet \
SOLANA_RPC_URL=https://api.devnet.solana.com \
CHERRY_TREE_QUALIFICATION=1 \
CHERRY_TREE_QUALIFICATION_PROGRAM_ID="$CHERRY_TREE_QUALIFICATION_PROGRAM_ID" \
ANCHOR_WALLET=/path/to/disposable-upgrade-authority.json \
./scripts/qualification-upgrade-v2.sh
```

The script verifies the RPC's genesis hash, staged identity, target, on-chain
upgrade authority and current capacity before it sends anything. Its explicit
extension is equivalent to:

```bash
solana program extend "$CHERRY_TREE_QUALIFICATION_PROGRAM_ID" 32768 \
  --url "$SOLANA_RPC_URL" \
  --keypair "$ANCHOR_WALLET"
```

It then reads `dataLen` back with `solana program show --output json`, requires
the capacity to have increased by exactly 32,768 bytes and to fit the V2
artifact, and upgrades with explicit `--url`, `--upgrade-authority`,
`--keypair`, `--fee-payer`, and `--no-auto-extend` arguments. It never changes
global Solana CLI configuration.

After upgrade, the script dumps the deployed program and verifies it with
`scripts/verify-program-dump.sh`. ProgramData extension can cause that dump to
contain unused zero padding. Verification therefore hashes exactly the first
426,920 bytes (more generally, the staged artifact's measured byte length),
compares that SHA-256 with the staged artifact, and separately requires every
remaining dumped byte to be zero. A whole-file hash comparison between a
padded dump and the unpadded artifact is intentionally not used.

The future qualification execution order is fixed:

1. Deploy staged V1 under the disposable ID.
2. Initialize its exact 321-byte Config with T0 = 0.
3. From staged V2, run `qualification-upgrade-v2.sh` to extend ProgramData by
   32,768 bytes, verify capacity, upgrade with auto-extension disabled, and
   verify the artifact prefix plus zero-only padding.
4. From staged V2, run `devnet-migrate-v2.sh` for the qualification-only
   321 → 375 migration with an explicit test cap.
5. Fund externally, then activate with `A <= C`; verify T0 from block time.
6. Exercise below/exact/over-cap fresh buys and cap-exhausted compounding.
7. At exact T0 + 24h verify no-domain Public buying and cap removal.
8. Verify claim, compound and all three vault/economic invariants.
9. Archive source refs, toolchain versions, artifacts/hashes, program ID,
   transaction signatures, account snapshots and boundary timestamps.

Never deploy, upgrade, migrate, fund or activate the canonical V1 Devnet as
part of qualification. This runbook prepares the lane; this Phase 13 work
does not execute it.

---

## 7. Frontend

```bash
cd app
cp .env.example .env.local   # NEXT_PUBLIC_* plus CHERRY_APP_ID
npm run build && npm start
```

`CHERRY_APP_ID` comes from Cherry mini-app registration. Without it
`/api/session` fails closed rather than accepting an unverified launch token.
The `inline:render` permission is required for blink sharing.

---

## Vault behaviour in V2

| Vault | Receives | Pays out |
|---|---|---|
| CAPITAL | non-fee portion of rig purchases | **nothing — locked in this program version** |
| REWARD | `fund_rewards` only | user claims and compounds only |
| FEE | disclosed purchase fee | `config.fee_recipient` only, triggered by the authority via `withdraw_fees` |

The capital vault doing nothing is deliberate. It has no withdrawal,
reward-support or treasury-extraction instruction. Rigs are non-redeemable.
REWARD is funded only by explicit external/non-purchase deposits; neither
purchase proceeds nor purchase fees ever fund it. Pause stops buy, compound
and claim, while deliberate reward funding remains open.

---

## Content Security Policy

Tightened and, more importantly, **verified survivable**: four browser tests
drive the embedded path, the standalone path, the blink card and an
interaction with a `securitypolicyviolation` listener attached, so a
directive that breaks the app fails the suite rather than degrading
production quietly.

What ships on a Node deployment:

```
default-src 'self'; script-src 'self' 'nonce-<per request>' 'strict-dynamic';
style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:;
connect-src 'self' <your RPC> <its wss> https://chat.cherry.fun;
frame-ancestors 'self' https://chat.cherry.fun;
frame-src 'none'; object-src 'none'; form-action 'none';
base-uri 'self'; worker-src 'self'; manifest-src 'self';
upgrade-insecure-requests
```

The policy lives in `app/src/lib/csp.ts`, and every emitter reads it from
there — `src/proxy.ts` mints the nonce and serves the policy per request on a
Node deployment, `next.config.ts` serves the nonce-free headers on the same
deployment, and `app/scripts/emit-static-headers.ts` writes a hash-based
variant into the static export's `_headers` file for a host that reads one
(Cloudflare Pages, Netlify). Hand-maintained copies of a security policy are
how they drift, so there is one builder.

`script-src` carries **no `'unsafe-inline'`** — `buildCsp` throws on it, so it
cannot quietly come back. The inline scripts in every document are Next.js's
own (the bootstrap and the RSC flight data), and each deployment mode admits
them its own way:

- **Node** — `src/proxy.ts` (the Next 16 proxy convention, formerly
  middleware) mints a nonce per request, sets the CSP on both the request
  (where Next parses the nonce out and stamps it on every script it emits)
  and the response (where the browser enforces it), and marks the document
  `Cache-Control: private, no-store` so a nonce can never be replayed from a
  cache. The root layout opts HTML routes into per-request rendering for the
  same reason: a build-time prerender cannot carry a nonce, and Next 16 will
  happily serve the stale prerender under a fresh nonce header — every script
  in it blocked. The browser tests in `e2e/render.spec.ts` pin the nonce
  shape, its rotation, and that the document's scripts carry the nonce the
  header minted.
- **Static export** — no server exists to mint anything, so the emitter
  hashes every inline script the export baked into each page and writes one
  `_headers` rule per document with those digests, computed at emit time
  from the exported HTML. The `/*` fallback admits nothing: a page the
  scanner missed fails closed rather than falling back to a blanket allow.

A host that sends no headers at all — GitHub Pages — cannot satisfy
`frame-ancestors`, and a `<meta http-equiv>` tag is not equivalent: the
directive is ignored there by specification. That is fine for Cherry's mobile
WebView, which does not frame anything, and not enough for Cherry web. See
`CHERRY.md` §3.

`connect-src` is **derived from configuration**, not a wildcard: it is built
from `NEXT_PUBLIC_SOLANA_RPC_URL`, `CHERRY_JWKS_URL` and the Cherry host at
build time. This matters more here than in most apps because the page holds a
live wallet session — a blanket `https:` would let any injected script
exfiltrate anywhere. `CSP_EXTRA_CONNECT_SRC` takes a space-separated list of
additional origins for deployments behind a proxy; it is still an allowlist.

`'unsafe-eval'` is gone from production and kept only for the dev server,
which needs it for React Refresh.

### Resolved: `script-src 'unsafe-inline'` is gone

The trade this section used to track was made for production hardening:
inline scripts are admitted by per-request nonce on a Node deployment and by
build-time hash on a static host (see above). The cost on the Node side is
that HTML routes render per request instead of being served from a build-time
prerender — first paint is still server-rendered HTML on first byte, and the
embedded path is unaffected functionally: the browser suite drives it end to
end under the nonce policy with a violation listener attached.

## Other pre-mainnet hardening

- Rate limiting and room attribution run behind an explicit storage
  boundary (`app/src/lib/server/stores.ts`), with the durable backend now
  chosen: **Upstash Redis**, spoken to over its REST protocol
  (`app/src/lib/server/upstashRedis.ts` — dependency-free, one POST of a
  JSON command array per command). Configuration is the official pair
  `UPSTASH_REDIS_REST_URL` + `UPSTASH_REDIS_REST_TOKEN` (both, or neither:
  half a pair is treated as unconfigured). Setting the pair selects the
  durable adapters in every environment, so staging can be pointed at a
  real resource first.
  - Rate limiting is one atomic Lua script per check — `INCR` + arming the
    TTL on first hit + reading the remaining window — so the counter can
    never outlive its TTL or race across instances. Keys are namespaced
    `cherry-tree:rate:*`.
  - Attribution claims atomically (`SET NX` on the wallet key, then the
    room's member set, in one script), preserving first-write-wins across
    instances. Keys: `cherry-tree:attr:*`.
  - Redis unreachable/erroring — network, timeout (3 s), HTTP error, or a
    rejected command — maps to the same `StoreUnavailableError` as having
    no store, and the endpoints answer 503. Nothing falls back to memory.
  - Outside production the boundary defaults to in-process memory; in
    production without the Upstash pair it **fails closed** — 503 — unless
    `CHERRY_ALLOW_INPROCESS_STORE=1` explicitly acknowledges a
    single-process deployment. The readiness check (`CHERRY_SERVER_STORE`)
    passes the configured pair, fails a half pair or a non-https URL, and
    fails an unconfigured production.
- A program upgrade-authority policy (multisig or burn) before any mainnet
  consideration.
