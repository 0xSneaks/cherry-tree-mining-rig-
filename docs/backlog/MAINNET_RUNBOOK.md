# Production / mainnet runbook (Backlog 18)

**PREPARED, NOT EXECUTED.** Nothing in this document has been run against mainnet. The
repository's signing ops scripts are devnet-only by design; mainnet signing happens through the
custody multisigs (`CUSTODY.md`) and the founder's Seed Vault wallet. Gates are sequential:
a FAIL stops the run.

## Gate 0 — prerequisites (all must be true before mainnet day)

- [ ] Part B of V2 qualification **PASS**, evidence archived.
- [ ] Founder decisions recorded: Genesis cap (`GENESIS_CAP_DECISION.md`), reward amount /
      duration / source (`REWARD_RUNWAY_OPS.md`), seed price + fee, custody members /
      thresholds / timelocks (`CUSTODY.md`), fee-recipient destination, official referrer,
      referral qualification thresholds, CAPITAL policy statement.
- [ ] Multisigs created by the founder; addresses recorded; test proposal executed on devnet.
- [ ] Physical Seeker plan (`SEEKER_DEVICE_TEST_PLAN.md`) PASS on devnet.
- [ ] `CHERRY_APP_ID` issued; production host chosen (header-capable).
- [ ] External audit status recorded (done / scheduled / accepted risk).

## Gate 1 — Phantom Pre-Ops

Record VERIFIED NOW for each: repository + remote, branch, HEAD, `git status` clean, no other
agents/processes operating on the release, canonical program ID
`C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt` in `declare_id!`, `Anchor.toml` (both profiles),
`app/src/lib/program/constants.ts` and the program keypair; qualification lane untouched.

## Gate 2 — source and artifact

1. Clean checkout at the **exact release commit** (a tag). Record the SHA.
2. `./scripts/verify.sh` → all green (record counts).
3. `./scripts/verify-deploy-ready.sh` → no FAIL.
4. `./scripts/build-sbf.sh` with the pinned toolchain (cargo-build-sbf ≥ 4.1.0,
   `--tools-version v1.52`). Record toolchain versions and `sha256sum target/deploy/sol_rigs.so`.
5. Rebuild on a second machine/runner; hashes must match (reproducibility).
6. Artifact size vs planned ProgramData allocation recorded.

## Gate 3 — deploy (upgrade authority → multisig immediately)

1. Deploy with an ephemeral deployer as upgrade authority **only for the deploy transaction**.
2. Immediately `set-upgrade-authority` to the upgrade multisig vault (one proposal if deploying
   via buffer owned by the multisig — preferred).
3. `solana program dump` → verify prefix hash equals artifact, remainder zero
   (`scripts/verify-program-dump.sh` logic).
4. `authority-audit.ts` with `CHERRY_TREE_EXPECTED_UPGRADE_AUTHORITY` → upgrade authority PASS,
   not a single key.

## Gate 4 — initialize Config

1. Proposal (config multisig as authority signer) for
   `initialize(price, fee_bps, reward_funder, fee_recipient, founder, genesis_cap_rigs)` with:
   founder `FtuidxSaoNDeTuNTDuf9jLACVwqjgQeBwKTYC54f3GYp`, the recorded cap, funder vault, fee
   destination.
2. **Two-person readback** of every argument (the cap has no setter).
3. Read back Config: all fields, version 2, `launch_t0 == 0`, `total_* == 0`, three vault PDAs
   owned by the program, `genesis_cap_rigs` exact.
4. `authority-audit.ts` → no FAIL (config authority multisig, funder separation, fee separation).

## Gate 5 — reward funding

1. `runway-report.ts --preview <sol> <days>` → record rate and end date.
2. Public announcement of amount, duration, rate, **source**.
3. Funder multisig executes `fund_rewards`.
4. `runway-report.ts` → status live, `BACKED yes`, `--history` shows the event. Schedule must
   outlive T0 + 24h by a comfortable margin.

## Gate 6 — production environment and frontend

1. `check-production-env.ts <env file>` → 0 FAIL; EXTERNAL only for deliberately unset growth
   options.
2. Deploy frontend at the release commit; verify headers (`frame-ancestors`, no XFO), CSP
   allowlist, `/api/global`, `/api/runway`, `/api/distribution` respond from mainnet.
3. Cherry registration updated with production URL/origin; `CHERRY_APP_ID` set; one real
   Cherry launch verifies `/api/session` 200.

## Gate 7 — physical Seeker gate (mainnet, read-only first)

On a Seeker against production: open in Cherry, verify grove renders, launch panel shows
PRE-LAUNCH, founder panel appears only for the founder wallet. **No purchases** until Gate 8.

## Gate 8 — founder activation (the one live transaction)

1. `first-buy-preflight.ts` (read-only, mainnet) with the explicit seed count `A <= C` → all PASS.
2. Founder opens the app with the Seed Vault wallet, runs the in-app preflight, types
   `ACTIVATE <A>`; the app simulates again and sends one transaction.
3. `verify-first-buy.ts <signature>` → founder signer, **T0 = block time**, exact SOL movement,
   fee, seeds, reward accounting, solvency, phase `.SKR GENESIS`, ~24h to Public. Record T0 and
   the Public boundary (T0 + 86,400) in UTC and local time.

## Gate 9 — post-activation verification and monitoring

- First non-founder `.skr` buy observed and verified (vault split, cap accounting).
- At exact T0 + 24h: Public open, cap no longer applied (observe first no-domain buy).
- Monitoring: `runway-report.ts` daily (alerts per `REWARD_RUNWAY_OPS.md`), `authority-audit.ts`
  weekly and after any governance action, `/api/distribution` concentration review during
  Genesis and the first week.
- Disclosure checks: UI shows "Locked in this program version" semantics, non-redeemable,
  per-wallet caveat, variable rate, Dormant copy; no return promises.

## Pause / emergency

- Containment: config multisig `set_paused(true)` (stops buy / claim / compound; funding stays
  open). Announce immediately.
- Diagnosis against two RPCs; archive state snapshots.
- **Forward-fix only**: new artifact through the upgrade process (`CUSTODY.md` §6) with the
  shortest safe timelock; never roll back past a state migration.
- Unpause only after the fix is verified on the qualification lane of the new artifact.

## Evidence archive (per gate)

Commit SHA, toolchain versions, artifact hash(es), program ID, ProgramData address + dump hash,
all proposal IDs / approvals / signatures, Config read-back, funding preview and announcement,
env-check output, Seeker test sheet, T0 / boundary timestamps, preflight + verify outputs.
