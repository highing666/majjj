import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const webRoot = path.resolve(__dirname, "..");
const assetsSrcDir = path.join(webRoot, "assets-src");
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

const GEOM = STYLE_A_SPEC.geometry;
const TYPO = STYLE_A_SPEC.typography;
const PALETTE = {
  ink: STYLE_A_SPEC.palette.ink,
  crimson: STYLE_A_SPEC.palette.crimson,
  jade: STYLE_A_SPEC.palette.jade,
  cobalt: STYLE_A_SPEC.palette.cobalt,
  ivoryTop: STYLE_A_SPEC.palette.ivoryTop,
  ivoryMid: STYLE_A_SPEC.palette.ivoryMid,
  ivoryBot: STYLE_A_SPEC.palette.ivoryBot,
  ivoryWallTop: STYLE_A_SPEC.palette.ivoryWallTop,
  ivoryWallBot: STYLE_A_SPEC.palette.ivoryWallBot,
  ivoryRimStroke: toRgbaCss(STYLE_A_SPEC.palette.ivoryRimStroke),
  ivoryBevelLight: toRgbaCss(STYLE_A_SPEC.palette.ivoryBevelLight),
  ivoryBevelShadow: toRgbaCss(STYLE_A_SPEC.palette.ivoryBevelShadow),
  jadeTopHi: STYLE_A_SPEC.palette.jadeTopHi,
  jadeMid: STYLE_A_SPEC.palette.jadeMid,
  jadeDeep: STYLE_A_SPEC.palette.jadeDeep,
  jadeRimStroke: toRgbaCss(STYLE_A_SPEC.palette.jadeRimStroke),
  jadeInlay: toRgbaCss(STYLE_A_SPEC.palette.jadeInlay),
  jadeGoldDot: toRgbaCss(STYLE_A_SPEC.palette.jadeGoldDot),
};

const FONT_CARVED = `'${TYPO.primaryFont}','${TYPO.fallbackFont}','Kaiti SC','STKaiti','Songti SC',serif`;
const CN_NUM = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
const HONOR_GLYPHS = ["東", "南", "西", "北", "中", "發", "白"];

function faceRectFor(laid) {
  if (laid) {
    const faceH = 133.5 - 2.0 - GEOM.laidJadeLip - GEOM.laidIvoryWall;
    return { x: 2.0, y: 2.0, w: 96.0, h: faceH };
  }
  const y = 1.5 + GEOM.standingTopCap;
  return { x: 2.0, y, w: 96.0, h: 133.5 - y };
}

function drawCarvedPinSvg(cx, cy, r, fill) {
  const ringR = r * 0.77;
  const rx = r * 0.145;
  const ry = r * 0.235;
  const petals = [0, 60, 120, 180, 240, 300]
    .map(
      (deg) =>
        `<g transform="rotate(${deg} ${cx} ${cy})"><ellipse cx="${cx}" cy="${(cy - r * 0.42).toFixed(2)}" rx="${rx.toFixed(2)}" ry="${ry.toFixed(2)}" fill="${PALETTE.ivoryTop}"/></g>`,
    )
    .join("");
  return `<g>
    <circle cx="${(cx + 0.65).toFixed(2)}" cy="${(cy + 0.95).toFixed(2)}" r="${r.toFixed(2)}" fill="rgba(255, 255, 255, 0.78)"/>
    <circle cx="${cx}" cy="${cy}" r="${r.toFixed(2)}" fill="${fill}"/>
    <circle cx="${cx}" cy="${cy}" r="${ringR.toFixed(2)}" fill="none" stroke="${PALETTE.ivoryTop}" stroke-width="${Math.max(1.4, r * 0.115).toFixed(2)}"/>
    ${petals}
    <circle cx="${cx}" cy="${cy}" r="${(r * 0.21).toFixed(2)}" fill="${fill}"/>
    <circle cx="${cx}" cy="${cy}" r="${(r * 0.095).toFixed(2)}" fill="${PALETTE.ivoryTop}"/>
  </g>`;
}

function drawCarvedBambooSvg(cx, cy, h, color, bw, angle = 0) {
  const halfH = h * 0.5;
  const stemW = bw * 0.74;
  const waistW = stemW * 0.78;
  const nodeW = bw * 1.18;
  const nodeH = Math.max(4.2, Math.min(6.2, h * 0.145));
  const nodeR = (nodeH * 0.48).toFixed(2);
  const segGap = nodeH * 0.65;
  const dotR = Math.max(0.85, bw * 0.085);

  const shaftD = [
    `M ${(-stemW * 0.5).toFixed(2)} ${(-halfH + 2.2).toFixed(2)}`,
    `Q ${(-waistW * 0.45).toFixed(2)} ${(-halfH * 0.5).toFixed(2)} ${(-stemW * 0.5).toFixed(2)} 0`,
    `Q ${(-waistW * 0.45).toFixed(2)} ${(halfH * 0.5).toFixed(2)} ${(-stemW * 0.5).toFixed(2)} ${(halfH - 2.2).toFixed(2)}`,
    `L ${(stemW * 0.5).toFixed(2)} ${(halfH - 2.2).toFixed(2)}`,
    `Q ${(waistW * 0.45).toFixed(2)} ${(halfH * 0.5).toFixed(2)} ${(stemW * 0.5).toFixed(2)} 0`,
    `Q ${(waistW * 0.45).toFixed(2)} ${(-halfH * 0.5).toFixed(2)} ${(stemW * 0.5).toFixed(2)} ${(-halfH + 2.2).toFixed(2)} Z`,
  ].join(" ");

  const body = (fill) => `<g fill="${fill}">
    <path d="${shaftD}"/>
    <rect x="${(-nodeW * 0.5).toFixed(2)}" y="${(-halfH).toFixed(2)}" width="${nodeW.toFixed(2)}" height="${nodeH.toFixed(2)}" rx="${nodeR}"/>
    <rect x="${(-nodeW * 0.54).toFixed(2)}" y="${(-nodeH * 0.52).toFixed(2)}" width="${(nodeW * 1.08).toFixed(2)}" height="${(nodeH * 1.04).toFixed(2)}" rx="${nodeR}"/>
    <rect x="${(-nodeW * 0.5).toFixed(2)}" y="${(halfH - nodeH).toFixed(2)}" width="${nodeW.toFixed(2)}" height="${nodeH.toFixed(2)}" rx="${nodeR}"/>
  </g>`;

  return `<g transform="translate(${cx} ${cy}) rotate(${angle})">
    <g transform="translate(0.7 0.95)">${body("rgba(255, 255, 255, 0.75)")}</g>
    ${body(color)}
    <path d="M 0 ${(-halfH + nodeH * 0.85).toFixed(2)} L 0 ${(-segGap).toFixed(2)} M 0 ${segGap.toFixed(2)} L 0 ${(halfH - nodeH * 0.85).toFixed(2)}" stroke="${PALETTE.ivoryTop}" stroke-width="${Math.max(1.5, bw * 0.16).toFixed(2)}" stroke-linecap="round"/>
    <path d="M ${(-nodeW * 0.28).toFixed(2)} 0 L ${(nodeW * 0.28).toFixed(2)} 0" stroke="${PALETTE.ivoryTop}" stroke-width="${Math.max(1.2, bw * 0.12).toFixed(2)}" stroke-linecap="round"/>
    <circle cx="0" cy="${(-halfH + nodeH * 0.5).toFixed(2)}" r="${dotR.toFixed(2)}" fill="${PALETTE.ivoryTop}"/>
    <circle cx="0" cy="${(halfH - nodeH * 0.5).toFixed(2)}" r="${dotR.toFixed(2)}" fill="${PALETTE.ivoryTop}"/>
  </g>`;
}

function drawEngravedOneBambooSvg() {
  const leaves = [
    [23, 94, -146, 16, 4.2],
    [34, 92, -106, 14, 3.8],
    [78, 76, -31.5, 15, 4.0],
    [71, 79, 20.1, 13, 3.6],
  ]
    .map(
      ([ox, oy, deg, len, wid]) =>
        `<g transform="translate(${ox} ${oy}) rotate(${deg})"><path d="M 0 0 Q ${(len * 0.45).toFixed(2)} ${(-wid).toFixed(2)} ${len} 0 Q ${(len * 0.45).toFixed(2)} ${wid.toFixed(2)} 0 0 Z" fill="${PALETTE.jade}"/></g>`,
    )
    .join("");

  const tailFeathers = [
    [18.3, 32, PALETTE.crimson, 4.8],
    [29.8, 37, PALETTE.jade, 5.4],
    [41.3, 40, PALETTE.cobalt, 5.4],
    [52.7, 36, PALETTE.jade, 5.0],
    [64.2, 30, PALETTE.crimson, 4.6],
  ]
    .map(
      ([deg, len, col, wid]) => `<g transform="translate(58 65) rotate(${deg})">
      <path d="M 0 0 Q ${(len * 0.55).toFixed(2)} ${(-wid).toFixed(2)} ${len} 0 Q ${(len * 0.55).toFixed(2)} ${wid.toFixed(2)} 0 0 Z" fill="${col}"/>
      <path d="M 4 0 L ${len - 5} 0" stroke="${PALETTE.ivoryTop}" stroke-width="1.25"/>
    </g>`,
    )
    .join("");

  const bodyD =
    "M 34 24 C 27 34 23 45 27 55 C 31 69 42 78 52 77 C 63 76 71 69 72 60 C 72 47 62 39 53 33 C 51 28 52 23 48 20 C 44 17 37 18 34 24 Z";

  return `<g>
    <path d="M 16 98 Q 50 94 84 74" fill="none" stroke="${PALETTE.jade}" stroke-width="4.4" stroke-linecap="round"/>
    ${leaves}
    <path d="M 47 74 L 44 89 M 56 72 L 55 86" fill="none" stroke="${PALETTE.crimson}" stroke-width="2.8" stroke-linecap="round"/>
    ${tailFeathers}
    <g transform="translate(0.8 1.1)"><path d="${bodyD}" fill="rgba(255, 255, 255, 0.78)"/></g>
    <path d="${bodyD}" fill="${PALETTE.jade}"/>
    <path d="M 34 37 Q 41 40 48 35" fill="none" stroke="${PALETTE.ivoryTop}" stroke-width="2.0" stroke-linecap="round"/>
    <path d="M 32 46 Q 30 64 46 71 Q 48 60 44 50 Z" fill="${PALETTE.crimson}"/>
    <path d="M 44 41 Q 63 39 74 58 Q 66 69 51 69 Q 42 55 44 41 Z" fill="${PALETTE.cobalt}"/>
    <path d="M 49 47 Q 58 47 66 54 M 49 54 Q 58 55 67 60 M 51 61 Q 57 62 63 65" fill="none" stroke="${PALETTE.ivoryTop}" stroke-width="1.65" stroke-linecap="round"/>
    <path d="M 34 22 Q 44 10 59 10 Q 53 18 49 22 Q 42 21 34 22 Z" fill="${PALETTE.crimson}"/>
    <path d="M 39 19 Q 46 15 53 13" fill="none" stroke="${PALETTE.ivoryTop}" stroke-width="1.3" stroke-linecap="round"/>
    <path d="M 32 24 L 18 27.5 L 31 31 Z" fill="${PALETTE.crimson}"/>
    <circle cx="36.7" cy="26.0" r="3.2" fill="${PALETTE.ivoryTop}"/>
    <circle cx="36.75" cy="26.05" r="1.65" fill="${PALETTE.ink}"/>
  </g>`;
}

function drawEngravedOneDotSvg() {
  const cx = 50;
  const cy = 56;
  const outerR = 41.0;
  const beadOrbit = outerR * 0.855;
  const beadR = outerR * 0.056;
  const beads = Array.from({ length: 16 }, (_, i) => {
    const ang = (i * Math.PI * 2) / 16;
    return `<circle cx="${(cx + Math.cos(ang) * beadOrbit).toFixed(2)}" cy="${(cy + Math.sin(ang) * beadOrbit).toFixed(2)}" r="${beadR.toFixed(2)}" fill="${PALETTE.ivoryTop}"/>`;
  }).join("");

  const wreathR = outerR * 0.66;
  const pw = wreathR * 0.28;
  const wreathPetals = Array.from({ length: 12 }, (_, i) => {
    const deg = i * 30;
    return `<g transform="translate(${cx} ${cy}) rotate(${deg})"><path d="M 0 0 Q ${(-pw).toFixed(2)} ${(-wreathR * 0.62).toFixed(2)} 0 ${(-wreathR).toFixed(2)} Q ${pw.toFixed(2)} ${(-wreathR * 0.62).toFixed(2)} 0 0 Z" fill="${PALETTE.jade}"/></g>`;
  }).join("");

  const dividers = Array.from({ length: 12 }, (_, i) => {
    const ang = ((i + 0.5) * Math.PI * 2) / 12;
    return `<path d="M ${cx} ${cy} L ${(cx + Math.cos(ang) * wreathR).toFixed(2)} ${(cy + Math.sin(ang) * wreathR).toFixed(2)}" stroke="${PALETTE.ivoryTop}" stroke-width="1.45"/>`;
  }).join("");

  const redR = outerR * 0.36;
  const innerPetals = Array.from({ length: 6 }, (_, i) => {
    const deg = i * 60;
    return `<g transform="translate(${cx} ${cy}) rotate(${deg})"><ellipse cx="0" cy="${(-redR * 0.5).toFixed(2)}" rx="${(redR * 0.2).toFixed(2)}" ry="${(redR * 0.3).toFixed(2)}" fill="${PALETTE.ivoryTop}"/></g>`;
  }).join("");

  return `<g>
    <circle cx="${(cx + 0.8).toFixed(2)}" cy="${(cy + 1.1).toFixed(2)}" r="${outerR}" fill="rgba(255, 255, 255, 0.8)"/>
    <circle cx="${cx}" cy="${cy}" r="${outerR}" fill="${PALETTE.cobalt}"/>
    ${beads}
    <circle cx="${cx}" cy="${cy}" r="${(outerR * 0.72).toFixed(2)}" fill="${PALETTE.ivoryTop}"/>
    ${wreathPetals}
    ${dividers}
    <circle cx="${cx}" cy="${cy}" r="${(outerR * 0.41).toFixed(2)}" fill="${PALETTE.ivoryTop}"/>
    <circle cx="${cx}" cy="${cy}" r="${redR.toFixed(2)}" fill="${PALETTE.crimson}"/>
    ${innerPetals}
    <circle cx="${cx}" cy="${cy}" r="${(redR * 0.25).toFixed(2)}" fill="${PALETTE.crimson}"/>
    <circle cx="${cx}" cy="${cy}" r="${(redR * 0.11).toFixed(2)}" fill="${PALETTE.ivoryTop}"/>
  </g>`;
}

function drawBlueFramedWhiteDragonSvg() {
  const outer = { x: 13.5, y: 10.0, w: 73.0, h: 92.0 };
  const inner = { x: 21.0, y: 17.5, w: 58.0, h: 77.0 };
  const cornerSize = 11.5;
  const corners = [
    [outer.x, outer.y],
    [outer.x + outer.w - cornerSize, outer.y],
    [outer.x, outer.y + outer.h - cornerSize],
    [outer.x + outer.w - cornerSize, outer.y + outer.h - cornerSize],
  ]
    .map(
      ([cx, cy]) => `<rect x="${cx}" y="${cy}" width="${cornerSize}" height="${cornerSize}" fill="${PALETTE.ivoryMid}" stroke="${PALETTE.cobalt}" stroke-width="2.4"/>
      <rect x="${(cx + 3.6).toFixed(1)}" y="${(cy + 3.6).toFixed(1)}" width="${(cornerSize - 7.2).toFixed(1)}" height="${(cornerSize - 7.2).toFixed(1)}" fill="${PALETTE.cobalt}"/>`,
    )
    .join("");

  return `<g>
    <rect x="${(outer.x + 0.7).toFixed(1)}" y="${(outer.y + 1.0).toFixed(1)}" width="${outer.w}" height="${outer.h}" rx="5" fill="none" stroke="rgba(255, 255, 255, 0.82)" stroke-width="4.4"/>
    <rect x="${outer.x}" y="${outer.y}" width="${outer.w}" height="${outer.h}" rx="5" fill="none" stroke="${PALETTE.cobalt}" stroke-width="4.0"/>
    <rect x="${inner.x}" y="${inner.y}" width="${inner.w}" height="${inner.h}" rx="2.5" fill="none" stroke="${PALETTE.cobalt}" stroke-width="2.0"/>
    ${corners}
    <rect x="42" y="${(outer.y + 1.5).toFixed(1)}" width="16" height="5.5" fill="${PALETTE.cobalt}"/>
    <rect x="42" y="${(outer.y + outer.h - 7.0).toFixed(1)}" width="16" height="5.5" fill="${PALETTE.cobalt}"/>
    <rect x="${(outer.x + 1.5).toFixed(1)}" y="47" width="5.5" height="18" fill="${PALETTE.cobalt}"/>
    <rect x="${(outer.x + outer.w - 7.0).toFixed(1)}" y="47" width="5.5" height="18" fill="${PALETTE.cobalt}"/>
  </g>`;
}

function drawEngravedTextSvg(text, cx, cy, fontSize, fill, scaleX = 1, scaleY = 1) {
  const strokeW = Math.max(0.8, (Math.abs(TYPO.strokeWidthPct) / 100) * fontSize).toFixed(2);
  return `<g transform="translate(${cx} ${cy}) scale(${scaleX} ${scaleY})">
    <text x="0.7" y="1.05" text-anchor="middle" dominant-baseline="central" font-family="${FONT_CARVED}" font-weight="900" font-size="${fontSize}" fill="rgba(255, 255, 255, 0.78)" stroke="rgba(255, 255, 255, 0.78)" stroke-width="${strokeW}" stroke-linejoin="round">${text}</text>
    <text x="0" y="0" text-anchor="middle" dominant-baseline="central" font-family="${FONT_CARVED}" font-weight="900" font-size="${fontSize}" fill="${fill}" stroke="${fill}" stroke-width="${strokeW}" stroke-linejoin="round">${text}</text>
  </g>`;
}

function tileFaceInnerSvg(tileIndex) {
  if (tileIndex < 9) {
    const n = tileIndex + 1;
    const numY = n === 1 ? 28.5 : 27.5;
    return `<g>
      ${drawEngravedTextSvg(CN_NUM[n], 50, numY, TYPO.manNumeralSize, PALETTE.ink, n === 1 ? 1.08 : 1.04, 0.96)}
      ${drawEngravedTextSvg("萬", 50, 82.0, TYPO.manWanSize, PALETTE.crimson, 1.05, 0.98)}
    </g>`;
  }
  if (tileIndex < 18) {
    const rank = tileIndex - 9 + 1;
    if (rank === 1) {
      return drawEngravedOneBambooSvg();
    }
    const bw = GEOM.bambooWidths[String(rank)];
    return STYLE_A_SPEC.bamboo[String(rank)]
      .map(({ x, y, h, color, angle }) =>
        drawCarvedBambooSvg(x, y, h, PALETTE[color], bw, angle),
      )
      .join("");
  }
  if (tileIndex < 27) {
    const rank = tileIndex - 18 + 1;
    if (rank === 1) {
      return drawEngravedOneDotSvg();
    }
    return STYLE_A_SPEC.dot[String(rank)]
      .map(({ x, y, r, color }) => drawCarvedPinSvg(x, y, r, PALETTE[color]))
      .join("");
  }
  if (tileIndex === 33) {
    return drawBlueFramedWhiteDragonSvg();
  }
  const glyph = HONOR_GLYPHS[tileIndex - 27];
  const color =
    tileIndex === 31
      ? PALETTE.crimson
      : tileIndex === 32
        ? PALETTE.jade
        : PALETTE.ink;
  return drawEngravedTextSvg(glyph, 50, 56, TYPO.honorSize, color, 1.02, 1.04);
}

function buildBodyInnerSvg(kind) {
  const r = GEOM.cornerRadius;
  if (kind === "shadow") {
    return `<rect x="1.5" y="3.5" width="97" height="130" rx="${(r + 2).toFixed(1)}" fill="rgba(4, 9, 13, 0.14)"/>
  <rect x="3.0" y="5.5" width="94" height="127" rx="${(r + 1).toFixed(1)}" fill="rgba(4, 9, 13, 0.22)"/>
  <rect x="4.5" y="7.5" width="91" height="124" rx="${r.toFixed(1)}" fill="rgba(4, 9, 13, 0.28)"/>
  <rect x="6.5" y="9.5" width="87" height="120" rx="${(r - 1).toFixed(1)}" fill="rgba(4, 9, 13, 0.22)"/>`;
  }

  const isBack = kind === "back" || kind === "laid_back";
  const isLaid = kind === "laid" || kind === "laid_back";
  const face = faceRectFor(isLaid);

  if (!isLaid) {
    const backBlock = `<rect x="2" y="1.5" width="96" height="132" rx="${r}" fill="url(#jadeStandingGrad)"/>
  <rect x="6" y="2.2" width="88" height="3.0" rx="1.5" fill="rgba(216, 255, 240, 0.26)"/>`;
    if (isBack) {
      const cy = face.y + face.h * 0.5;
      return `${backBlock}
  <rect x="${face.x}" y="${face.y}" width="${face.w}" height="${face.h}" rx="${r}" fill="url(#jadeBackFaceGrad)"/>
  <rect x="${(face.x + 10.5).toFixed(1)}" y="${(face.y + 10.5).toFixed(1)}" width="${(face.w - 21).toFixed(1)}" height="${(face.h - 21).toFixed(1)}" rx="${(r - 5).toFixed(1)}" fill="none" stroke="${PALETTE.jadeInlay}" stroke-width="1.8"/>
  <path d="M 50 ${(cy - 22).toFixed(1)} L 65 ${cy.toFixed(1)} L 50 ${(cy + 22).toFixed(1)} L 35 ${cy.toFixed(1)} Z" fill="none" stroke="${PALETTE.jadeInlay}" stroke-width="1.8"/>
  <circle cx="50" cy="${cy.toFixed(1)}" r="4.5" fill="${PALETTE.jadeGoldDot}"/>
  <rect x="2" y="1.5" width="96" height="132" rx="${r}" fill="none" stroke="${PALETTE.jadeRimStroke}" stroke-width="1.0"/>`;
    }
    return `${backBlock}
  <rect x="2" y="${(1.5 + GEOM.standingTopCap - 2.6).toFixed(1)}" width="96" height="6" fill="rgba(4, 43, 30, 0.34)"/>
  <rect x="${face.x}" y="${face.y}" width="${face.w}" height="${face.h}" rx="${(r - 0.5).toFixed(1)}" fill="url(#ivoryFaceGrad)" stroke="${PALETTE.ivoryRimStroke}" stroke-width="0.95"/>
  <rect x="4" y="${face.y}" width="92" height="3.2" rx="1.6" fill="${PALETTE.ivoryBevelLight}"/>
  <rect x="2" y="1.5" width="96" height="132" rx="${r}" fill="none" stroke="${PALETTE.jadeRimStroke}" stroke-width="0.85"/>`;
  }

  const wallBottomY = 133.5 - GEOM.laidJadeLip;
  if (isBack) {
    const cy = face.y + face.h * 0.5;
    return `<rect x="2" y="2" width="96" height="131.5" rx="${r}" fill="url(#ivoryWallGrad)"/>
  <rect x="2" y="2" width="96" height="${(wallBottomY - 2).toFixed(1)}" rx="${r}" fill="url(#jadeStandingGrad)"/>
  <rect x="${face.x}" y="${face.y}" width="${face.w}" height="${face.h}" rx="${(r - 0.5).toFixed(1)}" fill="url(#jadeBackFaceGrad)"/>
  <rect x="${(face.x + 10.5).toFixed(1)}" y="${(face.y + 9.5).toFixed(1)}" width="${(face.w - 21).toFixed(1)}" height="${(face.h - 19).toFixed(1)}" rx="${(r - 5).toFixed(1)}" fill="none" stroke="${PALETTE.jadeInlay}" stroke-width="1.7"/>
  <path d="M 50 ${(cy - 18).toFixed(1)} L 63 ${cy.toFixed(1)} L 50 ${(cy + 18).toFixed(1)} L 37 ${cy.toFixed(1)} Z" fill="none" stroke="${PALETTE.jadeInlay}" stroke-width="1.7"/>
  <circle cx="50" cy="${cy.toFixed(1)}" r="4" fill="${PALETTE.jadeGoldDot}"/>
  <rect x="2" y="2" width="96" height="131.5" rx="${r}" fill="none" stroke="${PALETTE.jadeRimStroke}" stroke-width="0.95"/>`;
  }

  return `<rect x="2" y="2" width="96" height="131.5" rx="${r}" fill="url(#jadeLipGrad)"/>
  <rect x="2" y="2" width="96" height="${(wallBottomY - 2).toFixed(1)}" rx="${(r - 0.2).toFixed(1)}" fill="url(#ivoryWallGrad)"/>
  <rect x="${face.x}" y="${face.y}" width="${face.w}" height="${face.h}" rx="${(r - 0.6).toFixed(1)}" fill="url(#ivoryFaceGrad)" stroke="${PALETTE.ivoryRimStroke}" stroke-width="0.75"/>
  <rect x="6" y="${(face.y + face.h - 2.3).toFixed(1)}" width="88" height="1.8" rx="0.9" fill="rgba(255, 255, 255, 0.76)"/>
  <rect x="6" y="${face.y}" width="88" height="2.2" rx="1.1" fill="${PALETTE.ivoryBevelLight}"/>
  <rect x="2" y="2" width="96" height="131.5" rx="${r}" fill="none" stroke="rgba(11, 43, 33, 0.36)" stroke-width="0.85"/>`;
}

function sharedDefsSvg() {
  return `<defs>
    <linearGradient id="ivoryFaceGrad" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="${PALETTE.ivoryTop}"/>
      <stop offset="75%" stop-color="${PALETTE.ivoryMid}"/>
      <stop offset="100%" stop-color="${PALETTE.ivoryBot}"/>
    </linearGradient>
    <linearGradient id="ivoryWallGrad" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="${PALETTE.ivoryWallTop}"/>
      <stop offset="100%" stop-color="${PALETTE.ivoryWallBot}"/>
    </linearGradient>
    <linearGradient id="jadeStandingGrad" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="${PALETTE.jadeTopHi}"/>
      <stop offset="18%" stop-color="${PALETTE.jadeMid}"/>
      <stop offset="100%" stop-color="${PALETTE.jadeDeep}"/>
    </linearGradient>
    <linearGradient id="jadeBackFaceGrad" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="${PALETTE.jadeTopHi}"/>
      <stop offset="50%" stop-color="${PALETTE.jadeMid}"/>
      <stop offset="100%" stop-color="${PALETTE.jadeDeep}"/>
    </linearGradient>
    <linearGradient id="jadeLipGrad" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0%" stop-color="${PALETTE.jadeMid}"/>
      <stop offset="60%" stop-color="${PALETTE.jadeDeep}"/>
      <stop offset="100%" stop-color="#04291D"/>
    </linearGradient>
  </defs>`;
}

function buildBodySvg(kind) {
  return `<svg viewBox="0 0 100 135" xmlns="http://www.w3.org/2000/svg">
  ${kind === "shadow" ? "" : sharedDefsSvg()}
  ${buildBodyInnerSvg(kind)}
</svg>\n`;
}

function buildTileSvg(tileIndex, laid = false) {
  const face = faceRectFor(laid);
  const { insetX, insetY, width: artBoxW, height: artBoxH } = GEOM.artBox;
  const targetW = face.w - insetX * 2;
  const targetH = face.h - insetY * 2;
  const uniformScale = Math.min(targetW / artBoxW, targetH / artBoxH);
  const artW = artBoxW * uniformScale;
  const artH = artBoxH * uniformScale;
  const artX = face.x + face.w * 0.5 - artW * 0.5;
  const artY = face.y + face.h * 0.5 - artH * 0.5;

  return `<svg viewBox="0 0 100 135" xmlns="http://www.w3.org/2000/svg">
  ${sharedDefsSvg()}
  ${buildBodyInnerSvg(laid ? "laid" : "standing")}
  <g transform="translate(${artX.toFixed(2)} ${artY.toFixed(2)}) scale(${uniformScale.toFixed(4)})">
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
      app: "majjj-atlas-builder",
      version: "3.0",
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
        specPath,
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
console.log("Built @2x physical carved thick-jade tile atlas (1920x2048) and impasto felt textures.");
