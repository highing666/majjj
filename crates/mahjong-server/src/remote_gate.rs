//! Minimal self-contained Chinese passcode gate page served to unauthenticated `Remote`
//! clients when `远程联机` is enabled.
//!
//! Intentionally loads zero external resources, zero frontend SPA bundles, and exposes zero
//! application version, nickname, or table state information.

use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

const REMOTE_GATE_HTML: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
<title>入桌口令验证</title>
<style>
:root{color-scheme:light;font-family:-apple-system,BlinkMacSystemFont,"PingFang SC","Microsoft YaHei",sans-serif}
*{box-sizing:border-box}
body{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;padding:20px;background:#f5efe4;color:#241b14}
main{width:100%;max-width:380px;background:#fffaf2;border:1px solid #e3d6c3;border-radius:16px;padding:24px;box-shadow:0 12px 30px rgba(36,27,20,.08)}
h1{margin:0 0 8px;font-size:20px;line-height:1.3}
p{margin:0 0 18px;font-size:14px;line-height:1.5;color:#6c5c4c}
form{display:flex;flex-direction:column;gap:12px}
label{font-size:13px;font-weight:600;color:#4a3c2f}
input{width:100%;padding:12px 14px;border:1px solid #d5c5af;border-radius:10px;background:#fff;color:#241b14;font-size:18px;letter-spacing:2px;text-transform:uppercase;text-align:center}
input:focus{outline:2px solid #1f6f5b;border-color:#1f6f5b}
button{padding:12px 16px;border:0;border-radius:10px;background:#1f6f5b;color:#fff;font-size:15px;font-weight:600;cursor:pointer}
button:disabled{opacity:.6;cursor:default}
.status{min-height:22px;margin-top:12px;font-size:13px;line-height:1.5;color:#6c5c4c}
.status.err{color:#b42318}
.status.ok{color:#1f6f5b}
</style>
</head>
<body>
<main>
  <h1>请输入入桌口令</h1>
  <p>当前牌桌已开启远程联机，请填写宿主提供的 8 位入桌口令以加入对局。</p>
  <form id="gate-form" novalidate>
    <label for="code-input">入桌口令</label>
    <input id="code-input" name="code" type="text" maxlength="16" autocomplete="one-time-code" autocapitalize="characters" spellcheck="false" placeholder="例如：ABCD2345" required>
    <button id="submit-btn" type="submit">进入牌桌</button>
  </form>
  <div id="status" class="status" role="status" aria-live="polite"></div>
</main>
<script>
(function () {
  var form = document.getElementById("gate-form");
  var input = document.getElementById("code-input");
  var btn = document.getElementById("submit-btn");
  var statusEl = document.getElementById("status");
  var busy = false;

  function setStatus(text, kind) {
    statusEl.textContent = text || "";
    statusEl.className = "status" + (kind ? " " + kind : "");
  }

  function submitCode(rawCode) {
    var code = (rawCode || "").trim();
    if (!code) {
      setStatus("请输入 8 位入桌口令。", "err");
      input.focus();
      return;
    }
    if (busy) return;
    busy = true;
    btn.disabled = true;
    setStatus("正在验证入桌口令…", "");

    fetch("/api/remote-auth", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code: code })
    })
      .then(function (res) {
        return res.json().catch(function () { return null; }).then(function (data) {
          return { ok: res.ok, status: res.status, data: data };
        });
      })
      .then(function (result) {
        if (result.ok && result.data && result.data.authenticated) {
          setStatus("验证通过，正在进入牌桌…", "ok");
          window.location.replace("/");
          return;
        }
        busy = false;
        btn.disabled = false;
        var msg = (result.data && typeof result.data.message === "string" && result.data.message)
          ? result.data.message
          : (result.status === 429
            ? "尝试次数过多，请稍后再试。"
            : result.status === 403
              ? "远程联机当前未开启。"
              : "入桌口令无效，请核对后重试。");
        setStatus(msg, "err");
        input.focus();
        input.select();
      })
      .catch(function () {
        busy = false;
        btn.disabled = false;
        setStatus("网络请求失败，请检查连接后重试。", "err");
      });
  }

  form.addEventListener("submit", function (ev) {
    ev.preventDefault();
    submitCode(input.value);
  });

  var params = new URLSearchParams(window.location.search);
  var queryCode = params.get("code");
  if (queryCode && queryCode.trim()) {
    var trimmed = queryCode.trim();
    input.value = trimmed;
    submitCode(trimmed);
  } else {
    input.focus();
  }
})();
</script>
</body>
</html>
"#;

/// Builds the HTTP `200 OK` response serving the minimal remote gate page with `Cache-Control: no-store`.
#[must_use]
pub fn remote_gate_response() -> Response {
    let mut response = (StatusCode::OK, REMOTE_GATE_HTML).into_response();
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
