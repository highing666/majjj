#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<EOF
Usage: $0 [--port <port>]

Gracefully stops local majjj server if running and removes ~/Applications/Majjj.app
and installed binaries while preserving *.jsonl history and preferences.json.
EOF
  exit 1
}

port="${MAJJJ_PORT:-8789}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --port)
      shift
      [ "$#" -gt 0 ] || usage
      port="$1"
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

data_dir="$HOME/Library/Application Support/majjj"
bin_dir="$data_dir/bin"
bin_path="$bin_dir/majjj"
prev_bin_path="$bin_dir/majjj.prev"
app_dir="$HOME/Applications/Majjj.app"

health_url="${MAJJJ_HEALTH_URL:-http://127.0.0.1:${port}/api/health}"
shutdown_url="${MAJJJ_SHUTDOWN_URL:-http://127.0.0.1:${port}/api/shutdown}"

if command -v curl >/dev/null 2>&1; then
  if health_body="$(curl -fsS --max-time 1 "$health_url" 2>/dev/null)"; then
    if printf '%s' "$health_body" | grep -Eq '"app"[[:space:]]*:[[:space:]]*"majjj"'; then
      curl -fsS --max-time 2 -X POST "$shutdown_url" >/dev/null 2>&1 || true
      i=0
      while [ "$i" -lt 20 ]; do
        if ! curl -fsS --max-time 1 "$health_url" >/dev/null 2>&1; then
          break
        fi
        sleep 0.1
        i=$((i + 1))
      done
    fi
  fi
fi

if command -v pkill >/dev/null 2>&1; then
  pkill -TERM -f "^${bin_path}( |$)" >/dev/null 2>&1 || true
fi

rm -rf "$app_dir"
rm -f "$bin_path" "$prev_bin_path"
rmdir "$bin_dir" 2>/dev/null || true

echo "Removed Majjj.app and installed binaries."
echo "Preserved history & preferences under: $data_dir"
