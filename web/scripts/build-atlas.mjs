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
const specPath = path.join(webRoot, "src/lib/table/styleASpec.json");
const STYLE_A_SPEC = JSON.parse(fs.readFileSync(specPath, "utf8"));

function toRgbaCss(entry) {
  if (typeof entry === "string") {
    return entry;
  }
  const clean = entry.hex.replace("#", "");
  const r = Number.parseInt(clean.slice(0, 2), 16);
  const g = Number.parseInt(clean.slice(2, 4), 16);
  const b = Number.parseInt(clean.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${entry.alpha})`;
}

const PALETTE = {
  ink: STYLE_A_SPEC.palette.ink,
  inkSoft: toRgbaCss(STYLE_A_SPEC.palette.inkSoft),
  crimson: STYLE_A_SPEC.palette.crimson,
  cobalt: STYLE_A_SPEC.palette.cobalt,
  jade: STYLE_A_SPEC.palette.jade,
  ivory: STYLE_A_SPEC.palette.ivory,
  ivoryTop: STYLE_A_SPEC.palette.ivoryTop.hex,
  ivoryBottom: STYLE_A_SPEC.palette.ivoryBottom,
  jadeBack: STYLE_A_SPEC.palette.jadeBack,
  jadeBackDark: STYLE_A_SPEC.palette.jadeBackDark,
  jadeBackHi: STYLE_A_SPEC.palette.jadeBackHi,
  jadeBorder: STYLE_A_SPEC.palette.jadeBorder,
  jadeRim: toRgbaCss(STYLE_A_SPEC.palette.jadeRim),
  goldDot: toRgbaCss(STYLE_A_SPEC.palette.goldDot),
  whiteBevel: toRgbaCss(STYLE_A_SPEC.palette.whiteBevel),
  bambooSpec: toRgbaCss(STYLE_A_SPEC.palette.bambooSpec),
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
  const stroke = PALETTE.inkSoft;
  return `<g transform="rotate(${angle} ${cx} ${cy})">
    <rect x="${(cx - bw * 0.42).toFixed(2)}" y="${(top + 2).toFixed(2)}" width="${(bw * 0.84).toFixed(2)}" height="${(h - 4).toFixed(2)}" rx="3.2" fill="${color}" stroke="${stroke}" stroke-width="1.1"/>
    <rect x="${(cx - bw * 0.58).toFixed(2)}" y="${(top - 0.5).toFixed(2)}" width="${(bw * 1.16).toFixed(2)}" height="4.6" rx="2.3" fill="${color}" stroke="${stroke}" stroke-width="1.1"/>
    <rect x="${(cx - bw * 0.58).toFixed(2)}" y="${(cy - 2.3).toFixed(2)}" width="${(bw * 1.16).toFixed(2)}" height="4.6" rx="2.3" fill="${color}" stroke="${stroke}" stroke-width="1.1"/>
    <rect x="${(cx - bw * 0.58).toFixed(2)}" y="${(bot - 4.1).toFixed(2)}" width="${(bw * 1.16).toFixed(2)}" height="4.6" rx="2.3" fill="${color}" stroke="${stroke}" stroke-width="1.1"/>
    <line x1="${(cx - bw * 0.1).toFixed(2)}" y1="${(top + 5.5).toFixed(2)}" x2="${(cx - bw * 0.1).toFixed(2)}" y2="${(bot - 5.5).toFixed(2)}" stroke="${PALETTE.bambooSpec}" stroke-width="1.4" stroke-linecap="round"/>
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
    const bw = rank >= 7 ? 10.2 : 12.2;
    return STYLE_A_SPEC.bamboo[String(rank)]
      .map(({ x, y, h, color, angle }) =>
        drawBambooSvg(x, y, h, PALETTE[color], bw, angle),
      )
      .join("");
  }
  if (tileIndex < 27) {
    const rank = tileIndex - 18 + 1;
    if (rank === 1) {
      return `<image href="../illustrations/art-a-1pin.png" x="4" y="11" width="92" height="108" preserveAspectRatio="xMidYMid meet"/>`;
    }
    return STYLE_A_SPEC.dot[String(rank)]
      .map(({ x, y, r, color }) => drawPlumPinSvg(x, y, r, PALETTE[color]))
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

  const { tileCount, bodyFrames } = STYLE_A_SPEC.grid;
  for (let t = 0; t < tileCount; t += 1) {
    fs.writeFileSync(path.join(tilesDir, `tile_${t}.svg`), buildTileSvg(t, false), "utf8");
    fs.writeFileSync(path.join(tilesDir, `tile_laid_${t}.svg`), buildTileSvg(t, true), "utf8");
  }
  for (const frameKey of bodyFrames) {
    const kind = frameKey.replace(/^body_/, "");
    fs.writeFileSync(path.join(bodiesDir, `${frameKey}.svg`), buildBodySvg(kind), "utf8");
  }
}

function writeSpritesheetJson() {
  const { cols, rows, cellW, cellH, frameW, frameH, pad, tileCount, bodyFrames } =
    STYLE_A_SPEC.grid;
  const atlasW = cols * cellW; // 1440 <= 2048
  const atlasH = rows * cellH; // 1536 <= 2048

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
        specPath,
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
