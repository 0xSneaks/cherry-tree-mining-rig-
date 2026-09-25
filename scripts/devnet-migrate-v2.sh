#!/usr/bin/env bash
# Qualification-only migration entrypoint. Never usable against canonical ID.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/guard.sh
source "$ROOT/scripts/lib/guard.sh"

load_env "$ROOT"
: "${SOLANA_CLUSTER:=devnet}"
: "${SOLANA_RPC_URL:=https://api.devnet.solana.com}"
require_devnet
require_cmd node

[[ "${CHERRY_TREE_QUALIFICATION:-}" == "1" ]] \
  || die "CHERRY_TREE_QUALIFICATION=1 is required"
[[ -n "${CHERRY_TREE_QUALIFICATION_PROGRAM_ID:-}" ]] \
  || die "CHERRY_TREE_QUALIFICATION_PROGRAM_ID is required"
[[ "$CHERRY_TREE_QUALIFICATION_PROGRAM_ID" != "$EXPECTED_PROGRAM_ID" ]] \
  || die "canonical program ID is forbidden as a qualification target"
[[ -d "$ROOT/app/node_modules" ]] \
  || die "app dependencies are not installed. Run: cd app && npm ci"

cd "$ROOT/app"
exec npx --no-install tsx scripts/ops/migrate-v2.ts "$@"
