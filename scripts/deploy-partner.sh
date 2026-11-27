#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"

usage() {
  cat >&2 <<EOF
Usage: $0 [--host <host>] [--user <user>] [--port <port>] [--bin <path>] [--remote-bin <path>] [--force] [--rollback] [--sync-app]

Deploys or rolls back mahjong-server on partner Mac over LAN SSH without com.apple.quarantine.
Reads defaults from .deploy.local (if present).
EOF
  exit 1
}

host=""
user=""
port=""
local_bin=""
remote_bin=""
force=0
rollback=0
sync_app=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --host)
      shift
      [ "$#" -gt 0 ] || usage
      host="$1"
      ;;
    --user)
      shift
      [ "$#" -gt 0 ] || usage
      user="$1"
      ;;
    --port)
      shift
      [ "$#" -gt 0 ] || usage
      port="$1"
      ;;
    --bin)
      shift
      [ "$#" -gt 0 ] || usage
      local_bin="$1"
      ;;
    --remote-bin)
      shift
      [ "$#" -gt 0 ] || usage
      remote_bin="$1"
      ;;
    --force)
      force=1
      ;;
    --rollback)
      rollback=1
      ;;
    --sync-app)
      sync_app=1
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

config_file="${MAJJJ_DEPLOY_CONFIG:-$repo_root/.deploy.local}"
cfg_host=""
cfg_user=""
cfg_port=""
cfg_remote_bin=""

if [ -f "$config_file" ]; then
  while IFS= read -r raw_line || [ -n "$raw_line" ]; do
    line="${raw_line%%#*}"
    line="$(printf '%s' "$line" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
    [ -n "$line" ] || continue
    case "$line" in
      *=*)
        key="${line%%=*}"
        val="${line#*=}"
        key="$(printf '%s' "$key" | sed -e 's/[[:space:]]*$//')"
        val="$(printf '%s' "$val" | sed -e 's/^[[:space:]]*//' -e 's/^"//' -e 's/"$//' -e "s/^'//" -e "s/'$//")"
        case "$key" in
          PARTNER_HOST|HOST) cfg_host="$val" ;;
          PARTNER_USER|USER) cfg_user="$val" ;;
          PARTNER_PORT|PORT) cfg_port="$val" ;;
          PARTNER_REMOTE_BIN|REMOTE_BIN) cfg_remote_bin="$val" ;;
        esac
        ;;
    esac
  done < "$config_file"
fi

host="${host:-$cfg_host}"
user="${user:-$cfg_user}"
port="${port:-${cfg_port:-${MAJJJ_PORT:-8789}}}"
remote_bin="${remote_bin:-${cfg_remote_bin:-Library/Application Support/majjj/bin/majjj}}"
local_bin="${local_bin:-${MAJJJ_BIN:-$repo_root/target/release/mahjong-server}}"

if [ -z "$host" ]; then
  echo "ERROR: partner host not specified. Pass --host <host> or set PARTNER_HOST in .deploy.local" >&2
  exit 1
fi

if [ -n "$user" ]; then
  ssh_target="${user}@${host}"
else
  ssh_target="$host"
fi

# Strip leading ~/ if provided so scp/ssh handle relative-to-$HOME cleanly
remote_bin_rel="${remote_bin#~/}"
remote_dir_rel="$(dirname "$remote_bin_rel")"
remote_tmp_rel="${remote_dir_rel}/.majjj.deploy.$$"

check_partner_idle() {
  local health_url="http://${host}:${port}/api/health"
  local health_body=""
  if health_body="$(curl -fsS --max-time 3 "$health_url" 2>/dev/null)"; then
    if printf '%s' "$health_body" | grep -Eq '"app"[[:space:]]*:[[:space:]]*"majjj"'; then
      local phase=""
      local online_humans="0"
      phase="$(printf '%s' "$health_body" | sed -nE 's/.*"phase"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p')"
      online_humans="$(printf '%s' "$health_body" | sed -nE 's/.*"online_humans"[[:space:]]*:[[:space:]]*([0-9]+).*/\1/p')"
      online_humans="${online_humans:-0}"
      if [ "$phase" = "InMatch" ] || [ "$online_humans" -gt 0 ]; then
        if [ "$force" -ne 1 ]; then
          echo "ABORT: partner server is active (phase=${phase:-unknown}, online_humans=${online_humans}). Re-run with --force to override." >&2
          exit 1
        fi
        echo "WARNING: overriding active partner session via --force (phase=${phase:-unknown}, online_humans=${online_humans})." >&2
      fi
    fi
  fi
}

stop_remote_server() {
  local stop_cmd
  if [[ "$remote_bin_rel" == /* ]]; then
    stop_cmd="bin='$remote_bin_rel'"
  else
    stop_cmd="bin=\"\$HOME/$remote_bin_rel\""
  fi
  ssh "$ssh_target" "
    $stop_cmd
    if command -v curl >/dev/null 2>&1; then
      curl -fsS --max-time 2 -X POST -H 'Content-Type: application/json' \"http://127.0.0.1:${port}/api/shutdown\" >/dev/null 2>&1 || true
    fi
    if command -v pkill >/dev/null 2>&1; then
      pkill -TERM -f \"^\${bin}( |\$)\" >/dev/null 2>&1 || true
    fi
  "
}

sync_remote_app() {
  local stage_dir
  stage_dir="$(mktemp -d)"
  trap 'rm -rf "$stage_dir"' RETURN
  "$repo_root/scripts/install-local.sh" --app-only --app-dir "$stage_dir/Majjj.app" >/dev/null
  ssh "$ssh_target" "mkdir -p \"\$HOME/Applications\" && rm -rf \"\$HOME/Applications/Majjj.app.tmp\""
  scp -rq "$stage_dir/Majjj.app" "${ssh_target}:'Applications/Majjj.app.tmp'"
  ssh "$ssh_target" "rm -rf \"\$HOME/Applications/Majjj.app\" && mv -f \"\$HOME/Applications/Majjj.app.tmp\" \"\$HOME/Applications/Majjj.app\""
  echo "Synced Majjj.app -> ${ssh_target}:~/Applications/Majjj.app"
}

do_rollback() {
  check_partner_idle
  stop_remote_server
  local path_setup
  if [[ "$remote_bin_rel" == /* ]]; then
    path_setup="bin='$remote_bin_rel'"
  else
    path_setup="bin=\"\$HOME/$remote_bin_rel\""
  fi
  ssh "$ssh_target" "
    set -eu
    $path_setup
    prev=\"\${bin}.prev\"
    if [ ! -f \"\$prev\" ]; then
      echo \"ERROR: rollback backup not found at \$prev\" >&2
      exit 1
    fi
    chmod 755 \"\$prev\"
    if command -v codesign >/dev/null 2>&1; then
      codesign --force --sign - \"\$prev\"
    fi
    mv -f \"\$prev\" \"\$bin\"
  "
  echo "Rolled back ${ssh_target}:${remote_bin_rel} from majjj.prev"
}

do_push() {
  if [ ! -f "$local_bin" ]; then
    echo "ERROR: local server binary not found: $local_bin" >&2
    exit 1
  fi
  check_partner_idle
  local local_sha
  local_sha="$(shasum -a 256 "$local_bin" | awk '{print $1}')"

  stop_remote_server

  local dir_setup bin_setup tmp_setup
  if [[ "$remote_bin_rel" == /* ]]; then
    dir_setup="rdir='$remote_dir_rel'"
    bin_setup="bin='$remote_bin_rel'"
    tmp_setup="tmp='$remote_tmp_rel'"
  else
    dir_setup="rdir=\"\$HOME/$remote_dir_rel\""
    bin_setup="bin=\"\$HOME/$remote_bin_rel\""
    tmp_setup="tmp=\"\$HOME/$remote_tmp_rel\""
  fi

  ssh "$ssh_target" "$dir_setup && mkdir -p \"\$rdir\""
  scp -q "$local_bin" "${ssh_target}:'${remote_tmp_rel}'"

  ssh "$ssh_target" "
    set -eu
    $bin_setup
    $tmp_setup
    prev=\"\${bin}.prev\"
    actual_sha=\$(shasum -a 256 \"\$tmp\" | awk '{print \$1}')
    if [ \"\$actual_sha\" != \"${local_sha}\" ]; then
      echo \"ERROR: SHA-256 mismatch (expected ${local_sha}, got \$actual_sha)\" >&2
      rm -f \"\$tmp\"
      exit 1
    fi
    chmod 755 \"\$tmp\"
    if command -v codesign >/dev/null 2>&1; then
      codesign --force --sign - \"\$tmp\"
    fi
    if [ -f \"\$bin\" ]; then
      cp -p \"\$bin\" \"\$prev\"
    fi
    mv -f \"\$tmp\" \"\$bin\"
  "
  echo "Deployed ${local_bin} (sha256:${local_sha}) -> ${ssh_target}:${remote_bin_rel}"
}

if [ "$rollback" -eq 1 ]; then
  do_rollback
else
  do_push
fi

if [ "$sync_app" -eq 1 ]; then
  sync_remote_app
fi
