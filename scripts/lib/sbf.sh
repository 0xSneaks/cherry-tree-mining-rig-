# The pinned SBF toolchain for THE CHERRY TREE.
#
# Sourced, not executed. Every script that builds the on-chain program goes
# through `resolve_cargo_build_sbf` + `build_sbf`, never a bare
# `cargo build-sbf` or `anchor build`.
#
# Why: `cargo build-sbf` lets cargo pick whichever `cargo-build-sbf` it finds
# first (~/.cargo/bin, then PATH). An old 2.3.0 runner with platform-tools
# v1.48 ships a Cargo that cannot parse edition-2024 transitive crates, so the
# build fails far from the cause. The proven combination is pinned here and
# anything older is refused up front.
#
# Proven: cargo-build-sbf 4.1.0, platform-tools v1.52 →
#   target/deploy/sol_rigs.so, `cargo test --workspace --locked` green.

SBF_TOOLS_VERSION="v1.52"
SBF_MIN_CARGO_BUILD_SBF="4.1.0"
SBF_MANIFEST="programs/sol-rigs/Cargo.toml"

# 0 when $1 >= $2 (dotted numeric versions).
sbf_version_ge() {
  [[ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -1)" == "$2" ]]
}

# Sets CARGO_BUILD_SBF to an absolute path of a runner >= the pinned minimum.
# Order: $CARGO_BUILD_SBF (explicit override), the Agave install's
# active_release, then PATH. Whatever is picked is version-checked; an old
# runner is a hard failure, never a silent fallback.
resolve_cargo_build_sbf() {
  local candidate="" version
  if [[ -n "${CARGO_BUILD_SBF:-}" ]]; then
    candidate="$CARGO_BUILD_SBF"
  elif [[ -x "$HOME/.local/share/solana/install/active_release/bin/cargo-build-sbf" ]]; then
    candidate="$HOME/.local/share/solana/install/active_release/bin/cargo-build-sbf"
  else
    candidate="$(command -v cargo-build-sbf || true)"
  fi

  if [[ -z "$candidate" || ! -x "$candidate" ]]; then
    echo "FAIL: cargo-build-sbf not found (need >= $SBF_MIN_CARGO_BUILD_SBF; install Agave v$SBF_MIN_CARGO_BUILD_SBF or set CARGO_BUILD_SBF)" >&2
    return 1
  fi

  version="$("$candidate" --version 2>/dev/null | head -1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
  if [[ -z "$version" ]] || ! sbf_version_ge "$version" "$SBF_MIN_CARGO_BUILD_SBF"; then
    echo "FAIL: $candidate is cargo-build-sbf ${version:-<unknown>}; need >= $SBF_MIN_CARGO_BUILD_SBF." >&2
    echo "      Older runners cannot parse edition-2024 crates. Install Agave v$SBF_MIN_CARGO_BUILD_SBF" >&2
    echo "      or point CARGO_BUILD_SBF at a newer cargo-build-sbf." >&2
    return 1
  fi

  CARGO_BUILD_SBF="$candidate"
  CARGO_BUILD_SBF_VERSION="$version"
  export CARGO_BUILD_SBF CARGO_BUILD_SBF_VERSION
}

# Builds the real SBF artifact with the pinned runner and platform tools.
# Run from the repository root. Extra arguments are passed through.
build_sbf() {
  "$CARGO_BUILD_SBF" --manifest-path "$SBF_MANIFEST" --tools-version "$SBF_TOOLS_VERSION" "$@"
}
