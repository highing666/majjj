// PROTOTYPE (throwaway for #78): Enlarged responsive table layout engine across 3 structurally
// distinct variants (`a`, `b`, `c`). Reclaims bottom horizontal width for `自家` hand, dynamically
// yields bottom-right width for `自家` melds, lifts CSS px tile caps (`handTileW <= 88..92`,
// `riverTileW <= 46..50`, `meldTileW <= 38..42`), and preserves strict non-occlusion of
// `TablePersonaPockets`, `BanterBubbles`, and `QuickEmoteBar`.

import {
  NARROW_PORTRAIT_MAX_WIDTH,
  SHORT_LANDSCAPE_MAX_HEIGHT,
} from "../../platform/viewportDisplay";
import {
  computeTableLayout,
  MELD_GROUP_GAP,
  MELD_TILE_GAP,
  type Point,
  type Rect,
  type TableLayout,
} from "../layout";
import type { PrototypeVariantId } from "./variants";

export interface PrototypeLayoutOptions {
  bottomMeldTileCount?: number;
  bottomMeldGroupCount?: number;
  viewerClosedCount?: number;
  viewerHasDrawn?: boolean;
}

interface VariantLayoutProfile {
  wingRatioDesktop: number;
  wingMinDesktop: number;
  wingMaxDesktop: number;
  wingRatioCompact: number;
  wingMinCompact: number;
  wingMaxCompact: number;
  handTileCap: number;
  handHeightFraction: number;
  handGapDesktop: number;
  handGapCompact: number;
  drawnGapDesktop: number;
  drawnGapCompact: number;
  riverTileCap: number;
  meldTileCap: number;
  perLineVDesktop: number;
  perLineVCompact: number;
  perLineHDesktop: number;
  perLineHCompact: number;
  maxLinesHDesktop: number;
  maxLinesHCompact: number;
}

const VARIANT_PROFILES: Record<PrototypeVariantId, VariantLayoutProfile> = {
  // Variant A: Balanced wide-hand theater, 10-col center rivers (3-col side rivers)
  a: {
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
  },
  // Variant B: Slim-wing grand center stage — taller sculpted rivers (9-col) & spacious hand gaps
  b: {
    wingRatioDesktop: 0.138,
    wingMinDesktop: 168,
    wingMaxDesktop: 202,
    wingRatioCompact: 0.128,
    wingMinCompact: 100,
    wingMaxCompact: 124,
    handTileCap: 86,
    handHeightFraction: 0.134,
    handGapDesktop: 4,
    handGapCompact: 2,
    drawnGapDesktop: 22,
    drawnGapCompact: 10,
    riverTileCap: 48,
    meldTileCap: 40,
    perLineVDesktop: 9,
    perLineVCompact: 9,
    perLineHDesktop: 6,
    perLineHCompact: 5,
    maxLinesHDesktop: 2,
    maxLinesHCompact: 2,
  },
  // Variant C: Competition max-hand & speed-reading 8-col huge river (tuned for iPhone landscape!)
  c: {
    wingRatioDesktop: 0.126,
    wingMinDesktop: 152,
    wingMaxDesktop: 184,
    wingRatioCompact: 0.118,
    wingMinCompact: 92,
    wingMaxCompact: 114,
    handTileCap: 92,
    handHeightFraction: 0.152,
    handGapDesktop: 2,
    handGapCompact: 1,
    drawnGapDesktop: 14,
    drawnGapCompact: 7,
    riverTileCap: 50,
    meldTileCap: 42,
    perLineVDesktop: 8,
    perLineVCompact: 8,
    perLineHDesktop: 6,
    perLineHCompact: 5,
    maxLinesHDesktop: 2,
    maxLinesHCompact: 2,
  },
};

const TILE_ASPECT = 1.35; // matches 184x248 high-res prototype frame (248/184 = 1.3478)
const OPP_HAND_GAP = 1;

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

export function computePrototypeTableLayout(
  rawWidth: number,
  rawHeight: number,
  variant: PrototypeVariantId,
  options: PrototypeLayoutOptions = {},
): TableLayout {
  const width = Math.max(320, Math.round(rawWidth));
  const height = Math.max(280, Math.round(rawHeight));
  const isWide = width >= height || width >= NARROW_PORTRAIT_MAX_WIDTH;
  if (!isWide) {
    return computeTableLayout(width, height);
  }

  const profile = VARIANT_PROFILES[variant];
  const isShortLandscape = height <= SHORT_LANDSCAPE_MAX_HEIGHT;
  const margin = isShortLandscape ? 6 : width >= 1024 ? 12 : 8;
  const colGap = isShortLandscape ? 5 : 8;
  const bottomPad = isShortLandscape ? 4 : 10;

  // Top safe HUD zones (glass-top-left / glass-top-right)
  const safeY = isShortLandscape ? 5 : 7;
  const safeH = isShortLandscape ? 20 : 26;
  const topLeftSafeRect: Rect = {
    x: margin,
    y: safeY,
    width: clamp(Math.round(width * 0.16), 96, 176),
    height: safeH,
  };
  const topRightW = clamp(Math.round(width * 0.26), 144, 256);
  const topRightSafeRect: Rect = {
    x: width - margin - topRightW,
    y: safeY,
    width: topRightW,
    height: safeH,
  };
  const wingTop = safeY + safeH + (isShortLandscape ? 3 : 6);
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

  // Compact bottom-left 自家 badge & bottom-right QuickEmoteBar clearance
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

  // Reclaim bottom horizontal corridor beyond centerStage!
  // Left boundary: right of compact bottomBadge.
  // Right boundary: left of QuickEmoteBar (which docks inside rightWing) minus dynamic bottom melds.
  const bottomSideGap = isShortLandscape ? 6 : 10;
  const bottomCorridorLeft = bottomBadge.x + bottomBadge.width + bottomSideGap;
  const bottomCorridorRight = rightWing.x - bottomSideGap;
  const totalBottomCorridorW = Math.max(
    260,
    bottomCorridorRight - bottomCorridorLeft,
  );

  // Estimate hand tile width for full 13+1 hand (stable tile size regardless of meld count!)
  const handGap = isShortLandscape
    ? profile.handGapCompact
    : width >= 1024
      ? profile.handGapDesktop
      : Math.max(1, profile.handGapDesktop - 1);
  const drawnGap = isShortLandscape
    ? profile.drawnGapCompact
    : width >= 1024
      ? profile.drawnGapDesktop
      : Math.round(profile.drawnGapDesktop * 0.72);

  const maxHandWByCorridor = Math.floor(
    (totalBottomCorridorW - 12 * handGap - drawnGap) / 14,
  );
  const maxHandWByHeight = Math.round(
    (height * (isShortLandscape ? 0.185 : profile.handHeightFraction)) /
      TILE_ASPECT,
  );
  // Keep desktop hand generously large (~72-82px on 13"/14" MacBook) and iPhone (~42-46px)
  const handTileW = clamp(
    Math.min(maxHandWByCorridor, Math.max(maxHandWByHeight, Math.round(height * 0.095))),
    Math.min(24, maxHandWByCorridor),
    profile.handTileCap,
  );
  const handTileH = Math.round(handTileW * TILE_ASPECT);
  const bottomHandY = height - bottomPad - handTileH;

  // Meld tile sizing (38-42px cap on desktop, proportionally scaled on iPhone)
  const meldTileW = clamp(
    Math.round(handTileW * 0.52),
    isShortLandscape ? 18 : 22,
    profile.meldTileCap,
  );
  const meldTileH = Math.round(meldTileW * TILE_ASPECT);

  // Dynamic bottom-right meld reservation based on actual meld count!
  const meldTiles = Math.max(0, options.bottomMeldTileCount ?? 0);
  const meldGroups = Math.max(0, options.bottomMeldGroupCount ?? 0);
  const bottomMeldTotalW =
    meldTiles > 0
      ? meldTiles * meldTileW +
        Math.max(0, meldTiles - meldGroups) * MELD_TILE_GAP +
        Math.max(0, meldGroups - 1) * MELD_GROUP_GAP +
        (isShortLandscape ? 8 : 14)
      : 0;

  const bottomMeldAnchor: Point = {
    x: rightWing.x - bottomSideGap,
    y: height - bottomPad - meldTileH,
  };

  // Center the viewer's hand inside the reclaimed bottom corridor (yielding right space if melds exist)
  const effectiveClosed = clamp(options.viewerClosedCount ?? 13, 1, 13);
  const hasDrawn = options.viewerHasDrawn ?? true;
  const fullHandW =
    effectiveClosed * handTileW +
    Math.max(0, effectiveClosed - 1) * handGap +
    (hasDrawn ? drawnGap + handTileW : 0);
  const availableHandRight = bottomCorridorRight - bottomMeldTotalW;
  const handCorridorCx = Math.round(
    (bottomCorridorLeft + availableHandRight) / 2,
  );
  // Prefer visual centerCx when it fits cleanly inside the corridor without colliding with badge or melds
  const idealX = Math.round(centerCx - fullHandW / 2);
  const clampedHandX = clamp(
    idealX,
    bottomCorridorLeft,
    Math.max(bottomCorridorLeft, availableHandRight - fullHandW),
  );
  const viewerHandX =
    availableHandRight - bottomCorridorLeft >= fullHandW
      ? clampedHandX
      : Math.round(handCorridorCx - fullHandW / 2);

  const viewerHandRect: Rect = {
    x: viewerHandX,
    y: bottomHandY,
    width: fullHandW,
    height: handTileH,
  };
  const handCenterX = Math.round(viewerHandRect.x + viewerHandRect.width / 2);

  // Turn bar / ActionBar dock centered directly above viewer hand
  const turnBarH = isShortLandscape ? 19 : 26;
  const turnBarGap = isShortLandscape ? 4 : 7;
  const turnBarW = Math.min(
    centerW - 8,
    clamp(Math.round(centerW * 0.62), 180, 440),
  );
  const turnBarRect: Rect = {
    x: Math.round(handCenterX - turnBarW / 2),
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

  // Wing badges & #63 persona pockets (strictly preserved at top of wings, never occluded)
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

  // QuickEmoteBar sits at bottom of rightWing (64px compact / 84px desktop)
  const quickEmoteReserveH =
    isShortLandscape || width < 700 || wingW < 190 ? 68 : 88;
  const wingMidTop = leftPocket.y + leftPocket.height + wingInnerGap + 2;
  const leftMidBottom = bottomBadge.y - wingInnerGap - 2;
  const rightMidBottom = height - bottomPad - quickEmoteReserveH - wingInnerGap;
  const wingMidH = Math.max(96, Math.min(leftMidBottom, rightMidBottom) - wingMidTop);

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

  // Dedicated top meld strip inside centerStage above riverTroughs.top
  const topMeldY = wingTop + topBandH + (isShortLandscape ? 3 : 7);
  const topMeldBandH = Math.min(meldTileH, isShortLandscape ? 26 : 44) + (isShortLandscape ? 3 : 6);

  // River sizing — lifted caps (46..50px) with variant-specific perLine density
  const playTop = topMeldY + topMeldBandH;
  const playBottom = turnBarRect.y - (isShortLandscape ? 3 : 7);
  const playH = Math.max(100, playBottom - playTop);
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
  const maxRiverW_WingV = Math.floor(
    (wingMidH - troughPadY * 2 - (perLineH - 1) * riverGap) / perLineH,
  );
  const maxRiverH_WingH = Math.floor(
    (availWingRiverW - troughPadX * 2 - (maxLinesH - 1) * riverGap) / maxLinesH,
  );

  const riverTileW = clamp(
    Math.min(
      Math.floor(maxRiverH_V / TILE_ASPECT),
      maxRiverW_Center,
      maxRiverW_WingV,
      Math.floor(maxRiverH_WingH / TILE_ASPECT),
    ),
    15,
    profile.riverTileCap,
  );
  const riverTileH = Math.round(riverTileW * TILE_ASPECT);

  const wideRiverW = perLineV * riverTileW + (perLineV - 1) * riverGap;
  const wideRiverH = maxLinesV * riverTileH + (maxLinesV - 1) * riverGap;
  const topRiver: Rect = {
    x: Math.round(centerCx - wideRiverW / 2),
    y: centerSeal.y - sealGap - troughPadY - wideRiverH,
    width: wideRiverW,
    height: wideRiverH,
  };
  const bottomRiver: Rect = {
    x: Math.round(centerCx - wideRiverW / 2),
    y: centerSeal.y + centerSeal.height + sealGap + troughPadY,
    width: wideRiverW,
    height: wideRiverH,
  };

  const sideRiverW = maxLinesH * riverTileH + (maxLinesH - 1) * riverGap;
  const sideRiverH = perLineH * riverTileW + (perLineH - 1) * riverGap;
  const sideRiverY = wingMidTop + troughPadY;
  const leftRiver: Rect = {
    x: leftWing.x + leftWing.width - troughPadX - sideRiverW,
    y: sideRiverY,
    width: sideRiverW,
    height: sideRiverH,
  };
  const rightRiver: Rect = {
    x: rightWing.x + troughPadX,
    y: sideRiverY,
    width: sideRiverW,
    height: sideRiverH,
  };

  return {
    width,
    height,
    // Keep `cx` aligned with viewer hand center so `computeViewerHandSlots` positions
    // hand tiles cleanly inside the reclaimed bottom corridor without touching badges/melds.
    cx: handCenterX,
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
      bottom: { perLine: perLineV, maxLines: maxLinesV },
      top: { perLine: perLineV, maxLines: maxLinesV },
      left: { perLine: perLineH, maxLines: maxLinesH },
      right: { perLine: perLineH, maxLines: maxLinesH },
    },
    meldTileW,
    meldTileH,
    centerSeal,
    rivers: {
      bottom: bottomRiver,
      top: topRiver,
      left: leftRiver,
      right: rightRiver,
    },
    riverTroughs: {
      bottom: createTrough(bottomRiver, troughPadX, troughPadY),
      top: createTrough(topRiver, troughPadX, troughPadY),
      left: createTrough(leftRiver, troughPadX, troughPadY),
      right: createTrough(rightRiver, troughPadX, troughPadY),
    },
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
        y: rightRiver.y + rightRiver.height + troughPadY + 6,
      },
      top: {
        x: centerStage.x + centerStage.width - 2,
        y: topMeldY,
      },
      left: {
        x: leftOppRect.x + leftOppRect.width + 5,
        y: leftRiver.y + leftRiver.height + troughPadY + 6,
      },
    },
  };
}
