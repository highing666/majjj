import fs from "node:fs";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  ATLAS_REQUIRED_BODY_FRAMES,
  ATLAS_TILE_FRAME_COUNT,
  loadTileAtlas,
} from "./tileAtlas";

function readPngDimensions(filePath: string): { width: number; height: number } {
  const buf = fs.readFileSync(filePath);
  // PNG signature (8 bytes) + IHDR length (4) + "IHDR" (4) + width (4 BE) + height (4 BE)
  const pngSig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  expect(buf.subarray(0, 8).equals(pngSig)).toBe(true);
  expect(buf.subarray(12, 16).toString("ascii")).toBe("IHDR");
  return {
    width: buf.readUInt32BE(16),
    height: buf.readUInt32BE(20),
  };
}

function collectFiles(dir: string): string[] {
  if (!fs.existsSync(dir)) {
    return [];
  }
  const out: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === "node_modules") {
      continue;
    }
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...collectFiles(full));
    } else {
      out.push(full);
    }
  }
  return out;
}

describe("@2x vector-export tile atlas and felt textures", () => {
  const webRoot = path.resolve(import.meta.dirname, "../../..");
  const atlasDir = path.join(webRoot, "src/assets/atlas");
  const texturesDir = path.join(webRoot, "src/assets/textures");
  const assetsSrcDir = path.join(webRoot, "assets-src");

  it("contains zero @3x files anywhere in web/src, web/assets-src, or web/dist", () => {
    const files = [
      ...collectFiles(path.join(webRoot, "src")),
      ...collectFiles(assetsSrcDir),
      ...collectFiles(path.join(webRoot, "dist")),
    ];
    const threeX = files.filter((f) => /@3x/i.test(path.basename(f)));
    expect(threeX).toEqual([]);
  });

  it("ships deterministic Variant A thick-jade SVG sources in web/assets-src without external watercolor PNGs", () => {
    const illustrationsDir = path.join(assetsSrcDir, "illustrations");
    expect(fs.existsSync(path.join(illustrationsDir, "art-a-1sou.png"))).toBe(false);
    expect(fs.existsSync(path.join(illustrationsDir, "art-a-1pin.png"))).toBe(false);

    for (let t = 0; t < 34; t += 1) {
      for (const prefix of ["tile", "tile_laid"] as const) {
        const svgPath = path.join(assetsSrcDir, "tiles", `${prefix}_${t}.svg`);
        expect(fs.existsSync(svgPath)).toBe(true);
        const svg = fs.readFileSync(svgPath, "utf8");
        expect(svg).toContain("<svg");
        expect(svg).toContain('viewBox="0 0 100 135"');
        // No external watercolor PNG dependency
        expect(svg).not.toContain("<image");
        expect(svg).not.toContain("art-a-1sou.png");
        expect(svg).not.toContain("art-a-1pin.png");
        // No heavy 2.6px flat black cartoon border
        expect(svg).not.toContain('stroke-width="2.6"');

        // Man (0..8) and Honors (27..32): WeibeiTC-Bold primary calligraphy
        if (t < 9 || (t >= 27 && t <= 32)) {
          expect(svg).toContain("WeibeiTC-Bold");
        }

        // Bamboo (9..17): 1-sou deterministic vector, 2..9 bamboo strictly zero blue/cobalt
        if (t >= 9 && t <= 17) {
          if (t === 9) {
            expect(svg).toContain("<path");
          } else {
            expect(svg.toUpperCase()).not.toContain("#144692");
            expect(svg.toUpperCase()).not.toContain("#1B4E8D");
            expect(svg.toUpperCase()).not.toContain("#1C5296");
          }
        }

        // Dots (18..26): 1-pin deterministic 12-petal medallion, 2..9 use 6-petal floral discs
        if (t >= 18 && t <= 26) {
          expect(svg).toContain("<circle");
          if (t === 18) {
            expect(svg).toContain("<path");
          } else {
            expect(svg).toContain("<ellipse");
            expect(svg).not.toMatch(/<line\b/);
          }
        }

        // White dragon (33): classic indigo double-line key-fret frame (#144692), no dashed stroke
        if (t === 33) {
          expect(svg).not.toContain("stroke-dasharray");
          expect(svg.toUpperCase()).toContain("#144692");
        }
      }
    }
    for (const body of ["standing", "laid", "back", "shadow", "laid_back"]) {
      const bodyPath = path.join(assetsSrcDir, "bodies", `body_${body}.svg`);
      expect(fs.existsSync(bodyPath)).toBe(true);
      const bodySvg = fs.readFileSync(bodyPath, "utf8");
      expect(bodySvg).toContain('viewBox="0 0 100 135"');
      expect(bodySvg).not.toContain('stroke-width="2.6"');
    }
  });

  it("produces a single @2x spritesheet at 1920x2048 (<= 2048x2048) with 73 non-overlapping 184x248 frames", () => {
    const pngPath = path.join(atlasDir, "tiles@2x.png");
    const jsonPath = path.join(atlasDir, "tiles@2x.json");

    expect(fs.existsSync(pngPath)).toBe(true);
    expect(fs.existsSync(jsonPath)).toBe(true);

    const pngDims = readPngDimensions(pngPath);
    expect(pngDims).toEqual({ width: 1920, height: 2048 });
    expect(pngDims.width).toBeLessThanOrEqual(2048);
    expect(pngDims.height).toBeLessThanOrEqual(2048);

    const metaJson = JSON.parse(fs.readFileSync(jsonPath, "utf8")) as {
      meta: {
        image: string;
        scale: string;
        size: { w: number; h: number };
      };
      frames: Record<
        string,
        {
          frame: { x: number; y: number; w: number; h: number };
          sourceSize: { w: number; h: number };
        }
      >;
    };

    expect(metaJson.meta.image).toBe("tiles@2x.png");
    expect(metaJson.meta.scale).toBe("2");
    expect(metaJson.meta.size).toEqual({ w: 1920, h: 2048 });

    const seenRects: Array<{ x: number; y: number; w: number; h: number }> = [];
    for (let t = 0; t < ATLAS_TILE_FRAME_COUNT; t += 1) {
      for (const prefix of ["tile", "tile_laid"] as const) {
        const entry = metaJson.frames[`${prefix}_${t}`];
        expect(entry).toBeDefined();
        expect(entry.frame.w).toBe(184);
        expect(entry.frame.h).toBe(248);
        expect(entry.sourceSize).toEqual({ w: 184, h: 248 });
        expect(entry.frame.x).toBeGreaterThanOrEqual(0);
        expect(entry.frame.y).toBeGreaterThanOrEqual(0);
        expect(entry.frame.x + entry.frame.w).toBeLessThanOrEqual(pngDims.width);
        expect(entry.frame.y + entry.frame.h).toBeLessThanOrEqual(pngDims.height);
        seenRects.push(entry.frame);
      }
    }

    for (const bodyKey of ATLAS_REQUIRED_BODY_FRAMES) {
      const entry = metaJson.frames[bodyKey];
      expect(entry).toBeDefined();
      expect(entry.frame.w).toBe(184);
      expect(entry.frame.h).toBe(248);
      expect(entry.sourceSize).toEqual({ w: 184, h: 248 });
      expect(entry.frame.x + entry.frame.w).toBeLessThanOrEqual(pngDims.width);
      expect(entry.frame.y + entry.frame.h).toBeLessThanOrEqual(pngDims.height);
      seenRects.push(entry.frame);
    }

    expect(seenRects).toHaveLength(73);

    // Verify no two spritesheet frames overlap
    for (let i = 0; i < seenRects.length; i += 1) {
      for (let j = i + 1; j < seenRects.length; j += 1) {
        const a = seenRects[i];
        const b = seenRects[j];
        const overlaps =
          a.x < b.x + b.w &&
          a.x + a.w > b.x &&
          a.y < b.y + b.h &&
          a.y + a.h > b.y;
        expect(overlaps).toBe(false);
      }
    }
  });

  it("produces seamless Piltover felt-impasto@2x.png (512x512) and felt-brush@2x.png (256x256)", () => {
    const impastoPath = path.join(texturesDir, "felt-impasto@2x.png");
    const brushPath = path.join(texturesDir, "felt-brush@2x.png");

    expect(fs.existsSync(impastoPath)).toBe(true);
    expect(fs.existsSync(brushPath)).toBe(true);

    expect(readPngDimensions(impastoPath)).toEqual({ width: 512, height: 512 });
    expect(readPngDimensions(brushPath)).toEqual({ width: 256, height: 256 });
  });

  describe("loadTileAtlas runtime fallback", () => {
    const prevDocument = globalThis.document;

    afterEach(() => {
      if (prevDocument === undefined) {
        delete (globalThis as { document?: Document }).document;
      } else {
        globalThis.document = prevDocument;
      }
    });

    it("falls back cleanly to deterministic Variant A dual-body canvas atlas with blue-frame White Dragon and destroys all textures", async () => {
      const noop = () => {};
      const recordedFonts: string[] = [];
      const recordedStrokes: string[] = [];
      const recordedFills: string[] = [];
      let ellipseCount = 0;
      let strokeTextCount = 0;
      let lineDashCount = 0;

      const fakeGradient = {
        addColorStop: noop,
      };

      const fakeCtx = {
        save: noop,
        restore: noop,
        beginPath: noop,
        closePath: noop,
        roundRect: noop,
        rect: noop,
        arc: noop,
        ellipse: () => {
          ellipseCount += 1;
        },
        moveTo: noop,
        lineTo: noop,
        quadraticCurveTo: noop,
        bezierCurveTo: noop,
        translate: noop,
        scale: noop,
        rotate: noop,
        createLinearGradient: () => fakeGradient,
        createRadialGradient: () => fakeGradient,
        fill: () => {
          recordedFills.push(String(fakeCtx.fillStyle));
        },
        stroke: () => {
          recordedStrokes.push(String(fakeCtx.strokeStyle));
        },
        clip: noop,
        fillRect: () => {
          recordedFills.push(String(fakeCtx.fillStyle));
        },
        strokeRect: () => {
          recordedStrokes.push(String(fakeCtx.strokeStyle));
        },
        fillText: () => {
          recordedFonts.push(String(fakeCtx.font));
        },
        strokeText: () => {
          strokeTextCount += 1;
        },
        setLineDash: () => {
          lineDashCount += 1;
        },
        fillStyle: "",
        strokeStyle: "",
        lineWidth: 1,
        globalAlpha: 1,
        lineCap: "butt",
        lineJoin: "miter",
        font: "",
        textAlign: "center",
        textBaseline: "middle",
      };
      (globalThis as unknown as { document: unknown }).document = {
        createElement: () => ({
          width: 0,
          height: 0,
          getContext: () => fakeCtx,
        }),
      };

      const atlas = await loadTileAtlas();
      expect(atlas.width).toBe(1920);
      expect(atlas.height).toBe(2048);
      expect(atlas.width).toBeLessThanOrEqual(2048);
      expect(atlas.height).toBeLessThanOrEqual(2048);
      expect(atlas.backTexture).toBeDefined();
      expect(atlas.laidBackTexture).toBeDefined();
      expect(atlas.laidBackTexture).not.toBe(atlas.backTexture);
      expect(atlas.shadowTexture).toBeDefined();
      expect(atlas.feltImpastoTexture).toBeDefined();
      expect(atlas.feltBrushTexture).toBeDefined();

      const standingSeen = new Set(
        Array.from({ length: 34 }, (_, t) => atlas.getTileTexture(t, "standing")),
      );
      const laidSeen = new Set(
        Array.from({ length: 34 }, (_, t) => atlas.getTileTexture(t, "laid")),
      );
      expect(standingSeen.size).toBe(34);
      expect(laidSeen.size).toBe(34);
      for (let t = 0; t < 34; t += 1) {
        expect(atlas.getTileTexture(t)).toBe(atlas.getTileTexture(t, "standing"));
        expect(atlas.getTileTexture(t, "laid")).not.toBe(
          atlas.getTileTexture(t, "standing"),
        );
      }

      // Variant A fallback invariants: WeibeiTC-Bold primary calligraphy, stroke-thickened carved glyphs,
      // 6-petal carved discs (ellipses), solid indigo (#144692) double-line White Dragon (zero dashes),
      // and zero legacy cobalt #1C5296.
      expect(recordedFonts.length).toBeGreaterThan(0);
      expect(recordedFonts.every((f) => f.includes("WeibeiTC-Bold"))).toBe(true);
      expect(strokeTextCount).toBeGreaterThan(0);
      expect(lineDashCount).toBe(0);
      expect(ellipseCount).toBeGreaterThan(0);
      expect(recordedStrokes.map((c) => c.toUpperCase())).toContain("#144692");
      expect(recordedFills.map((c) => c.toUpperCase())).not.toContain("#1C5296");
      expect(recordedStrokes.map((c) => c.toUpperCase())).not.toContain("#1C5296");

      expect(() => atlas.destroy()).not.toThrow();
    });
  });
});
