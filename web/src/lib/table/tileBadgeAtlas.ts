import atlasMetaJson from "../../assets/atlas/tiles@2x.json";
import type { Tile } from "../protocol/generated/Tile";
import {
  ATLAS_REQUIRED_BODY_FRAMES,
  ATLAS_TILE_FRAME_COUNT,
} from "./atlasConstants";
import { getTileLabel } from "./tiles";

export type TileBadgeSize = "xs" | "sm" | "md";

export interface TileBadgeDimensions {
  readonly width: number;
  readonly height: number;
}

export interface AtlasFrameRect {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
}

export interface TileBadgeProjection {
  readonly frameKey: string;
  readonly ariaLabel: string;
  readonly faceUp: boolean;
  readonly width: number;
  readonly height: number;
  readonly atlasWidth: number;
  readonly atlasHeight: number;
  readonly frame: AtlasFrameRect;
  readonly scaleX: number;
  readonly scaleY: number;
  readonly backgroundWidth: number;
  readonly backgroundHeight: number;
  readonly offsetX: number;
  readonly offsetY: number;
  readonly backgroundSize: string;
  readonly backgroundPosition: string;
}

interface AtlasFrameEntry {
  readonly frame: AtlasFrameRect;
}

interface AtlasMetaShape {
  readonly frames: Record<string, AtlasFrameEntry>;
  readonly meta: {
    readonly size: {
      readonly w: number;
      readonly h: number;
    };
  };
}

const LAID_BACK_FRAME_KEY = ATLAS_REQUIRED_BODY_FRAMES[4];

/**
 * Display sizes matching the `184:248` laid-tile frame aspect ratio:
 * keeps `ActionBar` `xs` compact while enlarging `SettlementModal` and
 * `MatchSummaryView` (`sm` / `md`) per ADR-0013.
 */
export const TILE_BADGE_DIMENSIONS: Record<TileBadgeSize, TileBadgeDimensions> = {
  xs: { width: 22, height: 30 },
  sm: { width: 30, height: 41 },
  md: { width: 38, height: 52 },
};

const ATLAS_DATA = atlasMetaJson as unknown as AtlasMetaShape;

function formatCssPx(value: number): string {
  const rounded = Number(value.toFixed(4));
  return `${Object.is(rounded, -0) ? 0 : rounded}px`;
}

function isValidTileIndex(tile: Tile | null): tile is Tile {
  return (
    tile !== null &&
    Number.isInteger(tile) &&
    tile >= 0 &&
    tile < ATLAS_TILE_FRAME_COUNT
  );
}

/**
 * Projects a `(tile, faceUp, size)` tuple onto the single `@2x` traditional tile
 * atlas (`tiles@2x.json`), returning exact laid-tile or laid-back frame crop
 * metrics and a glossary-compliant Chinese `ariaLabel`.
 */
export function projectTileBadgeAtlas(
  tile: Tile | null,
  faceUp: boolean,
  size: TileBadgeSize,
): TileBadgeProjection {
  const showFaceUp = faceUp && isValidTileIndex(tile);
  const frameKey = showFaceUp ? `tile_laid_${tile}` : LAID_BACK_FRAME_KEY;
  const ariaLabel = showFaceUp ? getTileLabel(tile) : "扣牌";

  const dims = TILE_BADGE_DIMENSIONS[size];
  const atlasWidth = ATLAS_DATA.meta.size.w;
  const atlasHeight = ATLAS_DATA.meta.size.h;
  const frameEntry =
    ATLAS_DATA.frames[frameKey] ?? ATLAS_DATA.frames[LAID_BACK_FRAME_KEY];
  const frame = frameEntry.frame;

  const scaleX = dims.width / frame.w;
  const scaleY = dims.height / frame.h;
  const backgroundWidth = atlasWidth * scaleX;
  const backgroundHeight = atlasHeight * scaleY;
  const offsetX = frame.x * scaleX;
  const offsetY = frame.y * scaleY;

  return {
    frameKey,
    ariaLabel,
    faceUp: showFaceUp,
    width: dims.width,
    height: dims.height,
    atlasWidth,
    atlasHeight,
    frame: { ...frame },
    scaleX,
    scaleY,
    backgroundWidth,
    backgroundHeight,
    offsetX,
    offsetY,
    backgroundSize: `${formatCssPx(backgroundWidth)} ${formatCssPx(backgroundHeight)}`,
    backgroundPosition: `${formatCssPx(-offsetX)} ${formatCssPx(-offsetY)}`,
  };
}
