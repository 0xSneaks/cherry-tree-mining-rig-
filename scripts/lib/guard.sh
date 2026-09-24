# Shared guards for THE CHERRY TREE deployment scripts.
#
# Sourced, not executed. Every script in scripts/ that touches a cluster must
# call `require_devnet` before it does anything else.

set -euo pipefail

EXPECTED_PROGRAM_ID="C9fJ8nRCXNE2iZPNkuyARyReykkmKceqYd9nF2qobBUt"

red()   { printf '\033[31m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
dim()   { printf '\033[2m%s\033[0m\n' "$*"; }
die()   { red "FAIL: $*"; exit 1; }

# Load .env if present. Never echo its contents, and never `source` it:
# sourcing would execute whatever is in the file, and an env file should be
# data. Parsed line by line into the environment instead.
load_env() {
  local root="$1" line key value
  [[ -f "$root/.env" ]] || return 0

  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line#"${line%%[![:space:]]*}"}"          # strip leading space
    [[ -z "$line" || "$line" == \#* ]] && continue
    [[ "$line" != *=* ]] && continue
    key="${line%%=*}"
    value="${line#*=}"
    key="${key%"${key##*[![:space:]]}"}"             # strip trailing space
    [[ "$key" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || continue
    value="${value#"${value%%[![:space:]]*}"}"
    value="${value%"${value##*[![:space:]]}"}"
    value="${value%\"}"; value="${value#\"}"         # strip matched quotes
    value="${value%\'}"; value="${value#\'}"
    # Anything already exported wins, so a one-off override on the command
    # line is not silently replaced by the file.
    [[ -n "${!key:-}" ]] || export "$key=$value"
  done < "$root/.env"

  dim "loaded $root/.env"
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

# Genesis hashes are the chain's own identity. `require_devnet` trusts the
# operator's label; this verifies it against the chain that answered.
DEVNET_GENESIS="EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"
MAINNET_GENESIS="5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"
TESTNET_GENESIS="4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY"

assert_connected_to_devnet() {
  local rpc="$1" hash
  hash=$(solana genesis-hash --url "$rpc" 2>/dev/null) \
    || die "could not read the genesis hash from $rpc"

  case "$hash" in
    "$DEVNET_GENESIS")
      green "  genesis $hash (devnet, verified on chain)" ;;
    "$MAINNET_GENESIS")
      die "MAINNET IS REFUSED. $rpc answered with the mainnet-beta genesis hash." ;;
    "$TESTNET_GENESIS")
      die "$rpc is testnet, not devnet. Refusing." ;;
    *)
      case "$rpc" in
        *localhost*|*127.0.0.1*)
          dim "  genesis $hash (local validator) — allowed" ;;
        *)
          die "$rpc is not devnet: genesis $hash does not match $DEVNET_GENESIS." ;;
      esac ;;
  esac
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
