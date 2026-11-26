import type { Rect, TableLayout } from "./layout";
import type { RelativeSlot } from "./viewModel";

export type BanterTailPosition =
  | "top-left"
  | "top-right"
  | "bottom-left";

export interface BanterBubbleAnchor {
  slot: RelativeSlot;
  leftPx: number;
  topPx: number;
  maxWidthPx: number;
  maxHeightPx: number;
  alignRight: boolean;
  alignBottom: boolean;
  tail: BanterTailPosition;
}

const EDGE_PAD = 6;
const GAP = 5;

/**
 * Single-sourced compact breakpoint shared by `computeQuickEmoteDockRect` and `QuickEmoteBar.svelte`.
 */
export function isCompactQuickEmoteDock(layout: TableLayout): boolean {
  return (
    layout.isShortLandscape ||
    layout.height > layout.width ||
    layout.width < 700 ||
    layout.rightWing.width < 190
  );
}

/**
 * Converts a pixel `Rect` inside `layout` into percentage CSS positioning declarations.
 */
export function rectToPercentStyle(
  layout: TableLayout,
  rect: Rect,
  includeHeight = true,
): string {
  const leftPct = (rect.x / layout.width) * 100;
  const topPct = (rect.y / layout.height) * 100;
  const widthPct = (rect.width / layout.width) * 100;
  const base = `left: ${leftPct.toFixed(3)}%; top: ${topPct.toFixed(3)}%; width: ${widthPct.toFixed(3)}%;`;
  if (!includeHeight) {
    return base;
  }
  const heightPct = (rect.height / layout.height) * 100;
  return `${base} height: ${heightPct.toFixed(3)}%;`;
}

function clamp(value: number, min: number, max: number): number {
  if (max < min) {
    return min;
  }
  return Math.min(max, Math.max(min, value));
}

function resolveAnchorBox(layout: TableLayout, slot: RelativeSlot): Rect {
  if (slot === "bottom") {
    return layout.badges.bottom;
  }
  const pocket = layout.personaPockets[slot];
  return pocket.width > 0 && pocket.height > 0 ? pocket : layout.badges[slot];
}

/**
 * Computes a viewport-clamped comic speech bubble anchor anchored to the speaker's
 * persona pocket / badge while staying strictly clear of `viewerHandRect`.
 */
export function computeBanterBubbleAnchor(
  layout: TableLayout,
  slot: RelativeSlot,
): BanterBubbleAnchor {
  const anchor = resolveAnchorBox(layout, slot);
  const compact = layout.isShortLandscape || layout.width < 680;
  const maxAllowedBottom = Math.max(
    EDGE_PAD + 36,
    layout.viewerHandRect.y - GAP,
  );
  const defaultMaxH = compact ? 52 : 76;

  if (slot === "left") {
    const wingW = Math.max(anchor.width, layout.leftWing.width);
    const maxWidthPx = clamp(
      Math.round(wingW + (compact ? 18 : 34)),
      116,
      Math.min(230, Math.max(116, layout.centerStage.x + 28)),
    );
    const leftPx = clamp(anchor.x, EDGE_PAD, layout.width - maxWidthPx - EDGE_PAD);
    const rawTop = anchor.y + anchor.height + GAP;
    const maxHeightPx = clamp(maxAllowedBottom - rawTop, 36, defaultMaxH);
    const topPx = clamp(rawTop, EDGE_PAD, maxAllowedBottom - maxHeightPx);
    return {
      slot,
      leftPx,
      topPx,
      maxWidthPx,
      maxHeightPx,
      alignRight: false,
      alignBottom: false,
      tail: "top-left",
    };
  }

  if (slot === "right") {
    const wingW = Math.max(anchor.width, layout.rightWing.width);
    const maxWidthPx = clamp(
      Math.round(wingW + (compact ? 18 : 34)),
      116,
      Math.min(230, Math.max(116, layout.width - layout.rightWing.x + 28)),
    );
    const rightEdge = clamp(
      anchor.x + anchor.width,
      maxWidthPx + EDGE_PAD,
      layout.width - EDGE_PAD,
    );
    const leftPx = rightEdge - maxWidthPx;
    const rawTop = anchor.y + anchor.height + GAP;
    const maxHeightPx = clamp(maxAllowedBottom - rawTop, 36, defaultMaxH);
    const topPx = clamp(rawTop, EDGE_PAD, maxAllowedBottom - maxHeightPx);
    return {
      slot,
      leftPx,
      topPx,
      maxWidthPx,
      maxHeightPx,
      alignRight: true,
      alignBottom: false,
      tail: "top-right",
    };
  }

  if (slot === "top") {
    const maxWidthPx = clamp(
      Math.round(Math.max(anchor.width + 72, layout.centerStage.width * 0.42)),
      132,
      Math.min(250, layout.width - EDGE_PAD * 2),
    );
    const preferredLeft = anchor.x + anchor.width - maxWidthPx;
    const leftPx = clamp(preferredLeft, EDGE_PAD, layout.width - maxWidthPx - EDGE_PAD);
    const rawTop = anchor.y + anchor.height + GAP;
    const maxHeightPx = clamp(maxAllowedBottom - rawTop, 36, defaultMaxH);
    const topPx = clamp(rawTop, EDGE_PAD, maxAllowedBottom - maxHeightPx);
    return {
      slot,
      leftPx,
      topPx,
      maxWidthPx,
      maxHeightPx,
      alignRight: true,
      alignBottom: false,
      tail: "top-right",
    };
  }

  // `bottom` slot: anchored above bottom-left badge, clear of viewer hand.
  const maxWidthPx = clamp(
    Math.round(Math.max(anchor.width, layout.leftWing.width)),
    116,
    Math.min(220, layout.width - EDGE_PAD * 2),
  );
  const leftPx = clamp(anchor.x, EDGE_PAD, layout.width - maxWidthPx - EDGE_PAD);
  const bottomEdge = clamp(anchor.y - GAP, EDGE_PAD + 36, maxAllowedBottom);
  const maxHeightPx = clamp(bottomEdge - EDGE_PAD, 36, defaultMaxH);
  const topPx = Math.max(EDGE_PAD, bottomEdge - maxHeightPx);
  return {
    slot,
    leftPx,
    topPx,
    maxWidthPx,
    maxHeightPx,
    alignRight: false,
    alignBottom: true,
    tail: "bottom-left",
  };
}

/**
 * Computes a viewport-clamped thumb-reachable dock rect for the 6-button `QuickEmoteBar`
 * that stays strictly clear of `viewerHandRect`, `turnBarRect`, and `badges.bottom`.
 */
export function computeQuickEmoteDockRect(layout: TableLayout): Rect {
  if (layout.height > layout.width) {
    const dockH = 44;
    const width = Math.max(180, layout.width - EDGE_PAD * 2);
    const maxBottom = Math.min(
      layout.badges.bottom.y,
      layout.turnBarRect.y,
      layout.viewerHandRect.y,
    ) - GAP;
    const top = clamp(maxBottom - dockH, EDGE_PAD, layout.height - dockH - EDGE_PAD);
    return {
      x: EDGE_PAD,
      y: top,
      width,
      height: dockH,
    };
  }

  const compact = isCompactQuickEmoteDock(layout);
  const dockH = compact ? 64 : 84;
  const width = clamp(
    layout.rightWing.width,
    92,
    Math.max(92, layout.width - layout.rightWing.x - EDGE_PAD),
  );
  const x = clamp(layout.rightWing.x, EDGE_PAD, layout.width - width - EDGE_PAD);
  const y = clamp(
    layout.height - EDGE_PAD - dockH,
    EDGE_PAD,
    layout.height - dockH - EDGE_PAD,
  );
  return {
    x,
    y,
    width,
    height: dockH,
  };
}
