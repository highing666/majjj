import type { Rect, TableLayout } from "./layout";
import type { RelativeSlot } from "./viewModel";

export type BanterTailPosition =
  | "top-left"
  | "top-right"
  | "top-center"
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
