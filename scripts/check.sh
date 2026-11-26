#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: $0 [--sandbox] [--staged | --since <ref>]" >&2
  exit 1
}

sandbox_mode=0
staged_mode=0
since_ref=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --sandbox)
      sandbox_mode=1
      ;;
    --staged)
      staged_mode=1
      ;;
    --since)
      if [ "$#" -lt 2 ] || [ -z "$2" ]; then
        echo "ERROR: --since requires a git ref" >&2
        usage
      fi
      since_ref="$2"
      shift
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

if [ "$staged_mode" -eq 1 ] && [ -n "$since_ref" ]; then
  echo "ERROR: --staged and --since are mutually exclusive" >&2
  usage
fi

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

if [ "$(uname -s)" = "Darwin" ] && [ -z "${DEVELOPER_DIR:-}" ] && [ -d "/Library/Developer/CommandLineTools" ]; then
  export DEVELOPER_DIR="/Library/Developer/CommandLineTools"
fi

run_web=1
run_rust=1
run_ai_heavy=1
run_dist_scripts=1

changed_files=""
filter_by_changes=0
if [ "$staged_mode" -eq 1 ]; then
  filter_by_changes=1
  changed_files="$(git diff --cached --name-only --diff-filter=ACMRD || true)"
elif [ -n "$since_ref" ]; then
  git rev-parse --verify "${since_ref}^{commit}" >/dev/null 2>&1 || {
    echo "ERROR: invalid --since git ref: $since_ref" >&2
    exit 1
  }
  filter_by_changes=1
  changed_files="$(
    {
      git diff --name-only --diff-filter=ACMRD "${since_ref}...HEAD" || true
      git diff --name-only --diff-filter=ACMRD || true
      git diff --cached --name-only --diff-filter=ACMRD || true
      git ls-files --others --exclude-standard || true
    } | sort -u
  )"
fi

if [ "$filter_by_changes" -eq 1 ]; then
  run_web=0
  run_rust=0
  run_ai_heavy=0
  run_dist_scripts=0

  if printf '%s\n' "$changed_files" | grep -qE '^(web/|scripts/check-web-safeguards\.sh$)'; then
    run_web=1
  fi
  if printf '%s\n' "$changed_files" | grep -qE '^(crates/|Cargo\.(toml|lock)$|scripts/check-core-boundaries\.sh$)'; then
    run_rust=1
  fi
  if printf '%s\n' "$changed_files" | grep -qE '^crates/(mahjong-ai|mahjong-core)/'; then
    run_ai_heavy=1
  fi
  if printf '%s\n' "$changed_files" | grep -qE '^(scripts/|packaging/|Makefile$)'; then
    run_dist_scripts=1
  fi
fi

# 1. Fail-fast static & frontend checks
./scripts/check-glossary.sh

if [ "$run_rust" -eq 1 ]; then
  cargo fmt --all -- --check
fi

if [ "$run_web" -eq 1 ]; then
  if [ ! -d "web/node_modules" ]; then
    echo "ERROR: web/node_modules not found. Run 'make web-install' (or 'npm ci --prefix web') first." >&2
    exit 1
  fi
  (cd web && npm run typecheck && npm test && npm run build)
  ./scripts/check-web-safeguards.sh
fi

if [ "$run_dist_scripts" -eq 1 ]; then
  ./scripts/test-dist-scripts.sh
fi

# 2. Heavier Rust clippy, crate boundary checks (after clippy warms cargo cache), tests & AI smoke gate
if [ "$run_rust" -eq 1 ]; then
  cargo clippy --workspace --all-targets -- -D warnings
  ./scripts/check-core-boundaries.sh

  if [ "$sandbox_mode" -eq 1 ]; then
    cargo test -p mahjong-core
    sandbox_server_tests=(--lib --bin mahjong-server --test lan_discovery --test ts_contract_drift)
    for test_file in crates/mahjong-server/tests/*_virtual_clock.rs crates/mahjong-server/tests/*_offline.rs; do
      if [ -f "$test_file" ]; then
        sandbox_server_tests+=(--test "$(basename "$test_file" .rs)")
      fi
    done
    cargo test -p mahjong-server "${sandbox_server_tests[@]}"
    if [ "$run_ai_heavy" -eq 1 ]; then
      cargo test -p mahjong-ai
    fi
  else
    cargo test --workspace
  fi

  if [ "$run_ai_heavy" -eq 1 ]; then
    cargo run --release -p mahjong-ai --bin bench-ai -- smoke
  fi
fi
