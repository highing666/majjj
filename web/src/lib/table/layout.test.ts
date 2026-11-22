import { describe, expect, it } from "vitest";
import { computeTableLayout } from "./layout";

function rectsOverlap(
  a: { x: number; y: number; width: number; height: number },
  b: { x: number; y: number; width: number; height: number },
): boolean {
  return (
    a.x < b.x + b.width &&
    a.x + a.width > b.x &&
    a.y < b.y + b.height &&
    a.y + a.height > b.y
  );
}

describe("computeTableLayout", () => {
  it.each([
    { name: "desktop", width: 1040, height: 660 },
    { name: "portrait phone", width: 375, height: 560 },
    { name: "landscape phone", width: 760, height: 340 },
  ])(
    "produces positive dimensions and non-overlapping core zones on $name ($width x $height)",
    ({ width, height }) => {
      const layout = computeTableLayout(width, height);

      expect(layout.isWide).toBe(width >= 640);
      expect(layout.margin).toBe(width >= 640 ? 20 : 10);
      expect(layout.topMargin).toBe(height >= 500 ? 14 : 8);
      expect(layout.handTileW).toBeGreaterThanOrEqual(20);
      expect(layout.handTileH).toBeGreaterThan(layout.handTileW);
      expect(layout.riverTileW).toBeGreaterThanOrEqual(14);
      expect(layout.riverTileH).toBeGreaterThan(layout.riverTileW);

      // Center hub must stay inside canvas bounds
      expect(layout.hub.x).toBeGreaterThanOrEqual(0);
      expect(layout.hub.y).toBeGreaterThanOrEqual(0);
      expect(layout.hub.x + layout.hub.width).toBeLessThanOrEqual(width);
      expect(layout.hub.y + layout.hub.height).toBeLessThanOrEqual(height);

      // Viewer 14-tile hand fits horizontally inside canvas
      const fullHandWidth =
        13 * layout.handTileW +
        12 * layout.handGap +
        layout.drawnGap +
        layout.handTileW;
      expect(fullHandWidth).toBeLessThanOrEqual(width);
      expect(layout.bottomHandY + layout.handTileH).toBeLessThanOrEqual(height);

      // Rivers must not overlap center hub
      expect(rectsOverlap(layout.hub, layout.rivers.bottom)).toBe(false);
      expect(rectsOverlap(layout.hub, layout.rivers.top)).toBe(false);
      expect(rectsOverlap(layout.hub, layout.rivers.left)).toBe(false);
      expect(rectsOverlap(layout.hub, layout.rivers.right)).toBe(false);

      // Bottom river must stay above bottom hand
      expect(layout.rivers.bottom.y + layout.rivers.bottom.height).toBeLessThanOrEqual(
        layout.bottomHandY,
      );
    },
  );
});
