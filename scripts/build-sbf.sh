#!/usr/bin/env bash
#
# THE CHERRY TREE — build the real SBF artifact with the pinned toolchain.
#
# No cluster, no wallet, no keys: this only compiles. See scripts/lib/sbf.sh
# for the pinned versions and why a bare `cargo build-sbf` is not used.
#
#   ./scripts/build-sbf.sh
#   CARGO_BUILD_SBF=/path/to/cargo-build-sbf ./scripts/build-sbf.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/sbf.sh
source "$ROOT/scripts/lib/sbf.sh"

resolve_cargo_build_sbf
echo "  runner         $CARGO_BUILD_SBF ($CARGO_BUILD_SBF_VERSION)"
echo "  platform-tools $SBF_TOOLS_VERSION"

cd "$ROOT"
build_sbf "$@"

SO_PATH="$ROOT/target/deploy/sol_rigs.so"
[[ -f "$SO_PATH" ]] || { echo "FAIL: expected artifact not produced: $SO_PATH" >&2; exit 1; }
echo "  artifact       $SO_PATH ($(wc -c < "$SO_PATH") bytes)"
