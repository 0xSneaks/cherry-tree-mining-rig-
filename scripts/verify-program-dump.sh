#!/usr/bin/env bash
# Verify a possibly padded deployed-program dump against an SBF artifact.
set -euo pipefail

die() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }

[[ $# -eq 2 ]] || die "usage: verify-program-dump.sh <artifact.so> <program-dump.so>"
ARTIFACT="$1"
DUMP="$2"
[[ -f "$ARTIFACT" ]] || die "artifact is missing: $ARTIFACT"
[[ -f "$DUMP" ]] || die "program dump is missing: $DUMP"

require_cmd() {
  for command_name in "$@"; do
    command -v "$command_name" >/dev/null 2>&1 || die "required command not found: $command_name"
  done
}
require_cmd wc sha256sum head tail od awk

ARTIFACT_SIZE=$(wc -c < "$ARTIFACT")
DUMP_SIZE=$(wc -c < "$DUMP")
(( DUMP_SIZE >= ARTIFACT_SIZE )) \
  || die "program dump is $DUMP_SIZE bytes; artifact is $ARTIFACT_SIZE bytes"

ARTIFACT_HASH=$(sha256sum "$ARTIFACT" | awk '{print $1}')
PREFIX_HASH=$(head -c "$ARTIFACT_SIZE" "$DUMP" | sha256sum | awk '{print $1}')
[[ "$PREFIX_HASH" == "$ARTIFACT_HASH" ]] \
  || die "deployed program prefix SHA-256 does not match the artifact"

# An extended ProgramData account may dump its unused capacity after the
# artifact. That suffix is valid only when every byte is zero.
if (( DUMP_SIZE > ARTIFACT_SIZE )); then
  if ! tail -c "+$((ARTIFACT_SIZE + 1))" "$DUMP" \
      | od -An -v -tu1 \
      | awk '{ for (i = 1; i <= NF; i++) if ($i != 0) exit 1 }'; then
    die "program dump contains non-zero bytes after the artifact prefix"
  fi
fi

printf 'VERIFIED: %s-byte artifact prefix SHA-256 %s; %s trailing bytes are zero.\n' \
  "$ARTIFACT_SIZE" "$ARTIFACT_HASH" "$((DUMP_SIZE - ARTIFACT_SIZE))"
