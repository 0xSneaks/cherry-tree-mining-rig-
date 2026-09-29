# Pass #6 R2 — D10 Settlement Provenance Remediation Report

- Branch: `claude/cherry-tree-pass6-r2-d10-provenance`
- Base SHA: `c0fc6a27d27f9cb7826dc1cd4ed647fac1310aa7`
- Final SHA: `bce1b68fd71db30e4fe23073f6443ad55f086ade`
- Date: 2026-09-29
- Recorded verbatim from the Claude Code session report.

---

The caller can no longer set a settlement verdict or participant list. The R2 fix is committed and pushed as `bce1b68`, and all tests pass.

**WORKTREE:** `/tmp/claude-0/r2`

**BRANCH:** `claude/cherry-tree-pass6-r2-d10-provenance`

**BASE SHA:** `c0fc6a27d27f9cb7826dc1cd4ed647fac1310aa7`

**FINAL SHA:** `bce1b68fd71db30e4fe23073f6443ad55f086ade`

**FILES CHANGED (10, all in `crates/eligibility-engine`):**
- Code: `src/settlement.rs`, `src/replay.rs`, `src/lib.rs`.
- New: `src/settlement_kernel_tests.rs`.
- Tests: `tests/epochs_timeline.rs`. In `tests/canonical.rs`, `tests/common/mod.rs`, `tests/pricing_safe_mode.rs`, `tests/properties.rs` and `tests/safe_mode_properties.rs` the only change is `.segments` → `.segments()`.
- Nothing outside this crate changed, so `programs/` and `app/` are untouched.

**D10 DESIGN:**
- **Settlement input:** `settle_epoch(budget, t_start, t_end, &ReplayReport, &SeedTimeline)` is now the only public settlement function. It works out each interval's verdict (`classify_segment`) and its participants (`weight_inputs`) itself. The old caller-labelled path is now the crate-private `settle_intervals` + `EpochInterval`.
- **Sealed report:** `ReplayReport.segments` is private and read through `segments()`. Only the replay can build a report or its segments, and nobody else can edit, drop or relabel them. `compile_fail` doctests prove the spoof shapes don't compile.
- **NotEarned:** it now comes only from replay state: a reconciled (`Verified`) segment with no registration. An unreconciled segment is always `Reserve` (D10 guard 2), and a segment with a registration settles normally.
- **Removed:** `classify_interval` is deleted and caller outage evidence no longer reaches settlement, per D10 item 1 ("no attestation accepted"). `OutageEvidence` and `classify_outage` remain only as a descriptive list.
- **Seed counts:** `SeedTimeline` holds seed counts over time. Segments are split wherever a seed count changes, and conflicting counts at the same timestamp are rejected.
- **Unchanged:** the P6R-1 fixed-point accumulator code.

**NEW TESTS:**
- **Replay-driven (in `epochs_timeline`):**
  - `r2a`: registered earned segment can't be forced to not-earned
  - `r2b`: empty-participant spoof can't send earned capacity to Z
  - `r2c`: `Reserve` can't be relabelled `NotEarned`
  - `r2d`: fabricated outage evidence authorizes nothing
  - `r2e`: genuine not-earned still behaves correctly
  - `r2f`: late reconciliation settles once, with no double allocation
  - `r2_seed_timeline…`: segment splitting at seed changes, plus window-coverage checks
- **Kernel test:** `p6r4_kernel_rejects_not_earned_over_registered_wallets`.
- **Doctests:** 3 `compile_fail` plus 1 positive.
- **Mutation check:** I broke each property on purpose (sealed field, fail-closed seeds, derived NotEarned, Reserve relabel, dropped participants). The tests caught every one.
- **Relocated tests:** tests 44–46, P6R-1 ×2, P6R-3, P6R-7 and the D5 invariants moved into the crate because they need hand-built intervals. I checked they are identical to R1 apart from the function name `settle_epoch`→`settle_intervals` (and `cargo fmt` whitespace). One comment line was also reworded.

**WORKSPACE TESTS:** `cargo test --workspace`: 426 passed, 0 failed.

**TARGETED TESTS:**

| Suite | Passed | Covers |
|---|---|---|
| `epochs_timeline` | 19/19 | P6R-2, 5, 8, P6-D3/D6/D7, t40/42/43, R2-A..F |
| lib unit | 49/49 | P6R-1, 3, 7, tests 44–46, D5 invariants |
| doctests | 4/4 | spoof shapes don't compile |

P6R-6 is still verified by test t42.

**PASS #5 REGRESSIONS:** none. `pricing_safe_mode` 27/27, `safe_mode_properties` 12/12, `properties` 8/8, `canonical` 17/17.

**FMT:** `cargo fmt --all -- --check` clean.

**CLIPPY:** `-p eligibility-engine -p rigs-engine --all-targets -D warnings` clean.

**MIMOSA:** not run. It isn't installed here and no repo hook exists. This is not a Mimosa clearance.

**LOW ADVISORY STATUS:** Fixed, fail-closed. A missing seed record now counts as unknown, not 0, so a Pending or Verified wallet without one holds the interval in reserve. An explicit `0` still means weight 0. Test `p6d6` was updated and asserts both cases. I took this from §5/D6 ("unreadable state is pending"); no founder decision was needed.

**Remaining trust boundary (not part of D10):**
- Settlement still trusts two inputs that aren't labels: the replay's event stream and the seed values. An explicit `seeds = 0` for a wallet, or an omitted `Register` event, can still zero that wallet.
- Seeds (`Miner.rigs`) aren't replayed by this crate. Closing this would mean turning seeds into replay events, which is a scope decision for you.

**GIT STATUS:** clean. Nothing merged or deployed.

**REMOTE SHA:** `origin/claude/cherry-tree-pass6-r2-d10-provenance` = `bce1b68fd71db30e4fe23073f6443ad55f086ade`

PASS #6 R2 READY FOR INDEPENDENT VERIFICATION — bce1b68fd71db30e4fe23073f6443ad55f086ade
