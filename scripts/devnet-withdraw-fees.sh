#!/usr/bin/env bash
#
# THE CHERRY TREE — withdraw accrued protocol fees.
#
# DEVNET ONLY. Thin entrypoint: the cluster guard runs here AND again inside
# the TypeScript, so neither path can be bypassed.
#
#   ./scripts/devnet-withdraw-fees.sh <amountSol>
#   ./scripts/devnet-withdraw-fees.sh all
#
# The destination is NOT an argument. It is `config.fee_recipient`, pinned on
# chain; the program rejects a withdrawal to anything else. The authority signs
# the transaction, the recipient never signs, and no key for the recipient is
# needed anywhere.

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
exec npx --no-install tsx "scripts/ops/devnet-withdraw-fees.ts" "$@"
