#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

if [ "$(uname -s)" = "Darwin" ] && [ -z "${DEVELOPER_DIR:-}" ] && [ -d "/Library/Developer/CommandLineTools" ]; then
  export DEVELOPER_DIR="/Library/Developer/CommandLineTools"
fi

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
if [ ! -d "web/node_modules" ]; then
  echo "ERROR: web/node_modules not found. Run 'make web-install' (or 'npm ci --prefix web') first." >&2
  exit 1
fi
(cd web && npm run typecheck && npm test && npm run build)
cargo test --workspace
./scripts/check-core-boundaries.sh
./scripts/check-glossary.sh
