# z.ai prompt — THE CHERRY TREE docs site

Paste everything below the line into z.ai. It is self-contained: theme tokens,
hero copy, page list, and the full current docs content are all inline.

Why a custom site instead of more GitBook: GitBook's free plan only lets you
set a logo and one accent colour. A static site costs nothing, ships on the
GitHub Pages deploy this repo already runs, and can match the app exactly.

---

You are building the documentation website for **THE CHERRY TREE**, a
Solana-native planting/mining simulation. Build it as a **static site in plain
HTML + CSS + a small amount of vanilla JS**. No frameworks, no build step, no
npm, no paid services, no external CDNs, no web fonts, no analytics, no
trackers. It must work when opened straight from disk and when served from a
sub-path like `https://<user>.github.io/cherry-tree-mining-rig-/docs/` — so
use **relative links only**, never root-absolute `/…` paths.

## Output

Return every file in full, each in its own code block headed by its path:

```
docs-site/index.html           ← hero landing + overview
docs-site/user-flow.html
docs-site/architecture.html
docs-site/protocol.html
docs-site/economics.html
docs-site/security.html
docs-site/development.html
docs-site/deployment.html
docs-site/operations.html
docs-site/glossary.html
docs-site/assets/style.css     ← the one stylesheet, all tokens in :root
docs-site/assets/site.js       ← nav toggle, active link, copy buttons, petals
```

The hero image already exists; reference it as `assets/cherry-tree-hero.png`
(I will copy it in). Do not generate or inline any other raster images — draw
anything else with inline SVG or CSS.

## The brand (match the app exactly — these are the shipped values)

The app's own description of its look: *"Midnight navy ground, one
cherry-blossom pink, off-white type. Dark base, luminous blossom, a glow only
where it is the brand (the wordmark). Terminal-adjacent, not a casino and not a
glowing-card dashboard."*

Use these tokens verbatim in `:root` and reference only tokens in rules:

```css
:root {
  --bg: #080c16;          /* page ground, deep navy */
  --bg-panel: #0f1526;    /* cards, sidebar, code blocks */
  --bg-inset: #0b101d;    /* inset wells, table header */
  --bg-raised: #151d33;   /* hover — one step up, still navy, never grey */
  --rule: #1c2740;        /* hairlines */
  --rule-strong: #2b3a5c; /* borders on interactive things */

  --text: #e9ecf5;
  --text-dim: #8e9ab8;
  --text-faint: #5c6885;

  --blossom: #ff5fa2;     /* THE brand pink — the only accent colour */
  --blossom-soft: #ff8fbf;
  --blossom-dim: #8c3560;
  --blossom-wash: color-mix(in srgb, var(--blossom) 8%, transparent);
  --blossom-wash-strong: color-mix(in srgb, var(--blossom) 16%, transparent);
  --blossom-glow: color-mix(in srgb, var(--blossom) 35%, transparent);

  /* Status only — never a surface, button or body text. */
  --leaf: #6ee7b7;        /* VERIFIED / live */
  --sky: #7dd3fc;         /* DOCUMENTED INTENT / info */
  --steel: #7d8bb0;       /* INFERENCE / neutral */
  --alert: #ff6b81;       /* INCOMPLETE / UNCONFIRMED, warnings */

  --mono: ui-monospace, "SF Mono", "JetBrains Mono", Menlo, Consolas, monospace;
}
```

Typography and shape rules taken from the app:

- **Everything is monospace** (`--mono`). Body 15px, line-height 1.6 on docs
  pages (the app uses 14px/1.45; docs need a little more air).
- **Wordmark** "THE CHERRY TREE": 700 weight, `letter-spacing: 0.16em`,
  colour `--blossom`, `text-shadow: 0 0 18px var(--blossom-glow)`. This is the
  ONLY element with a glow.
- **Labels / section eyebrows / nav group titles**: 11px, uppercase,
  `letter-spacing: 0.18em–0.2em`, colour `--text-dim` or `--text-faint`.
- **Corners are tight**: 2–3px radius on panels, buttons, code blocks. Only
  status dots and pills are round.
- **Panels**: `--bg-panel` fill, 1px `--rule` border. No drop shadows, no
  glassmorphism, no gradients except a faint top-down `--blossom-wash` on
  primary buttons.
- **Callouts** mirror the app's `.banner`: `--bg-panel`, 1px `--rule-strong`
  border, 2px left border in the status colour, 11–13px text.
- **Links**: `--blossom-soft`, underline on hover. Inline `code`: `--bg-inset`
  fill, `--blossom-soft` text, 1px `--rule` border, 2px radius.
- **Tables**: full-width, `--bg-inset` header row with uppercase 11px tracked
  labels, `--rule` hairlines between rows, numbers right-aligned with
  `font-variant-numeric: tabular-nums`.
- Pink is used sparingly: wordmark, active nav item, links, primary CTA,
  headings' small prefix marker (e.g. a `▍` or `//` in `--blossom-dim`), and
  the hero. Everything else is navy and off-white.

## Second reference: the Cherry community page

The site must also feel like the product's Cherry community page, so a user
going chat → docs sees one brand. That page has:

- A near-black navy ground (`#05081a`–`#080c16`), white text.
- A **wide cinematic banner**: a huge glowing pink cherry tree on a cliff at
  left, a giant dusky-pink **moon** behind it, dark jagged **mountains**, a
  faint pink-lit **city skyline** on the horizon, a **reflective lake**, and
  pink petals drifting across. Top-right text block:
  `A SOLANA-NATIVE COMMUNITY` (tiny, wide-tracked) → `— THE —` → big
  **CHERRY TREE** in heavy pink display type with a soft glow →
  `PLANT · GROW · EARN · COMPOUND · TOGETHER` → a row of five thin-line pink
  icons with two-line captions: **PLANT ONCE**, **GROW TOGETHER**,
  **EARN SOL**, **COMPOUND REWARDS**, **SAME ROOTS BIGGER FRUITS**. Bottom
  strip: `THE CHERRY TREE · 🍒 · BUILT ON SOLANA`, and a script
  signature *It All Grows* at bottom right.
- The round tree logo (the hero image) overlapping the banner's bottom-left
  edge, like a profile avatar with a dark ring.
- Title **THE CHERRY TREE 🌸** in a heavy (800) white sans-serif.
- Description: *"Plant. Grow. Earn. Compound. 🌸 A Solana-native mining
  simulation built for Cherry. Grow your tree, earn SOL rewards, and compound
  together."*
- Row cards: `--bg-panel`, **16px radius**, a pink line icon at left, white
  semibold label, dim count and a `›` chevron at right.

How to combine the two references:

- **Display type** (hero title, page `h1`, banner title): heavy system sans —
  `font-family: system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  font-weight: 800` — like the chat page. **Everything else stays mono** like
  the app. Add `--display` to `:root` for this.
- Rebuild the banner scene **in inline SVG + CSS** (no image files, no
  generated rasters): radial-gradient moon in `--blossom-dim` →
  `--blossom-soft`, 2–3 layered mountain silhouettes in navy shades, a thin
  row of skyline rectangles with a few `--blossom` lit windows, a lake made by
  a mirrored, blurred, low-opacity copy of the skyline, and the hero PNG as the
  tree. Keep it to one SVG under ~8KB.
- Use the five pillars (PLANT ONCE / GROW TOGETHER / EARN SOL /
  COMPOUND REWARDS / SAME ROOTS BIGGER FRUITS) as a strip of thin-line inline
  SVG icons under the hero, each linking to the page that explains it
  (User flow, Overview, Economics, Protocol → `compound`, Economics →
  referral weight).
- Use the 16px-radius row-card pattern for the "Explore the docs" list on the
  landing page and for the mobile nav. Keep 2–3px radius for code, tables and
  callouts.
- The banner's marketing words ("earn", "compound") may appear **only** in
  the banner/pillar strip as brand taglines. The simulation disclaimer must be
  visible on the same screen as them.

## Hero (index.html only)

Top of the page is the community-page banner (above), full-bleed, ~56vh on
desktop / ~40vh on mobile, with the round logo overlapping its bottom-left
edge. Below it, centred, on `--bg`:

1. The hero image `assets/cherry-tree-hero.png` (a neon-pink blossom tree in a
   pink ring on black) is the overlapping avatar: ~120px mobile / 160px
   desktop, `border-radius: 50%`, 4px `--bg` ring, soft `box-shadow: 0 0 60px
   var(--blossom-wash-strong)` — the image's black corners must not show as a
   square.
2. Title **THE CHERRY TREE 🌸** in `--display` 800, white, ~44px desktop /
   30px mobile (chat-page style). The small mono glowing wordmark stays in
   the top bar.
3. Eyebrow line in `--text-faint`, uppercase tracked:
   `SOLANA · PLANTING SIMULATION · FUNDED SOL REWARDS`
4. Hero message, `--text`, max-width 60ch:
   > THE CHERRY TREE is a Solana-native mining simulation. Users buy
   > non-redeemable rigs, receive a share of a transparently funded SOL reward
   > stream, and may claim rewards or compound them into additional rigs.
5. Disclaimer callout directly under it, styled with the `--alert` left
   border:
   > This is a simulation. Rigs do not mine SOL, participate in Solana
   > consensus, or represent a deposit or redeemable principal.
6. Two buttons: primary **Read the docs →** (links to the Overview section
   below) with `--blossom` text, `--blossom-dim` border and the faint wash;
   secondary **View source** (link to the GitHub repo) in the plain style.
7. A small status row like the app's "Grove status" line: a 6px `--leaf` dot
   that pulses (2.4s ease-in-out) + `DOCS · V2`. Respect
   `prefers-reduced-motion`.

**Falling petals (the "do a lot more" part, still free):** behind the hero
only, 10–14 small SVG petals drawn in `--blossom` / `--blossom-soft` at 25–60%
opacity, drifting down and swaying with pure CSS keyframes at staggered
delays. Petals sit behind the content (`pointer-events: none`), never over
text. Disable entirely under `prefers-reduced-motion: reduce`. Keep it under
~2KB of CSS/SVG.

Under the hero, a **"Core invariant"** strip styled like a terminal readout:

```
BUYING RIGS != FUNDING EXISTING MINER REWARDS
```

then a 3-column vault card row (stacks on mobile) — CAPITAL / REWARD / FEE —
each card: uppercase tracked title, one line "Receives: …", one line
"Pays out: …":

| Vault   | Receives                            | Pays out                       |
|---------|-------------------------------------|--------------------------------|
| CAPITAL | non-fee portion of rig purchases    | nothing in V1                  |
| REWARD  | realized revenue via `fund_rewards` | user claims and compounds only |
| FEE     | disclosed purchase fee              | authority withdrawal           |

Then the Overview content (below) and a card grid linking to every other page.

## Layout for the doc pages

- **Top bar** (all pages): small round hero image (28px) + wordmark at 15px
  (glow still on), right side: GitHub link. Sticky, `--bg` with a 1px `--rule`
  bottom border.
- **Desktop ≥ 960px**: left sidebar 240px (`--bg-panel`, `--rule` right
  border) with the nav, content column max-width 760px, right rail "On this
  page" built by JS from the page's h2s.
- **Mobile**: sidebar collapses behind a "☰ MENU" button in the top bar; opens
  as a full-height panel. No horizontal scroll at 360px width; 16px side
  gutters; tables scroll inside their own wrapper, not the page.
- **Nav groups** (uppercase tracked group titles):
  - START — Overview, User flow, Glossary
  - HOW IT WORKS — Architecture, Protocol, Economics, Security
  - BUILD & RUN — Development, Deployment, Operations
- Active page: `--blossom` text + 2px `--blossom` left border +
  `--blossom-wash` fill.
- Each page ends with **← Previous / Next →** cards in nav order.
- Footer: "Implementation source is available under the Business Source
  License 1.1. This documentation is not a yield promise, investment advice,
  or a claim that a live deployment exists." in `--text-faint`, 12px.

## Honesty labels — render as coloured badges/callouts

The content uses four status markers. Whenever a list item or paragraph starts
with one, render it as a callout with a small uppercase pill:

| Marker                     | Colour    |
|----------------------------|-----------|
| **VERIFIED**               | `--leaf`  |
| **DOCUMENTED INTENT**      | `--sky`   |
| **INFERENCE**              | `--steel` |
| **INCOMPLETE / UNCONFIRMED** | `--alert` |

Never soften, drop, or reword these. Never add yield, APR, "earn",
"passive income", or price claims that are not in the content. Do not invent
features, numbers, addresses or links.

## Nice-to-haves (all free, all vanilla JS, keep site.js < 6KB)

- Copy button on every code block (top-right, 11px uppercase "COPY" →
  "COPIED").
- Auto-generated heading anchors (`#` link in `--blossom-dim` on hover).
- Client-side search box in the sidebar that filters nav links by title.
- The Economics phase table: render APR bands as small horizontal bars in
  `--blossom` at decreasing opacity (Genesis brightest → Dormant empty), pure
  CSS, numbers still visible as text.
- The claim-penalty schedule (25% / 15% / 5% / 0%) as a 4-step stepped
  timeline in CSS.
- `<meta name="theme-color" content="#080c16">`, a favicon as an inline SVG
  data-URI blossom in `#ff5fa2`, and Open Graph tags using the hero image.

## Accessibility

Semantic HTML (`header`, `nav`, `main`, `article`, `footer`), one `h1` per
page, visible focus ring (2px `--blossom` outline, 2px offset), all text at
WCAG AA contrast on its background, `alt` text on the hero, skip-to-content
link.

## Content — use verbatim, split one file per page

Keep every sentence. You may add headings, callouts, tables and layout, but do
not rewrite facts. Code identifiers in backticks stay as `code`.

````markdown
# Overview

## Plain English

THE CHERRY TREE is a game loop built around rigs and a funded reward stream. A purchase creates production capacity; it does not fund rewards owed to other users. Rewards exist only when an authorized funder deposits SOL into the reward vault and sets an emission window.

Rigs cannot be sold back or redeemed. Users can claim settled rewards or compound them into whole additional rigs.

## Technical detail

Three program-owned vaults are separate: CAPITAL receives non-fee purchase proceeds and is locked in this version; REWARD receives `fund_rewards` deposits and pays `claim`/`compound`; FEE receives purchase fees and is the only vault with an authority withdrawal path. Accounting lives in `crates/rigs-engine/`; Anchor handlers validate accounts and move lamports.

- **VERIFIED:** source/tests cover separation, integer accounting, instruction validation, and adversarial cases.
- **DOCUMENTED INTENT:** deployment and operational procedures describe a live launch.
- **INCOMPLETE / UNCONFIRMED:** this checkout does not prove live deployment, real Cherry handshake, or live wallet execution.
- **INFERENCE:** figures are not a live schedule unless read from chain state.

# User flow

1. Connect through Cherry’s wallet bridge, or use the standalone wallet path.
2. During launch gating, satisfy the applicable access rule.
3. Buy whole rigs; the quote separates capital and fee.
4. Let an active funded schedule accrue rewards.
5. Claim the eligible balance or compound it into whole rigs.

The embedded path is intended to arrive with a wallet and show no connect-wallet step. Standalone wallet behavior is implemented but real extension execution is not verified here.

Technically, actions are `buy_rigs`, `claim`, and `compound`. The client builds transactions and receives signatures from the wallet/Cherry bridge. Chain state is authoritative for balances, schedules, gates, and APR. Pre-launch permits only the configured founder activation. From T0 through the first 24 hours, valid `.skr` proof and a cumulative per-wallet fresh-purchase cap apply; at the exact boundary access becomes public. `.sns` is not the V2 launch gate.

# Architecture

## Plain English

The frontend is the interface, the Rust engine is the accounting calculator, and the Solana program checks accounts and moves SOL. The backend is intended to read/index state and authenticate integration requests; it does not hold a user-transaction signing key.

## Technical detail

| Layer | Location | Responsibility |
|---|---|---|
| UI/integration | `app/src/` | Next.js, wallet signing, chain reads, Cherry surfaces |
| Pure engine | `crates/rigs-engine/src/` | pricing, accrual, shares, penalties, APR |
| Anchor program | `programs/sol-rigs/src/` | constraints, state transitions, lamport movement |
| Tests | `crates/rigs-engine/tests/`, `programs/sol-rigs/tests/`, `app/tests/`, `app/e2e/` | unit, native program-test, browser, parity |

The canonical program identity in the primary source is `C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt`, declared in `programs/sol-rigs/src/lib.rs` and `Anchor.toml`. The release reconciliation branch independently confirms it; that branch was inspected read-only and not combined with this worktree.

# Protocol

`initialize` creates configuration/vaults; `migrate_v2` appends the Genesis cap; `fund_rewards` deposits and schedules emissions; `buy_rigs` purchases whole rigs and may bind a referrer; `claim` pays settled net rewards; `compound` converts affordable whole rigs and preserves the remainder. Authority instructions pause, change configuration, hand over authority, and withdraw fees.

PDAs, defined in `programs/sol-rigs/src/constants.rs`:

| Account | Seeds |
|---|---|
| Config | `config` |
| Miner | `miner`, owner |
| Capital vault | `capital_vault` |
| Reward vault | `reward_vault` |
| Fee vault | `fee_vault` |

`Config` stores authority, funder, fee recipient, pause state, pricing, accumulator, launch, referral, penalty, and cap state. `Miner` stores rigs, reward watermark, pending rewards, lifetime counters, weight, referrer, and planting time. The engine uses integer lamports and checked `u128` intermediates. Purchase lamports cannot reach REWARD because `buy_rigs` treats it as read-only.

# Economics

For `n` rigs: capital = `n × rig_price_lamports`; fee = `floor(capital × purchase_fee_bps / 10,000)`; total paid = capital + fee. The fee is capped at 10% and a single purchase at 1,000,000 rigs. Repository examples of 0.01 SOL and 3% are rehearsal values, not a live promise.

Rewards come only from `fund_rewards(amount, duration)`, which must back the schedule. When the window ends, APR is zero; existing liabilities remain accounted for. Mid-schedule funding rolls the remaining schedule into the new one.

APR uses the funded rate, 365-day annualization, fee-inclusive cost, and weighted total shares. More rigs dilute each rig’s share of the same emission. Phases are labels derived from state:

| Phase | Base APR |
|---|---:|
| Genesis | ≥ 1,200% |
| EarlyGrove | 600%–<1,200% |
| Growth | 250%–<600% |
| Mature | >0%–<250% |
| Dormant | no funded emission |

Claim penalties are 25% before 24h, 15% at 24–48h, 5% at 48–72h, and 0% thereafter. They stay in REWARD and are redistributed or recorded as dust. A valid first referrer changes weight from 100 to 105; it is not an extra payout or guaranteed 5% APR.

# Security

Only disclosed fees can be withdrawn, and only to the configured fee recipient. CAPITAL and REWARD have no authority drain path. Pausing blocks user actions while funding remains available. Authority transfer is two-step.

The program checks PDA seeds, owners, vault kind, signers, fee bounds, arithmetic, schedule solvency, launch rules, referrers, and reward watermarks. Tests cover vault substitution, drain attempts, forged referrers, pause behavior, checked math, token verification, and CSP. **INCOMPLETE / UNCONFIRMED:** no independent audit or live deployment assessment is claimed.

# Development

Run `./scripts/verify.sh` for the repository’s main local verification pipeline. It runs Rust tests, regenerates pricing/accrual/yield/launch/penalty fixtures, Vitest, and configured browser checks where dependencies exist. Native `solana-program-test` exercises the real Anchor entrypoint without SBF. Browser tests stub RPC and Cherry boundaries, so they do not prove live network/device integration.

The app is under `app/`; workspace metadata is in `Cargo.toml`; Anchor configuration is in `Anchor.toml`; SBF tooling is `scripts/build-sbf.sh`. Never place keypairs or secrets in the repository.

# Deployment

**DOCUMENTED INTENT:** scripts are devnet-guarded and verify the cluster by genesis hash. **INCOMPLETE / UNCONFIRMED:** this worktree does not establish a live deployment.

Safe sequence: run `./scripts/verify.sh`; run `./scripts/verify-deploy-ready.sh`; build/deploy with `./scripts/devnet-deploy.sh`; initialize with `./scripts/devnet-init.sh`; fund an explicit schedule with `./scripts/devnet-fund.sh amount duration`; then run smoke/rehearsal scripts and inspect read-back checks. Scripts refuse mainnet. Vercel configuration is in `app/vercel.json`; production hosting, Cherry registration, and production deployment are not verified.

Variables are documented in `.env.example` and `app/.env.example`. This package intentionally omits secret values.

# Operations

Operations mean deliberately funding rewards, monitoring runway and vault balances, pausing user actions when necessary, and verifying state after administration. Funding is not automatic; empty runway stops future emission rather than creating an uncovered promise.

Scripts under `scripts/` and `app/scripts/ops/` cover initialization, funding, smoke/rehearsal, first-buy preflight/verification, fee withdrawal, and authority audits. `fund_rewards` is restricted to `reward_funder`; `withdraw_fees` is authority-triggered but destination-pinned. The primary source reports no real devnet rehearsal from this environment, no issued/configured mini-app ID, and unverified live wallet/ticker checks.

# Glossary

- **APR:** annualized current reward rate from funded chain state; not a promise.
- **Capital vault:** non-fee purchase proceeds; locked in this version.
- **Compound:** convert settled rewards into whole rigs and retain the remainder.
- **Genesis cap:** per-wallet fresh-purchase limit during the first 24 hours after T0.
- **Miner PDA:** owner-specific position account.
- **REWARD vault:** funded source for claims and compounding.
- **T0:** chain timestamp of founder activation.
- **Weight:** reward-share units; base 100, referred 105.
````
