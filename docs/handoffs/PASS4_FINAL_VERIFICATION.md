# THE CHERRY TREE — PASS #4 FINAL VERIFICATION

## Final verdict

**PASS #4 VERIFIED**

Commit `f58be39a1a99364c7f505e42a77d8b9f75ae4bd3` independently satisfies
canonical tests 16–29 for the ownership, encumbrance, and debt-eligibility
foundation. No blocking defect was found. Pass #5 was not started.

## Verified repository state

- Verification checkout: `/home/bronzesneaks/cherry-tree-pass4`
- Verification state: intentionally detached before this report branch was
  created
- Verified HEAD: `f58be39a1a99364c7f505e42a77d8b9f75ae4bd3`
- Expected parent: `c034123d9c96d37476273734f21894c5715f898d`
- Live-ref check: `git fetch origin` completed successfully
- Remote branch: `origin/claude/cherry-tree-pass4-eligibility`
- Remote branch HEAD: exact match with the verified HEAD
- Worktree before report creation: clean; index clean; `git diff --check`
  passed
- Configured origin: `https://github.com/PhantomCapAI/cherry-tree-mining-rig-.git`
- No implementation or production configuration was changed during
  verification

## Scope and design review

The complete Pass #4 diff from its parent was reviewed, including the new
`eligibility-engine` crate, its canonical and property tests, workspace
integration, product disclosure, and documentation.

Verified properties:

- Finalized events are totally ordered by `(slot, transaction index,
  instruction index, inner-instruction index)` with one timestamp per slot,
  monotonic timestamps, half-open intervals, and zero-duration intermediate
  states.
- Token replay covers mint, burn, transfer, delegated transfer, account-owner
  authority changes, approve/revoke, freeze/thaw, stake lifecycle, and
  adapter-reported collateral/vesting encumbrances. Impossible replay events
  fail closed into an unresolved reconciliation window.
- A checkpoint mismatch or declared gap marks the entire affected common-pool
  window unresolved for every participating wallet, then permits
  resynchronization from coherent observed state.
- Recognized units exclude delegation, frozen balances, adapter encumbrance,
  pending unstake, custody double-counting, and duplicate receipts.
- Token-2022 permanent-delegate, transfer-hook, and confidential-transfer
  risk extensions are ineligible unless explicitly adapter-cleared.
- Registry debt is read and netted once at wallet level. Missing, unreadable,
  or version-unpinned registry state is pending; debt above gross equity
  saturates at `X=0`.
- Adapter and debt-program pins fail closed until their finalized observed
  versions match.
- xSOL accepts owned-token NAV and rejects both leveraged underlying notional
  and a plain reference-price basis.
- Only SKR and xSOL can be activated in this pass. Deferred assets and
  leveraged/queued/vault JLP remain refused.
- The new crate has no normal dependencies (`cargo tree -p
  eligibility-engine --edges normal` contains only the crate) and therefore
  no Solana/Anchor linkage.
- No production mint, staking program, registry program, oracle, wallet, or
  threshold was embedded. Those remain explicit gate 14.2-D configuration.
- The non-registry-debt limitation is shared verbatim between the Rust engine
  and product, and is rendered on the production-built page.

## Canonical test matrix

| Test | Requirement | Result |
|---:|---|---|
| 16 | A→B→A across slots attributes only actual holding intervals | PASS |
| 17 | A→B→C within one transaction creates no intermediate duration | PASS |
| 18 | Different slots sharing one timestamp create zero overlap | PASS |
| 19 | `SetAuthority(AccountOwner)` is a whole-account transfer | PASS |
| 20 | Approve reduces recognized units; revoke restores prospectively | PASS |
| 21 | Frozen token accounts are excluded | PASS |
| 22 | Token-2022 risk extensions reject unless adapter-cleared | PASS |
| 23 | Flash-loan-sized intra-transaction balance has zero weight | PASS |
| 24 | Missing transfer reserves the whole common-pool interval | PASS |
| 25 | Pending unstake excluded; wallet/stake units counted once | PASS |
| 26 | Registry debt netted at wallet level; `X` never negative | PASS |
| 27 | Debt increase applies from its event; unreadable state is pending | PASS |
| 28 | Non-registry debt is undetected and disclosed in the product | PASS |
| 29 | Owned xSOL uses owned-token NAV, never underlying notional | PASS |

The canonical Rust file also passed three adjacent scope tests: launch-asset
activation restrictions, adapter encumbrance release behavior, and
registration/exit timing.

## Executable evidence

### Rust

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy -p eligibility-engine --all-targets -- -D warnings` | PASS |
| `cargo test -p eligibility-engine --all-targets` | PASS — 25/25 |
| Canonical/adjacent test binary | PASS — 17/17 |
| Property/adversarial test binary | PASS — 8/8 |
| `cargo test --workspace` | PASS — 312/312 |
| `git diff --check` | PASS |

The broad workspace run emitted the pre-existing Anchor macro `check-cfg`
warnings, two existing `AccountInfo::realloc` deprecation warnings, and a
future-incompatibility notice for `proc-macro-error2`; it had no test failure.

### Product and browser

The Pass #4 checkout had no local `node_modules`. To avoid a network install,
verification reused the existing Pass #3 dependency tree in an isolated
`/tmp` copy after proving its `package-lock.json` SHA-256 exactly matched the
Pass #4 lockfile:

```text
0cfca7d50d9c892bd8c978ba72ca4810c7ca95dd69fbdab2c8078d47130c17ad
```

| Check | Result |
|---|---|
| Focused disclosure Vitest | PASS — 3/3 |
| Full Vitest suite | PASS — 680/680 across 37 files |
| TypeScript `tsc --noEmit` | PASS |
| ESLint | PASS — 0 errors, 13 pre-existing warnings |
| Next.js production build | PASS |
| Focused Playwright disclosure test | PASS — 1/1 |
| Playwright responsive/console-error test | PASS — 1/1 |

The preferred `agent-browser` binary was unavailable, so the repository's
Playwright harness and pinned Chrome for Testing were used for the equivalent
production-rendered checks. The page rendered meaningful content, the debt
disclosure was visible with the required limitation, and the responsive check
reported no browser console or page errors.

## Toolchain

| Tool | Version |
|---|---|
| Rust | `rustc 1.98.1` |
| Cargo | `cargo 1.98.1` |
| Node | `v20.19.2` |
| npm | `9.2.0` |
| Vitest | `5.0.1` |
| Next.js | `16.3.5` |
| Playwright | `1.63.0` |
| Browser | Chrome for Testing `153.0.8010.12` |

## Boundary and deferred work

- Pass #4 is an off-chain eligibility foundation. It does not deploy or wire
  production adapter/indexer configuration.
- Real SKR/xSOL mint and program identities, verified staking layout, approved
  debt-registry contents and decoders, price/NAV sources, and archive-provider
  checkpoint policy remain deliberately deferred behind gate 14.2-D.
- Reference-price commitment, oracle confidence/TWAP rules, and Safe Mode are
  Pass #5 concerns and were not implemented or verified here.
- No merge, deployment, wallet action, authority change, or production
  configuration change occurred.

## Final statement

**PASS #4 VERIFIED**
