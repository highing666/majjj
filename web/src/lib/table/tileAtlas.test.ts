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

  it("ships deterministic vector SVG sources in web/assets-src for all 34 tiles and impasto bodies", () => {
    for (let t = 0; t < 34; t += 1) {
      const svgPath = path.join(assetsSrcDir, "tiles", `tile_${t}.svg`);
      expect(fs.existsSync(svgPath)).toBe(true);
      const svg = fs.readFileSync(svgPath, "utf8");
      expect(svg).toContain("<svg");
      expect(svg).toContain('viewBox="0 0 100 136"');
    }
    for (const body of ["standing", "laid", "back", "shadow", "laid_back"]) {
      expect(
        fs.existsSync(path.join(assetsSrcDir, "bodies", `body_${body}.svg`)),
      ).toBe(true);
    }
  });

  it("produces a single @2x spritesheet <= 2048x2048 with all 34 tiles and impasto tile bodies", () => {
    const pngPath = path.join(atlasDir, "tiles@2x.png");
    const jsonPath = path.join(atlasDir, "tiles@2x.json");

    expect(fs.existsSync(pngPath)).toBe(true);
    expect(fs.existsSync(jsonPath)).toBe(true);

    const pngDims = readPngDimensions(pngPath);
    expect(pngDims.width).toBeGreaterThan(0);
    expect(pngDims.height).toBeGreaterThan(0);
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
    expect(metaJson.meta.size.w).toBe(pngDims.width);
    expect(metaJson.meta.size.h).toBe(pngDims.height);

    const seenRects: Array<{ x: number; y: number; w: number; h: number }> = [];
    for (let t = 0; t < ATLAS_TILE_FRAME_COUNT; t += 1) {
      const entry = metaJson.frames[`tile_${t}`];
      expect(entry).toBeDefined();
      expect(entry.frame.w).toBeGreaterThanOrEqual(100);
      expect(entry.frame.h).toBeGreaterThanOrEqual(136);
      expect(entry.frame.x).toBeGreaterThanOrEqual(0);
      expect(entry.frame.y).toBeGreaterThanOrEqual(0);
      expect(entry.frame.x + entry.frame.w).toBeLessThanOrEqual(pngDims.width);
      expect(entry.frame.y + entry.frame.h).toBeLessThanOrEqual(pngDims.height);
      seenRects.push(entry.frame);
    }

    for (const bodyKey of ATLAS_REQUIRED_BODY_FRAMES) {
      const entry = metaJson.frames[bodyKey];
      expect(entry).toBeDefined();
      expect(entry.frame.x + entry.frame.w).toBeLessThanOrEqual(pngDims.width);
      expect(entry.frame.y + entry.frame.h).toBeLessThanOrEqual(pngDims.height);
      seenRects.push(entry.frame);
    }

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

    it("falls back cleanly to deterministic canvas atlas and destroys all textures", async () => {
      const noop = () => {};
      const fakeCtx = {
        save: noop,
        restore: noop,
        beginPath: noop,
        closePath: noop,
        roundRect: noop,
        rect: noop,
        arc: noop,
        moveTo: noop,
        lineTo: noop,
        fill: noop,
        stroke: noop,
        clip: noop,
        fillRect: noop,
        fillText: noop,
        fillStyle: "",
        strokeStyle: "",
        lineWidth: 1,
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
      expect(atlas.width).toBeLessThanOrEqual(2048);
      expect(atlas.height).toBeLessThanOrEqual(2048);
      expect(atlas.backTexture).toBeDefined();
      expect(atlas.shadowTexture).toBeDefined();
      expect(atlas.laidBaseTexture).toBeDefined();
      expect(atlas.standingBaseTexture).toBeDefined();
      expect(atlas.feltImpastoTexture).toBeDefined();
      expect(atlas.feltBrushTexture).toBeDefined();

      const seen = new Set(
        Array.from({ length: 34 }, (_, t) => atlas.getTileTexture(t)),
      );
      expect(seen.size).toBe(34);
      expect(() => atlas.destroy()).not.toThrow();
    });
  });
});
