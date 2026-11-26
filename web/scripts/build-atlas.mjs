import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const webRoot = path.resolve(__dirname, "..");
const assetsSrcDir = path.join(webRoot, "assets-src");
const illustrationsDir = path.join(assetsSrcDir, "illustrations");
const atlasOutDir = path.join(webRoot, "src/assets/atlas");
const texturesOutDir = path.join(webRoot, "src/assets/textures");

const PALETTE = {
  ink: "#1C1917",
  crimson: "#B92822",
  cobalt: "#1B4E8D",
  jade: "#137449",
  ivory: "#F6EFE0",
  ivoryTop: "#FFFDF8",
  ivoryBottom: "#E2D4B7",
  jadeBack: "#186258",
  jadeBackDark: "#0E433C",
  jadeBackHi: "#21786B",
  jadeBorder: "#091D1B",
  jadeRim: "rgba(167, 243, 208, 0.42)",
  goldDot: "rgba(212, 168, 87, 0.55)",
  whiteBevel: "rgba(128, 114, 88, 0.22)",
};

const FONT_KAITI = "'Kaiti SC','STKaiti','BiauKai','Songti SC',serif";
const CN_NUM = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
const HONOR_GLYPHS = ["東", "南", "西", "北", "中", "發", "白"];

function drawPlumPinSvg(cx, cy, r, fill) {
  const petals = [0, 72, 144, 216, 288]
    .map(
      (deg) =>
        `<g transform="rotate(${deg} ${cx} ${cy})"><ellipse cx="${cx}" cy="${(cy - r * 0.37).toFixed(2)}" rx="${(r * 0.17).toFixed(2)}" ry="${(r * 0.25).toFixed(2)}" fill="${PALETTE.ivory}"/></g>`,
    )
    .join("");
  return `<g>
    <circle cx="${cx}" cy="${cy}" r="${r}" fill="${fill}" stroke="${PALETTE.ink}" stroke-width="1.4"/>
    <circle cx="${cx}" cy="${cy}" r="${(r * 0.74).toFixed(2)}" fill="none" stroke="${PALETTE.ivory}" stroke-width="${Math.max(1.1, r * 0.1).toFixed(2)}" opacity="0.92"/>
    ${petals}
    <circle cx="${cx}" cy="${cy}" r="${(r * 0.19).toFixed(2)}" fill="${fill}"/>
    <circle cx="${cx}" cy="${cy}" r="${(r * 0.09).toFixed(2)}" fill="${PALETTE.ivory}"/>
  </g>`;
}

function drawBambooSvg(cx, cy, h, color, bw = 12.2, angle = 0) {
  const top = cy - h * 0.5;
  const bot = cy + h * 0.5;
  const stroke = "rgba(18, 22, 24, 0.72)";
  return `<g transform="rotate(${angle} ${cx} ${cy})">
    <rect x="${(cx - bw * 0.42).toFixed(2)}" y="${(top + 2).toFixed(2)}" width="${(bw * 0.84).toFixed(2)}" height="${(h - 4).toFixed(2)}" rx="3.2" fill="${color}" stroke="${stroke}" stroke-width="1.1"/>
    <rect x="${(cx - bw * 0.58).toFixed(2)}" y="${(top - 0.5).toFixed(2)}" width="${(bw * 1.16).toFixed(2)}" height="4.6" rx="2.3" fill="${color}" stroke="${stroke}" stroke-width="1.1"/>
    <rect x="${(cx - bw * 0.58).toFixed(2)}" y="${(cy - 2.3).toFixed(2)}" width="${(bw * 1.16).toFixed(2)}" height="4.6" rx="2.3" fill="${color}" stroke="${stroke}" stroke-width="1.1"/>
    <rect x="${(cx - bw * 0.58).toFixed(2)}" y="${(bot - 4.1).toFixed(2)}" width="${(bw * 1.16).toFixed(2)}" height="4.6" rx="2.3" fill="${color}" stroke="${stroke}" stroke-width="1.1"/>
    <line x1="${(cx - bw * 0.1).toFixed(2)}" y1="${(top + 5.5).toFixed(2)}" x2="${(cx - bw * 0.1).toFixed(2)}" y2="${(bot - 5.5).toFixed(2)}" stroke="rgba(255, 252, 240, 0.7)" stroke-width="1.4" stroke-linecap="round"/>
  </g>`;
}

function tileFaceInnerSvg(tileIndex) {
  if (tileIndex < 9) {
    const n = tileIndex + 1;
    return `<g>
      <text x="50" y="54" text-anchor="middle" font-family="${FONT_KAITI}" font-weight="900" font-size="42" fill="${PALETTE.ink}">${CN_NUM[n]}</text>
      <text x="50" y="108" text-anchor="middle" font-family="${FONT_KAITI}" font-weight="900" font-size="45" fill="${PALETTE.crimson}">萬</text>
    </g>`;
  }
  if (tileIndex < 18) {
    const rank = tileIndex - 9 + 1;
    if (rank === 1) {
      return `<image href="../illustrations/art-a-1sou.png" x="5" y="10" width="90" height="110" preserveAspectRatio="xMidYMid meet"/>`;
    }
    const g = PALETTE.jade;
    const r = PALETTE.crimson;
    const bw = rank >= 7 ? 10.2 : 12.2;
    const layouts = {
      2: [
        [50, 38, 42, g, 0],
        [50, 92, 42, g, 0],
      ],
      3: [
        [50, 36, 40, g, 0],
        [30, 92, 40, g, 0],
        [70, 92, 40, g, 0],
      ],
      4: [
        [31, 38, 40, g, 0],
        [69, 38, 40, g, 0],
        [31, 92, 40, g, 0],
        [69, 92, 40, g, 0],
      ],
      5: [
        [25, 35, 36, g, 0],
        [75, 35, 36, g, 0],
        [50, 65, 36, r, 0],
        [25, 95, 36, g, 0],
        [75, 95, 36, g, 0],
      ],
      6: [
        [25, 38, 40, g, 0],
        [50, 38, 40, g, 0],
        [75, 38, 40, g, 0],
        [25, 92, 40, g, 0],
        [50, 92, 40, g, 0],
        [75, 92, 40, g, 0],
      ],
      7: [
        [50, 26, 30, r, 0],
        [25, 64, 30, g, 0],
        [50, 64, 30, g, 0],
        [75, 64, 30, g, 0],
        [25, 102, 30, g, 0],
        [50, 102, 30, g, 0],
        [75, 102, 30, g, 0],
      ],
      8: [
        [20, 38, 36, g, 0],
        [39, 40, 34, g, 28],
        [61, 40, 34, g, -28],
        [80, 38, 36, g, 0],
        [20, 92, 36, g, 0],
        [39, 90, 34, g, -28],
        [61, 90, 34, g, 28],
        [80, 92, 36, g, 0],
      ],
      9: [27, 65, 103].flatMap((y) => [
        [24, y, 29, g, 0],
        [50, y, 29, r, 0],
        [76, y, 29, g, 0],
      ]),
    };
    return layouts[rank]
      .map(([x, y, h, color, angle]) => drawBambooSvg(x, y, h, color, bw, angle))
      .join("");
  }
  if (tileIndex < 27) {
    const rank = tileIndex - 18 + 1;
    if (rank === 1) {
      return `<image href="../illustrations/art-a-1pin.png" x="4" y="11" width="92" height="108" preserveAspectRatio="xMidYMid meet"/>`;
    }
    const g = PALETTE.jade;
    const r = PALETTE.crimson;
    const b = PALETTE.cobalt;
    const layouts = {
      2: [
        [50, 38, 17, g],
        [50, 92, 17, b],
      ],
      3: [
        [26, 31, 14, b],
        [50, 65, 14, r],
        [74, 99, 14, g],
      ],
      4: [
        [30, 39, 15, b],
        [70, 39, 15, g],
        [30, 91, 15, g],
        [70, 91, 15, b],
      ],
      5: [
        [26, 33, 13, b],
        [74, 33, 13, g],
        [50, 65, 13.5, r],
        [26, 97, 13, g],
        [74, 97, 13, b],
      ],
      6: [
        [31, 29, 13, g],
        [69, 29, 13, g],
        [31, 65, 13, r],
        [69, 65, 13, r],
        [31, 101, 13, r],
        [69, 101, 13, r],
      ],
      7: [
        [24, 23, 11.2, g],
        [50, 34, 11.2, g],
        [76, 45, 11.2, g],
        [30, 74, 12.2, r],
        [70, 74, 12.2, r],
        [30, 103, 12.2, r],
        [70, 103, 12.2, r],
      ],
      8: [23, 51, 79, 107].flatMap((y) => [
        [30, y, 11.6, b],
        [70, y, 11.6, b],
      ]),
      9: [
        [27, g],
        [65, r],
        [103, b],
      ].flatMap(([y, c]) => [
        [23, y, 11.4, c],
        [50, y, 11.4, c],
        [77, y, 11.4, c],
      ]),
    };
    return layouts[rank]
      .map(([x, y, rad, fill]) => drawPlumPinSvg(x, y, rad, fill))
      .join("");
  }
  if (tileIndex === 33) {
    return `<rect x="13" y="14" width="74" height="102" rx="7" fill="none" stroke="${PALETTE.whiteBevel}" stroke-width="1.6"/>`;
  }
  const glyph = HONOR_GLYPHS[tileIndex - 27];
  const color =
    tileIndex === 31
      ? PALETTE.crimson
      : tileIndex === 32
        ? PALETTE.jade
        : PALETTE.ink;
  return `<text x="50" y="89" text-anchor="middle" font-family="${FONT_KAITI}" font-weight="900" font-size="63" fill="${color}">${glyph}</text>`;
}

function buildBodySvg(kind) {
  if (kind === "shadow") {
    return `<svg viewBox="0 0 100 136" xmlns="http://www.w3.org/2000/svg">
  <rect x="4" y="6" width="92" height="126" rx="11" fill="#080D12" opacity="0.42"/>
  <rect x="6" y="8" width="88" height="122" rx="10" fill="#080D12" opacity="0.30"/>
</svg>\n`;
  }
  const isBack = kind === "back" || kind === "laid_back";
  const isLaid = kind === "laid" || kind === "laid_back";
  const faceY = isLaid ? 2.5 : 7.5;
  const faceH = isLaid ? 131 : 125;
  const spine = isLaid
    ? ""
    : `<rect x="2.5" y="1.5" width="95" height="133" rx="11" fill="${isBack ? PALETTE.jadeBackDark : PALETTE.jadeBack}" stroke="${PALETTE.jadeBorder}" stroke-width="1.8"/>`;
  const inner = isBack
    ? `<rect x="12.5" y="${(faceY + 10).toFixed(1)}" width="75" height="${(faceH - 20).toFixed(1)}" rx="7" fill="none" stroke="${PALETTE.jadeRim}" stroke-width="2.2"/>
       <path d="M50 ${(faceY + faceH * 0.5 - 24).toFixed(1)} L66 ${(faceY + faceH * 0.5).toFixed(1)} L50 ${(faceY + faceH * 0.5 + 24).toFixed(1)} L34 ${(faceY + faceH * 0.5).toFixed(1)} Z" fill="none" stroke="${PALETTE.jadeRim}" stroke-width="2.2"/>
       <circle cx="50" cy="${(faceY + faceH * 0.5).toFixed(1)}" r="5" fill="${PALETTE.goldDot}"/>`
    : `<rect x="4" y="${(faceY + 0.8).toFixed(1)}" width="92" height="5.5" rx="3" fill="${PALETTE.ivoryTop}" opacity="0.88"/>
       <rect x="4" y="${(faceY + faceH - (isLaid ? 7 : 8)).toFixed(1)}" width="92" height="${isLaid ? 6.5 : 7.5}" rx="3" fill="${PALETTE.ivoryBottom}"/>`;
  return `<svg viewBox="0 0 100 136" xmlns="http://www.w3.org/2000/svg">
  ${spine}
  <rect x="2.5" y="${faceY}" width="95" height="${faceH}" rx="10.5" fill="${isBack ? PALETTE.jadeBack : PALETTE.ivory}" stroke="${isBack ? PALETTE.jadeBorder : PALETTE.ink}" stroke-width="2.6"/>
  ${inner}
</svg>\n`;
}

function buildTileSvg(tileIndex, laid = false) {
  const faceY = laid ? 2.5 : 7.5;
  const faceH = laid ? 131 : 125;
  const artScaleY = laid ? 128 / 130 : 121 / 130;
  const artOffsetY = laid ? 2.5 : 8.5;
  const spine = laid
    ? ""
    : `<rect x="2.5" y="1.5" width="95" height="133" rx="11" fill="${PALETTE.jadeBack}" stroke="${PALETTE.jadeBorder}" stroke-width="1.8"/>`;
  return `<svg viewBox="0 0 100 136" xmlns="http://www.w3.org/2000/svg">
  ${spine}
  <rect x="2.5" y="${faceY}" width="95" height="${faceH}" rx="10.5" fill="${PALETTE.ivory}" stroke="${PALETTE.ink}" stroke-width="2.6"/>
  <rect x="4" y="${(faceY + 0.8).toFixed(1)}" width="92" height="5.5" rx="3" fill="${PALETTE.ivoryTop}" opacity="0.88"/>
  <rect x="4" y="${(faceY + faceH - (laid ? 7 : 8)).toFixed(1)}" width="92" height="${laid ? 6.5 : 7.5}" rx="3" fill="${PALETTE.ivoryBottom}"/>
  <g transform="translate(0 ${artOffsetY.toFixed(2)}) scale(1 ${artScaleY.toFixed(4)})">
    ${tileFaceInnerSvg(tileIndex)}
  </g>
</svg>\n`;
}

function writeVectorSources() {
  const tilesDir = path.join(assetsSrcDir, "tiles");
  const bodiesDir = path.join(assetsSrcDir, "bodies");
  fs.mkdirSync(tilesDir, { recursive: true });
  fs.mkdirSync(bodiesDir, { recursive: true });

  for (let t = 0; t < 34; t += 1) {
    fs.writeFileSync(path.join(tilesDir, `tile_${t}.svg`), buildTileSvg(t, false), "utf8");
    fs.writeFileSync(path.join(tilesDir, `tile_laid_${t}.svg`), buildTileSvg(t, true), "utf8");
  }
  for (const kind of ["standing", "laid", "back", "shadow", "laid_back"]) {
    fs.writeFileSync(path.join(bodiesDir, `body_${kind}.svg`), buildBodySvg(kind), "utf8");
  }
}

function writeSpritesheetJson() {
  const cols = 10;
  const rows = 8;
  const cellW = 144;
  const cellH = 192;
  const atlasW = cols * cellW; // 1440 <= 2048
  const atlasH = rows * cellH; // 1536 <= 2048
  const frameW = 136;
  const frameH = 184;
  const pad = 4;

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

  for (let t = 0; t < 34; t += 1) {
    addSlot(`tile_${t}`, t);
    addSlot(`tile_laid_${t}`, 34 + t);
  }
  ["body_standing", "body_laid", "body_back", "body_shadow", "body_laid_back"].forEach(
    (name, idx) => addSlot(name, 68 + idx),
  );

  const sheetJson = {
    frames,
    meta: {
      app: "majjj-atlas-builder",
      version: "2.0",
      image: "tiles@2x.png",
      format: "RGBA8888",
      size: { w: atlasW, h: atlasH },
      scale: "2",
    },
  };

  fs.mkdirSync(atlasOutDir, { recursive: true });
  fs.writeFileSync(
    path.join(atlasOutDir, "tiles@2x.json"),
    `${JSON.stringify(sheetJson, null, 2)}\n`,
    "utf8",
  );
}

function rasterizePngAssets() {
  fs.mkdirSync(atlasOutDir, { recursive: true });
  fs.mkdirSync(texturesOutDir, { recursive: true });

  const swiftScript = path.join(__dirname, "rasterize-assets.swift");
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

  const sou1Art = path.join(illustrationsDir, "art-a-1sou.png");
  const pin1Art = path.join(illustrationsDir, "art-a-1pin.png");
  if (!fs.existsSync(sou1Art) || !fs.existsSync(pin1Art)) {
    throw new Error(`Missing offline illustration PNGs under ${illustrationsDir}`);
  }

  try {
    execFileSync(
      swiftBin,
      [
        "-module-cache-path",
        cacheDir,
        swiftScript,
        path.join(atlasOutDir, "tiles@2x.png"),
        path.join(texturesOutDir, "felt-impasto@2x.png"),
        path.join(texturesOutDir, "felt-brush@2x.png"),
        sou1Art,
        pin1Art,
      ],
      { env, stdio: "inherit" },
    );
  } finally {
    fs.rmSync(cacheDir, { recursive: true, force: true });
  }
}

writeVectorSources();
writeSpritesheetJson();
rasterizePngAssets();
console.log("Built @2x Style A (玉骨工笔) dual-body tile atlas and Piltover impasto textures.");
