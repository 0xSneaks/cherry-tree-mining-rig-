# THE CHERRY TREE — Cherry integrations

This project has **two** Cherry surfaces. They share a brand and nothing else
— different SDKs, different auth, opposite directions — and neither may absorb
the other.

| | Surface 1: mini app | Surface 2: chat embed |
|---|---|---|
| What | THE CHERRY TREE running **inside Cherry** | Cherry chat running **inside THE CHERRY TREE** |
| SDK | `@cherrydotfun/miniapp-sdk` 0.1.21 | `@cherrydotfun/chat-embed-sdk` 0.1.7 |
| Auth | launch token (RS256/JWKS), Cherry wallet bridge | `app-trusted+wallet` — backend token + wallet signature |
| Secret | none | `CHERRY_APP_SECRET`, server-side only |
| Identifier | `CHERRY_APP_ID` (blocked, not yet issued) | `NEXT_PUBLIC_CHERRY_CHAT_APP_ID` (issued) |
| Renders when | `isInsideCherry({ strict: true })` | **not** inside Cherry, and configured |

The embed deliberately never mounts inside the mini app: there the user is
already in the room, so framing Cherry inside a Cherry mini app would be
recursive. `CommunityChat` enforces that with the same strict environment
check the mini-app path uses, and there is a browser test for it.

Section 5 covers the chat embed. Sections 1–4 cover the mini app.

---

# Cherry mini-app integration

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

Deployment URL:      https://cherry-tree-mining-rig.vercel.app
Origin:              https://cherry-tree-mining-rig.vercel.app
  (see §3 — Vercel is the CURRENT production host: a Node host serves
   `frame-ancestors` for Cherry WEB and every `/api/*` route the mini-app
   backend needs. The former GitHub Pages origin is historical only.)

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
                     Program ID: C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt
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

**Current production host: Vercel — `https://cherry-tree-mining-rig.vercel.app`.**
It is a Node host, so `src/proxy.ts` serves the nonce CSP (with
`frame-ancestors` for Cherry web) on every HTML response and every `/api/*`
route the mini-app backend needs (launch-token session, room leaderboard,
blink SSR, chat embed) runs there. This is the URL and origin registered with
Cherry (§1).

| Host | Cherry mobile | Cherry web | Why |
|---|---|---|---|
| **Vercel (current production)** | yes | yes | `src/proxy.ts` serves the nonce CSP and headers directly; full Node `/api/*` |
| Cloudflare Pages | yes | yes | reads the `_headers` file the build already emits |
| Netlify | yes | yes | reads the same `_headers` file |
| GitHub Pages (historical) | yes | **no** | Pages cannot send custom response headers, at all. There is no configuration that changes this. |

GitHub Pages is retained only as historical/secondary information: the
`pages.yml` workflow still publishes the static export there, but that origin
cannot serve `frame-ancestors` (Cherry web) or any `/api` route, and it is not
the registered submission URL. The static build writes `out/_headers` from the
same `app/src/lib/csp.ts` that the Node deployment serves, so pointing
Cloudflare Pages or Netlify at this repository would be the whole change — no
code edit, no second copy of the policy to drift.

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

---

## 5. Chat embed (`@cherrydotfun/chat-embed-sdk` 0.1.7)

Cherry chat, embedded in the standalone app. Path **A (web)** per Cherry's own
`cherry-embed-integration` skill — a Next.js browser frontend, no
`react-native` dependency, no `pubspec.yaml`.

| | |
|---|---|
| App id | `ee009264-9c63-47cc-b346-46a44b25ff29` |
| Room id | `934ad16c-61b7-4cdb-b015-cd61dd217df4` |
| Auth mode | `app-trusted+wallet` |
| Mode / position | `single` / `inline` |
| Frame origin | `https://embed.cherry.fun` (`embedUrl`; no `apiUrl` — the iframe calls Cherry from its own origin) |

### Auth: two independent proofs

Cherry trusts a session only when both hold:

1. **This backend vouches for it** — an HS256 JWT with exactly `sub` and
   `app_id` (not `walletAddress`/`appId`), 5-minute expiry, a fresh `jti`,
   signed with `CHERRY_APP_SECRET`. `src/lib/server/embedToken.ts`, checked
   against the RFC 7515 A.1 vector.
2. **The user holds the wallet** — a live signature through
   `signChallengeHandler`, registered in the constructor, never after
   `mount()` (that races the iframe's first request).

### Why the token route takes three fields, not one

Cherry's example posts `{ walletAddress }`, alongside the rule that the route
must authenticate the caller first and never take `sub` from the request body
— room access, bans and rate limits are all enforced against that `sub`, so an
unproven one lets a client inherit another wallet's identity.

THE CHERRY TREE has no accounts: identity *is* the wallet. So the login step is
an ownership proof, and the body is `{ walletAddress, nonce, signature }` —
the extra two fields are what make the first one trustworthy.

```
POST /api/cherry-embed-nonce   ->  { nonce, message }      server-issued, HMAC-stamped, 2-minute TTL
   wallet signs `message`
POST /api/cherry-embed-token   ->  { token }               verifies Ed25519 over the nonce, then mints
```

Stateless by construction: the nonce carries its own timestamp and MAC, so
nothing is remembered between the two calls and it works unchanged across
serverless instances. The MAC key is derived from the app secret with a domain
separator, so there is no second secret to configure and the app secret is
never itself the MAC key.

`tests/embedToken.test.ts` exercises this against real crypto — including the
case that matters: Alice signs, Bob's address is claimed, request refused.

### The secret

`CHERRY_APP_SECRET` is an HMAC key, read only under `src/lib/server/`, never
prefixed `NEXT_PUBLIC_`, never returned in a response, never logged. Unset,
both routes answer `503` and the chat stays in read-only preview — it does not
fall back to a weaker mode. Verified by tests, by the readiness script, and by
grepping the built client bundle.

`appId` and `roomId` are different in kind: public identifiers the SDK takes
in browser code, so they are `NEXT_PUBLIC_` and ship in the bundle by design.

### This needs a server

`app-trusted+wallet` needs the two `/api/cherry-embed-*` routes, and the
static export has none — the Pages workflow deletes `src/app/api` outright.
`next.config.ts` therefore **fails the build** when `STATIC_EXPORT=1` and the
chat ids are set, rather than shipping a chat whose every sign-in 404s.

So: the static export (GitHub Pages) can serve the mine, but not the chat. The
chat needs a host that runs Node — which is the current production host
already: Vercel (§3), the same host that serves Cherry web's `frame-ancestors`
header.

### The embed is opt-in twice over

With the ids unset: `CommunityChat` renders nothing, the SDK is never imported
(it is behind a dynamic `import()`), and the CSP keeps `frame-src 'none'`.
Set them and `frame-src` widens by exactly one origin. Nothing else in the
policy moves; `frame-ancestors` still names `chat.cherry.fun`.

### Theme — seeds, not a palette

The embed runs a calibrated derivation engine: give it 2–4 honest brand
colours and it derives ~60 variables with contrast floors baked in.
Hand-tuning the granular variables fights that engine and lands on a
tint-over-default look, so there are exactly three seeds plus `mode`:

| Seed | Value | Role | From |
|---|---|---|---|
| `mode` | `dark` | the app's real content surface | — |
| `primaryColor` | `#FF5BA8` | CTA → own bubble + send button | Cherry's stored value |
| `backgroundColor` | `#080c16` | content surface | `--bg` |
| `accentColor` | `#ff8fbf` | links + mentions | `--blossom-soft` |

`incomingBubbleColor` is deliberately **omitted**. It is worth passing only
when a soft tinted brand fill genuinely separates from the page ground; ours
(`--bg-panel` `#0f1526`) sits at **1.075** against `--bg` — a near-identical
neutral navy, not a second brand colour — so the engine derives a better
bubble than we would.

Measured against the guide's floors:

| Check | Ratio | Floor |
|---|---|---|
| body text on background | 16.55:1 | 4.5 |
| accent on background | 9.24:1 | 4.5 |
| send glyph on primary | 6.81:1 | 3 |

**On `#FF5BA8` vs `--blossom` `#ff5fa2`:** three hex digits apart, a 1.0132
contrast ratio — the same pink. Cherry's stored value is passed through
verbatim rather than silently substituted. To make them literally identical,
change `--blossom` in `globals.css`; that moves the app and the chat together.

### Allowed origins

The deployment origin must be in the embed's **Allowed origins** at
portal.cherry.fun, or the iframe fails with an origin mismatch. Because that
is not yet confirmed, `pages.yml` reads the ids from repository *variables*
(`CHERRY_CHAT_APP_ID`, `CHERRY_CHAT_ROOM_ID`) and ships with them unset.

Note the static-export guard above: setting those variables on the Pages
workflow will now **fail the build** rather than deploy a broken chat. That is
deliberate — it is the signal to move the deployment to a Node host first.
