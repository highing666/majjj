import styleASpecJson from "./styleASpec.json";

export interface SpecAlphaColor {
  readonly hex: string;
  readonly alpha: number;
}

export type SpecColorKey = "jade" | "crimson" | "cobalt";

export interface BambooStalkSpec {
  readonly x: number;
  readonly y: number;
  readonly h: number;
  readonly color: SpecColorKey;
  readonly angle: number;
}

export interface PlumPinSpec {
  readonly x: number;
  readonly y: number;
  readonly r: number;
  readonly color: SpecColorKey;
}

export const STYLE_A_SPEC = styleASpecJson;
export const ATLAS_GRID = STYLE_A_SPEC.grid;
export const ATLAS_TILE_FRAME_COUNT = ATLAS_GRID.tileCount;
export const ATLAS_REQUIRED_BODY_FRAMES = ATLAS_GRID.bodyFrames;
export const STYLE_A_GEOMETRY = STYLE_A_SPEC.geometry;
export const STYLE_A_TYPOGRAPHY = STYLE_A_SPEC.typography;
export const STYLE_A_PALETTE = STYLE_A_SPEC.palette;

export function toRgbaCss(color: SpecAlphaColor): string {
  const clean = color.hex.replace("#", "");
  const r = Number.parseInt(clean.slice(0, 2), 16);
  const g = Number.parseInt(clean.slice(2, 4), 16);
  const b = Number.parseInt(clean.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${color.alpha})`;
}
