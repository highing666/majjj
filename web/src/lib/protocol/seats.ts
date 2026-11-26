import type { Seat } from "./generated/Seat";
import type { SeatKind } from "./generated/SeatKind";

export const ALL_SEATS: [Seat, Seat, Seat, Seat] = [
  "East",
  "South",
  "West",
  "North",
];

export interface RelativeSeats {
  bottom: Seat;
  right: Seat;
  top: Seat;
  left: Seat;
}

export type RelativeSlot = keyof RelativeSeats;
export type RelativeRole = "自家" | "下家" | "对门" | "上家";
export type RelativeRoleShort = "自" | "下" | "对" | "上";

export type SeatDisplayEntry =
  | { nickname: string; kind: SeatKind }
  | { nickname: string; seat_kind: SeatKind };

export type SeatDisplayList = readonly [
  SeatDisplayEntry,
  SeatDisplayEntry,
  SeatDisplayEntry,
  SeatDisplayEntry,
];

const SLOT_TO_ROLE: Record<RelativeSlot, RelativeRole> = {
  bottom: "自家",
  right: "下家",
  top: "对门",
  left: "上家",
};

const SLOT_TO_SHORT_ROLE: Record<RelativeSlot, RelativeRoleShort> = {
  bottom: "自",
  right: "下",
  top: "对",
  left: "上",
};

/**
 * Returns the canonical `0..3` seat index (`East=0`, `South=1`, `West=2`, `North=3`).
 */
export function seatIndex(seat: Seat): number {
  switch (seat) {
    case "East":
      return 0;
    case "South":
      return 1;
    case "West":
      return 2;
    case "North":
      return 3;
  }
}

/**
 * Maps canonical seats counter-clockwise relative to `viewer` (`bottom` -> `right` -> `top` -> `left`).
 */
export function getRelativeSeats(viewer: Seat): RelativeSeats {
  const idx = seatIndex(viewer);
  return {
    bottom: ALL_SEATS[idx],
    right: ALL_SEATS[(idx + 1) % 4],
    top: ALL_SEATS[(idx + 2) % 4],
    left: ALL_SEATS[(idx + 3) % 4],
  };
}

/**
 * Maps `target` seat to its viewer-relative canvas slot (`bottom` / `right` / `top` / `left`).
 */
export function seatRelativeSlot(viewer: Seat, target: Seat): RelativeSlot {
  const rel = getRelativeSeats(viewer);
  if (target === rel.bottom) {
    return "bottom";
  }
  if (target === rel.right) {
    return "right";
  }
  if (target === rel.top) {
    return "top";
  }
  return "left";
}

/**
 * Returns the viewer-relative role label (`自家` / `下家` / `对门` / `上家`) for `target`.
 */
export function seatRelativeRole(viewer: Seat, target: Seat): RelativeRole {
  return SLOT_TO_ROLE[seatRelativeSlot(viewer, target)];
}

/**
 * Returns the single-character viewer-relative role label (`自` / `下` / `对` / `上`) for `target`.
 */
export function seatRelativeShortRole(
  viewer: Seat,
  target: Seat,
): RelativeRoleShort {
  return SLOT_TO_SHORT_ROLE[seatRelativeSlot(viewer, target)];
}

/**
 * Returns the wind-free 1-based seat number label (`1号位`..`4号位`).
 */
export function seatNumberLabel(seat: Seat): string {
  return `${seatIndex(seat) + 1}号位`;
}

/**
 * Resolves the display nickname for a single `SeatDisplayEntry` at `seat`
 * (trimmed nickname, else `AI {idx+1}` for AI, else `""`).
 */
export function resolveSeatEntryNickname(
  seat: Seat,
  entry?: SeatDisplayEntry | null,
): string {
  const trimmed = entry?.nickname.trim() ?? "";
  if (trimmed.length > 0) {
    return trimmed;
  }
  const seatKind = entry
    ? "kind" in entry
      ? entry.kind
      : entry.seat_kind
    : null;
  return seatKind === "Ai" ? `AI ${seatIndex(seat) + 1}` : "";
}

/**
 * Resolves the display nickname for `seat` (trimmed nickname, else `AI {idx+1}` for AI, else `""`).
 */
export function resolveSeatNickname(
  seat: Seat,
  seats?: SeatDisplayList | null,
): string {
  return resolveSeatEntryNickname(seat, seats?.[seatIndex(seat)]);
}

/**
 * Formats a single seat entry in viewerless contexts (nickname, falling back to `1~4号位`).
 */
export function formatSeatEntrySeatless(
  seat: Seat,
  entry?: SeatDisplayEntry | null,
): string {
  return resolveSeatEntryNickname(seat, entry) || seatNumberLabel(seat);
}

/**
 * Formats a seat in viewerless contexts (nickname, falling back to `1~4号位`).
 */
export function formatSeatlessDisplay(
  seat: Seat,
  seats?: SeatDisplayList | null,
): string {
  return formatSeatEntrySeatless(seat, seats?.[seatIndex(seat)]);
}

/**
 * Formats a seat with viewer-relative role and dot-separated nickname (`下家 · 小红`),
 * or falls back to `formatSeatlessDisplay` when `viewer === null`.
 */
export function formatRelativeSeatDisplay(
  seat: Seat,
  viewer: Seat | null,
  seats?: SeatDisplayList | null,
): string {
  if (viewer === null) {
    return formatSeatlessDisplay(seat, seats);
  }
  const role = seatRelativeRole(viewer, seat);
  const name = resolveSeatNickname(seat, seats);
  return name ? `${role} · ${name}` : role;
}

/**
 * Formats a seat with viewer-relative role and parenthesized nickname (`下家（小红）`),
 * or falls back to `formatSeatlessDisplay` when `viewer === null`.
 */
export function formatRelativeSeatParen(
  seat: Seat,
  viewer: Seat | null,
  seats?: SeatDisplayList | null,
): string {
  if (viewer === null) {
    return formatSeatlessDisplay(seat, seats);
  }
  const role = seatRelativeRole(viewer, seat);
  const name = resolveSeatNickname(seat, seats);
  return name ? `${role}（${name}）` : role;
}
