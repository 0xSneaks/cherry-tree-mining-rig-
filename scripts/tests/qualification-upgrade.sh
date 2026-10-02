#!/usr/bin/env bash
# Offline command-shape, capacity, and padded-hash tests for qualification upgrade.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CANONICAL_ID="C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt"
QUALIFICATION_ID="So11111111111111111111111111111111111111112"
AUTHORITY_ID="BPFLoaderUpgradeab1e11111111111111111111111"
RPC_URL="https://api.devnet.solana.com"
TEST_ROOT=$(mktemp -d)
trap 'rm -rf "$TEST_ROOT"' EXIT
FIXTURE="$TEST_ROOT/staged"
mkdir -p "$FIXTURE/scripts/lib" "$FIXTURE/programs/sol-rigs/src" \
  "$FIXTURE/app/src/lib/program" "$FIXTURE/target/deploy" "$FIXTURE/bin"
cp "$ROOT/scripts/qualification-upgrade-v2.sh" "$FIXTURE/scripts/qualification-upgrade-v2.sh"
cp "$ROOT/scripts/verify-program-dump.sh" "$FIXTURE/scripts/verify-program-dump.sh"
cp "$ROOT/scripts/lib/guard.sh" "$FIXTURE/scripts/lib/guard.sh"
cp "$ROOT/programs/sol-rigs/src/lib.rs" "$FIXTURE/programs/sol-rigs/src/lib.rs"
cp "$ROOT/Anchor.toml" "$FIXTURE/Anchor.toml"
cp "$ROOT/app/src/lib/program/constants.ts" "$FIXTURE/app/src/lib/program/constants.ts"
for identity_file in \
  "$FIXTURE/scripts/lib/guard.sh" \
  "$FIXTURE/programs/sol-rigs/src/lib.rs" \
  "$FIXTURE/Anchor.toml" \
  "$FIXTURE/app/src/lib/program/constants.ts"
do
  sed -i "s/$CANONICAL_ID/$QUALIFICATION_ID/g" "$identity_file"
done
chmod +x "$FIXTURE/scripts/qualification-upgrade-v2.sh" "$FIXTURE/scripts/verify-program-dump.sh"

ARTIFACT="$FIXTURE/target/deploy/sol_rigs.so"
truncate -s 426920 "$ARTIFACT"
printf 'qualification-v2' | dd of="$ARTIFACT" conv=notrunc status=none
AUTHORITY="$TEST_ROOT/authority.json"
printf '{}\n' > "$AUTHORITY"
LOG="$TEST_ROOT/solana.log"
SHOW_COUNT="$TEST_ROOT/show-count"
printf '0\n' > "$SHOW_COUNT"

cat > "$FIXTURE/bin/solana" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$1" == "genesis-hash" ]]; then
  printf '%s\n' 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG'
elif [[ "$1" == "program" && "$2" == "show" ]]; then
  count=$(<"$MOCK_SHOW_COUNT")
  count=$((count + 1))
  printf '%s\n' "$count" > "$MOCK_SHOW_COUNT"
  if (( count == 1 )); then capacity=398168; else capacity=430936; fi
  printf '{"dataLen":%s,"authority":"%s"}\n' "$capacity" "$MOCK_AUTHORITY_ID"
elif [[ "$1" == "program" && "$2" == "extend" ]]; then
  printf '%s\n' "$*" >> "$MOCK_LOG"
elif [[ "$1" == "program" && "$2" == "deploy" ]]; then
  printf '%s\n' "$*" >> "$MOCK_LOG"
elif [[ "$1" == "program" && "$2" == "dump" ]]; then
  printf '%s\n' "$*" >> "$MOCK_LOG"
  cp "$MOCK_ARTIFACT" "$4"
  truncate -s 430936 "$4"
else
  printf 'unexpected mock solana command: %s\n' "$*" >&2
  exit 90
fi
EOF
cat > "$FIXTURE/bin/solana-keygen" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == "pubkey" ]]
printf '%s\n' "$MOCK_AUTHORITY_ID"
EOF
chmod +x "$FIXTURE/bin/solana" "$FIXTURE/bin/solana-keygen"

OUTPUT=$(env -i PATH="$FIXTURE/bin:/usr/bin:/bin" HOME="$TEST_ROOT/home" \
  MOCK_SHOW_COUNT="$SHOW_COUNT" MOCK_AUTHORITY_ID="$AUTHORITY_ID" \
  MOCK_LOG="$LOG" MOCK_ARTIFACT="$ARTIFACT" \
  SOLANA_CLUSTER=devnet SOLANA_RPC_URL="$RPC_URL" \
  CHERRY_TREE_QUALIFICATION=1 \
  CHERRY_TREE_QUALIFICATION_PROGRAM_ID="$QUALIFICATION_ID" \
  ANCHOR_WALLET="$AUTHORITY" \
  "$FIXTURE/scripts/qualification-upgrade-v2.sh" 2>&1)
[[ "$OUTPUT" == *"verified capacity   430936 bytes"* ]]
[[ "$OUTPUT" == *"4016 trailing bytes are zero"* ]]
grep -Fxq "program extend $QUALIFICATION_ID 32768 --url $RPC_URL --keypair $AUTHORITY" "$LOG"
grep -F "program deploy $ARTIFACT --program-id $QUALIFICATION_ID" "$LOG" \
  | grep -Fq -- "--upgrade-authority $AUTHORITY --keypair $AUTHORITY --fee-payer $AUTHORITY --url $RPC_URL --no-auto-extend"
[[ "$(<"$SHOW_COUNT")" == "2" ]]
printf 'PASS: guarded upgrade explicitly extends, verifies capacity, and disables auto-extension\n'

SMALL_ARTIFACT="$TEST_ROOT/artifact.so"
PADDED_DUMP="$TEST_ROOT/padded.so"
BAD_PREFIX="$TEST_ROOT/bad-prefix.so"
BAD_SUFFIX="$TEST_ROOT/bad-suffix.so"
printf 'artifact-bytes' > "$SMALL_ARTIFACT"
cp "$SMALL_ARTIFACT" "$PADDED_DUMP"
truncate -s 32 "$PADDED_DUMP"
"$ROOT/scripts/verify-program-dump.sh" "$SMALL_ARTIFACT" "$PADDED_DUMP" >/dev/null
printf 'PASS: zero-padded deployed dump is accepted\n'

cp "$PADDED_DUMP" "$BAD_PREFIX"
printf 'X' | dd of="$BAD_PREFIX" bs=1 seek=0 conv=notrunc status=none
if "$ROOT/scripts/verify-program-dump.sh" "$SMALL_ARTIFACT" "$BAD_PREFIX" >/dev/null 2>&1; then
  printf 'FAIL: corrupt artifact prefix was accepted\n' >&2
  exit 1
fi
printf 'PASS: corrupt artifact prefix is rejected\n'

cp "$PADDED_DUMP" "$BAD_SUFFIX"
printf 'X' | dd of="$BAD_SUFFIX" bs=1 seek=31 conv=notrunc status=none
if "$ROOT/scripts/verify-program-dump.sh" "$SMALL_ARTIFACT" "$BAD_SUFFIX" >/dev/null 2>&1; then
  printf 'FAIL: non-zero ProgramData padding was accepted\n' >&2
  exit 1
fi
printf 'PASS: non-zero ProgramData padding is rejected\n'

printf 'All qualification upgrade tests passed with a mocked Solana CLI; no transaction was sent.\n'
