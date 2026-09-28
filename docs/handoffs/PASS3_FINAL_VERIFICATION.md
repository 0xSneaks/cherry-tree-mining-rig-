# THE CHERRY TREE — PASS #3 FINAL VERIFICATION

## VERIFIED LOCAL STATE

- Repository: `/home/bronzesneaks/cherry-tree-pass3`
- Branch verified: `claude/cherry-tree-pass3-fees`
- Verified HEAD: `c034123d9c96d37476273734f21894c5715f898d`
- Pass #3 base: `10acc65`
- Remote branch HEAD: exact match with the verified local HEAD.
- `origin/main`: `85ee95545a7eedd5013f46323bb96ca06a5d43e2`, matching expected main.
- Local `main`: `a0ee7e8075bd7fd835b16d5bed2d6b65a388902d`, held by a stale/prunable `/tmp/cherry-publish` worktree record. It was not modified.
- Configured origin: `PhantomCapAI/cherry-tree-mining-rig-`, not the stated `0xSneaks/cherry-tree-mining-rig-`. No remote was rewritten.
- No concurrent Cargo, Rust, Solana, validator, Node, or Git mutation process was found.
- No sub-agents were created.
- The verification checkout was initially clean.
- `git diff --check`: PASS.

Two requested non-protocol cleanup amendments were prepared in the verification worktree after executable verification:

- `docs/tokenomics/CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md`
- `scripts/verify-deploy-ready.sh`

Those amendments remain uncommitted in the verification worktree and are intentionally excluded from this docs-only report branch.

## TOOLCHAIN VERSIONS

| Tool | Version/result |
|---|---|
| Rust | `rustc 1.98.1` |
| Cargo | `cargo 1.98.1` |
| Anchor | `anchor-cli 0.31.2` through the existing pinned AVM binary |
| Solana CLI | `2.3.0 (Agave)` |
| solana-test-validator | `2.3.0` |
| cargo-build-sbf | `4.1.0` |
| SBF platform-tools | Script-pinned `v1.52` |
| Platform Rust/Cargo | `rustc 1.89.0-dev`, `cargo 1.89.0` |
| Node | `v20.19.2` |
| npm | `9.2.0` |
| Playwright | `1.63.0` |
| Chromium | Chrome for Testing `153.0.8010.12` |

Bare `anchor --version` encountered a read-only AVM lock in the verification session, so the existing pinned `anchor-0.31.2` binary was invoked directly. Several JavaScript packages declare Node 22+, but all requested tests and builds passed under Node 20.

## REGRESSION RESULTS

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo test --workspace` | PASS — 287/287 |
| `cargo clippy --workspace --all-targets` | PASS — warnings only |
| Full Vitest | PASS — 677/677 across 36 files |
| TypeScript typecheck | PASS |
| ESLint | PASS — 0 errors, 13 pre-existing warnings |
| Next.js production build | PASS |
| Full Playwright | PASS — 73/73 discovered tests |

The first full Rust run encountered one transient `AccountInUse` result in test 78. Its isolated retry passed, and the required complete workspace rerun then passed all 287 tests.

## SBF BUILD RESULT

Exact command:

```bash
CARGO_BUILD_SBF=/home/bronzesneaks/.local/share/solana/install/releases/stable-e29e5d910f0c2b7176f58174e592e8488099ef75/solana-release/bin/cargo-build-sbf ./scripts/build-sbf.sh
```

Result: PASS.

- Artifact: `/home/bronzesneaks/cherry-tree-pass3/target/deploy/sol_rigs.so`
- Size: `854,536 bytes`
- Expected program ID: `C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt`
- `declare_id!`, Anchor configuration, frontend configuration, and actual SBF loader execution all used that ID.
- No deployment occurred.

## SBF ARTIFACT HASH

```text
b4fe9d4ca359fb4cda2276dd166f7d9a5291cf350727b75f1093ef1bb086cc21
```

## LOCAL VALIDATOR TRANSACTION MATRIX

The isolated repository ProgramTest workflow loaded the actual built `.so` through the SBF loader. It did not contact Devnet or Mainnet.

A focused executable SBF run covered 111 integration tests. Loader logs explicitly showed the built `sol_rigs.so` executing at the expected program address.

| Scenario | Status | Executable evidence |
|---|---|---|
| Purchase fee | PASS | 10% purchase fee charged |
| Distinct compound fee | PASS | 8% compound fee charged |
| Capped compound | PASS | Cap enforced |
| First referral ledger creation | PASS | Ledger created; buyer paid rent |
| Commission reservation | PASS | Reserved balance increased atomically |
| Referral 20/80 split | PASS | 20% reserved, 80% retained |
| 8% retained-margin clamp | PASS | Clamp enforced |
| Valid commission claim | PASS | Exact accrued amount paid |
| Repeat/double claim | PASS | `NothingToClaimCommission`; no second payment |
| Withdrawal versus reserved commission | PASS | Excess withdrawal rejected |
| Fee schedule | PASS | Pending fee recorded |
| Fee cancel | PASS | Pending fee cancelled |
| Application before 72 hours | PASS | Rejected/old fee retained |
| Anyone-can-apply after maturity | PASS | Applied successfully |
| V1 Config migration | PASS | V1 upgraded |
| V2 Config migration | PASS | V2 upgraded |
| Migration with zero price | PASS | Rejected |
| Migration above 1,000 bps | PASS | Rejected |
| Migration at exactly 1,000 bps | PASS | Accepted |
| Repeat migration | PASS | Rejected |
| 30% early-claim tier | PASS | Boundary exercised |
| 20% early-claim tier | PASS | Boundary exercised |
| 10% early-claim tier | PASS | Boundary exercised |
| 0% early-claim tier | PASS | Boundary exercised |
| 60% treasury / 40% redistribution | PASS | Exact split reconciled |

A broader SBF sweep passed 49 tests before a harness-only test attempted to find a nonexistent `hostile_cpi.so`. That test uses a native helper program and is incompatible with global SBF preference; no Cherry Tree SBF assertion failed.

## ACTUAL BALANCE RECONCILIATION

All figures below are lamports and exclude account rent from distributable vault balances.

| State | CAPITAL | REWARD | FEE | Reserved | Withdrawable |
|---|---:|---:|---:|---:|---:|
| Initial | 100,000,000 | 30,000,000,000 | 10,000,000 | 0 | 10,000,000 |
| Referred purchase | 600,000,000 | 30,000,000,000 | 60,000,000 | 10,000,000 | 50,000,000 |
| Compound | 630,000,000 | 29,967,600,000 | 62,400,000 | 10,000,000 | 52,400,000 |
| 30% early claim | 630,000,000 | 29,934,230,904 | 69,225,496 | 10,000,000 | 59,225,496 |
| Fee withdrawal | 630,000,000 | 29,934,230,904 | 10,000,000 | 10,000,000 | 0 |
| Commission claim | 630,000,000 | 29,934,230,904 | 0 | 0 | 0 |
| Repeat claim attempt | 630,000,000 | 29,934,230,904 | 0 | 0 | 0 |

Verified formula at every state:

```text
withdrawable = distributable(FEE) - reserved_commissions
```

Transaction detail:

- Purchase:
  - CAPITAL `+500,000,000`
  - FEE `+50,000,000`
  - Buyer rent: miner `2,081,040`; referral ledger `1,287,600`
  - Commission reserved: `10,000,000`
- Compound:
  - CAPITAL `+30,000,000`
  - FEE `+2,400,000`
  - REWARD `-32,400,000`
- Early claim:
  - Gross: `37,919,428`
  - Claimant net: `26,543,600`
  - Penalty: `11,375,828`
  - Treasury portion: `6,825,496`
  - Redistribution portion: `4,550,332`
  - Redistribution accumulator moved from `2,006,653,824,000,000,000` to `2,017,550,903,905,559,786`
- Withdrawal:
  - `59,225,497` rejected
  - Exact withdrawable `59,225,496` succeeded
  - External treasury balance became `1,059,225,496`
- Commission claim:
  - Ledger accrued `10,000,000`
  - Claimed `10,000,000`
  - Unpaid `0`
  - Referrer received exactly `10,000,000`
  - Repeat claim changed no vault or recipient balance.

CAPITAL increased only through permitted purchase/compound flows and was unchanged by reward claims, fee withdrawal, commission claims, and the rejected repeat claim.

## CANONICAL TEST MATRIX

| Test | Result |
|---|---|
| 6 | PASS |
| 7 | PASS |
| 68 | PASS |
| 69 | PASS |
| 70 | PASS |
| 71 | PASS |
| 72 | PASS |
| 78 | PASS |
| 79 | PASS |
| 80 | PASS |
| 81 | PASS |
| 82 | PASS |
| 83 | PASS |
| 84 | PASS |
| 85 | PASS |
| 86 | PASS |
| 87 | PASS |
| 89 | PARTIAL, as expected for Pass #3 |
| 90 | PASS |
| 91 | PASS |

Test 89 status:

- Tight engine forfeiture bound: PASS.
- On-chain non-zero forfeiture fails closed without lock-allocation state: PASS.
- Positive on-chain 80/20 release: BLOCKED by design and remains unreachable until the later lock-engine pass.
- No Pass #4 implementation was started.

## WALLET CONSISTENCY

| Role | Address | Result |
|---|---|---|
| Founder | `FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp` | Consistent |
| Fee / treasury | `4FsYyXTbF1iftkUTtZVmzmvisxUaGvoYfZDj4DuGLvwF` | Consistent |
| Reward / funding | `Eu866FnHkUHmB6HYiPNz7vNVWbegPmFvmtQRdD9GNBsz` | Consistent |
| Superseded fee wallet | `35DndRT7F5ovtPKADF7SiSoCc22v6meszFcthYeqtAE5` | Not active or referenced |

No Vercel, GitHub, operator, wallet, authority, or production configuration was changed.

## REMAINING FINDINGS

- The ignored local `target/deploy/sol_rigs-keypair.json` resolves to `9XmYJ2dXWwjdqhzsX7dpPYJYicmchtupRw8thG52XuvT`, not the canonical program ID. The built binary itself loaded and executed successfully at the canonical ID, but this keypair mismatch must be resolved before any future deployment.
- The deploy-readiness script warns that SBF tooling is blocked when it uses the bare AVM/default tool discovery path. The canonical manual build using the installed pinned toolchain succeeded.
- The configured origin owner differs from the repository owner stated for the verification, although branch and commit equality were verified against the configured origin.
- Node 20 is below several dependency-declared Node 22 engine ranges, despite the complete JavaScript regression passing.
- Clippy reports warnings but no errors.

## DOCUMENTATION / BRAND CLEANUP RESULT

- Architecture disclosure amendment prepared: claiming before a top-up avoids resetting the timer, but the claim still pays the applicable early-claim fee. Protocol economics were not changed.
- Brand investigation found:
  - Canonical base product name: `THE CHERRY TREE`
  - Intentional display wordmark: `THE CHERRY TREE 🌸`
  - Commit history explicitly restored the blossom wordmark.
- The deploy-readiness script was classified as stale. A non-brand-changing amendment was prepared to validate the intentional decorated display constant while retaining the README check for the canonical base name.
- Deploy-readiness validation passed with the prepared amendment.
- Neither cleanup amendment is included in this report-only branch.

## PASS #3 FINAL VERDICT

PASS #3 VERIFIED
