# Production custody: multisig, timelock and immutability (Backlog 12)

**Status:** architecture and read-only audit tooling complete. **No authority has been
transferred and no multisig deployed.** Execution is a founder action on mainnet day.

## 1. Powers that exist in qualified V2

| Power | Holder | What it can do | What it cannot do |
|---|---|---|---|
| BPF upgrade | upgrade authority (ProgramData) | Replace program code — the most powerful key | — |
| `set_paused` | config authority | Stop buy, claim, compound | Move any lamport |
| `set_pricing` | config authority | Change future price / fee (≤ 10%) | Touch accrued balances |
| `set_reward_funder`, `set_fee_recipient` | config authority | Rotate funder / fee destination | Redirect a withdrawal elsewhere |
| `transfer_authority` → `accept_authority` | config authority → new | Two-step handover | — |
| `withdraw_fees` | config authority | Move FEE vault → `config.fee_recipient` only | Touch REWARD or CAPITAL |
| `fund_rewards` | reward funder | Add SOL to REWARD, set the schedule | Remove any SOL |
| founder activation | `config.founder` (pinned, no setter) | First purchase sets T0 | Anything after T0 |

Note: **pause freezes claims.** A single-key config authority could hold users' accrued SOL
hostage indefinitely. This is why config authority must be a multisig with independent humans.

## 2. Target layout

| Role | Custody | Threshold | Separation rule |
|---|---|---|---|
| Upgrade authority | Squads-style multisig vault (PDA) | 2-of-3 at launch → 3-of-5 after audit | Distinct from every other role |
| Config authority | Separate multisig vault | 2-of-3 | ≠ upgrade authority, ≠ funder |
| Reward funder | Separate multisig vault, funded only from eligible sources | 2-of-3 | ≠ config, ≠ upgrade, ≠ fee recipient, ≠ founder |
| Fee recipient | Cold address or multisig vault (destination only, never signs) | n/a | ≠ reward funder (fees must never become reward funding) |
| Founder activation | Founder's Seeker Seed Vault wallet | single | Holds **no** operational authority |

`app/src/lib/launch/expected.ts` currently expects the fee recipient to equal the founder wallet.
`authority-audit` flags that as WARN: acceptable only as a disclosed interim.

## 3. Signer requirements

- Each signer key on its own **hardware wallet** (Ledger/Keystone or Seeker Seed Vault), never
  an online signing file; recovery material stored separately.
- **Independent humans.** A 2-of-3 where one person controls all three keys is operationally
  **single-sig**, whatever the chain shows. At least two different people per multisig; for
  the upgrade authority, at least one signer outside the founder's day-to-day control.
- A signed **custody register** (off-chain, private): role, signer name, device, location
  class, backup holder, date. Reviewed quarterly and on any change.
- No signer holds keys for more than two roles.

## 4. Operational signing flow

1. Proposer drafts the transaction in the multisig UI and posts the **decoded** instruction,
   accounts and intent in the ops channel.
2. Each approver independently decodes on their own device and compares: program ID, instruction,
   arguments, accounts. Approvers never sign a blob they did not decode.
3. Execute; the proposer runs the matching read-only verification (`authority-audit`,
   `runway-report`, `verify-first-buy`, …) and archives proposal, approvals, signature and
   post-state.

## 5. Timelock

- Squads v4 supports a per-multisig **time lock** between approval and execution. Target:
  - upgrade authority: **≥ 48 h at launch**, **≥ 7 days** after the stability period;
  - config authority: **no timelock**. In V2 pause and pricing share one authority, so a
    timelock on pricing would also delay an emergency pause. Pricing / funder / fee-recipient
    changes are public events (`PricingUpdated`, `RewardFunderUpdated`, `FeeRecipientUpdated`)
    and are announced before proposal. Splitting pause from config is Category C.
- Every upgrade is announced when proposed, not when executed.

## 6. Upgrade process

1. Source frozen at a tagged commit; reproducible SBF build (`scripts/build-sbf.sh`, pinned
   toolchain); artifact SHA-256 published.
2. Fresh qualification of the new version on a disposable Devnet ID (as V2 was qualified).
3. Buffer written by an operator; buffer authority set to the upgrade multisig.
4. Proposal to upgrade from buffer; timelock; approvers verify the buffer hash equals the
   published artifact (`scripts/verify-program-dump.sh` logic).
5. Execute; dump and verify the deployed program; archive.

**Emergency upgrade:** same steps with the minimum timelock the multisig allows; used only for
an actively exploited fund-safety bug; publish an incident report within 24 h. Pausing first
(config multisig) is the preferred containment.

## 7. Compromised / lost keys

| Event | Response |
|---|---|
| One signer compromised | Remaining signers rotate the member out (multisig config change), rotate device, review last 30 days of proposals. |
| Threshold compromised on config | Pause is itself in the attacker's hands → upgrade multisig ships a forward fix; publish incident. |
| Threshold compromised on upgrade | Worst case. Pre-agreed public statement; users should claim; no on-chain defence in V2 beyond the timelock window — hence the ≥ 48 h timelock. |
| One signer lost | Rotate member using remaining threshold. A 2-of-3 tolerates one loss; never run with a single remaining backup. |
| Funder compromised | Config multisig `set_reward_funder`. The funder can only add SOL. |

## 8. Path to immutability

Revoke the upgrade authority (`solana program set-upgrade-authority --final`, via the multisig)
only when **all** hold:

1. External audit completed, findings resolved;
2. ≥ 6 months on mainnet with no fund-safety incident;
3. No planned instruction changes (including any CAPITAL policy — see `CAPITAL_FUTURE_POLICY.md`);
4. Advance public notice (≥ 30 days), because revocation also removes the ability to fix bugs.

Config authority can remain (pause/pricing) after revocation; consider transferring it to a
wider multisig at that point.

## 9. Tooling (built, read-only)

- `app/scripts/ops/authority-audit.ts` — reads ProgramData upgrade authority and Config roles,
  checks them against `CHERRY_TREE_EXPECTED_*`, flags on-curve single keys, role overlaps,
  founder overlap, open pending handovers, and always prints the "human separation is not
  checkable" warning. Logic: `app/src/lib/ops/custody.ts`; tests: `app/tests/opsReadiness.test.ts`.
- It never transfers, proposes or accepts anything.

**Founder decisions remaining:** multisig membership (names/devices), thresholds, timelock
lengths, fee-recipient destination, audit timing, revocation date.
