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

Three independent identities. None is hardcoded anywhere, and they do **not**
have to be the same key.

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

It verifies afterwards that the recipient was credited by exactly the amount
and that the reward and capital vaults are byte-for-byte unchanged.

**Nothing here ever holds key material.** Wallets are referenced by file path
or public key. The scripts print public keys only — never secret bytes, and
never the contents of a keypair file.

---

## 0. Prerequisites

```bash
# Solana CLI + Anchor
sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
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
| `DEVNET_PROGRAM_KEYPAIR` | the program's own keypair. Must be the keypair for `By7qjU4ZYBYQvdkh41UjnhxWmLyP63S3PbdZXbg1UiKH` — the workflow refuses to deploy under any other address rather than rewriting `declare_id!`. |
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

Builds the real SBF artifact with `anchor build` and deploys it. It refuses to
continue if the program ID differs between `declare_id!`, `Anchor.toml`
(both profiles), the frontend constant, and the build's program keypair — and
it will **not** rewrite any of them to make them agree. A mismatch is yours to
resolve.

Expected program ID:

```
By7qjU4ZYBYQvdkh41UjnhxWmLyP63S3PbdZXbg1UiKH
```

If `target/deploy/sol_rigs-keypair.json` is not that key, the deploy stops. Put
the correct program keypair there.

---

## 4. Initialize

```bash
./scripts/devnet-init.sh
```

Prints the cluster, program ID, authority, reward funder, rig price, fee and
all four PDAs **before** sending anything. Then it reads the state back and
verifies every field it set — authority, reward funder, price, fee,
`total_rigs == 0`, `total_reward_funded == 0`, and that each vault PDA exists
and is owned by the program. Any mismatch fails loudly.

Initialization is one-shot: if the config PDA already exists, the script
refuses rather than touching live state.

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
./scripts/devnet-fund.sh 1.0 259200          # must outlive T0 + 48h

CHERRY_TREE_FOUNDER_KEYPAIR=<devnet founder keypair> \
./scripts/devnet-rehearsal.sh activate 60     # right away
./scripts/devnet-rehearsal.sh public 60       # at or after T0 + 48h, fresh wallets A/B
```

`activate` proves:

- a non-founder purchase before T0 is refused (`AccessDenied`);
- the founder purchase writes T0 = its block time, and a second founder
  purchase leaves T0 unchanged;
- a wallet with no domain is refused in the `.skr` phase;
- a founder replant pays no penalty and keeps the planting clock;
- a founder claim inside 24h pays 25%, and a sole planter's penalty stays
  in the reward vault as dust;
- the fee and capital vaults are untouched, and the reward vault is solvent.

`public` proves:

- public planting at weight 100, and a referred first planting at weight
  105 with the referrer bound;
- a second referrer is refused (`ReferrerAlreadyBound`);
- a replant pays no penalty;
- a claim inside 24h pays 25% and only the net leaves the reward vault;
- the reward vault is solvent.

`.skr` / `.sns` acceptance cannot run on devnet: those registries are
mainnet-only. They are covered by `programs/sol-rigs/tests/launch.rs` and
`app/tests/identity.test.ts`.

The `devnet.yml` workflow runs the same thing (`stage: activate`, then
`stage: public` two days later).

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

## Vault behaviour in V1

| Vault | Receives | Pays out |
|---|---|---|
| CAPITAL | non-fee portion of rig purchases | **nothing** |
| REWARD | `fund_rewards` only | user claims and compounds only |
| FEE | disclosed purchase fee | `config.fee_recipient` only, triggered by the authority via `withdraw_fees` |

The capital vault doing nothing is deliberate. The realized-yield / staking
adapter is a separate phase that does not begin until a real devnet
buy → accrue → claim → compound cycle is proven by step 6.

There is no admin path out of the reward or capital vaults, and none is coming
in V1.

---

## Content Security Policy

Tightened and, more importantly, **verified survivable**: four browser tests
drive the embedded path, the standalone path, the blink card and an
interaction with a `securitypolicyviolation` listener attached, so a
directive that breaks the app fails the suite rather than degrading
production quietly.

What ships:

```
default-src 'self'; script-src 'self' 'unsafe-inline';
style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:;
connect-src 'self' <your RPC> <its wss> https://chat.cherry.fun;
frame-ancestors 'self' https://chat.cherry.fun;
frame-src 'none'; object-src 'none'; form-action 'none';
base-uri 'self'; worker-src 'self'; manifest-src 'self';
upgrade-insecure-requests
```

The policy lives in `app/src/lib/csp.ts` and has exactly two consumers:
`next.config.ts`, which serves it as real response headers on a Node
deployment, and `app/scripts/emit-static-headers.ts`, which writes the same
policy into the static export's `_headers` file for a host that reads one
(Cloudflare Pages, Netlify). Two hand-maintained copies of a security policy
is how they drift, so there is one.

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

### Still open: `script-src 'unsafe-inline'`

Next.js inlines a bootstrap script into every document. Removing
`'unsafe-inline'` means serving that script under a per-request nonce, which
forces **every page to render dynamically** — and the embedded path's
prerendered first paint is the single thing the spec is most insistent about.
The trade is not worth making for devnet. Before mainnet, either accept
dynamic rendering for `/` or move the bootstrap behind a hash-based
`script-src`.

## Other pre-mainnet hardening

Tracked, not done, deliberately out of scope for devnet readiness:

- Rate-limit and attribution stores are in-memory; a deployment needs real ones.
- A program upgrade-authority policy (multisig or burn) before any mainnet
  consideration.
