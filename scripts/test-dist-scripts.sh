#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_eq() {
  local expected="$1"
  local actual="$2"
  local label="$3"
  if [ "$expected" != "$actual" ]; then
    fail "$label: expected '$expected', got '$actual'"
  fi
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if ! printf '%s' "$haystack" | grep -Fq "$needle"; then
    fail "$label: expected output to contain '$needle'"
  fi
}

assert_file_contains() {
  local file="$1"
  local needle="$2"
  local label="$3"
  if [ ! -f "$file" ]; then
    fail "$label: missing file $file"
  fi
  if ! grep -Fq "$needle" "$file"; then
    fail "$label: $file missing '$needle'"
  fi
}

# 1. Static syntax check for all repository shell scripts
for script in "$repo_root"/scripts/*.sh; do
  bash -n "$script" || fail "bash -n failed for $script"
done

# 2. Slice 1: .gitignore, Makefile dist-mac, and comic .icns asset
assert_file_contains "$repo_root/.gitignore" ".deploy.local" ".gitignore must ignore .deploy.local"
assert_file_contains "$repo_root/Makefile" "dist-mac:" "Makefile must define dist-mac target"
assert_file_contains "$repo_root/Makefile" "cargo build --release -p mahjong-server" "Makefile dist-mac must build release mahjong-server"

icns_path="$repo_root/assets/Majjj.icns"
[ -s "$icns_path" ] || fail "missing non-empty $icns_path"
magic="$(head -c 4 "$icns_path" | tr -d '\0')"
assert_eq "icns" "$magic" "Majjj.icns header magic"

# Helper: create isolated test workspace with stubbed OS tools in PATH
make_test_env() {
  local name="$1"
  local root
  root="$(mktemp -d)"
  mkdir -p "$root/home" "$root/bin" "$root/logs"
  printf '%s\n' "$root"
}

write_common_stubs() {
  local bin_dir="$1"
  local log_dir="$2"

  cat > "$bin_dir/codesign" <<EOF
#!/usr/bin/env bash
printf 'codesign %s\n' "\$*" >> "$log_dir/calls.log"
exit 0
EOF
  chmod +x "$bin_dir/codesign"

  cat > "$bin_dir/open" <<EOF
#!/usr/bin/env bash
printf 'open %s\n' "\$*" >> "$log_dir/calls.log"
exit 0
EOF
  chmod +x "$bin_dir/open"

  cat > "$bin_dir/osascript" <<EOF
#!/usr/bin/env bash
printf 'osascript %s\n' "\$*" >> "$log_dir/calls.log"
exit 0
EOF
  chmod +x "$bin_dir/osascript"

  cat > "$bin_dir/lsof" <<EOF
#!/usr/bin/env bash
printf 'lsof %s\n' "\$*" >> "$log_dir/calls.log"
if [ "\${STUB_LSOF_BUSY:-0}" = "1" ]; then
  printf 'other 999 user 3u IPv4 0t0 TCP *:8789 (LISTEN)\n'
  exit 0
fi
exit 1
EOF
  chmod +x "$bin_dir/lsof"

  cat > "$bin_dir/curl" <<EOF
#!/usr/bin/env bash
printf 'curl %s\n' "\$*" >> "$log_dir/calls.log"
mode="\${STUB_CURL_MODE:-down}"
include_code=0
method="GET"
url=""
has_json_ct=0
while [ "\$#" -gt 0 ]; do
  case "\$1" in
    -w)
      shift
      include_code=1
      ;;
    -X)
      shift
      method="\$1"
      ;;
    -H)
      shift
      if [ "\$1" = "Content-Type: application/json" ]; then
        has_json_ct=1
      fi
      ;;
    --max-time|-o|-d|--data)
      shift
      ;;
    http://*|https://*)
      url="\$1"
      ;;
  esac
  shift
done

if [ "\$mode" = "wait_marker" ] && [ -f "$log_dir/server_ready" ]; then
  mode="healthy"
fi

emit_resp() {
  local code="\$1"
  local body="\$2"
  if [ "\$include_code" -eq 1 ]; then
    printf '%s\n%s' "\$body" "\$code"
  else
    printf '%s' "\$body"
  fi
}

case "\$mode" in
  healthy)
    if [ "\$method" = "POST" ] && [[ "\$url" == */api/shutdown ]]; then
      if [ "\$has_json_ct" -ne 1 ]; then
        emit_resp 415 '{"error":"Content-Type must be application/json"}'
        exit 22
      fi
      printf 'shutdown_requested\n' >> "$log_dir/calls.log"
      rm -f "$log_dir/server_ready"
      emit_resp 200 '{"ok":true}'
      exit 0
    fi
    emit_resp 200 '{"app":"majjj","version":"0.1.0","phase":"Lobby","online_humans":0,"port":8789}'
    exit 0
    ;;
  in_match)
    emit_resp 200 '{"app":"majjj","version":"0.1.0","phase":"InMatch","online_humans":0,"port":8789}'
    exit 0
    ;;
  online_humans)
    emit_resp 200 '{"app":"majjj","version":"0.1.0","phase":"Lobby","online_humans":2,"port":8789}'
    exit 0
    ;;
  conflict)
    emit_resp 200 '{"app":"something-else"}'
    exit 0
    ;;
  down|wait_marker|*)
    exit 7
    ;;
esac
EOF
  chmod +x "$bin_dir/curl"
}

# 3. Slice 2: install-local.sh & Majjj.app launcher black-box tests
test_install_and_launcher() {
  local root
  root="$(make_test_env install)"
  trap "rm -rf '$root'" RETURN

  write_common_stubs "$root/bin" "$root/logs"

  # Stub majjj binary that records invocation and signals readiness
  cat > "$root/stub-majjj" <<EOF
#!/usr/bin/env bash
printf 'server_started %s\n' "\$*" >> "$root/logs/calls.log"
if [ "\${STUB_SERVER_FAIL_LLM:-0}" = "1" ]; then
  printf 'error: 无法连接局域网推理服务（http://192.168.5.50:8080）："连接超时"\n' >&2
  exit 1
fi
echo "stub server listening"
touch "$root/logs/server_ready"
if [ "\${STUB_SERVER_HANG:-0}" = "1" ]; then
  rm -f "$root/logs/server_ready"
  sleep 2
fi
exit 0
EOF
  chmod +x "$root/stub-majjj"

  # 3a. install-local.sh layout & codesign
  HOME="$root/home" PATH="$root/bin:$PATH" \
    "$repo_root/scripts/install-local.sh" --bin "$root/stub-majjj"

  local installed_bin="$root/home/Library/Application Support/majjj/bin/majjj"
  local app_dir="$root/home/Applications/Majjj.app"
  local launcher="$app_dir/Contents/MacOS/Majjj"
  local plist="$app_dir/Contents/Info.plist"
  local app_icon="$app_dir/Contents/Resources/Majjj.icns"

  [ -x "$installed_bin" ] || fail "installed binary missing or not executable"
  [ -x "$launcher" ] || fail "Majjj.app launcher missing or not executable"
  [ -f "$plist" ] || fail "Info.plist missing"
  [ -f "$app_icon" ] || fail "Majjj.icns missing in app bundle"
  cmp -s "$icns_path" "$app_icon" || fail "bundled icon differs from assets/Majjj.icns"

  assert_file_contains "$plist" "<key>LSUIElement</key>" "Info.plist LSUIElement"
  assert_file_contains "$plist" "<true/>" "Info.plist LSUIElement true"
  assert_file_contains "$plist" "<string>Majjj</string>" "Info.plist CFBundleExecutable"
  assert_file_contains "$root/logs/calls.log" "codesign --force --sign -" "install-local must codesign"

  # 3b. Cold start -> log rotation, --no-open, browser fallback
  mkdir -p "$root/home/Library/Logs/majjj"
  printf 'previous log entry\n' > "$root/home/Library/Logs/majjj/server.log"
  : > "$root/logs/calls.log"
  rm -f "$root/logs/server_ready"

  HOME="$root/home" PATH="$root/bin:$PATH" \
    STUB_CURL_MODE="wait_marker" \
    MAJJJ_WAIT_TICKS=20 MAJJJ_WAIT_INTERVAL=0.02 \
    "$launcher"

  [ -f "$root/home/Library/Logs/majjj/server.log.1" ] || fail "cold start must rotate server.log to server.log.1"
  assert_file_contains "$root/home/Library/Logs/majjj/server.log.1" "previous log entry" "rotated log content"
  assert_file_contains "$root/logs/calls.log" "server_started --no-open" "launcher must start majjj with --no-open"
  assert_file_contains "$root/logs/calls.log" "open http://127.0.0.1:8789" "launcher fallback browser open"
  if grep -Fq "osascript" "$root/logs/calls.log"; then
    fail "cold start should not invoke osascript"
  fi

  # 3c. Reuse healthy server without double-start + Majjj桌面.app priority
  mkdir -p "$root/home/Applications/Majjj桌面.app"
  : > "$root/logs/calls.log"

  HOME="$root/home" PATH="$root/bin:$PATH" \
    STUB_CURL_MODE="healthy" \
    "$launcher"

  if grep -Fq "server_started" "$root/logs/calls.log"; then
    fail "healthy server should not be started a second time"
  fi
  assert_file_contains "$root/logs/calls.log" "open -a $root/home/Applications/Majjj桌面.app" "launcher must prefer Majjj桌面.app"

  # 3d. Port conflict (non-majjj HTTP response) -> osascript alert + non-zero exit
  : > "$root/logs/calls.log"
  if HOME="$root/home" PATH="$root/bin:$PATH" STUB_CURL_MODE="conflict" "$launcher"; then
    fail "port conflict (HTTP non-majjj) must exit non-zero"
  fi
  assert_file_contains "$root/logs/calls.log" "osascript" "port conflict must trigger osascript alert"
  if grep -Fq "server_started" "$root/logs/calls.log"; then
    fail "port conflict must not spawn majjj"
  fi

  # 3e. Port conflict (non-HTTP listener detected by lsof) -> osascript alert + non-zero exit
  : > "$root/logs/calls.log"
  if HOME="$root/home" PATH="$root/bin:$PATH" STUB_CURL_MODE="down" STUB_LSOF_BUSY="1" "$launcher"; then
    fail "port conflict (lsof) must exit non-zero"
  fi
  assert_file_contains "$root/logs/calls.log" "osascript" "lsof port conflict must trigger osascript alert"
  if grep -Fq "server_started" "$root/logs/calls.log"; then
    fail "lsof port conflict must not spawn majjj"
  fi

  # 3f. Startup timeout -> osascript alert pointing to server.log + non-zero exit
  : > "$root/logs/calls.log"
  rm -f "$root/logs/server_ready"
  if HOME="$root/home" PATH="$root/bin:$PATH" \
    STUB_CURL_MODE="down" STUB_LSOF_BUSY="0" STUB_SERVER_HANG="1" \
    MAJJJ_WAIT_TICKS=3 MAJJJ_WAIT_INTERVAL=0.02 \
    "$launcher"; then
    fail "startup timeout must exit non-zero"
  fi
  assert_file_contains "$root/logs/calls.log" "osascript" "startup timeout must trigger osascript alert"
  assert_file_contains "$root/logs/calls.log" "server.log" "startup timeout alert must mention server.log"

  # 3g. Early exit on mandatory LLM startup gate -> osascript alert includes Chinese reason & escapes quotes
  : > "$root/logs/calls.log"
  rm -f "$root/logs/server_ready"
  if HOME="$root/home" PATH="$root/bin:$PATH" \
    STUB_CURL_MODE="down" STUB_LSOF_BUSY="0" STUB_SERVER_FAIL_LLM="1" \
    MAJJJ_WAIT_TICKS=10 MAJJJ_WAIT_INTERVAL=0.02 \
    "$launcher"; then
    fail "LLM unavailable startup failure must exit non-zero"
  fi
  assert_file_contains "$root/logs/calls.log" "osascript" "LLM startup failure must trigger osascript alert"
  assert_file_contains "$root/logs/calls.log" "无法连接局域网推理服务" "LLM startup alert must surface Chinese server error"
  assert_file_contains "$root/logs/calls.log" '\"连接超时\"' "LLM startup alert must escape double quotes for osascript"
  assert_file_contains "$root/logs/calls.log" "server.log" "LLM startup alert must include log path"
}

test_install_and_launcher

# 4. Slice 3: uninstall-local.sh stops server, removes app & binaries, preserves JSONL/preferences
test_uninstall_preserves_data() {
  local root
  root="$(make_test_env uninstall)"
  trap "rm -rf '$root'" RETURN

  write_common_stubs "$root/bin" "$root/logs"

  cat > "$root/stub-majjj" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
  chmod +x "$root/stub-majjj"

  HOME="$root/home" PATH="$root/bin:$PATH" \
    "$repo_root/scripts/install-local.sh" --bin "$root/stub-majjj"

  local data_dir="$root/home/Library/Application Support/majjj"
  local bin_dir="$data_dir/bin"
  cp "$bin_dir/majjj" "$bin_dir/majjj.prev"
  printf '{"schema_version":1,"hand_id":"h1"}\n' > "$data_dir/hands.jsonl"
  printf '{"schema_version":1,"match_id":"m1"}\n' > "$data_dir/matches.jsonl"
  printf '{"schema_version":1,"recent_nicknames":["Alice"]}\n' > "$data_dir/preferences.json"

  : > "$root/logs/calls.log"
  HOME="$root/home" PATH="$root/bin:$PATH" \
    STUB_CURL_MODE="healthy" \
    "$repo_root/scripts/uninstall-local.sh"

  assert_file_contains "$root/logs/calls.log" "shutdown_requested" "uninstall must call POST /api/shutdown when healthy"
  [ ! -e "$root/home/Applications/Majjj.app" ] || fail "Majjj.app should be removed"
  [ ! -e "$bin_dir/majjj" ] || fail "majjj binary should be removed"
  [ ! -e "$bin_dir/majjj.prev" ] || fail "majjj.prev binary should be removed"

  assert_file_contains "$data_dir/hands.jsonl" '"hand_id":"h1"' "hands.jsonl must be preserved"
  assert_file_contains "$data_dir/matches.jsonl" '"match_id":"m1"' "matches.jsonl must be preserved"
  assert_file_contains "$data_dir/preferences.json" '"Alice"' "preferences.json must be preserved"

  # Idempotent when already stopped
  HOME="$root/home" PATH="$root/bin:$PATH" \
    STUB_CURL_MODE="down" \
    "$repo_root/scripts/uninstall-local.sh"
  assert_file_contains "$data_dir/hands.jsonl" '"hand_id":"h1"' "hands.jsonl preserved on idempotent uninstall"
}

test_uninstall_preserves_data

# 5. Slice 4: deploy-partner.sh pre-flight guard, SHA-256, codesign, backup, rollback & sync-app
test_deploy_partner() {
  local root
  root="$(make_test_env deploy)"
  trap "rm -rf '$root'" RETURN

  write_common_stubs "$root/bin" "$root/logs"
  mkdir -p "$root/remote_home"

  # Stub ssh: records target & executes remote script with HOME=$root/remote_home
  cat > "$root/bin/ssh" <<EOF
#!/usr/bin/env bash
set -euo pipefail
target="\$1"
shift
printf 'ssh target=%s cmd=%s\n' "\$target" "\$*" >> "$root/logs/calls.log"
HOME="$root/remote_home" PATH="$root/bin:\$PATH" bash -c "\$*"
EOF
  chmod +x "$root/bin/ssh"

  # Stub scp: supports -q / -r and copies to $root/remote_home/<remote_rel_path>
  cat > "$root/bin/scp" <<EOF
#!/usr/bin/env bash
set -euo pipefail
recursive=0
args=()
while [ "\$#" -gt 0 ]; do
  case "\$1" in
    -r|-rq|-qr)
      recursive=1
      ;;
    -q|-p)
      ;;
    -P)
      shift
      ;;
    *)
      args+=("\$1")
      ;;
  esac
  shift
done
src="\${args[0]}"
dest="\${args[1]}"
printf 'scp src=%s dest=%s\n' "\$src" "\$dest" >> "$root/logs/calls.log"
if [[ "\$dest" == *"]:"* ]]; then
  remote_spec="\${dest#*]:}"
else
  remote_spec="\${dest#*:}"
fi
# Strip matching outer single quotes if present
remote_spec="\${remote_spec#\'}"
remote_spec="\${remote_spec%\'}"
if [[ "\$remote_spec" == /* ]]; then
  full_dest="\$remote_spec"
else
  full_dest="$root/remote_home/\$remote_spec"
fi
mkdir -p "\$(dirname "\$full_dest")"
if [ "\$recursive" -eq 1 ]; then
  rm -rf "\$full_dest"
  cp -R "\$src" "\$full_dest"
else
  cp "\$src" "\$full_dest"
  if [ "\${STUB_CORRUPT_SCP:-0}" = "1" ]; then
    printf 'corrupted' >> "\$full_dest"
  fi
fi
EOF
  chmod +x "$root/bin/scp"

  cat > "$root/deploy.local" <<'EOF'
PARTNER_HOST=partner.local
PARTNER_USER=partner
PARTNER_PORT=8789
EOF

  printf '#!/bin/sh\necho v1\n' > "$root/bin-v1"
  printf '#!/bin/sh\necho v2\n' > "$root/bin-v2"
  chmod +x "$root/bin-v1" "$root/bin-v2"

  local remote_bin_dir="$root/remote_home/Library/Application Support/majjj/bin"
  mkdir -p "$remote_bin_dir"
  cp "$root/bin-v1" "$remote_bin_dir/majjj"

  # 5a. Abort on InMatch without --force
  : > "$root/logs/calls.log"
  if PATH="$root/bin:$PATH" \
    MAJJJ_DEPLOY_CONFIG="$root/deploy.local" \
    STUB_CURL_MODE="in_match" \
    "$repo_root/scripts/deploy-partner.sh" --bin "$root/bin-v2"; then
    fail "deploy-partner must abort during InMatch without --force"
  fi
  if grep -Eq '^(ssh|scp) ' "$root/logs/calls.log"; then
    fail "aborted deploy must not invoke ssh or scp"
  fi

  # 5b. Abort on online_humans > 0 without --force
  : > "$root/logs/calls.log"
  if PATH="$root/bin:$PATH" \
    MAJJJ_DEPLOY_CONFIG="$root/deploy.local" \
    STUB_CURL_MODE="online_humans" \
    "$repo_root/scripts/deploy-partner.sh" --bin "$root/bin-v2"; then
    fail "deploy-partner must abort when online_humans > 0 without --force"
  fi
  if grep -Eq '^(ssh|scp) ' "$root/logs/calls.log"; then
    fail "aborted deploy (online_humans) must not invoke ssh or scp"
  fi

  # 5c. Allow --force during InMatch + verify SHA-256, codesign, backup & atomic swap
  : > "$root/logs/calls.log"
  PATH="$root/bin:$PATH" \
    MAJJJ_DEPLOY_CONFIG="$root/deploy.local" \
    STUB_CURL_MODE="in_match" \
    "$repo_root/scripts/deploy-partner.sh" --force --bin "$root/bin-v2"

  assert_file_contains "$root/logs/calls.log" "ssh target=partner@partner.local" "deploy reads .deploy.local user@host"
  assert_file_contains "$root/logs/calls.log" "codesign --force --sign -" "deploy must codesign uploaded binary"
  cmp -s "$root/bin-v2" "$remote_bin_dir/majjj" || fail "remote majjj should match v2"
  cmp -s "$root/bin-v1" "$remote_bin_dir/majjj.prev" || fail "remote majjj.prev should backup v1"

  # 5d. SHA-256 mismatch aborts and preserves existing remote binary
  : > "$root/logs/calls.log"
  printf '#!/bin/sh\necho v3\n' > "$root/bin-v3"
  if PATH="$root/bin:$PATH" \
    MAJJJ_DEPLOY_CONFIG="$root/deploy.local" \
    STUB_CURL_MODE="healthy" \
    STUB_CORRUPT_SCP="1" \
    "$repo_root/scripts/deploy-partner.sh" --bin "$root/bin-v3"; then
    fail "SHA-256 mismatch must abort deploy"
  fi
  cmp -s "$root/bin-v2" "$remote_bin_dir/majjj" || fail "SHA-256 failure must preserve existing v2 binary"

  # 5e. --rollback respects pre-flight guard, then swaps majjj.prev back to majjj
  if PATH="$root/bin:$PATH" \
    MAJJJ_DEPLOY_CONFIG="$root/deploy.local" \
    STUB_CURL_MODE="online_humans" \
    "$repo_root/scripts/deploy-partner.sh" --rollback; then
    fail "rollback must abort when online_humans > 0 without --force"
  fi

  : > "$root/logs/calls.log"
  PATH="$root/bin:$PATH" \
    MAJJJ_DEPLOY_CONFIG="$root/deploy.local" \
    STUB_CURL_MODE="healthy" \
    "$repo_root/scripts/deploy-partner.sh" --rollback --host override.local --user alice

  assert_file_contains "$root/logs/calls.log" "ssh target=alice@override.local" "--host and --user must override .deploy.local"
  assert_file_contains "$root/logs/calls.log" "shutdown_requested" "rollback must gracefully stop healthy remote server"
  cmp -s "$root/bin-v1" "$remote_bin_dir/majjj" || fail "rollback must restore v1 from majjj.prev"

  # 5f. --sync-app without --bin still deploys default binary (MAJJJ_BIN) AND syncs Majjj.app
  : > "$root/logs/calls.log"
  PATH="$root/bin:$PATH" \
    MAJJJ_DEPLOY_CONFIG="$root/deploy.local" \
    MAJJJ_BIN="$root/bin-v2" \
    STUB_CURL_MODE="healthy" \
    "$repo_root/scripts/deploy-partner.sh" --sync-app

  local remote_app="$root/remote_home/Applications/Majjj.app"
  [ -x "$remote_app/Contents/MacOS/Majjj" ] || fail "--sync-app must deploy executable Majjj launcher"
  [ -f "$remote_app/Contents/Info.plist" ] || fail "--sync-app must deploy Info.plist"
  [ -f "$remote_app/Contents/Resources/Majjj.icns" ] || fail "--sync-app must deploy Majjj.icns"
  cmp -s "$root/bin-v2" "$remote_bin_dir/majjj" || fail "--sync-app without --bin must deploy default binary"
  cmp -s "$root/bin-v1" "$remote_bin_dir/majjj.prev" || fail "--sync-app without --bin must backup previous binary"

  # 5g. --rollback combined with --sync-app rolls back binary AND syncs Majjj.app
  rm -rf "$remote_app"
  : > "$root/logs/calls.log"
  PATH="$root/bin:$PATH" \
    MAJJJ_DEPLOY_CONFIG="$root/deploy.local" \
    STUB_CURL_MODE="healthy" \
    "$repo_root/scripts/deploy-partner.sh" --rollback --sync-app
  cmp -s "$root/bin-v1" "$remote_bin_dir/majjj" || fail "--rollback --sync-app must restore v1"
  [ -x "$remote_app/Contents/MacOS/Majjj" ] || fail "--rollback --sync-app must also sync Majjj.app"

  # 5h. --bin combined with --sync-app pushes explicit binary AND syncs Majjj.app
  : > "$root/logs/calls.log"
  PATH="$root/bin:$PATH" \
    MAJJJ_DEPLOY_CONFIG="$root/deploy.local" \
    STUB_CURL_MODE="healthy" \
    "$repo_root/scripts/deploy-partner.sh" --bin "$root/bin-v2" --sync-app
  cmp -s "$root/bin-v2" "$remote_bin_dir/majjj" || fail "--bin --sync-app must update binary"
  [ -x "$remote_app/Contents/MacOS/Majjj" ] || fail "--bin --sync-app must also sync Majjj.app"
  assert_file_contains "$root/logs/calls.log" "http://partner.local:8789/api/health" ".local host health URL must remain unbracketed"

  # 5i. Bare IPv6 host brackets /api/health URL and scp destination while keeping ssh target clean
  : > "$root/logs/calls.log"
  PATH="$root/bin:$PATH" \
    MAJJJ_DEPLOY_CONFIG="$root/deploy.local" \
    STUB_CURL_MODE="healthy" \
    "$repo_root/scripts/deploy-partner.sh" --host 2001:db8::1 --user partner --bin "$root/bin-v1"
  assert_file_contains "$root/logs/calls.log" "http://[2001:db8::1]:8789/api/health" "bare IPv6 host must be bracketed in health URL"
  assert_file_contains "$root/logs/calls.log" "ssh target=partner@2001:db8::1" "bare IPv6 ssh target must remain unbracketed"
  assert_file_contains "$root/logs/calls.log" "dest=partner@[2001:db8::1]:" "bare IPv6 scp destination must be bracketed"
  cmp -s "$root/bin-v1" "$remote_bin_dir/majjj" || fail "bare IPv6 deploy must update binary"

  # 5j. Already-bracketed IPv6 host avoids double brackets and strips brackets for ssh
  : > "$root/logs/calls.log"
  PATH="$root/bin:$PATH" \
    MAJJJ_DEPLOY_CONFIG="$root/deploy.local" \
    STUB_CURL_MODE="healthy" \
    "$repo_root/scripts/deploy-partner.sh" --host "[2001:db8::1]" --user partner --bin "$root/bin-v2"
  assert_file_contains "$root/logs/calls.log" "http://[2001:db8::1]:8789/api/health" "bracketed IPv6 host must not double-bracket health URL"
  assert_file_contains "$root/logs/calls.log" "ssh target=partner@2001:db8::1" "bracketed IPv6 ssh target must strip outer brackets"
  assert_file_contains "$root/logs/calls.log" "dest=partner@[2001:db8::1]:" "bracketed IPv6 scp destination must stay single-bracketed"

  # 5k. IPv4 host preserves exact unbracketed health URL, ssh, and scp behavior
  : > "$root/logs/calls.log"
  PATH="$root/bin:$PATH" \
    MAJJJ_DEPLOY_CONFIG="$root/deploy.local" \
    STUB_CURL_MODE="healthy" \
    "$repo_root/scripts/deploy-partner.sh" --host 192.168.1.20 --user partner --bin "$root/bin-v1"
  assert_file_contains "$root/logs/calls.log" "http://192.168.1.20:8789/api/health" "IPv4 host health URL must remain unbracketed"
  assert_file_contains "$root/logs/calls.log" "ssh target=partner@192.168.1.20" "IPv4 ssh target must remain unbracketed"
  assert_file_contains "$root/logs/calls.log" "dest=partner@192.168.1.20:" "IPv4 scp destination must remain unbracketed"
}

test_deploy_partner

# 6. Windows llama-server deployment scripts offline contract tests
node --test "$repo_root/scripts/windows-llm/test-scripts.mjs"

# 7. Windows LAN emotional TTS service & PowerShell ops scripts offline contract tests
node --test "$repo_root/scripts/windows-tts/test-scripts.mjs"

resolve_python3() {
  local candidate
  for candidate in /opt/homebrew/bin/python3 /opt/homebrew/bin/python3.12 /opt/homebrew/bin/python3.13 /usr/local/bin/python3; do
    if [ -x "$candidate" ] && "$candidate" --version >/dev/null 2>&1; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  if command -v python3 >/dev/null 2>&1; then
    candidate="$(command -v python3)"
    if "$candidate" --version >/dev/null 2>&1; then
      printf '%s\n' "$candidate"
      return 0
    fi
  fi
  return 1
}

python3_bin="$(resolve_python3)" || fail "no working python3 interpreter found for windows-tts unit tests"
"$python3_bin" -B -m unittest discover -s "$repo_root/scripts/windows-tts/server" -p "test_*.py" -v
"$python3_bin" -B -m unittest "$repo_root/scripts/windows-tts/test_voice_pack_pipeline.py" -v

echo "test-dist-scripts: OK"
