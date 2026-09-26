# Mobile UI and Mimosa blocker review — 2026-09-26

This note records a read-only review of the staged mobile UX change on
`zai/devnet-state-read-fix` at
`5633a60332576a876e4a1da112e267eb4e46b631`. The review inspected
`git diff --cached` directly; it did not require a commit hash for the UI
change and did not modify, push, merge, or deploy that change.

## Staged scope verified

The index contained exactly these four files:

- `app/src/components/Game.tsx`
- `app/src/components/MinerCockpit.tsx`
- `app/src/components/MilestonesPanel.tsx`
- `app/src/app/globals.css`

There were no unstaged tracked changes. The existing untracked `.mimosa/` and
`app/.mimosa/` directories were outside the index.

## UI review result

The staged UI diff is safe to keep.

- `BuyPanel` / Plant Seeds moves above `MinerCockpit` while retaining the same
  props and `onBuy` handler.
- A wallet with no miner position receives a compact cockpit with the existing
  cadence/status header and “Plant your first seeds to activate your grove.”
  It does not render the five management blocks or harvest/replant buttons.
- An existing position continues through the original full cockpit branch,
  including position, penalty, replant, reward-health and referral/milestone
  blocks, plus the unchanged replant and claim handlers.
- Milestone achievement and reminder lists gain an 18px inline-start indent,
  zero list margin and 2px item margins.
- The desktop layout remains centered at its existing maximum width and does
  not overflow. Phone and 320px-wide layouts also do not overflow.
- No protocol code, economics, APR calculation, purchase implementation,
  reward calculation, harvesting, replanting, wallet handling, RPC handling,
  or backend behavior changed. The diff only reorders existing JSX, adds a
  no-position render branch, and adds list-spacing CSS.

No high- or medium-severity production UI issue was found.

One low-severity test-maintenance issue remains: the no-position referral test
in `app/e2e/render.spec.ts` still expects “My grove” even though the requested
compact empty cockpit intentionally removes that zero-filled position row.
That assertion should instead wait for or assert `cockpit-empty`. A second,
unrelated existing result-card test expects the exact title without the
already-present blossom glyph.

## Checks performed

- `git diff --cached` and a word-level staged diff review
- `git diff --cached --check` — passed
- `npm run typecheck` — passed
- `npm run build` — passed
- `npm test` — 33 test files and 458 tests passed
- `npm run lint` — exited successfully with zero errors and 13 pre-existing
  warnings; none point to the staged JSX or CSS
- Full `npx playwright test` — 61 passed and 2 stale assertions failed as
  described above
- Focused temporary production-browser probe — 3 passed:
  compact empty cockpit and DOM order; complete existing-position cockpit and
  milestone computed spacing; desktop centered width and no overflow
- The generated mobile screenshot was inspected and showed Plant Seeds above
  the complete 42-seed cockpit without clipping.

The temporary browser test was deleted after it ran. Starting Next.js created
untracked `app/AGENTS.md` and `app/CLAUDE.md`; those generated files were also
removed, restoring the worktree to the original staged UI files plus the
pre-existing untracked Mimosa state.

## Mimosa findings

The flagged files are unchanged from HEAD and are not staged:

- HIGH and MEDIUM: `app/src/proxy.ts:32`
- MEDIUM: `app/src/lib/server/growthConfig.ts:34`
- MEDIUM: `app/scripts/ops/runway-report.ts:105`

Git blame and ancestry checks show that `proxy.ts:32` came from `1b2c54cc`,
while the other two lines came from `fc75b3bf`. Both commits predate and are
ancestors of the reviewed HEAD. The staged UI does not import or change these
files and does not alter their data flow.

### `app/src/proxy.ts:32`

The code-injection classification is a likely static-analysis false positive.
`buildCsp` constructs a CSP header; it does not evaluate code. The nonce comes
from `crypto.randomUUID()` and passes explicit character validation. Configured
origins are normalized through `URL.origin`, and request-controlled data does
not enter the policy. Production omits `unsafe-eval`; every request receives a
fresh nonce and a `private, no-store` response. CSP unit and production-browser
tests passed, including nonce matching and absence of CSP violations.

There is an ordinary deployment-configuration responsibility: an operator
could weaken allowed connections by configuring extra origins or by running a
non-production mode. That is not an exploit reachable from this UI diff and
does not justify changing the CSP implementation for this commit.

### `app/src/lib/server/growthConfig.ts:34`

This is a likely false positive at a configuration-validation boundary.
Season data comes from a deployment environment variable, is parsed as JSON,
validated, and fails closed to no seasons on validation or parsing errors.
There is no code evaluator, shell sink, raw-HTML sink, or other execution
primitive. React escapes the season text that reaches the UI.

### `app/scripts/ops/runway-report.ts:105`

This is a type-safety/static-analysis concern rather than an exploitable
security issue. The assertion is erased at compile time. Its input comes from
fixed-schema event decoding, is filtered to `RewardsFunded`, and is used only
by a read-only operations report. The script signs and sends nothing and does
not pass the values to a shell or evaluator.

## Gate recommendation

No production security patch is required before the UI change can safely
land. Do not use `--no-verify`, change Mimosa configuration, suppress the
findings, or make speculative security edits.

The safest first step is to retry the ordinary verified commit from a fresh
Mimosa session or clean worktree based on the same HEAD with only the four UI
files staged. The captured Mimosa state reported an incomplete baseline and a
broader earlier touched-file session, so resetting to the exact diff scope is
legitimate isolation rather than a gate bypass.

If Mimosa intentionally scans the full repository and still blocks, send the
unchanged-file, ancestry, source-to-sink and passing-test evidence above
through Mimosa's normal review/reclassification process. A test-only update to
the stale no-position assertion is appropriate if a fully green Playwright
suite is required, but no production UI or security patch is warranted by
this review.
