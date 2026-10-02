#!/usr/bin/env bash
#
# THE CHERRY TREE — deployment readiness.
#
# Checks the things that must be true BEFORE anyone runs a deploy. It does
# not touch a cluster and it does not weaken scripts/verify.sh, which remains
# the full test pipeline.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/guard.sh
source "$ROOT/scripts/lib/guard.sh"
cd "$ROOT"

FAILURES=0
pass() { green "  PASS  $*"; }
bad()  { red   "  FAIL  $*"; FAILURES=$((FAILURES + 1)); }
warn() { printf '  WARN  %s\n' "$*"; }

echo "=============================================="
echo " THE CHERRY TREE — deploy readiness"
echo "=============================================="

# ---------------------------------------------------------------- program ID
echo
echo "--> program ID consistency"
if check_program_id_consistency "$ROOT" >/tmp/cherry-tree-pid.txt 2>&1; then
  cat /tmp/cherry-tree-pid.txt
else
  cat /tmp/cherry-tree-pid.txt
  bad "program IDs disagree"
fi

# ---------------------------------------------------------------- branding
echo
echo "--> product-facing branding"
if grep -rn "SOL RIGS" --include="*.tsx" --include="*.ts" app/src 2>/dev/null | grep -v node_modules; then
  bad "'SOL RIGS' still appears in user-facing frontend code"
else
  pass "no stale 'SOL RIGS' in app/src"
fi
for needle in "PRODUCT_NAME = 'THE CHERRY TREE'"; do
  grep -q "$needle" app/src/lib/brand.ts \
    && pass "brand module declares THE CHERRY TREE" \
    || bad "app/src/lib/brand.ts does not declare the product name"
done
head -1 README.md | grep -q "THE CHERRY TREE" \
  && pass "README title is THE CHERRY TREE" \
  || bad "README title is not THE CHERRY TREE"

# ---------------------------------------------------------------- scripts
echo
echo "--> deployment scripts present and executable"
for s in devnet-deploy.sh devnet-init.sh devnet-fund.sh devnet-smoke.sh \
         devnet-migrate-v2.sh qualification-build.sh \
         qualification-upgrade-v2.sh verify-program-dump.sh; do
  if [[ -x "scripts/$s" ]]; then pass "scripts/$s"; else bad "scripts/$s missing or not executable"; fi
done
for s in app/scripts/ops/devnet-init.ts app/scripts/ops/devnet-fund.ts app/scripts/ops/devnet-smoke.ts; do
  [[ -f "$s" ]] && pass "$s" || bad "$s missing"
done

# ---------------------------------------------------------------- mainnet refusal
echo
echo "--> scripts refuse mainnet"
for s in devnet-deploy.sh devnet-init.sh devnet-fund.sh devnet-smoke.sh \
         devnet-migrate-v2.sh qualification-upgrade-v2.sh; do
  if SOLANA_CLUSTER=mainnet-beta "./scripts/$s" >/dev/null 2>&1; then
    bad "scripts/$s did NOT refuse mainnet-beta"
  else
    pass "scripts/$s refuses mainnet-beta"
  fi
done

# ---------------------------------------------------------------- secrets
echo
echo "--> no key material tracked"
TRACKED_KEYS=$(git ls-files \
  | grep -Ei '(keypair|authority|funder|deployer|wallet).*\.json$|^keys/|\.pem$|\.key$|id\.json$' \
  || true)
if [[ -n "$TRACKED_KEYS" ]]; then
  echo "$TRACKED_KEYS"
  bad "files that look like key material are tracked"
else
  pass "no tracked filenames look like key material"
fi

if git ls-files | grep -qE '(^|/)\.env$|(^|/)\.env\.local$'; then
  bad "a real .env file is tracked"
else
  pass "no .env committed (only .env.example)"
fi

# A 64-element byte array in a tracked file is what a leaked keypair looks like.
LEAKED=$(git ls-files -- '*.json' \
  | grep -v package-lock.json \
  | xargs grep -lE '^\[[0-9]{1,3}(,[0-9]{1,3}){63}\]$' 2>/dev/null || true)
if [[ -n "$LEAKED" ]]; then
  echo "$LEAKED"
  bad "a tracked JSON file contains a 64-byte array — that looks like a secret key"
else
  pass "no tracked JSON holds a 64-byte secret-key array"
fi

# .gitignore must protect keys WITHOUT swallowing legitimate config.
echo
echo "--> .gitignore is precise"
for f in app/package.json app/tsconfig.json app/package-lock.json; do
  git check-ignore -q "$f" && bad "$f is ignored — .gitignore is too broad" || pass "$f still tracked"
done
for f in keys/x.json cherry-tree-authority.json my-keypair.json smoke-a.json .env; do
  git check-ignore -q "$f" && pass "$f is ignored" || bad "$f is NOT ignored"
done

# ---------------------------------------------------------------- config docs
echo
echo "--> configuration documented"
for v in SOLANA_CLUSTER SOLANA_RPC_URL ANCHOR_WALLET CHERRY_TREE_REWARD_FUNDER \
         CHERRY_TREE_RIG_PRICE_LAMPORTS CHERRY_TREE_PURCHASE_FEE_BPS CHERRY_TREE_COMPOUND_FEE_BPS \
         CHERRY_TREE_GENESIS_CAP_RIGS CHERRY_APP_ID; do
  grep -q "^# *$v=\|^$v=" .env.example && pass ".env.example documents $v" \
    || bad ".env.example does not document $v"
done
[[ -f DEPLOY.md ]] && pass "DEPLOY.md runbook present" || bad "DEPLOY.md runbook missing"

# ---------------------------------------------------- qualification isolation
echo
echo "--> qualification lane is explicit and disposable"
QUAL_SCRIPT="scripts/qualification-build.sh"
MIGRATE_SCRIPT="scripts/devnet-migrate-v2.sh"
UPGRADE_SCRIPT="scripts/qualification-upgrade-v2.sh"
grep -q 'CHERRY_TREE_QUALIFICATION.*==.*1' "$QUAL_SCRIPT" \
  && pass "qualification requires explicit opt-in" \
  || bad "qualification does not require explicit opt-in"
grep -q 'canonical program ID is forbidden' "$QUAL_SCRIPT" \
  && pass "qualification rejects canonical program ID" \
  || bad "qualification does not reject canonical program ID"
grep -q 'require_devnet' "$QUAL_SCRIPT" \
  && pass "qualification is devnet-only" \
  || bad "qualification is not devnet-only"
grep -q 'target/qualification' "$QUAL_SCRIPT" \
  && pass "qualification artifacts are isolated and CI-distinguishable" \
  || bad "qualification artifact path is not isolated"
grep -q 'CANONICAL_PROGRAM_ID=' "$MIGRATE_SCRIPT" \
  && grep -q 'EXPECTED_PROGRAM_ID.*CHERRY_TREE_QUALIFICATION_PROGRAM_ID' "$MIGRATE_SCRIPT" \
  && pass "qualification migration separates canonical deny-list from staged identity" \
  || bad "qualification migration identity guards are incomplete"
grep -q 'program extend.*EXTEND_BYTES' "$UPGRADE_SCRIPT" \
  && grep -q -- '--no-auto-extend' "$UPGRADE_SCRIPT" \
  && pass "qualification upgrade explicitly extends before deploying" \
  || bad "qualification upgrade does not own explicit ProgramData extension"
grep -q 'verify-program-dump.sh' "$UPGRADE_SCRIPT" \
  && pass "qualification upgrade verifies padded deployed bytes safely" \
  || bad "qualification upgrade does not verify the deployed program dump"

# ---------------------------------------------------------------- no hardcoded wallets
echo
echo "--> no personal wallet hardcoded"
# Allowed besides the program ID: the official name-registry constants the
# identity checks must pin on-chain (an admin-settable registry would be a
# hole). None is a wallet; the founder WALLET lives in config, not source.
#   .skr (AllDomains): ANS program · ANS origin · .skr TLD parent ·
#     Name House program · .skr name house · TLD House program
#   .sns (SNS): name program · .sns TLD root · name tokenizer
#   SPL Token program
REGISTRY_ALLOW='ALTNSZ46uaAUU7XUV6awvdorLGqAsPwa9shm7h4uP2FK|3mX9b4AZaQehNoQGfckVcmgmA6bkBoFcbLj9RMmMyNcU|F3A8kuikEiu6k2399oSJ1PWfcJYDHqpwoQ2e8psSDNuF|NH3uX6FtVE2fNREAioP7hm5RaozotZxeL6khU1EHx51|CPD3hKJE51VuiDE5u5Nosx1u7MXWwKGPff8HFJTtpX4V|TLDHkysf5pCnKsVA4gXpNvmy7psXLPEu4LAdDJthT9S|namesLPneVptA9Z5rqUDD9tMTWEJwofgaYwp8cawRkX|58PwtjSDuFHuUkYjH9BYnnQKHfwo9reZhC2zMJv9JPkx|nftD3vbNkNqfj2Sd3HZwbpw4BxxKWr4AjGb9X38JeZk|TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
HARDCODED=$(grep -rnE "\"[1-9A-HJ-NP-Za-km-z]{43,44}\"" \
  programs/*/src crates/*/src 2>/dev/null \
  | grep -v "C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt" \
  | grep -vE "pubkey!\\(\"($REGISTRY_ALLOW)\"\\)" || true)
if [[ -n "$HARDCODED" ]]; then
  echo "$HARDCODED"
  bad "a base58 address other than the program ID is hardcoded in Rust source"
else
  pass "no wallet addresses hardcoded in Rust source (program ID + pinned name-registry constants only)"
fi

# ---------------------------------------------------------------- CSP
echo
echo "--> content security policy"
# One source, two consumers: next.config.ts serves these as response headers
# on a Node deployment, and scripts/emit-static-headers.ts writes the same
# policy into the static export's _headers file.
CSP_SRC="app/src/lib/csp.ts"
[[ -f "$CSP_SRC" ]] || bad "$CSP_SRC is missing — the CSP has no single source"

if grep -q "connect-src 'self' https: wss:" "$CSP_SRC" 2>/dev/null; then
  bad "connect-src is still a wildcard"
else
  pass "connect-src is an allowlist, not a wildcard"
fi
if grep -q "unsafe-eval" "$CSP_SRC" && ! grep -q "dev ? \[\"'unsafe-eval'\"\]" "$CSP_SRC"; then
  warn "'unsafe-eval' may be reachable in production — check $CSP_SRC"
else
  pass "'unsafe-eval' is dev-only"
fi
# `frame-src` is deliberately absent here: it is now conditional on the Cherry
# Chat embed being configured, and is asserted in the chat-embed section above.
for d in "object-src 'none'" "base-uri 'self'"; do
  grep -q "$d" "$CSP_SRC" && pass "CSP sets $d" || bad "CSP is missing $d"
done
# frame-ancestors is what lets Cherry web frame the app, and X-Frame-Options
# is what would silently block it. Both are asserted, not assumed.
grep -q "frame-ancestors 'self' \${CHERRY_HOST}" "$CSP_SRC" \
  && pass "CSP sets frame-ancestors for the Cherry host" \
  || bad "CSP is missing frame-ancestors for chat.cherry.fun"
# Matches the header being SET (a header key, or a setHeader call) rather than
# merely mentioned — the source comments explain at length why it is absent.
if grep -rniE "(key:[[:space:]]*['\"]x-frame-options|setHeader\([[:space:]]*['\"]x-frame-options|['\"]x-frame-options['\"][[:space:]]*:)" \
     app/src app/next.config.ts app/scripts 2>/dev/null | grep -q .; then
  bad "X-Frame-Options is set somewhere — it silently blocks the Cherry web iframe"
else
  pass "X-Frame-Options is never set"
fi
# A static host cannot send headers from Next, so the export ships _headers.
if [[ -f app/scripts/emit-static-headers.ts ]]; then
  pass "static export emits _headers for a header-capable host"
else
  bad "app/scripts/emit-static-headers.ts is missing — a static host would send no CSP"
fi

# --------------------------------------------------------- chat embed
echo
echo "--> Cherry Chat embed (second Cherry surface)"

# The mini app must survive the embed's existence. These are the two surfaces
# pointing in opposite directions; neither may absorb the other.
grep -q "@cherrydotfun/miniapp-sdk" app/package.json \
  && pass "the mini-app SDK is still a dependency" \
  || bad "the mini-app SDK is GONE — the Cherry mini app was replaced, not extended"
grep -q "@cherrydotfun/chat-embed-sdk" app/package.json \
  && pass "the chat-embed SDK is a dependency" \
  || bad "the chat-embed SDK is missing"

# --- the app secret must be server-side only ---
# Cherry's own verification step: these greps must find nothing in client
# source or in the build output. `src/lib/server/` is excluded because that is
# exactly where the secret is SUPPOSED to be read.
# Matches an actual READ (`process.env.CHERRY_APP_SECRET`) rather than the
# bare name, which the source comments legitimately mention when explaining
# why the key is server-side.
CLIENT_DIRS=(app/src/app app/src/components app/src/hooks app/src/lib)
LEAKS=$(grep -rln 'process\.env\.CHERRY_APP_SECRET' "${CLIENT_DIRS[@]}" 2>/dev/null \
          | grep -v '^app/src/lib/server/' || true)
if [[ -n "$LEAKS" ]]; then
  bad "CHERRY_APP_SECRET is read outside app/src/lib/server: $LEAKS"
else
  pass "CHERRY_APP_SECRET is read only under app/src/lib/server"
fi

# A NEXT_PUBLIC_ variable is inlined into the bundle by definition.
if grep -rqE "NEXT_PUBLIC_[A-Z_]*SECRET" app/src app/scripts .env.example 2>/dev/null; then
  bad "a NEXT_PUBLIC_ variable names a secret — it would ship in the bundle"
else
  pass "no NEXT_PUBLIC_ variable names a secret"
fi

# The built client bundle is the real proof. Only meaningful once a build
# exists; a fresh clone has none, so its absence is a note, not a failure.
if [[ -d app/.next/static ]]; then
  if grep -rql "CHERRY_APP_SECRET" app/.next/static 2>/dev/null; then
    bad "CHERRY_APP_SECRET appears in the built client bundle"
  else
    pass "the built client bundle contains no reference to the app secret"
  fi
else
  dim "  note  no build output to scan (run: cd app && npx next build)"
fi

# The routes that mint the token must exist and be server-side.
for route in app/src/app/api/cherry-embed-token/route.ts app/src/app/api/cherry-embed-nonce/route.ts; do
  [[ -f "$route" ]] && pass "$(basename "$(dirname "$route")") route present" \
    || bad "$route is missing — app-trusted+wallet needs it"
done
grep -q "server-only" app/src/lib/server/embedToken.ts \
  && pass "the token minter is marked server-only" \
  || bad "app/src/lib/server/embedToken.ts is not marked server-only"

# --- the token route must authenticate the caller ---
# A `sub` taken from the request body lets a client mint a token for any
# wallet and inherit its room access.
grep -q "verifyWalletProof" app/src/app/api/cherry-embed-token/route.ts \
  && pass "the token route proves wallet ownership before minting" \
  || bad "the token route mints without proving who the caller is"

# --- frame-src stays opt-in and narrow ---
if grep -q "cherryChatEnabled() ? CHERRY_EMBED_ORIGIN" app/src/lib/csp.ts; then
  pass "frame-src is 'none' unless the embed is configured"
else
  bad "frame-src is not gated on the embed being configured"
fi
grep -q "CHERRY_EMBED_ORIGIN = 'https://embed.cherry.fun'" app/src/lib/cherryChat.ts \
  && pass "the embed origin is one exact https origin" \
  || bad "the embed origin is missing or not pinned"

# --- boundaries ---
grep -q "useCherryEnvironment" app/src/components/CommunityChat.tsx \
  && pass "the embed checks for Cherry before mounting" \
  || bad "the embed does not check for Cherry — it could mount inside the mini app"
grep -q "STATIC_EXPORT && cherryChatEnabled()" app/next.config.ts \
  && pass "a static build refuses to ship a chat whose sign-in routes are absent" \
  || bad "a static build could ship the chat without its /api routes"

# ------------------------------------------------------- fee recipient
echo
echo "--> protocol fee recipient"
FEE_RECIPIENT="4FsYyXTbF1iftkUTtZVmzmvisxUaGvoYfZDj4DuGLvwF"

grep -q "CHERRY_TREE_FEE_RECIPIENT" .env.example \
  && pass ".env.example documents CHERRY_TREE_FEE_RECIPIENT" \
  || bad ".env.example does not document CHERRY_TREE_FEE_RECIPIENT"

# The address is pinned in the Rust test, which is what proves the deployed
# config carries it — not a comment somewhere.
grep -q "$FEE_RECIPIENT" programs/sol-rigs/tests/common/mod.rs \
  && pass "the program tests assert the Cherry Tree fee recipient" \
  || bad "the program tests do not pin the fee recipient address"

# `withdraw_fees` must not be free to choose a destination.
if grep -q "destination.key() == config.fee_recipient" programs/sol-rigs/src/instructions/admin.rs; then
  pass "withdraw_fees is pinned to config.fee_recipient"
else
  bad "withdraw_fees does not constrain its destination — fees could be redirected"
fi

# Changing it must be an authority action.
if grep -q "pub fn set_fee_recipient(ctx: Context<AdminOnly>" programs/sol-rigs/src/instructions/admin.rs; then
  pass "set_fee_recipient is authority-gated (AdminOnly)"
else
  bad "set_fee_recipient is missing or not authority-gated"
fi

# The recipient is a payout address, never a signer.
if grep -qE "fee_recipient:\s*Signer" programs/sol-rigs/src/instructions/*.rs; then
  bad "the fee recipient is declared as a Signer — collecting fees would need its key"
else
  pass "the fee recipient never signs; no key for it is ever required"
fi

# ---------------------------------------------------------------- SBF
echo
echo "--> SBF / Anchor toolchain"
# shellcheck source=lib/sbf.sh
source "$ROOT/scripts/lib/sbf.sh"
if command -v anchor >/dev/null 2>&1 && resolve_cargo_build_sbf 2>/dev/null; then
  pass "anchor and cargo-build-sbf $CARGO_BUILD_SBF_VERSION (platform-tools $SBF_TOOLS_VERSION) — scripts/devnet-deploy.sh can produce a real artifact"
else
  warn "SBF BUILD: BLOCKED — anchor and/or cargo-build-sbf >= $SBF_MIN_CARGO_BUILD_SBF are not installed here."
  warn "            The program's tests run the real Anchor entrypoint under"
  warn "            solana-program-test's NATIVE processor. That is NOT an SBF pass."
fi

echo
echo "--> devnet reachability"
if curl -s -o /dev/null --max-time 8 https://api.devnet.solana.com; then
  pass "https://api.devnet.solana.com reachable"
else
  warn "DEVNET: BLOCKED — https://api.devnet.solana.com is not reachable from here."
fi

echo
echo "--> Cherry registration"
if [[ -n "${CHERRY_APP_ID:-}" ]]; then
  pass "CHERRY_APP_ID is set"
else
  warn "CHERRY REAL HOST: BLOCKED — CHERRY_APP_ID REQUIRED (from Cherry mini-app registration)."
fi

echo
if [[ "$FAILURES" -gt 0 ]]; then
  red "NOT DEPLOY READY: $FAILURES check(s) failed."
  exit 1
fi
green "DEPLOY READY (subject to any WARN lines above)."
