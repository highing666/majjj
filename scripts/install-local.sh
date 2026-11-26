#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"

usage() {
  cat >&2 <<EOF
Usage: $0 [--bin <path>] [--app-dir <path>] [--app-only] [--build]

Installs mahjong-server to ~/Library/Application Support/majjj/bin/majjj
and generates/updates ~/Applications/Majjj.app without sudo.
EOF
  exit 1
}

bin_path=""
app_dir=""
app_only=0
force_build=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --bin)
      shift
      [ "$#" -gt 0 ] || usage
      bin_path="$1"
      ;;
    --app-dir)
      shift
      [ "$#" -gt 0 ] || usage
      app_dir="$1"
      ;;
    --app-only)
      app_only=1
      ;;
    --build)
      force_build=1
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

if [ -z "$app_dir" ]; then
  app_dir="$HOME/Applications/Majjj.app"
fi

icon_src="${MAJJJ_ICON_PATH:-$repo_root/assets/Majjj.icns}"
if [ ! -f "$icon_src" ]; then
  echo "ERROR: missing app icon at $icon_src" >&2
  exit 1
fi

if [ "$app_only" -eq 0 ]; then
  if [ -z "$bin_path" ]; then
    bin_path="${MAJJJ_BIN:-$repo_root/target/release/mahjong-server}"
    if [ "$force_build" -eq 1 ] || [ ! -f "$bin_path" ]; then
      (cd "$repo_root/web" && npm run build)
      cargo build --release --manifest-path "$repo_root/Cargo.toml" -p mahjong-server
    fi
  fi

  if [ ! -f "$bin_path" ]; then
    echo "ERROR: server binary not found: $bin_path" >&2
    exit 1
  fi

  dest_bin_dir="$HOME/Library/Application Support/majjj/bin"
  dest_bin="$dest_bin_dir/majjj"
  mkdir -p "$dest_bin_dir"
  tmp_bin="$dest_bin_dir/.majjj.install.$$"
  trap 'rm -f "$tmp_bin"' EXIT
  cp "$bin_path" "$tmp_bin"
  chmod 755 "$tmp_bin"
  if command -v codesign >/dev/null 2>&1; then
    codesign --force --sign - "$tmp_bin"
  fi
  mv -f "$tmp_bin" "$dest_bin"
  trap - EXIT
fi

mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources"
cp "$icon_src" "$app_dir/Contents/Resources/Majjj.icns"

cat > "$app_dir/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>zh_CN</string>
  <key>CFBundleDisplayName</key>
  <string>Majjj</string>
  <key>CFBundleExecutable</key>
  <string>Majjj</string>
  <key>CFBundleIconFile</key>
  <string>Majjj</string>
  <key>CFBundleIdentifier</key>
  <string>com.majjj.launcher</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>Majjj</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSUIElement</key>
  <true/>
</dict>
</plist>
PLIST

cat > "$app_dir/Contents/MacOS/Majjj" <<'LAUNCHER'
#!/bin/sh
set -eu

PORT="${MAJJJ_PORT:-8789}"
HEALTH_URL="${MAJJJ_HEALTH_URL:-http://127.0.0.1:${PORT}/api/health}"
OPEN_URL="${MAJJJ_OPEN_URL:-http://127.0.0.1:${PORT}}"
BIN_PATH="$HOME/Library/Application Support/majjj/bin/majjj"
LOG_DIR="$HOME/Library/Logs/majjj"
LOG_FILE="$LOG_DIR/server.log"
WEB_APP="$HOME/Applications/Majjj桌面.app"
WAIT_TICKS="${MAJJJ_WAIT_TICKS:-50}"
WAIT_INTERVAL="${MAJJJ_WAIT_INTERVAL:-0.2}"

show_alert() {
  msg="$1"
  printf '%s\n' "$msg" >&2
  if command -v osascript >/dev/null 2>&1; then
    escaped_msg="$(printf '%s' "$msg" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' | tr '\n' ' ')"
    osascript -e "display alert \"Majjj 启动提示\" message \"${escaped_msg}\" as critical buttons {\"好\"} default button \"好\"" >/dev/null 2>&1 || true
  fi
}

probe_health() {
  if resp="$(curl -sS --max-time 1 -w '\n%{http_code}' "$HEALTH_URL" 2>/dev/null)"; then
    code="$(printf '%s\n' "$resp" | tail -n 1)"
    body="$(printf '%s\n' "$resp" | sed '$d')"
    if [ "$code" = "200" ] && printf '%s' "$body" | grep -Eq '"app"[[:space:]]*:[[:space:]]*"majjj"'; then
      return 0
    fi
    return 1
  fi
  return 2
}

open_client() {
  if [ -d "$WEB_APP" ]; then
    open -a "$WEB_APP"
  else
    open "$OPEN_URL"
  fi
}

probe_rc=0
probe_health || probe_rc=$?
if [ "$probe_rc" -eq 0 ]; then
  open_client
  exit 0
elif [ "$probe_rc" -eq 1 ]; then
  show_alert "端口 ${PORT} 已被其他程序占用，请释放端口或检查日志：${LOG_FILE}"
  exit 1
fi

if command -v lsof >/dev/null 2>&1 && lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
  show_alert "端口 ${PORT} 已被其他程序占用，请释放端口或检查日志：${LOG_FILE}"
  exit 1
fi

if [ ! -x "$BIN_PATH" ]; then
  show_alert "未找到 Majjj 服务端程序：${BIN_PATH}，请重新运行安装脚本。"
  exit 1
fi

mkdir -p "$LOG_DIR"
if [ -f "$LOG_FILE" ]; then
  mv -f "$LOG_FILE" "${LOG_FILE}.1"
fi

"$BIN_PATH" --no-open >"$LOG_FILE" 2>&1 &
server_pid=$!

i=0
ready=0
while [ "$i" -lt "$WAIT_TICKS" ]; do
  if probe_health; then
    ready=1
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    if probe_health; then
      ready=1
    fi
    break
  fi
  sleep "$WAIT_INTERVAL"
  i=$((i + 1))
done

if [ "$ready" -ne 1 ]; then
  err_detail=""
  if [ -s "$LOG_FILE" ]; then
    err_detail="$(grep -E '^error:' "$LOG_FILE" | tail -n 1 | sed 's/^error:[[:space:]]*//' || true)"
    if [ -z "$err_detail" ]; then
      err_detail="$(tail -n 1 "$LOG_FILE" || true)"
    fi
  fi
  if [ -n "$err_detail" ]; then
    show_alert "Majjj 服务启动失败：${err_detail}（日志：${LOG_FILE}）"
  else
    show_alert "Majjj 服务启动超时或失败，请查看日志：${LOG_FILE}"
  fi
  exit 1
fi

open_client
exit 0
LAUNCHER

chmod 755 "$app_dir/Contents/MacOS/Majjj"
echo "Installed Majjj.app -> $app_dir"
if [ "$app_only" -eq 0 ]; then
  echo "Installed binary -> $HOME/Library/Application Support/majjj/bin/majjj"
fi
