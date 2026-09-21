# Shared guards for THE CHERRY TREE deployment scripts.
#
# Sourced, not executed. Every script in scripts/ that touches a cluster must
# call `require_devnet` before it does anything else.

set -euo pipefail

EXPECTED_PROGRAM_ID="By7qjU4ZYBYQvdkh41UjnhxWmLyP63S3PbdZXbg1UiKH"

red()   { printf '\033[31m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
dim()   { printf '\033[2m%s\033[0m\n' "$*"; }
die()   { red "FAIL: $*"; exit 1; }

# Load .env if present. Never echo its contents.
load_env() {
  local root="$1"
  if [[ -f "$root/.env" ]]; then
    set -a
    # shellcheck disable=SC1091
    source "$root/.env"
    set +a
    dim "loaded $root/.env"
  fi
}

# Hard-fail on anything that is not devnet. This is the single place that
# decides, so no script can drift onto mainnet by accident.
require_devnet() {
  local cluster="${SOLANA_CLUSTER:-}"
  local rpc="${SOLANA_RPC_URL:-}"

  [[ -n "$cluster" ]] || die "SOLANA_CLUSTER is not set. Refusing to guess."

  case "$cluster" in
    devnet) ;;
    mainnet|mainnet-beta|mainnet_beta)
      die "MAINNET IS REFUSED. These scripts are devnet-only." ;;
    *)
      die "SOLANA_CLUSTER='$cluster' is not devnet. Refusing." ;;
  esac

  # The RPC URL is checked independently: a devnet label pointed at a mainnet
  # endpoint is exactly the mistake worth catching.
  if [[ -n "$rpc" ]]; then
    case "$rpc" in
      *mainnet*)
        die "SOLANA_RPC_URL looks like mainnet: $rpc" ;;
      *devnet*|*localhost*|*127.0.0.1*) ;;
      *)
        red "WARNING: SOLANA_RPC_URL is not obviously devnet: $rpc"
        red "         Continuing only because SOLANA_CLUSTER=devnet."
        ;;
    esac
  fi
}

require_cmd() {
  for c in "$@"; do
    command -v "$c" >/dev/null 2>&1 || die "required command not found: $c"
  done
}

# Resolve the wallet path and print ONLY its public key. Secret material is
# never read into a variable, never echoed, and never logged.
wallet_pubkey() {
  local path="$1"
  [[ -n "$path" ]] || die "wallet path is empty"
  [[ -f "$path" ]] || die "wallet file does not exist: $path"
  solana-keygen pubkey "$path" 2>/dev/null || die "could not read a public key from $path"
}

# The four places a program ID appears must agree. Nothing here rewrites an
# ID: a mismatch is a hard failure for a human to resolve.
check_program_id_consistency() {
  local root="$1"
  local declared anchor_localnet anchor_devnet frontend

  declared=$(grep -oE 'declare_id!\("[^"]+"\)' "$root/programs/sol-rigs/src/lib.rs" \
    | head -1 | sed -E 's/.*"(.*)".*/\1/')
  anchor_localnet=$(grep -A2 '^\[programs.localnet\]' "$root/Anchor.toml" \
    | grep -oE '"[^"]+"' | head -1 | tr -d '"')
  anchor_devnet=$(grep -A2 '^\[programs.devnet\]' "$root/Anchor.toml" \
    | grep -oE '"[^"]+"' | head -1 | tr -d '"')
  frontend=$(grep -A2 'PROGRAM_ID: Address = address(' "$root/app/src/lib/program/constants.ts" \
    | grep -oE "'[^']+'" | head -1 | tr -d "'")

  echo "  declare_id!           $declared"
  echo "  Anchor.toml localnet  $anchor_localnet"
  echo "  Anchor.toml devnet    $anchor_devnet"
  echo "  frontend PROGRAM_ID   $frontend"
  echo "  expected              $EXPECTED_PROGRAM_ID"

  for v in "$declared" "$anchor_localnet" "$anchor_devnet" "$frontend"; do
    [[ -n "$v" ]] || die "could not extract one of the program IDs"
    [[ "$v" == "$EXPECTED_PROGRAM_ID" ]] \
      || die "program ID mismatch: '$v' != '$EXPECTED_PROGRAM_ID'. Not rewriting it — resolve by hand."
  done
  green "  program IDs agree"
}
