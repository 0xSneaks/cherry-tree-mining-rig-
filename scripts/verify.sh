#!/usr/bin/env bash
#
# Everything that can be verified without a live chain or a real Cherry host.
# Run this before claiming anything works.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> cargo fmt --check"
cargo fmt --all -- --check

echo "==> cargo clippy"
# Anchor's macros emit unavoidable cfg warnings, so this is not -D warnings;
# the filter below fails the run on anything originating in our own code.
cargo clippy --workspace --all-targets 2>&1 \
  | grep -E '^(warning|error)' \
  | grep -vE 'anchor-debug|custom-panic|custom-heap|condition value: .solana|deprecated method|generated [0-9]+ warning' \
  && { echo "!! clippy findings in first-party code"; exit 1; }
echo "    clean"

echo "==> cargo test (program + engine, 35 tests)"
RUST_LOG=error cargo test --workspace

echo "==> regenerating cross-language fixtures"
# One test per invocation: the two emitters interleave on stdout otherwise.
RUST_LOG=error cargo test -p rigs-engine --test vectors -- \
  --exact emit_pricing_vectors --nocapture 2>/dev/null \
  | sed -n '/---QUOTES---/,/---END---/p' > app/tests/fixtures/pricing-vectors.txt
RUST_LOG=error cargo test -p rigs-engine --test vectors -- \
  --exact emit_accrual_vectors --nocapture 2>/dev/null \
  | sed -n '/---ACCRUAL---/,/---END---/p' > app/tests/fixtures/accrual-vectors.txt
git diff --quiet app/tests/fixtures || {
  echo "!! fixtures drifted — the TypeScript port is out of step with the engine"
  exit 1
}

cd app
echo "==> tsc --noEmit"
npx tsc --noEmit

echo "==> vitest (57 tests)"
npx vitest run

echo "==> next build"
npx next build

echo "==> playwright (22 tests)"
PLAYWRIGHT_CHROMIUM_PATH="${PLAYWRIGHT_CHROMIUM_PATH:-/opt/pw-browsers/chromium}" \
  npx playwright test

echo
echo "All green."
