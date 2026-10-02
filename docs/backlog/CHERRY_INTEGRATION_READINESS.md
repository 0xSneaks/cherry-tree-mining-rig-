# Cherry integration readiness: CHERRY_APP_ID and real host / messenger (Backlog 15, 16)

**Status:** local side production-ready and tested against stubs. **No real Cherry host
validation has happened. No APP_ID exists.** Nothing below claims otherwise.

## 1. CHERRY_APP_ID — where it is consumed

| Location | Behaviour |
|---|---|
| `app/src/lib/server/launchToken.ts` `appId()` | Throws `LaunchTokenError` when unset → `verifySession` refuses every token. |
| `app/src/app/api/session/route.ts` | Via `requireSession` → fails closed (no session, no room attribution). |
| `app/src/app/api/room/[id]/leaderboard/route.ts` | Same → room leaderboard unavailable. |
| `app/src/app/inline/page.tsx` (blink SSR) | Launch token verification → blink card refuses to render ("expired/invalid"). |
| `.github/workflows/pages.yml` | Static export deletes `src/app/api`; the comment notes CHERRY_APP_ID does not exist. |
| `scripts/verify-deploy-ready.sh` | Prints WARN when absent. |

Fail-closed verified by existing tests (`tests/launchToken.test.ts` "rejects … unaudienced") and
now by `app/scripts/ops/check-production-env.ts` → `EXTERNAL` when unset, `FAIL` for placeholder
or test values (`sol-rigs-e2e`, `changeme`, `<…>`), tested in `tests/opsReadiness.test.ts`.

**Not affected by CHERRY_APP_ID:** the game itself (embedded rendering, wallet signing via the
Cherry bridge) — the client never needs the id. The new public growth APIs don't need it either.

## 2. Configuration path

```
CHERRY_APP_ID=<issued by Cherry>             # server env, required
CHERRY_EXPECTED_ORIGIN=https://<deployed origin>   # server env, recommended
CHERRY_JWKS_URL=                              # leave unset → https://chat.cherry.fun/.well-known/jwks.json
```

Validate: `cd app && npx tsx scripts/ops/check-production-env.ts path/to/env` → no FAIL,
CHERRY_APP_ID PASS.

## 3. Exact external founder actions

1. **Choose the production host — DONE. Vercel is the chosen and deployed production host**
   (`https://cherry-tree-mining-rig.vercel.app`): it sends the headers Cherry web needs
   (`frame-ancestors` via `src/proxy.ts`) and runs `/api/*`, the chat embed and the growth
   APIs. GitHub Pages cannot serve any of it and is historical only (CHERRY.md §3).
2. **Register the mini-app at https://portal.cherry.fun** using the packet in `CHERRY.md` §1,
   with the production URL/origin (`https://cherry-tree-mining-rig.vercel.app`), permissions
   `wallet:connect` + `inline:render`, and network details at the time of registration
   (the packet currently says devnet only — update it if/when mainnet is real).
3. Copy the issued id into `CHERRY_APP_ID` on the host; set `CHERRY_EXPECTED_ORIGIN`.
4. Add the production origin to the chat embed's **Allowed origins** (if the chat is used).
5. Ask Cherry the open questions in §4 and record the answers.

## 4. Host / messenger behaviour — open questions for Cherry (cannot be answered locally)

| Question | Why it matters | Local state |
|---|---|---|
| Does opening a shared mini-app link preserve the query string (`?ref=…`) into the WebView/iframe launch URL? | Referral links inside Cherry depend on it. The SDK documents only `cherry_embed=1` and blink `route`/`params`; no query-forwarding contract. | App reads `?ref=` identically in both modes (e2e "embedded: a new wallet sees the referrer"). If Cherry strips it, the **official fallback** still applies (when configured) and explicit links only work outside Cherry. |
| Is there a supported deep-link format for "open mini-app X with params"? | Would allow a signed referral param instead of a raw query. | Not built — would require Cherry to specify it. |
| Can a blink (`share({route, params})`) open the full app rather than the read-only card? | A referral-carrying blink. | Not built: SDK says blinks are read-only, no callbacks. |
| Launch token `origin` claim value for mobile WebView vs web iframe | Decides whether `CHERRY_EXPECTED_ORIGIN` can be pinned for both. | Optional env; pin after observing a real token. |
| JWKS rotation policy | Operational. | Default JWKS URL, cached by SDK. |

## 5. Local readiness checklist (all done)

- Strict environment detection (`isInsideCherry({ strict: true })`), no connect UI in embed.
- Signing via `cherry.wallet.signAndSendTransaction`, confirmed before UI moves.
- CSP: `frame-ancestors 'self' https://chat.cherry.fun`, no `X-Frame-Options`, `connect-src`
  allowlist; zero CSP violations under e2e.
- Error UX: `explainActionError` product messages; wallet disconnect disables actions.
- Blink card: signed params win over query; tamper-inert (e2e).
- Referral: explicit precedence + official fallback with disclosure (unit + e2e).
- Readiness: `check-production-env.ts`.

## 6. What only a real Cherry session can prove

Handshake with a real `CHERRY_APP_ID`; the real signing sheet; `?ref=` survival; inline render
and share into a real chat; suspend/resume behaviour on device. Covered as steps in
`SEEKER_DEVICE_TEST_PLAN.md`.
