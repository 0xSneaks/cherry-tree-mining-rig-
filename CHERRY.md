# THE CHERRY TREE — Cherry mini-app integration

Status of the integration against `@cherrydotfun/miniapp-sdk`, and the packet
to register the mini-app with.

- **SDK version implemented against:** `0.1.21` (the `latest` dist-tag on npm
  at the time of writing; `app/package.json` pins `^0.1.21`).
- **Authoritative checklist:** the official skill Cherry ships inside the
  package, at `node_modules/@cherrydotfun/miniapp-sdk/skills/cherry-miniapp-integration/SKILL.md`.
  It is installed for Claude Code at `~/.claude/skills/cherry-miniapp-integration/`.
- **Bridge protocol:** v2 (`BRIDGE_VERSION = 2`).

---

## 1. Registration packet

Cherry's portal is at <https://portal.cherry.fun>. Registration is what issues
the `app_id` / `miniAppId`; nothing below can be invented locally, and the code
treats it as configuration until it exists.

Copy and paste:

```
Product name:        THE CHERRY TREE
Short name:          CHERRY TREE
Slug:                cherry-tree
Category:            Game / simulation (Solana)

Description:
  A Solana mining simulation. Players hire "rigs" with SOL and earn SOL from a
  transparently funded reward vault on a published schedule. Rewards are paid
  only from what has actually been funded — there is no yield promise, no
  redemption of principal, and every figure on screen is read live from chain
  state. Rigs are non-redeemable. The reward vault, the capital vault and the
  fee vault are separate program-owned accounts; only the fee vault is
  withdrawable by the program authority.

Deployment URL:      https://phantomcapai.github.io/cherry-tree-mining-rig-/
Origin:              https://phantomcapai.github.io
  (see §3 — this origin covers Cherry MOBILE only. A header-capable host is
   needed for Cherry WEB, and that URL will replace this one.)

Icon:                Cherry Tree logo, square PNG, transparent background.
                     Supply at 512×512 and 1024×1024.

Requested permissions:
  wallet:connect     — the game is entirely wallet-signed: buy, compound and
                       claim are user-signed Solana transactions. No server of
                       ours ever holds a key or moves SOL.
  inline:render      — the player can share a read-only result card (rigs
                       hired, amount compounded, amount claimed) into a chat as
                       a blink. Required by the SDK for share() to work.

Solana network during testing:   DEVNET ONLY
                     Program ID: By7qjU4ZYBYQvdkh41UjnhxWmLyP63S3PbdZXbg1UiKH
                     No mainnet deployment exists and the deployment scripts
                     refuse mainnet outright (they verify the cluster's genesis
                     hash on chain, not just the URL).

Community / room:    THE CHERRY TREE — private Cherry community, owned by the
                     submitter, created on Solana Seeker. Branded logo and
                     1500×500 cover already in place.

Contact:             PhantomCapAI
```

**Once Cherry issues the app id**, set it as configuration — no code changes:

| Variable | Where | What it does |
|---|---|---|
| `CHERRY_APP_ID` | server env | `expectedAppId` for `verifyLaunchToken`. Until it is set, `app/src/lib/server/launchToken.ts` **refuses to verify any token** rather than accepting an unaudienced one. |
| `CHERRY_EXPECTED_ORIGIN` | server env, optional | pins `expectedOrigin` on the launch token. |
| `CHERRY_JWKS_URL` | server env, optional | overrides the default `https://chat.cherry.fun/.well-known/jwks.json`. |

---

## 2. Integration status against the SDK skill

| SDK step | Where | State |
|---|---|---|
| 1 — framework discovery | Next.js 16 App Router, `@solana/kit` | done |
| 2 — install SDK | `app/package.json` → `^0.1.21` | done |
| 2b — CSP / `frame-ancestors` | `app/src/lib/csp.ts` | done in code; **host-dependent**, see §3 |
| 3 — environment detection | `GameRoot.tsx`, `useCherryEnvironment({ strict: true })` | done, strict |
| 4b — wallet, `@solana/kit` path | `app/src/lib/signer.ts` | done, see note below |
| 5 — `CherryMiniAppProvider`, user & room | `GameRoot.tsx`, `EmbeddedGame.tsx` | done |
| 6 — hide standalone-only UI | `EmbeddedGame.tsx` renders no connect step at all | done |
| 7 — navigation | `useCherryNavigate().userProfile` on the leaderboard | done |
| 7b — sharing / blinks | `EmbeddedGame.tsx` → `useCherryShare`, rendered by `/result` | done |
| 8 — backend token verification | `app/src/lib/server/launchToken.ts` | done, blocked on `CHERRY_APP_ID` |
| 9 — verify both modes | `app/e2e/render.spec.ts`, `app/e2e/blink.spec.ts` | done for standalone + embedded + blink; real Cherry host untested |
| Lifecycle events | `EmbeddedGame.tsx` (`walletDisconnected`), `useLiveAccrual` (suspend/resume) | done |

### Note on the kit signer

The skill's step 4b shows `createCherrySigner(cherry)`, which returns a
`TransactionSigner` — it *signs* and hands the bytes back for the app to
broadcast. This app uses `wallet.signAndSendTransaction` instead, which is a
first-class method on the SDK's own `useCherryWallet()` result and on
`CherryMiniApp.wallet`. The difference is who broadcasts: with
`signAndSendTransaction` the Cherry host does, which is the behaviour wanted
here — the user sees one Cherry signing sheet and the host owns the send. The
kit transaction is still built with `@solana/kit` and handed over as wire
bytes, so there is no `web3.js` round trip either way.

---

## 3. Hosting — mobile and web are different problems

Cherry embeds the mini-app two different ways, and only one of them cares about
response headers.

**Cherry mobile (WebView, the Seeker target).** The page is loaded in a native
WebView, not an iframe. `frame-ancestors` is irrelevant. A static host is
sufficient.

**Cherry web (iframe).** The browser enforces `frame-ancestors`. Every HTML
response must carry:

```
Content-Security-Policy: frame-ancestors 'self' https://chat.cherry.fun
```

and must not carry `X-Frame-Options: DENY` or `SAMEORIGIN`. A `<meta
http-equiv>` tag is **not** equivalent — `frame-ancestors` is ignored in a meta
tag by specification. This is a real constraint, not a preference.

### Where that leaves the current deployment

| Host | Cherry mobile | Cherry web | Why |
|---|---|---|---|
| GitHub Pages (current) | yes | **no** | Pages cannot send custom response headers, at all. There is no configuration that changes this. |
| Cloudflare Pages | yes | yes | reads the `_headers` file the build already emits |
| Netlify | yes | yes | reads the same `_headers` file |
| Vercel / any Node host | yes | yes | `next.config.ts` serves the headers directly |

The static build now writes `out/_headers` from the same
`app/src/lib/csp.ts` that `next.config.ts` serves, so pointing Cloudflare Pages
or Netlify at this repository is the whole change — no code edit, no second
copy of the policy to drift.

Recommended: **Cloudflare Pages**, free tier, connected to this repo with build
command `npm run build && npm run emit:headers` in `app/`, output directory
`app/out`, and `STATIC_EXPORT=1`. Then register that origin with Cherry instead
of the Pages one.

---

## 4. What is verified, and what is not

**Verified here**, by the suites in `scripts/verify.sh`:

- strict environment detection, including that a non-Cherry wallet in-app
  browser is *not* mistaken for Cherry
- the embedded path renders no connect-wallet UI whatsoever
- room context, room leaderboard, profile navigation
- `walletDisconnected` disables every action without losing on-screen state
- the blink card renders from the host-supplied snapshot, including the
  share-picker preview, and ignores a tampered query string
- launch-token verification: signature, audience, expiry, and rejection of an
  unaudienced token
- the production CSP is an allowlist, carries `frame-ancestors` for
  chat.cherry.fun, sets no `X-Frame-Options`, and the app runs under it with
  zero violations

**Not verified**, and honestly cannot be from here:

- a real Cherry handshake against a real host (needs a registered `app_id`)
- the Cherry signing sheet on a real device
- anything on devnet — this environment has no Solana toolchain and cannot
  reach `api.devnet.solana.com` or `release.anza.xyz`. That path is
  `.github/workflows/devnet.yml`, which runs on a GitHub runner instead.
