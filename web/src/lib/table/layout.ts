import {
  NARROW_PORTRAIT_MAX_WIDTH,
  SHORT_LANDSCAPE_MAX_HEIGHT,
} from "../platform/viewportDisplay";
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

export interface SeatBadgeLayout {
  compact: boolean;
  padX: number;
  padY: number;
  cornerRadius: number;
  pillH: number;
  pillFontSize: number;
  rolePillW: number;
  rolePillRect: Rect;
  dealerGap: number;
  dealerPillW: number;
  dealerPillRect: Rect | null;
  scoreFontSize: number;
  scoreRightX: number;
  scoreGap: number;
  nameStartX: number;
  nameFontSize: number;
  nameMaxW: number;
  centerY: number;
}

export const MELD_TILE_GAP = 1;
export const MELD_GROUP_GAP = 8;
const OPP_HAND_GAP = 1;
const TILE_ASPECT = 1.35;

export const QUICK_EMOTE_DOCK_METRICS = {
  compactMaxWidth: 700,
  compactMinWingWidth: 190,
  compactHeight: 64,
  desktopHeight: 84,
  portraitHeight: 44,
  reservePadY: 4,
} as const;

export interface QuickEmoteDockMetricsInput {
  width: number;
  height: number;
  isShortLandscape: boolean;
  rightWingWidth: number;
}

/**
 * Single-sourced compact breakpoint shared by `layout.ts` right-wing reservation
 * and `banterLayout.ts` / `QuickEmoteBar.svelte`.
 */
export function isCompactQuickEmoteViewport(
  input: QuickEmoteDockMetricsInput,
): boolean {
  return (
    input.isShortLandscape ||
    input.height > input.width ||
    input.width < QUICK_EMOTE_DOCK_METRICS.compactMaxWidth ||
    input.rightWingWidth < QUICK_EMOTE_DOCK_METRICS.compactMinWingWidth
  );
}

/**
 * Single-sourced `QuickEmoteBar` dock height (px) shared by `layout.ts` and `banterLayout.ts`.
 */
export function computeQuickEmoteDockHeight(
  input: QuickEmoteDockMetricsInput,
): number {
  if (input.height > input.width) {
    return QUICK_EMOTE_DOCK_METRICS.portraitHeight;
  }
  return isCompactQuickEmoteViewport(input)
    ? QUICK_EMOTE_DOCK_METRICS.compactHeight
    : QUICK_EMOTE_DOCK_METRICS.desktopHeight;
}

const LANDSCAPE_LAYOUT_PROFILE = {
  wingRatioDesktop: 0.145,
  wingMinDesktop: 176,
  wingMaxDesktop: 214,
  wingRatioCompact: 0.132,
  wingMinCompact: 102,
  wingMaxCompact: 128,
  handTileCap: 88,
  handHeightFraction: 0.138,
  handGapDesktop: 3,
  handGapCompact: 1,
  drawnGapDesktop: 18,
  drawnGapCompact: 8,
  riverTileCap: 46,
  meldTileCap: 38,
  perLineVDesktop: 10,
  perLineVCompact: 10,
  perLineHDesktop: 6,
  perLineHCompact: 5,
  maxLinesHDesktop: 2,
  maxLinesHCompact: 2,
} as const;

export interface TableLayoutOptions {
  bottomMeldTileCount?: number;
  bottomMeldGroupCount?: number;
  viewerClosedCount?: number;
  viewerHasDrawn?: boolean;
}

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
  if (max < min) {
    return min;
  }
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

function computeMeldSpan(
  tileCount: number,
  groupCount: number,
  tileW: number,
): number {
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

function computeViewerHandSpan(
  closedCount: number,
  hasDrawn: boolean,
  tileW: number,
  handGap: number,
  drawnGap: number,
): number {
  return (
    closedCount * tileW +
    Math.max(0, closedCount - 1) * handGap +
    (hasDrawn ? drawnGap + tileW : 0)
  );
}

interface NormalizedTableLayoutOptions {
  meldTiles: number;
  meldGroups: number;
  hasDrawn: boolean;
  effectiveClosed: number;
  handTileCount: number;
}

function normalizeTableLayoutOptions(
  options: TableLayoutOptions,
): NormalizedTableLayoutOptions {
  const meldTiles = Math.max(0, Math.floor(options.bottomMeldTileCount ?? 0));
  const meldGroups =
    meldTiles > 0 ? Math.max(1, Math.floor(options.bottomMeldGroupCount ?? 1)) : 0;
  const hasDrawn = options.viewerHasDrawn ?? true;
  const effectiveClosed = clamp(
    Math.floor(options.viewerClosedCount ?? 13),
    hasDrawn ? 0 : 1,
    13,
  );
  return {
    meldTiles,
    meldGroups,
    hasDrawn,
    effectiveClosed,
    handTileCount: effectiveClosed + (hasDrawn ? 1 : 0),
  };
}

interface ViewerHandRectParams {
  centerX: number;
  height: number;
  bottomPad: number;
  handTileW: number;
  handGap: number;
  drawnGap: number;
  closedCount?: number;
  hasDrawn?: boolean;
}

function buildViewerHandRect(params: ViewerHandRectParams): {
  handTileH: number;
  bottomHandY: number;
  viewerHandRect: Rect;
} {
  const {
    centerX,
    height,
    bottomPad,
    handTileW,
    handGap,
    drawnGap,
    closedCount = 13,
    hasDrawn = true,
  } = params;
  const handTileH = Math.round(handTileW * TILE_ASPECT);
  const bottomHandY = height - bottomPad - handTileH;
  const fullHandW = computeViewerHandSpan(
    closedCount,
    hasDrawn,
    handTileW,
    handGap,
    drawnGap,
  );
  return {
    handTileH,
    bottomHandY,
    viewerHandRect: {
      x: Math.round(centerX - fullHandW / 2),
      y: bottomHandY,
      width: fullHandW,
      height: handTileH,
    },
  };
}

interface RiverLayoutBoxesParams {
  centerX: number;
  centerSeal: Rect;
  leftWing: Rect;
  rightWing: Rect;
  sideRiverY: number;
  riverTileW: number;
  riverGap: number;
  perLineV: number;
  maxLinesV: number;
  perLineH: number;
  maxLinesH: number;
  sealGap: number;
  troughPadX: number;
  troughPadY: number;
}

function buildRiverLayoutBoxes(params: RiverLayoutBoxesParams): {
  riverTileH: number;
  rivers: TableLayout["rivers"];
  riverTroughs: TableLayout["riverTroughs"];
  riverGrid: TableLayout["riverGrid"];
} {
  const {
    centerX,
    centerSeal,
    leftWing,
    rightWing,
    sideRiverY,
    riverTileW,
    riverGap,
    perLineV,
    maxLinesV,
    perLineH,
    maxLinesH,
    sealGap,
    troughPadX,
    troughPadY,
  } = params;
  const riverTileH = Math.round(riverTileW * TILE_ASPECT);

  const wideRiverW = perLineV * riverTileW + (perLineV - 1) * riverGap;
  const wideRiverH = maxLinesV * riverTileH + (maxLinesV - 1) * riverGap;
  const top: Rect = {
    x: Math.round(centerX - wideRiverW / 2),
    y: centerSeal.y - sealGap - troughPadY - wideRiverH,
    width: wideRiverW,
    height: wideRiverH,
  };
  const bottom: Rect = {
    x: Math.round(centerX - wideRiverW / 2),
    y: centerSeal.y + centerSeal.height + sealGap + troughPadY,
    width: wideRiverW,
    height: wideRiverH,
  };

  const sideRiverW = maxLinesH * riverTileH + (maxLinesH - 1) * riverGap;
  const sideRiverH = perLineH * riverTileW + (perLineH - 1) * riverGap;
  const left: Rect = {
    x: leftWing.x + leftWing.width - troughPadX - sideRiverW,
    y: sideRiverY,
    width: sideRiverW,
    height: sideRiverH,
  };
  const right: Rect = {
    x: rightWing.x + troughPadX,
    y: sideRiverY,
    width: sideRiverW,
    height: sideRiverH,
  };

  return {
    riverTileH,
    rivers: { bottom, top, left, right },
    riverTroughs: {
      bottom: createTrough(bottom, troughPadX, troughPadY),
      top: createTrough(top, troughPadX, troughPadY),
      left: createTrough(left, troughPadX, troughPadY),
      right: createTrough(right, troughPadX, troughPadY),
    },
    riverGrid: {
      bottom: { perLine: perLineV, maxLines: maxLinesV },
      top: { perLine: perLineV, maxLines: maxLinesV },
      left: { perLine: perLineH, maxLines: maxLinesH },
      right: { perLine: perLineH, maxLines: maxLinesH },
    },
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
  const safeY = isShortLandscape ? 5 : 7;
  const safeH = isShortLandscape ? 20 : 26;
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
  options: TableLayoutOptions = {},
): TableLayout {
  const profile = LANDSCAPE_LAYOUT_PROFILE;
  const isShortLandscape = height <= SHORT_LANDSCAPE_MAX_HEIGHT;
  const margin = isShortLandscape ? 6 : width >= 1024 ? 12 : 8;
  const colGap = isShortLandscape ? 5 : 8;
  const bottomPad = isShortLandscape ? 4 : 10;

  const { topLeftSafeRect, topRightSafeRect, safeBottom } = computeTopSafeZones(
    width,
    margin,
    isShortLandscape,
  );
  const wingTop = safeBottom + (isShortLandscape ? 3 : 6);
  const theaterH = height - wingTop - bottomPad;

  const wingW =
    width >= 1024
      ? clamp(
          Math.round(width * profile.wingRatioDesktop),
          profile.wingMinDesktop,
          profile.wingMaxDesktop,
        )
      : clamp(
          Math.round(width * profile.wingRatioCompact),
          profile.wingMinCompact,
          profile.wingMaxCompact,
        );
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

  // Compact bottom-left 自家 badge & reclaimed bottom corridor extending up to rightWing
  const viewerCardH = isShortLandscape ? 22 : 32;
  const bottomBadgeW = isShortLandscape
    ? clamp(Math.round(wingW * 0.94), 90, 112)
    : clamp(Math.round(wingW * 0.78), 126, 156);
  const bottomBadge: Rect = {
    x: leftWing.x,
    y: height - bottomPad - viewerCardH,
    width: bottomBadgeW,
    height: viewerCardH,
  };

  const bottomSideGap = isShortLandscape ? 6 : 10;
  const bottomCorridorLeft = bottomBadge.x + bottomBadge.width + bottomSideGap;
  const bottomCorridorRight = rightWing.x - bottomSideGap;
  const corridorW = Math.max(120, bottomCorridorRight - bottomCorridorLeft);
  // Floor baseline sizing at 260px so narrow viewports start from readable tile proportions before clamping to corridorW
  const sizingCorridorW = Math.max(260, corridorW);

  const handGap = isShortLandscape
    ? profile.handGapCompact
    : width >= 1024
      ? profile.handGapDesktop
      : Math.max(1, profile.handGapDesktop - 1);
  let drawnGap = isShortLandscape
    ? profile.drawnGapCompact
    : width >= 1024
      ? profile.drawnGapDesktop
      : Math.round(profile.drawnGapDesktop * 0.72);

  const maxHandWByCorridor = Math.floor(
    (sizingCorridorW - 12 * handGap - drawnGap) / 14,
  );
  const maxHandWByHeight = Math.round(
    (height * (isShortLandscape ? 0.185 : profile.handHeightFraction)) /
      TILE_ASPECT,
  );
  let handTileW = clamp(
    Math.min(
      maxHandWByCorridor,
      Math.max(maxHandWByHeight, Math.round(height * 0.095)),
    ),
    Math.min(24, maxHandWByCorridor),
    profile.handTileCap,
  );

  // Meld tile sizing (capped at 38px on desktop, proportional floor on very narrow viewports)
  const minMeldW = Math.min(
    isShortLandscape ? 18 : 22,
    Math.max(12, Math.round(handTileW * 0.6)),
  );
  let meldTileW = clamp(
    Math.round(handTileW * 0.52),
    minMeldW,
    profile.meldTileCap,
  );

  // Dynamic bottom-right meld reservation & strict zero-overlap corridor fit for 0..4 melds
  const { meldTiles, meldGroups, hasDrawn, effectiveClosed, handTileCount } =
    normalizeTableLayoutOptions(options);
  let meldSepGap = meldTiles > 0 ? (isShortLandscape ? 8 : 14) : 0;

  const calcMeldReserve = (mw: number, sep: number): number =>
    meldTiles > 0 ? computeMeldSpan(meldTiles, meldGroups, mw) + sep : 0;

  if (
    computeViewerHandSpan(effectiveClosed, hasDrawn, handTileW, handGap, drawnGap) +
      calcMeldReserve(meldTileW, meldSepGap) >
    corridorW
  ) {
    const fixedHandGaps = computeViewerHandSpan(
      effectiveClosed,
      hasDrawn,
      0,
      handGap,
      drawnGap,
    );
    const availForHand = corridorW - calcMeldReserve(meldTileW, meldSepGap);
    const maxFitHandW = Math.floor((availForHand - fixedHandGaps) / handTileCount);

    if (maxFitHandW >= Math.min(18, handTileW)) {
      handTileW = Math.max(10, Math.min(handTileW, maxFitHandW));
    } else {
      const meldFixedGaps =
        meldTiles > 0 ? computeMeldSpan(meldTiles, meldGroups, 0) + meldSepGap : 0;
      const budget = Math.max(
        handTileCount * 10 + meldTiles * 8,
        corridorW - fixedHandGaps - meldFixedGaps,
      );
      const currentSum = handTileCount * handTileW + meldTiles * meldTileW;
      if (currentSum > 0 && budget < currentSum) {
        const scale = budget / currentSum;
        meldTileW = Math.max(10, Math.floor(meldTileW * scale));
        const remForHand =
          corridorW - calcMeldReserve(meldTileW, meldSepGap) - fixedHandGaps;
        handTileW = Math.max(
          10,
          Math.min(handTileW, Math.floor(remForHand / handTileCount)),
        );
      }
      if (
        computeViewerHandSpan(effectiveClosed, hasDrawn, handTileW, handGap, drawnGap) +
          calcMeldReserve(meldTileW, meldSepGap) >
        corridorW
      ) {
        drawnGap = Math.max(4, Math.min(drawnGap, 6));
        meldSepGap = meldTiles > 0 ? Math.min(meldSepGap, 6) : 0;
        const tightHandGaps = computeViewerHandSpan(
          effectiveClosed,
          hasDrawn,
          0,
          handGap,
          drawnGap,
        );
        const rem =
          corridorW - calcMeldReserve(meldTileW, meldSepGap) - tightHandGaps;
        handTileW = Math.max(8, Math.min(handTileW, Math.floor(rem / handTileCount)));
      }
    }
  }

  const handTileH = Math.round(handTileW * TILE_ASPECT);
  const bottomHandY = height - bottomPad - handTileH;
  const meldTileH = Math.round(meldTileW * TILE_ASPECT);
  const bottomMeldTotalW = calcMeldReserve(meldTileW, meldSepGap);
  const bottomMeldAnchor: Point = {
    x: rightWing.x - bottomSideGap,
    y: height - bottomPad - meldTileH,
  };

  // Center viewer hand inside reclaimed bottom corridor (yielding right width to bottom melds)
  const fullHandW = computeViewerHandSpan(
    effectiveClosed,
    hasDrawn,
    handTileW,
    handGap,
    drawnGap,
  );
  const availableHandRight = bottomCorridorRight - bottomMeldTotalW;
  const idealX = Math.round(centerCx - fullHandW / 2);
  const viewerHandX = clamp(
    idealX,
    bottomCorridorLeft,
    Math.max(bottomCorridorLeft, availableHandRight - fullHandW),
  );
  const viewerHandRect: Rect = {
    x: viewerHandX,
    y: bottomHandY,
    width: fullHandW,
    height: handTileH,
  };
  const handCenterX = Math.round(viewerHandRect.x + viewerHandRect.width / 2);

  // Turn bar / ActionBar dock follows viewer hand center while clamped strictly inside centerStage
  const turnBarH = isShortLandscape ? 19 : 26;
  const turnBarGap = isShortLandscape ? 4 : 7;
  const turnBarW = Math.min(
    centerW - 8,
    clamp(Math.round(centerW * 0.62), 180, 440),
  );
  const turnBarMinX = centerStage.x + 4;
  const turnBarMaxX = Math.max(
    turnBarMinX,
    centerStage.x + centerStage.width - 4 - turnBarW,
  );
  const turnBarRect: Rect = {
    x: clamp(Math.round(handCenterX - turnBarW / 2), turnBarMinX, turnBarMaxX),
    y: bottomHandY - turnBarGap - turnBarH,
    width: turnBarW,
    height: turnBarH,
  };

  // Top theater band: 对门 inline badge + concealed hand + persona pocket
  const oppGap = OPP_HAND_GAP;
  const oppHandTileW = clamp(
    Math.round(handTileW * 0.36),
    12,
    isShortLandscape ? 16 : 24,
  );
  const oppHandTileH = Math.round(oppHandTileW * TILE_ASPECT);
  const topOppW = 14 * oppHandTileW + 13 * oppGap;
  const topBandH = Math.max(oppHandTileH, isShortLandscape ? 20 : 28);
  const topBandGap = centerW >= 460 ? 8 : 4;
  const availTopFlank = Math.max(40, centerW - topOppW - topBandGap * 2);
  const topBadgeW = clamp(Math.round(availTopFlank * 0.52), 64, 160);
  const topPocketW = clamp(availTopFlank - topBadgeW, 44, 152);
  const topGroupW = topBadgeW + topBandGap + topOppW + topBandGap + topPocketW;
  const topGroupX = Math.round(centerCx - topGroupW / 2);

  const topBadge: Rect = {
    x: topGroupX,
    y: wingTop + Math.round((topBandH - (isShortLandscape ? 19 : 26)) / 2),
    width: topBadgeW,
    height: isShortLandscape ? 19 : 26,
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

  // Wing badges & persona pockets (strictly preserved at top of wings, never occluded)
  const wingBadgeH = isShortLandscape ? 19 : 26;
  const wingPocketH = isShortLandscape ? 20 : 36;
  const wingInnerGap = isShortLandscape ? 3 : 5;

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

  // QuickEmoteBar sits at bottom of rightWing (single-sourced with banterLayout.ts)
  const quickEmoteReserveH =
    computeQuickEmoteDockHeight({
      width,
      height,
      isShortLandscape,
      rightWingWidth: wingW,
    }) + QUICK_EMOTE_DOCK_METRICS.reservePadY;
  const wingMidTop = leftPocket.y + leftPocket.height + wingInnerGap + 2;
  const leftMidBottom = bottomBadge.y - wingInnerGap - 2;
  const rightMidBottom = height - bottomPad - quickEmoteReserveH - wingInnerGap;
  const wingMidH = Math.max(
    96,
    Math.min(leftMidBottom, rightMidBottom) - wingMidTop,
  );

  const maxSideTileByH = Math.floor((wingMidH - 13 * oppGap) / 14);
  const oppSideTileW = clamp(
    Math.min(Math.round(oppHandTileW * 0.8), maxSideTileByH),
    10,
    17,
  );
  const oppSideTileH = Math.round(oppSideTileW * TILE_ASPECT);
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

  // Dedicated top meld strip strictly taller than meldTileH so top melds never touch riverTroughs.top
  const topMeldY = wingTop + topBandH + (isShortLandscape ? 3 : 7);
  const topMeldBandH = meldTileH + (isShortLandscape ? 3 : 6);

  // River sizing across centerStage (10/row) and wings (6/col desktop, 5/col compact, 2 cols)
  const playTop = topMeldY + topMeldBandH;
  const playBottom = turnBarRect.y - (isShortLandscape ? 3 : 7);
  const playH = Math.max(60, playBottom - playTop);
  const cy = Math.round(playTop + playH / 2);

  const sealH = isShortLandscape ? 20 : 28;
  const sealW = clamp(Math.round(centerW * 0.24), 116, 178);
  const centerSeal: Rect = {
    x: Math.round(centerCx - sealW / 2),
    y: Math.round(cy - sealH / 2),
    width: sealW,
    height: sealH,
  };

  const riverGap = isShortLandscape ? 1 : 2;
  const sealGap = isShortLandscape ? 3 : 6;
  const troughPadX = isShortLandscape ? 3 : 5;
  const troughPadY = isShortLandscape ? 2 : 4;
  const sideMeldGap = isShortLandscape ? 4 : 6;

  const perLineV = isShortLandscape
    ? profile.perLineVCompact
    : profile.perLineVDesktop;
  const maxLinesV = height >= 640 ? 3 : 2;
  const perLineH = isShortLandscape
    ? profile.perLineHCompact
    : profile.perLineHDesktop;
  const maxLinesH = isShortLandscape
    ? profile.maxLinesHCompact
    : profile.maxLinesHDesktop;

  const availPerVerticalRiverH = Math.floor(
    (playH - sealH - sealGap * 2 - troughPadY * 4) / 2,
  );
  const maxRiverH_V = Math.floor(
    (availPerVerticalRiverH - (maxLinesV - 1) * riverGap) / maxLinesV,
  );
  const maxRiverW_Center = Math.floor(
    (centerW - troughPadX * 2 - (perLineV - 1) * riverGap - 8) / perLineV,
  );

  const wingRiverGap = isShortLandscape ? 3 : 6;
  const availWingRiverW = wingW - oppSideTileH - wingRiverGap;
  // Reserve vertical space in wings so side melds below side river troughs stay above QuickEmoteBar
  const sampleSideMeldH = computeMeldSpan(3, 1, meldTileW);
  const availSideRiverH = Math.max(
    perLineH * 14,
    wingMidH - troughPadY * 2 - sideMeldGap - sampleSideMeldH,
  );
  const maxRiverW_WingV = Math.floor(
    (availSideRiverH - (perLineH - 1) * riverGap) / perLineH,
  );
  const maxRiverH_WingH = Math.floor(
    (availWingRiverW - troughPadX * 2 - (maxLinesH - 1) * riverGap) / maxLinesH,
  );

  const rawMaxRiverW = Math.min(
    Math.floor(maxRiverH_V / TILE_ASPECT),
    maxRiverW_Center,
    maxRiverW_WingV,
    Math.floor(maxRiverH_WingH / TILE_ASPECT),
  );
  const riverTileW = clamp(
    rawMaxRiverW,
    Math.min(15, Math.max(12, rawMaxRiverW)),
    profile.riverTileCap,
  );
  const { riverTileH, rivers, riverTroughs, riverGrid } = buildRiverLayoutBoxes({
    centerX: centerCx,
    centerSeal,
    leftWing,
    rightWing,
    sideRiverY: wingMidTop + troughPadY,
    riverTileW,
    riverGap,
    perLineV,
    maxLinesV,
    perLineH,
    maxLinesH,
    sealGap,
    troughPadX,
    troughPadY,
  });

  return {
    width,
    height,
    // Keep `cx` at table center so feltBackdrop spotlight stays centered regardless of viewer melds
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
    riverGrid,
    meldTileW,
    meldTileH,
    centerSeal,
    rivers,
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
        x: rightOppRect.x - 5,
        y: riverTroughs.right.y + riverTroughs.right.height + sideMeldGap,
      },
      top: {
        x: centerStage.x + centerStage.width - 2,
        y: topMeldY,
      },
      left: {
        x: leftOppRect.x + leftOppRect.width + 5,
        y: riverTroughs.left.y + riverTroughs.left.height + sideMeldGap,
      },
    },
  };
}

function computePortraitFallbackLayout(
  width: number,
  height: number,
  options: TableLayoutOptions = {},
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

  // Viewer hand across full portrait width
  const handGap = 1;
  const drawnGap = 6;
  const maxHandW = Math.floor((width - margin * 2 - 12 * handGap - drawnGap) / 14);
  const handTileW = clamp(
    Math.min(maxHandW, Math.round(height * 0.085)),
    Math.min(19, maxHandW),
    36,
  );
  const { meldTiles, meldGroups, hasDrawn, effectiveClosed } =
    normalizeTableLayoutOptions(options);
  const { handTileH, bottomHandY, viewerHandRect } = buildViewerHandRect({
    centerX: width / 2,
    height,
    bottomPad,
    handTileW,
    handGap,
    drawnGap,
    closedCount: effectiveClosed,
    hasDrawn,
  });

  const turnBarH = 24;
  const turnBarW = Math.min(width - margin * 2, clamp(Math.round(width * 0.72), 180, 300));
  const turnBarRect: Rect = {
    x: Math.round(width / 2 - turnBarW / 2),
    y: bottomHandY - 6 - turnBarH,
    width: turnBarW,
    height: turnBarH,
  };

  let meldTileW = clamp(Math.round(handTileW * 0.65), 14, 22);
  let bottomBadgeW = clamp(Math.round(width * 0.4), 116, 156);
  if (meldTiles > 0) {
    const portraitMeldGap = 6;
    const rawSpan = computeMeldSpan(meldTiles, meldGroups, meldTileW);
    const availForMeld = width - margin * 2 - bottomBadgeW - portraitMeldGap;
    if (rawSpan > availForMeld) {
      const minBadgeW = 88;
      const maxMeldW = width - margin * 2 - minBadgeW - portraitMeldGap;
      if (rawSpan <= maxMeldW) {
        bottomBadgeW = width - margin * 2 - portraitMeldGap - rawSpan;
      } else {
        bottomBadgeW = minBadgeW;
        const fixedMeldGaps = computeMeldSpan(meldTiles, meldGroups, 0);
        meldTileW = Math.max(
          8,
          Math.floor((maxMeldW - fixedMeldGaps) / meldTiles),
        );
      }
    }
  }
  const meldTileH = Math.round(meldTileW * TILE_ASPECT);
  const bottomRowH = Math.max(24, meldTileH);
  const bottomRowY = turnBarRect.y - 6 - bottomRowH;

  const bottomBadge: Rect = {
    x: margin,
    y: bottomRowY + Math.round((bottomRowH - 24) / 2),
    width: bottomBadgeW,
    height: 24,
  };
  const bottomMeldAnchor: Point = {
    x: width - margin,
    y: bottomRowY,
  };

  // Top opponent concealed row
  const oppGap = OPP_HAND_GAP;
  const oppHandTileW = clamp(Math.round(handTileW * 0.56), 13, 18);
  const oppHandTileH = Math.round(oppHandTileW * TILE_ASPECT);
  const topOppW = 14 * oppHandTileW + 13 * oppGap;
  const topOppRect: Rect = {
    x: Math.round(width / 2 - topOppW / 2),
    y: topBandY + topBadgeH + 6,
    width: topOppW,
    height: oppHandTileH,
  };

  // Middle three-column theater fallback between dedicated top meld strip and bottomRowY
  const topMeldY = topOppRect.y + topOppRect.height + 4;
  const playTop = topMeldY + meldTileH + 6;
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
  const oppSideTileH = Math.round(oppSideTileW * TILE_ASPECT);
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
      Math.floor(maxRiverH_Wing / TILE_ASPECT),
      Math.floor(maxRiverH_Center / TILE_ASPECT),
    ),
    14,
    20,
  );
  const { riverTileH, rivers, riverTroughs, riverGrid } = buildRiverLayoutBoxes({
    centerX: cx,
    centerSeal,
    leftWing,
    rightWing,
    sideRiverY: sideTop + troughPadY,
    riverTileW,
    riverGap,
    perLineV,
    maxLinesV,
    perLineH,
    maxLinesH,
    sealGap: 6,
    troughPadX,
    troughPadY,
  });

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
    riverGrid,
    meldTileW,
    meldTileH,
    centerSeal,
    rivers,
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
        x: width - margin,
        y: topMeldY,
      },
      left: {
        x: leftOppRect.x + leftOppRect.width + 4,
        y: riverTroughs.left.y + riverTroughs.left.height + 6,
      },
    },
  };
}

/**
 * Computes responsive cross-column enlarged theater layout coordinates for Mac wide and iPhone viewports
 * (ADR-0013), with graceful narrow-portrait fallback. Guarantees zero overlap and strict canvas containment.
 */
export function computeTableLayout(
  rawWidth: number,
  rawHeight: number,
  options: TableLayoutOptions = {},
): TableLayout {
  const width = Math.max(320, Math.round(rawWidth));
  const height = Math.max(280, Math.round(rawHeight));
  const isWide = width >= height || width >= NARROW_PORTRAIT_MAX_WIDTH;
  return isWide
    ? computeLandscapeTheaterLayout(width, height, options)
    : computePortraitFallbackLayout(width, height, options);
}

/**
 * Computes inner pill, dealer chip, score, and name-width metrics for a seat badge rectangle.
 */
export function computeSeatBadgeLayout(
  rect: Rect,
  isDealer: boolean,
  scoreWidth = 0,
): SeatBadgeLayout {
  const compact = rect.width < 120 || rect.height < 24;
  const padX = compact ? 4 : 7;
  const padY = compact ? 3 : 4;
  const cornerRadius = compact ? 7 : 9;
  const pillH = Math.max(12, rect.height - padY * 2);
  const pillFontSize = compact ? 9 : 11;
  const rolePillW = compact ? 24 : 32;
  const rolePillRect: Rect = {
    x: rect.x + padX,
    y: rect.y + padY,
    width: rolePillW,
    height: pillH,
  };
  const dealerGap = compact ? 3 : 4;
  const dealerPillW = compact ? 14 : 19;
  const dealerX = rolePillRect.x + rolePillW + dealerGap;
  const dealerPillRect: Rect | null = isDealer
    ? {
        x: dealerX,
        y: rect.y + padY,
        width: dealerPillW,
        height: pillH,
      }
    : null;
  const leftCursor = dealerPillRect
    ? dealerPillRect.x + dealerPillW
    : rolePillRect.x + rolePillW;
  const scoreFontSize = compact ? 10 : 12;
  const scoreRightX = rect.x + rect.width - padX;
  const scoreGap = compact ? 3 : 5;
  const nameStartX = leftCursor + (compact ? 4 : 6);
  const nameFontSize = compact ? 10 : 12;
  const nameMaxW = Math.max(
    8,
    scoreRightX - scoreWidth - scoreGap - nameStartX,
  );
  return {
    compact,
    padX,
    padY,
    cornerRadius,
    pillH,
    pillFontSize,
    rolePillW,
    rolePillRect,
    dealerGap,
    dealerPillW,
    dealerPillRect,
    scoreFontSize,
    scoreRightX,
    scoreGap,
    nameStartX,
    nameFontSize,
    nameMaxW,
    centerY: rect.y + rect.height / 2,
  };
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
 * using that side's layout-declared `perLine` (10/row top & bottom, 6/col desktop or 5/col short landscape on left & right).
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

  const closedW = computeViewerHandSpan(safeClosed, false, tw, gap, drawnGap);
  const fullW = computeViewerHandSpan(safeClosed, hasDrawn, tw, gap, drawnGap);
  const handCenter = layout.viewerHandRect.x + layout.viewerHandRect.width / 2;
  const startX = Math.round(handCenter - fullW / 2);

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
