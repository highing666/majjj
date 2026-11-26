import { describe, expect, it } from "vitest";
import { computeBanterBubbleAnchor } from "./banterLayout";
import { computeTableLayout, type Rect } from "./layout";
import type { RelativeSlot } from "./viewModel";

function rectsOverlap(a: Rect, b: Rect): boolean {
  return (
    a.x < b.x + b.width &&
    a.x + a.width > b.x &&
    a.y < b.y + b.height &&
    a.y + a.height > b.y
  );
}

describe("banterLayout — comic speech bubble anchors", () => {
  const viewports: Array<{ name: string; width: number; height: number }> = [
    { name: "desktop-1440x900", width: 1440, height: 900 },
    { name: "ipad-landscape-1024x768", width: 1024, height: 768 },
    { name: "iphone-landscape-844x390", width: 844, height: 390 },
    { name: "compact-landscape-568x320", width: 568, height: 320 },
    { name: "portrait-390x844", width: 390, height: 844 },
  ];

  const slots: RelativeSlot[] = ["top", "left", "right", "bottom"];

  it("keeps all bubble anchors inside viewport bounds and strictly clear of viewerHandRect", () => {
    for (const vp of viewports) {
      const layout = computeTableLayout(vp.width, vp.height);
      for (const slot of slots) {
        const anchor = computeBanterBubbleAnchor(layout, slot);
        const bubbleRect: Rect = {
          x: anchor.leftPx,
          y: anchor.topPx,
          width: anchor.maxWidthPx,
          height: anchor.maxHeightPx,
        };
        expect(bubbleRect.x, `${vp.name}/${slot} left`).toBeGreaterThanOrEqual(0);
        expect(bubbleRect.y, `${vp.name}/${slot} top`).toBeGreaterThanOrEqual(0);
        expect(
          bubbleRect.x + bubbleRect.width,
          `${vp.name}/${slot} right`,
        ).toBeLessThanOrEqual(vp.width);
        expect(
          bubbleRect.y + bubbleRect.height,
          `${vp.name}/${slot} bottom`,
        ).toBeLessThanOrEqual(vp.height);
        expect(
          rectsOverlap(bubbleRect, layout.viewerHandRect),
          `${vp.name}/${slot} must never cover viewerHandRect`,
        ).toBe(false);
      }
    }
  });
});
