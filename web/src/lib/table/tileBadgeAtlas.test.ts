import { describe, expect, it } from "vitest";
import atlasMeta from "../../assets/atlas/tiles@2x.json";
import { getTileLabel } from "./tiles";
import {
  projectTileBadgeAtlas,
  TILE_BADGE_DIMENSIONS,
  type TileBadgeSize,
} from "./tileBadgeAtlas";

const ALL_SIZES: TileBadgeSize[] = ["xs", "sm", "md"];

describe("projectTileBadgeAtlas", () => {
  it("matches expected badge proportions and worked CSS crop values for tile_laid_0 (一万)", () => {
    // Independent worked example from 1920x2048 tiles@2x.json (10x8 grid, 192x256 cell, pad 4, 184x248 frame):
    // tile_laid_0 is slot 34 (col=4, row=3) -> frame = { x: 772, y: 772, w: 184, h: 248 }
    // At xs (22x30): scaleX = 22/184, scaleY = 30/248
    const xs = projectTileBadgeAtlas(0, true, "xs");
    expect(xs.width).toBe(22);
    expect(xs.height).toBe(30);
    expect(xs.frameKey).toBe("tile_laid_0");
    expect(xs.ariaLabel).toBe("一万");
    expect(xs.atlasWidth).toBe(1920);
    expect(xs.atlasHeight).toBe(2048);
    expect(xs.frame).toEqual({ x: 772, y: 772, w: 184, h: 248 });
    expect(xs.backgroundWidth).toBeCloseTo((1920 * 22) / 184, 4);
    expect(xs.backgroundHeight).toBeCloseTo((2048 * 30) / 248, 4);
    expect(xs.offsetX).toBeCloseTo((772 * 22) / 184, 4);
    expect(xs.offsetY).toBeCloseTo((772 * 30) / 248, 4);

    const sm = projectTileBadgeAtlas(0, true, "sm");
    expect(sm.width).toBe(30);
    expect(sm.height).toBe(41);
    expect(sm.backgroundWidth).toBeCloseTo((1920 * 30) / 184, 4);
    expect(sm.backgroundHeight).toBeCloseTo((2048 * 41) / 248, 4);

    const md = projectTileBadgeAtlas(0, true, "md");
    expect(md.width).toBe(38);
    expect(md.height).toBe(52);
    expect(md.backgroundWidth).toBeCloseTo((1920 * 38) / 184, 4);
    expect(md.backgroundHeight).toBeCloseTo((2048 * 52) / 248, 4);
  });

  it("projects all 34 face-up tiles across xs/sm/md to in-bounds tile_laid frames with CSS sprite metrics and glossary labels", () => {
    const atlasW = atlasMeta.meta.size.w;
    const atlasH = atlasMeta.meta.size.h;
    const frames = atlasMeta.frames as Record<
      string,
      { frame: { x: number; y: number; w: number; h: number } }
    >;

    for (const size of ALL_SIZES) {
      const expectedDims = TILE_BADGE_DIMENSIONS[size];
      expect(expectedDims.width).toBeGreaterThan(0);
      expect(expectedDims.height).toBeGreaterThan(0);

      for (let tile = 0; tile < 34; tile += 1) {
        const projection = projectTileBadgeAtlas(tile, true, size);
        const expectedFrameKey = `tile_laid_${tile}`;
        const atlasFrame = frames[expectedFrameKey]?.frame;

        expect(atlasFrame).toBeDefined();
        expect(projection.faceUp).toBe(true);
        expect(projection.frameKey).toBe(expectedFrameKey);
        expect(projection.ariaLabel).toBe(getTileLabel(tile));
        expect(projection.width).toBe(expectedDims.width);
        expect(projection.height).toBe(expectedDims.height);
        expect(projection.atlasWidth).toBe(atlasW);
        expect(projection.atlasHeight).toBe(atlasH);

        // In-bounds frame coordinates
        expect(projection.frame).toEqual(atlasFrame);
        expect(projection.frame.x).toBeGreaterThanOrEqual(0);
        expect(projection.frame.y).toBeGreaterThanOrEqual(0);
        expect(projection.frame.x + projection.frame.w).toBeLessThanOrEqual(atlasW);
        expect(projection.frame.y + projection.frame.h).toBeLessThanOrEqual(atlasH);

        // Sprite scale and crop offsets stay strictly within scaled atlas bounds
        expect(projection.scaleX).toBeCloseTo(expectedDims.width / atlasFrame.w, 6);
        expect(projection.scaleY).toBeCloseTo(expectedDims.height / atlasFrame.h, 6);
        expect(projection.backgroundWidth).toBeCloseTo(atlasW * projection.scaleX, 4);
        expect(projection.backgroundHeight).toBeCloseTo(atlasH * projection.scaleY, 4);
        expect(projection.offsetX).toBeGreaterThanOrEqual(0);
        expect(projection.offsetY).toBeGreaterThanOrEqual(0);
        expect(projection.offsetX + projection.width).toBeLessThanOrEqual(
          projection.backgroundWidth + 0.001,
        );
        expect(projection.offsetY + projection.height).toBeLessThanOrEqual(
          projection.backgroundHeight + 0.001,
        );

        expect(projection.backgroundSize).toMatch(
          /^\d+(\.\d+)?px \d+(\.\d+)?px$/,
        );
        expect(projection.backgroundPosition).toMatch(
          /^-?\d+(\.\d+)?px -?\d+(\.\d+)?px$/,
        );
      }
    }
  });

  it("projects face-down, null, and out-of-range tiles to body_laid_back with ariaLabel 扣牌", () => {
    const atlasW = atlasMeta.meta.size.w;
    const atlasH = atlasMeta.meta.size.h;
    const backFrame = atlasMeta.frames.body_laid_back.frame;

    const cases: Array<[number | null, boolean]> = [
      [0, false],
      [33, false],
      [null, true],
      [null, false],
      [-1, true],
      [34, true],
      [1.5, true],
    ];

    for (const size of ALL_SIZES) {
      for (const [tile, faceUp] of cases) {
        const projection = projectTileBadgeAtlas(tile, faceUp, size);
        expect(projection.faceUp).toBe(false);
        expect(projection.frameKey).toBe("body_laid_back");
        expect(projection.ariaLabel).toBe("扣牌");
        expect(projection.frame).toEqual(backFrame);
        expect(projection.offsetX).toBeCloseTo(
          (backFrame.x * projection.width) / backFrame.w,
          4,
        );
        expect(projection.offsetY).toBeCloseTo(
          (backFrame.y * projection.height) / backFrame.h,
          4,
        );
        expect(projection.offsetX + projection.width).toBeLessThanOrEqual(
          (atlasW * projection.width) / backFrame.w + 0.001,
        );
        expect(projection.offsetY + projection.height).toBeLessThanOrEqual(
          (atlasH * projection.height) / backFrame.h + 0.001,
        );
      }
    }
  });
});
