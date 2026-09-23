#!/usr/bin/env bash
#
# THE CHERRY TREE — devnet build and deploy.
#
# DEVNET ONLY. This script refuses mainnet outright.
#
# It builds the real SBF artifact and deploys it. It does NOT initialise the
# program and it does NOT fund rewards — those are separate, explicit steps
# (scripts/devnet-init.ts, scripts/devnet-fund.ts).
#
#   SOLANA_CLUSTER=devnet \
#   ANCHOR_WALLET=/path/to/cherry-tree-authority.json \
#   ./scripts/devnet-deploy.sh

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/guard.sh
source "$ROOT/scripts/lib/guard.sh"
# shellcheck source=lib/sbf.sh
source "$ROOT/scripts/lib/sbf.sh"

load_env "$ROOT"
: "${SOLANA_CLUSTER:=devnet}"
: "${SOLANA_RPC_URL:=https://api.devnet.solana.com}"

echo "=============================================="
echo " THE CHERRY TREE — devnet deploy"
echo "=============================================="

echo
echo "--> cluster guard"
require_devnet
green "  cluster=$SOLANA_CLUSTER rpc=$SOLANA_RPC_URL"

echo
echo "--> toolchain"
require_cmd solana solana-keygen anchor cargo
echo "  solana  $(solana --version)"
echo "  anchor  $(anchor --version)"
resolve_cargo_build_sbf || die "no usable cargo-build-sbf (see above)"
echo "  sbf     $CARGO_BUILD_SBF ($CARGO_BUILD_SBF_VERSION), platform-tools $SBF_TOOLS_VERSION"

echo
echo "--> deployer wallet"
[[ -n "${ANCHOR_WALLET:-}" ]] || die "ANCHOR_WALLET is not set (path to the authority keypair JSON)"
DEPLOYER=$(wallet_pubkey "$ANCHOR_WALLET")
# Only the public key is ever printed.
echo "  keypair file  $ANCHOR_WALLET"
echo "  public key    $DEPLOYER"

solana config set --url "$SOLANA_RPC_URL" --keypair "$ANCHOR_WALLET" >/dev/null

echo
echo "--> cluster identity (asked of the chain, not the config)"
assert_connected_to_devnet "$SOLANA_RPC_URL"

echo
echo "--> deployer balance"
BALANCE=$(solana balance "$DEPLOYER" --url "$SOLANA_RPC_URL")
echo "  devnet balance $BALANCE"

case "$BALANCE" in
  0\ SOL|0\ *) die "deployer has no devnet SOL. Run: solana airdrop 2 $DEPLOYER --url $SOLANA_RPC_URL" ;;
esac

echo
echo "--> program ID consistency (pre-build)"
check_program_id_consistency "$ROOT"

echo
echo "--> SBF build (real artifact, pinned toolchain)"
cd "$ROOT"
build_sbf || die "SBF build failed"

# `anchor build` used to emit these as a second step after the SBF compile.
# Same generator (host `cargo test` with the idl-build feature, not SBF), same
# Anchor.toml settings, same output paths. `anchor deploy` reads the IDL only
# to stamp the program address into it; it never uploads it.
echo "--> IDL (anchor idl build)"
mkdir -p target/idl target/types
anchor idl build -p sol_rigs -o target/idl/sol_rigs.json -t target/types/sol_rigs.ts \
  || die "IDL build failed"
echo "  idl       $ROOT/target/idl/sol_rigs.json"

SO_PATH="$ROOT/target/deploy/sol_rigs.so"
[[ -f "$SO_PATH" ]] || die "expected artifact not produced: $SO_PATH"
echo "  artifact  $SO_PATH ($(wc -c < "$SO_PATH") bytes)"

echo
echo "--> program ID consistency (built artifact)"
KEYPAIR_ID=$(solana-keygen pubkey "$ROOT/target/deploy/sol_rigs-keypair.json" 2>/dev/null || true)
echo "  target/deploy keypair  ${KEYPAIR_ID:-<none>}"
if [[ -n "$KEYPAIR_ID" && "$KEYPAIR_ID" != "$EXPECTED_PROGRAM_ID" ]]; then
  die "the build's program keypair is $KEYPAIR_ID but the source declares $EXPECTED_PROGRAM_ID.
     Place the correct program keypair at target/deploy/sol_rigs-keypair.json.
     NOT rewriting declare_id! or Anchor.toml to match — that would silently
     change the deployed address."
fi

echo
echo "--> deploy"
anchor deploy --provider.cluster "$SOLANA_RPC_URL" --provider.wallet "$ANCHOR_WALLET" \
  || die "anchor deploy failed"

echo
echo "--> verify on chain"
solana program show "$EXPECTED_PROGRAM_ID" --url "$SOLANA_RPC_URL" \
  || die "program $EXPECTED_PROGRAM_ID is not on devnet after deploy"

echo
green "DEPLOYED: $EXPECTED_PROGRAM_ID on $SOLANA_CLUSTER"
echo
echo "Next, as separate explicit steps:"
echo "  node --experimental-strip-types scripts/devnet-init.ts"
echo "  node --experimental-strip-types scripts/devnet-fund.ts <amountSol> <durationSeconds>"
