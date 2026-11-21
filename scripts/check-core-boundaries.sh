#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

core_tree="$(cargo tree -p mahjong-core --edges normal --offline)"
if echo "$core_tree" | grep -qE '\b(tokio|async-std|smol|getrandom|axum|hyper|reqwest|rusqlite)\b'; then
  echo "ERROR: prohibited dependency detected in mahjong-core dependency tree:" >&2
  echo "$core_tree" | grep -E '\b(tokio|async-std|smol|getrandom|axum|hyper|reqwest|rusqlite)\b' >&2
  exit 1
fi

if grep -rnE 'std::(time|fs|net)|std::thread::sleep' crates/mahjong-core/src/; then
  echo "ERROR: prohibited clock/fs/net/sleep reference in crates/mahjong-core/src/" >&2
  exit 1
fi

if grep -nE 'mahjong-server|\b(tokio|axum)\b' crates/mahjong-ai/Cargo.toml; then
  echo "ERROR: mahjong-ai must not depend on mahjong-server, tokio, or axum" >&2
  exit 1
fi

echo "check-core-boundaries: OK"
