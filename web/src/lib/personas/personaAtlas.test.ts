import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  PERSONA_IDS,
  PERSONA_MOODS,
  personaFrameKey,
} from "../protocol/personas";
import { projectPersonaAvatar } from "./personaAtlas";

function readPngDimensions(filePath: string): { width: number; height: number } {
  const buf = fs.readFileSync(filePath);
  const pngSig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  expect(buf.subarray(0, 8).equals(pngSig)).toBe(true);
  expect(buf.subarray(12, 16).toString("ascii")).toBe("IHDR");
  return {
    width: buf.readUInt32BE(16),
    height: buf.readUInt32BE(20),
  };
}

describe("@2x comic impasto persona avatar atlas", () => {
  const webRoot = path.resolve(import.meta.dirname, "../../..");
  const atlasDir = path.join(webRoot, "src/assets/atlas");
  const personasSrcDir = path.join(webRoot, "assets-src/personas");

  it("ships a single @2x spritesheet <= 2048x2048 with all 30 non-overlapping persona x mood frames", () => {
    const pngPath = path.join(atlasDir, "personas@2x.png");
    const jsonPath = path.join(atlasDir, "personas@2x.json");

    expect(fs.existsSync(pngPath)).toBe(true);
    expect(fs.existsSync(jsonPath)).toBe(true);

    const pngDims = readPngDimensions(pngPath);
    expect(pngDims.width).toBeGreaterThan(0);
    expect(pngDims.height).toBeGreaterThan(0);
    expect(pngDims.width).toBeLessThanOrEqual(2048);
    expect(pngDims.height).toBeLessThanOrEqual(2048);

    const sheet = JSON.parse(fs.readFileSync(jsonPath, "utf8")) as {
      meta: {
        image: string;
        scale: string;
        size: { w: number; h: number };
      };
      frames: Record<
        string,
        {
          frame: { x: number; y: number; w: number; h: number };
        }
      >;
    };

    expect(sheet.meta.image).toBe("personas@2x.png");
    expect(sheet.meta.scale).toBe("2");
    expect(sheet.meta.size.w).toBe(pngDims.width);
    expect(sheet.meta.size.h).toBe(pngDims.height);

    const seenRects: Array<{ x: number; y: number; w: number; h: number }> = [];
    for (const personaId of PERSONA_IDS) {
      for (const mood of PERSONA_MOODS) {
        const key = personaFrameKey(personaId, mood);
        const svgPath = path.join(personasSrcDir, `${key}.svg`);
        expect(fs.existsSync(svgPath)).toBe(true);

        const entry = sheet.frames[key];
        expect(entry).toBeDefined();
        expect(entry.frame.w).toBeGreaterThanOrEqual(128);
        expect(entry.frame.h).toBeGreaterThanOrEqual(128);
        expect(entry.frame.x).toBeGreaterThanOrEqual(0);
        expect(entry.frame.y).toBeGreaterThanOrEqual(0);
        expect(entry.frame.x + entry.frame.w).toBeLessThanOrEqual(pngDims.width);
        expect(entry.frame.y + entry.frame.h).toBeLessThanOrEqual(pngDims.height);
        seenRects.push(entry.frame);
      }
    }

    expect(seenRects).toHaveLength(30);

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

  it("projects any (personaId, mood, size) to exact CSS background crop coordinates", () => {
    const projected = projectPersonaAvatar("QuickHothead", "Tilted", 48);
    expect(projected.frameKey).toBe("persona_QuickHothead_Tilted");
    expect(projected.ariaLabel).toBe("大川 · 红温破防");
    expect(projected.size).toBe(48);
    expect(projected.atlasWidth).toBeLessThanOrEqual(2048);
    expect(projected.atlasHeight).toBeLessThanOrEqual(2048);
    expect(projected.backgroundSize).toContain("px");
    expect(projected.backgroundPosition).toContain("px");

    const defaultMoodProj = projectPersonaAvatar("TeaSarcasm", null, 36);
    expect(defaultMoodProj.frameKey).toBe("persona_TeaSarcasm_Mocking");
  });
});
