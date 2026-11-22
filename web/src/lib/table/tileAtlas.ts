import { CanvasSource, Rectangle, Texture } from "pixi.js";
import type { Tile } from "../protocol/generated/Tile";
import {
  getDragonKind,
  getTileLabel,
  getTileNumeral,
  getTileRank,
  getTileSuit,
} from "./tiles";

const CELL_W = 128;
const CELL_H = 176;
const GRID_COLS = 6;
const GRID_ROWS = 6;
const ATLAS_W = CELL_W * GRID_COLS; // 768 <= 2048
const ATLAS_H = CELL_H * GRID_ROWS; // 1056 <= 2048

const IVORY = "#F5EEDF";
const IVORY_HIGHLIGHT = "#FFFDF8";
const IVORY_BOTTOM_LIP = "#DDD0B6";
const JADE_BACK = "#1E5C5E";
const JADE_RIM = "#2E787A";
const INK = "#181A20";
const RED = "#B12824";
const GREEN = "#1F6E4A";
const BLUE = "#1D4F8C";

export interface TileAtlas {
  readonly width: number;
  readonly height: number;
  readonly backTexture: Texture;
  getTileTexture(tile: Tile): Texture;
  destroy(): void;
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
  isBack: boolean,
): void {
  const pad = 4;
  const x = ox + pad;
  const y = oy + pad;
  const w = CELL_W - pad * 2;
  const h = CELL_H - pad * 2;
  const radius = 14;

  // Main face / back body
  ctx.save();
  roundRectPath(ctx, x, y, w, h, radius);
  ctx.fillStyle = isBack ? JADE_BACK : IVORY;
  ctx.fill();

  if (!isBack) {
    // Bottom ivory bevel lip for subtle 2D depth
    ctx.save();
    ctx.clip();
    ctx.fillStyle = IVORY_BOTTOM_LIP;
    ctx.fillRect(x, y + h - 12, w, 12);
    ctx.fillStyle = IVORY_HIGHLIGHT;
    ctx.fillRect(x, y, w, 6);
    ctx.restore();
  } else {
    // Inner engraved frame on jade back
    roundRectPath(ctx, x + 12, y + 12, w - 24, h - 24, 8);
    ctx.strokeStyle = JADE_RIM;
    ctx.lineWidth = 3;
    ctx.stroke();

    // Subtle center diamond emblem
    const cx = ox + CELL_W / 2;
    const cy = oy + CELL_H / 2;
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

  // Crisp outer ink outline
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
  const cx = ox + CELL_W / 2;
  ctx.save();
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";

  ctx.fillStyle = RED;
  ctx.font =
    '800 50px "PingFang SC", "Hiragino Sans GB", "Noto Sans CJK SC", sans-serif';
  ctx.fillText(getTileNumeral(tile), cx, oy + 54);

  ctx.fillStyle = INK;
  ctx.font =
    '800 48px "PingFang SC", "Hiragino Sans GB", "Noto Sans CJK SC", sans-serif';
  ctx.fillText("万", cx, oy + 114);
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
  ctx.lineWidth = 1.6;

  ctx.beginPath();
  ctx.roundRect(x - w / 2, y - h / 2, w, h, w / 2);
  ctx.fill();
  ctx.stroke();

  // Middle bamboo joint notch
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
  const cx = ox + CELL_W / 2;
  const cy = oy + CELL_H / 2 - 4;

  if (rank === 1) {
    // Classic 1-bamboo stylized emblem + single clear stalk
    ctx.save();
    ctx.strokeStyle = GREEN;
    ctx.fillStyle = GREEN;
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
      [0, -28, GREEN],
      [0, 28, BLUE],
    ],
    3: [
      [0, -28, GREEN],
      [-20, 28, GREEN],
      [20, 28, GREEN],
    ],
    4: [
      [-20, -28, BLUE],
      [20, -28, GREEN],
      [-20, 28, GREEN],
      [20, 28, BLUE],
    ],
    5: [
      [-24, -28, GREEN],
      [24, -28, BLUE],
      [0, 0, RED],
      [-24, 28, BLUE],
      [24, 28, GREEN],
    ],
    6: [
      [-24, -28, GREEN],
      [0, -28, GREEN],
      [24, -28, GREEN],
      [-24, 28, BLUE],
      [0, 28, BLUE],
      [24, 28, BLUE],
    ],
    7: [
      [0, -40, RED],
      [-24, 0, GREEN],
      [0, 0, BLUE],
      [24, 0, GREEN],
      [-24, 40, GREEN],
      [0, 40, BLUE],
      [24, 40, GREEN],
    ],
    8: [
      [-28, -28, GREEN],
      [0, -28, GREEN],
      [28, -28, GREEN],
      [-14, 0, RED],
      [14, 0, RED],
      [-28, 28, BLUE],
      [0, 28, BLUE],
      [28, 28, BLUE],
    ],
    9: [
      [-26, -40, RED],
      [0, -40, BLUE],
      [26, -40, GREEN],
      [-26, 0, RED],
      [0, 0, BLUE],
      [26, 0, GREEN],
      [-26, 40, RED],
      [0, 40, BLUE],
      [26, 40, GREEN],
    ],
  };

  const stalks = layouts[rank];
  const stalkH = rank >= 7 ? 32 : 42;
  const stalkW = rank >= 8 ? 11 : 13;
  for (const [dx, dy, color] of stalks) {
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

  // Inner concentric ring
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
  const cx = ox + CELL_W / 2;
  const cy = oy + CELL_H / 2 - 4;

  if (rank === 1) {
    drawPip(ctx, cx, cy, 34, GREEN);
    drawPip(ctx, cx, cy, 17, RED);
    return;
  }

  const layouts: Record<number, Array<[number, number, number, string]>> = {
    2: [
      [0, -30, 17, GREEN],
      [0, 30, 17, BLUE],
    ],
    3: [
      [-26, -34, 14, BLUE],
      [0, 0, 14, RED],
      [26, 34, 14, GREEN],
    ],
    4: [
      [-22, -26, 15, BLUE],
      [22, -26, 15, GREEN],
      [-22, 26, 15, GREEN],
      [22, 26, 15, BLUE],
    ],
    5: [
      [-26, -34, 13, BLUE],
      [26, -34, 13, GREEN],
      [0, 0, 13, RED],
      [-26, 34, 13, GREEN],
      [26, 34, 13, BLUE],
    ],
    6: [
      [-22, -38, 13, GREEN],
      [22, -38, 13, GREEN],
      [-22, 0, 13, RED],
      [22, 0, 13, RED],
      [-22, 38, 13, RED],
      [22, 38, 13, RED],
    ],
    7: [
      [-26, -42, 11, GREEN],
      [0, -32, 11, GREEN],
      [26, -22, 11, GREEN],
      [-22, 12, 12, RED],
      [22, 12, 12, RED],
      [-22, 42, 12, RED],
      [22, 42, 12, RED],
    ],
    8: [
      [-22, -45, 11, BLUE],
      [22, -45, 11, BLUE],
      [-22, -15, 11, BLUE],
      [22, -15, 11, BLUE],
      [-22, 15, 11, BLUE],
      [22, 15, 11, BLUE],
      [-22, 45, 11, BLUE],
      [22, 45, 11, BLUE],
    ],
    9: [
      [-26, -40, 11, GREEN],
      [0, -40, 11, GREEN],
      [26, -40, 11, GREEN],
      [-26, 0, 11, RED],
      [0, 0, 11, RED],
      [26, 0, 11, RED],
      [-26, 40, 11, BLUE],
      [0, 40, 11, BLUE],
      [26, 40, 11, BLUE],
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
  const cx = ox + CELL_W / 2;
  const cy = oy + CELL_H / 2 - 4;
  ctx.save();

  const dragon = getDragonKind(tile);
  if (dragon === "White") {
    // 白: crisp double cobalt rectangular frame
    roundRectPath(ctx, ox + 26, oy + 26, CELL_W - 52, CELL_H - 60, 8);
    ctx.strokeStyle = BLUE;
    ctx.lineWidth = 6;
    ctx.stroke();

    roundRectPath(ctx, ox + 36, oy + 36, CELL_W - 72, CELL_H - 80, 4);
    ctx.strokeStyle = BLUE;
    ctx.lineWidth = 2.5;
    ctx.stroke();
    ctx.restore();
    return;
  }

  const color = dragon === "Red" ? RED : dragon === "Green" ? GREEN : INK;

  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillStyle = color;
  ctx.font =
    '800 68px "PingFang SC", "Hiragino Sans GB", "Noto Sans CJK SC", sans-serif';
  ctx.fillText(getTileLabel(tile), cx, cy);
  ctx.restore();
}

function drawTileArtwork(
  ctx: CanvasRenderingContext2D,
  ox: number,
  oy: number,
  tile: Tile,
): void {
  drawTileBase(ctx, ox, oy, false);
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

/**
 * Generates a single `768x1056` (`<= 2048x2048`) spritesheet atlas containing all 34 tile faces
 * and the jade back tile, backed by one GPU `CanvasSource`.
 */
export function createTileAtlas(): TileAtlas {
  const canvas = document.createElement("canvas");
  canvas.width = ATLAS_W;
  canvas.height = ATLAS_H;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    throw new Error("2D canvas context unavailable for tile atlas");
  }

  for (let t = 0; t < 34; t += 1) {
    const col = t % GRID_COLS;
    const row = Math.floor(t / GRID_COLS);
    drawTileArtwork(ctx, col * CELL_W, row * CELL_H, t);
  }

  // Slot 34: jade back tile
  const backCol = 34 % GRID_COLS;
  const backRow = Math.floor(34 / GRID_COLS);
  drawTileBase(ctx, backCol * CELL_W, backRow * CELL_H, true);

  const source = new CanvasSource({
    resource: canvas,
    resolution: 1,
    autoGenerateMipmaps: true,
    label: "majjj-tile-atlas",
  });

  const faceTextures: Texture[] = [];
  for (let t = 0; t < 34; t += 1) {
    const col = t % GRID_COLS;
    const row = Math.floor(t / GRID_COLS);
    faceTextures.push(
      new Texture({
        source,
        frame: new Rectangle(col * CELL_W, row * CELL_H, CELL_W, CELL_H),
      }),
    );
  }

  const backTexture = new Texture({
    source,
    frame: new Rectangle(backCol * CELL_W, backRow * CELL_H, CELL_W, CELL_H),
  });

  return {
    width: ATLAS_W,
    height: ATLAS_H,
    backTexture,
    getTileTexture(tile: Tile): Texture {
      return faceTextures[tile] ?? backTexture;
    },
    destroy(): void {
      for (const tex of faceTextures) {
        tex.destroy(false);
      }
      backTexture.destroy(false);
      source.destroy();
    },
  };
}
