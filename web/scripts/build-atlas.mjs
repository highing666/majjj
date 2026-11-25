import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const webRoot = path.resolve(__dirname, "..");
const assetsSrcDir = path.join(webRoot, "assets-src");
const atlasOutDir = path.join(webRoot, "src/assets/atlas");
const texturesOutDir = path.join(webRoot, "src/assets/textures");

const PALETTE = {
  ink: "#181A20",
  crimson: "#C7352B",
  cobalt: "#1C5296",
  jade: "#157F56",
  gold: "#C48A28",
  ivory: "#F5EEDF",
  ivoryShadow: "#E4D8BE",
  jadeBack: "#1E5C5E",
  jadeRim: "#399294",
};

const CN_NUM = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
const HONOR_GLYPHS = ["東", "南", "西", "北", "中", "發", "白"];

function drawPinCircleSvg(cx, cy, r, fill) {
  const innerR = (r * 0.56).toFixed(1);
  const dotR = (r * 0.22).toFixed(1);
  const ticks = [];
  for (let i = 0; i < 8; i += 1) {
    const rad = (i * Math.PI) / 4;
    const x1 = (cx + Math.cos(rad) * (r * 0.56 + 1.2)).toFixed(1);
    const y1 = (cy + Math.sin(rad) * (r * 0.56 + 1.2)).toFixed(1);
    const x2 = (cx + Math.cos(rad) * (r - 2.2)).toFixed(1);
    const y2 = (cy + Math.sin(rad) * (r - 2.2)).toFixed(1);
    ticks.push(
      `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="${PALETTE.ivory}" stroke-width="${Math.max(1.2, r * 0.12).toFixed(1)}" stroke-linecap="round"/>`,
    );
  }
  return `<g>
    <circle cx="${cx}" cy="${cy}" r="${r}" fill="${fill}" stroke="${PALETTE.ink}" stroke-width="2.1"/>
    <circle cx="${cx}" cy="${cy}" r="${innerR}" fill="none" stroke="${PALETTE.ivory}" stroke-width="${Math.max(1.4, r * 0.14).toFixed(1)}"/>
    ${ticks.join("")}
    <circle cx="${cx}" cy="${cy}" r="${dotR}" fill="${PALETTE.ivory}" stroke="${PALETTE.ink}" stroke-width="1.2"/>
  </g>`;
}

function drawOnePinSvg() {
  const petals = [];
  for (let i = 0; i < 8; i += 1) {
    petals.push(
      `<g transform="rotate(${i * 45} 50 66)"><ellipse cx="50" cy="42" rx="6.6" ry="11" fill="${i % 2 === 0 ? PALETTE.jade : PALETTE.gold}" stroke="${PALETTE.ink}" stroke-width="1.8"/></g>`,
    );
  }
  return `<g>
    <circle cx="50" cy="66" r="35" fill="${PALETTE.cobalt}" stroke="${PALETTE.ink}" stroke-width="2.8"/>
    <circle cx="50" cy="66" r="30.5" fill="${PALETTE.ivory}" stroke="${PALETTE.ink}" stroke-width="1.6"/>
    ${petals.join("")}
    <circle cx="50" cy="66" r="14" fill="${PALETTE.crimson}" stroke="${PALETTE.ink}" stroke-width="2.2"/>
    <circle cx="50" cy="66" r="7.5" fill="${PALETTE.ivory}" stroke="${PALETTE.ink}" stroke-width="1.6"/>
    <circle cx="50" cy="66" r="3.4" fill="${PALETTE.crimson}"/>
  </g>`;
}

function drawBambooSvg(cx, cy, h, color, angle = 0) {
  const w = 11.5;
  const top = cy - h * 0.5;
  const bot = cy + h * 0.5;
  return `<g transform="rotate(${angle} ${cx} ${cy})">
    <rect x="${(cx - w * 0.44).toFixed(1)}" y="${(top + 2).toFixed(1)}" width="${(w * 0.88).toFixed(1)}" height="${(h - 4).toFixed(1)}" rx="3.4" fill="${color}" stroke="${PALETTE.ink}" stroke-width="2"/>
    <rect x="${(cx - w * 0.56).toFixed(1)}" y="${(top - 1).toFixed(1)}" width="${(w * 1.12).toFixed(1)}" height="5.3" rx="2.5" fill="${color}" stroke="${PALETTE.ink}" stroke-width="1.8"/>
    <rect x="${(cx - w * 0.56).toFixed(1)}" y="${(cy - 2.6).toFixed(1)}" width="${(w * 1.12).toFixed(1)}" height="5.3" rx="2.5" fill="${color}" stroke="${PALETTE.ink}" stroke-width="1.8"/>
    <rect x="${(cx - w * 0.56).toFixed(1)}" y="${(bot - 4.5).toFixed(1)}" width="${(w * 1.12).toFixed(1)}" height="5.3" rx="2.5" fill="${color}" stroke="${PALETTE.ink}" stroke-width="1.8"/>
    <line x1="${cx}" y1="${(top + 5.5).toFixed(1)}" x2="${cx}" y2="${(bot - 5.5).toFixed(1)}" stroke="${PALETTE.ivory}" stroke-width="1.7" stroke-linecap="round" opacity="0.85"/>
  </g>`;
}

function drawOneBambooBirdSvg() {
  return `<g>
    <path d="M18 104 Q50 96 82 104" fill="none" stroke="${PALETTE.jade}" stroke-width="6" stroke-linecap="round"/>
    <path d="M18 104 Q50 96 82 104" fill="none" stroke="${PALETTE.ink}" stroke-width="2" stroke-linecap="round"/>
    <path d="M40 82 L22 112 L34 112 L46 86 Z" fill="${PALETTE.jade}" stroke="${PALETTE.ink}" stroke-width="2" stroke-linejoin="round"/>
    <path d="M48 84 L42 116 L54 116 L55 84 Z" fill="${PALETTE.crimson}" stroke="${PALETTE.ink}" stroke-width="2" stroke-linejoin="round"/>
    <path d="M56 82 L64 112 L75 109 L61 80 Z" fill="${PALETTE.cobalt}" stroke="${PALETTE.ink}" stroke-width="2" stroke-linejoin="round"/>
    <ellipse cx="51" cy="65" rx="18" ry="22" fill="${PALETTE.jade}" stroke="${PALETTE.ink}" stroke-width="2.4"/>
    <ellipse cx="53" cy="68" rx="10.5" ry="13" fill="${PALETTE.ivory}" stroke="${PALETTE.ink}" stroke-width="1.6"/>
    <path d="M35 56 C27 65 30 80 44 80 C42 69 42 62 35 56 Z" fill="${PALETTE.cobalt}" stroke="${PALETTE.ink}" stroke-width="2" stroke-linejoin="round"/>
    <path d="M67 56 C75 65 72 80 58 80 C60 69 60 62 67 56 Z" fill="${PALETTE.cobalt}" stroke="${PALETTE.ink}" stroke-width="2" stroke-linejoin="round"/>
    <circle cx="52" cy="38" r="13" fill="${PALETTE.jade}" stroke="${PALETTE.ink}" stroke-width="2.3"/>
    <path d="M46 26 C44 15 53 14 53 25 C57 16 65 19 58 28 Z" fill="${PALETTE.crimson}" stroke="${PALETTE.ink}" stroke-width="1.9" stroke-linejoin="round"/>
    <path d="M62 37 L76 40 L62 44 Z" fill="${PALETTE.gold}" stroke="${PALETTE.ink}" stroke-width="1.8" stroke-linejoin="round"/>
    <circle cx="56" cy="37" r="3" fill="${PALETTE.ivory}" stroke="${PALETTE.ink}" stroke-width="1.4"/>
    <circle cx="56.8" cy="37" r="1.4" fill="${PALETTE.ink}"/>
    <line x1="45" y1="86" x2="43" y2="100" stroke="${PALETTE.ink}" stroke-width="2.4" stroke-linecap="round"/>
    <line x1="57" y1="86" x2="59" y2="100" stroke="${PALETTE.ink}" stroke-width="2.4" stroke-linecap="round"/>
  </g>`;
}

function tileFaceInnerSvg(tileIndex) {
  if (tileIndex < 9) {
    const n = tileIndex + 1;
    return `<g>
      <text x="50" y="53" text-anchor="middle" font-family="'Songti SC','STSong','PingFang SC',serif" font-weight="900" font-size="38" fill="${PALETTE.ink}" stroke="${PALETTE.ink}" stroke-width="0.6">${CN_NUM[n]}</text>
      <text x="50" y="106" text-anchor="middle" font-family="'Songti SC','STSong','PingFang SC',serif" font-weight="900" font-size="44" fill="${PALETTE.crimson}" stroke="${PALETTE.ink}" stroke-width="1.2">萬</text>
    </g>`;
  }
  if (tileIndex < 18) {
    const n = tileIndex - 9 + 1;
    const souLayouts = {
      1: () => drawOneBambooBirdSvg(),
      2: () => drawBambooSvg(50, 40, 38, PALETTE.jade) + drawBambooSvg(50, 90, 38, PALETTE.cobalt),
      3: () => drawBambooSvg(50, 40, 36, PALETTE.jade) + drawBambooSvg(32, 90, 36, PALETTE.cobalt) + drawBambooSvg(68, 90, 36, PALETTE.jade),
      4: () => drawBambooSvg(32, 40, 36, PALETTE.cobalt) + drawBambooSvg(68, 40, 36, PALETTE.jade) + drawBambooSvg(32, 90, 36, PALETTE.jade) + drawBambooSvg(68, 90, 36, PALETTE.cobalt),
      5: () => drawBambooSvg(27, 36, 34, PALETTE.jade) + drawBambooSvg(73, 36, 34, PALETTE.cobalt) + drawBambooSvg(50, 65, 34, PALETTE.crimson) + drawBambooSvg(27, 94, 34, PALETTE.cobalt) + drawBambooSvg(73, 94, 34, PALETTE.jade),
      6: () => [40, 90].flatMap((y) => [26, 50, 74].map((x) => drawBambooSvg(x, y, 36, PALETTE.jade))).join(""),
      7: () => drawBambooSvg(50, 29, 28, PALETTE.crimson) + [65, 101].flatMap((y) => [drawBambooSvg(26, y, 28, PALETTE.jade), drawBambooSvg(50, y, 28, PALETTE.cobalt), drawBambooSvg(74, y, 28, PALETTE.jade)]).join(""),
      8: () => drawBambooSvg(22, 40, 34, PALETTE.jade) + drawBambooSvg(41, 42, 31, PALETTE.cobalt, 26) + drawBambooSvg(59, 42, 31, PALETTE.cobalt, -26) + drawBambooSvg(78, 40, 34, PALETTE.jade) + drawBambooSvg(22, 91, 34, PALETTE.jade) + drawBambooSvg(41, 89, 31, PALETTE.cobalt, -26) + drawBambooSvg(59, 89, 31, PALETTE.cobalt, 26) + drawBambooSvg(78, 91, 34, PALETTE.jade),
      9: () => [29, 65, 101].flatMap((y) => [drawBambooSvg(25, y, 27, PALETTE.jade), drawBambooSvg(50, y, 27, PALETTE.crimson), drawBambooSvg(75, y, 27, PALETTE.cobalt)]).join(""),
    };
    return souLayouts[n]();
  }
  if (tileIndex < 27) {
    const n = tileIndex - 18 + 1;
    if (n === 1) return drawOnePinSvg();
    const pinLayouts = {
      2: [[50, 40, 16, PALETTE.jade], [50, 90, 16, PALETTE.cobalt]],
      3: [[27, 33, 13.5, PALETTE.cobalt], [50, 65, 13.5, PALETTE.crimson], [73, 97, 13.5, PALETTE.jade]],
      4: [[31, 40, 14, PALETTE.cobalt], [69, 40, 14, PALETTE.jade], [31, 90, 14, PALETTE.jade], [69, 90, 14, PALETTE.cobalt]],
      5: [[27, 34, 12.2, PALETTE.cobalt], [73, 34, 12.2, PALETTE.jade], [50, 65, 12.6, PALETTE.crimson], [27, 96, 12.2, PALETTE.jade], [73, 96, 12.2, PALETTE.cobalt]],
      6: [[32, 30, 12.2, PALETTE.jade], [68, 30, 12.2, PALETTE.jade], [32, 65, 12.2, PALETTE.crimson], [68, 65, 12.2, PALETTE.crimson], [32, 99, 12.2, PALETTE.crimson], [68, 99, 12.2, PALETTE.crimson]],
      7: [[25, 25, 10.6, PALETTE.jade], [50, 35, 10.6, PALETTE.jade], [75, 45, 10.6, PALETTE.jade], [31, 73, 11.4, PALETTE.crimson], [69, 73, 11.4, PALETTE.crimson], [31, 102, 11.4, PALETTE.crimson], [69, 102, 11.4, PALETTE.crimson]],
      8: [24, 51, 79, 106].flatMap((y) => [[31, y, 11, PALETTE.cobalt], [69, y, 11, PALETTE.cobalt]]),
      9: [[29, PALETTE.jade], [65, PALETTE.crimson], [101, PALETTE.cobalt]].flatMap(([y, c]) => [[24, y, 10.8, c], [50, y, 10.8, c], [76, y, 10.8, c]]),
    };
    return pinLayouts[n].map(([x, y, r, fill]) => drawPinCircleSvg(x, y, r, fill)).join("");
  }
  if (tileIndex === 33) {
    return `<g>
      <rect x="20" y="23" width="60" height="84" rx="6" fill="none" stroke="${PALETTE.cobalt}" stroke-width="5"/>
      <rect x="27" y="30" width="46" height="70" rx="3" fill="none" stroke="${PALETTE.ink}" stroke-width="1.8" stroke-dasharray="6 4"/>
      <path d="M20 36 L29 23 M71 23 L80 36 M20 94 L29 107 M71 107 L80 94" stroke="${PALETTE.cobalt}" stroke-width="3" stroke-linecap="round"/>
    </g>`;
  }
  const glyph = HONOR_GLYPHS[tileIndex - 27];
  const color = tileIndex === 31 ? PALETTE.crimson : tileIndex === 32 ? PALETTE.jade : PALETTE.ink;
  const stroke = tileIndex === 31 || tileIndex === 32 ? PALETTE.ink : "none";
  return `<text x="50" y="86" text-anchor="middle" font-family="'Songti SC','STSong','PingFang SC',serif" font-weight="900" font-size="58" fill="${color}" stroke="${stroke}" stroke-width="1.2">${glyph}</text>`;
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
  const bodyH = isLaid ? 131 : 126;
  const lip = isLaid
    ? ""
    : `<rect x="2.5" y="5.5" width="95" height="128" rx="11" fill="${isBack ? "#144244" : PALETTE.jadeBack}"/>`;
  const inner = isBack
    ? `<rect x="12.5" y="13.5" width="75" height="${bodyH - 22}" rx="6" fill="none" stroke="${PALETTE.jadeRim}" stroke-width="2.4"/>
       <path d="M50 ${bodyH * 0.5 + 2.5 - 16} L62 ${bodyH * 0.5 + 2.5} L50 ${bodyH * 0.5 + 2.5 + 16} L38 ${bodyH * 0.5 + 2.5} Z" fill="none" stroke="${PALETTE.jadeRim}" stroke-width="2.2"/>`
    : `<rect x="4" y="3" width="92" height="6" rx="3" fill="#FFFDF5" opacity="0.72"/>
       <rect x="4" y="${2.5 + bodyH - (isLaid ? 6 : 9)}" width="92" height="${isLaid ? 6 : 9}" rx="3" fill="${PALETTE.ivoryShadow}"/>`;
  return `<svg viewBox="0 0 100 136" xmlns="http://www.w3.org/2000/svg">
  ${lip}
  <rect x="2.5" y="2.5" width="95" height="${bodyH}" rx="10.5" fill="${isBack ? PALETTE.jadeBack : PALETTE.ivory}" stroke="${PALETTE.ink}" stroke-width="3"/>
  ${inner}
</svg>\n`;
}

function buildTileSvg(tileIndex) {
  return `<svg viewBox="0 0 100 136" xmlns="http://www.w3.org/2000/svg">
  <rect x="2.5" y="5.5" width="95" height="128" rx="11" fill="${PALETTE.jadeBack}"/>
  <rect x="2.5" y="2.5" width="95" height="126" rx="10.5" fill="${PALETTE.ivory}" stroke="${PALETTE.ink}" stroke-width="3"/>
  <rect x="4" y="3" width="92" height="6" rx="3" fill="#FFFDF5" opacity="0.72"/>
  <rect x="4" y="119.5" width="92" height="9" rx="3" fill="${PALETTE.ivoryShadow}"/>
  ${tileFaceInnerSvg(tileIndex)}
</svg>\n`;
}

function writeVectorSources() {
  const tilesDir = path.join(assetsSrcDir, "tiles");
  const bodiesDir = path.join(assetsSrcDir, "bodies");
  fs.mkdirSync(tilesDir, { recursive: true });
  fs.mkdirSync(bodiesDir, { recursive: true });

  for (let t = 0; t < 34; t += 1) {
    fs.writeFileSync(path.join(tilesDir, `tile_${t}.svg`), buildTileSvg(t), "utf8");
  }
  for (const kind of ["standing", "laid", "back", "shadow", "laid_back"]) {
    fs.writeFileSync(path.join(bodiesDir, `body_${kind}.svg`), buildBodySvg(kind), "utf8");
  }
}

function writeSpritesheetJson() {
  const atlasW = 1152;
  const atlasH = 960;
  const cellW = 144;
  const cellH = 192;
  const frameW = 136;
  const frameH = 184;
  const pad = 4;
  const cols = 8;

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
  }
  ["body_standing", "body_laid", "body_back", "body_shadow", "body_laid_back"].forEach(
    (name, idx) => addSlot(name, 34 + idx),
  );

  const sheetJson = {
    frames,
    meta: {
      app: "majjj-atlas-builder",
      version: "1.0",
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
console.log("Built @2x vector-export tile atlas and Piltover impasto textures.");
