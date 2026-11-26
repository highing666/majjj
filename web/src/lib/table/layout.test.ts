import { describe, expect, it } from "vitest";
import {
  computeBottomRiverSlotPoint,
  computeCenterSealEdgeBars,
  computeCenterSealLayout,
  computeOpponentHandSlots,
  computeRiverSlotPoint,
  computeSeatBadgeLayout,
  computeTableLayout,
  computeViewerHandSlots,
  MELD_GROUP_GAP,
  MELD_TILE_GAP,
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

function expectRectContains(outer: Rect, inner: Rect): void {
  expect(inner.x).toBeGreaterThanOrEqual(outer.x);
  expect(inner.y).toBeGreaterThanOrEqual(outer.y);
  expect(inner.x + inner.width).toBeLessThanOrEqual(outer.x + outer.width);
  expect(inner.y + inner.height).toBeLessThanOrEqual(outer.y + outer.height);
}

describe("Scheme B three-column theater computeTableLayout", () => {
  it.each([
    { name: "Mac wide 1440x900", width: 1440, height: 900, wide: true, minHandW: 54, minRiverW: 28 },
    { name: "Mac laptop 1280x720", width: 1280, height: 720, wide: true, minHandW: 46, minRiverW: 24 },
    { name: "desktop 1024x720", width: 1024, height: 720, wide: true, minHandW: 36, minRiverW: 20 },
    { name: "iPhone landscape 844x390", width: 844, height: 390, wide: true, minHandW: 30, minRiverW: 18 },
    { name: "compact landscape 760x340", width: 760, height: 340, wide: true, minHandW: 26, minRiverW: 16 },
    { name: "SE landscape 568x320", width: 568, height: 320, wide: true, minHandW: 20, minRiverW: 14 },
    { name: "iPhone portrait fallback 390x844", width: 390, height: 844, wide: false, minHandW: 23, minRiverW: 15 },
    { name: "SE portrait fallback 320x568", width: 320, height: 568, wide: false, minHandW: 19, minRiverW: 14 },
  ])(
    "produces zero-overlap, in-bounds Scheme B layout on $name ($width x $height)",
    ({ width, height, wide, minHandW, minRiverW }) => {
      const layout = computeTableLayout(width, height);

      expect(layout.isWide).toBe(wide);
      expect(layout.handTileW).toBeGreaterThanOrEqual(minHandW);
      expect(layout.handTileH).toBeGreaterThan(layout.handTileW);
      expect(layout.riverTileW).toBeGreaterThanOrEqual(minRiverW);
      expect(layout.riverTileH).toBeGreaterThan(layout.riverTileW);

      // Three theater regions + top floating capsule safe areas stay in bounds and pairwise disjoint
      for (const zone of [
        layout.leftWing,
        layout.centerStage,
        layout.rightWing,
        layout.topLeftSafeRect,
        layout.topRightSafeRect,
      ]) {
        expectInsideBounds(zone, width, height);
        expect(zone.width).toBeGreaterThan(0);
        expect(zone.height).toBeGreaterThan(0);
      }

      const theaterColumns = [
        layout.leftWing,
        layout.centerStage,
        layout.rightWing,
        layout.topLeftSafeRect,
        layout.topRightSafeRect,
      ];
      for (let i = 0; i < theaterColumns.length; i += 1) {
        for (let j = i + 1; j < theaterColumns.length; j += 1) {
          expect(rectsOverlap(theaterColumns[i], theaterColumns[j])).toBe(false);
        }
      }

      if (wide) {
        expect(layout.leftWing.x + layout.leftWing.width).toBeLessThan(layout.centerStage.x);
        expect(layout.centerStage.x + layout.centerStage.width).toBeLessThan(layout.rightWing.x);
        expect(layout.riverGrid.top.perLine).toBe(10);
        expect(layout.riverGrid.bottom.perLine).toBe(10);
        expect(layout.riverGrid.left.perLine).toBe(4);
        expect(layout.riverGrid.right.perLine).toBe(4);
      } else {
        expect(layout.riverGrid.top.perLine).toBeGreaterThanOrEqual(4);
        expect(layout.riverGrid.bottom.perLine).toBeGreaterThanOrEqual(4);
        expect(layout.riverGrid.left.perLine).toBeGreaterThanOrEqual(4);
        expect(layout.riverGrid.right.perLine).toBeGreaterThanOrEqual(4);
      }

      // Ultra-flat rounded pill center seal
      expectInsideBounds(layout.centerSeal, width, height);
      expect(layout.centerSeal.width).toBeGreaterThan(layout.centerSeal.height * 2.5);
      expect(layout.centerSeal.height).toBeLessThanOrEqual(36);

      const sealLayout = computeCenterSealLayout(layout.centerSeal);
      expect(sealLayout.pillRadius).toBe(layout.centerSeal.height / 2);
      expect(sealLayout.fontSize).toBeGreaterThanOrEqual(10);
      expect(sealLayout.textMaxWidth).toBeGreaterThanOrEqual(40);
      expect(sealLayout.textY).toBeGreaterThan(layout.centerSeal.y);
      expect(sealLayout.textY).toBeLessThan(layout.centerSeal.y + layout.centerSeal.height);

      // Viewer 13+1 hand + centered turnBarRect directly above hand
      expectInsideBounds(layout.viewerHandRect, width, height);
      expectInsideBounds(layout.turnBarRect, width, height);
      expect(layout.viewerHandRect.y).toBe(layout.bottomHandY);
      expect(layout.turnBarRect.y + layout.turnBarRect.height).toBeLessThan(
        layout.viewerHandRect.y,
      );
      expect(
        Math.abs(
          layout.turnBarRect.x +
            layout.turnBarRect.width / 2 -
            (layout.viewerHandRect.x + layout.viewerHandRect.width / 2),
        ),
      ).toBeLessThanOrEqual(2);

      if (wide) {
        expectRectContains(layout.centerStage, layout.viewerHandRect);
        expectRectContains(layout.centerStage, layout.turnBarRect);
        expectRectContains(layout.centerStage, layout.centerSeal);
        expectRectContains(layout.centerStage, layout.badges.top);
        expectRectContains(layout.centerStage, layout.oppHands.top);
        expectRectContains(layout.centerStage, layout.personaPockets.top);
        expectRectContains(layout.centerStage, layout.riverTroughs.top);
        expectRectContains(layout.centerStage, layout.riverTroughs.bottom);

        expectRectContains(layout.leftWing, layout.badges.left);
        expectRectContains(layout.leftWing, layout.personaPockets.left);
        expectRectContains(layout.leftWing, layout.oppHands.left);
        expectRectContains(layout.leftWing, layout.riverTroughs.left);
        expectRectContains(layout.leftWing, layout.badges.bottom);

        expectRectContains(layout.rightWing, layout.badges.right);
        expectRectContains(layout.rightWing, layout.personaPockets.right);
        expectRectContains(layout.rightWing, layout.oppHands.right);
        expectRectContains(layout.rightWing, layout.riverTroughs.right);
      }

      // Opponent hands, badges, persona pockets, river troughs, and inner rivers
      const riverSides = ["bottom", "top", "left", "right"] as const;
      for (const side of riverSides) {
        expectInsideBounds(layout.riverTroughs[side], width, height);
        expectRectContains(layout.riverTroughs[side], layout.rivers[side]);
      }

      const sampleMeldW = 3 * layout.meldTileW + 2;
      const sampleMelds: Rect[] = [
        {
          x: layout.melds.bottom.x - sampleMeldW,
          y: layout.melds.bottom.y,
          width: sampleMeldW,
          height: layout.meldTileH,
        },
        {
          x: layout.melds.top.x - sampleMeldW,
          y: layout.melds.top.y,
          width: sampleMeldW,
          height: layout.meldTileH,
        },
        {
          x: layout.melds.left.x,
          y: layout.melds.left.y,
          width: layout.meldTileH,
          height: sampleMeldW,
        },
        {
          x: layout.melds.right.x - layout.meldTileH,
          y: layout.melds.right.y,
          width: layout.meldTileH,
          height: sampleMeldW,
        },
      ];
      for (const meldRect of sampleMelds) {
        expectInsideBounds(meldRect, width, height);
      }

      const coreBoxes: Rect[] = [
        layout.topLeftSafeRect,
        layout.topRightSafeRect,
        layout.centerSeal,
        layout.turnBarRect,
        layout.viewerHandRect,
        layout.oppHands.top,
        layout.oppHands.left,
        layout.oppHands.right,
        layout.badges.top,
        layout.badges.right,
        layout.badges.left,
        layout.badges.bottom,
        layout.personaPockets.top,
        layout.personaPockets.left,
        layout.personaPockets.right,
        layout.riverTroughs.bottom,
        layout.riverTroughs.top,
        layout.riverTroughs.left,
        layout.riverTroughs.right,
        ...sampleMelds,
      ];
      for (let i = 0; i < coreBoxes.length; i += 1) {
        expectInsideBounds(coreBoxes[i], width, height);
        for (let j = i + 1; j < coreBoxes.length; j += 1) {
          expect(rectsOverlap(coreBoxes[i], coreBoxes[j])).toBe(false);
        }
      }

      // Multi-meld 对门 (2-4 melds, up to 4 tiles each): zero overlap with all core regions
      for (const meldCount of [2, 3, 4]) {
        const kongMeldW = 4 * layout.meldTileW + 3 * MELD_TILE_GAP;
        const totalTopMeldW =
          meldCount * kongMeldW + (meldCount - 1) * MELD_GROUP_GAP;
        const multiTopMeldRect: Rect = {
          x: layout.melds.top.x - totalTopMeldW,
          y: layout.melds.top.y,
          width: totalTopMeldW,
          height: layout.meldTileH,
        };
        expectInsideBounds(multiTopMeldRect, width, height);
        if (wide) {
          expectRectContains(layout.centerStage, multiTopMeldRect);
        }
        const nonTopMeldBoxes = [
          layout.topLeftSafeRect,
          layout.topRightSafeRect,
          layout.centerSeal,
          layout.turnBarRect,
          layout.viewerHandRect,
          layout.oppHands.top,
          layout.oppHands.left,
          layout.oppHands.right,
          layout.badges.top,
          layout.badges.right,
          layout.badges.left,
          layout.badges.bottom,
          layout.personaPockets.top,
          layout.personaPockets.left,
          layout.personaPockets.right,
          layout.riverTroughs.bottom,
          layout.riverTroughs.top,
          layout.riverTroughs.left,
          layout.riverTroughs.right,
        ];
        for (const box of nonTopMeldBoxes) {
          expect(rectsOverlap(multiTopMeldRect, box)).toBe(false);
        }
      }
    },
  );

  it("computes 10-per-row vertical and 4-per-column side river slot coordinates with exact ghost preview parity", () => {
    const layout = computeTableLayout(1440, 900);

    expect(layout.riverGrid.bottom.perLine).toBe(10);
    expect(layout.riverGrid.top.perLine).toBe(10);
    expect(layout.riverGrid.left.perLine).toBe(4);
    expect(layout.riverGrid.right.perLine).toBe(4);

    // Bottom 10-per-row slots (indices 0..19)
    const p0 = computeBottomRiverSlotPoint(layout, 0);
    const p1 = computeBottomRiverSlotPoint(layout, 1);
    const p9 = computeBottomRiverSlotPoint(layout, 9);
    const p10 = computeBottomRiverSlotPoint(layout, 10);

    expect(p0.x).toBe(layout.rivers.bottom.x + layout.riverTileW / 2);
    expect(p0.y).toBe(layout.rivers.bottom.y + layout.riverTileH / 2);
    expect(p1.x - p0.x).toBe(layout.riverTileW + layout.riverGap);
    expect(p1.y).toBe(p0.y);
    expect(p9.x + layout.riverTileW / 2).toBe(
      layout.rivers.bottom.x + layout.rivers.bottom.width,
    );
    expect(p10.x).toBe(p0.x);
    expect(p10.y - p0.y).toBe(layout.riverTileH + layout.riverGap);

    expect(computeBottomRiverSlotPoint(layout, 13)).toEqual({
      x: computeRiverSlotPoint(layout, "bottom", 13).x,
      y: computeRiverSlotPoint(layout, "bottom", 13).y,
    });

    // Side 4-per-col slots (indices 0..15)
    const l0 = computeRiverSlotPoint(layout, "left", 0);
    const l3 = computeRiverSlotPoint(layout, "left", 3);
    const l4 = computeRiverSlotPoint(layout, "left", 4);
    expect(l0.x).toBe(l3.x);
    expect(l3.y - l0.y).toBe(3 * (layout.riverTileW + layout.riverGap));
    expect(l4.y).toBe(l0.y);
    expect(l0.x - l4.x).toBe(layout.riverTileH + layout.riverGap);

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
      const maxIndex = horizontal ? 19 : 15;
      for (const idx of [0, 3, 4, 9, 10, maxIndex]) {
        const pt = computeRiverSlotPoint(layout, side, idx);
        expect(pt.rotation).toBe(expectedRotations[side]);
        expect(pt.x - halfW).toBeGreaterThanOrEqual(rect.x - 0.001);
        expect(pt.x + halfW).toBeLessThanOrEqual(rect.x + rect.width + 0.001);
        expect(pt.y - halfH).toBeGreaterThanOrEqual(rect.y - 0.001);
        expect(pt.y + halfH).toBeLessThanOrEqual(rect.y + rect.height + 0.001);
      }
    }
  });

  it("computes opponent hand slots and viewer 13+1 hand slots inside layout bounds", () => {
    const layout = computeTableLayout(1280, 720);

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
  });

  it("computes four-way edge glow bars strictly inside the ultra-flat pill centerSeal", () => {
    const layout = computeTableLayout(1440, 900);
    const seal = layout.centerSeal;
    const cx = seal.x + seal.width / 2;
    const cy = seal.y + seal.height / 2;

    const bars = computeCenterSealEdgeBars(seal);

    // Top = 对门, Bottom = 自家, Left = 上家, Right = 下家
    expect(bars.top.rect.y + bars.top.rect.height / 2).toBeLessThan(cy);
    expect(bars.bottom.rect.y + bars.bottom.rect.height / 2).toBeGreaterThan(cy);
    expect(bars.left.rect.x + bars.left.rect.width / 2).toBeLessThan(cx);
    expect(bars.right.rect.x + bars.right.rect.width / 2).toBeGreaterThan(cx);

    for (const slot of ["top", "bottom", "left", "right"] as const) {
      const bar = bars[slot];
      expect(bar.slot).toBe(slot);
      expectRectContains(seal, bar.rect);
      expectRectContains(seal, bar.activeRect);
      expect(
        bar.activeRect.width * bar.activeRect.height,
      ).toBeGreaterThan(bar.rect.width * bar.rect.height);
    }
  });

  it("computes seat badge pill, dealer chip, and clamped name-width metrics inside badge bounds", () => {
    const standardRect: Rect = { x: 20, y: 40, width: 180, height: 28 };
    const dealerMetrics = computeSeatBadgeLayout(standardRect, true, 36);
    expect(dealerMetrics.compact).toBe(false);
    expect(dealerMetrics.padX).toBe(7);
    expect(dealerMetrics.padY).toBe(4);
    expect(dealerMetrics.pillH).toBe(20);
    expect(dealerMetrics.rolePillW).toBe(32);
    expect(dealerMetrics.dealerPillW).toBe(19);
    expect(dealerMetrics.scoreGap).toBe(5);
    expectRectContains(standardRect, dealerMetrics.rolePillRect);
    expect(dealerMetrics.dealerPillRect).not.toBeNull();
    expectRectContains(standardRect, dealerMetrics.dealerPillRect!);
    expect(rectsOverlap(dealerMetrics.rolePillRect, dealerMetrics.dealerPillRect!)).toBe(
      false,
    );
    expect(dealerMetrics.nameStartX).toBeGreaterThan(
      dealerMetrics.dealerPillRect!.x + dealerMetrics.dealerPillRect!.width,
    );
    expect(dealerMetrics.nameMaxW).toBe(
      dealerMetrics.scoreRightX - 36 - dealerMetrics.scoreGap - dealerMetrics.nameStartX,
    );

    const compactRect: Rect = { x: 10, y: 12, width: 96, height: 20 };
    const compactNonDealer = computeSeatBadgeLayout(compactRect, false, 120);
    expect(compactNonDealer.compact).toBe(true);
    expect(compactNonDealer.dealerPillRect).toBeNull();
    expect(compactNonDealer.rolePillW).toBe(24);
    expect(compactNonDealer.nameMaxW).toBe(8);
    expectRectContains(compactRect, compactNonDealer.rolePillRect);
  });
});
