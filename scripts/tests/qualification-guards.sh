#!/usr/bin/env bash
# Offline regression coverage for qualification migration wrapper guards.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CANONICAL_ID="C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt"
QUALIFICATION_ID="So11111111111111111111111111111111111111112"
OTHER_ID="BPFLoaderUpgradeab1e11111111111111111111111"
TEST_ROOT=$(mktemp -d)
trap 'rm -rf "$TEST_ROOT"' EXIT

make_fixture() {
  local name="$1" expected_id="$2" fixture
  fixture="$TEST_ROOT/$name"
  mkdir -p "$fixture/scripts/lib" "$fixture/app/node_modules" \
    "$fixture/programs/sol-rigs/src" "$fixture/app/src/lib/program" "$fixture/bin"
  cp "$ROOT/scripts/devnet-migrate-v2.sh" "$fixture/scripts/devnet-migrate-v2.sh"
  cp "$ROOT/scripts/lib/guard.sh" "$fixture/scripts/lib/guard.sh"
  cp "$ROOT/programs/sol-rigs/src/lib.rs" "$fixture/programs/sol-rigs/src/lib.rs"
  cp "$ROOT/Anchor.toml" "$fixture/Anchor.toml"
  cp "$ROOT/app/src/lib/program/constants.ts" "$fixture/app/src/lib/program/constants.ts"
  if [[ "$expected_id" != "$CANONICAL_ID" ]]; then
    for identity_file in \
      "$fixture/scripts/lib/guard.sh" \
      "$fixture/programs/sol-rigs/src/lib.rs" \
      "$fixture/Anchor.toml" \
      "$fixture/app/src/lib/program/constants.ts"
    do
      sed -i "s/$CANONICAL_ID/$expected_id/g" "$identity_file"
    done
  fi
  printf '#!/usr/bin/env bash\nprintf "FAKE_NPX %s\\n" "$*"\n' > "$fixture/bin/npx"
  chmod +x "$fixture/bin/npx" "$fixture/scripts/devnet-migrate-v2.sh"
  printf '%s\n' "$fixture"
}

expect_success() {
  local label="$1" expected="$2"
  shift 2
  local output
  if ! output=$("$@" 2>&1); then
    printf 'FAIL: %s unexpectedly failed:\n%s\n' "$label" "$output" >&2
    exit 1
  fi
  [[ "$output" == *"$expected"* ]] || {
    printf 'FAIL: %s did not contain %q:\n%s\n' "$label" "$expected" "$output" >&2
    exit 1
  }
  printf 'PASS: %s\n' "$label"
}

expect_failure() {
  local label="$1" expected="$2"
  shift 2
  local output
  if output=$("$@" 2>&1); then
    printf 'FAIL: %s unexpectedly succeeded:\n%s\n' "$label" "$output" >&2
    exit 1
  fi
  [[ "$output" == *"$expected"* ]] || {
    printf 'FAIL: %s did not contain %q:\n%s\n' "$label" "$expected" "$output" >&2
    exit 1
  }
  printf 'PASS: %s\n' "$label"
}

CANONICAL_FIXTURE=$(make_fixture canonical "$CANONICAL_ID")
STAGED_FIXTURE=$(make_fixture staged "$QUALIFICATION_ID")
SAFE_PATH="$STAGED_FIXTURE/bin:/usr/bin:/bin"

expect_success "canonical identity locks remain intact" "program IDs agree" \
  bash -c 'source "$1/scripts/lib/guard.sh"; check_program_id_consistency "$1"' _ "$CANONICAL_FIXTURE"

expect_failure "canonical source cannot run a disposable migration" \
  "staged source declares $CANONICAL_ID" \
  env -i PATH="$SAFE_PATH" HOME="$TEST_ROOT/home" SOLANA_CLUSTER=devnet \
    SOLANA_RPC_URL=https://api.devnet.solana.com CHERRY_TREE_QUALIFICATION=1 \
    CHERRY_TREE_QUALIFICATION_PROGRAM_ID="$QUALIFICATION_ID" \
    "$CANONICAL_FIXTURE/scripts/devnet-migrate-v2.sh"

expect_success "valid staged disposable ID reaches the migration command" "FAKE_NPX" \
  env -i PATH="$SAFE_PATH" HOME="$TEST_ROOT/home" SOLANA_CLUSTER=devnet \
    SOLANA_RPC_URL=https://api.devnet.solana.com CHERRY_TREE_QUALIFICATION=1 \
    CHERRY_TREE_QUALIFICATION_PROGRAM_ID="$QUALIFICATION_ID" \
    "$STAGED_FIXTURE/scripts/devnet-migrate-v2.sh"

expect_failure "canonical ID is rejected in qualification mode" \
  "canonical program ID is forbidden" \
  env -i PATH="$SAFE_PATH" HOME="$TEST_ROOT/home" SOLANA_CLUSTER=devnet \
    SOLANA_RPC_URL=https://api.devnet.solana.com CHERRY_TREE_QUALIFICATION=1 \
    CHERRY_TREE_QUALIFICATION_PROGRAM_ID="$CANONICAL_ID" \
    "$STAGED_FIXTURE/scripts/devnet-migrate-v2.sh"

expect_failure "missing qualification opt-in is rejected" \
  "CHERRY_TREE_QUALIFICATION=1 is required" \
  env -i PATH="$SAFE_PATH" HOME="$TEST_ROOT/home" SOLANA_CLUSTER=devnet \
    SOLANA_RPC_URL=https://api.devnet.solana.com \
    CHERRY_TREE_QUALIFICATION_PROGRAM_ID="$QUALIFICATION_ID" \
    "$STAGED_FIXTURE/scripts/devnet-migrate-v2.sh"

expect_failure "mainnet is rejected" "MAINNET IS REFUSED" \
  env -i PATH="$SAFE_PATH" HOME="$TEST_ROOT/home" SOLANA_CLUSTER=mainnet-beta \
    SOLANA_RPC_URL=https://api.mainnet-beta.solana.com CHERRY_TREE_QUALIFICATION=1 \
    CHERRY_TREE_QUALIFICATION_PROGRAM_ID="$QUALIFICATION_ID" \
    "$STAGED_FIXTURE/scripts/devnet-migrate-v2.sh"

expect_failure "mismatched staged ID is rejected" \
  "staged source declares $QUALIFICATION_ID" \
  env -i PATH="$SAFE_PATH" HOME="$TEST_ROOT/home" SOLANA_CLUSTER=devnet \
    SOLANA_RPC_URL=https://api.devnet.solana.com CHERRY_TREE_QUALIFICATION=1 \
    CHERRY_TREE_QUALIFICATION_PROGRAM_ID="$OTHER_ID" \
    "$STAGED_FIXTURE/scripts/devnet-migrate-v2.sh"

printf 'All qualification migration guard tests passed without a transaction.\n'
