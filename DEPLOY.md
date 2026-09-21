# THE CHERRY TREE — devnet runbook

Devnet only. Every script in `scripts/` refuses mainnet outright, in two
independent places: the shell guard in `scripts/lib/guard.sh` and again inside
the TypeScript in `app/scripts/ops/common.ts`. Neither can be bypassed by
going around the other.

No script in this repository funds rewards on its own. Deploy does not,
initialize does not. Funding is always a separate, deliberate command.

---

## Roles

Two independent roles. Neither is hardcoded anywhere, and they do **not** have
to be the same key.

| Role | Supplied as | Used by |
|---|---|---|
| **Deployer / program authority** | `ANCHOR_WALLET` — a path to a keypair JSON kept outside this repo | deploy, initialize, `set_paused`, `set_pricing`, `withdraw_fees` |
| **Reward funder** | `CHERRY_TREE_REWARD_FUNDER` — a **public key**; the keypair path goes in `CHERRY_TREE_REWARD_FUNDER_KEYPAIR` | `fund_rewards` only |

The authority cannot fund rewards unless it is also the reward funder, and the
reward funder cannot change config. `set_reward_funder` lets the authority
rotate the funder later; authority handover is two-step
(`transfer_authority` then `accept_authority`), so a typo cannot brick the
program.

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

## 6. Smoke test on real devnet

```bash
./scripts/devnet-smoke.sh 5 5 45     # rigsA rigsB waitSeconds
```

Two distinct wallets, real elapsed time:

```
WALLET A  buy rigs
   wait   (real seconds, real slots)
WALLET B  buy rigs — later
   wait
WALLET A  claim
WALLET B  compound
```

Then it reads chain state and asserts, in exact integer lamports:

1. **Wallet B received no reward attributable to time before it owned rigs.**
   Checked at the mechanism — B's `reward_per_rig_paid` watermark equals the
   accumulator at the moment of purchase and its ore bin is empty — and again
   against everything B has since realised.
2. **`reward vault distributable >= funded - claimed - compounded`.**
3. Purchase lamports landed in CAPITAL and FEE, never REWARD.
4. `claim` paid a positive amount and left the rig count untouched.
5. `compound` minted whole rigs and preserved the exact remainder.

It refuses to run if the two wallet paths resolve to the same key, if the
program is paused, or if the reward schedule has already finished.

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
| FEE | disclosed purchase fee | authority, via `withdraw_fees` |

The capital vault doing nothing is deliberate. The realized-yield / staking
adapter is a separate phase that does not begin until a real devnet
buy → accrue → claim → compound cycle is proven by step 6.

There is no admin path out of the reward or capital vaults, and none is coming
in V1.

---

## Pre-mainnet hardening

Tracked, not done, and deliberately out of scope for devnet readiness:

- **CSP.** `script-src` currently allows `'unsafe-inline' 'unsafe-eval'` and
  `connect-src` allows `https: wss:`. Next.js needs a nonce-based CSP to drop
  `unsafe-inline`, and `connect-src` should be narrowed to the specific RPC
  and JWKS origins once those are fixed. Tightening it needs iteration against
  a real Cherry host, so it is not worth destabilising devnet readiness for.
- Rate-limit and attribution stores are in-memory; a deployment needs real ones.
- A program upgrade-authority policy (multisig or burn) before any mainnet
  consideration.
