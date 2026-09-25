#!/usr/bin/env bash
#
# Stage and build a disposable qualification binary without mutating this
# worktree. Identity replacement is explicit, limited to the generated copy,
# and guarded by a disposable keypair whose public key must match the target.
#
#   CHERRY_TREE_QUALIFICATION=1 \
#   CHERRY_TREE_QUALIFICATION_PROGRAM_ID=<disposable id> \
#   CHERRY_TREE_QUALIFICATION_PROGRAM_KEYPAIR=/path/to/disposable.json \
#   SOLANA_CLUSTER=devnet ./scripts/qualification-build.sh v1|v2
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/guard.sh
source "$ROOT/scripts/lib/guard.sh"

load_env "$ROOT"
require_devnet
require_cmd git tar sed solana-keygen
[[ "${CHERRY_TREE_QUALIFICATION:-}" == "1" ]] \
  || die "CHERRY_TREE_QUALIFICATION=1 is required"
TARGET_ID="${CHERRY_TREE_QUALIFICATION_PROGRAM_ID:-}"
TARGET_KEYPAIR="${CHERRY_TREE_QUALIFICATION_PROGRAM_KEYPAIR:-}"
[[ -n "$TARGET_ID" ]] || die "CHERRY_TREE_QUALIFICATION_PROGRAM_ID is required"
[[ "$TARGET_ID" != "$EXPECTED_PROGRAM_ID" ]] \
  || die "canonical program ID is forbidden as a qualification target"
[[ -f "$TARGET_KEYPAIR" ]] || die "qualification program keypair file is missing"
KEYPAIR_ID="$(wallet_pubkey "$TARGET_KEYPAIR")"
[[ "$KEYPAIR_ID" == "$TARGET_ID" ]] \
  || die "qualification keypair is $KEYPAIR_ID, expected $TARGET_ID"

case "${1:-}" in
  v1) STAGE=v1; SOURCE_REF=faf95124010c995504621b92882b0922926558b5 ;;
  v2) STAGE=v2; SOURCE_REF=HEAD ;;
  *) die "usage: qualification-build.sh v1|v2" ;;
esac

check_program_id_consistency "$ROOT"
if [[ "$STAGE" == v2 ]]; then
  git -C "$ROOT" diff --quiet && git -C "$ROOT" diff --cached --quiet \
    || die "commit V2 before staging qualification source; uncommitted code is never omitted silently"
  [[ -z "$(git -C "$ROOT" ls-files --others --exclude-standard)" ]] \
    || die "commit or remove untracked V2 files before staging"
fi

OUT="$ROOT/target/qualification/$TARGET_ID/$STAGE"
[[ ! -e "$OUT" ]] || die "$OUT already exists; remove that exact disposable stage deliberately before rebuilding"
mkdir -p "$OUT/source" "$OUT/artifact"
git -C "$ROOT" archive "$SOURCE_REF" | tar -x -C "$OUT/source"

# These are the four canonical identity locks. Only the generated disposable
# copy is rewritten, and its guard is then required to agree on the new ID.
for file in \
  programs/sol-rigs/src/lib.rs \
  Anchor.toml \
  app/src/lib/program/constants.ts \
  scripts/lib/guard.sh
do
  sed -i "s/$EXPECTED_PROGRAM_ID/$TARGET_ID/g" "$OUT/source/$file"
done
mkdir -p "$OUT/source/target/deploy"
cp "$TARGET_KEYPAIR" "$OUT/source/target/deploy/sol_rigs-keypair.json"

echo "QUALIFICATION MODE: $STAGE from $SOURCE_REF"
echo "  disposable program ID  $TARGET_ID"
echo "  staged source           $OUT/source"
echo "  canonical source        unchanged"
# The canonical tree already passed its normal readiness lock above. Re-run
# the same four-way identity check inside the archive, using the staged guard
# whose expected ID was explicitly replaced. The full readiness script is not
# suitable inside target/: its git-ignore checks intentionally resolve to the
# enclosing canonical repository.
(
  # shellcheck source=/dev/null
  source "$OUT/source/scripts/lib/guard.sh"
  check_program_id_consistency "$OUT/source" >/dev/null
)

(
  cd "$OUT/source"
  ./scripts/build-sbf.sh -- --locked
)
cp "$OUT/source/target/deploy/sol_rigs.so" "$OUT/artifact/sol_rigs.so"
[[ -f "$OUT/artifact/sol_rigs.so" ]] || die "qualification artifact was not produced"
echo "  artifact                $OUT/artifact/sol_rigs.so"
echo "  keypair public key       $KEYPAIR_ID"
echo "STAGED ONLY. No deploy, upgrade, migration, funding, or activation occurred."
