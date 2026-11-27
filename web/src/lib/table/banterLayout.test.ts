import { describe, expect, it } from "vitest";
import {
  computeBanterBubbleAnchor,
  computeQuickEmoteDockRect,
  isCompactQuickEmoteDock,
  rectToPercentStyle,
} from "./banterLayout";
import { computeTableLayout, type Rect } from "./layout";
import {
  ADR_0013_VIEWPORT_MATRIX,
  computeBottomMeldRect,
  rectsOverlap,
  VIEWER_MELD_SCENARIOS,
} from "./testFixtures";
import type { RelativeSlot } from "./viewModel";

describe("banterLayout — comic speech bubble anchors", () => {
  const slots: RelativeSlot[] = ["top", "left", "right", "bottom"];

  it("keeps all bubble anchors inside viewport bounds and strictly clear of viewerHandRect", () => {
    for (const vp of ADR_0013_VIEWPORT_MATRIX) {
      for (const state of VIEWER_MELD_SCENARIOS) {
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
    for (const vp of ADR_0013_VIEWPORT_MATRIX) {
      for (const state of VIEWER_MELD_SCENARIOS) {
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
        const bottomMeldRect = computeBottomMeldRect(
          layout,
          state.options.bottomMeldTileCount,
          state.options.bottomMeldGroupCount,
        );
        if (bottomMeldRect !== null) {
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
