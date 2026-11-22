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

ai_tree="$(cargo tree -p mahjong-ai --edges normal --offline)"
if echo "$ai_tree" | grep -qE '\b(mahjong-server|tokio|async-std|smol|axum|hyper|reqwest|rusqlite)\b'; then
  echo "ERROR: prohibited dependency detected in mahjong-ai dependency tree:" >&2
  echo "$ai_tree" | grep -E '\b(mahjong-server|tokio|async-std|smol|axum|hyper|reqwest|rusqlite)\b' >&2
  exit 1
fi

if grep -rnE 'async |\.await\b|std::(fs|net)|std::thread::sleep' crates/mahjong-ai/src/; then
  echo "ERROR: prohibited async/fs/net/sleep reference in crates/mahjong-ai/src/" >&2
  exit 1
fi

cargo metadata --format-version 1 --offline | node -e '
const fs = require("fs");
const allowed = new Set([
  "MIT",
  "Apache-2.0",
  "Apache-2.0 WITH LLVM-exception",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Unlicense",
  "Zlib",
  "BSL-1.0",
  "Unicode-3.0",
  "Unicode-DFS-2016",
  "CC0-1.0",
  "0BSD",
]);

function stripParens(expr) {
  let s = expr.trim();
  while (s.startsWith("(") && s.endsWith(")")) {
    let depth = 0;
    let wrapsWhole = true;
    for (let i = 0; i < s.length; i += 1) {
      if (s[i] === "(") depth += 1;
      else if (s[i] === ")") depth -= 1;
      if (depth === 0 && i < s.length - 1) {
        wrapsWhole = false;
        break;
      }
    }
    if (!wrapsWhole) break;
    s = s.slice(1, -1).trim();
  }
  return s;
}

function splitTopLevel(expr, opRegex) {
  const parts = [];
  let depth = 0;
  let last = 0;
  for (let i = 0; i < expr.length; i += 1) {
    if (expr[i] === "(") depth += 1;
    else if (expr[i] === ")") depth -= 1;
    else if (depth === 0) {
      const rest = expr.slice(i);
      const m = rest.match(opRegex);
      if (m && m.index === 0) {
        parts.push(expr.slice(last, i).trim());
        i += m[0].length - 1;
        last = i + 1;
      }
    }
  }
  parts.push(expr.slice(last).trim());
  return parts;
}

function isPermissiveExpr(expr) {
  const s = stripParens(expr);
  if (!s) return false;
  const andParts = splitTopLevel(s, /^\s+AND\s+/);
  if (andParts.length > 1) {
    return andParts.every(isPermissiveExpr);
  }
  const orParts = splitTopLevel(s, /^(?:\s+OR\s+|\/)/);
  if (orParts.length > 1) {
    return orParts.some(isPermissiveExpr);
  }
  return allowed.has(s);
}

const meta = JSON.parse(fs.readFileSync(0, "utf8"));
const bad = [];
for (const pkg of meta.packages) {
  if (!pkg.source) continue;
  const lic = pkg.license || "";
  if (!isPermissiveExpr(lic)) {
    bad.push(`${pkg.name}@${pkg.version}: ${lic || "<missing>"}`);
  }
}
if (bad.length > 0) {
  console.error("ERROR: disallowed or missing third-party crate license(s):");
  for (const item of bad) console.error("  - " + item);
  process.exit(1);
}
'

echo "check-core-boundaries: OK"
