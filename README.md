# 🌸 THE CHERRY TREE

### Plant. Earn. Replant. Grow.

**THE CHERRY TREE** is a Solana-native yield game built for the Cherry ecosystem.

You plant **Seeds**.  
Your Seeds generate a share of the funded SOL reward stream.  
You can harvest your SOL — or **replant it into more Seeds and compound your production.**

The idea is simple:

```text
PLANT → EARN SOL → REPLANT → GROW PRODUCTION → EARN MORE
```

Instead of leaving rewards sitting idle, THE CHERRY TREE is built around a compounding-first game loop where earned SOL can increase a player's productive position.

The longer the Tree grows, the more interesting the strategy becomes.

---

## 🌱 The Compounding Flywheel

The heart of THE CHERRY TREE is **replanting**.

When a player earns enough SOL to purchase additional Seeds, those rewards can be compounded directly back into the position.

```text
10 Seeds
   ↓
Earn SOL
   ↓
Replant rewards
   ↓
More Seeds
   ↓
Larger share of production
   ↓
More SOL
   ↓
Repeat
```

Every successful replant increases the number of Seeds producing for that wallet.

That means players have two fundamentally different ways to use their rewards:

### 🍒 Harvest

Claim earned SOL back to your wallet.

### 🌳 Replant

Use earned rewards to acquire additional Seeds and increase your productive position.

Replanting is executed through the protocol's compound instruction rather than requiring a separate withdrawal and repurchase flow.

Unused reward dust remains credited to the miner after the compound operation.

---

## ⚡ Auto-Compounding

THE CHERRY TREE is being designed around an **opt-in auto-compounding strategy layer**.

The goal is simple:

> Let players choose a replant cadence and allow the Tree to keep growing whenever enough earned SOL exists to acquire another Seed.

Instead of manually checking the miner throughout the day, a player could choose a strategy such as:

```text
Earn
  ↓
Threshold reached
  ↓
Replant
  ↓
More Seeds
  ↓
Higher productive share
  ↓
Repeat
```

The current protocol already contains the underlying **replant / compound engine** required for this loop.

Automated execution is treated separately from the underlying accounting and will only be described as live once the opt-in execution layer is deployed.

**Current:** user-triggered compounding  
**Designed direction:** configurable opt-in auto-compounding

No guaranteed return is implied. Reward output depends on the funded reward schedule, total network participation, Seed weight, protocol configuration, and time.

---

# 🍒 Why Compound?

A player who harvests takes SOL out of the Tree.

A player who replants uses earned SOL to increase their productive position.

That creates an intentionally different strategy.

```text
HARVEST
Earn SOL → Wallet

REPLANT
Earn SOL → More Seeds → Larger productive position
```

Compounding does not create free SOL.

It changes what the player owns inside the game.

The reward stream remains fully funded separately from Seed purchases.

---

# 💰 Real SOL Rewards

THE CHERRY TREE is not a fake balance counter.

Rewards are denominated in **SOL** and backed by a dedicated reward vault.

The protocol separates three different flows:

| Vault | Purpose |
|---|---|
| **REWARD** | Funds player rewards and compounding |
| **CAPITAL** | Receives the non-fee portion of Seed purchases |
| **FEE** | Receives the disclosed protocol fee |

This separation matters.

```text
BUYING SEEDS ≠ FUNDING EXISTING REWARDS
```

Seed purchases do not secretly pay earlier players.

Rewards must exist inside the funded reward vault before they can be distributed.

The protocol tracks reward liabilities independently from purchase capital.

---

# 🌳 Your Share Grows With Your Tree

Players participate in the funded reward stream according to their productive share.

More Seeds mean more productive weight.

That creates a competitive growth loop:

```text
PLANT EARLY
     ↓
BUILD SEEDS
     ↓
REPLANT REWARDS
     ↓
INCREASE SHARE
     ↓
GROW THE TREE
```

But the system is not static.

As other players add Seeds, the total network share changes.

Your displayed production therefore reflects your share of the active reward schedule rather than a guaranteed fixed return.

---

# ⏳ Harvest vs. Growth

THE CHERRY TREE rewards patience.

Fresh planting begins a **72-hour harvest penalty window**.

As the position becomes more rooted, that penalty decreases until reaching the fully rooted state.

Replanting existing rewards does **not** restart that timer.

Fresh purchases do.

This creates another strategic choice:

```text
HARVEST NOW
vs.
LET IT ROOT
vs.
REPLANT AND GROW
```

The miner cockpit exposes the current state so players can make that decision themselves.

---

# 🤝 Referral Growth

THE CHERRY TREE includes an on-chain-aware referral system built around actual planter activity.

Referral attribution is bound according to protocol rules rather than simply trusting a URL parameter.

That gives the project another growth loop:

```text
PLANT
  ↓
SHARE
  ↓
BRING NEW PLANTERS
  ↓
GROW THE GROVE
```

Referral mechanics are separate from the core reward accounting.

---

# 🍒 Built for Cherry

THE CHERRY TREE is designed as a native **Cherry mini-app**.

Inside Cherry, the goal is to remove as much friction as possible:

- Cherry wallet context
- no unnecessary wallet-connect screen
- native transaction signing
- room-aware leaderboards
- sharing
- referrals
- live production display
- Seed management
- harvest controls
- replant / compounding
- transparent reward runway

The miner can also run in standalone mode through supported Solana wallet infrastructure.

---

# 🔥 The Loop

THE CHERRY TREE is deliberately easy to understand.

### 1. Plant Seeds

Acquire productive capacity.

### 2. Seeds Earn

Your position receives its proportional share of the funded SOL reward schedule.

### 3. Choose

Harvest the SOL or put it back to work.

### 4. Replant

Convert eligible earned rewards into additional Seeds.

### 5. Compound

More Seeds increase your productive position.

### 6. Grow

Repeat.

```text
🌱 → 🌿 → 🌳 → 🍒
```

That loop is THE CHERRY TREE.

---

# 🔐 Built Around Solvency

The reward system is designed around a simple principle:

> The Tree should never promise SOL that has not been funded.

The protocol maintains separate accounting for:

- funded rewards
- claimed rewards
- compounded rewards
- Seed capital
- protocol fees
- outstanding reward liabilities

Reward calculations use integer lamport arithmetic.

There is no floating-point math in the protocol money path.

Rounding truncates toward the vault rather than creating value for the user.

---

# 🏦 Vault Architecture

```text
                    ┌──────────────┐
                    │   PLANTER    │
                    └──────┬───────┘
                           │
                       Buy Seeds
                           │
             ┌─────────────┴─────────────┐
             ↓                           ↓
      ┌─────────────┐             ┌─────────────┐
      │   CAPITAL   │             │     FEE     │
      │    VAULT    │             │    VAULT    │
      └─────────────┘             └─────────────┘


      Independent funded stream
                 ↓
        ┌─────────────────┐
        │  REWARD VAULT   │
        └────────┬────────┘
                 │
          ┌──────┴──────┐
          ↓             ↓
       HARVEST        REPLANT
          │             │
       Wallet        More Seeds
```

The reward vault cannot be treated as general protocol capital.

That separation is one of the core architectural properties of the project.

---

# 🧠 Protocol Architecture

```text
crates/rigs-engine/
```

Deterministic economic and reward accounting.

```text
programs/sol-rigs/
```

Anchor program, account validation, vault management and SOL movement.

```text
app/
```

Next.js Cherry mini-app and standalone frontend.

```text
app/scripts/ops/
```

Deployment, funding, runway, initialization and operational tooling.

```text
DEPLOY.md
```

Deployment runbook.

```text
LAUNCH.md
```

Launch operations.

```text
YIELD.md
```

Reward and yield-model documentation.

The public product uses **Seeds / planting / replanting** terminology.

Some internal crates, instructions and account names retain the original `rigs` terminology because changing deployed technical identifiers unnecessarily can affect compatibility.

---

# 🧪 Verification

The protocol has dedicated coverage for the economic properties that matter most:

- new Seeds cannot collect rewards from before they existed
- rewards divide according to productive share
- claims cannot be double-spent
- replanting only purchases whole affordable Seeds
- compound accounting moves the correct SOL
- reward liabilities remain backed
- funding during an active schedule rolls correctly
- rewards stop when the funded schedule ends
- Seed purchases cannot fund the reward vault
- protocol authority cannot drain reward capital through the normal admin path
- vault substitution attacks are rejected
- arithmetic errors instead of silently overflowing
- UI pricing is checked against protocol pricing
- wallet disconnects disable actions without destroying state
- Cherry lifecycle suspension re-anchors against chain state

The objective is not simply to make the miner look alive.

The objective is for the accounting underneath it to remain understandable and verifiable.

---

# ⚠️ What THE CHERRY TREE Is Not

THE CHERRY TREE does **not** proof-of-work mine SOL.

Seeds do not participate in Solana consensus and do not cryptographically create new SOL.

"Mining," "Seeds," "planting," and "growing" describe the product's game mechanics.

Seed purchases acquire non-redeemable productive game capacity.

They are not deposits.

Rewards come from a separately funded SOL reward stream.

There are no guaranteed profits or guaranteed APYs.

---

# 🌸 The Vision

Most yield products end at:

> Deposit → wait → withdraw.

THE CHERRY TREE is built around something more active:

> **Plant → earn → replant → compound → grow.**

Cherry gives the project a social layer.

Solana gives it programmable settlement.

The reward vault gives it a funded economic foundation.

Compounding gives players a reason to keep building their Tree.

Referrals give the Grove a reason to spread.

And every new Seed turns the miner from a passive reward screen into a strategy game.

**Plant your first Seed.  
Grow your Tree.  
Replant the harvest.  
Let compounding do what compounding does.**

---

## Documentation

See:

- [`DEPLOY.md`](DEPLOY.md) — deployment and operations
- [`LAUNCH.md`](LAUNCH.md) — launch procedures
- [`YIELD.md`](YIELD.md) — reward economics
- [`CHERRY.md`](CHERRY.md) — Cherry integration

---

## Licence

**Source-available, not open source.**

THE CHERRY TREE is licensed under the [Business Source License 1.1](LICENSE).

The source may be reviewed, audited, studied and run locally under the terms of that licence.

Public deployment or commercial operation may require a separate commercial licence.

See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE) for complete terms.

**THE CHERRY TREE**, its wordmark and associated branding remain trademarks of PhantomCapAI.
