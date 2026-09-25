#!/usr/bin/env bash
# Qualification-only migration entrypoint. Never usable against canonical ID.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/guard.sh
source "$ROOT/scripts/lib/guard.sh"

# This value deliberately lives outside guard.sh. qualification-build.sh
# rewrites the staged guard's EXPECTED_PROGRAM_ID to the disposable identity,
# while this deny-list value must remain the canonical production identity.
CANONICAL_PROGRAM_ID="C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt"

load_env "$ROOT"
: "${SOLANA_CLUSTER:=devnet}"
: "${SOLANA_RPC_URL:=https://api.devnet.solana.com}"
require_devnet
require_cmd node

[[ "${CHERRY_TREE_QUALIFICATION:-}" == "1" ]] \
  || die "CHERRY_TREE_QUALIFICATION=1 is required"
[[ -n "${CHERRY_TREE_QUALIFICATION_PROGRAM_ID:-}" ]] \
  || die "CHERRY_TREE_QUALIFICATION_PROGRAM_ID is required"
[[ "$CHERRY_TREE_QUALIFICATION_PROGRAM_ID" != "$CANONICAL_PROGRAM_ID" ]] \
  || die "canonical program ID is forbidden as a qualification target"
[[ "$EXPECTED_PROGRAM_ID" == "$CHERRY_TREE_QUALIFICATION_PROGRAM_ID" ]] \
  || die "staged source declares $EXPECTED_PROGRAM_ID, but qualification target is $CHERRY_TREE_QUALIFICATION_PROGRAM_ID"
check_program_id_consistency "$ROOT" >/dev/null
[[ -d "$ROOT/app/node_modules" ]] \
  || die "app dependencies are not installed. Run: cd app && npm ci"

cd "$ROOT/app"
exec npx --no-install tsx scripts/ops/migrate-v2.ts "$@"
