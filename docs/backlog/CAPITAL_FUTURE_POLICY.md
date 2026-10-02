# CAPITAL — launch policy and the safest future model (Backlog 11)

**Category C for any change.** Nothing here changes qualified V2 behaviour.

## 1. Launch policy (qualified V2, as built)

- CAPITAL receives the non-fee portion of purchases and **pays nothing**.
- No withdrawal, reward-support or treasury instruction exists (`t15`).
- Seeds are non-redeemable production capacity, not principal.
- Correct disclosure: **"Locked in this program version."** Never "permanently locked" while an
  upgrade authority exists — an upgrade could add a path. The honest statement is that any path
  would require a new, disclosed, time-locked, requalified program version.

## 2. Threat model

| Threat | Why it matters |
|---|---|
| CAPITAL → REWARD | Converts the product into a deposit-funded miner: new buyers pay old ones; insolvent by construction when inflows slow. |
| Price support / buyback loops funded by CAPITAL | Same loop, hidden behind market mechanics. |
| Silent withdrawal via upgrade | Upgrade authority can replace code; without a timelock users cannot exit or object first. |
| Retroactive re-purposing | Buyers purchased under "pays nothing"; changing that after the fact changes the deal. |
| Insider extraction | A single key or colluding signers draining a large, visible pool. |
| Governance capture | Token/vote systems captured by the largest holders — often the same concentrated wallets. |

## 3. Allowed-use matrix (only in a future, requalified version)

| Use | Allowed? | Conditions |
|---|---|---|
| Stay locked forever (no instruction) | **Yes — default and recommended** | Revoke upgrade authority eventually → "locked" becomes provably permanent. |
| Protocol security audits, bug bounties | Possibly | Capped per period, timelocked, named payee class, published invoice. |
| Burn / send to an unspendable address | Possibly | Irreversible; disclosed in advance. |
| Charitable / ecosystem grant with public recipient | Possibly | Capped, timelocked, recipient disclosed before execution. |

## 4. Forbidden-use matrix (in every version)

| Use | Forbidden |
|---|---|
| Funding REWARD, directly or via any intermediary wallet | ✔ |
| Paying existing miners, referrers or season prizes | ✔ |
| Market-making, price support, buybacks, LP provisioning | ✔ |
| Operating expenses / salaries / founder compensation | ✔ |
| Collateral, lending, yield strategies (staking adapter included) | ✔ |
| Any withdrawal not preceded by the full governance process | ✔ |

## 5. Safest future architecture (if a use is ever adopted)

1. **Default: never.** Keep CAPITAL instruction-less and plan to revoke the upgrade authority.
2. If a narrow use is adopted, a new program version adds **one** instruction:
   `release_capital(amount, destination_class)` that only executes a **queued** release:
   - `queue_release` by a CAPITAL-governance multisig distinct from config, funder and upgrade
     authorities (≥ 3-of-5, independent humans);
   - on-chain **timelock ≥ 30 days** between queue and execute; cancellable by any 2 signers;
   - hard **per-period cap** (e.g. ≤ 2% of CAPITAL per 90 days) enforced in code;
   - destination restricted to an allow-listed class fixed in code (burn, audit escrow);
   - REWARD vault address rejected as a destination in code; a test proves no instruction
     sequence moves CAPITAL lamports into REWARD;
   - `CapitalReleaseQueued` / `Executed` / `Cancelled` events for public audit.
3. Ship only with: advance disclosure (≥ 30 days before deployment), a narrow written use
   policy, external audit, and fresh qualification of the new version.

## 6. Disclosure, audit and emergency

- **Disclosure:** public notice before the upgrade is proposed; the policy text; the timelock;
  the multisig members' roles; how to verify on chain.
- **Auditability:** events + `runway-report`-style read-only tool that reconciles CAPITAL
  balance against `total_capital_received` and released amounts.
- **Emergency:** the only emergency action on CAPITAL is to **do nothing**. A compromised
  governance key → cancel queued releases within the timelock and rotate signers. There is no
  "emergency withdrawal".

**Founder decision remaining:** whether CAPITAL should ever have any use (recommendation: no),
and when to revoke the upgrade authority to make the lock provable.
