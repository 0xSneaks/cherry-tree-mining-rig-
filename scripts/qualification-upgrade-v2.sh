#!/usr/bin/env bash
# Explicitly extend and upgrade a staged disposable qualification program.
# Run this only from the staged V2 source tree produced by qualification-build.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/guard.sh
source "$ROOT/scripts/lib/guard.sh"

# Unlike EXPECTED_PROGRAM_ID, qualification-build.sh never rewrites this file.
# The canonical identity therefore remains permanently forbidden here.
CANONICAL_PROGRAM_ID="C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt"
EXTEND_BYTES=32768

load_env "$ROOT"
require_devnet
require_cmd solana solana-keygen node wc sha256sum head tail od awk mktemp

[[ "${CHERRY_TREE_QUALIFICATION:-}" == "1" ]] \
  || die "CHERRY_TREE_QUALIFICATION=1 is required"
TARGET_ID="${CHERRY_TREE_QUALIFICATION_PROGRAM_ID:-}"
[[ -n "$TARGET_ID" ]] || die "CHERRY_TREE_QUALIFICATION_PROGRAM_ID is required"
[[ "$TARGET_ID" != "$CANONICAL_PROGRAM_ID" ]] \
  || die "canonical program ID is forbidden as a qualification target"
[[ "$EXPECTED_PROGRAM_ID" == "$TARGET_ID" ]] \
  || die "staged source declares $EXPECTED_PROGRAM_ID, but qualification target is $TARGET_ID"
check_program_id_consistency "$ROOT" >/dev/null

RPC_URL="${SOLANA_RPC_URL:-}"
AUTHORITY="${ANCHOR_WALLET:-}"
[[ -n "$RPC_URL" ]] || die "SOLANA_RPC_URL is required; qualification never uses the global Solana RPC config"
[[ -n "$AUTHORITY" ]] || die "ANCHOR_WALLET is required (qualification upgrade authority keypair)"
[[ -f "$AUTHORITY" ]] || die "qualification upgrade authority keypair is missing: $AUTHORITY"
AUTHORITY_ID=$(wallet_pubkey "$AUTHORITY")

ARTIFACT="${1:-$ROOT/target/deploy/sol_rigs.so}"
[[ -f "$ARTIFACT" ]] || die "staged V2 artifact is missing: $ARTIFACT"
ARTIFACT_SIZE=$(wc -c < "$ARTIFACT")

json_field() {
  node -e '
    const fs = require("node:fs");
    const value = JSON.parse(fs.readFileSync(0, "utf8"))[process.argv[1]];
    if (value === undefined || value === null || value === "") process.exit(2);
    process.stdout.write(String(value));
  ' "$1"
}

echo "--> qualification cluster identity"
assert_connected_to_devnet "$RPC_URL"

echo "--> qualification ProgramData before extension"
BEFORE_JSON=$(solana program show "$TARGET_ID" --url "$RPC_URL" --output json)
BEFORE_CAPACITY=$(printf '%s' "$BEFORE_JSON" | json_field dataLen) \
  || die "could not read ProgramData dataLen before extension"
ON_CHAIN_AUTHORITY=$(printf '%s' "$BEFORE_JSON" | json_field authority) \
  || die "could not read the on-chain upgrade authority"
[[ "$ON_CHAIN_AUTHORITY" == "$AUTHORITY_ID" ]] \
  || die "upgrade authority is $ON_CHAIN_AUTHORITY, but keypair is $AUTHORITY_ID"
[[ "$BEFORE_CAPACITY" =~ ^[0-9]+$ ]] || die "invalid ProgramData dataLen: $BEFORE_CAPACITY"
(( BEFORE_CAPACITY + EXTEND_BYTES >= ARTIFACT_SIZE )) \
  || die "$EXTEND_BYTES-byte extension is insufficient: capacity $BEFORE_CAPACITY, artifact $ARTIFACT_SIZE"
printf '  program             %s\n' "$TARGET_ID"
printf '  authority           %s\n' "$AUTHORITY_ID"
printf '  current capacity    %s bytes\n' "$BEFORE_CAPACITY"
printf '  V2 artifact         %s bytes\n' "$ARTIFACT_SIZE"
printf '  explicit extension  %s bytes\n' "$EXTEND_BYTES"

echo "--> extend ProgramData explicitly"
solana program extend "$TARGET_ID" "$EXTEND_BYTES" \
  --url "$RPC_URL" \
  --keypair "$AUTHORITY"

AFTER_JSON=$(solana program show "$TARGET_ID" --url "$RPC_URL" --output json)
AFTER_CAPACITY=$(printf '%s' "$AFTER_JSON" | json_field dataLen) \
  || die "could not read ProgramData dataLen after extension"
[[ "$AFTER_CAPACITY" =~ ^[0-9]+$ ]] || die "invalid ProgramData dataLen: $AFTER_CAPACITY"
(( AFTER_CAPACITY == BEFORE_CAPACITY + EXTEND_BYTES )) \
  || die "ProgramData capacity is $AFTER_CAPACITY; expected $((BEFORE_CAPACITY + EXTEND_BYTES)) after extension"
(( AFTER_CAPACITY >= ARTIFACT_SIZE )) \
  || die "extended ProgramData capacity $AFTER_CAPACITY is smaller than V2 artifact $ARTIFACT_SIZE"
printf '  verified capacity   %s bytes\n' "$AFTER_CAPACITY"

echo "--> upgrade with CLI auto-extension disabled"
solana program deploy "$ARTIFACT" \
  --program-id "$TARGET_ID" \
  --upgrade-authority "$AUTHORITY" \
  --keypair "$AUTHORITY" \
  --fee-payer "$AUTHORITY" \
  --url "$RPC_URL" \
  --no-auto-extend

DUMP_DIR=$(mktemp -d)
DUMP_PATH="$DUMP_DIR/deployed.so"
cleanup() {
  rm -f "$DUMP_PATH"
  rmdir "$DUMP_DIR" 2>/dev/null || true
}
trap cleanup EXIT

echo "--> verify deployed bytes"
solana program dump "$TARGET_ID" "$DUMP_PATH" --url "$RPC_URL"
"$ROOT/scripts/verify-program-dump.sh" "$ARTIFACT" "$DUMP_PATH"

green "UPGRADED: disposable qualification program $TARGET_ID on verified Devnet"
