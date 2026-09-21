#!/usr/bin/env bash
#
# THE CHERRY TREE — two-wallet devnet smoke test.
#
# DEVNET ONLY. Thin entrypoint: the cluster guard runs here AND again inside
# the TypeScript, so neither path can be bypassed.
#
#   ./scripts/devnet-smoke.sh [rigsA] [rigsB] [waitSeconds]
#
# The implementation lives in app/scripts/ops/ so it resolves @solana/kit and
# reuses the same PDA derivation, decoders and pricing the frontend uses —
# one implementation, no second copy to drift.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/guard.sh
source "$ROOT/scripts/lib/guard.sh"

load_env "$ROOT"
: "${SOLANA_CLUSTER:=devnet}"
: "${SOLANA_RPC_URL:=https://api.devnet.solana.com}"
require_devnet
require_cmd node

[[ -d "$ROOT/app/node_modules" ]] || die "app dependencies are not installed. Run: cd app && npm ci"

# Run through tsx: the shared modules under app/src use extensionless imports
# (what the Next.js bundler wants), which bare Node ESM will not resolve.
cd "$ROOT/app"
exec npx --no-install tsx "scripts/ops/devnet-smoke.ts" "$@"
