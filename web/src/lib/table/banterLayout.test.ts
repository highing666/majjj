import { describe, expect, it } from "vitest";
import {
  computeBanterBubbleAnchor,
  computeQuickEmoteDockRect,
  isCompactQuickEmoteDock,
  rectToPercentStyle,
} from "./banterLayout";
import {
  computeTableLayout,
  MELD_GROUP_GAP,
  MELD_TILE_GAP,
  type Rect,
  type TableLayoutOptions,
} from "./layout";
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
    { name: "mbp14-full-1512x982", width: 1512, height: 982 },
    { name: "mbp14-browser-1512x860", width: 1512, height: 860 },
    { name: "mba13-full-1470x956", width: 1470, height: 956 },
    { name: "mba13-browser-1470x835", width: 1470, height: 835 },
    { name: "desktop-1440x900", width: 1440, height: 900 },
    { name: "ipad-landscape-1024x768", width: 1024, height: 768 },
    { name: "iphone17-full-874x402", width: 874, height: 402 },
    { name: "iphone17-safari-874x335", width: 874, height: 335 },
    { name: "iphone13-landscape-844x390", width: 844, height: 390 },
    { name: "iphone13-safari-844x325", width: 844, height: 325 },
    { name: "compact-landscape-568x320", width: 568, height: 320 },
    { name: "portrait-390x844", width: 390, height: 844 },
  ];

  const meldStates: Array<{ label: string; options: Required<TableLayoutOptions> }> = [
    { label: "0m", options: { bottomMeldTileCount: 0, bottomMeldGroupCount: 0, viewerClosedCount: 13, viewerHasDrawn: true } },
    { label: "2m-kongs", options: { bottomMeldTileCount: 8, bottomMeldGroupCount: 2, viewerClosedCount: 7, viewerHasDrawn: true } },
    { label: "4m-kongs", options: { bottomMeldTileCount: 16, bottomMeldGroupCount: 4, viewerClosedCount: 1, viewerHasDrawn: true } },
  ];

  const slots: RelativeSlot[] = ["top", "left", "right", "bottom"];

  it("keeps all bubble anchors inside viewport bounds and strictly clear of viewerHandRect", () => {
    for (const vp of viewports) {
      for (const state of meldStates) {
        const layout = computeTableLayout(vp.width, vp.height, state.options);
        for (const slot of slots) {
          const anchor = computeBanterBubbleAnchor(layout, slot);
          const bubbleRect: Rect = {
            x: anchor.leftPx,
            y: anchor.topPx,
            width: anchor.maxWidthPx,
            height: anchor.maxHeightPx,
          };
          expect(bubbleRect.x, `${vp.name}/${state.label}/${slot} left`).toBeGreaterThanOrEqual(0);
          expect(bubbleRect.y, `${vp.name}/${state.label}/${slot} top`).toBeGreaterThanOrEqual(0);
          expect(
            bubbleRect.x + bubbleRect.width,
            `${vp.name}/${state.label}/${slot} right`,
          ).toBeLessThanOrEqual(vp.width);
          expect(
            bubbleRect.y + bubbleRect.height,
            `${vp.name}/${state.label}/${slot} bottom`,
          ).toBeLessThanOrEqual(vp.height);
          expect(
            rectsOverlap(bubbleRect, layout.viewerHandRect),
            `${vp.name}/${state.label}/${slot} must never cover viewerHandRect`,
          ).toBe(false);
        }
      }
    }
  });

  it("keeps QuickEmoteBar dock inside viewport bounds and strictly clear of hand, action bar, bottom badge, and bottom melds", () => {
    for (const vp of viewports) {
      for (const state of meldStates) {
        const layout = computeTableLayout(vp.width, vp.height, state.options);
        const dock = computeQuickEmoteDockRect(layout);
        expect(dock.x, `${vp.name}/${state.label} left`).toBeGreaterThanOrEqual(0);
        expect(dock.y, `${vp.name}/${state.label} top`).toBeGreaterThanOrEqual(0);
        expect(dock.x + dock.width, `${vp.name}/${state.label} right`).toBeLessThanOrEqual(
          vp.width,
        );
        expect(dock.y + dock.height, `${vp.name}/${state.label} bottom`).toBeLessThanOrEqual(
          vp.height,
        );
        expect(
          rectsOverlap(dock, layout.viewerHandRect),
          `${vp.name}/${state.label} quick-emote dock must never overlap viewerHandRect`,
        ).toBe(false);
        expect(
          rectsOverlap(dock, layout.turnBarRect),
          `${vp.name}/${state.label} quick-emote dock must never overlap turnBarRect`,
        ).toBe(false);
        expect(
          rectsOverlap(dock, layout.badges.bottom),
          `${vp.name}/${state.label} quick-emote dock must never overlap bottom badge`,
        ).toBe(false);
        if (state.options.bottomMeldTileCount > 0) {
          const { bottomMeldTileCount: t, bottomMeldGroupCount: g } = state.options;
          const meldW =
            t * layout.meldTileW +
            Math.max(0, t - g) * MELD_TILE_GAP +
            Math.max(0, g - 1) * MELD_GROUP_GAP;
          const bottomMeldRect: Rect = {
            x: layout.melds.bottom.x - meldW,
            y: layout.melds.bottom.y,
            width: meldW,
            height: layout.meldTileH,
          };
          expect(
            rectsOverlap(dock, bottomMeldRect),
            `${vp.name}/${state.label} quick-emote dock must never overlap bottomMelds`,
          ).toBe(false);
        }
      }
    }
  });

  it("single-sources compact quick-emote breakpoint and percentage CSS rect styles", () => {
    const desktop = computeTableLayout(1440, 900);
    const phoneLandscape = computeTableLayout(844, 390);
    const portrait = computeTableLayout(390, 844);

    expect(isCompactQuickEmoteDock(desktop)).toBe(false);
    expect(isCompactQuickEmoteDock(phoneLandscape)).toBe(true);
    expect(isCompactQuickEmoteDock(portrait)).toBe(true);

    const styleWithHeight = rectToPercentStyle(desktop, {
      x: 144,
      y: 90,
      width: 288,
      height: 180,
    });
    expect(styleWithHeight).toBe(
      "left: 10.000%; top: 10.000%; width: 20.000%; height: 20.000%;",
    );

    const styleWithoutHeight = rectToPercentStyle(
      desktop,
      { x: 144, y: 90, width: 288, height: 180 },
      false,
    );
    expect(styleWithoutHeight).toBe(
      "left: 10.000%; top: 10.000%; width: 20.000%;",
    );
  });
});
