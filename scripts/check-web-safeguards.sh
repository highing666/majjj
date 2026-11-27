#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

node -e '
const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");

const cwd = process.cwd();
const webRoot = path.join(cwd, "web");
const srcRoot = path.join(webRoot, "src");
const generatedPrefix = path.join(srcRoot, "lib", "protocol", "generated") + path.sep;
const banned = [
  { name: "Safari ResizeObserver pitfall", regex: /\bdevicePixelContentBoxSize\b/ },
  { name: "prohibited in-game hint UI", regex: /听牌|叫胡|荐牌/ },
  { name: "prohibited player-facing seat wind label (ADR-0008)", regex: /东座|南座|西座|北座/ },
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

// 3. PNG IHDR dimensions <= 2048x2048 under web/src/assets/, web/public/, and web/dist/
const MAX_TEX_DIM = 2048;
const srcAssetsDir = path.join(srcRoot, "assets");
const publicDir = path.join(webRoot, "public");
const distDir = path.join(webRoot, "dist");
const distAssetsDir = path.join(distDir, "assets");
const pngDirs = [srcAssetsDir, publicDir];
if (fs.existsSync(distDir)) {
  pngDirs.push(distDir);
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

// Check every @2x spritesheet JSON under web/src/assets/atlas/ matches its paired PNG and <= 2048x2048
const atlasDir = path.join(srcAssetsDir, "atlas");
const requiredAtlases = ["tiles@2x", "personas@2x"];
for (const atlasBase of requiredAtlases) {
  if (
    !fs.existsSync(path.join(atlasDir, `${atlasBase}.png`)) ||
    !fs.existsSync(path.join(atlasDir, `${atlasBase}.json`))
  ) {
    failures.push(`missing web/src/assets/atlas/${atlasBase}.{png,json}`);
  }
}
for (const atlasJsonPath of collectFiles(atlasDir).filter((f) => /@2x\.json$/i.test(f))) {
  const atlasPngPath = atlasJsonPath.replace(/\.json$/i, ".png");
  const relJson = path.relative(cwd, atlasJsonPath);
  if (!fs.existsSync(atlasPngPath)) {
    failures.push(`${relJson}: missing paired PNG ${path.relative(cwd, atlasPngPath)}`);
    continue;
  }
  try {
    const pngDims = readPngIhdrDimensions(atlasPngPath);
    const meta = JSON.parse(fs.readFileSync(atlasJsonPath, "utf8")).meta;
    if (!meta || !meta.size || typeof meta.size.w !== "number" || typeof meta.size.h !== "number") {
      failures.push(`${relJson}: missing meta.size.{w,h}`);
    } else if (
      meta.size.w !== pngDims.width ||
      meta.size.h !== pngDims.height ||
      meta.size.w > MAX_TEX_DIM ||
      meta.size.h > MAX_TEX_DIM
    ) {
      failures.push(
        `${relJson}: meta.size (${meta.size.w}x${meta.size.h}) invalid or mismatches PNG (${pngDims.width}x${pngDims.height})`,
      );
    }
  } catch (err) {
    failures.push(`${relJson}: ${err instanceof Error ? err.message : String(err)}`);
  }
}

// 4. SFX AudioSprite .m4a budget: <= 307200 bytes (300 KB) AND parsed mvhd duration < 15.0 s
// (Scoped strictly to sfx-sprite*.m4a; VoicePack AudioSprites under assets/vo/ have their own budget.)
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
  const distVoPrefix = path.join(distAssetsDir, "vo") + path.sep;
  const distM4as = collectFiles(distAssetsDir).filter(
    (f) => !f.startsWith(distVoPrefix) && /^sfx-sprite.*\.m4a$/i.test(path.basename(f)),
  );
  if (distM4as.length === 0) {
    failures.push("web/dist/assets/ contains no built sfx-sprite .m4a AudioSprite");
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

// 5. Offline 7-pack VoicePack AudioSprite gate (web/public/assets/vo & web/dist/assets/vo)
// Single-sourced directly from web/src/lib/audio/voicePackSchema.ts to prevent list drift.
function loadVoicePackSchemaConstants() {
  const schemaPath = path.join(srcRoot, "lib", "audio", "voicePackSchema.ts");
  const src = fs.readFileSync(schemaPath, "utf8");
  const parseConstStringArray = (name) => {
    const match = src.match(new RegExp(`export const ${name}\\s*=\\s*\\[([\\s\\S]*?)\\]\\s*as const;`));
    if (!match) {
      throw new Error(`Could not locate ${name} in ${path.relative(cwd, schemaPath)}`);
    }
    return [...match[1].matchAll(/"([a-z0-9_]+)"/g)].map((m) => m[1]);
  };
  const speechMatch = src.match(
    /export const VOICE_SLOT_SPEECH_TEXT:\s*Record<VoiceSlotId,\s*string>\s*=\s*\{([\s\S]*?)\};/,
  );
  if (!speechMatch) {
    throw new Error(`Could not locate VOICE_SLOT_SPEECH_TEXT in ${path.relative(cwd, schemaPath)}`);
  }
  const packIds = [
    ...parseConstStringArray("AI_VOICE_PACK_IDS"),
    ...parseConstStringArray("HUMAN_VOICE_PACK_IDS"),
  ];
  const slots = [...speechMatch[1].matchAll(/([a-z0-9_]+)\s*:\s*"[^"]+"/g)].map((m) => m[1]);
  if (packIds.length !== 7) {
    throw new Error(`Expected 7 canonical VoicePack IDs in schema, parsed ${packIds.length}`);
  }
  if (slots.length !== 46) {
    throw new Error(`Expected 46 canonical VoicePack slots in schema, parsed ${slots.length}`);
  }
  return { packIds, slots };
}

const { packIds: VOICE_PACK_IDS, slots: CANONICAL_VOICE_SLOTS } = loadVoicePackSchemaConstants();
const MAX_VOICE_PACK_TOTAL_BYTES = 2 * 1024 * 1024; // 2.0 MiB single-binary budget ceiling
const hasAfinfo = process.platform === "darwin" && fs.existsSync("/usr/bin/afinfo");

function validateVoicePackRoot(voRoot) {
  const relRoot = path.relative(cwd, voRoot);
  if (!fs.existsSync(voRoot)) {
    failures.push(`${relRoot}: missing VoicePack directory`);
    return;
  }

  let totalBytes = 0;
  for (const packId of VOICE_PACK_IDS) {
    const packDir = path.join(voRoot, packId);
    const manifestPath = path.join(packDir, "manifest.json");
    const spritePath = path.join(packDir, "sprite.m4a");
    if (!fs.existsSync(manifestPath)) {
      failures.push(`${relRoot}/${packId}: missing manifest.json`);
      continue;
    }
    if (!fs.existsSync(spritePath)) {
      failures.push(`${relRoot}/${packId}: missing sprite.m4a`);
      continue;
    }

    try {
      const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
      if (manifest.pack_id !== packId) {
        failures.push(`${relRoot}/${packId}/manifest.json: pack_id "${manifest.pack_id}" != "${packId}"`);
      }
      if (manifest.audio !== "sprite.m4a") {
        failures.push(`${relRoot}/${packId}/manifest.json: audio "${manifest.audio}" != "sprite.m4a"`);
      }
      if (!(typeof manifest.duration === "number" && manifest.duration > 0)) {
        failures.push(`${relRoot}/${packId}/manifest.json: invalid duration ${manifest.duration}`);
      }
      const sprites = manifest.sprites;
      if (!sprites || typeof sprites !== "object" || Array.isArray(sprites)) {
        failures.push(`${relRoot}/${packId}/manifest.json: missing sprites object`);
        continue;
      }

      const keys = Object.keys(sprites);
      const keySet = new Set(keys);
      const missingSlots = CANONICAL_VOICE_SLOTS.filter((s) => !keySet.has(s));
      const extraSlots = keys.filter((k) => !CANONICAL_VOICE_SLOTS.includes(k));
      if (missingSlots.length > 0) {
        failures.push(`${relRoot}/${packId}/manifest.json: missing canonical slots: ${missingSlots.join(", ")}`);
      }
      if (extraSlots.length > 0) {
        failures.push(`${relRoot}/${packId}/manifest.json: unexpected extra slots: ${extraSlots.join(", ")}`);
      }

      const bytes = fs.statSync(spritePath).size;
      if (bytes <= 1024) {
        failures.push(`${relRoot}/${packId}/sprite.m4a: file too small (${bytes} bytes)`);
      }
      totalBytes += bytes;

      const mvhdSec = readM4aDurationSeconds(spritePath);
      if (!(mvhdSec > 0)) {
        failures.push(`${relRoot}/${packId}/sprite.m4a: invalid mvhd duration ${mvhdSec}`);
      }
      let durationBoundSec = mvhdSec;
      if (hasAfinfo) {
        const out = execFileSync("/usr/bin/afinfo", [spritePath], { encoding: "utf8" });
        const m = out.match(/estimated duration:\s*([0-9.]+)\s*sec/);
        if (!m || !(Number(m[1]) > 0)) {
          failures.push(`${relRoot}/${packId}/sprite.m4a: afinfo reported invalid duration`);
        } else {
          durationBoundSec = Math.min(durationBoundSec, Number(m[1]));
        }
      }

      const ordered = CANONICAL_VOICE_SLOTS.filter((s) => Array.isArray(sprites[s])).map((s) => ({
        slot: s,
        start: sprites[s][0],
        dur: sprites[s][1],
      })).sort((a, b) => a.start - b.start);

      for (let i = 0; i < ordered.length; i += 1) {
        const cur = ordered[i];
        if (
          typeof cur.start !== "number" ||
          typeof cur.dur !== "number" ||
          !Number.isFinite(cur.start) ||
          !Number.isFinite(cur.dur) ||
          cur.start < 0 ||
          cur.dur <= 0
        ) {
          failures.push(`${relRoot}/${packId}/manifest.json: invalid slice for ${cur.slot}`);
          continue;
        }
        if (cur.start + cur.dur > manifest.duration + 0.001) {
          failures.push(
            `${relRoot}/${packId}/manifest.json: ${cur.slot} end ${(cur.start + cur.dur).toFixed(4)}s > manifest.duration ${manifest.duration}s`,
          );
        }
        if (cur.start + cur.dur > durationBoundSec + 0.05) {
          failures.push(
            `${relRoot}/${packId}/manifest.json: ${cur.slot} end ${(cur.start + cur.dur).toFixed(4)}s > audio duration ${durationBoundSec.toFixed(4)}s`,
          );
        }
        if (i > 0) {
          const prev = ordered[i - 1];
          if (cur.start < prev.start + prev.dur - 1e-6) {
            failures.push(`${relRoot}/${packId}/manifest.json: slot ${cur.slot} overlaps ${prev.slot}`);
          }
        }
      }
    } catch (err) {
      failures.push(`${relRoot}/${packId}: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  if (totalBytes > MAX_VOICE_PACK_TOTAL_BYTES) {
    failures.push(
      `${relRoot}: total VoicePack .m4a size ${totalBytes} bytes exceeds ${MAX_VOICE_PACK_TOTAL_BYTES} bytes (2.0 MiB)`,
    );
  }
}

validateVoicePackRoot(path.join(publicDir, "assets", "vo"));
if (fs.existsSync(distDir)) {
  validateVoicePackRoot(path.join(distAssetsDir, "vo"));
}

// 6. PWA manifest integrity (web/public/manifest.webmanifest + icons + web/dist parity)
const publicManifestPath = path.join(publicDir, "manifest.webmanifest");
if (!fs.existsSync(publicManifestPath)) {
  failures.push("missing web/public/manifest.webmanifest");
} else {
  try {
    const manifest = JSON.parse(fs.readFileSync(publicManifestPath, "utf8"));
    if (!Array.isArray(manifest.icons) || manifest.icons.length === 0) {
      failures.push("web/public/manifest.webmanifest: icons must be a non-empty array");
    } else {
      for (const icon of manifest.icons) {
        if (!icon || typeof icon.src !== "string" || icon.src.trim().length === 0) {
          failures.push("web/public/manifest.webmanifest: icon entry missing src");
          continue;
        }
        const relIcon = icon.src.replace(/^\/+/, "");
        const pubIcon = path.join(publicDir, relIcon);
        if (!fs.existsSync(pubIcon)) {
          failures.push(`web/public/manifest.webmanifest: missing public icon ${path.relative(cwd, pubIcon)}`);
        }
        if (fs.existsSync(distDir)) {
          const distIcon = path.join(distDir, relIcon);
          if (!fs.existsSync(distIcon)) {
            failures.push(`web/dist: missing built PWA icon ${path.relative(cwd, distIcon)}`);
          }
        }
      }
    }
    if (fs.existsSync(distDir) && !fs.existsSync(path.join(distDir, "manifest.webmanifest"))) {
      failures.push("web/dist: missing built manifest.webmanifest");
    }
  } catch (err) {
    failures.push(`web/public/manifest.webmanifest: ${err instanceof Error ? err.message : String(err)}`);
  }
}

// 6. Zero-Pixi boundary in pure table & persona modules (inverted allowlist: protect new files by default)
const pixiImportRegex = /\bfrom\s+["\x27]pixi\.js["\x27]|import\s+["\x27]pixi\.js["\x27]|require\(\s*["\x27]pixi\.js["\x27]\s*\)/;
const pixiAllowedFiles = new Set([
  "web/src/lib/table/TableCanvas.svelte",
  "web/src/lib/table/feltBackdrop.ts",
  "web/src/lib/table/paperGrainFilter.ts",
  "web/src/lib/table/tableRenderer.ts",
  "web/src/lib/table/testFixtures.ts",
  "web/src/lib/table/tileAtlas.ts",
]);
for (const dir of [path.join(srcRoot, "lib", "table"), path.join(srcRoot, "lib", "personas")]) {
  for (const fullPath of collectFiles(dir)) {
    const base = path.basename(fullPath);
    if (/\.test\.[cm]?[jt]s$/.test(base) || !/\.(ts|js|svelte)$/.test(base)) {
      continue;
    }
    const relFile = path.relative(cwd, fullPath).split(path.sep).join("/");
    if (pixiAllowedFiles.has(relFile)) {
      continue;
    }
    const clean = stripCommentsPreservingLineNumbers(fs.readFileSync(fullPath, "utf8"));
    if (pixiImportRegex.test(clean)) {
      failures.push(`${relFile}: prohibited pixi.js import in pure/non-canvas module`);
    }
  }
}

if (failures.length > 0) {
  console.error("ERROR: web safeguard / asset gate check failed:");
  for (const f of failures) console.error("  " + f);
  process.exit(1);
}

console.log("check-web-safeguards: OK");
'