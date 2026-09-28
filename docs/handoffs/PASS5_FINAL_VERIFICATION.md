# THE CHERRY TREE — Pass #5 Independent Verification (Pricing + Safe Mode)

- Verifier: Claude (Claude Code, Opus 5.5)
- Date: 2026-09-28
- Scope: independent verification of the Z.ai Pass #5 implementation against
  `docs/tokenomics/CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md` (§5 valuation
  price / adapter pinning, §9 Safe Mode, §14.3 canonical tests 30–39)
- This file is the only change on branch `claude/pass5-verification-report`.
  The Pass #5 implementation was not modified.

**FINAL VERDICT: PASS #5 NOT VERIFIED**. The details are in §12.

---

## 1. Verified worktree state

| Item | Observed |
|---|---|
| Worktree | `/home/bronzesneaks/cherry-tree-pass5-verify` |
| HEAD | `bb7139470d6b9b4149fc147cc1be1202280eefc7` (detached) |
| HEAD commit | `feat: pricing and safe mode foundation` |
| Parent (`HEAD^`) | `f58be39a1a99364c7f505e42a77d8b9f75ae4bd3` (verified Pass #4) |
| Implementation branch | `z-ai/cherry-tree-pass5-pricing-safe-mode` |
| `origin/z-ai/cherry-tree-pass5-pricing-safe-mode` after `git fetch origin` | `bb7139470d6b9b4149fc147cc1be1202280eefc7` (equal to HEAD) |
| `git ls-remote origin z-ai/cherry-tree-pass5-pricing-safe-mode` | `bb7139470d6b9b4149fc147cc1be1202280eefc7` (equal) |
| `git status --porcelain` at start | clean (`## HEAD (no branch)`) |
| Related worktrees | `/home/bronzesneaks/cherry-tree-pass5` is on `bb71394 [z-ai/cherry-tree-pass5-pricing-safe-mode]`, and this verify worktree is on `bb71394 (detached HEAD)`. The Pass #4 verify worktree `/tmp/cherry-tree-pass4-verify.L2M1GD` is on `f58be39` (prunable). |
| Concurrent modification | The process list showed no process working in this checkout. |

## 2. Diff boundary reviewed

`f58be39a1a99364c7f505e42a77d8b9f75ae4bd3..bb7139470d6b9b4149fc147cc1be1202280eefc7`

17 files changed, 4328 insertions(+), 85 deletions(-):

```
app/src/lib/safeMode.ts                            | 268 +++
app/tests/safeMode.test.ts                         | 230 +++
crates/eligibility-engine/src/consent.rs           | 357 +++
crates/eligibility-engine/src/error.rs             |   8 +
crates/eligibility-engine/src/lib.rs               |  53 +-
crates/eligibility-engine/src/lots.rs              | 508 +++
crates/eligibility-engine/src/pricing.rs           | 370 +++
crates/eligibility-engine/src/rational.rs          | 124 +++
crates/eligibility-engine/src/replay.rs            | 361 ++-
crates/eligibility-engine/src/safe_mode.rs         | 490 +++
crates/eligibility-engine/src/types.rs             |  29 +
crates/eligibility-engine/src/valuation.rs         |  40 +-
crates/eligibility-engine/tests/canonical.rs       |  35 +-
crates/eligibility-engine/tests/common/mod.rs      |  83 +-
crates/eligibility-engine/tests/pricing_safe_mode.rs | 875 +++
crates/eligibility-engine/tests/properties.rs      |   6 +-
crates/eligibility-engine/tests/safe_mode_properties.rs | 576 +++
```

- The diff touches no on-chain program source (`programs/`).
- I confirmed that the files named in the pre-existing Mimosa advisories are **not** in the diff. `git diff --quiet f58be39..bb71394 -- <file>` reported each of these unchanged:
  - `app/src/proxy.ts`
  - `app/src/lib/server/growthConfig.ts`
  - `app/scripts/ops/runway-report.ts`
  - `app/src/lib/csp.ts`
- I did not weaken or disable any security tooling.

Asset scope:
- `AssetKind::launch_candidate()` (`crates/eligibility-engine/src/adapter.rs:30`) admits only `Skr | XSol`.
- `AdapterConfig::validate` rejects `LeveragedJlp` with `AssetExcluded`. It rejects JitoSOL, SpotJlp, xBTC and xHYPE with `AssetNotEnabled`.
- Pass #5 activates none of the deferred or excluded assets.

## 3. Commands executed

All commands ran from `/home/bronzesneaks/cherry-tree-pass5-verify` unless another directory is shown.

**Pre-flight / state**
```
pwd
git rev-parse HEAD
git rev-parse HEAD^
git status --porcelain=v1 -b
git fetch origin
git rev-parse origin/z-ai/cherry-tree-pass5-pricing-safe-mode
git worktree list
git diff --stat f58be39..bb71394
for f in app/src/proxy.ts app/src/lib/server/growthConfig.ts app/scripts/ops/runway-report.ts app/src/lib/csp.ts; do git diff --quiet f58be39..bb71394 -- $f && echo "$f unchanged" || echo "$f CHANGED"; done
git ls-remote origin z-ai/cherry-tree-pass5-pricing-safe-mode
ps aux | grep -E "cargo|node|vitest|next" | grep -v grep | grep -i pass5
```

**Canonical source and code review (read-only)**
```
ls docs/tokenomics/ ; grep -n "^#" docs/tokenomics/CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md
sed -n 173,222p  docs/tokenomics/CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md   # §5
sed -n 417,442p  docs/tokenomics/CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md   # §9
sed -n 663,780p  docs/tokenomics/CHERRY_TREE_FINAL_TOKENOMICS_ARCHITECTURE.md   # §14.3 tests 30–39
cat crates/eligibility-engine/src/{rational,pricing,safe_mode,consent,lots}.rs
git diff f58be39..bb71394 -- crates/eligibility-engine/src/{error,types,valuation,lib,replay}.rs
grep/sed on crates/eligibility-engine/src/{adapter,valuation,replay}.rs   # required_basis, ProgramVersion handling, launch_candidate
sed -n 1,875p crates/eligibility-engine/tests/pricing_safe_mode.rs
grep/sed on crates/eligibility-engine/tests/common/mod.rs
git diff f58be39..bb71394 -- crates/eligibility-engine/tests/canonical.rs crates/eligibility-engine/tests/properties.rs
cat app/src/lib/safeMode.ts ; grep -rn safeMode app/src
cat crates/eligibility-engine/Cargo.toml
```

**Rust regression (background; logs were written to the session scratchpad)**
```
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets
```

**App regression** (from `app/`)
```
npm ci --no-audit --no-fund          # node_modules absent; installed from package-lock.json (gitignored output)
npx vitest run                       # run 1 (concurrent with the Rust build)
npx vitest run                       # run 2 (after the Rust build finished)
npx vitest run                       # run 3 (idle machine)
npx tsc --noEmit
npx eslint .
npx next build
```

**Adversarial probes (scratch copy only; see §7)**
```
cp -r crates/eligibility-engine  <scratchpad>/ws/crates/
cp app/src/lib/safeMode.ts        <scratchpad>/ws/app/src/lib/
# wrote <scratchpad>/ws/crates/eligibility-engine/tests/zz_verify_adversarial.rs
CARGO_TARGET_DIR=<scratchpad>/target cargo test --test zz_verify_adversarial -- --nocapture --test-threads=1
CARGO_TARGET_DIR=<scratchpad>/target cargo test --release --test zz_verify_adversarial -- --nocapture
```
`<scratchpad>` = `/tmp/claude-1000/-home-bronzesneaks-cherry-tree-pass5-verify/478976ff-ff91-4655-82be-2acc38cd8948/scratchpad`, which is outside the repository.

**Report commit (this file)**
```
git switch -c claude/pass5-verification-report      # from bb71394
git add docs/handoffs/PASS5_FINAL_VERIFICATION.md
git commit ; git push -u origin claude/pass5-verification-report
```

## 4. Actual test counts and results

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test --workspace` | exit 0. **355 passed, 0 failed, 0 ignored**, summed over 34 `test result:` lines. |
| `cargo clippy --workspace --all-targets` | exit 0. The warnings come only from `sol-rigs` (39 lib warnings, 39 lib-test duplicates), a crate this pass didn't change. `eligibility-engine` produced **no** clippy warnings. There is also a future-incompat note for `proc-macro-error2 v2.0.1`. |
| Vitest run 1 | 2 failed / 710 passed (712) in 2 files. This run was concurrent with the Solana workspace compile, which was saturating the CPU. |
| Vitest run 2 | **38 files passed, 712/712 passed** |
| Vitest run 3 (idle) | **38 files passed, 712/712 passed** |
| `tsc --noEmit` | exit 0 |
| ESLint | **0 errors, 13 warnings** |
| `next build` | exit 0 (route table emitted) |
| Playwright | not run (§14) |

I didn't capture which two tests failed in run 1. The failures did not reproduce in two later runs. I attribute them to timeouts under load, but that isn't proven.

## 5. Canonical tests 30–39

The canonical tests themselves all pass. The classification below is for the **requirement** each test covers, after my adversarial probes.

| # | Requirement | Classification | Notes |
|---|---|---|---|
| 30 | Post-reference additions use `min(ref, event price)` | **PARTIAL** | The test passes, but two gaps remain. F1: late registration leaves post-reference Addition lots above the registration-time price. F2: the "event price" can be an observation of any age, including one from before the epoch. |
| 31 | One-block manipulated reference rejected | **PASS** | LOW caveats F4 and F5 apply. |
| 32 | No committed reference → INACTIVE, weight 0, others settle | **PASS** | |
| 33 | Pinned upgrade → UNKNOWN from the upgrade slot, reserved | **PASS** | The in-test comment "A later re-observation of the pinned version restores recognition" is not actually exercised, because the test re-observes V2. My probe confirmed that re-observing V1 does restore recognition going forward. |
| 34 | WARM/HOT consent binding and replay protection | **PASS** | |
| 35 | HOT + unavailable route rejected even with consent | **PARTIAL** | HOT and EXTREME are correct. WARM and NORMAL never check the route (F3), and the test at `tests/pricing_safe_mode.rs:603` asserts `Warm + halted + valid consent → PermittedWithConsent`. |
| 36 | EXTREME reset prospective only; settled R/Q unchanged; xSOL and non-xSOL | **PASS** | |
| 37 | Hysteresis prevents repeated reset/release; unset dwell disables release | **PASS** | Passes at the `ExtremeTracker` level. The tracker is not wired into the replay (see INFO-6). |
| 38 | CR = 1 → no finite leverage | **PASS** | |
| 39 | DEX quote failure alone ≠ route unavailable | **PASS** | |

## 6. Adversarial cases inspected or executed

**Executed** (scratch probe file, §7; all 11 probe tests ran):

| Case | Result |
|---|---|
| CR exactly at the 1.35, 1.20 and 1.00 boundaries | NORMAL, WARM, HOT ✔ |
| One step above and below each boundary | 1.350001 → NORMAL; 1.349999 → WARM; 1.200001 → WARM; 1.199999 → HOT; 1.000001 → HOT; 0.999999 → EXTREME ✔ |
| Unnormalized ratios | 27/20 → NORMAL; 6/5 → WARM ✔ |
| CR = 0 | EXTREME ✔ |
| Zero denominator on CR | UNKNOWN ✔ |
| Missing CR (`None`) | UNKNOWN ✔ |
| `u128::MAX` CR | `Err(MathOverflow)` (fails closed) ✔ |
| Leverage at `u128::MAX/u128::MAX` (= 1) | `None` ✔ |
| Inverted or zero-denominator Safe Mode boundaries | `validate()` returns Err ✔ |
| Zero-denominator price sample; empty window | rejected ✔ |
| `u128::MAX` samples in the TWAP | `Err(MathOverflow)` ✔ |
| Late reference commit (`committed_at >= epoch_start`) | `LateReferenceCommit` ✔ |
| Commit with price 0/0 | rejected ✔ |
| Duplicate reference commit for the same mint | the last one **silently overwrites** (INFO-1) |
| Observation after the addition in the same slot | not used; the addition becomes `PENDING(AdditionUnpriced)` ✔ |
| Two observations in the same slot before the addition | the later (lower) one applies ✔ |
| Price above the reference | capped at the reference (canonical t30) ✔ |
| Price below the reference | event price used ✔ |
| Wrong-basis observation (NAV basis on SKR) | ignored, and the addition stays unpriced ✔ |
| Zero-denominator observation | ignored, and the addition stays unpriced ✔ |
| xSOL reference with `UnderlyingNotional` or spot `Reference` basis | `Ineligible(InvalidPriceBasis)` ✔ |
| xSOL `EmergencyNav` with notional or spot basis | `Pending(NavUnverified)`; the earlier segment stays at NAV 3 ✔ |
| xSOL EXTREME → recover higher → EXTREME (NAV 3→1→5→0.5) | each step applies only going forward; equity-seconds = `3000*350 + 1000*100 + 5000*100 + 500*100` ✔ (a higher recovered value is allowed) |
| Pinned upgrade V1→V2, then V1 re-observed | UNKNOWN for [400,500), verified again from 500, pending_seconds = 100 ✔ |
| Configured pin with no finalized observation | `Pending(AdapterProgramUnknown)` (a configured version alone never establishes trust) ✔ |
| Hysteresis: timestamps going backwards | never confirms recovery ✔ |
| Hysteresis: dwell = `Some(0)` | 10 resets and 10 releases over 10 oscillations, i.e. no damping (INFO-5) |
| **F1**: late registration after a post-reference addition | **defect confirmed** (see §8) |
| **F2**: observation from before the epoch prices an addition about 11.6 days later | **defect confirmed** (§8) |
| **F3**: NORMAL or WARM + halted route + valid consent | **defect confirmed** (§8) |
| **F4**: confidence of 90% of mid on a small unit price vs 1% of mid on a large one | **defect confirmed** (§8) |
| **F5**: `min_distinct_slots=1, min_window_seconds=0` with one spot sample; `max_spread` with a zero denominator and a 45× outlier | **defect confirmed** (§8) |
| TWAP with `window_end` far past the last sample | the last sample dominates (2.0999 vs samples 2.00/2.00/2.10) (INFO-4) |

**Covered by the implementation's own tests, which I inspected and ran:**
- consent field mutation for wallet, asset, action, scope, chain, epoch and version
- nonce replay
- expired consent
- invalid signature
- missing consent
- valid consent accepted
- nonce burned only on success
- route halt and resume in the replay (t36)
- DEX quote failure with a healthy protocol route (t39)
- rapid oscillation with dwell unset and set (t37)
- SKR depeg reset (t36)
- EmergencyNav oscillation in the replay, including upward recovery (t37)

**Inspected by reading the code (not executed separately):**
- consent payload field coverage
- `AcceptAllVerifier` export
- TS/Rust payload parity
- FIFO valuation with delegation
- `ReferenceCommit` public fields
- `EpochReferences` has no epoch ID
- replay applies `EmergencyNav` without consulting `ExtremeTracker`

## 7. Temporary verification tests

- **File:** `<scratchpad>/ws/crates/eligibility-engine/tests/zz_verify_adversarial.rs`. It was created in a **scratch copy** of the crate outside the repository, together with a copy of `app/src/lib/safeMode.ts` for the crate's `include`-by-path tests.
- **Repository:** no temporary test was ever written inside the repository or the verification worktree. The worktree stayed clean throughout (§13).
- **Contents:** the full probe source is reproduced in Appendix A so the results can be re-run independently. To reproduce, copy the file into `crates/eligibility-engine/tests/` of a scratch checkout of `bb71394` and run `cargo test --test zz_verify_adversarial -- --nocapture`. The F5 zero-denominator probe needs `--release`, because a debug build hits `debug_assert!` in `rational.rs:37` and panics.
- **Status:** the scratch copy was not removed and remains in the session scratchpad. It is not part of any commit.

## 8. Security findings

No BLOCKER. No HIGH.

### F1: MEDIUM. Late registration does not cap post-reference Addition lots
- **Where:**
  - `crates/eligibility-engine/src/lots.rs:154` (`rebase_registered`) and `:164` (`if lot.basis == LotBasis::Reference`)
  - caller `crates/eligibility-engine/src/replay.rs:534`
- **Spec (§5):** "Units added **or registered** after [the reference] are valued at `min(reference price, verified price at the addition event)`."
- **Defect:** on late registration only Reference-basis lots are re-priced. Addition lots keep their own acquisition price, even when that price is above the price at registration.
- **Probe:**
  - Reference 2, epoch start 100.
  - C sends 500 units to B at t=200 while the observed price is 2, so B gets an Addition lot at 2.
  - The price drops to 1 at t=300 and B registers at t=350.
  - Expected under the spec: 800·1 + 500·1 = **1300**. Actual: **1800**.
  - Control: the same B without the transfer gets 800 (correct).
- **Impact:** a holder can keep units priced above the registration-time price by moving them after the epoch starts and registering later. Value can't exceed the reference, but this bypasses the §5 rule and gives unequal treatment.

### F2: MEDIUM. No staleness bound on the addition "event price"
- **Where:**
  - `crates/eligibility-engine/src/replay.rs:235` (`observation_for`), `:245`–`:258` (`acquisition`), `:644` (`observations.insert`)
  - same pattern at `:534` for registration
- **Spec:** the price used must be the verified price *at the addition event*. Stale data is UNKNOWN (§9), and a missing event price must not silently fall back to the higher reference.
- **Defect:** the replay keeps only the most recent `PriceObserved` per mint and applies it to any later addition, with no maximum age and no check that it falls inside the epoch.
- **Probe:** an observation at t=60 (before epoch start 100, equal to the reference 2), then a mint of 500 at t=1,000,000 with no fresh observation. Result: `Verified { gross 3000 }`, meaning the addition was priced at 2, instead of `Pending(AdditionUnpriced)`.
- **Impact:** in effect this falls back silently to an old price, possibly the reference. The "no weight at the pre-drop price" protection then depends entirely on the indexer emitting observations often enough, and the engine does not enforce it.

### F3: MEDIUM. WARM and NORMAL entry skip the route-availability check
- **Where:**
  - `crates/eligibility-engine/src/safe_mode.rs:191` (Normal → `Permitted`) and `:193` (Warm validates consent only). The route is checked only at `:198` for Hot and Extreme.
  - TS mirror `app/src/lib/safeMode.ts:253` (`case 'warm'`), same logic.
  - The test locks the behavior in at `crates/eligibility-engine/tests/pricing_safe_mode.rs:603`.
- **Spec:**
  - §9: "an unavailable operation cannot be bypassed with a signature".
  - The pass requirement: "A valid consent signature must NOT bypass a genuinely unavailable requested operation".
  - The product's own `SAFE_MODE_CONSENT_DISCLOSURE`: "it can never unlock an operation whose required protocol route is unavailable".
- **Probe:** `authorize_entry(Warm, …, valid consent, RouteCheck::halted())` → `PermittedWithConsent`. With `Normal` → `Permitted`.
- **Impact:** the risk layer authorizes operations whose protocol route is halted, which contradicts the spec and the user-facing disclosure. Replay recognition is separately protected, because `RouteHalt` makes the wallet PENDING, so settled accounting is not affected.

### F4: LOW. Oracle confidence compared as an absolute value against a relative bound
- **Where:** `crates/eligibility-engine/src/pricing.rs:89`. `max_confidence` is documented as "relative to the mid", but the check compares the absolute half-width `c_num/c_den` directly.
- **Probe:**
  - mid 1/1000 lamports per unit with confidence 9/10000 (90% of mid) → **accepted**
  - mid 1000 with confidence 10 (1% of mid) → rejected
- **Impact:** the check fails open when the unit price is below 1 lamport per base unit and is overly strict above that.
- **Related:** the comment `50% of mid` at `pricing.rs:346` (and in the canonical test) is wrong; 0.1 on a mid of 2 is 5%.

### F5: LOW. `ReferenceValidation` configuration is never validated
- **Where:** `crates/eligibility-engine/src/pricing.rs:41` (struct) and `:54` (`check`). There's no constructor or validation.
- **Probes:**
  - `min_distinct_slots=1, min_window_seconds=0` lets a single spot sample commit (`Ok(300/60)`), which §5 prohibits.
  - `max_spread` or `max_confidence` with a zero denominator lets a 45× outlier pass in a release build. A debug build panics at `rational.rs:37` (`debug_assert!`).
- **Impact:** the check depends on correct configuration and does not fail closed when the configuration is malformed.

### LOW-6: The TS consent payload is not byte-compatible with the Rust payload
- **Where:** `app/src/lib/safeMode.ts:181`–`:190` compared with `crates/eligibility-engine/src/consent.rs:71` onward.
- **Differences:**
  - TS joins fields with `:`; Rust writes raw bytes.
  - Asset codes: TS uses `01`/`02`; Rust uses the enum discriminant (`Skr=0`, `XSol=1`), which also shifts if the enum is reordered.
  - Chain, epoch, version, nonce and expiry: TS writes 16 bytes each; Rust writes 8.
  - TS treats the wallet as hex.
- **Impact:** a signature made over the TS payload won't verify against the Rust payload. This fails closed, but the "mirrors" comment is wrong, and it will break integration.

### LOW-7: FIFO valuation after delegation counts the oldest (highest-priced) lots
- **Where:** `crates/eligibility-engine/src/replay.rs:312` and `:326` (`head_value(…, recognized)`).
- **Impact:** when part of a balance is delegated, the recognized units are valued from the oldest lots, which are reference-priced and therefore at least as high as later ones. That is not the conservative choice.

### Informational
- **INFO-1:** `EpochReferences::with` (`pricing.rs:228`) silently overwrites a duplicate commit.
- **INFO-2:** `ReferenceCommit` has public fields (`pricing.rs:185` onward), so a struct literal can bypass the `new()` late-commit check.
- **INFO-3:** `EpochReferences` isn't bound to an epoch ID, so a stale epoch's commits could be passed in.
- **INFO-4:** the TWAP tail (`pricing.rs:164`) weights the last sample up to an unvalidated `window_end`.
- **INFO-5:** `ExtremeTracker::new(Some(0))` (`safe_mode.rs:260`) is accepted and gives no damping.
- **INFO-6:** `ExtremeTracker` (`safe_mode.rs:251`) is not wired into the replay. The replay applies every `EmergencyNav` unconditionally (`replay.rs:648`), and the lock safety release lives outside this crate.
- **INFO-7:** `AcceptAllVerifier` (`consent.rs:128`) is publicly re-exported from `lib.rs`.

## 9. Discrepancies against Z.ai's implementation report

| Reported | Observed |
|---|---|
| Rust: 353 passed | **355 passed**, 0 failed |
| Vitest: 712 passed | 712/712 on two idle runs; 2 failures on one run under heavy load that did not reproduce |
| ESLint: 0 errors / 13 pre-existing warnings | 0 errors / 13 warnings (matches) |
| Next build: passed | passed (matches) |
| Pass #5 implements the tests 30–39 requirements | Canonical tests pass. The requirements for 30 and 35 are only PARTIAL (F1, F2, F3). |
| `safeMode.ts` "mirrors" the Rust consent payload | not byte-compatible (LOW-6) |
| t33 comment claims re-observation restores recognition | the test never exercises it (it re-observes V2). Recovery does work, per my probe. |
| Test comment "50% of mid" | actually 5% |

## 10. Pass #4 regression

- All 355 workspace tests pass, including the Pass #4 canonical tests 16–29, the property tests and the replay tests.
- The Pass #4 test files changed only mechanically: `PriceTable` became `EpochReferences`/`ReferenceCommit`, with the epoch start set to `PRE_REFERENCE_EPOCH_START = i64::MAX/2` so that all Pass #4 worlds stay reference-priced. **No assertion was removed or weakened.**
- The following invariants are covered by passing tests:
  - ownership timeline
  - delegation and encumbrance
  - SKR wallet and stake deduplication
  - debt netted once per wallet
  - `X >= 0`
  - unreadable registry → PENDING
  - missing history → unresolved common pool
  - xSOL owned-equity-only valuation (notional rejected)
- Caveat: Pass #4 behavior is now tested only with pre-epoch reference pricing. Its interaction with in-epoch lot pricing is covered only by the Pass #5 tests and my probes.

## 11. Deferred production inputs

These are not configured anywhere, and they must not be guessed:
- oracle feeds
- confidence and TWAP thresholds for each adapter (`ReferenceValidation`)
- verified xSOL zone boundaries for the deployed pool version
- xSOL mint, pool and program pins
- the SKR production adapter (mint, stake program, IDL)
- debt-registry contents and pins
- the dwell period (unset, so the EXTREME safety release stays disabled)
- chain ID (the genesis hash; currently `ChainId::FIXTURE`)
- the production signature verifier
- persistent storage for the nonce ledger
- extra volatility, drawdown and slippage triggers
- integration of the lock safety release

Where the code handles these, it fails closed while they're absent, apart from F2, F4 and F5.

## 12. Final verdict

**PASS #5 NOT VERIFIED**

The regression suites are green, and most of the foundation is architecturally correct:
- INACTIVE and UNKNOWN stay separate
- exact integer handling of the zone boundaries
- CR=1 leverage singularity
- consent binding and replay protection
- resets apply only going forward, for both xSOL and SKR
- xSOL is valued on the NAV basis only
- pinned-upgrade UNKNOWN

However, F1 and F2 let value be recognized above what the §5 min rule allows, and F3 lets a consent authorize an operation whose route is unavailable. The canonical t35 asserts that last behavior as correct.

Remediation should:
- apply the min rule to Addition lots on late registration
- add a maximum age or epoch-window check for observations, failing closed to `AdditionUnpriced`
- check route availability in every zone where an operation is submitted, and correct the t35 WARM assertion and the TS mirror
- then address F4, F5 and LOW-6

## 13. Final git status (implementation unchanged)

- Before this report commit: `git status --porcelain` was empty, and `git rev-parse HEAD` was `bb7139470d6b9b4149fc147cc1be1202280eefc7`. The `npm ci`, `next build` and `cargo` outputs are gitignored (`node_modules/`, `.next/`, `target/`).
- The report branch `claude/pass5-verification-report` starts at `bb71394`, and its only commit adds this single file. To check: `git diff --stat bb71394..claude/pass5-verification-report` should list only `docs/handoffs/PASS5_FINAL_VERIFICATION.md`.
- The implementation branch `z-ai/cherry-tree-pass5-pricing-safe-mode` was not touched. Nothing was merged, deployed, initialized or funded, and no wallets, authorities, Vercel settings, GitHub secrets or production configuration were changed. Pass #6 was not started.

## 14. Commands/tests not run

- **Playwright:** not run. Pass #5 adds no user-facing UI; `safeMode.ts` is a library module that no page imports (`grep -rn safeMode app/src` found no other importer), so browser validation wasn't needed.
- **Failing test names from Vitest run 1:** not captured. The log wasn't kept for that run. Two later runs passed 712/712.
- **No on-chain or devnet actions:** none were in scope.

---

## Appendix A: scratch adversarial probe source (verification-only, not committed as code)

```rust
//! VERIFICATION-ONLY adversarial probes (scratch copy; not committed).
mod common;
use common::*;
use eligibility_engine::*;

fn skr() -> (EligibilityConfig, EpochReferences) {
    (config(vec![skr_adapter(StakeSupport::None)], &[LENDER_A]),
     EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 100)))
}
fn base() -> Tl {
    let mut t = Tl::new();
    t.slot(1, 50).init(A, SKR_MINT).init(B, SKR_MINT).init(C, SKR_MINT)
        .mint(A, SKR_MINT, 1_000).mint(B, SKR_MINT, 800).mint(C, SKR_MINT, 500)
        .pinned(&[LENDER_A]).register(A, AssetKind::Skr).no_debt(A, &[LENDER_A]);
    t
}

// ---- F1: late registration does not cap post-reference ADDITION lots ----
#[test]
fn f1_late_registration_leaves_addition_lots_above_registration_price() {
    let (cfg, refs) = skr();
    // Control: B holds 800 pre-epoch, price drops to 1, B registers late -> 800*1.
    let mut t = base();
    t.slot(2, 150).observe(SKR_MINT, price(2, 1));
    t.slot(3, 300).observe(SKR_MINT, price(1, 1));
    t.slot(4, 350).register(B, AssetKind::Skr).no_debt(B, &[LENDER_A]);
    t.slot(5, 400).close();
    let r = run_with(&cfg, &refs, &t, 400);
    println!("control B@370 = {:?}", x_at(&r, &B, 370));
    // Variant: C sends 500 to B at t=200 while price is still 2 (addition lot @2),
    // price drops to 1, B registers at 350.
    let mut t = base();
    t.slot(2, 150).observe(SKR_MINT, price(2, 1));
    t.slot(3, 200).send(C, B, SKR_MINT, 500);
    t.slot(4, 300).observe(SKR_MINT, price(1, 1));
    t.slot(5, 350).register(B, AssetKind::Skr).no_debt(B, &[LENDER_A]);
    t.slot(6, 400).close();
    let r = run_with(&cfg, &refs, &t, 400);
    let x = verified_x(x_at(&r, &B, 370));
    println!("variant B@370 = {x} (spec min(ref, price at registration)=1 -> expected 1300)");
    assert_eq!(x, 800 * 1 + 500 * 2, "DEFECT CONFIRMED if this passes");
}

// ---- F2: stale / pre-epoch observation used as 'price at the addition event' ----
#[test]
fn f2_pre_epoch_observation_prices_a_much_later_addition() {
    let (cfg, refs) = skr();
    let mut t = base();
    t.slot(2, 60).observe(SKR_MINT, price(2, 1)); // before epoch start (100)
    t.slot(3, 1_000_000).mint(A, SKR_MINT, 500); // ~11.6 days later, no fresh obs
    t.slot(4, 1_000_100).close();
    let r = run_with(&cfg, &refs, &t, 1_000_100);
    let e = x_at(&r, &A, 1_000_050);
    println!("A after stale-priced addition = {e:?}");
    assert_eq!(verified_x(e), 1_000 * 2 + 500 * 2, "no max-age: stale obs accepted");
}

// ---- F3: WARM / NORMAL + halted route + valid consent => permitted ----
struct Ok7;
impl ConsentVerifier for Ok7 { fn verify(&self, _: &Addr, _: &[u8], s: &[u8; 64]) -> bool { *s == [7; 64] } }
fn req() -> ConsentRequest { ConsentRequest { wallet: A, asset: AssetKind::XSol, action: ConsentAction::Enter,
    amount_lamports: 1_000, chain: ChainId::FIXTURE, epoch: 7, risk_version: 3, now: 1_000 } }
#[test]
fn f3_warm_and_normal_do_not_check_route_availability() {
    let r = req();
    let mut c = consent::fixture_consent_for(&r, 1, 2_000); c.signature = [7; 64];
    for zone in [RiskZone::Normal, RiskZone::Warm] {
        let mut l = ConsentLedger::new();
        let d = authorize_entry(zone, &r, Some(&c), &RouteCheck::halted(),
            &SafeModeBoundaries::fixture_xsol(), &mut l, &Ok7).unwrap();
        println!("{zone:?} + halted route + valid consent => {d:?}");
        assert_ne!(d, EntryDecision::Denied(EntryDenial::RouteUnavailable));
    }
}

// ---- F4: oracle confidence compared absolutely against a relative bound ----
#[test]
fn f4_confidence_is_not_relative_to_mid() {
    let v = ReferenceValidation { min_distinct_slots: 3, min_window_seconds: 60,
        max_spread: Ratio::new(1, 20), max_confidence: Ratio::new(1, 50) };
    let s = |slot, ts, n, d, c| PriceSample { slot, unix_timestamp: ts, lamports_num: n, units_den: d, confidence: c };
    // mid = 1/1000 lamports/unit, confidence half-width 9/10000 = 90% of mid.
    let tiny = [s(1, 100, 1, 1000, Some((9, 10_000))), s(2, 130, 1, 1000, Some((9, 10_000))), s(3, 160, 1, 1000, Some((9, 10_000)))];
    println!("90%-of-mid confidence on small price: {:?}", v.check(&tiny));
    // mid = 1000 lamports/unit, confidence 10 = 1% of mid.
    let big = [s(1, 100, 1000, 1, Some((10, 1))), s(2, 130, 1000, 1, Some((10, 1))), s(3, 160, 1000, 1, Some((10, 1)))];
    println!("1%-of-mid confidence on large price: {:?}", v.check(&big));
    assert!(v.check(&tiny).is_ok() && v.check(&big).is_err());
}

// ---- F5: validation config is not itself validated ----
#[test]
fn f5_degenerate_validation_config_accepts_single_spot_or_any_spread() {
    let s = |slot, ts, n| PriceSample { slot, unix_timestamp: ts, lamports_num: n, units_den: 1, confidence: None };
    let lax = ReferenceValidation { min_distinct_slots: 1, min_window_seconds: 0,
        max_spread: Ratio::new(1, 20), max_confidence: Ratio::new(1, 50) };
    println!("single spot w/ min_slots=1: {:?}", commit_reference(&[s(7, 1000, 5)], 1060, &lax, PriceBasis::Reference));
    let den0 = ReferenceValidation { min_distinct_slots: 3, min_window_seconds: 60,
        max_spread: Ratio::new(1, 0), max_confidence: Ratio::new(1, 0) };
    let manip = [s(1, 100, 2), s(2, 130, 2), s(3, 160, 2), s(4, 190, 90)];
    println!("45x outlier with max_spread den=0: {:?}", den0.check(&manip));
    assert!(den0.check(&manip).is_ok());
}

// ---- Boundaries, overflow, malformed ----
#[test]
fn boundaries_and_malformed() {
    let b = SafeModeBoundaries::fixture_xsol();
    let z = |n: u128, d: u128| zone_for_cr(Some(Ratio::new(n, d)), &b);
    for (n, d, want) in [(135u128, 100u128, RiskZone::Normal), (1_350_001, 1_000_000, RiskZone::Normal),
        (1_349_999, 1_000_000, RiskZone::Warm), (120, 100, RiskZone::Warm), (6, 5, RiskZone::Warm),
        (1_199_999, 1_000_000, RiskZone::Hot), (1_200_001, 1_000_000, RiskZone::Warm),
        (100, 100, RiskZone::Hot), (1_000_001, 1_000_000, RiskZone::Hot), (999_999, 1_000_000, RiskZone::Extreme),
        (0, 1, RiskZone::Extreme), (5, 0, RiskZone::Unknown)] {
        assert_eq!(z(n, d).unwrap(), want, "{n}/{d}");
    }
    assert_eq!(zone_for_cr(None, &b).unwrap(), RiskZone::Unknown);
    assert_eq!(z(u128::MAX, 1), Err(EligibilityError::MathOverflow));
    assert_eq!(z(u128::MAX, u128::MAX), Err(EligibilityError::MathOverflow));
    assert!(leverage_for_cr(Ratio::new(u128::MAX, u128::MAX)).unwrap().is_none());
    assert!(leverage_for_cr(Ratio::new(u128::MAX, u128::MAX - 1)).is_err() || true);
    let bad = SafeModeBoundaries { warm_below: Ratio::new(120, 100), hot_below: Ratio::new(135, 100) };
    assert!(bad.validate().is_err());
    let bad = SafeModeBoundaries { warm_below: Ratio::new(1, 0), hot_below: Ratio::new(120, 100) };
    assert!(bad.validate().is_err());
    // zero-denominator samples, empty window, late & duplicate commits
    let v = ReferenceValidation { min_distinct_slots: 3, min_window_seconds: 60, max_spread: Ratio::new(1, 20), max_confidence: Ratio::new(1, 50) };
    let zs = PriceSample { slot: 1, unix_timestamp: 1, lamports_num: 1, units_den: 0, confidence: None };
    assert!(v.check(&[zs]).is_err());
    assert!(v.check(&[]).is_err());
    let huge = [1u64, 2, 3].map(|i| PriceSample { slot: i, unix_timestamp: 100 * i as i64, lamports_num: u128::MAX, units_den: 1, confidence: None });
    println!("u128::MAX samples: {:?}", commit_reference(&huge, 400, &v, PriceBasis::Reference));
    assert!(commit_reference(&huge, 400, &v, PriceBasis::Reference).is_err());
    assert_eq!(ReferenceCommit::new(PriceKey::Mint(SKR_MINT), price(2, 1), 101, 100), Err(EligibilityError::LateReferenceCommit));
    assert!(ReferenceCommit::new(PriceKey::Mint(SKR_MINT), price(0, 0), 1, 100).is_err());
    let dup = EpochReferences::new().with(ref_commit_at(SKR_MINT, price(2, 1), 100)).with(ref_commit_at(SKR_MINT, price(9, 1), 100));
    println!("duplicate commit: len={} price={:?} (last silently wins)", dup.len(), dup.get(&PriceKey::Mint(SKR_MINT)).unwrap().price);
}

// ---- Price above reference, same-slot observations, wrong basis ----
#[test]
fn same_slot_and_basis() {
    let (cfg, refs) = skr();
    // Observation AFTER the addition in the same slot must not price it.
    let mut t = base();
    t.slot(3, 200).mint(A, SKR_MINT, 500).observe(SKR_MINT, price(1, 1));
    t.slot(4, 250).close();
    let r = run_with(&cfg, &refs, &t, 250);
    assert_eq!(x_at(&r, &A, 230), Some(Eligibility::Pending(PendingReason::AdditionUnpriced)));
    // Two same-slot observations before the addition: the later (lower) one applies.
    let mut t = base();
    t.slot(3, 200).observe(SKR_MINT, price(3, 1)).observe(SKR_MINT, price(1, 1)).mint(A, SKR_MINT, 500);
    t.slot(4, 250).close();
    let r = run_with(&cfg, &refs, &t, 250);
    assert_eq!(verified_x(x_at(&r, &A, 230)), 2_500);
    // Wrong-basis observation (NAV basis on SKR) is ignored -> unpriced.
    let mut t = base();
    t.slot(3, 200).observe(SKR_MINT, nav(1, 1)).mint(A, SKR_MINT, 500);
    t.slot(4, 250).close();
    let r = run_with(&cfg, &refs, &t, 250);
    assert_eq!(x_at(&r, &A, 230), Some(Eligibility::Pending(PendingReason::AdditionUnpriced)));
    // zero-den observation ignored
    let mut t = base();
    t.slot(3, 200).observe(SKR_MINT, price(1, 0)).mint(A, SKR_MINT, 500);
    t.slot(4, 250).close();
    let r = run_with(&cfg, &refs, &t, 250);
    assert_eq!(x_at(&r, &A, 230), Some(Eligibility::Pending(PendingReason::AdditionUnpriced)));
}

// ---- xSOL: notional / reference basis rejected, emergency NAV basis checked ----
#[test]
fn xsol_basis() {
    let cfg = config(vec![xsol_adapter()], &[LENDER_A]);
    let w = || { let mut t = Tl::new(); t.slot(1, 50).init(A, XSOL_MINT).mint(A, XSOL_MINT, 1_000)
        .pinned(&[XSOL_PROGRAM, LENDER_A]).register(A, AssetKind::XSol).no_debt(A, &[LENDER_A]); t };
    let notional = UnitPrice { lamports_num: 30, units_den: 1, basis: PriceBasis::UnderlyingNotional };
    for p in [notional, price(3, 1)] {
        let refs = EpochReferences::new().with(ref_commit_at(XSOL_MINT, p, 100));
        let mut t = w(); t.slot(2, 200).close();
        let r = run_with(&cfg, &refs, &t, 200);
        assert_eq!(x_at(&r, &A, 150), Some(Eligibility::Ineligible(IneligibleReason::InvalidPriceBasis)), "{p:?}");
    }
    let refs = EpochReferences::new().with(ref_commit_at(XSOL_MINT, nav(3, 1), 100));
    let mut t = w(); t.slot(2, 200).emergency_nav(XSOL_MINT, notional); t.slot(3, 300).close();
    let r = run_with(&cfg, &refs, &t, 300);
    assert_eq!(x_at(&r, &A, 250), Some(Eligibility::Pending(PendingReason::NavUnverified)));
    assert_eq!(verified_x(x_at(&r, &A, 150)), 3_000);
    // spot (Reference basis) emergency datum for xSOL also rejected
    let mut t = w(); t.slot(2, 200).emergency_nav(XSOL_MINT, price(1, 1)); t.slot(3, 300).close();
    let r = run_with(&cfg, &refs, &t, 300);
    assert_eq!(x_at(&r, &A, 250), Some(Eligibility::Pending(PendingReason::NavUnverified)));
    // EXTREME -> recover higher -> EXTREME again, prospective each time
    let mut t = w(); t.slot(2, 300).close();
    t.slot(3, 400).emergency_nav(XSOL_MINT, nav(1, 1));
    t.slot(4, 500).emergency_nav(XSOL_MINT, nav(5, 1)); // recovery ABOVE original NAV 3
    t.slot(5, 600).emergency_nav(XSOL_MINT, nav(1, 2));
    t.slot(6, 700).close();
    let r = run_with(&cfg, &refs, &t, 700);
    assert_eq!(r.wallet(&A).equity_seconds, 3_000 * 350 + 1_000 * 100 + 5_000 * 100 + 500 * 100);
}

// ---- Pinned upgrade then re-observation of pinned version: recovery ----
#[test]
fn upgrade_then_repin_recovers_prospectively() {
    let cfg = config(vec![xsol_adapter()], &[LENDER_A]);
    let refs = EpochReferences::new().with(ref_commit_at(XSOL_MINT, nav(3, 1), 100));
    let mut t = Tl::new();
    t.slot(1, 50).init(A, XSOL_MINT).mint(A, XSOL_MINT, 1_000)
        .pinned(&[XSOL_PROGRAM, LENDER_A]).register(A, AssetKind::XSol).no_debt(A, &[LENDER_A]);
    t.slot(2, 300).close();
    t.slot(3, 400).ev(EventKind::ProgramVersion { program: XSOL_PROGRAM, version: V2 });
    t.slot(4, 500).ev(EventKind::ProgramVersion { program: XSOL_PROGRAM, version: V1 });
    t.slot(5, 600).close();
    let r = run_with(&cfg, &refs, &t, 600);
    assert_eq!(x_at(&r, &A, 450), Some(Eligibility::Pending(PendingReason::AdapterProgramUnknown)));
    assert!(matches!(x_at(&r, &A, 550), Some(Eligibility::Verified { .. })));
    assert_eq!(r.wallet(&A).pending_seconds, 100);
    // Configured pin alone (no finalized observation) never establishes trust.
    let mut t = Tl::new();
    t.slot(1, 50).init(A, XSOL_MINT).mint(A, XSOL_MINT, 1_000)
        .pinned(&[LENDER_A]).register(A, AssetKind::XSol).no_debt(A, &[LENDER_A]);
    t.slot(2, 200).close();
    let r = run_with(&cfg, &refs, &t, 200);
    assert_eq!(x_at(&r, &A, 150), Some(Eligibility::Pending(PendingReason::AdapterProgramUnknown)));
}

// ---- Hysteresis extras ----
#[test]
fn hysteresis_extras() {
    // dwell = 0 (a configured value) gives no damping: every oscillation resets/releases.
    let mut t = ExtremeTracker::new(Some(0));
    for i in 0..10 { t.observe(ExtremeCondition::Extreme, i * 10); t.observe(ExtremeCondition::Extreme, i * 10); t.observe(ExtremeCondition::Healthy, i * 10 + 5); }
    println!("dwell=0: resets={} releases={}", t.resets(), t.releases());
    // time going backwards never confirms
    let mut t = ExtremeTracker::new(Some(300));
    t.observe(ExtremeCondition::Extreme, 1_000);
    t.observe(ExtremeCondition::Healthy, 900);
    t.observe(ExtremeCondition::Healthy, 0);
    assert!(t.is_extreme());
}

#[test]
fn f6_twap_tail_lets_last_sample_dominate() {
    let v = ReferenceValidation { min_distinct_slots: 3, min_window_seconds: 60, max_spread: Ratio::new(1, 20), max_confidence: Ratio::new(1, 50) };
    let s = |slot, ts, n| PriceSample { slot, unix_timestamp: ts, lamports_num: n, units_den: 100, confidence: None };
    // honest 200 for 60s, last sample 210 (+5%) at t=160, window_end far later.
    let w = [s(1, 100, 200), s(2, 130, 200), s(3, 160, 210)];
    let p = commit_reference(&w, 100_000, &v, PriceBasis::Reference).unwrap();
    println!("TWAP with far window_end = {}/{}", p.lamports_num, p.units_den);
}
```
