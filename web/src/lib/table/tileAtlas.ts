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
const GRID_COLS = 8;
const GRID_ROWS = 5;
const ATLAS_W = CELL_W * GRID_COLS; // 1152 <= 2048
const ATLAS_H = CELL_H * GRID_ROWS; // 960 <= 2048

const IVORY = "#F5EEDF";
const IVORY_HIGHLIGHT = "#FFFDF8";
const IVORY_BOTTOM_LIP = "#E4D8BE";
const JADE_BACK = "#1E5C5E";
const JADE_RIM = "#399294";
const INK = "#181A20";
const RED = "#C7352B";
const GREEN = "#157F56";
const BLUE = "#1C5296";

export interface TileAtlas {
  readonly width: number;
  readonly height: number;
  readonly backTexture: Texture;
  readonly shadowTexture: Texture;
  readonly feltImpastoTexture: Texture;
  readonly feltBrushTexture: Texture;
  getTileTexture(tile: Tile): Texture;
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

    const faceTextures: Texture[] = [];
    for (let t = 0; t < ATLAS_TILE_FRAME_COUNT; t += 1) {
      const tex = sheet.textures[`tile_${t}`];
      if (!tex) {
        throw new Error(`Missing spritesheet frame tile_${t}`);
      }
      faceTextures.push(tex);
    }

    const backTexture = sheet.textures.body_back;
    const shadowTexture = sheet.textures.body_shadow;
    if (!backTexture || !shadowTexture) {
      throw new Error("Missing required impasto body frame in tiles@2x.json");
    }

    const metaSize = sheetData.meta.size ?? { w: ATLAS_W, h: ATLAS_H };

    return {
      width: metaSize.w,
      height: metaSize.h,
      backTexture,
      shadowTexture,
      feltImpastoTexture,
      feltBrushTexture,
      getTileTexture(tile: Tile): Texture {
        return faceTextures[tile] ?? backTexture;
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
  const x = ox + 4;
  const y = oy + 4;
  const w = FRAME_W - 8;
  const h = FRAME_H - 8;
  const radius = 14;

  if (kind === "shadow") {
    ctx.save();
    roundRectPath(ctx, x + 2, y + 4, w - 4, h - 6, radius);
    ctx.fillStyle = "rgba(8, 13, 18, 0.42)";
    ctx.fill();
    ctx.restore();
    return;
  }

  const isBack = kind === "back" || kind === "laid_back";
  const isLaid = kind === "laid" || kind === "laid_back";

  ctx.save();
  roundRectPath(ctx, x, y, w, h, radius);
  ctx.fillStyle = isBack ? JADE_BACK : IVORY;
  ctx.fill();

  if (!isBack) {
    ctx.save();
    ctx.clip();
    ctx.fillStyle = IVORY_BOTTOM_LIP;
    ctx.fillRect(x, y + h - (isLaid ? 8 : 12), w, isLaid ? 8 : 12);
    ctx.fillStyle = IVORY_HIGHLIGHT;
    ctx.fillRect(x, y, w, 6);
    ctx.restore();
  } else {
    roundRectPath(ctx, x + 12, y + 12, w - 24, h - 24, 8);
    ctx.strokeStyle = JADE_RIM;
    ctx.lineWidth = 3;
    ctx.stroke();

    const cx = ox + FRAME_W / 2;
    const cy = oy + FRAME_H / 2;
    ctx.beginPath();
    ctx.moveTo(cx, cy - 18);
    ctx.lineTo(cx + 14, cy);
    ctx.lineTo(cx, cy + 18);
    ctx.lineTo(cx - 14, cy);
    ctx.closePath();
    ctx.strokeStyle = JADE_RIM;
    ctx.lineWidth = 2.5;
    ctx.stroke();
  }

  roundRectPath(ctx, x, y, w, h, radius);
  ctx.strokeStyle = INK;
  ctx.lineWidth = 3.5;
  ctx.stroke();
  ctx.restore();
}

function drawManTile(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  tile: Tile,
): void {
  const cx = ox + FRAME_W / 2;
  ctx.save();
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";

  ctx.fillStyle = INK;
  ctx.font = '900 50px "Songti SC", "STSong", "PingFang SC", serif';
  ctx.fillText(getTileNumeral(tile), cx, oy + 56);

  ctx.fillStyle = RED;
  ctx.font = '900 54px "Songti SC", "STSong", "PingFang SC", serif';
  ctx.fillText("萬", cx, oy + 122);
  ctx.restore();
}

function drawBambooStalk(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  color: string,
): void {
  ctx.save();
  ctx.fillStyle = color;
  ctx.strokeStyle = INK;
  ctx.lineWidth = 1.8;

  ctx.beginPath();
  ctx.roundRect(x - w / 2, y - h / 2, w, h, w / 2);
  ctx.fill();
  ctx.stroke();

  ctx.fillStyle = IVORY;
  ctx.fillRect(x - w / 2 - 1, y - 2, w + 2, 4);
  ctx.fillStyle = color;
  ctx.fillRect(x - w / 2 + 1, y - 1, w - 2, 2);
  ctx.restore();
}

function drawBambooTile(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  rank: number,
): void {
  const cx = ox + FRAME_W / 2;
  const cy = oy + FRAME_H / 2 - 4;

  if (rank === 1) {
    ctx.save();
    ctx.strokeStyle = GREEN;
    ctx.lineWidth = 4;
    ctx.beginPath();
    ctx.arc(cx, cy, 30, 0, Math.PI * 2);
    ctx.stroke();

    drawBambooStalk(ctx, cx, cy, 14, 54, GREEN);
    ctx.fillStyle = RED;
    ctx.beginPath();
    ctx.arc(cx, cy - 12, 7, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = INK;
    ctx.lineWidth = 1.8;
    ctx.stroke();
    ctx.restore();
    return;
  }

  const layouts: Record<number, Array<[number, number, string]>> = {
    2: [
      [0, -30, GREEN],
      [0, 30, BLUE],
    ],
    3: [
      [0, -30, GREEN],
      [-22, 30, GREEN],
      [22, 30, GREEN],
    ],
    4: [
      [-22, -30, BLUE],
      [22, -30, GREEN],
      [-22, 30, GREEN],
      [22, 30, BLUE],
    ],
    5: [
      [-26, -30, GREEN],
      [26, -30, BLUE],
      [0, 0, RED],
      [-26, 30, BLUE],
      [26, 30, GREEN],
    ],
    6: [
      [-26, -30, GREEN],
      [0, -30, GREEN],
      [26, -30, GREEN],
      [-26, 30, BLUE],
      [0, 30, BLUE],
      [26, 30, BLUE],
    ],
    7: [
      [0, -42, RED],
      [-26, 0, GREEN],
      [0, 0, BLUE],
      [26, 0, GREEN],
      [-26, 42, GREEN],
      [0, 42, BLUE],
      [26, 42, GREEN],
    ],
    8: [
      [-28, -30, GREEN],
      [0, -30, GREEN],
      [28, -30, GREEN],
      [-14, 0, RED],
      [14, 0, RED],
      [-28, 30, BLUE],
      [0, 30, BLUE],
      [28, 30, BLUE],
    ],
    9: [
      [-28, -42, RED],
      [0, -42, BLUE],
      [28, -42, GREEN],
      [-28, 0, RED],
      [0, 0, BLUE],
      [28, 0, GREEN],
      [-28, 42, RED],
      [0, 42, BLUE],
      [28, 42, GREEN],
    ],
  };

  const stalkH = rank >= 7 ? 34 : 44;
  const stalkW = rank >= 8 ? 12 : 14;
  for (const [dx, dy, color] of layouts[rank]) {
    drawBambooStalk(ctx, cx + dx, cy + dy, stalkW, stalkH, color);
  }
}

function drawPip(
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
  ctx.lineWidth = 2;
  ctx.stroke();

  ctx.beginPath();
  ctx.arc(x, y, radius * 0.56, 0, Math.PI * 2);
  ctx.strokeStyle = IVORY;
  ctx.lineWidth = 2.2;
  ctx.stroke();

  ctx.beginPath();
  ctx.arc(x, y, radius * 0.24, 0, Math.PI * 2);
  ctx.fillStyle = IVORY;
  ctx.fill();
  ctx.restore();
}

function drawDotTile(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  rank: number,
): void {
  const cx = ox + FRAME_W / 2;
  const cy = oy + FRAME_H / 2 - 4;

  if (rank === 1) {
    drawPip(ctx, cx, cy, 36, BLUE);
    drawPip(ctx, cx, cy, 18, RED);
    return;
  }

  const layouts: Record<number, Array<[number, number, number, string]>> = {
    2: [
      [0, -32, 18, GREEN],
      [0, 32, 18, BLUE],
    ],
    3: [
      [-28, -36, 15, BLUE],
      [0, 0, 15, RED],
      [28, 36, 15, GREEN],
    ],
    4: [
      [-24, -28, 16, BLUE],
      [24, -28, 16, GREEN],
      [-24, 28, 16, GREEN],
      [24, 28, 16, BLUE],
    ],
    5: [
      [-28, -36, 14, BLUE],
      [28, -36, 14, GREEN],
      [0, 0, 14, RED],
      [-28, 36, 14, GREEN],
      [28, 36, 14, BLUE],
    ],
    6: [
      [-24, -40, 14, GREEN],
      [24, -40, 14, GREEN],
      [-24, 0, 14, RED],
      [24, 0, 14, RED],
      [-24, 40, 14, RED],
      [24, 40, 14, RED],
    ],
    7: [
      [-28, -44, 12, GREEN],
      [0, -34, 12, GREEN],
      [28, -24, 12, GREEN],
      [-24, 12, 13, RED],
      [24, 12, 13, RED],
      [-24, 44, 13, RED],
      [24, 44, 13, RED],
    ],
    8: [
      [-24, -46, 12, BLUE],
      [24, -46, 12, BLUE],
      [-24, -15, 12, BLUE],
      [24, -15, 12, BLUE],
      [-24, 15, 12, BLUE],
      [24, 15, 12, BLUE],
      [-24, 46, 12, BLUE],
      [24, 46, 12, BLUE],
    ],
    9: [
      [-28, -42, 12, GREEN],
      [0, -42, 12, GREEN],
      [28, -42, 12, GREEN],
      [-28, 0, 12, RED],
      [0, 0, 12, RED],
      [28, 0, 12, RED],
      [-28, 42, 12, BLUE],
      [0, 42, 12, BLUE],
      [28, 42, 12, BLUE],
    ],
  };

  for (const [dx, dy, r, color] of layouts[rank]) {
    drawPip(ctx, cx + dx, cy + dy, r, color);
  }
}

function drawHonorTile(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  tile: Tile,
): void {
  const cx = ox + FRAME_W / 2;
  const cy = oy + FRAME_H / 2 - 4;
  ctx.save();

  const dragon = getDragonKind(tile);
  if (dragon === "White") {
    roundRectPath(ctx, ox + 28, oy + 28, FRAME_W - 56, FRAME_H - 62, 8);
    ctx.strokeStyle = BLUE;
    ctx.lineWidth = 6;
    ctx.stroke();

    roundRectPath(ctx, ox + 38, oy + 38, FRAME_W - 76, FRAME_H - 82, 4);
    ctx.strokeStyle = INK;
    ctx.lineWidth = 2;
    ctx.stroke();
    ctx.restore();
    return;
  }

  const color = dragon === "Red" ? RED : dragon === "Green" ? GREEN : INK;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillStyle = color;
  ctx.font = '900 72px "Songti SC", "STSong", "PingFang SC", serif';
  ctx.fillText(getTileLabel(tile), cx, cy);
  ctx.restore();
}

function drawTileArtwork(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  tile: Tile,
): void {
  drawTileBase(ctx, ox, oy, "standing");
  const suit = getTileSuit(tile);
  const rank = getTileRank(tile);
  switch (suit) {
    case "Man":
      drawManTile(ctx, ox, oy, tile);
      break;
    case "Bamboo":
      drawBambooTile(ctx, ox, oy, rank ?? 1);
      break;
    case "Dot":
      drawDotTile(ctx, ox, oy, rank ?? 1);
      break;
    case "Wind":
    case "Dragon":
      drawHonorTile(ctx, ox, oy, tile);
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
      const x = ((i * 53) % size);
      const y = ((i * 97) % size);
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
    const { x, y } = slotCoords(t);
    drawTileArtwork(ctx, x, y, t);
  }

  const bodyKinds = [
    "standing",
    "laid",
    "back",
    "shadow",
    "laid_back",
  ] as const;
  bodyKinds.forEach((kind, idx) => {
    const { x, y } = slotCoords(34 + idx);
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

  const faceTextures: Texture[] = [];
  for (let t = 0; t < ATLAS_TILE_FRAME_COUNT; t += 1) {
    faceTextures.push(frameAt(t));
  }

  const backTexture = frameAt(36);
  const shadowTexture = frameAt(37);

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
    shadowTexture,
    feltImpastoTexture: impasto.texture,
    feltBrushTexture: brush.texture,
    getTileTexture(tile: Tile): Texture {
      return faceTextures[tile] ?? backTexture;
    },
    destroy(): void {
      for (const tex of faceTextures) {
        tex.destroy(false);
      }
      backTexture.destroy(false);
      shadowTexture.destroy(false);
      source.destroy();
      impasto.texture.destroy(false);
      impasto.source.destroy();
      brush.texture.destroy(false);
      brush.source.destroy();
    },
  };
}
