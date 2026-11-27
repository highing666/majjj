import { describe, expect, it } from "vitest";
import { computeQuickEmoteDockRect } from "./banterLayout";
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
  type TableLayoutOptions,
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

function computeMeldSpan(tileCount: number, groupCount: number, tileW: number): number {
  if (tileCount <= 0) {
    return 0;
  }
  const groups = Math.max(1, groupCount);
  return (
    tileCount * tileW +
    Math.max(0, tileCount - groups) * MELD_TILE_GAP +
    Math.max(0, groups - 1) * MELD_GROUP_GAP
  );
}

const VIEWPORT_MATRIX = [
  // 4 core target devices from ADR-0013 / Spec #96 (full & browser/Safari-bar states)
  { name: '14" MBP full 1512x982', width: 1512, height: 982, wide: true, minHandW: 74, minRiverW: 46, minMeldW: 38 },
  { name: '14" MBP browser 1512x860', width: 1512, height: 860, wide: true, minHandW: 74, minRiverW: 46, minMeldW: 38 },
  { name: '13" MBA M2 full 1470x956', width: 1470, height: 956, wide: true, minHandW: 71, minRiverW: 46, minMeldW: 37 },
  { name: '13" MBA M2 browser 1470x835', width: 1470, height: 835, wide: true, minHandW: 71, minRiverW: 46, minMeldW: 37 },
  { name: "iPhone 13 Pro full 844x390", width: 844, height: 390, wide: true, minHandW: 41, minRiverW: 28, minMeldW: 20 },
  { name: "iPhone 13 Pro Safari 844x325", width: 844, height: 325, wide: true, minHandW: 41, minRiverW: 18, minMeldW: 20 },
  { name: "iPhone 17 full 874x402", width: 874, height: 402, wide: true, minHandW: 43, minRiverW: 28, minMeldW: 21 },
  { name: "iPhone 17 Safari 874x335", width: 874, height: 335, wide: true, minHandW: 43, minRiverW: 19, minMeldW: 21 },
  // Standard / ultra-wide / compact landscape + narrow portrait fallbacks
  { name: "desktop FHD 1920x1080", width: 1920, height: 1080, wide: true, minHandW: 88, minRiverW: 46, minMeldW: 38 },
  { name: "Mac wide 1440x900", width: 1440, height: 900, wide: true, minHandW: 69, minRiverW: 46, minMeldW: 36 },
  { name: "Mac laptop 1280x720", width: 1280, height: 720, wide: true, minHandW: 60, minRiverW: 46, minMeldW: 31 },
  { name: "desktop 1024x720", width: 1024, height: 720, wide: true, minHandW: 43, minRiverW: 40, minMeldW: 22 },
  { name: "compact landscape 760x340", width: 760, height: 340, wide: true, minHandW: 36, minRiverW: 20, minMeldW: 18 },
  { name: "SE landscape 568x320", width: 568, height: 320, wide: true, minHandW: 22, minRiverW: 18, minMeldW: 12 },
  { name: "iPhone portrait fallback 390x844", width: 390, height: 844, wide: false, minHandW: 23, minRiverW: 15, minMeldW: 14 },
  { name: "SE portrait fallback 320x568", width: 320, height: 568, wide: false, minHandW: 19, minRiverW: 14, minMeldW: 12 },
] as const;

const VIEWER_MELD_SCENARIOS: ReadonlyArray<{
  label: string;
  options: Required<TableLayoutOptions>;
}> = [
  { label: "0 melds (13+1)", options: { bottomMeldTileCount: 0, bottomMeldGroupCount: 0, viewerClosedCount: 13, viewerHasDrawn: true } },
  { label: "1 pon (10+1)", options: { bottomMeldTileCount: 3, bottomMeldGroupCount: 1, viewerClosedCount: 10, viewerHasDrawn: true } },
  { label: "1 pon post-pon (10+0)", options: { bottomMeldTileCount: 3, bottomMeldGroupCount: 1, viewerClosedCount: 10, viewerHasDrawn: false } },
  { label: "1 kong (10+1)", options: { bottomMeldTileCount: 4, bottomMeldGroupCount: 1, viewerClosedCount: 10, viewerHasDrawn: true } },
  { label: "2 pons (7+1)", options: { bottomMeldTileCount: 6, bottomMeldGroupCount: 2, viewerClosedCount: 7, viewerHasDrawn: true } },
  { label: "2 kongs (7+1)", options: { bottomMeldTileCount: 8, bottomMeldGroupCount: 2, viewerClosedCount: 7, viewerHasDrawn: true } },
  { label: "3 pons (4+1)", options: { bottomMeldTileCount: 9, bottomMeldGroupCount: 3, viewerClosedCount: 4, viewerHasDrawn: true } },
  { label: "3 kongs (4+1)", options: { bottomMeldTileCount: 12, bottomMeldGroupCount: 3, viewerClosedCount: 4, viewerHasDrawn: true } },
  { label: "4 pons (1+1)", options: { bottomMeldTileCount: 12, bottomMeldGroupCount: 4, viewerClosedCount: 1, viewerHasDrawn: true } },
  { label: "4 kongs (1+1)", options: { bottomMeldTileCount: 16, bottomMeldGroupCount: 4, viewerClosedCount: 1, viewerHasDrawn: true } },
];

describe("Cross-column enlarged theater computeTableLayout (ADR-0013)", () => {
  it.each(VIEWPORT_MATRIX)(
    "produces enlarged, capped, zero-overlap layout on $name ($width x $height)",
    ({ width, height, wide, minHandW, minRiverW, minMeldW }) => {
      const layout = computeTableLayout(width, height);

      expect(layout.isWide).toBe(wide);
      expect(layout.handTileW).toBeGreaterThanOrEqual(minHandW);
      expect(layout.handTileW).toBeLessThanOrEqual(88);
      expect(layout.handTileH).toBeGreaterThan(layout.handTileW);
      expect(layout.riverTileW).toBeGreaterThanOrEqual(minRiverW);
      expect(layout.riverTileW).toBeLessThanOrEqual(46);
      expect(layout.riverTileH).toBeGreaterThan(layout.riverTileW);
      expect(layout.meldTileW).toBeGreaterThanOrEqual(minMeldW);
      expect(layout.meldTileW).toBeLessThanOrEqual(38);
      expect(layout.meldTileH).toBeGreaterThan(layout.meldTileW);

      // Three theater regions + top floating capsule safe areas stay in bounds and pairwise disjoint
      const theaterColumns = [
        layout.leftWing,
        layout.centerStage,
        layout.rightWing,
        layout.topLeftSafeRect,
        layout.topRightSafeRect,
      ];
      for (const zone of theaterColumns) {
        expectInsideBounds(zone, width, height);
        expect(zone.width).toBeGreaterThan(0);
        expect(zone.height).toBeGreaterThan(0);
      }
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
        const expectedSidePerLine = layout.isShortLandscape ? 5 : 6;
        expect(layout.riverGrid.left.perLine).toBe(expectedSidePerLine);
        expect(layout.riverGrid.right.perLine).toBe(expectedSidePerLine);
        expect(layout.riverGrid.left.maxLines).toBe(2);
        expect(layout.riverGrid.right.maxLines).toBe(2);

        // Compact bottom-left 自家 badge width (126..156 desktop, 90..112 short landscape)
        if (layout.isShortLandscape) {
          expect(layout.badges.bottom.width).toBeGreaterThanOrEqual(90);
          expect(layout.badges.bottom.width).toBeLessThanOrEqual(112);
        } else {
          expect(layout.badges.bottom.width).toBeGreaterThanOrEqual(126);
          expect(layout.badges.bottom.width).toBeLessThanOrEqual(156);
        }
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

      // Viewer 13+1 hand + turnBarRect directly above hand
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
        // Viewer hand crosses out of centerStage into reclaimed bottom corridor while strictly
        // clearing compact bottomBadge on the left and rightWing/QuickEmoteBar on the right
        expect(layout.viewerHandRect.x).toBeGreaterThan(
          layout.badges.bottom.x + layout.badges.bottom.width,
        );
        expect(layout.viewerHandRect.x + layout.viewerHandRect.width).toBeLessThan(
          layout.rightWing.x,
        );

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

      // River troughs and inner rivers
      const riverSides = ["bottom", "top", "left", "right"] as const;
      for (const side of riverSides) {
        expectInsideBounds(layout.riverTroughs[side], width, height);
        expectRectContains(layout.riverTroughs[side], layout.rivers[side]);
      }

      // Multi-meld 对门 (2-4 melds, up to 4 tiles each): zero overlap with all core regions
      for (const meldCount of [2, 3, 4]) {
        const totalTopMeldW = computeMeldSpan(meldCount * 4, meldCount, layout.meldTileW);
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

  it("maintains strict in-bounds and zero-overlap across all viewports for 0..4 viewer melds (pons and kongs)", () => {
    for (const vp of VIEWPORT_MATRIX) {
      for (const scenario of VIEWER_MELD_SCENARIOS) {
        const {
          bottomMeldTileCount,
          bottomMeldGroupCount,
          viewerClosedCount,
          viewerHasDrawn,
        } = scenario.options;
        const layout = computeTableLayout(vp.width, vp.height, scenario.options);
        const tag = `${vp.name} / ${scenario.label}`;

        expect(layout.handTileW, `${tag} handTileW cap`).toBeLessThanOrEqual(88);
        expect(layout.riverTileW, `${tag} riverTileW cap`).toBeLessThanOrEqual(46);
        expect(layout.meldTileW, `${tag} meldTileW cap`).toBeLessThanOrEqual(38);

        // Viewer hand slots align pixel-for-pixel with viewerHandRect
        const handSlots = computeViewerHandSlots(layout, viewerClosedCount, viewerHasDrawn);
        expect(handSlots.closed).toHaveLength(viewerClosedCount);
        expect(handSlots.closed[0].x, `${tag} first hand tile x`).toBe(layout.viewerHandRect.x);
        const lastTile = handSlots.drawn ?? handSlots.closed[handSlots.closed.length - 1];
        expect(lastTile.x + lastTile.width, `${tag} last hand tile right`).toBe(
          layout.viewerHandRect.x + layout.viewerHandRect.width,
        );

        const sampleMeldW = computeMeldSpan(3, 1, layout.meldTileW);
        const coreBoxes: Array<[string, Rect]> = [
          ["topLeftSafe", layout.topLeftSafeRect],
          ["topRightSafe", layout.topRightSafeRect],
          ["centerSeal", layout.centerSeal],
          ["turnBar", layout.turnBarRect],
          ["viewerHand", layout.viewerHandRect],
          ["oppTop", layout.oppHands.top],
          ["oppLeft", layout.oppHands.left],
          ["oppRight", layout.oppHands.right],
          ["badgeTop", layout.badges.top],
          ["badgeRight", layout.badges.right],
          ["badgeLeft", layout.badges.left],
          ["badgeBottom", layout.badges.bottom],
          ["pocketTop", layout.personaPockets.top],
          ["pocketLeft", layout.personaPockets.left],
          ["pocketRight", layout.personaPockets.right],
          ["troughBottom", layout.riverTroughs.bottom],
          ["troughTop", layout.riverTroughs.top],
          ["troughLeft", layout.riverTroughs.left],
          ["troughRight", layout.riverTroughs.right],
          [
            "topMeld",
            {
              x: layout.melds.top.x - sampleMeldW,
              y: layout.melds.top.y,
              width: sampleMeldW,
              height: layout.meldTileH,
            },
          ],
          [
            "leftMeld",
            {
              x: layout.melds.left.x,
              y: layout.melds.left.y,
              width: layout.meldTileH,
              height: sampleMeldW,
            },
          ],
          [
            "rightMeld",
            {
              x: layout.melds.right.x - layout.meldTileH,
              y: layout.melds.right.y,
              width: layout.meldTileH,
              height: sampleMeldW,
            },
          ],
        ];

        if (vp.wide) {
          // Landscape QuickEmoteBar sits inside rightWing and must never overlap any core region
          coreBoxes.push(["quickEmoteDock", computeQuickEmoteDockRect(layout)]);
        }

        if (bottomMeldTileCount > 0) {
          const bottomMeldW = computeMeldSpan(
            bottomMeldTileCount,
            bottomMeldGroupCount,
            layout.meldTileW,
          );
          const bottomMeldRect: Rect = {
            x: layout.melds.bottom.x - bottomMeldW,
            y: layout.melds.bottom.y,
            width: bottomMeldW,
            height: layout.meldTileH,
          };
          coreBoxes.push(["bottomMelds", bottomMeldRect]);
          if (vp.wide) {
            expect(
              layout.viewerHandRect.x + layout.viewerHandRect.width,
              `${tag} viewerHand strictly left of bottomMelds`,
            ).toBeLessThan(bottomMeldRect.x);
            expect(
              bottomMeldRect.x + bottomMeldRect.width,
              `${tag} bottomMelds strictly left of rightWing`,
            ).toBeLessThan(layout.rightWing.x);
          }
        }

        for (const [name, box] of coreBoxes) {
          expect(box.x, `${tag}/${name} x>=0`).toBeGreaterThanOrEqual(0);
          expect(box.y, `${tag}/${name} y>=0`).toBeGreaterThanOrEqual(0);
          expect(box.x + box.width, `${tag}/${name} right<=w`).toBeLessThanOrEqual(vp.width);
          expect(box.y + box.height, `${tag}/${name} bottom<=h`).toBeLessThanOrEqual(vp.height);
        }

        for (let i = 0; i < coreBoxes.length; i += 1) {
          for (let j = i + 1; j < coreBoxes.length; j += 1) {
            expect(
              rectsOverlap(coreBoxes[i][1], coreBoxes[j][1]),
              `${tag}: ${coreBoxes[i][0]} overlaps ${coreBoxes[j][0]}`,
            ).toBe(false);
          }
        }
      }
    }
  });

  it("reclaims horizontal width beyond centerStage for 14-tile viewer hand on wide screens", () => {
    const mbp = computeTableLayout(1512, 982);
    expect(mbp.handTileW).toBe(74);
    expect(mbp.handTileH).toBe(100);
    expect(mbp.riverTileW).toBe(46);
    expect(mbp.riverTileH).toBe(62);
    expect(mbp.meldTileW).toBe(38);
    expect(mbp.meldTileH).toBe(51);
    // 14 enlarged tiles exceed centerStage width and gracefully span the reclaimed bottom corridor
    expect(mbp.viewerHandRect.width).toBeGreaterThan(mbp.centerStage.width);
    expect(mbp.viewerHandRect.x).toBeGreaterThan(mbp.badges.bottom.x + mbp.badges.bottom.width);
    expect(mbp.viewerHandRect.x + mbp.viewerHandRect.width).toBeLessThan(mbp.rightWing.x);

    const mba = computeTableLayout(1470, 956);
    expect(mba.handTileW).toBe(71);
    expect(mba.handTileH).toBe(96);
    expect(mba.riverTileW).toBe(46);
    expect(mba.riverTileH).toBe(62);

    const iphone13 = computeTableLayout(844, 390);
    expect(iphone13.handTileW).toBeGreaterThanOrEqual(41);
    const iphone17 = computeTableLayout(874, 402);
    expect(iphone17.handTileW).toBeGreaterThanOrEqual(43);
  });

  it("computes 10-per-row vertical and 2-column side river slot coordinates with exact ghost preview parity", () => {
    const layout = computeTableLayout(1440, 900);

    expect(layout.riverGrid.bottom.perLine).toBe(10);
    expect(layout.riverGrid.top.perLine).toBe(10);
    expect(layout.riverGrid.left).toEqual({ perLine: 6, maxLines: 2 });
    expect(layout.riverGrid.right).toEqual({ perLine: 6, maxLines: 2 });

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

    // Side 6-per-col, 2-col slots (indices 0..11) on desktop
    const l0 = computeRiverSlotPoint(layout, "left", 0);
    const l5 = computeRiverSlotPoint(layout, "left", 5);
    const l6 = computeRiverSlotPoint(layout, "left", 6);
    expect(l0.x).toBe(l5.x);
    expect(l5.y - l0.y).toBe(5 * (layout.riverTileW + layout.riverGap));
    expect(l6.y).toBe(l0.y);
    expect(l0.x - l6.x).toBe(layout.riverTileH + layout.riverGap);

    const expectedRotations = {
      bottom: 0,
      top: Math.PI,
      left: Math.PI / 2,
      right: -Math.PI / 2,
    } as const;

    for (const side of ["bottom", "top", "left", "right"] as const) {
      const rect = layout.rivers[side];
      const grid = layout.riverGrid[side];
      const horizontal = side === "bottom" || side === "top";
      const halfW = (horizontal ? layout.riverTileW : layout.riverTileH) / 2;
      const halfH = (horizontal ? layout.riverTileH : layout.riverTileW) / 2;
      const maxIndex = grid.perLine * grid.maxLines - 1;
      for (const idx of [0, grid.perLine - 1, grid.perLine, maxIndex]) {
        const pt = computeRiverSlotPoint(layout, side, idx);
        expect(pt.rotation).toBe(expectedRotations[side]);
        expect(pt.x - halfW).toBeGreaterThanOrEqual(rect.x - 0.001);
        expect(pt.x + halfW).toBeLessThanOrEqual(rect.x + rect.width + 0.001);
        expect(pt.y - halfH).toBeGreaterThanOrEqual(rect.y - 0.001);
        expect(pt.y + halfH).toBeLessThanOrEqual(rect.y + rect.height + 0.001);
      }
    }

    // Short landscape uses 5-per-col, 2-col side rivers
    const shortLayout = computeTableLayout(844, 390);
    expect(shortLayout.riverGrid.left).toEqual({ perLine: 5, maxLines: 2 });
    expect(shortLayout.riverGrid.right).toEqual({ perLine: 5, maxLines: 2 });
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
