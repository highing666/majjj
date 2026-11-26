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

export interface RiverGridSpec {
  perLine: number;
  maxLines: number;
}

export interface CenterSealLayout {
  pillRadius: number;
  fontSize: number;
  textY: number;
  textMaxWidth: number;
}

export interface CenterSealEdgeBar {
  slot: RelativeSlot;
  rect: Rect;
  activeRect: Rect;
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
  isShortLandscape: boolean;
  leftWing: Rect;
  centerStage: Rect;
  rightWing: Rect;
  topLeftSafeRect: Rect;
  topRightSafeRect: Rect;
  turnBarRect: Rect;
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
  riverGrid: Record<RelativeSlot, RiverGridSpec>;
  meldTileW: number;
  meldTileH: number;
  centerSeal: Rect;
  rivers: {
    bottom: Rect;
    top: Rect;
    left: Rect;
    right: Rect;
  };
  riverTroughs: {
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
  personaPockets: {
    top: Rect;
    left: Rect;
    right: Rect;
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

function createTrough(inner: Rect, padX: number, padY: number): Rect {
  return {
    x: inner.x - padX,
    y: inner.y - padY,
    width: inner.width + padX * 2,
    height: inner.height + padY * 2,
  };
}

function computeTopSafeZones(
  width: number,
  margin: number,
  isShortLandscape: boolean,
): {
  topLeftSafeRect: Rect;
  topRightSafeRect: Rect;
  safeBottom: number;
} {
  const safeY = isShortLandscape ? 6 : 8;
  const safeH = isShortLandscape ? 22 : 28;
  const leftW = clamp(Math.round(width * 0.16), 96, 176);
  const rightW = clamp(Math.round(width * 0.26), 144, 256);
  return {
    topLeftSafeRect: { x: margin, y: safeY, width: leftW, height: safeH },
    topRightSafeRect: {
      x: width - margin - rightW,
      y: safeY,
      width: rightW,
      height: safeH,
    },
    safeBottom: safeY + safeH,
  };
}

function computeLandscapeTheaterLayout(
  width: number,
  height: number,
): TableLayout {
  const isShortLandscape = height <= 460;
  const margin = isShortLandscape ? 8 : width >= 1024 ? 14 : 10;
  const colGap = isShortLandscape ? 6 : 10;
  const bottomPad = isShortLandscape ? 6 : 12;

  const { topLeftSafeRect, topRightSafeRect, safeBottom } = computeTopSafeZones(
    width,
    margin,
    isShortLandscape,
  );
  const wingTop = safeBottom + (isShortLandscape ? 4 : 8);
  const theaterH = height - wingTop - bottomPad;

  const wingW =
    width >= 1000
      ? clamp(Math.round(width * 0.19), 188, 256)
      : width >= 700
        ? clamp(Math.round(width * 0.19), 146, 188)
        : clamp(Math.round(width * 0.18), 100, 132);
  const centerW = width - margin * 2 - wingW * 2 - colGap * 2;

  const leftWing: Rect = { x: margin, y: wingTop, width: wingW, height: theaterH };
  const centerStage: Rect = {
    x: margin + wingW + colGap,
    y: wingTop,
    width: centerW,
    height: theaterH,
  };
  const rightWing: Rect = {
    x: centerStage.x + centerW + colGap,
    y: wingTop,
    width: wingW,
    height: theaterH,
  };
  const centerCx = Math.round(centerStage.x + centerW / 2);

  // Viewer 13+1 hand maximized inside centerStage
  const handGap = centerW >= 540 ? 3 : centerW >= 400 ? 2 : 1;
  const drawnGap = centerW >= 540 ? 16 : centerW >= 400 ? 10 : 6;
  const maxHandWByCenter = Math.floor(
    (centerW - 8 - 12 * handGap - drawnGap) / 14,
  );
  const handTileW = clamp(
    Math.min(maxHandWByCenter, Math.round(height * 0.115)),
    Math.min(20, maxHandWByCenter),
    62,
  );
  const handTileH = Math.round(handTileW * 1.36);
  const bottomHandY = height - bottomPad - handTileH;
  const fullHandW = 13 * handTileW + 12 * handGap + drawnGap + handTileW;
  const viewerHandRect: Rect = {
    x: Math.round(centerCx - fullHandW / 2),
    y: bottomHandY,
    width: fullHandW,
    height: handTileH,
  };

  // Turn bar / ActionBar dock centered directly above viewer hand
  const turnBarH = isShortLandscape ? 20 : 28;
  const turnBarGap = isShortLandscape ? 4 : 8;
  const turnBarW = Math.min(
    centerW - 8,
    clamp(Math.round(centerW * 0.62), 180, 420),
  );
  const turnBarRect: Rect = {
    x: Math.round(centerCx - turnBarW / 2),
    y: bottomHandY - turnBarGap - turnBarH,
    width: turnBarW,
    height: turnBarH,
  };

  // Top theater band: 对门 inline badge + concealed hand + #63 top pocket
  const oppGap = OPP_HAND_GAP;
  const oppHandTileW = clamp(
    Math.round(handTileW * 0.44),
    12,
    isShortLandscape ? 16 : 24,
  );
  const oppHandTileH = Math.round(oppHandTileW * 1.36);
  const topOppW = 14 * oppHandTileW + 13 * oppGap;
  const topBandH = Math.max(oppHandTileH, isShortLandscape ? 22 : 28);
  const topBandGap = centerW >= 460 ? 8 : 4;
  const availTopFlank = Math.max(40, centerW - topOppW - topBandGap * 2);
  const topBadgeW = clamp(
    Math.round(availTopFlank * 0.52),
    64,
    160,
  );
  const topPocketW = clamp(
    availTopFlank - topBadgeW,
    40,
    152,
  );
  const topGroupW = topBadgeW + topBandGap + topOppW + topBandGap + topPocketW;
  const topGroupX = Math.round(centerCx - topGroupW / 2);

  const topBadge: Rect = {
    x: topGroupX,
    y: wingTop + Math.round((topBandH - (isShortLandscape ? 20 : 26)) / 2),
    width: topBadgeW,
    height: isShortLandscape ? 20 : 26,
  };
  const topOppRect: Rect = {
    x: topBadge.x + topBadge.width + topBandGap,
    y: wingTop + Math.round((topBandH - oppHandTileH) / 2),
    width: topOppW,
    height: oppHandTileH,
  };
  const topPocket: Rect = {
    x: topOppRect.x + topOppRect.width + topBandGap,
    y: topBadge.y,
    width: topPocketW,
    height: topBadge.height,
  };

  // Wing badges, #63 persona pockets, bottom-left 自家 dashboard, bottom-right melds
  const meldTileW = clamp(Math.round(handTileW * 0.48), 14, 28);
  const meldTileH = Math.round(meldTileW * 1.36);
  const wingBadgeH = isShortLandscape ? 20 : 28;
  const wingPocketH = isShortLandscape ? 20 : 38;
  const wingInnerGap = isShortLandscape ? 3 : 6;
  const viewerCardH = isShortLandscape ? 24 : 34;

  const leftBadge: Rect = {
    x: leftWing.x,
    y: wingTop,
    width: wingW,
    height: wingBadgeH,
  };
  const leftPocket: Rect = {
    x: leftWing.x,
    y: leftBadge.y + leftBadge.height + wingInnerGap,
    width: wingW,
    height: wingPocketH,
  };
  const bottomBadge: Rect = {
    x: leftWing.x,
    y: height - bottomPad - viewerCardH,
    width: wingW,
    height: viewerCardH,
  };

  const rightBadge: Rect = {
    x: rightWing.x,
    y: wingTop,
    width: wingW,
    height: wingBadgeH,
  };
  const rightPocket: Rect = {
    x: rightWing.x,
    y: rightBadge.y + rightBadge.height + wingInnerGap,
    width: wingW,
    height: wingPocketH,
  };

  const bottomMeldAnchor: Point = {
    x: rightWing.x + rightWing.width - 2,
    y: height - bottomPad - meldTileH,
  };

  const wingMidTop = leftPocket.y + leftPocket.height + wingInnerGap + 2;
  const leftMidBottom = bottomBadge.y - wingInnerGap - 2;
  const rightMidBottom = bottomMeldAnchor.y - wingInnerGap - 2;
  const wingMidH = Math.min(leftMidBottom, rightMidBottom) - wingMidTop;

  const maxSideTileByH = Math.floor((wingMidH - 13 * oppGap) / 14);
  const oppSideTileW = clamp(
    Math.min(Math.round(oppHandTileW * 0.85), maxSideTileByH),
    11,
    18,
  );
  const oppSideTileH = Math.round(oppSideTileW * 1.36);
  const sideOppSpan = 14 * oppSideTileW + 13 * oppGap;

  const leftOppRect: Rect = {
    x: leftWing.x,
    y: Math.round(wingMidTop + (leftMidBottom - wingMidTop - sideOppSpan) / 2),
    width: oppSideTileH,
    height: sideOppSpan,
  };
  const rightOppRect: Rect = {
    x: rightWing.x + rightWing.width - oppSideTileH,
    y: Math.round(wingMidTop + (rightMidBottom - wingMidTop - sideOppSpan) / 2),
    width: oppSideTileH,
    height: sideOppSpan,
  };

  // River sizing across centerStage (10/row) and wings (4/col)
  const playTop = wingTop + topBandH + (isShortLandscape ? 4 : 10);
  const playBottom = turnBarRect.y - (isShortLandscape ? 4 : 10);
  const playH = playBottom - playTop;
  const cy = Math.round(playTop + playH / 2);

  const sealH = isShortLandscape ? 22 : 30;
  const sealW = clamp(Math.round(centerW * 0.26), 120, 184);
  const centerSeal: Rect = {
    x: Math.round(centerCx - sealW / 2),
    y: Math.round(cy - sealH / 2),
    width: sealW,
    height: sealH,
  };

  const riverGap = isShortLandscape ? 1 : 2;
  const sealGap = isShortLandscape ? 4 : 8;
  const troughPadX = isShortLandscape ? 4 : 6;
  const troughPadY = isShortLandscape ? 3 : 5;

  const maxLinesV = height >= 640 ? 3 : 2;
  const maxLinesH = width >= 1000 ? 4 : 3;
  const availPerVerticalRiverH = Math.floor(
    (playH - sealH - sealGap * 2 - troughPadY * 4) / 2,
  );
  const maxRiverH_V = Math.floor(
    (availPerVerticalRiverH - (maxLinesV - 1) * riverGap) / maxLinesV,
  );
  const maxRiverW_Center = Math.floor(
    (centerW - troughPadX * 2 - 9 * riverGap - 8) / 10,
  );

  const wingRiverGap = isShortLandscape ? 4 : 8;
  const availWingRiverW = wingW - oppSideTileH - wingRiverGap;
  const maxRiverW_WingV = Math.floor(
    (wingMidH - troughPadY * 2 - 3 * riverGap) / 4,
  );
  const maxRiverH_WingH = Math.floor(
    (availWingRiverW - troughPadX * 2 - (maxLinesH - 1) * riverGap) / maxLinesH,
  );

  const riverTileW = clamp(
    Math.min(
      Math.floor(maxRiverH_V / 1.36),
      maxRiverW_Center,
      maxRiverW_WingV,
      Math.floor(maxRiverH_WingH / 1.36),
    ),
    14,
    34,
  );
  const riverTileH = Math.round(riverTileW * 1.36);

  const wideRiverW = 10 * riverTileW + 9 * riverGap;
  const wideRiverH = maxLinesV * riverTileH + (maxLinesV - 1) * riverGap;
  const topRiverRect: Rect = {
    x: Math.round(centerCx - wideRiverW / 2),
    y: centerSeal.y - sealGap - troughPadY - wideRiverH,
    width: wideRiverW,
    height: wideRiverH,
  };
  const bottomRiverRect: Rect = {
    x: Math.round(centerCx - wideRiverW / 2),
    y: centerSeal.y + centerSeal.height + sealGap + troughPadY,
    width: wideRiverW,
    height: wideRiverH,
  };

  const sideRiverW = maxLinesH * riverTileH + (maxLinesH - 1) * riverGap;
  const sideRiverH = 4 * riverTileW + 3 * riverGap;
  const leftRiverRect: Rect = {
    x: leftWing.x + leftWing.width - troughPadX - sideRiverW,
    y: wingMidTop + troughPadY,
    width: sideRiverW,
    height: sideRiverH,
  };
  const rightRiverRect: Rect = {
    x: rightWing.x + troughPadX,
    y: wingMidTop + troughPadY,
    width: sideRiverW,
    height: sideRiverH,
  };

  const riverTroughs = {
    bottom: createTrough(bottomRiverRect, troughPadX, troughPadY),
    top: createTrough(topRiverRect, troughPadX, troughPadY),
    left: createTrough(leftRiverRect, troughPadX, troughPadY),
    right: createTrough(rightRiverRect, troughPadX, troughPadY),
  };

  return {
    width,
    height,
    cx: centerCx,
    cy,
    margin,
    topMargin: wingTop,
    isWide: true,
    isShortLandscape,
    leftWing,
    centerStage,
    rightWing,
    topLeftSafeRect,
    topRightSafeRect,
    turnBarRect,
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
    riverGrid: {
      bottom: { perLine: 10, maxLines: maxLinesV },
      top: { perLine: 10, maxLines: maxLinesV },
      left: { perLine: 4, maxLines: maxLinesH },
      right: { perLine: 4, maxLines: maxLinesH },
    },
    meldTileW,
    meldTileH,
    centerSeal,
    rivers: {
      bottom: bottomRiverRect,
      top: topRiverRect,
      left: leftRiverRect,
      right: rightRiverRect,
    },
    riverTroughs,
    badges: {
      bottom: bottomBadge,
      right: rightBadge,
      top: topBadge,
      left: leftBadge,
    },
    personaPockets: {
      top: topPocket,
      left: leftPocket,
      right: rightPocket,
    },
    melds: {
      bottom: bottomMeldAnchor,
      right: {
        x: rightOppRect.x - 6,
        y: riverTroughs.right.y + riverTroughs.right.height + 6,
      },
      top: {
        x: centerStage.x + centerStage.width - 2,
        y: riverTroughs.top.y,
      },
      left: {
        x: leftOppRect.x + leftOppRect.width + 6,
        y: riverTroughs.left.y + riverTroughs.left.height + 6,
      },
    },
  };
}

function computePortraitFallbackLayout(
  width: number,
  height: number,
): TableLayout {
  const margin = 8;
  const bottomPad = 10;
  const { topLeftSafeRect, topRightSafeRect, safeBottom } = computeTopSafeZones(
    width,
    margin,
    false,
  );
  const topBandY = safeBottom + 6;
  const topBadgeH = 24;
  const topHalfW = Math.floor((width - margin * 2 - 8) / 2);

  const topBadge: Rect = {
    x: margin,
    y: topBandY,
    width: topHalfW,
    height: topBadgeH,
  };
  const topPocket: Rect = {
    x: width - margin - topHalfW,
    y: topBandY,
    width: topHalfW,
    height: topBadgeH,
  };

  // Viewer 13+1 hand across full portrait width
  const handGap = 1;
  const drawnGap = 6;
  const maxHandW = Math.floor((width - margin * 2 - 12 * handGap - drawnGap) / 14);
  const handTileW = clamp(
    Math.min(maxHandW, Math.round(height * 0.085)),
    Math.min(19, maxHandW),
    36,
  );
  const handTileH = Math.round(handTileW * 1.36);
  const bottomHandY = height - bottomPad - handTileH;
  const fullHandW = 13 * handTileW + 12 * handGap + drawnGap + handTileW;
  const viewerHandRect: Rect = {
    x: Math.round(width / 2 - fullHandW / 2),
    y: bottomHandY,
    width: fullHandW,
    height: handTileH,
  };

  const turnBarH = 24;
  const turnBarW = Math.min(width - margin * 2, clamp(Math.round(width * 0.72), 180, 300));
  const turnBarRect: Rect = {
    x: Math.round(width / 2 - turnBarW / 2),
    y: bottomHandY - 6 - turnBarH,
    width: turnBarW,
    height: turnBarH,
  };

  const meldTileW = clamp(Math.round(handTileW * 0.65), 14, 22);
  const meldTileH = Math.round(meldTileW * 1.36);
  const bottomRowH = Math.max(24, meldTileH);
  const bottomRowY = turnBarRect.y - 6 - bottomRowH;

  const bottomBadge: Rect = {
    x: margin,
    y: bottomRowY + Math.round((bottomRowH - 24) / 2),
    width: clamp(Math.round(width * 0.4), 116, 156),
    height: 24,
  };
  const bottomMeldAnchor: Point = {
    x: width - margin,
    y: bottomRowY,
  };

  // Top opponent concealed row
  const oppGap = OPP_HAND_GAP;
  const oppHandTileW = clamp(Math.round(handTileW * 0.56), 13, 18);
  const oppHandTileH = Math.round(oppHandTileW * 1.36);
  const topOppW = 14 * oppHandTileW + 13 * oppGap;
  const topOppRect: Rect = {
    x: Math.round(width / 2 - topOppW / 2),
    y: topBandY + topBadgeH + 6,
    width: topOppW,
    height: oppHandTileH,
  };

  // Middle three-column theater fallback between topOppRect and bottomRowY
  const playTop = topOppRect.y + topOppRect.height + 6;
  const playBottom = bottomRowY - 6;
  const playH = Math.max(180, playBottom - playTop);
  const colGap = 4;
  const wingW = width >= 370 ? 94 : 84;
  const centerW = width - margin * 2 - wingW * 2 - colGap * 2;

  const leftWing: Rect = { x: margin, y: playTop, width: wingW, height: playH };
  const centerStage: Rect = {
    x: margin + wingW + colGap,
    y: playTop,
    width: centerW,
    height: playH,
  };
  const rightWing: Rect = {
    x: centerStage.x + centerW + colGap,
    y: playTop,
    width: wingW,
    height: playH,
  };
  const cx = Math.round(width / 2);
  const cy = Math.round(playTop + playH / 2);

  const wingBadgeH = 22;
  const wingPocketH = 22;
  const leftBadge: Rect = { x: leftWing.x, y: playTop, width: wingW, height: wingBadgeH };
  const leftPocket: Rect = {
    x: leftWing.x,
    y: playTop + wingBadgeH + 3,
    width: wingW,
    height: wingPocketH,
  };
  const rightBadge: Rect = { x: rightWing.x, y: playTop, width: wingW, height: wingBadgeH };
  const rightPocket: Rect = {
    x: rightWing.x,
    y: playTop + wingBadgeH + 3,
    width: wingW,
    height: wingPocketH,
  };

  const sideTop = leftPocket.y + leftPocket.height + 6;
  const sideAvailH = playBottom - sideTop;
  const oppSideTileW = clamp(
    Math.min(13, Math.floor((sideAvailH - 13 * oppGap) / 14)),
    11,
    15,
  );
  const oppSideTileH = Math.round(oppSideTileW * 1.36);
  const sideOppSpan = 14 * oppSideTileW + 13 * oppGap;

  const leftOppRect: Rect = {
    x: leftWing.x,
    y: sideTop,
    width: oppSideTileH,
    height: sideOppSpan,
  };
  const rightOppRect: Rect = {
    x: rightWing.x + rightWing.width - oppSideTileH,
    y: sideTop,
    width: oppSideTileH,
    height: sideOppSpan,
  };

  const perLineV = width >= 370 ? 8 : 6;
  const maxLinesV = 3;
  const perLineH = 5;
  const maxLinesH = 3;
  const riverGap = 1;
  const troughPadX = 3;
  const troughPadY = 3;
  const sealH = 24;
  const sealW = Math.min(centerW - 4, 152);
  const centerSeal: Rect = {
    x: Math.round(cx - sealW / 2),
    y: Math.round(cy - sealH / 2),
    width: sealW,
    height: sealH,
  };

  const availWingRiverW = wingW - oppSideTileH - 4;
  const maxRiverH_Wing = Math.floor(
    (availWingRiverW - troughPadX * 2 - (maxLinesH - 1) * riverGap) / maxLinesH,
  );
  const maxRiverW_Center = Math.floor(
    (centerW - troughPadX * 2 - (perLineV - 1) * riverGap) / perLineV,
  );
  const availVertRiverH = Math.floor(
    (playH - meldTileH - 10 - sealH - 12 - troughPadY * 4) / 2,
  );
  const maxRiverH_Center = Math.floor(
    (availVertRiverH - (maxLinesV - 1) * riverGap) / maxLinesV,
  );
  const riverTileW = clamp(
    Math.min(
      maxRiverW_Center,
      Math.floor(maxRiverH_Wing / 1.36),
      Math.floor(maxRiverH_Center / 1.36),
    ),
    14,
    20,
  );
  const riverTileH = Math.round(riverTileW * 1.36);

  const wideRiverW = perLineV * riverTileW + (perLineV - 1) * riverGap;
  const wideRiverH = maxLinesV * riverTileH + (maxLinesV - 1) * riverGap;
  const topRiverRect: Rect = {
    x: Math.round(cx - wideRiverW / 2),
    y: centerSeal.y - 6 - troughPadY - wideRiverH,
    width: wideRiverW,
    height: wideRiverH,
  };
  const bottomRiverRect: Rect = {
    x: Math.round(cx - wideRiverW / 2),
    y: centerSeal.y + centerSeal.height + 6 + troughPadY,
    width: wideRiverW,
    height: wideRiverH,
  };

  const sideRiverW = maxLinesH * riverTileH + (maxLinesH - 1) * riverGap;
  const sideRiverH = perLineH * riverTileW + (perLineH - 1) * riverGap;
  const leftRiverRect: Rect = {
    x: leftWing.x + leftWing.width - troughPadX - sideRiverW,
    y: sideTop + troughPadY,
    width: sideRiverW,
    height: sideRiverH,
  };
  const rightRiverRect: Rect = {
    x: rightWing.x + troughPadX,
    y: sideTop + troughPadY,
    width: sideRiverW,
    height: sideRiverH,
  };

  const riverTroughs = {
    bottom: createTrough(bottomRiverRect, troughPadX, troughPadY),
    top: createTrough(topRiverRect, troughPadX, troughPadY),
    left: createTrough(leftRiverRect, troughPadX, troughPadY),
    right: createTrough(rightRiverRect, troughPadX, troughPadY),
  };

  return {
    width,
    height,
    cx,
    cy,
    margin,
    topMargin: topBandY,
    isWide: false,
    isShortLandscape: false,
    leftWing,
    centerStage,
    rightWing,
    topLeftSafeRect,
    topRightSafeRect,
    turnBarRect,
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
    riverGrid: {
      bottom: { perLine: perLineV, maxLines: maxLinesV },
      top: { perLine: perLineV, maxLines: maxLinesV },
      left: { perLine: perLineH, maxLines: maxLinesH },
      right: { perLine: perLineH, maxLines: maxLinesH },
    },
    meldTileW,
    meldTileH,
    centerSeal,
    rivers: {
      bottom: bottomRiverRect,
      top: topRiverRect,
      left: leftRiverRect,
      right: rightRiverRect,
    },
    riverTroughs,
    badges: {
      bottom: bottomBadge,
      right: rightBadge,
      top: topBadge,
      left: leftBadge,
    },
    personaPockets: {
      top: topPocket,
      left: leftPocket,
      right: rightPocket,
    },
    melds: {
      bottom: bottomMeldAnchor,
      right: {
        x: rightOppRect.x - 4,
        y: riverTroughs.right.y + riverTroughs.right.height + 6,
      },
      top: {
        x: centerStage.x + centerStage.width - 2,
        y: playTop,
      },
      left: {
        x: leftOppRect.x + leftOppRect.width + 4,
        y: riverTroughs.left.y + riverTroughs.left.height + 6,
      },
    },
  };
}

/**
 * Computes responsive Scheme B three-column theater layout coordinates for Mac wide and iPhone viewports,
 * with graceful narrow-portrait fallback. Guarantees zero overlap and strict canvas containment.
 */
export function computeTableLayout(
  rawWidth: number,
  rawHeight: number,
): TableLayout {
  const width = Math.max(320, Math.round(rawWidth));
  const height = Math.max(280, Math.round(rawHeight));
  const isWide = width >= height || width >= 600;
  return isWide
    ? computeLandscapeTheaterLayout(width, height)
    : computePortraitFallbackLayout(width, height);
}

/**
 * Computes layout metrics for the ultra-flat rounded pill center seal (`东2 · 牌墙余 N`).
 */
export function computeCenterSealLayout(seal: Rect): CenterSealLayout {
  const pillRadius = seal.height / 2;
  const fontSize = seal.height >= 28 ? 13 : seal.height >= 24 ? 11.5 : 10.5;
  return {
    pillRadius,
    fontSize,
    textY: Math.round(seal.y + seal.height / 2),
    textMaxWidth: Math.max(40, seal.width - 20),
  };
}

/**
 * Computes the four-way directional edge glow bar rects strictly bounded inside `seal`
 * (`top=对门`, `bottom=自家`, `left=上家`, `right=下家`).
 */
export function computeCenterSealEdgeBars(
  seal: Rect,
): Record<RelativeSlot, CenterSealEdgeBar> {
  const idleHorizW = Math.max(28, Math.round(seal.width * 0.56));
  const activeHorizW = Math.min(seal.width - 16, Math.round(seal.width * 0.78));
  const idleHorizH = 2.5;
  const activeHorizH = 4;

  const idleVertH = Math.max(8, Math.round(seal.height * 0.44));
  const activeVertH = Math.min(seal.height - 6, Math.round(seal.height * 0.64));
  const idleVertW = 3;
  const activeVertW = 4.5;

  return {
    top: {
      slot: "top",
      rect: {
        x: Math.round(seal.x + (seal.width - idleHorizW) / 2),
        y: seal.y,
        width: idleHorizW,
        height: idleHorizH,
      },
      activeRect: {
        x: Math.round(seal.x + (seal.width - activeHorizW) / 2),
        y: seal.y,
        width: activeHorizW,
        height: activeHorizH,
      },
    },
    bottom: {
      slot: "bottom",
      rect: {
        x: Math.round(seal.x + (seal.width - idleHorizW) / 2),
        y: seal.y + seal.height - idleHorizH,
        width: idleHorizW,
        height: idleHorizH,
      },
      activeRect: {
        x: Math.round(seal.x + (seal.width - activeHorizW) / 2),
        y: seal.y + seal.height - activeHorizH,
        width: activeHorizW,
        height: activeHorizH,
      },
    },
    left: {
      slot: "left",
      rect: {
        x: seal.x,
        y: Math.round(seal.y + (seal.height - idleVertH) / 2),
        width: idleVertW,
        height: idleVertH,
      },
      activeRect: {
        x: seal.x,
        y: Math.round(seal.y + (seal.height - activeVertH) / 2),
        width: activeVertW,
        height: activeVertH,
      },
    },
    right: {
      slot: "right",
      rect: {
        x: seal.x + seal.width - idleVertW,
        y: Math.round(seal.y + (seal.height - idleVertH) / 2),
        width: idleVertW,
        height: idleVertH,
      },
      activeRect: {
        x: seal.x + seal.width - activeVertW,
        y: Math.round(seal.y + (seal.height - activeVertH) / 2),
        width: activeVertW,
        height: activeVertH,
      },
    },
  };
}

/**
 * Computes the center point and rotation of the `visibleIndex`-th slot in any seat's river
 * using that side's layout-declared `perLine` (10/row top & bottom, 4/col left & right on landscape).
 */
export function computeRiverSlotPoint(
  layout: TableLayout,
  side: RelativeSlot,
  visibleIndex: number,
): RiverSlotPlacement {
  const safeIndex = Math.max(0, Math.floor(visibleIndex));
  const perLine = layout.riverGrid[side].perLine;
  const col = safeIndex % perLine;
  const row = Math.floor(safeIndex / perLine);
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
 * Computes the center point `(x, y)` of the `visibleIndex`-th slot in the bottom player's river.
 * Shared between `tableRenderer.ts` and discard ghost preview for zero-offset alignment.
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
