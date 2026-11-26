import { describe, expect, it } from "vitest";
import {
  computeBottomRiverSlotPoint,
  computeCenterSealLayout,
  computeCenterSealPointer,
  computeOpponentHandSlots,
  computeRiverSlotPoint,
  computeTableLayout,
  computeViewerHandSlots,
  type Rect,
} from "./layout";

function rectsOverlap(a: Rect, b: Rect): boolean {
  return (
    a.x < b.x + b.width &&
    a.x + a.width > b.x &&
    a.y < b.y + b.height &&
    a.y + a.height > b.y
  );
}

function expectInsideBounds(rect: Rect, width: number, height: number): void {
  expect(rect.x).toBeGreaterThanOrEqual(0);
  expect(rect.y).toBeGreaterThanOrEqual(0);
  expect(rect.x + rect.width).toBeLessThanOrEqual(width);
  expect(rect.y + rect.height).toBeLessThanOrEqual(height);
}

describe("Scheme C four-corner minimal computeTableLayout", () => {
  it.each([
    { name: "Mac wide 1440x900", width: 1440, height: 900, minHandW: 54, minRiverW: 28 },
    { name: "Mac wide 1440x860", width: 1440, height: 860, minHandW: 54, minRiverW: 28 },
    { name: "Mac laptop 1280x720", width: 1280, height: 720, minHandW: 48, minRiverW: 26 },
    { name: "desktop 1040x660", width: 1040, height: 660, minHandW: 44, minRiverW: 24 },
    { name: "desktop 1024x720", width: 1024, height: 720, minHandW: 44, minRiverW: 24 },
    { name: "iPhone portrait 390x844", width: 390, height: 844, minHandW: 24, minRiverW: 16 },
    { name: "iPhone portrait 390x780", width: 390, height: 780, minHandW: 24, minRiverW: 16 },
    { name: "compact portrait 375x560", width: 375, height: 560, minHandW: 23, minRiverW: 15 },
    { name: "SE portrait 320x568", width: 320, height: 568, minHandW: 20, minRiverW: 15 },
    { name: "iPhone landscape 844x390", width: 844, height: 390, minHandW: 34, minRiverW: 18 },
    { name: "compact landscape 760x340", width: 760, height: 340, minHandW: 29, minRiverW: 15 },
    { name: "SE landscape 568x320", width: 568, height: 320, minHandW: 28, minRiverW: 15 },
  ])(
    "produces zero-overlap, in-bounds four-corner layout on $name ($width x $height)",
    ({ width, height, minHandW, minRiverW }) => {
      const layout = computeTableLayout(width, height);

      expect(layout.isWide).toBe(width >= 640);
      expect(layout.handTileW).toBeGreaterThanOrEqual(minHandW);
      expect(layout.handTileH).toBeGreaterThan(layout.handTileW);
      expect(layout.riverTileW).toBeGreaterThanOrEqual(minRiverW);
      expect(layout.riverTileH).toBeGreaterThan(layout.riverTileW);

      // Compact center compass seal (round + wall remaining)
      expect(layout.centerSeal.width).toBeLessThanOrEqual(76);
      expect(layout.centerSeal.height).toBeLessThanOrEqual(76);
      expectInsideBounds(layout.centerSeal, width, height);

      const sealLayout = computeCenterSealLayout(layout.centerSeal);
      expect("windY" in sealLayout).toBe(false);
      expect("windFontSize" in sealLayout).toBe(false);
      expect(sealLayout.outerRadius).toBe(
        Math.min(layout.centerSeal.width, layout.centerSeal.height) / 2,
      );
      expect(sealLayout.ringInset).toBeGreaterThanOrEqual(4);
      expect(sealLayout.ringRadius).toBeGreaterThan(0);
      expect(sealLayout.ringRadius).toBeLessThan(sealLayout.outerRadius);
      expect(sealLayout.roundFontSize).toBeGreaterThan(0);
      expect(sealLayout.wallFontSize).toBeGreaterThan(0);
      expect(sealLayout.roundY - sealLayout.roundFontSize / 2).toBeGreaterThanOrEqual(
        layout.centerSeal.y,
      );
      expect(sealLayout.wallY + sealLayout.wallFontSize / 2).toBeLessThanOrEqual(
        layout.centerSeal.y + layout.centerSeal.height,
      );
      expect(sealLayout.wallY - sealLayout.roundY).toBeGreaterThanOrEqual(
        (sealLayout.roundFontSize + sealLayout.wallFontSize) * 0.45,
      );

      // Viewer 13+1 hand inside bounds
      const fullHandWidth =
        13 * layout.handTileW +
        12 * layout.handGap +
        layout.drawnGap +
        layout.handTileW;
      expect(fullHandWidth).toBeLessThanOrEqual(width);
      expectInsideBounds(layout.viewerHandRect, width, height);
      expect(layout.viewerHandRect.y).toBe(layout.bottomHandY);

      // Opponent hand bounds stay inside canvas
      expectInsideBounds(layout.oppHands.top, width, height);
      expectInsideBounds(layout.oppHands.left, width, height);
      expectInsideBounds(layout.oppHands.right, width, height);

      // Four-corner seat/score badges (TL / TR / BL / BR) sized for 2-char role + optional dealer pill
      const tl = layout.badges.top;
      const tr = layout.badges.right;
      const bl = layout.badges.left;
      const br = layout.badges.bottom;
      const expectedMinBadgeW = layout.isWide ? 140 : 114;
      for (const badge of [tl, tr, bl, br]) {
        expectInsideBounds(badge, width, height);
        expect(badge.width).toBeGreaterThanOrEqual(expectedMinBadgeW);
      }
      if (!layout.isWide && width >= 390) {
        expect(tl.width).toBeGreaterThanOrEqual(132);
      }
      expect(tl.x + tl.width / 2).toBeLessThan(width / 2);
      expect(tl.y + tl.height / 2).toBeLessThan(height / 2);
      expect(tr.x + tr.width / 2).toBeGreaterThan(width / 2);
      expect(tr.y + tr.height / 2).toBeLessThan(height / 2);
      expect(bl.x + bl.width / 2).toBeLessThan(width / 2);
      expect(bl.y + bl.height / 2).toBeGreaterThan(height / 2);
      expect(br.x + br.width / 2).toBeGreaterThan(width / 2);
      expect(br.y + br.height / 2).toBeGreaterThan(height / 2);

      // All 4 rivers inside bounds and never overlapping centerSeal or each other
      const riverRects = [
        layout.rivers.bottom,
        layout.rivers.top,
        layout.rivers.left,
        layout.rivers.right,
      ];
      for (const r of riverRects) {
        expectInsideBounds(r, width, height);
        expect(rectsOverlap(r, layout.centerSeal)).toBe(false);
        expect(rectsOverlap(r, layout.viewerHandRect)).toBe(false);
      }
      for (let i = 0; i < riverRects.length; i += 1) {
        for (let j = i + 1; j < riverRects.length; j += 1) {
          expect(rectsOverlap(riverRects[i], riverRects[j])).toBe(false);
        }
      }

      // Check pairwise non-overlap among badges, opponent hands, rivers, and viewer hand
      const coreBoxes: Rect[] = [
        layout.centerSeal,
        layout.viewerHandRect,
        layout.oppHands.top,
        layout.oppHands.left,
        layout.oppHands.right,
        tl,
        tr,
        bl,
        br,
        ...riverRects,
      ];
      for (let i = 0; i < coreBoxes.length; i += 1) {
        for (let j = i + 1; j < coreBoxes.length; j += 1) {
          expect(rectsOverlap(coreBoxes[i], coreBoxes[j])).toBe(false);
        }
      }
      // Viewer bottom meld anchor (1 group of 3 tiles) stays clear of badges, viewer hand, and bottom river
      const sampleMeldW = 3 * layout.meldTileW + 2;
      const bottomMeldBox: Rect = {
        x: layout.melds.bottom.x - sampleMeldW,
        y: layout.melds.bottom.y,
        width: sampleMeldW,
        height: layout.meldTileH,
      };
      expectInsideBounds(bottomMeldBox, width, height);
      expect(rectsOverlap(bottomMeldBox, br)).toBe(false);
      expect(rectsOverlap(bottomMeldBox, bl)).toBe(false);
      expect(rectsOverlap(bottomMeldBox, layout.viewerHandRect)).toBe(false);
      expect(rectsOverlap(bottomMeldBox, layout.rivers.bottom)).toBe(false);
    },
  );

  it("computes exact 6-per-row bottom river slot coordinates via computeBottomRiverSlotPoint", () => {
    const layout = computeTableLayout(1440, 900);
    const p0 = computeBottomRiverSlotPoint(layout, 0);
    const p1 = computeBottomRiverSlotPoint(layout, 1);
    const p5 = computeBottomRiverSlotPoint(layout, 5);
    const p6 = computeBottomRiverSlotPoint(layout, 6);
    const p12 = computeBottomRiverSlotPoint(layout, 12);

    expect(p0.x).toBe(layout.rivers.bottom.x + layout.riverTileW / 2);
    expect(p0.y).toBe(layout.rivers.bottom.y + layout.riverTileH / 2);
    expect(p1.x - p0.x).toBe(layout.riverTileW + layout.riverGap);
    expect(p1.y).toBe(p0.y);
    expect(p5.x + layout.riverTileW / 2).toBe(
      layout.rivers.bottom.x + layout.rivers.bottom.width,
    );

    // Row 1 starts back at column 0 and steps down by riverTileH + riverGap
    expect(p6.x).toBe(p0.x);
    expect(p6.y - p0.y).toBe(layout.riverTileH + layout.riverGap);

    // Row 2
    expect(p12.x).toBe(p0.x);
    expect(p12.y - p6.y).toBe(layout.riverTileH + layout.riverGap);
  });

  it("computes river slots, opponent hand slots, and viewer hand slots strictly inside their layout bounds", () => {
    const layout = computeTableLayout(1280, 720);

    // All 4 river sides stay within their river rect and have expected orientations
    const expectedRotations = {
      bottom: 0,
      top: Math.PI,
      left: Math.PI / 2,
      right: -Math.PI / 2,
    } as const;
    for (const side of ["bottom", "top", "left", "right"] as const) {
      const rect = layout.rivers[side];
      const horizontal = side === "bottom" || side === "top";
      const halfW = (horizontal ? layout.riverTileW : layout.riverTileH) / 2;
      const halfH = (horizontal ? layout.riverTileH : layout.riverTileW) / 2;
      for (const idx of [0, 5, 6, 17]) {
        const pt = computeRiverSlotPoint(layout, side, idx);
        expect(pt.rotation).toBe(expectedRotations[side]);
        expect(pt.x - halfW).toBeGreaterThanOrEqual(rect.x - 0.001);
        expect(pt.x + halfW).toBeLessThanOrEqual(rect.x + rect.width + 0.001);
        expect(pt.y - halfH).toBeGreaterThanOrEqual(rect.y - 0.001);
        expect(pt.y + halfH).toBeLessThanOrEqual(rect.y + rect.height + 0.001);
      }
    }
    expect(computeBottomRiverSlotPoint(layout, 7)).toEqual({
      x: computeRiverSlotPoint(layout, "bottom", 7).x,
      y: computeRiverSlotPoint(layout, "bottom", 7).y,
    });

    // Opponent hands (14 tiles) stay inside oppHands bounds
    for (const side of ["top", "left", "right"] as const) {
      const slots = computeOpponentHandSlots(layout, side, 14);
      expect(slots).toHaveLength(14);
      const rect = layout.oppHands[side];
      for (const s of slots) {
        const boxW = side === "top" ? s.width : s.height;
        const boxH = side === "top" ? s.height : s.width;
        expect(s.x - boxW / 2).toBeGreaterThanOrEqual(rect.x - 0.001);
        expect(s.x + boxW / 2).toBeLessThanOrEqual(rect.x + rect.width + 0.001);
        expect(s.y - boxH / 2).toBeGreaterThanOrEqual(rect.y - 0.001);
        expect(s.y + boxH / 2).toBeLessThanOrEqual(rect.y + rect.height + 0.001);
      }
    }

    // Viewer 13+1 hand slots align with viewerHandRect and preserve drawnGap
    const fullHand = computeViewerHandSlots(layout, 13, true);
    expect(fullHand.closed).toHaveLength(13);
    expect(fullHand.drawn).not.toBeNull();
    expect(fullHand.closed[0].x).toBe(layout.viewerHandRect.x);
    expect(fullHand.closed[0].y).toBe(layout.viewerHandRect.y);
    expect(fullHand.drawn!.x - (fullHand.closed[12].x + fullHand.closed[12].width)).toBe(
      layout.drawnGap,
    );
    expect(fullHand.drawn!.x + fullHand.drawn!.width).toBe(
      layout.viewerHandRect.x + layout.viewerHandRect.width,
    );

    const thirteenHand = computeViewerHandSlots(layout, 13, false);
    expect(thirteenHand.closed).toHaveLength(13);
    expect(thirteenHand.drawn).toBeNull();
  });

  it("computes four-way directional center seal pointer geometry bounded within centerSeal", () => {
    const layout = computeTableLayout(1440, 860);
    const seal = layout.centerSeal;
    const cx = seal.x + seal.width / 2;
    const cy = seal.y + seal.height / 2;

    expect(computeCenterSealPointer(seal, null)).toBeNull();

    const bottom = computeCenterSealPointer(seal, "bottom")!;
    expect(bottom).not.toBeNull();
    expect(bottom.slot).toBe("bottom");
    expect(bottom.tip.y).toBeGreaterThan(cy);
    expect(bottom.tip.y).toBeGreaterThan(bottom.baseLeft.y);
    expect(bottom.tip.y).toBeGreaterThan(bottom.baseRight.y);
    expect(Math.abs(bottom.tip.x - cx)).toBeLessThan(0.01);

    const right = computeCenterSealPointer(seal, "right")!;
    expect(right.slot).toBe("right");
    expect(right.tip.x).toBeGreaterThan(cx);
    expect(right.tip.x).toBeGreaterThan(right.baseLeft.x);
    expect(right.tip.x).toBeGreaterThan(right.baseRight.x);
    expect(Math.abs(right.tip.y - cy)).toBeLessThan(0.01);

    const top = computeCenterSealPointer(seal, "top")!;
    expect(top.slot).toBe("top");
    expect(top.tip.y).toBeLessThan(cy);
    expect(top.tip.y).toBeLessThan(top.baseLeft.y);
    expect(top.tip.y).toBeLessThan(top.baseRight.y);
    expect(Math.abs(top.tip.x - cx)).toBeLessThan(0.01);

    const left = computeCenterSealPointer(seal, "left")!;
    expect(left.slot).toBe("left");
    expect(left.tip.x).toBeLessThan(cx);
    expect(left.tip.x).toBeLessThan(left.baseLeft.x);
    expect(left.tip.x).toBeLessThan(left.baseRight.x);
    expect(Math.abs(left.tip.y - cy)).toBeLessThan(0.01);

    const sealLayout = computeCenterSealLayout(seal);
    for (const ptr of [bottom, right, top, left]) {
      expect(ptr.arcRadius).toBe(sealLayout.ringRadius);
      expect(ptr.arcRadius).toBeGreaterThan(0);
      expect(ptr.arcRadius).toBeLessThan(Math.min(seal.width, seal.height) / 2);
      expect(ptr.arcEnd).toBeGreaterThan(ptr.arcStart);
      for (const pt of [ptr.tip, ptr.baseLeft, ptr.baseRight]) {
        expect(pt.x).toBeGreaterThanOrEqual(seal.x);
        expect(pt.x).toBeLessThanOrEqual(seal.x + seal.width);
        expect(pt.y).toBeGreaterThanOrEqual(seal.y);
        expect(pt.y).toBeLessThanOrEqual(seal.y + seal.height);
      }
    }
  });
});
