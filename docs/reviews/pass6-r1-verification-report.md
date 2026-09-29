# Pass #6 R1 — Independent Verification Report

- Reviewed SHA: `c0fc6a27d27f9cb7826dc1cd4ed647fac1310aa7`
- Decisions SHA: `e3acf3959e899f99f8645403e9f07b85e59c104e`
- Date: 2026-09-29
- Recorded verbatim from the Claude Code session report.

---

**Handoff note:** Codex's checkpoint wasn't in your message or anywhere in the repo, so I couldn't use its results. I took the P6R-1..8 scope from the labels in the R1 diff (`1884ac5..c0fc6a2`) and verified each item myself against `e3acf39`. P6R-6 carries no label in the code; I matched it to the D2 change (the test-41 primitive removed), which is the only other change in the diff.

---

**P6R-1 (D5 fixed-point accumulator): VERIFIED CLOSED**
- The exact-fraction code (`Frac`) is gone. Each segment now computes `acc_seg = floor(E·Δt·10^18/(D·H_seg))`, each wallet keeps one cumulative `u128`, the leaf is floored once, then capped at `E/5`. This matches D5 exactly.
- The overflow bounds hold, and the tests check them at the Storm ceiling over 950,400 s with 240 segments.
- A realistic four-interval Harvest settles and matches the D5 formula computed independently in the test.

**P6R-2 (D7 item 5, Safe Mode crosses the epoch boundary): VERIFIED CLOSED**
- An emergency NAV that hasn't recovered and an unusable NAV (`nav_invalid`) both carry into the next epoch. A new committed reference no longer drops them.
- A `RouteResumed` recovery clears the emergency price, which then isn't carried. Values at or before `epoch_start` use the active emergency price first.
- Both directions are tested, and earlier segments are never repriced.

**P6R-3 (D6, `RouteHalted` no longer carved out): VERIFIED CLOSED**
- `classify_segment` now reserves the interval for every Pending reason on a seed-qualified wallet, `RouteHalted` included. Nothing is redistributed.

**P6R-4 (D10, not-earned needs canonical evidence): NOT VERIFIED.** See NEW-1.
- `classify_interval` itself is correct.
- But `settle_epoch` still takes the verdict as a caller-supplied label. Its only guard is "participants must be empty", and the caller controls that field too.

**P6R-5 (D3 switch state machine): VERIFIED CLOSED**
- An exit drops any queued switch (`replay.rs:804`).
- Re-registering the anchored asset cancels the queued switch (`:781`), and the latest switch request wins.
- Timing matches D3: a switch at the boundary is deferred to the following epoch, a join at the boundary is immediate, and exit then re-register of a different asset re-queues it.

**P6R-6 (D2, test 41 out of Pass #6): VERIFIED CLOSED**
- `registration_window`, `season_start`, `SEASON_START_OFFSET`, `registration_finalized_in_window` and their test are removed, with no remaining references.
- Test 42 still asserts the 518,400 s Season boundary.

**P6R-7 (D6, a sub-threshold Pending wallet doesn't block): VERIFIED CLOSED**
- Only a Pending wallet with `S ≥ 150` blocks. The 149/150 boundary is tested, and the other wallets' leaves are identical with and without the sub-threshold wallet.

**P6R-8 (D7 item 1, epoch anchoring): VERIFIED CLOSED**
- `replay_epoch` now requires:
  - a `Checkpoint` at exactly `epoch_start`;
  - `end > epoch_start`;
  - every reference committed with that same `epoch_start` (disagreeing references are rejected).
- Tests cover a missing checkpoint and checkpoints at 400, 499, 501 and 550.

**NEW-1 (caller-built `NotEarned` verdict): NOT VERIFIED.** A caller can turn an earned interval into `NotEarned`.

| | |
|---|---|
| Severity | **HIGH** (Fruit settlement path; breaks D10 guard 2 and P6R-4's own claim) |
| Affected API/types | `settle_epoch`; `EpochInterval { start, end, verdict, participants }` (all fields `pub`); `IntervalVerdict::NotEarned`; `OutageEvidence` (public enum, no proof of origin); `Segment` (all fields `pub`); `classify_interval` |
| Can outside code build it? | Yes. Everything is re-exported from `lib.rs:78-84` and has plain public constructors. |
| Provenance enforced anywhere else? | No. `settle_epoch` never sees a `Segment` or any evidence. Nothing in the crate calls `classify_interval`. `programs/sol-rigs` only checks Merkle roots and knows nothing about not-earned. The doc comment at `settlement.rs:434` ("reachable ONLY through `classify_interval` … never a caller's bare label") and the guard comment at `:678` ("caller-controlled classification must not decide economics") are both false. |
| Can an already-earned interval be suppressed? | **Yes.** I reproduced it with a throwaway test, then deleted it. A Verified segment with registered wallet A classifies as `Settle`, and the honest settlement allocates 136,986,301 to A. Passing `NotEarned` with empty participants is accepted: allocated 0, `not_earned` 684,931,507 (the full E), conservation still holds. |
| Economic consequence | (a) Earned capacity is sent U→Z at pool close instead of paid to registered wallets. (b) An unreconciled window (canonically `Reserve`, i.e. D10's wallet-B indexer-miss case) relabelled `NotEarned` moves the whole held reserve (684,931,507) into terminal U→Z, so B's claim is lost permanently. That is exactly what D10 guard 2 forbids. |
| Minimum safe remediation | 1. Make `settle_epoch` derive verdicts itself: take the replay's `Segment`s, the seeds and the per-segment evidence, then call `classify_interval` and `weight_inputs` internally. 2. Stop callers from supplying verdicts and participants: make `EpochInterval`'s constructor crate-private, or wrap `NotEarned` in a sealed type only `classify_interval` can produce. 3. Make `Segment` impossible to build outside replay (private fields or `#[non_exhaustive]`). 4. Source the three registration-program `OutageEvidence` forms from finalized replay events, not a bare public enum value. 5. Add negative tests: `NotEarned` over a registered segment, and over an unreconciled segment, must be impossible to express or rejected. |

Related but not blocking: the same trust gap also lets a caller pass `Settle` with an empty participant list, which sends earned capacity to Z as "unallocated". It is fixed by the same change.

---

**PASS #5 REGRESSIONS:** None. `pricing_safe_mode` and `safe_mode_properties` are green inside the 415. The one Pass #5 behaviour change (values at or before `epoch_start` now use the active emergency price) is what D7 item 5 requires.

**ENGINEERING GATE:** Rerun independently on `c0fc6a2`:
- Workspace tests: 415 passed, 0 failed.
- `epochs_timeline`: 21/21.
- `cargo fmt --check`: clean.
- `clippy -D warnings` on `eligibility-engine` and `rigs-engine`: clean.
- Workspace-wide clippy fails with 39 errors in `programs/sol-rigs`. R1 didn't touch `programs/`, so I'm treating those as pre-existing; I didn't rerun clippy on the old commit to prove it.

**MIMOSA:** I couldn't run it; the Mimosa tool isn't available in this environment. The commit message's own claim is zero findings in the remediation files, with the global gate still blocked by standing unrelated app findings. Treat that as unverified by me.

**NEW FINDINGS:**
- **NEW-1 (HIGH, blocking):** described above.
- **Advisory (low, not blocking):** `classify_segment` and `weight_inputs` treat a missing seed record as 0 seeds. So a qualifying Pending wallet with no seed record doesn't block, and it drops out of H. That fails open on caller input, and one test asserts it as intended behaviour.

PASS #6 R1 NOT VERIFIED — P6R-4, NEW-1
