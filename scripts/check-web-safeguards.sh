#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

node -e '
const fs = require("fs");
const path = require("path");

const cwd = process.cwd();
const webRoot = path.join(cwd, "web");
const srcRoot = path.join(webRoot, "src");
const generatedPrefix = path.join(srcRoot, "lib", "protocol", "generated") + path.sep;
const banned = [
  { name: "Safari ResizeObserver pitfall", regex: /\bdevicePixelContentBoxSize\b/ },
  { name: "prohibited in-game hint UI", regex: /听牌|叫胡|荐牌/ },
];

function collectFiles(dir) {
  if (!fs.existsSync(dir)) {
    return [];
  }
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === "node_modules") {
      continue;
    }
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...collectFiles(full));
    } else if (entry.isFile()) {
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

function readPngIhdrDimensions(filePath) {
  const buf = fs.readFileSync(filePath);
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  if (buf.length < 24 || !buf.subarray(0, 8).equals(sig) || buf.subarray(12, 16).toString("ascii") !== "IHDR") {
    throw new Error("invalid PNG header / missing IHDR");
  }
  return {
    width: buf.readUInt32BE(16),
    height: buf.readUInt32BE(20),
  };
}

function findMp4Box(buf, start, end, targetType) {
  let pos = start;
  while (pos + 8 <= end) {
    let size = buf.readUInt32BE(pos);
    const type = buf.toString("ascii", pos + 4, pos + 8);
    let headerSize = 8;
    if (size === 1) {
      if (pos + 16 > end) break;
      size = Number(buf.readBigUInt64BE(pos + 8));
      headerSize = 16;
    } else if (size === 0) {
      size = end - pos;
    }
    if (size < headerSize || pos + size > end) {
      break;
    }
    if (type === targetType) {
      return { offset: pos + headerSize, end: pos + size };
    }
    if (type === "moov") {
      const nested = findMp4Box(buf, pos + headerSize, pos + size, targetType);
      if (nested) return nested;
    }
    pos += size;
  }
  return null;
}

function readM4aDurationSeconds(filePath) {
  const buf = fs.readFileSync(filePath);
  const mvhd = findMp4Box(buf, 0, buf.length, "mvhd");
  if (!mvhd) {
    throw new Error("missing MP4 mvhd box");
  }
  const version = buf.readUInt8(mvhd.offset);
  let timescale;
  let duration;
  if (version === 1) {
    if (mvhd.offset + 32 > mvhd.end) throw new Error("truncated mvhd v1 box");
    timescale = buf.readUInt32BE(mvhd.offset + 20);
    duration = Number(buf.readBigUInt64BE(mvhd.offset + 24));
  } else {
    if (mvhd.offset + 20 > mvhd.end) throw new Error("truncated mvhd v0 box");
    timescale = buf.readUInt32BE(mvhd.offset + 12);
    duration = buf.readUInt32BE(mvhd.offset + 16);
  }
  if (!timescale || timescale <= 0) {
    throw new Error(`invalid mvhd timescale: ${timescale}`);
  }
  return duration / timescale;
}

const failures = [];

// 1. Source code safeguards (Safari ResizeObserver & prohibited in-game hint UI)
for (const file of collectFiles(srcRoot)) {
  const base = path.basename(file);
  if (
    file.startsWith(generatedPrefix) ||
    /\.test\.[cm]?[jt]s$/.test(base) ||
    !/\.(ts|js|svelte|css|html)$/.test(base)
  ) {
    continue;
  }
  const raw = fs.readFileSync(file, "utf8");
  const clean = stripCommentsPreservingLineNumbers(raw);
  const lines = clean.split("\n");
  for (let idx = 0; idx < lines.length; idx += 1) {
    const line = lines[idx];
    for (const rule of banned) {
      if (rule.regex.test(line)) {
        failures.push(`${path.relative(cwd, file)}:${idx + 1}: ${rule.name}: ${line.trim()}`);
      }
    }
  }
}

// 2. Zero @3x files anywhere in web/src, web/assets-src, web/public, or web/dist
const threeXDirs = [
  srcRoot,
  path.join(webRoot, "assets-src"),
  path.join(webRoot, "public"),
  path.join(webRoot, "dist"),
];
for (const dir of threeXDirs) {
  for (const file of collectFiles(dir)) {
    if (/@3x/i.test(path.basename(file))) {
      failures.push(`${path.relative(cwd, file)}: prohibited @3x asset detected`);
    }
  }
}

// 3. PNG IHDR dimensions <= 2048x2048 under web/src/assets/ and web/dist/assets/
const MAX_TEX_DIM = 2048;
const srcAssetsDir = path.join(srcRoot, "assets");
const distDir = path.join(webRoot, "dist");
const distAssetsDir = path.join(distDir, "assets");
const pngDirs = [srcAssetsDir];
if (fs.existsSync(distDir)) {
  pngDirs.push(distAssetsDir);
}

for (const dir of pngDirs) {
  for (const file of collectFiles(dir)) {
    if (!/\.png$/i.test(file)) continue;
    try {
      const { width, height } = readPngIhdrDimensions(file);
      if (width <= 0 || height <= 0 || width > MAX_TEX_DIM || height > MAX_TEX_DIM) {
        failures.push(
          `${path.relative(cwd, file)}: PNG dimensions ${width}x${height} exceed ${MAX_TEX_DIM}x${MAX_TEX_DIM}`,
        );
      }
    } catch (err) {
      failures.push(`${path.relative(cwd, file)}: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
}

// Check tiles@2x.json meta.size matches tiles@2x.png and <= 2048x2048
const atlasPngPath = path.join(srcAssetsDir, "atlas", "tiles@2x.png");
const atlasJsonPath = path.join(srcAssetsDir, "atlas", "tiles@2x.json");
if (!fs.existsSync(atlasPngPath) || !fs.existsSync(atlasJsonPath)) {
  failures.push("missing web/src/assets/atlas/tiles@2x.{png,json}");
} else {
  try {
    const pngDims = readPngIhdrDimensions(atlasPngPath);
    const meta = JSON.parse(fs.readFileSync(atlasJsonPath, "utf8")).meta;
    if (!meta || !meta.size || typeof meta.size.w !== "number" || typeof meta.size.h !== "number") {
      failures.push("web/src/assets/atlas/tiles@2x.json: missing meta.size.{w,h}");
    } else if (
      meta.size.w !== pngDims.width ||
      meta.size.h !== pngDims.height ||
      meta.size.w > MAX_TEX_DIM ||
      meta.size.h > MAX_TEX_DIM
    ) {
      failures.push(
        `web/src/assets/atlas/tiles@2x.json: meta.size (${meta.size.w}x${meta.size.h}) invalid or mismatches PNG (${pngDims.width}x${pngDims.height})`,
      );
    }
  } catch (err) {
    failures.push(`web/src/assets/atlas/tiles@2x.json: ${err instanceof Error ? err.message : String(err)}`);
  }
}

// 4. AudioSprite .m4a budget: <= 307200 bytes (300 KB) AND parsed mvhd duration < 15.0 s
const MAX_M4A_BYTES = 307200;
const MAX_M4A_DURATION_SEC = 15.0;
const m4aFiles = [];
const srcSpriteM4a = path.join(srcAssetsDir, "audio", "sfx-sprite.m4a");
if (!fs.existsSync(srcSpriteM4a)) {
  failures.push("missing web/src/assets/audio/sfx-sprite.m4a");
} else {
  m4aFiles.push(srcSpriteM4a);
}

if (fs.existsSync(distDir)) {
  const distM4as = collectFiles(distAssetsDir).filter((f) => /\.m4a$/i.test(f));
  if (distM4as.length === 0) {
    failures.push("web/dist/assets/ contains no built .m4a AudioSprite");
  } else {
    m4aFiles.push(...distM4as);
  }
}

for (const file of m4aFiles) {
  try {
    const bytes = fs.statSync(file).size;
    if (bytes > MAX_M4A_BYTES) {
      failures.push(`${path.relative(cwd, file)}: size ${bytes} bytes exceeds ${MAX_M4A_BYTES} bytes`);
    }
    const sec = readM4aDurationSeconds(file);
    if (!(sec > 0 && sec < MAX_M4A_DURATION_SEC)) {
      failures.push(`${path.relative(cwd, file)}: mvhd duration ${sec.toFixed(3)}s must be > 0 and < ${MAX_M4A_DURATION_SEC}s`);
    }
  } catch (err) {
    failures.push(`${path.relative(cwd, file)}: ${err instanceof Error ? err.message : String(err)}`);
  }
}

if (failures.length > 0) {
  console.error("ERROR: web safeguard / asset gate check failed:");
  for (const f of failures) console.error("  " + f);
  process.exit(1);
}

console.log("check-web-safeguards: OK");
'