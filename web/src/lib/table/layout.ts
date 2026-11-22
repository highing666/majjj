export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

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
  oppHandTileW: number;
  oppHandTileH: number;
  riverTileW: number;
  riverTileH: number;
  riverGap: number;
  meldTileW: number;
  meldTileH: number;
  hub: Rect;
  rivers: {
    bottom: Rect;
    top: Rect;
    left: Rect;
    right: Rect;
  };
  badges: {
    bottom: { x: number; y: number };
    right: { x: number; y: number };
    top: { x: number; y: number };
    left: { x: number; y: number };
  };
  melds: {
    bottom: { x: number; y: number };
    right: { x: number; y: number };
    top: { x: number; y: number };
    left: { x: number; y: number };
  };
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/**
 * Computes responsive, non-overlapping 2D table coordinates for desktop Mac and iPhone viewports.
 */
export function computeTableLayout(
  rawWidth: number,
  rawHeight: number,
): TableLayout {
  const width = Math.max(320, Math.round(rawWidth));
  const height = Math.max(280, Math.round(rawHeight));

  const isWide = width >= 640;
  const handGap = isWide ? 3 : 1;
  const drawnGap = isWide ? 14 : 7;
  const margin = isWide ? 20 : 10;

  const maxHandWByWidth = Math.floor(
    (width - margin * 2 - 12 * handGap - drawnGap) / 14,
  );
  const handTileW = clamp(
    Math.min(maxHandWByWidth, Math.round(height * 0.1)),
    21,
    50,
  );
  const handTileH = Math.round(handTileW * 1.38);
  const bottomPad = height >= 500 ? 14 : 8;
  const bottomHandY = height - handTileH - bottomPad;

  const oppHandTileW = clamp(Math.round(handTileW * 0.54), 14, 25);
  const oppHandTileH = Math.round(oppHandTileW * 1.38);

  const meldTileW = clamp(Math.round(handTileW * 0.58), 15, 28);
  const meldTileH = Math.round(meldTileW * 1.38);

  const topMargin = height >= 500 ? 14 : 8;
  const topOccupiedH = topMargin + oppHandTileH + 14;
  const playTop = topOccupiedH;
  const playBottom = bottomHandY - 8;
  const playH = Math.max(120, playBottom - playTop);

  const hubW = clamp(Math.round(Math.min(width, playH) * 0.28), 74, 124);
  const hubH = clamp(Math.round(hubW * 0.7), 52, 88);

  const cx = Math.round(width / 2);
  const cy = Math.round(playTop + playH / 2);
  const hub: Rect = {
    x: Math.round(cx - hubW / 2),
    y: Math.round(cy - hubH / 2),
    width: hubW,
    height: hubH,
  };

  const riverGap = isWide ? 2 : 1;
  const riverHubGap = height >= 500 ? 10 : 6;
  const maxRiverAreaV = Math.max(
    40,
    Math.floor((playH - hubH) / 2 - riverHubGap - 4),
  );
  const maxRiverAreaH = Math.max(
    40,
    Math.floor((width - hubW) / 2 - oppHandTileH - margin * 2 - riverHubGap - 8),
  );
  const riverDepthBudget = Math.min(maxRiverAreaV, maxRiverAreaH);
  const maxRiverTileHFromDepth = Math.floor((riverDepthBudget - riverGap * 2) / 3);
  const maxRiverTileWFromSpan = Math.floor(
    (Math.min(width * 0.44, playH * 0.46) - riverGap * 5) / 6,
  );
  const riverTileW = clamp(
    Math.min(Math.floor(maxRiverTileHFromDepth / 1.36), maxRiverTileWFromSpan),
    14,
    28,
  );
  const riverTileH = Math.round(riverTileW * 1.36);

  const riverSpan = 6 * riverTileW + 5 * riverGap;
  const riverThreeRows = 3 * riverTileH + 2 * riverGap;

  const bottomRiver: Rect = {
    x: Math.round(cx - riverSpan / 2),
    y: hub.y + hub.height + riverHubGap,
    width: riverSpan,
    height: Math.min(riverThreeRows, Math.max(riverTileH, bottomHandY - (hub.y + hub.height + riverHubGap) - 4)),
  };

  const topRiverHeight = Math.min(riverThreeRows, Math.max(riverTileH, hub.y - riverHubGap - playTop));
  const topRiver: Rect = {
    x: Math.round(cx - riverSpan / 2),
    y: hub.y - riverHubGap - topRiverHeight,
    width: riverSpan,
    height: topRiverHeight,
  };

  const leftRiverWidth = Math.min(riverThreeRows, Math.max(riverTileH, hub.x - riverHubGap - margin - oppHandTileH - 8));
  const leftRiver: Rect = {
    x: hub.x - riverHubGap - leftRiverWidth,
    y: Math.round(cy - riverSpan / 2),
    width: leftRiverWidth,
    height: riverSpan,
  };

  const rightRiverWidth = Math.min(riverThreeRows, Math.max(riverTileH, width - margin - oppHandTileH - 8 - (hub.x + hub.width + riverHubGap)));
  const rightRiver: Rect = {
    x: hub.x + hub.width + riverHubGap,
    y: Math.round(cy - riverSpan / 2),
    width: rightRiverWidth,
    height: riverSpan,
  };

  const badgePad = isWide ? 16 : 8;
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
    oppHandTileW,
    oppHandTileH,
    riverTileW,
    riverTileH,
    riverGap,
    meldTileW,
    meldTileH,
    hub,
    rivers: {
      bottom: bottomRiver,
      top: topRiver,
      left: leftRiver,
      right: rightRiver,
    },
    badges: {
      bottom: { x: badgePad, y: bottomHandY - 34 },
      right: { x: width - badgePad, y: topMargin + 4 },
      top: { x: badgePad, y: topMargin + 2 },
      left: { x: badgePad, y: bottomHandY - 76 },
    },
    melds: {
      bottom: { x: width - badgePad, y: bottomHandY - meldTileH - 8 },
      right: { x: width - margin - oppHandTileH - 8, y: cy + riverSpan / 2 + 10 },
      top: { x: width - badgePad, y: topMargin + oppHandTileH + 6 },
      left: { x: margin + oppHandTileH + 8, y: topMargin + oppHandTileH + 6 },
    },
  };
}
