#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: $0 [--sandbox]" >&2
  exit 1
}

sandbox_mode=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --sandbox)
      sandbox_mode=1
      ;;
    -h|--help)
      usage
      ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      usage
      ;;
  esac
  shift
done

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

if [ "$(uname -s)" = "Darwin" ] && [ -z "${DEVELOPER_DIR:-}" ] && [ -d "/Library/Developer/CommandLineTools" ]; then
  export DEVELOPER_DIR="/Library/Developer/CommandLineTools"
fi

cargo fmt --all -- --check

if [ ! -d "web/node_modules" ]; then
  echo "ERROR: web/node_modules not found. Run 'make web-install' (or 'npm ci --prefix web') first." >&2
  exit 1
fi
(cd web && npm run typecheck && npm test && npm run build)

cargo clippy --workspace --all-targets -- -D warnings

if [ "$sandbox_mode" -eq 1 ]; then
  cargo test -p mahjong-core -p mahjong-ai
  cargo test -p mahjong-server --lib --bin mahjong-server --test lan_discovery --test pacing_virtual_clock --test ts_contract_drift
else
  cargo test --workspace
fi

cargo run --release -p mahjong-ai --bin bench-ai -- smoke

./scripts/check-core-boundaries.sh
./scripts/check-web-safeguards.sh
./scripts/check-glossary.sh
