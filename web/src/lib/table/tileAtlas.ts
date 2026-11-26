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
  getDragonKind,
  getTileLabel,
  getTileNumeral,
  getTileRank,
  getTileSuit,
} from "./tiles";

export type TileBodyKind = "standing" | "laid";

export const ATLAS_TILE_FRAME_COUNT = 34;
export const ATLAS_REQUIRED_BODY_FRAMES = [
  "body_standing",
  "body_laid",
  "body_back",
  "body_shadow",
  "body_laid_back",
] as const;

const CELL_W = 144;
const CELL_H = 192;
const FRAME_W = 136;
const FRAME_H = 184;
const FRAME_PAD = 4;
const GRID_COLS = 10;
const GRID_ROWS = 8;
const ATLAS_W = CELL_W * GRID_COLS; // 1440 <= 2048
const ATLAS_H = CELL_H * GRID_ROWS; // 1536 <= 2048

const IVORY = "#F6EFE0";
const IVORY_HIGHLIGHT = "#FFFDF8";
const IVORY_BOTTOM_LIP = "#E2D4B7";
const JADE_BACK = "#186258";
const JADE_BACK_DARK = "#0E433C";
const JADE_BORDER = "#091D1B";
const JADE_RIM = "rgba(167, 243, 208, 0.42)";
const GOLD_DOT = "rgba(212, 168, 87, 0.55)";
const WHITE_BEVEL = "rgba(128, 114, 88, 0.22)";
const INK = "#1C1917";
const RED = "#B92822";
const GREEN = "#137449";
const BLUE = "#1B4E8D";
const KAITI_FONT_STACK = '"Kaiti SC", "STKaiti", "BiauKai", "Songti SC", serif';

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
      throw new Error("Missing required Style A body frame in tiles@2x.json");
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

function roundRectPath(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
}

function drawTileBase(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  kind: "standing" | "laid" | "back" | "shadow" | "laid_back",
): void {
  const x = ox + 3;
  const w = FRAME_W - 6;
  const radius = 14;

  if (kind === "shadow") {
    ctx.save();
    roundRectPath(ctx, x + 2, oy + 6, w - 4, FRAME_H - 10, radius);
    ctx.fillStyle = "rgba(8, 13, 18, 0.42)";
    ctx.fill();
    ctx.restore();
    return;
  }

  const isBack = kind === "back" || kind === "laid_back";
  const isLaid = kind === "laid" || kind === "laid_back";
  const y = isLaid ? oy + 3 : oy + 10;
  const h = isLaid ? FRAME_H - 6 : FRAME_H - 14;

  ctx.save();
  if (!isLaid) {
    roundRectPath(ctx, x, oy + 2, w, FRAME_H - 4, radius + 1);
    ctx.fillStyle = isBack ? JADE_BACK_DARK : JADE_BACK;
    ctx.fill();
    ctx.strokeStyle = JADE_BORDER;
    ctx.lineWidth = 2.4;
    ctx.stroke();
  }

  roundRectPath(ctx, x, y, w, h, radius);
  ctx.fillStyle = isBack ? JADE_BACK : IVORY;
  ctx.fill();

  if (!isBack) {
    ctx.save();
    ctx.clip();
    ctx.fillStyle = IVORY_BOTTOM_LIP;
    ctx.fillRect(x, y + h - (isLaid ? 9 : 10), w, isLaid ? 9 : 10);
    ctx.fillStyle = IVORY_HIGHLIGHT;
    ctx.fillRect(x, y, w, 7);
    ctx.restore();
  } else {
    roundRectPath(ctx, x + 13, y + 13, w - 26, h - 26, 9);
    ctx.strokeStyle = JADE_RIM;
    ctx.lineWidth = 2.8;
    ctx.stroke();

    const cx = ox + FRAME_W / 2;
    const cy = y + h / 2;
    ctx.beginPath();
    ctx.moveTo(cx, cy - 28);
    ctx.lineTo(cx + 19, cy);
    ctx.lineTo(cx, cy + 28);
    ctx.lineTo(cx - 19, cy);
    ctx.closePath();
    ctx.strokeStyle = JADE_RIM;
    ctx.lineWidth = 2.5;
    ctx.stroke();

    ctx.beginPath();
    ctx.arc(cx, cy, 6, 0, Math.PI * 2);
    ctx.fillStyle = GOLD_DOT;
    ctx.fill();
  }

  roundRectPath(ctx, x, y, w, h, radius);
  ctx.strokeStyle = isBack ? JADE_BORDER : INK;
  ctx.lineWidth = 3.2;
  ctx.stroke();
  ctx.restore();
}

function drawManTile(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  tile: Tile,
  laid: boolean,
): void {
  const cx = ox + FRAME_W / 2;
  const topOffset = laid ? 0 : 5;
  ctx.save();
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";

  ctx.fillStyle = INK;
  ctx.font = `900 54px ${KAITI_FONT_STACK}`;
  ctx.fillText(getTileNumeral(tile), cx, oy + 54 + topOffset);

  ctx.fillStyle = RED;
  ctx.font = `900 58px ${KAITI_FONT_STACK}`;
  ctx.fillText("萬", cx, oy + 124 + topOffset * 0.5);
  ctx.restore();
}

function drawBambooStalk(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  color: string,
  angleDeg = 0,
): void {
  ctx.save();
  if (angleDeg !== 0 && typeof ctx.translate === "function" && typeof ctx.rotate === "function") {
    ctx.translate(x, y);
    ctx.rotate((angleDeg * Math.PI) / 180);
    x = 0;
    y = 0;
  }
  ctx.fillStyle = color;
  ctx.strokeStyle = INK;
  ctx.lineWidth = 1.5;

  ctx.beginPath();
  ctx.roundRect(x - w / 2, y - h / 2, w, h, w / 2);
  ctx.fill();
  ctx.stroke();

  const nodeW = w * 1.18;
  for (const ny of [y - h / 2, y - 2, y + h / 2 - 4]) {
    ctx.beginPath();
    ctx.roundRect(x - nodeW / 2, ny, nodeW, 5, 2.5);
    ctx.fill();
    ctx.stroke();
  }
  ctx.restore();
}

function drawBambooTile(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  rank: number,
  laid: boolean,
): void {
  const cx = ox + FRAME_W / 2;
  const cy = oy + FRAME_H / 2 + (laid ? 0 : 4);

  if (rank === 1) {
    ctx.save();
    ctx.strokeStyle = GREEN;
    ctx.lineWidth = 4;
    ctx.beginPath();
    ctx.arc(cx, cy, 32, 0, Math.PI * 2);
    ctx.stroke();

    drawBambooStalk(ctx, cx, cy, 14, 58, GREEN);
    ctx.fillStyle = RED;
    ctx.beginPath();
    ctx.arc(cx, cy - 12, 8, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = INK;
    ctx.lineWidth = 1.6;
    ctx.stroke();
    ctx.restore();
    return;
  }

  // Style A Bamboo: ZERO blue! Traditional jade green & crimson only, symmetric M/W 8-bamboo.
  const layouts: Record<number, Array<[number, number, number, string, number?]>> = {
    2: [
      [0, -34, 48, GREEN],
      [0, 34, 48, GREEN],
    ],
    3: [
      [0, -34, 46, GREEN],
      [-26, 34, 46, GREEN],
      [26, 34, 46, GREEN],
    ],
    4: [
      [-24, -34, 46, GREEN],
      [24, -34, 46, GREEN],
      [-24, 34, 46, GREEN],
      [24, 34, 46, GREEN],
    ],
    5: [
      [-30, -36, 42, GREEN],
      [30, -36, 42, GREEN],
      [0, 0, 42, RED],
      [-30, 36, 42, GREEN],
      [30, 36, 42, GREEN],
    ],
    6: [
      [-30, -34, 46, GREEN],
      [0, -34, 46, GREEN],
      [30, -34, 46, GREEN],
      [-30, 34, 46, GREEN],
      [0, 34, 46, GREEN],
      [30, 34, 46, GREEN],
    ],
    7: [
      [0, -48, 36, RED],
      [-30, 0, 36, GREEN],
      [0, 0, 36, GREEN],
      [30, 0, 36, GREEN],
      [-30, 48, 36, GREEN],
      [0, 48, 36, GREEN],
      [30, 48, 36, GREEN],
    ],
    8: [
      [-38, -34, 42, GREEN],
      [-14, -32, 40, GREEN, 28],
      [14, -32, 40, GREEN, -28],
      [38, -34, 42, GREEN],
      [-38, 34, 42, GREEN],
      [-14, 32, 40, GREEN, -28],
      [14, 32, 40, GREEN, 28],
      [38, 34, 42, GREEN],
    ],
    9: [
      [-32, -46, 34, GREEN],
      [0, -46, 34, RED],
      [32, -46, 34, GREEN],
      [-32, 0, 34, GREEN],
      [0, 0, 34, RED],
      [32, 0, 34, GREEN],
      [-32, 46, 34, GREEN],
      [0, 46, 34, RED],
      [32, 46, 34, GREEN],
    ],
  };

  const stalkW = rank >= 7 ? 12 : 14;
  for (const [dx, dy, h, color, angle] of layouts[rank]) {
    drawBambooStalk(ctx, cx + dx, cy + dy, stalkW, h, color, angle ?? 0);
  }
}

function drawPlumPip(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  radius: number,
  color: string,
): void {
  ctx.save();
  ctx.beginPath();
  ctx.arc(x, y, radius, 0, Math.PI * 2);
  ctx.fillStyle = color;
  ctx.fill();
  ctx.strokeStyle = INK;
  ctx.lineWidth = 1.8;
  ctx.stroke();

  ctx.beginPath();
  ctx.arc(x, y, radius * 0.74, 0, Math.PI * 2);
  ctx.strokeStyle = IVORY;
  ctx.lineWidth = Math.max(1.4, radius * 0.1);
  ctx.stroke();

  // 5-petal plum blossom inside coin disc (zero radial gear ticks)
  ctx.fillStyle = IVORY;
  for (const deg of [0, 72, 144, 216, 288]) {
    ctx.save();
    if (typeof ctx.translate === "function" && typeof ctx.rotate === "function") {
      ctx.translate(x, y);
      ctx.rotate((deg * Math.PI) / 180);
      ctx.beginPath();
      if (typeof ctx.ellipse === "function") {
        ctx.ellipse(0, -radius * 0.37, radius * 0.17, radius * 0.25, 0, 0, Math.PI * 2);
      } else {
        ctx.arc(0, -radius * 0.37, radius * 0.2, 0, Math.PI * 2);
      }
      ctx.fill();
    }
    ctx.restore();
  }

  ctx.beginPath();
  ctx.arc(x, y, radius * 0.19, 0, Math.PI * 2);
  ctx.fillStyle = color;
  ctx.fill();

  ctx.beginPath();
  ctx.arc(x, y, radius * 0.09, 0, Math.PI * 2);
  ctx.fillStyle = IVORY;
  ctx.fill();
  ctx.restore();
}

function drawDotTile(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  rank: number,
  laid: boolean,
): void {
  const cx = ox + FRAME_W / 2;
  const cy = oy + FRAME_H / 2 + (laid ? 0 : 4);

  if (rank === 1) {
    drawPlumPip(ctx, cx, cy, 40, BLUE);
    drawPlumPip(ctx, cx, cy, 20, RED);
    return;
  }

  const layouts: Record<number, Array<[number, number, number, string]>> = {
    2: [
      [0, -34, 21, GREEN],
      [0, 34, 21, BLUE],
    ],
    3: [
      [-30, -42, 17, BLUE],
      [0, 0, 17, RED],
      [30, 42, 17, GREEN],
    ],
    4: [
      [-26, -32, 18, BLUE],
      [26, -32, 18, GREEN],
      [-26, 32, 18, GREEN],
      [26, 32, 18, BLUE],
    ],
    5: [
      [-30, -40, 16, BLUE],
      [30, -40, 16, GREEN],
      [0, 0, 16.5, RED],
      [-30, 40, 16, GREEN],
      [30, 40, 16, BLUE],
    ],
    6: [
      [-24, -44, 16, GREEN],
      [24, -44, 16, GREEN],
      [-24, 0, 16, RED],
      [24, 0, 16, RED],
      [-24, 44, 16, RED],
      [24, 44, 16, RED],
    ],
    7: [
      [-32, -52, 14, GREEN],
      [0, -38, 14, GREEN],
      [32, -24, 14, GREEN],
      [-25, 11, 15, RED],
      [25, 11, 15, RED],
      [-25, 48, 15, RED],
      [25, 48, 15, RED],
    ],
    8: [
      [-25, -52, 14.5, BLUE],
      [25, -52, 14.5, BLUE],
      [-25, -17, 14.5, BLUE],
      [25, -17, 14.5, BLUE],
      [-25, 17, 14.5, BLUE],
      [25, 17, 14.5, BLUE],
      [-25, 52, 14.5, BLUE],
      [25, 52, 14.5, BLUE],
    ],
    9: [
      [-34, -48, 14, GREEN],
      [0, -48, 14, GREEN],
      [34, -48, 14, GREEN],
      [-34, 0, 14, RED],
      [0, 0, 14, RED],
      [34, 0, 14, RED],
      [-34, 48, 14, BLUE],
      [0, 48, 14, BLUE],
      [34, 48, 14, BLUE],
    ],
  };

  for (const [dx, dy, r, color] of layouts[rank]) {
    drawPlumPip(ctx, cx + dx, cy + dy, r, color);
  }
}

function drawHonorTile(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  tile: Tile,
  laid: boolean,
): void {
  const cx = ox + FRAME_W / 2;
  const cy = oy + FRAME_H / 2 + (laid ? 0 : 4);
  ctx.save();

  const dragon = getDragonKind(tile);
  if (dragon === "White") {
    // Mainland plain white dragon: faint recessed inner bevel only (zero blue dashed frame)
    roundRectPath(ctx, ox + 19, oy + (laid ? 20 : 25), FRAME_W - 38, FRAME_H - 46, 9);
    ctx.strokeStyle = WHITE_BEVEL;
    ctx.lineWidth = 2;
    ctx.stroke();
    ctx.restore();
    return;
  }

  const color = dragon === "Red" ? RED : dragon === "Green" ? GREEN : INK;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillStyle = color;
  ctx.font = `900 78px ${KAITI_FONT_STACK}`;
  ctx.fillText(getTileLabel(tile), cx, cy);
  ctx.restore();
}

function drawTileArtwork(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  tile: Tile,
  laid: boolean,
): void {
  drawTileBase(ctx, ox, oy, laid ? "laid" : "standing");
  const suit = getTileSuit(tile);
  const rank = getTileRank(tile);
  switch (suit) {
    case "Man":
      drawManTile(ctx, ox, oy, tile, laid);
      break;
    case "Bamboo":
      drawBambooTile(ctx, ox, oy, rank ?? 1, laid);
      break;
    case "Dot":
      drawDotTile(ctx, ox, oy, rank ?? 1, laid);
      break;
    case "Wind":
    case "Dragon":
      drawHonorTile(ctx, ox, oy, tile, laid);
      break;
  }
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
 * Generates a deterministic procedural canvas `TileAtlas` (`<= 2048x2048`) as a fallback
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
