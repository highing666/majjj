#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

# Reject non-RFC global unicast IPv6 literals (2000::/3 outside RFC 3849 2001:db8::/32
# and the exact IANA CIDR notation 2000::/3) in deployment and ADR docs (ADR-0012 privacy redline).
node -e '
const fs = require("fs");
const net = require("net");
const path = require("path");

function expandIpv6(ip) {
  let addr = ip.toLowerCase();
  const pct = addr.indexOf("%");
  if (pct !== -1) addr = addr.slice(0, pct);
  if (addr.includes(".")) {
    const lastColon = addr.lastIndexOf(":");
    const v4 = addr.slice(lastColon + 1).split(".").map(Number);
    if (v4.length !== 4 || v4.some((n) => !Number.isInteger(n) || n < 0 || n > 255)) return null;
    const hi = ((v4[0] << 8) | v4[1]).toString(16);
    const lo = ((v4[2] << 8) | v4[3]).toString(16);
    addr = `${addr.slice(0, lastColon)}:${hi}:${lo}`;
  }
  const halves = addr.split("::");
  if (halves.length > 2) return null;
  const left = halves[0] ? halves[0].split(":") : [];
  const right = halves.length === 2 && halves[1] ? halves[1].split(":") : [];
  const missing = halves.length === 2 ? 8 - (left.length + right.length) : 0;
  if (missing < 0 || (halves.length === 1 && left.length !== 8)) return null;
  const parts = [...left, ...Array(missing).fill("0"), ...right];
  if (parts.length !== 8) return null;
  return parts.map((p) => parseInt(p, 16));
}

function isDisallowedPublicIpv6(ip, cidrSuffix) {
  if (!net.isIPv6(ip)) return false;
  if (ip.toLowerCase() === "2000::" && cidrSuffix === "/3") return false;
  const h = expandIpv6(ip);
  if (!h) return false;
  const isGlobalUnicast = (h[0] & 0xe000) === 0x2000;
  const isRfc3849Doc = h[0] === 0x2001 && h[1] === 0x0db8;
  return isGlobalUnicast && !isRfc3849Doc;
}

function defaultTargets() {
  const out = [];
  if (fs.existsSync("docs")) {
    for (const entry of fs.readdirSync("docs", { withFileTypes: true })) {
      if (entry.isFile() && entry.name.endsWith("-deployment.md")) {
        out.push(path.join("docs", entry.name));
      }
    }
  }
  if (fs.existsSync("docs/adr")) {
    for (const entry of fs.readdirSync("docs/adr", { withFileTypes: true })) {
      if (entry.isFile() && entry.name.endsWith(".md")) {
        out.push(path.join("docs/adr", entry.name));
      }
    }
  }
  return out.sort();
}

const files = process.argv.slice(1).length > 0 ? process.argv.slice(1) : defaultTargets();
const failures = [];
const tokenRe = /(?<![A-Za-z0-9_])([0-9a-fA-F:]{3,})(\/\d{1,3})?(?![A-Za-z0-9_])/g;

for (const file of files) {
  const lines = fs.readFileSync(file, "utf8").split("\n");
  for (let i = 0; i < lines.length; i += 1) {
    for (const m of lines[i].matchAll(tokenRe)) {
      const raw = m[1];
      const cidr = m[2] || "";
      const cand = raw.endsWith("::") ? raw : raw.replace(/^:+|:+$/g, "");
      if (cand.includes(":") && isDisallowedPublicIpv6(cand, cidr)) {
        failures.push(`${file}:${i + 1}: non-RFC public IPv6 literal "${m[0]}" (use RFC 3849 2001:db8::/32 placeholders)`);
      }
    }
  }
}

if (failures.length > 0) {
  console.error("ERROR: disallowed public IPv6 literal(s) found in documentation:");
  for (const f of failures) console.error("  " + f);
  process.exit(1);
}

console.log("check-doc-privacy: OK");
' "$@"
