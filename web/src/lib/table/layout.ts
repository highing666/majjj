import type { RelativeSlot } from "./viewModel";

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Point {
  x: number;
  y: number;
}

export interface RiverSlotPlacement extends Point {
  rotation: number;
}

export interface OpponentHandSlotGeometry extends Point {
  width: number;
  height: number;
  rotation: number;
}

export interface ViewerHandSlotGeometry {
  x: number;
  y: number;
  restX: number;
  restY: number;
  width: number;
  height: number;
}

export interface ViewerHandGeometry {
  closed: ViewerHandSlotGeometry[];
  drawn: ViewerHandSlotGeometry | null;
}

export interface CenterSealLayout {
  outerRadius: number;
  ringInset: number;
  ringRadius: number;
  roundFontSize: number;
  wallFontSize: number;
  roundY: number;
  wallY: number;
}

export interface CenterSealPointer {
  slot: RelativeSlot;
  angle: number;
  arcStart: number;
  arcEnd: number;
  arcRadius: number;
  tip: Point;
  baseLeft: Point;
  baseRight: Point;
}

const OPP_HAND_GAP = 1;

export interface TableLayout {
  width: number;
  height: number;
  cx: number;
  cy: number;
  margin: number;
  topMargin: number;
  isWide: boolean;
  handTileW: number;
  handTileH: number;
  handGap: number;
  drawnGap: number;
  bottomHandY: number;
  viewerHandRect: Rect;
  oppHandTileW: number;
  oppHandTileH: number;
  oppSideTileW: number;
  oppSideTileH: number;
  oppGap: number;
  oppHands: {
    top: Rect;
    left: Rect;
    right: Rect;
  };
  riverTileW: number;
  riverTileH: number;
  riverGap: number;
  meldTileW: number;
  meldTileH: number;
  centerSeal: Rect;
  rivers: {
    bottom: Rect;
    top: Rect;
    left: Rect;
    right: Rect;
  };
  badges: {
    bottom: Rect;
    right: Rect;
    top: Rect;
    left: Rect;
  };
  melds: {
    bottom: Point;
    right: Point;
    top: Point;
    left: Point;
  };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/**
 * Computes responsive Scheme C four-corner minimal layout coordinates for Mac wide and iPhone viewports.
 * Guarantees zero overlap across center seal, 4 corner badges, 4 rivers, opponent hands, and 13+1 hand.
 */
export function computeTableLayout(
  rawWidth: number,
  rawHeight: number,
): TableLayout {
  const width = Math.max(320, Math.round(rawWidth));
  const height = Math.max(280, Math.round(rawHeight));

  const isWide = width >= 640;
  const handGap = isWide ? 3 : 1;
  const drawnGap = isWide ? 16 : 8;
  const margin = isWide ? 20 : 10;
  const topMargin = height >= 500 ? 12 : 8;
  const bottomPad = height >= 500 ? 14 : 8;

  // Maximized 13+1 viewer hand
  const maxHandWByWidth = Math.floor(
    (width - margin * 2 - 12 * handGap - drawnGap) / 14,
  );
  const handTileW = clamp(
    Math.min(maxHandWByWidth, Math.round(height * 0.105)),
    Math.min(23, maxHandWByWidth),
    62,
  );
  const handTileH = Math.round(handTileW * 1.36);
  const bottomHandY = height - handTileH - bottomPad;
  const fullHandW = 13 * handTileW + 12 * handGap + drawnGap + handTileW;
  const viewerHandRect: Rect = {
    x: Math.round(width / 2 - fullHandW / 2),
    y: bottomHandY,
    width: fullHandW,
    height: handTileH,
  };

  // Compact four-corner seat/score badges (TL=top, TR=right, BL=left, BR=bottom)
  const badgePad = isWide ? 18 : 8;
  const badgeW = isWide
    ? clamp(Math.round(width * 0.16), 140, 178)
    : clamp(Math.round(width * 0.34), 114, 134);
  const badgeH = height >= 500 ? 28 : 22;

  const tlBadge: Rect = {
    x: badgePad,
    y: topMargin,
    width: badgeW,
    height: badgeH,
  };
  const trBadge: Rect = {
    x: width - badgePad - badgeW,
    y: topMargin,
    width: badgeW,
    height: badgeH,
  };

  const hasBottomCornerRoom =
    viewerHandRect.x >= badgePad + badgeW + 8 &&
    width - (viewerHandRect.x + viewerHandRect.width) >= badgePad + badgeW + 8;
  const bottomBadgeY = hasBottomCornerRoom
    ? height - bottomPad - badgeH
    : bottomHandY - badgeH - 6;

  const blBadge: Rect = {
    x: badgePad,
    y: bottomBadgeY,
    width: badgeW,
    height: badgeH,
  };
  const brBadge: Rect = {
    x: width - badgePad - badgeW,
    y: bottomBadgeY,
    width: badgeW,
    height: badgeH,
  };

  // Opponent hand sizing & placement
  const oppGap = OPP_HAND_GAP;
  const oppHandTileW = clamp(Math.round(handTileW * 0.44), 14, 24);
  const oppHandTileH = Math.round(oppHandTileW * 1.36);
  const maxTopOppW = 14 * oppHandTileW + 13 * oppGap;
  const topOppLeft = Math.round(width / 2 - maxTopOppW / 2);
  const topClearsCornerBadges =
    topOppLeft >= tlBadge.x + tlBadge.width + 8 &&
    topOppLeft + maxTopOppW <= trBadge.x - 8;
  const topOppY = topClearsCornerBadges ? topMargin : topMargin + badgeH + 6;
  const topOppRect: Rect = {
    x: topOppLeft,
    y: topOppY,
    width: maxTopOppW,
    height: oppHandTileH,
  };

  const meldTileW = clamp(Math.round(handTileW * 0.56), 15, 28);
  const meldTileH = Math.round(meldTileW * 1.36);

  const playTop = topOppY + oppHandTileH + (height >= 500 ? 10 : 6);
  const reservePortraitMeldBand = !hasBottomCornerRoom && !isWide;
  const playBottom = hasBottomCornerRoom
    ? bottomHandY - 10
    : reservePortraitMeldBand
      ? bottomBadgeY - meldTileH - 8
      : bottomBadgeY - 6;
  const playH = Math.max(110, playBottom - playTop);

  // Side opponent hands strictly fit within [playTop, playBottom]
  const maxSideTileWByHeight = Math.floor((playH - 10 - 13 * oppGap) / 14);
  const oppSideTileW = clamp(
    Math.min(Math.round(oppHandTileW * 0.86), maxSideTileWByHeight),
    11,
    22,
  );
  const oppSideTileH = Math.round(oppSideTileW * 1.36);
  const maxSideOppSpan = 14 * oppSideTileW + 13 * oppGap;

  const cx = Math.round(width / 2);
  const cy = Math.round(playTop + playH / 2);

  const leftOppRect: Rect = {
    x: margin,
    y: Math.round(cy - maxSideOppSpan / 2),
    width: oppSideTileH,
    height: maxSideOppSpan,
  };
  const rightOppRect: Rect = {
    x: width - margin - oppSideTileH,
    y: Math.round(cy - maxSideOppSpan / 2),
    width: oppSideTileH,
    height: maxSideOppSpan,
  };

  // Compact circular center wind + wall seal
  const sealSize = clamp(Math.round(Math.min(width, playH) * 0.14), 44, 68);
  const centerSeal: Rect = {
    x: Math.round(cx - sealSize / 2),
    y: Math.round(cy - sealSize / 2),
    width: sealSize,
    height: sealSize,
  };

  // 6-per-row rivers arranged cleanly around centerSeal with zero corner overlap
  const riverGap = isWide ? 2 : 1;
  const riverSealGap = height >= 500 ? 12 : 6;
  const halfSeal = Math.ceil(sealSize / 2);
  const centerHalfY = halfSeal + riverSealGap;

  const maxRiverAreaV = Math.max(36, Math.floor(playH / 2 - centerHalfY - 4));
  const horizPlayHalf = Math.floor(
    (rightOppRect.x - (leftOppRect.x + leftOppRect.width) - 16) / 2,
  );

  const maxRiverTileHFromV = Math.floor((maxRiverAreaV - riverGap * 2) / 3);
  const maxRiverTileWFromSpan = Math.floor(
    (Math.min(width * 0.44, playH * 0.54) - riverGap * 5) / 6,
  );
  // Horizontal constraint: centerHalfX (at least riverSpan/2 + 4) + 3 rows of riverTileH <= horizPlayHalf
  // Solve roughly: 3 * W + 2.5 * gap + 3 * 1.36 * W <= horizPlayHalf - 4 => 7.08 * W <= horizPlayHalf - 10
  const maxRiverTileWFromH = Math.floor((horizPlayHalf - 10 - riverGap * 5) / 7.15);

  const riverTileW = clamp(
    Math.min(
      Math.floor(maxRiverTileHFromV / 1.36),
      maxRiverTileWFromSpan,
      maxRiverTileWFromH,
    ),
    15,
    34,
  );
  const riverTileH = Math.round(riverTileW * 1.36);

  const riverSpan = 6 * riverTileW + 5 * riverGap;
  const riverThreeRows = 3 * riverTileH + 2 * riverGap;
  const centerHalfX = Math.max(centerHalfY, Math.ceil(riverSpan / 2) + 4);

  const bottomRiver: Rect = {
    x: Math.round(cx - riverSpan / 2),
    y: cy + centerHalfY,
    width: riverSpan,
    height: Math.min(riverThreeRows, Math.max(riverTileH, playBottom - (cy + centerHalfY))),
  };

  const topRiverHeight = Math.min(
    riverThreeRows,
    Math.max(riverTileH, cy - centerHalfY - playTop),
  );
  const topRiver: Rect = {
    x: Math.round(cx - riverSpan / 2),
    y: cy - centerHalfY - topRiverHeight,
    width: riverSpan,
    height: topRiverHeight,
  };

  const leftRiverWidth = Math.min(
    riverThreeRows,
    Math.max(riverTileH, cx - centerHalfX - (leftOppRect.x + leftOppRect.width + 6)),
  );
  const leftRiver: Rect = {
    x: cx - centerHalfX - leftRiverWidth,
    y: Math.round(cy - riverSpan / 2),
    width: leftRiverWidth,
    height: riverSpan,
  };

  const rightRiverWidth = Math.min(
    riverThreeRows,
    Math.max(riverTileH, rightOppRect.x - 6 - (cx + centerHalfX)),
  );
  const rightRiver: Rect = {
    x: cx + centerHalfX,
    y: Math.round(cy - riverSpan / 2),
    width: rightRiverWidth,
    height: riverSpan,
  };

  return {
    width,
    height,
    cx,
    cy,
    margin,
    topMargin,
    isWide,
    handTileW,
    handTileH,
    handGap,
    drawnGap,
    bottomHandY,
    viewerHandRect,
    oppHandTileW,
    oppHandTileH,
    oppSideTileW,
    oppSideTileH,
    oppGap,
    oppHands: {
      top: topOppRect,
      left: leftOppRect,
      right: rightOppRect,
    },
    riverTileW,
    riverTileH,
    riverGap,
    meldTileW,
    meldTileH,
    centerSeal,
    rivers: {
      bottom: bottomRiver,
      top: topRiver,
      left: leftRiver,
      right: rightRiver,
    },
    badges: {
      bottom: brBadge,
      right: trBadge,
      top: tlBadge,
      left: blBadge,
    },
    melds: {
      bottom: {
        x: hasBottomCornerRoom || !isWide ? width - badgePad : brBadge.x - 10,
        y: hasBottomCornerRoom
          ? brBadge.y - meldTileH - 8
          : !isWide
            ? bottomBadgeY - meldTileH - 4
            : bottomHandY - meldTileH - 6,
      },
      right: {
        x: rightOppRect.x - 8,
        y: cy + riverSpan / 2 + 8,
      },
      top: {
        x: topClearsCornerBadges ? trBadge.x - 8 : width - badgePad,
        y: topOppY + oppHandTileH + 6,
      },
      left: {
        x: leftOppRect.x + leftOppRect.width + 8,
        y: playTop,
      },
    },
  };
}

/**
 * Computes strictly bounded inner ring, compact-round, and wall-remaining coordinates for the center seal.
 */
export function computeCenterSealLayout(seal: Rect): CenterSealLayout {
  const outerRadius = Math.min(seal.width, seal.height) / 2;
  const ringInset = Math.max(4, Math.round(outerRadius * 0.16));
  const ringRadius = Math.max(4, outerRadius - ringInset);
  const roundFontSize = seal.height >= 58 ? 14 : 11;
  const wallFontSize = seal.height >= 58 ? 11 : 10;
  return {
    outerRadius,
    ringInset,
    ringRadius,
    roundFontSize,
    wallFontSize,
    roundY: Math.round(seal.y + seal.height * 0.38),
    wallY: Math.round(seal.y + seal.height * 0.69),
  };
}

const POINTER_DIRECTIONS: Record<
  RelativeSlot,
  { angle: number; arcStart: number; arcEnd: number; dx: number; dy: number }
> = {
  right: {
    angle: 0,
    arcStart: -Math.PI / 4,
    arcEnd: Math.PI / 4,
    dx: 1,
    dy: 0,
  },
  bottom: {
    angle: Math.PI / 2,
    arcStart: Math.PI / 4,
    arcEnd: (3 * Math.PI) / 4,
    dx: 0,
    dy: 1,
  },
  left: {
    angle: Math.PI,
    arcStart: (3 * Math.PI) / 4,
    arcEnd: (5 * Math.PI) / 4,
    dx: -1,
    dy: 0,
  },
  top: {
    angle: -Math.PI / 2,
    arcStart: (-3 * Math.PI) / 4,
    arcEnd: -Math.PI / 4,
    dx: 0,
    dy: -1,
  },
};

/**
 * Computes four-way directional compass arc angles and pointer triangle coordinates
 * strictly bounded inside `seal` for the active relative slot (or `null` when idle).
 */
export function computeCenterSealPointer(
  seal: Rect,
  activeSlot: RelativeSlot | null,
): CenterSealPointer | null {
  if (activeSlot === null) {
    return null;
  }
  const dir = POINTER_DIRECTIONS[activeSlot];
  const cx = seal.x + seal.width / 2;
  const cy = seal.y + seal.height / 2;
  const { outerRadius, ringRadius: arcRadius } = computeCenterSealLayout(seal);
  const baseRadius = arcRadius - 0.5;
  const halfBase = Math.max(4, Math.round(outerRadius * 0.22));
  const px = -dir.dy;
  const py = dir.dx;

  return {
    slot: activeSlot,
    angle: dir.angle,
    arcStart: dir.arcStart,
    arcEnd: dir.arcEnd,
    arcRadius,
    tip: {
      x: cx + dir.dx * outerRadius,
      y: cy + dir.dy * outerRadius,
    },
    baseLeft: {
      x: cx + dir.dx * baseRadius + px * halfBase,
      y: cy + dir.dy * baseRadius + py * halfBase,
    },
    baseRight: {
      x: cx + dir.dx * baseRadius - px * halfBase,
      y: cy + dir.dy * baseRadius - py * halfBase,
    },
  };
}

/**
 * Computes the center point and rotation of the `visibleIndex`-th slot in any seat's 6-per-row river.
 */
export function computeRiverSlotPoint(
  layout: TableLayout,
  side: RelativeSlot,
  visibleIndex: number,
): RiverSlotPlacement {
  const safeIndex = Math.max(0, Math.floor(visibleIndex));
  const col = safeIndex % 6;
  const row = Math.floor(safeIndex / 6);
  const { riverTileW: tw, riverTileH: th, riverGap: gap } = layout;
  const rect = layout.rivers[side];

  switch (side) {
    case "bottom":
      return {
        x: rect.x + col * (tw + gap) + tw / 2,
        y: rect.y + row * (th + gap) + th / 2,
        rotation: 0,
      };
    case "top":
      return {
        x: rect.x + rect.width - col * (tw + gap) - tw / 2,
        y: rect.y + rect.height - row * (th + gap) - th / 2,
        rotation: Math.PI,
      };
    case "left":
      return {
        x: rect.x + rect.width - row * (th + gap) - th / 2,
        y: rect.y + col * (tw + gap) + tw / 2,
        rotation: Math.PI / 2,
      };
    case "right":
      return {
        x: rect.x + row * (th + gap) + th / 2,
        y: rect.y + rect.height - col * (tw + gap) - tw / 2,
        rotation: -Math.PI / 2,
      };
  }
}

/**
 * Computes the center point `(x, y)` of the `visibleIndex`-th slot in the bottom player's 6-per-row river.
 * Shared between `tableRenderer.ts` and Ticket #42 discard ghost preview.
 */
export function computeBottomRiverSlotPoint(
  layout: TableLayout,
  visibleIndex: number,
): Point {
  const { x, y } = computeRiverSlotPoint(layout, "bottom", visibleIndex);
  return { x, y };
}

/**
 * Computes center coordinates, tile dimensions, and rotations for an opponent's concealed hand.
 */
export function computeOpponentHandSlots(
  layout: TableLayout,
  side: "top" | "left" | "right",
  count: number,
): OpponentHandSlotGeometry[] {
  const safeCount = Math.max(0, Math.floor(count));
  if (safeCount === 0) {
    return [];
  }
  const gap = layout.oppGap;

  if (side === "top") {
    const { oppHandTileW: ow, oppHandTileH: oh } = layout;
    const span = safeCount * ow + Math.max(0, safeCount - 1) * gap;
    const rect = layout.oppHands.top;
    const startX = rect.x + (rect.width - span) / 2;
    const y = rect.y + oh / 2;
    return Array.from({ length: safeCount }, (_, i) => ({
      x: startX + i * (ow + gap) + ow / 2,
      y,
      width: ow,
      height: oh,
      rotation: Math.PI,
    }));
  }

  const { oppSideTileW: sw, oppSideTileH: sh } = layout;
  const span = safeCount * sw + Math.max(0, safeCount - 1) * gap;
  const rect = layout.oppHands[side];
  const startY = rect.y + (rect.height - span) / 2;
  const x = rect.x + sh / 2;
  const rotation = side === "left" ? Math.PI / 2 : -Math.PI / 2;
  return Array.from({ length: safeCount }, (_, i) => ({
    x,
    y: startY + i * (sw + gap) + sw / 2,
    width: sw,
    height: sh,
    rotation,
  }));
}

/**
 * Computes slot geometry for the viewer's closed tiles plus optional separated drawn tile.
 */
export function computeViewerHandSlots(
  layout: TableLayout,
  closedCount: number,
  hasDrawn: boolean,
): ViewerHandGeometry {
  const safeClosed = Math.max(0, Math.floor(closedCount));
  const {
    handTileW: tw,
    handTileH: th,
    handGap: gap,
    drawnGap,
    bottomHandY: y,
  } = layout;

  const closedW = safeClosed * tw + Math.max(0, safeClosed - 1) * gap;
  const fullW = closedW + (hasDrawn ? drawnGap + tw : 0);
  const startX = Math.round(layout.cx - fullW / 2);

  const closed: ViewerHandSlotGeometry[] = Array.from(
    { length: safeClosed },
    (_, idx) => {
      const x = startX + idx * (tw + gap);
      return {
        x,
        y,
        restX: x + tw / 2,
        restY: y + th / 2,
        width: tw,
        height: th,
      };
    },
  );

  let drawn: ViewerHandSlotGeometry | null = null;
  if (hasDrawn) {
    const drawnX = startX + closedW + drawnGap;
    drawn = {
      x: drawnX,
      y,
      restX: drawnX + tw / 2,
      restY: y + th / 2,
      width: tw,
      height: th,
    };
  }

  return { closed, drawn };
}
