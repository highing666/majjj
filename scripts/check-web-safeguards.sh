#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

node -e '
const fs = require("fs");
const path = require("path");

const root = path.join(process.cwd(), "web", "src");
const generatedPrefix = path.join(root, "lib", "protocol", "generated") + path.sep;
const banned = [
  { name: "Safari ResizeObserver pitfall", regex: /\bdevicePixelContentBoxSize\b/ },
  { name: "prohibited in-game hint UI", regex: /听牌|叫胡|荐牌/ },
];

function collectFiles(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...collectFiles(full));
    } else if (
      entry.isFile() &&
      !full.startsWith(generatedPrefix) &&
      !/\.test\.[cm]?[jt]s$/.test(entry.name) &&
      /\.(ts|js|svelte|css|html)$/.test(entry.name)
    ) {
      out.push(full);
    }
  }
  return out;
}

function stripCommentsPreservingLineNumbers(src) {
  const tokenRegex = /(["\x27`])(?:\\.|(?!\1)[\s\S])*\1|<!--[\s\S]*?-->|\/\*[\s\S]*?\*\/|\/\/[^\n]*/g;
  return src.replace(tokenRegex, (match, quote) => {
    if (quote) return match;
    return match.replace(/[^\n]/g, " ");
  });
}

const failures = [];
for (const file of collectFiles(root)) {
  const raw = fs.readFileSync(file, "utf8");
  const clean = stripCommentsPreservingLineNumbers(raw);
  const lines = clean.split("\n");
  for (let idx = 0; idx < lines.length; idx += 1) {
    const line = lines[idx];
    for (const rule of banned) {
      if (rule.regex.test(line)) {
        failures.push(`${path.relative(process.cwd(), file)}:${idx + 1}: ${rule.name}: ${line.trim()}`);
      }
    }
  }
}

if (failures.length > 0) {
  console.error("ERROR: prohibited pattern detected in web/src/:");
  for (const f of failures) console.error("  " + f);
  process.exit(1);
}

console.log("check-web-safeguards: OK");
'
