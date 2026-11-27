import {
  CanvasSource,
  ImageSource,
  Rectangle,
  Spritesheet,
  type SpritesheetData,
  Texture,
} from "pixi.js";
import atlasMetaJson from "../../assets/atlas/tiles@2x.json";
import atlasPngUrl from "../../assets/atlas/tiles@2x.png";
import feltBrushUrl from "../../assets/textures/felt-brush@2x.png";
import feltImpastoUrl from "../../assets/textures/felt-impasto@2x.png";
import type { Tile } from "../protocol/generated/Tile";
import {
  ATLAS_GRID,
  ATLAS_REQUIRED_BODY_FRAMES,
  ATLAS_TILE_FRAME_COUNT,
  type BambooStalkSpec,
  type PlumPinSpec,
  type SpecColorKey,
  STYLE_A_GEOMETRY,
  STYLE_A_PALETTE,
  STYLE_A_SPEC,
  STYLE_A_TYPOGRAPHY,
  toRgbaCss,
} from "./atlasConstants";
import {
  getDragonKind,
  getTileLabel,
  getTileNumeral,
  getTileRank,
  getTileSuit,
} from "./tiles";

export type TileBodyKind = "standing" | "laid";
export { ATLAS_REQUIRED_BODY_FRAMES, ATLAS_TILE_FRAME_COUNT };

const CELL_W = ATLAS_GRID.cellW;
const CELL_H = ATLAS_GRID.cellH;
const FRAME_W = ATLAS_GRID.frameW;
const FRAME_H = ATLAS_GRID.frameH;
const FRAME_PAD = ATLAS_GRID.pad;
const GRID_COLS = ATLAS_GRID.cols;
const GRID_ROWS = ATLAS_GRID.rows;
const ATLAS_W = CELL_W * GRID_COLS; // 1920 <= 2048
const ATLAS_H = CELL_H * GRID_ROWS; // 2048 <= 2048

const CORNER_RADIUS = STYLE_A_GEOMETRY.cornerRadius;
const STANDING_TOP_CAP = STYLE_A_GEOMETRY.standingTopCap;
const LAID_IVORY_WALL = STYLE_A_GEOMETRY.laidIvoryWall;
const LAID_JADE_LIP = STYLE_A_GEOMETRY.laidJadeLip;

const IVORY_TOP = STYLE_A_PALETTE.ivoryTop;
const IVORY_MID = STYLE_A_PALETTE.ivoryMid;
const IVORY_BOT = STYLE_A_PALETTE.ivoryBot;
const IVORY_WALL_TOP = STYLE_A_PALETTE.ivoryWallTop;
const IVORY_WALL_BOT = STYLE_A_PALETTE.ivoryWallBot;
const IVORY_RIM_STROKE = toRgbaCss(STYLE_A_PALETTE.ivoryRimStroke);
const IVORY_BEVEL_LIGHT = toRgbaCss(STYLE_A_PALETTE.ivoryBevelLight);
const IVORY_BEVEL_SHADOW = toRgbaCss(STYLE_A_PALETTE.ivoryBevelShadow);
const JADE_TOP_HI = STYLE_A_PALETTE.jadeTopHi;
const JADE_MID = STYLE_A_PALETTE.jadeMid;
const JADE_DEEP = STYLE_A_PALETTE.jadeDeep;
const JADE_RIM_STROKE = toRgbaCss(STYLE_A_PALETTE.jadeRimStroke);
const JADE_INLAY = toRgbaCss(STYLE_A_PALETTE.jadeInlay);
const JADE_GOLD_DOT = toRgbaCss(STYLE_A_PALETTE.jadeGoldDot);
const INK = STYLE_A_PALETTE.ink;
const RED = STYLE_A_PALETTE.crimson;
const GREEN = STYLE_A_PALETTE.jade;
const BLUE = STYLE_A_PALETTE.cobalt;

const CARVED_FONT_STACK = `"${STYLE_A_TYPOGRAPHY.primaryFont}", "${STYLE_A_TYPOGRAPHY.fallbackFont}", "Kaiti SC", "STKaiti", "Songti SC", serif`;

function resolveSpecColor(key: SpecColorKey): string {
  return STYLE_A_PALETTE[key];
}

export interface TileAtlas {
  readonly width: number;
  readonly height: number;
  readonly backTexture: Texture;
  readonly laidBackTexture: Texture;
  readonly shadowTexture: Texture;
  readonly feltImpastoTexture: Texture;
  readonly feltBrushTexture: Texture;
  getTileTexture(tile: Tile, body?: TileBodyKind): Texture;
  destroy(): void;
}

function loadImageElement(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.decoding = "async";
    img.onload = () => resolve(img);
    img.onerror = () =>
      reject(new Error(`Failed to load @2x asset image: ${url}`));
    img.src = url;
  });
}

async function loadImageTexture(
  url: string,
  label: string,
  resolution = 2,
): Promise<{ texture: Texture; source: ImageSource }> {
  const img = await loadImageElement(url);
  const source = new ImageSource({
    resource: img,
    resolution,
    autoGenerateMipmaps: true,
    label,
  });
  return {
    texture: new Texture({ source }),
    source,
  };
}

/**
 * Loads the Vite-bundled `@2x` vector-export Spritesheet (`tiles@2x.png` + `tiles@2x.json`)
 * and seamless Piltover impasto felt textures (`felt-impasto@2x.png`, `felt-brush@2x.png`).
 * Falls back to `createTileAtlas()` in headless/test environments or if image decoding fails.
 */
export async function loadTileAtlas(): Promise<TileAtlas> {
  if (typeof Image === "undefined" || typeof document === "undefined") {
    return createTileAtlas();
  }

  let atlasSource: ImageSource | null = null;
  let impastoSource: ImageSource | null = null;
  let brushSource: ImageSource | null = null;
  let sheet: Spritesheet | null = null;
  let atlasTexture: Texture | null = null;
  let feltImpastoTexture: Texture | null = null;
  let feltBrushTexture: Texture | null = null;

  try {
    const [atlasLoaded, impastoLoaded, brushLoaded] = await Promise.all([
      loadImageTexture(atlasPngUrl, "majjj-tiles@2x", 2),
      loadImageTexture(feltImpastoUrl, "majjj-felt-impasto@2x", 2),
      loadImageTexture(feltBrushUrl, "majjj-felt-brush@2x", 2),
    ]);

    atlasSource = atlasLoaded.source;
    atlasTexture = atlasLoaded.texture;
    impastoSource = impastoLoaded.source;
    feltImpastoTexture = impastoLoaded.texture;
    brushSource = brushLoaded.source;
    feltBrushTexture = brushLoaded.texture;

    const sheetData = atlasMetaJson as unknown as SpritesheetData;
    sheet = new Spritesheet(atlasTexture, sheetData);
    await sheet.parse();

    const standingTextures: Texture[] = [];
    const laidTextures: Texture[] = [];
    for (let t = 0; t < ATLAS_TILE_FRAME_COUNT; t += 1) {
      const standingTex = sheet.textures[`tile_${t}`];
      if (!standingTex) {
        throw new Error(`Missing spritesheet frame tile_${t}`);
      }
      const laidTex = sheet.textures[`tile_laid_${t}`];
      if (!laidTex) {
        throw new Error(`Missing spritesheet frame tile_laid_${t}`);
      }
      standingTextures.push(standingTex);
      laidTextures.push(laidTex);
    }

    const backTexture = sheet.textures.body_back;
    const laidBackTexture = sheet.textures.body_laid_back;
    const shadowTexture = sheet.textures.body_shadow;
    if (!backTexture || !laidBackTexture || !shadowTexture) {
      throw new Error("Missing required tile body frame in tiles@2x.json");
    }

    const metaSize = sheetData.meta.size ?? { w: ATLAS_W, h: ATLAS_H };

    return {
      width: metaSize.w,
      height: metaSize.h,
      backTexture,
      laidBackTexture,
      shadowTexture,
      feltImpastoTexture,
      feltBrushTexture,
      getTileTexture(tile: Tile, body: TileBodyKind = "standing"): Texture {
        if (body === "laid") {
          return laidTextures[tile] ?? laidBackTexture;
        }
        return standingTextures[tile] ?? backTexture;
      },
      destroy(): void {
        sheet?.destroy(true);
        feltImpastoTexture?.destroy(true);
        feltBrushTexture?.destroy(true);
      },
    };
  } catch (err) {
    if (sheet) {
      sheet.destroy(true);
    } else {
      atlasTexture?.destroy(true);
      atlasSource?.destroy();
    }
    if (feltImpastoTexture) {
      feltImpastoTexture.destroy(true);
    } else {
      impastoSource?.destroy();
    }
    if (feltBrushTexture) {
      feltBrushTexture.destroy(true);
    } else {
      brushSource?.destroy();
    }
    if (typeof console !== "undefined") {
      console.warn(
        "Falling back to procedural canvas TileAtlas:",
        err instanceof Error ? err.message : err,
      );
    }
    return createTileAtlas();
  }
}

interface NormalizedRect {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
}

function faceRectFor(laid: boolean): NormalizedRect {
  if (laid) {
    const faceH = 133.5 - 2.0 - LAID_JADE_LIP - LAID_IVORY_WALL;
    return { x: 2.0, y: 2.0, w: 96.0, h: faceH };
  }
  const y = 1.5 + STANDING_TOP_CAP;
  return { x: 2.0, y, w: 96.0, h: 133.5 - y };
}

function roundRectPath(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  const clampedR = Math.min(r, Math.min(w, h) * 0.5);
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, clampedR);
}

function makeVerticalGradient(
  ctx: CanvasRenderingContext2D,
  y0: number,
  y1: number,
  stops: ReadonlyArray<readonly [number, string]>,
  fallback: string,
): string | CanvasGradient {
  if (typeof ctx.createLinearGradient !== "function") {
    return fallback;
  }
  const grad = ctx.createLinearGradient(0, y0, 0, y1);
  for (const [offset, color] of stops) {
    grad.addColorStop(offset, color);
  }
  return grad;
}

function drawTileBase(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  kind: "standing" | "laid" | "back" | "shadow" | "laid_back",
): void {
  ctx.save();
  ctx.translate(ox, oy);
  ctx.scale(FRAME_W / 100, FRAME_H / 135);

  const r = CORNER_RADIUS;

  if (kind === "shadow") {
    const layers: ReadonlyArray<readonly [number, number, number, number, number, number]> = [
      [1.5, 3.5, 97, 130, r + 2, 0.14],
      [3.0, 5.5, 94, 127, r + 1, 0.22],
      [4.5, 7.5, 91, 124, r, 0.28],
      [6.5, 9.5, 87, 120, r - 1, 0.22],
    ];
    for (const [x, y, w, h, rad, alpha] of layers) {
      roundRectPath(ctx, x, y, w, h, rad);
      ctx.fillStyle = `rgba(4, 9, 13, ${alpha})`;
      ctx.fill();
    }
    ctx.restore();
    return;
  }

  const isBack = kind === "back" || kind === "laid_back";
  const isLaid = kind === "laid" || kind === "laid_back";
  const face = faceRectFor(isLaid);

  if (!isLaid) {
    // Standing tile: thick emerald jade back block extending above ivory face
    roundRectPath(ctx, 2, 1.5, 96, 132, r);
    ctx.fillStyle = makeVerticalGradient(
      ctx,
      1.5,
      133.5,
      [
        [0, JADE_TOP_HI],
        [0.18, JADE_MID],
        [1, JADE_DEEP],
      ],
      JADE_MID,
    );
    ctx.fill();

    ctx.fillStyle = "rgba(216, 255, 240, 0.26)";
    ctx.fillRect(6, 2.2, 88, 3.0);

    if (isBack) {
      roundRectPath(ctx, face.x, face.y, face.w, face.h, r);
      ctx.fillStyle = makeVerticalGradient(
        ctx,
        face.y,
        face.y + face.h,
        [
          [0, JADE_TOP_HI],
          [0.45, JADE_MID],
          [1, JADE_DEEP],
        ],
        JADE_MID,
      );
      ctx.fill();

      roundRectPath(
        ctx,
        face.x + 10.5,
        face.y + 10.5,
        face.w - 21,
        face.h - 21,
        Math.max(4, r - 5),
      );
      ctx.strokeStyle = JADE_INLAY;
      ctx.lineWidth = 1.8;
      ctx.stroke();

      const cy = face.y + face.h * 0.5;
      ctx.beginPath();
      ctx.moveTo(50, cy - 22);
      ctx.lineTo(65, cy);
      ctx.lineTo(50, cy + 22);
      ctx.lineTo(35, cy);
      ctx.closePath();
      ctx.stroke();

      ctx.beginPath();
      ctx.arc(50, cy, 4.5, 0, Math.PI * 2);
      ctx.fillStyle = JADE_GOLD_DOT;
      ctx.fill();

      roundRectPath(ctx, 2, 1.5, 96, 132, r);
      ctx.strokeStyle = JADE_RIM_STROKE;
      ctx.lineWidth = 1.0;
      ctx.stroke();
      ctx.restore();
      return;
    }

    // Subtle ivory-to-jade transitional bevel seam
    ctx.fillStyle = "rgba(4, 43, 30, 0.34)";
    ctx.fillRect(2, 1.5 + STANDING_TOP_CAP - 2.6, 96, 6);

    // Standing front ivory face
    roundRectPath(ctx, face.x, face.y, face.w, face.h, r - 0.5);
    ctx.fillStyle = makeVerticalGradient(
      ctx,
      face.y,
      face.y + face.h,
      [
        [0, IVORY_TOP],
        [0.72, IVORY_MID],
        [1, IVORY_BOT],
      ],
      IVORY_MID,
    );
    ctx.fill();

    ctx.save();
    ctx.clip();
    ctx.fillStyle = IVORY_BEVEL_LIGHT;
    ctx.fillRect(4, face.y, 92, 3.2);
    ctx.fillStyle = IVORY_BEVEL_SHADOW;
    ctx.fillRect(4, face.y + face.h - 5.5, 92, 5.5);
    ctx.restore();

    roundRectPath(ctx, face.x, face.y, face.w, face.h, r - 0.5);
    ctx.strokeStyle = IVORY_RIM_STROKE;
    ctx.lineWidth = 0.95;
    ctx.stroke();

    roundRectPath(ctx, 2, 1.5, 96, 132, r);
    ctx.strokeStyle = JADE_RIM_STROKE;
    ctx.lineWidth = 0.85;
    ctx.stroke();

    ctx.restore();
    return;
  }

  // Laid tile: dual-layer 3D block (ivory side wall + emerald jade base lip)
  const wallBottomY = 133.5 - LAID_JADE_LIP;

  if (isBack) {
    // Upside-down laid tile: ivory underside lip, jade wall, jade top back
    roundRectPath(ctx, 2, 2, 96, 131.5, r);
    ctx.fillStyle = makeVerticalGradient(
      ctx,
      wallBottomY - 2,
      133.5,
      [
        [0, IVORY_WALL_TOP],
        [1, IVORY_WALL_BOT],
      ],
      IVORY_WALL_BOT,
    );
    ctx.fill();

    roundRectPath(ctx, 2, 2, 96, wallBottomY - 2, r);
    ctx.fillStyle = makeVerticalGradient(
      ctx,
      face.y + face.h - 2,
      wallBottomY,
      [
        [0, JADE_MID],
        [1, JADE_DEEP],
      ],
      JADE_DEEP,
    );
    ctx.fill();

    roundRectPath(ctx, face.x, face.y, face.w, face.h, r - 0.5);
    ctx.fillStyle = makeVerticalGradient(
      ctx,
      face.y,
      face.y + face.h,
      [
        [0, JADE_TOP_HI],
        [0.55, JADE_MID],
        [1, JADE_DEEP],
      ],
      JADE_MID,
    );
    ctx.fill();

    roundRectPath(
      ctx,
      face.x + 10.5,
      face.y + 9.5,
      face.w - 21,
      face.h - 19,
      Math.max(4, r - 5),
    );
    ctx.strokeStyle = JADE_INLAY;
    ctx.lineWidth = 1.7;
    ctx.stroke();

    const cy = face.y + face.h * 0.5;
    ctx.beginPath();
    ctx.moveTo(50, cy - 18);
    ctx.lineTo(63, cy);
    ctx.lineTo(50, cy + 18);
    ctx.lineTo(37, cy);
    ctx.closePath();
    ctx.stroke();

    ctx.beginPath();
    ctx.arc(50, cy, 4, 0, Math.PI * 2);
    ctx.fillStyle = JADE_GOLD_DOT;
    ctx.fill();

    roundRectPath(ctx, 2, 2, 96, 131.5, r);
    ctx.strokeStyle = JADE_RIM_STROKE;
    ctx.lineWidth = 0.95;
    ctx.stroke();
    ctx.restore();
    return;
  }

  // 1. Bottom emerald jade base lip
  roundRectPath(ctx, 2, 2, 96, 131.5, r);
  ctx.fillStyle = makeVerticalGradient(
    ctx,
    wallBottomY - 3,
    133.5,
    [
      [0, JADE_MID],
      [0.6, JADE_DEEP],
      [1, "#04291D"],
    ],
    JADE_DEEP,
  );
  ctx.fill();

  // 2. Middle ivory vertical side wall
  roundRectPath(ctx, 2, 2, 96, wallBottomY - 2, r - 0.2);
  ctx.fillStyle = makeVerticalGradient(
    ctx,
    face.y + face.h - 2,
    wallBottomY,
    [
      [0, IVORY_WALL_TOP],
      [1, IVORY_WALL_BOT],
    ],
    IVORY_WALL_TOP,
  );
  ctx.fill();

  // 3. Top engraved ivory face
  roundRectPath(ctx, face.x, face.y, face.w, face.h, r - 0.6);
  ctx.fillStyle = makeVerticalGradient(
    ctx,
    face.y,
    face.y + face.h,
    [
      [0, IVORY_TOP],
      [0.78, IVORY_MID],
      [1, IVORY_BOT],
    ],
    IVORY_MID,
  );
  ctx.fill();

  ctx.save();
  ctx.clip();
  ctx.fillStyle = "rgba(255, 255, 255, 0.76)";
  ctx.fillRect(6, face.y + face.h - 2.3, 88, 1.8);
  ctx.fillStyle = IVORY_BEVEL_LIGHT;
  ctx.fillRect(6, face.y, 88, 2.2);
  ctx.restore();

  roundRectPath(ctx, face.x + 0.35, face.y + 0.35, face.w - 0.7, face.h - 0.7, r - 0.8);
  ctx.strokeStyle = IVORY_RIM_STROKE;
  ctx.lineWidth = 0.75;
  ctx.stroke();

  roundRectPath(ctx, 2, 2, 96, 131.5, r);
  ctx.strokeStyle = "rgba(11, 43, 33, 0.36)";
  ctx.lineWidth = 0.85;
  ctx.stroke();

  ctx.restore();
}

function drawEngravedGlyph(
  ctx: CanvasRenderingContext2D,
  text: string,
  cx: number,
  cy: number,
  fontSize: number,
  color: string,
  scaleX = 1,
  scaleY = 1,
): void {
  ctx.save();
  ctx.translate(cx, cy);
  ctx.scale(scaleX, scaleY);
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.lineJoin = "round";
  ctx.font = `900 ${fontSize}px ${CARVED_FONT_STACK}`;
  ctx.lineWidth = Math.max(0.8, (Math.abs(STYLE_A_TYPOGRAPHY.strokeWidthPct) / 100) * fontSize);

  // Intaglio recessed catch-light
  ctx.fillStyle = "rgba(255, 255, 255, 0.78)";
  ctx.strokeStyle = "rgba(255, 255, 255, 0.78)";
  ctx.strokeText(text, 0.7, 1.05);
  ctx.fillText(text, 0.7, 1.05);

  // Primary bold carved lacquer glyph
  ctx.fillStyle = color;
  ctx.strokeStyle = color;
  ctx.strokeText(text, 0, 0);
  ctx.fillText(text, 0, 0);
  ctx.restore();
}

function drawManTile(ctx: CanvasRenderingContext2D, tile: Tile): void {
  const rank = getTileRank(tile) ?? 1;
  drawEngravedGlyph(
    ctx,
    getTileNumeral(tile),
    50,
    rank === 1 ? 28.5 : 27.5,
    STYLE_A_TYPOGRAPHY.manNumeralSize,
    INK,
    rank === 1 ? 1.08 : 1.04,
    0.96,
  );
  drawEngravedGlyph(
    ctx,
    "萬",
    50,
    82,
    STYLE_A_TYPOGRAPHY.manWanSize,
    RED,
    1.05,
    0.98,
  );
}

function drawCarvedBambooStalk(
  ctx: CanvasRenderingContext2D,
  cx: number,
  cy: number,
  h: number,
  bw: number,
  color: string,
  angleDeg = 0,
): void {
  ctx.save();
  ctx.translate(cx, cy);
  if (angleDeg !== 0) {
    ctx.rotate((angleDeg * Math.PI) / 180);
  }

  const halfH = h * 0.5;
  const stemW = bw * 0.74;
  const waistW = stemW * 0.78;
  const nodeW = bw * 1.18;
  const nodeH = Math.max(4.2, Math.min(6.2, h * 0.145));

  const drawBody = (dx: number, dy: number, fillColor: string) => {
    ctx.save();
    ctx.translate(dx, dy);
    ctx.fillStyle = fillColor;
    ctx.beginPath();
    ctx.moveTo(-stemW * 0.5, -halfH + 2.2);
    ctx.quadraticCurveTo(-waistW * 0.45, -halfH * 0.5, -stemW * 0.5, 0);
    ctx.quadraticCurveTo(-waistW * 0.45, halfH * 0.5, -stemW * 0.5, halfH - 2.2);
    ctx.lineTo(stemW * 0.5, halfH - 2.2);
    ctx.quadraticCurveTo(waistW * 0.45, halfH * 0.5, stemW * 0.5, 0);
    ctx.quadraticCurveTo(waistW * 0.45, -halfH * 0.5, stemW * 0.5, -halfH + 2.2);
    ctx.closePath();
    ctx.fill();

    for (const [nx, ny, nw, nh] of [
      [-nodeW * 0.5, -halfH, nodeW, nodeH],
      [-nodeW * 0.54, -nodeH * 0.52, nodeW * 1.08, nodeH * 1.04],
      [-nodeW * 0.5, halfH - nodeH, nodeW, nodeH],
    ] as const) {
      roundRectPath(ctx, nx, ny, nw, nh, nodeH * 0.48);
      ctx.fill();
    }
    ctx.restore();
  };

  drawBody(0.7, 0.95, "rgba(255, 255, 255, 0.75)");
  drawBody(0, 0, color);

  // Carved ivory split groove & center notch
  ctx.strokeStyle = IVORY_TOP;
  ctx.lineCap = "round";
  ctx.lineWidth = Math.max(1.5, bw * 0.16);
  const segGap = nodeH * 0.65;
  ctx.beginPath();
  ctx.moveTo(0, -halfH + nodeH * 0.85);
  ctx.lineTo(0, -segGap);
  ctx.moveTo(0, segGap);
  ctx.lineTo(0, halfH - nodeH * 0.85);
  ctx.stroke();

  ctx.lineWidth = Math.max(1.2, bw * 0.12);
  ctx.beginPath();
  ctx.moveTo(-nodeW * 0.28, 0);
  ctx.lineTo(nodeW * 0.28, 0);
  ctx.stroke();

  const dotR = Math.max(0.85, bw * 0.085);
  ctx.fillStyle = IVORY_TOP;
  ctx.beginPath();
  ctx.arc(0, -halfH + nodeH * 0.5, dotR, 0, Math.PI * 2);
  ctx.arc(0, halfH - nodeH * 0.5, dotR, 0, Math.PI * 2);
  ctx.fill();

  ctx.restore();
}

function drawEngravedOneBamboo(ctx: CanvasRenderingContext2D): void {
  ctx.save();
  // Branch
  ctx.strokeStyle = GREEN;
  ctx.lineWidth = 4.4;
  ctx.lineCap = "round";
  ctx.beginPath();
  ctx.moveTo(16, 98);
  ctx.quadraticCurveTo(50, 94, 84, 74);
  ctx.stroke();

  // Legs
  ctx.strokeStyle = RED;
  ctx.lineWidth = 2.8;
  ctx.beginPath();
  ctx.moveTo(47, 74);
  ctx.lineTo(44, 89);
  ctx.moveTo(56, 72);
  ctx.lineTo(55, 86);
  ctx.stroke();

  // Tail feathers
  const feathers: ReadonlyArray<readonly [number, number, string, number]> = [
    [0.32, 32, RED, 4.8],
    [0.52, 37, GREEN, 5.4],
    [0.72, 40, BLUE, 5.4],
    [0.92, 36, GREEN, 5.0],
    [1.12, 30, RED, 4.6],
  ];
  for (const [ang, len, col, wid] of feathers) {
    ctx.save();
    ctx.translate(58, 65);
    ctx.rotate(ang);
    ctx.beginPath();
    ctx.moveTo(0, 0);
    ctx.quadraticCurveTo(len * 0.55, -wid, len, 0);
    ctx.quadraticCurveTo(len * 0.55, wid, 0, 0);
    ctx.closePath();
    ctx.fillStyle = col;
    ctx.fill();
    ctx.restore();
  }

  // Bird torso
  ctx.beginPath();
  ctx.moveTo(34, 24);
  if (typeof ctx.bezierCurveTo === "function") {
    ctx.bezierCurveTo(27, 34, 23, 45, 27, 55);
    ctx.bezierCurveTo(31, 69, 42, 78, 52, 77);
    ctx.bezierCurveTo(63, 76, 71, 69, 72, 60);
    ctx.bezierCurveTo(72, 47, 62, 39, 53, 33);
    ctx.bezierCurveTo(51, 28, 52, 23, 48, 20);
    ctx.bezierCurveTo(44, 17, 37, 18, 34, 24);
  } else {
    ctx.quadraticCurveTo(23, 48, 52, 77);
    ctx.quadraticCurveTo(73, 62, 53, 33);
    ctx.quadraticCurveTo(48, 17, 34, 24);
  }
  ctx.closePath();
  ctx.fillStyle = GREEN;
  ctx.fill();

  // Crimson breast & head crest
  ctx.beginPath();
  ctx.moveTo(32, 46);
  ctx.quadraticCurveTo(30, 64, 46, 71);
  ctx.quadraticCurveTo(48, 60, 44, 50);
  ctx.closePath();
  ctx.fillStyle = RED;
  ctx.fill();

  // Cobalt wing + ivory feather arcs
  ctx.beginPath();
  ctx.moveTo(44, 41);
  ctx.quadraticCurveTo(63, 39, 74, 58);
  ctx.quadraticCurveTo(66, 69, 51, 69);
  ctx.quadraticCurveTo(42, 55, 44, 41);
  ctx.closePath();
  ctx.fillStyle = BLUE;
  ctx.fill();

  ctx.beginPath();
  ctx.moveTo(34, 22);
  ctx.quadraticCurveTo(44, 10, 59, 10);
  ctx.quadraticCurveTo(53, 18, 49, 22);
  ctx.closePath();
  ctx.fillStyle = RED;
  ctx.fill();

  // Beak & eye
  ctx.beginPath();
  ctx.moveTo(32, 24);
  ctx.lineTo(18, 27.5);
  ctx.lineTo(31, 31);
  ctx.closePath();
  ctx.fillStyle = RED;
  ctx.fill();

  ctx.beginPath();
  ctx.arc(36.7, 26, 3.2, 0, Math.PI * 2);
  ctx.fillStyle = IVORY_TOP;
  ctx.fill();
  ctx.beginPath();
  ctx.arc(36.75, 26.05, 1.65, 0, Math.PI * 2);
  ctx.fillStyle = INK;
  ctx.fill();
  ctx.restore();
}

function drawBambooTile(ctx: CanvasRenderingContext2D, rank: number): void {
  if (rank === 1) {
    drawEngravedOneBamboo(ctx);
    return;
  }

  // Variant A Bamboo: strictly zero blue; emerald jade & crimson with symmetric M/W 8-bamboo.
  const entries = STYLE_A_SPEC.bamboo[
    String(rank) as keyof typeof STYLE_A_SPEC.bamboo
  ] as readonly BambooStalkSpec[];
  const bw = rank <= 4 ? 16.2 : rank <= 6 ? 14.8 : 12.6;
  for (const entry of entries) {
    drawCarvedBambooStalk(
      ctx,
      entry.x,
      entry.y,
      entry.h,
      bw,
      resolveSpecColor(entry.color),
      entry.angle,
    );
  }
}

function drawCarvedPin(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  radius: number,
  color: string,
): void {
  ctx.save();
  ctx.beginPath();
  ctx.arc(x + 0.65, y + 0.95, radius, 0, Math.PI * 2);
  ctx.fillStyle = "rgba(255, 255, 255, 0.78)";
  ctx.fill();

  ctx.beginPath();
  ctx.arc(x, y, radius, 0, Math.PI * 2);
  ctx.fillStyle = color;
  ctx.fill();

  ctx.beginPath();
  ctx.arc(x, y, radius * 0.77, 0, Math.PI * 2);
  ctx.strokeStyle = IVORY_TOP;
  ctx.lineWidth = Math.max(1.4, radius * 0.115);
  ctx.stroke();

  // 6-petal carved rosette inside concentric disc
  ctx.fillStyle = IVORY_TOP;
  for (const deg of [0, 60, 120, 180, 240, 300]) {
    ctx.save();
    ctx.translate(x, y);
    ctx.rotate((deg * Math.PI) / 180);
    ctx.beginPath();
    if (typeof ctx.ellipse === "function") {
      ctx.ellipse(0, -radius * 0.42, radius * 0.145, radius * 0.235, 0, 0, Math.PI * 2);
    } else {
      ctx.arc(0, -radius * 0.42, radius * 0.19, 0, Math.PI * 2);
    }
    ctx.fill();
    ctx.restore();
  }

  ctx.beginPath();
  ctx.arc(x, y, radius * 0.21, 0, Math.PI * 2);
  ctx.fillStyle = color;
  ctx.fill();

  ctx.beginPath();
  ctx.arc(x, y, radius * 0.095, 0, Math.PI * 2);
  ctx.fillStyle = IVORY_TOP;
  ctx.fill();
  ctx.restore();
}

function drawEngravedOneDot(ctx: CanvasRenderingContext2D): void {
  const cx = 50;
  const cy = 56;
  const outerR = 41.0;

  ctx.save();
  ctx.beginPath();
  ctx.arc(cx, cy, outerR, 0, Math.PI * 2);
  ctx.fillStyle = BLUE;
  ctx.fill();

  // 16 perimeter beads
  ctx.fillStyle = IVORY_TOP;
  const beadOrbit = outerR * 0.855;
  const beadR = outerR * 0.056;
  for (let i = 0; i < 16; i += 1) {
    const ang = (i * Math.PI * 2) / 16;
    ctx.beginPath();
    ctx.arc(cx + Math.cos(ang) * beadOrbit, cy + Math.sin(ang) * beadOrbit, beadR, 0, Math.PI * 2);
    ctx.fill();
  }

  // Ivory ring + 12-petal jade wreath
  ctx.beginPath();
  ctx.arc(cx, cy, outerR * 0.72, 0, Math.PI * 2);
  ctx.fillStyle = IVORY_TOP;
  ctx.fill();

  const wreathR = outerR * 0.66;
  const pw = wreathR * 0.28;
  ctx.fillStyle = GREEN;
  for (let i = 0; i < 12; i += 1) {
    ctx.save();
    ctx.translate(cx, cy);
    ctx.rotate((i * Math.PI * 2) / 12);
    ctx.beginPath();
    ctx.moveTo(0, 0);
    ctx.quadraticCurveTo(-pw, -wreathR * 0.62, 0, -wreathR);
    ctx.quadraticCurveTo(pw, -wreathR * 0.62, 0, 0);
    ctx.closePath();
    ctx.fill();
    ctx.restore();
  }

  // Inner crimson medallion with 6-petal ivory rosette
  ctx.beginPath();
  ctx.arc(cx, cy, outerR * 0.41, 0, Math.PI * 2);
  ctx.fillStyle = IVORY_TOP;
  ctx.fill();

  drawCarvedPin(ctx, cx, cy, outerR * 0.36, RED);
  ctx.restore();
}

function drawDotTile(ctx: CanvasRenderingContext2D, rank: number): void {
  if (rank === 1) {
    drawEngravedOneDot(ctx);
    return;
  }

  const entries = STYLE_A_SPEC.dot[
    String(rank) as keyof typeof STYLE_A_SPEC.dot
  ] as readonly PlumPinSpec[];
  for (const entry of entries) {
    drawCarvedPin(ctx, entry.x, entry.y, entry.r, resolveSpecColor(entry.color));
  }
}

function drawBlueFramedWhiteDragon(ctx: CanvasRenderingContext2D): void {
  const outer = { x: 13.5, y: 10.0, w: 73.0, h: 92.0 };
  const inner = { x: 21.0, y: 17.5, w: 58.0, h: 77.0 };

  ctx.save();
  // Recessed ivory catch-light
  roundRectPath(ctx, outer.x + 0.7, outer.y + 1.0, outer.w, outer.h, 5.0);
  ctx.strokeStyle = "rgba(255, 255, 255, 0.82)";
  ctx.lineWidth = 4.4;
  ctx.stroke();

  // Outer solid indigo frame
  roundRectPath(ctx, outer.x, outer.y, outer.w, outer.h, 5.0);
  ctx.strokeStyle = BLUE;
  ctx.lineWidth = 4.0;
  ctx.stroke();

  // Inner fine indigo frame
  roundRectPath(ctx, inner.x, inner.y, inner.w, inner.h, 2.5);
  ctx.lineWidth = 2.0;
  ctx.stroke();

  // Classic key-fret corner squares
  const cornerSize = 11.5;
  const corners: ReadonlyArray<readonly [number, number]> = [
    [outer.x, outer.y],
    [outer.x + outer.w - cornerSize, outer.y],
    [outer.x, outer.y + outer.h - cornerSize],
    [outer.x + outer.w - cornerSize, outer.y + outer.h - cornerSize],
  ];
  for (const [cx, cy] of corners) {
    ctx.fillStyle = IVORY_MID;
    ctx.fillRect(cx, cy, cornerSize, cornerSize);
    ctx.strokeStyle = BLUE;
    ctx.lineWidth = 2.4;
    if (typeof ctx.strokeRect === "function") {
      ctx.strokeRect(cx, cy, cornerSize, cornerSize);
    } else {
      ctx.beginPath();
      ctx.rect(cx, cy, cornerSize, cornerSize);
      ctx.stroke();
    }
    ctx.fillStyle = BLUE;
    ctx.fillRect(cx + 3.6, cy + 3.6, cornerSize - 7.2, cornerSize - 7.2);
  }

  // Mid-edge traditional fret bars
  ctx.fillStyle = BLUE;
  ctx.fillRect(42, outer.y + 1.5, 16, 5.5);
  ctx.fillRect(42, outer.y + outer.h - 7.0, 16, 5.5);
  ctx.fillRect(outer.x + 1.5, 47, 5.5, 18);
  ctx.fillRect(outer.x + outer.w - 7.0, 47, 5.5, 18);
  ctx.restore();
}

function drawHonorTile(ctx: CanvasRenderingContext2D, tile: Tile): void {
  const dragon = getDragonKind(tile);
  if (dragon === "White") {
    drawBlueFramedWhiteDragon(ctx);
    return;
  }

  const color = dragon === "Red" ? RED : dragon === "Green" ? GREEN : INK;
  drawEngravedGlyph(
    ctx,
    getTileLabel(tile),
    50,
    56,
    STYLE_A_TYPOGRAPHY.honorSize,
    color,
    1.02,
    1.04,
  );
}

function drawTileArtwork(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  tile: Tile,
  laid: boolean,
): void {
  drawTileBase(ctx, ox, oy, laid ? "laid" : "standing");

  ctx.save();
  ctx.translate(ox, oy);
  ctx.scale(FRAME_W / 100, FRAME_H / 135);

  const face = faceRectFor(laid);
  roundRectPath(ctx, face.x, face.y, face.w, face.h, CORNER_RADIUS - 0.6);
  ctx.clip();

  const insetX = 4.2;
  const insetY = 5.0;
  const targetW = face.w - insetX * 2;
  const targetH = face.h - insetY * 2;
  const uniformScale = Math.min(targetW / 100, targetH / 112);
  const artW = 100 * uniformScale;
  const artH = 112 * uniformScale;
  const artX = face.x + face.w * 0.5 - artW * 0.5;
  const artY = face.y + face.h * 0.5 - artH * 0.5;

  ctx.translate(artX, artY);
  ctx.scale(uniformScale, uniformScale);

  const suit = getTileSuit(tile);
  const rank = getTileRank(tile);
  switch (suit) {
    case "Man":
      drawManTile(ctx, tile);
      break;
    case "Bamboo":
      drawBambooTile(ctx, rank ?? 1);
      break;
    case "Dot":
      drawDotTile(ctx, rank ?? 1);
      break;
    case "Wind":
    case "Dragon":
      drawHonorTile(ctx, tile);
      break;
  }

  ctx.restore();
}

function createFallbackFeltTexture(
  size: number,
  fill: string | null,
  label: string,
): { texture: Texture; source: CanvasSource } {
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d");
  if (ctx) {
    if (fill) {
      ctx.fillStyle = fill;
      ctx.fillRect(0, 0, size, size);
    }
    ctx.fillStyle = "rgba(255, 240, 210, 0.04)";
    for (let i = 0; i < 64; i += 1) {
      const x = (i * 53) % size;
      const y = (i * 97) % size;
      ctx.fillRect(x, y, 28, 4);
    }
  }
  const source = new CanvasSource({
    resource: canvas,
    resolution: 2,
    autoGenerateMipmaps: true,
    label,
  });
  return {
    texture: new Texture({ source }),
    source,
  };
}

/**
 * Generates a deterministic procedural canvas `TileAtlas` (`1920x2048 <= 2048x2048`) as a fallback
 * for headless/canvas-only environments when `@2x` PNG images cannot be loaded.
 */
export function createTileAtlas(): TileAtlas {
  const canvas = document.createElement("canvas");
  canvas.width = ATLAS_W;
  canvas.height = ATLAS_H;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    throw new Error("2D canvas context unavailable for tile atlas");
  }

  const slotCoords = (slot: number) => {
    const col = slot % GRID_COLS;
    const row = Math.floor(slot / GRID_COLS);
    return {
      x: col * CELL_W + FRAME_PAD,
      y: row * CELL_H + FRAME_PAD,
    };
  };

  for (let t = 0; t < ATLAS_TILE_FRAME_COUNT; t += 1) {
    const standingPos = slotCoords(t);
    drawTileArtwork(ctx, standingPos.x, standingPos.y, t, false);
    const laidPos = slotCoords(34 + t);
    drawTileArtwork(ctx, laidPos.x, laidPos.y, t, true);
  }

  const bodyKinds = [
    "standing",
    "laid",
    "back",
    "shadow",
    "laid_back",
  ] as const;
  bodyKinds.forEach((kind, idx) => {
    const { x, y } = slotCoords(68 + idx);
    drawTileBase(ctx, x, y, kind);
  });

  const source = new CanvasSource({
    resource: canvas,
    resolution: 2,
    autoGenerateMipmaps: true,
    label: "majjj-tile-atlas-fallback",
  });

  const frameAt = (slot: number) => {
    const { x, y } = slotCoords(slot);
    return new Texture({
      source,
      frame: new Rectangle(x / 2, y / 2, FRAME_W / 2, FRAME_H / 2),
    });
  };

  const standingTextures: Texture[] = [];
  const laidTextures: Texture[] = [];
  for (let t = 0; t < ATLAS_TILE_FRAME_COUNT; t += 1) {
    standingTextures.push(frameAt(t));
    laidTextures.push(frameAt(34 + t));
  }

  const backTexture = frameAt(70);
  const shadowTexture = frameAt(71);
  const laidBackTexture = frameAt(72);

  const impasto = createFallbackFeltTexture(
    512,
    "#14282E",
    "majjj-felt-impasto-fallback",
  );
  const brush = createFallbackFeltTexture(
    256,
    null,
    "majjj-felt-brush-fallback",
  );

  return {
    width: ATLAS_W,
    height: ATLAS_H,
    backTexture,
    laidBackTexture,
    shadowTexture,
    feltImpastoTexture: impasto.texture,
    feltBrushTexture: brush.texture,
    getTileTexture(tile: Tile, body: TileBodyKind = "standing"): Texture {
      if (body === "laid") {
        return laidTextures[tile] ?? laidBackTexture;
      }
      return standingTextures[tile] ?? backTexture;
    },
    destroy(): void {
      for (const tex of standingTextures) {
        tex.destroy(false);
      }
      for (const tex of laidTextures) {
        tex.destroy(false);
      }
      backTexture.destroy(false);
      laidBackTexture.destroy(false);
      shadowTexture.destroy(false);
      source.destroy();
      impasto.texture.destroy(false);
      impasto.source.destroy();
      brush.texture.destroy(false);
      brush.source.destroy();
    },
  };
}
