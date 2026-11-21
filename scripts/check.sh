#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

if [ "$(uname -s)" = "Darwin" ] && [ -z "${DEVELOPER_DIR:-}" ] && [ -d "/Library/Developer/CommandLineTools" ]; then
  export DEVELOPER_DIR="/Library/Developer/CommandLineTools"
fi

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/check-core-boundaries.sh
./scripts/check-glossary.sh
