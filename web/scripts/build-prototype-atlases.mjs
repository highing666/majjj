import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Throwaway prototype atlas builder for #78 (3 variants: a, b, c; 184x248 @2x frames, 1920x2048).

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const webRoot = path.resolve(__dirname, "..");
const atlasOutDir = path.join(webRoot, "src/assets/atlas");

const GRID = {
  cols: 10,
  rows: 8,
  cellW: 192,
  cellH: 256,
  frameW: 184,
  frameH: 248,
  pad: 4,
  tileCount: 34,
  bodyFrames: [
    "body_standing",
    "body_laid",
    "body_back",
    "body_shadow",
    "body_laid_back",
  ],
};

function writeSpritesheetJson(variant) {
  const { cols, rows, cellW, cellH, frameW, frameH, pad, tileCount, bodyFrames } = GRID;
  const atlasW = cols * cellW; // 1920 <= 2048
  const atlasH = rows * cellH; // 2048 <= 2048

  const frames = {};
  const addSlot = (key, slot) => {
    const col = slot % cols;
    const row = Math.floor(slot / cols);
    frames[key] = {
      frame: { x: col * cellW + pad, y: row * cellH + pad, w: frameW, h: frameH },
      rotated: false,
      trimmed: false,
      spriteSourceSize: { x: 0, y: 0, w: frameW, h: frameH },
      sourceSize: { w: frameW, h: frameH },
    };
  };

  for (let t = 0; t < tileCount; t += 1) {
    addSlot(`tile_${t}`, t);
    addSlot(`tile_laid_${t}`, tileCount + t);
  }
  bodyFrames.forEach((name, idx) => addSlot(name, tileCount * 2 + idx));

  const sheetJson = {
    frames,
    meta: {
      app: "majjj-prototype-atlas-builder",
      version: "78-proto-1",
      image: `prototype-tiles-${variant}@2x.png`,
      format: "RGBA8888",
      size: { w: atlasW, h: atlasH },
      scale: "2",
    },
  };

  fs.mkdirSync(atlasOutDir, { recursive: true });
  fs.writeFileSync(
    path.join(atlasOutDir, `prototype-tiles-${variant}@2x.json`),
    `${JSON.stringify(sheetJson, null, 2)}\n`,
    "utf8",
  );
}

function rasterizeAll() {
  const swiftScript = path.join(__dirname, "rasterize-prototype-tiles.swift");
  const swiftBin = fs.existsSync("/Library/Developer/CommandLineTools/usr/bin/swift")
    ? "/Library/Developer/CommandLineTools/usr/bin/swift"
    : "swift";
  const env = { ...process.env };
  if (
    process.platform === "darwin" &&
    !env.DEVELOPER_DIR &&
    fs.existsSync("/Library/Developer/CommandLineTools")
  ) {
    env.DEVELOPER_DIR = "/Library/Developer/CommandLineTools";
  }

  const cacheDir = path.join(webRoot, ".swift-module-cache");
  fs.mkdirSync(cacheDir, { recursive: true });
  env.CLANG_MODULE_CACHE_PATH = cacheDir;
  env.SWIFT_MODULE_CACHE_PATH = cacheDir;

  try {
    execFileSync(
      swiftBin,
      ["-module-cache-path", cacheDir, swiftScript, atlasOutDir],
      { env, stdio: "inherit" },
    );
  } finally {
    fs.rmSync(cacheDir, { recursive: true, force: true });
  }
}

for (const v of ["a", "b", "c"]) {
  writeSpritesheetJson(v);
}
rasterizeAll();
console.log("Built 3 prototype @2x tile atlases (variants a, b, c).");
