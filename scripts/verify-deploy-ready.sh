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
for s in devnet-deploy.sh devnet-init.sh devnet-fund.sh devnet-smoke.sh; do
  if [[ -x "scripts/$s" ]]; then pass "scripts/$s"; else bad "scripts/$s missing or not executable"; fi
done
for s in app/scripts/ops/devnet-init.ts app/scripts/ops/devnet-fund.ts app/scripts/ops/devnet-smoke.ts; do
  [[ -f "$s" ]] && pass "$s" || bad "$s missing"
done

# ---------------------------------------------------------------- mainnet refusal
echo
echo "--> scripts refuse mainnet"
for s in devnet-deploy.sh devnet-init.sh devnet-fund.sh devnet-smoke.sh; do
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
         CHERRY_TREE_RIG_PRICE_LAMPORTS CHERRY_TREE_PURCHASE_FEE_BPS CHERRY_APP_ID; do
  grep -q "^# *$v=\|^$v=" .env.example && pass ".env.example documents $v" \
    || bad ".env.example does not document $v"
done
[[ -f DEPLOY.md ]] && pass "DEPLOY.md runbook present" || bad "DEPLOY.md runbook missing"

# ---------------------------------------------------------------- no hardcoded wallets
echo
echo "--> no personal wallet hardcoded"
HARDCODED=$(grep -rnE "\"[1-9A-HJ-NP-Za-km-z]{43,44}\"" \
  programs/*/src crates/*/src 2>/dev/null \
  | grep -v "By7qjU4ZYBYQvdkh41UjnhxWmLyP63S3PbdZXbg1UiKH" || true)
if [[ -n "$HARDCODED" ]]; then
  echo "$HARDCODED"
  bad "a base58 address other than the program ID is hardcoded in Rust source"
else
  pass "no wallet addresses hardcoded in Rust source (only the program ID)"
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
for d in "object-src 'none'" "frame-src 'none'" "base-uri 'self'"; do
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

# ---------------------------------------------------------------- SBF
echo
echo "--> SBF / Anchor toolchain"
if command -v anchor >/dev/null 2>&1 && cargo build-sbf --version >/dev/null 2>&1; then
  pass "anchor and cargo build-sbf available — scripts/devnet-deploy.sh can produce a real artifact"
else
  warn "SBF BUILD: BLOCKED — anchor and/or cargo build-sbf are not installed here."
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
