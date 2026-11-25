import type { DiscardEntry } from "../protocol/generated/DiscardEntry";
import type { MatchSummary } from "../protocol/generated/MatchSummary";
import type { MeldView } from "../protocol/generated/MeldView";
import type { PlayerAction } from "../protocol/generated/PlayerAction";
import type { PlayerView } from "../protocol/generated/PlayerView";
import type { PublicSeatView } from "../protocol/generated/PublicSeatView";
import type { RevealedMeld } from "../protocol/generated/RevealedMeld";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { Tile } from "../protocol/generated/Tile";
import { parseMeld, type MeldKind } from "../protocol/melds";
import { ALL_SEATS, seatIndex, seatLabel, seatShortWind } from "../protocol/seats";
import type { PendingAddedKong } from "../session/reducer";
import { getTileLabel } from "./tiles";

export interface RelativeSeats {
  bottom: Seat;
  right: Seat;
  top: Seat;
  left: Seat;
}

export type RelativeSlot = keyof RelativeSeats;
export type RelativeRole = "自家" | "下家" | "对家" | "上家";

export interface MeldTileSlot {
  tile: Tile | null;
  faceUp: boolean;
}

export interface MeldRenderItem {
  kind: MeldKind;
  tiles: MeldTileSlot[];
  pendingAddedKong: boolean;
}

export type ActionBarTone = "hu" | "kong" | "pon" | "pass";

export interface ActionBarItem {
  key: string;
  label: string;
  tile: Tile | null;
  tileLabel: string | null;
  tone: ActionBarTone;
  shortcut: string;
  shortcutKey: string;
  action: PlayerAction;
}

export interface CanvasSize {
  width: number;
  height: number;
}

export interface SeatBadgeModel {
  wind: string;
  isDealer: boolean;
  isActive: boolean;
  isOffline: boolean;
  name: string;
  scoreText: string;
}

export interface ShortcutContext {
  disabled: boolean;
  hasModalOpen: boolean;
  isInputFocused: boolean;
}

export interface MatchStandingRow {
  rank: number;
  seat: Seat;
  seatName: string;
  nickname: string;
  isAi: boolean;
  initialScore: number;
  finalScore: number;
  netDelta: number;
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

const SLOT_TO_ROLE: Record<RelativeSlot, RelativeRole> = {
  bottom: "自家",
  right: "下家",
  top: "对家",
  left: "上家",
};

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
 * Returns the viewer-relative role label for `target`.
 */
export function seatRelativeRole(viewer: Seat, target: Seat): RelativeRole {
  return SLOT_TO_ROLE[seatRelativeSlot(viewer, target)];
}

/**
 * Formats a canonical seat display title (`东座 · 房主`, `南座 · AI 2`, or `西座`).
 */
export function formatSeatDisplayName(
  seat: Seat,
  seats?: readonly [SeatInfo, SeatInfo, SeatInfo, SeatInfo] | null,
): string {
  const idx = seatIndex(seat);
  const info = seats?.[idx];
  const name = info?.nickname || (info?.kind === "Ai" ? `AI ${idx + 1}` : "");
  return name ? `${seatLabel(seat)} · ${name}` : seatLabel(seat);
}

/**
 * Filters out claimed discards (`claimed_by !== null`) and chunks river tiles into rows of `rowSize` (6).
 */
export function buildVisibleRiverRows(
  river: readonly DiscardEntry[],
  rowSize = 6,
): DiscardEntry[][] {
  const visible = river.filter((entry) => entry.claimed_by === null);
  const rows: DiscardEntry[][] = [];
  for (let i = 0; i < visible.length; i += rowSize) {
    rows.push(visible.slice(i, i + rowSize));
  }
  return rows;
}

function mapMeldToRenderItem(
  meld: MeldView | RevealedMeld,
  pendingAddedKong: PendingAddedKong | null,
  seat: Seat | null,
): MeldRenderItem {
  const parsed = parseMeld(meld);
  switch (parsed.kind) {
    case "Pon": {
      const { tile } = parsed;
      const isPendingAdded =
        pendingAddedKong !== null &&
        seat !== null &&
        pendingAddedKong.seat === seat &&
        pendingAddedKong.tile === tile;
      return {
        kind: "Pon",
        tiles: [
          { tile, faceUp: true },
          { tile, faceUp: true },
          { tile, faceUp: true },
        ],
        pendingAddedKong: isPendingAdded,
      };
    }
    case "ExposedKong":
    case "AddedKong": {
      const { kind, tile } = parsed;
      return {
        kind,
        tiles: [
          { tile, faceUp: true },
          { tile, faceUp: true },
          { tile, faceUp: true },
          { tile, faceUp: true },
        ],
        pendingAddedKong: false,
      };
    }
    case "ConcealedKong": {
      const { tile } = parsed;
      return {
        kind: "ConcealedKong",
        tiles:
          tile !== null
            ? [
                { tile: null, faceUp: false },
                { tile: null, faceUp: false },
                { tile, faceUp: true },
                { tile: null, faceUp: false },
              ]
            : [
                { tile: null, faceUp: false },
                { tile: null, faceUp: false },
                { tile: null, faceUp: false },
                { tile: null, faceUp: false },
              ],
        pendingAddedKong: false,
      };
    }
    case "Chi": {
      const [a, b, c] = parsed.tiles;
      return {
        kind: "Chi",
        tiles: [
          { tile: a, faceUp: true },
          { tile: b, faceUp: true },
          { tile: c, faceUp: true },
        ],
        pendingAddedKong: false,
      };
    }
  }
}

/**
 * Converts a seat's `MeldView` list into render slots, honoring viewer vs opponent `ConcealedKong`
 * visibility and highlighting unresolved `AddedKongAttempted` without finalizing the meld early.
 */
export function buildMeldRenderItems(
  melds: readonly MeldView[],
  pendingAddedKong: PendingAddedKong | null,
  seat: Seat,
): MeldRenderItem[] {
  return melds.map((meld) => mapMeldToRenderItem(meld, pendingAddedKong, seat));
}

/**
 * Converts settlement `RevealedMeld` entries into render slots (all `ConcealedKong` tiles revealed).
 */
export function buildRevealedMeldItems(
  melds: readonly RevealedMeld[],
): MeldRenderItem[] {
  return melds.map((meld) => mapMeldToRenderItem(meld, null, null));
}

/**
 * Formats signed integer point delta (`+12`, `0`, `-6`).
 */
export function formatScoreDelta(delta: number): string {
  if (delta > 0) {
    return `+${delta}`;
  }
  return String(delta);
}

/**
 * Builds ranked standings from `MatchSummary` and `SeatInfo[4]`.
 */
export function buildMatchStandings(
  summary: MatchSummary,
  seats: readonly [SeatInfo, SeatInfo, SeatInfo, SeatInfo],
): MatchStandingRow[] {
  const rows = ALL_SEATS.map((seat, idx) => {
    const info = seats[idx];
    const nickname =
      info.nickname || (info.kind === "Ai" ? `AI ${idx + 1}` : seatLabel(seat));
    return {
      rank: 1,
      seat,
      seatName: seatLabel(seat),
      nickname,
      isAi: info.kind === "Ai",
      initialScore: summary.initial_scores[idx],
      finalScore: summary.final_scores[idx],
      netDelta: summary.net_deltas[idx],
    };
  });

  rows.sort((a, b) => b.finalScore - a.finalScore);
  rows.forEach((row, idx) => {
    row.rank = idx + 1;
  });
  return rows;
}

/**
 * Finds an exact matching `Discard` action in `legalActions`, preserving `from_drawn` (`手切` vs `摸切`).
 */
export function findLegalDiscardAction(
  legalActions: readonly PlayerAction[],
  tile: Tile,
  fromDrawn: boolean,
): PlayerAction | null {
  for (const action of legalActions) {
    if (
      typeof action === "object" &&
      action !== null &&
      "Discard" in action &&
      action.Discard.tile === tile &&
      action.Discard.from_drawn === fromDrawn
    ) {
      return action;
    }
  }
  return null;
}

function createActionItem(
  key: string,
  label: string,
  tone: ActionBarTone,
  shortcut: string,
  shortcutKey: string,
  action: PlayerAction,
  tile: Tile | null = null,
): ActionBarItem {
  return {
    key,
    label,
    tile,
    tileLabel: tile !== null ? getTileLabel(tile) : null,
    tone,
    shortcut,
    shortcutKey,
    action,
  };
}

/**
 * Builds authoritative in-play action bar buttons exclusively from `view.legal_actions`.
 * Never renders `吃` when `rule_profile.allow_chi` is `false` (Tuidaohu), and excludes hand discards
 * and `BetweenHands` actions.
 */
export function buildActionBarItems(view: PlayerView): ActionBarItem[] {
  if (view.phase !== "Playing") {
    return [];
  }

  const tsumoItems: ActionBarItem[] = [];
  const ronItems: ActionBarItem[] = [];
  const robKongItems: ActionBarItem[] = [];
  const concealedKongItems: ActionBarItem[] = [];
  const addedKongItems: ActionBarItem[] = [];
  const exposedKongItems: ActionBarItem[] = [];
  const ponItems: ActionBarItem[] = [];
  const passItems: ActionBarItem[] = [];

  for (const action of view.legal_actions) {
    if (action === "Tsumo") {
      tsumoItems.push(createActionItem("Tsumo", "自摸", "hu", "H", "h", action));
      continue;
    }
    if (action === "Pass") {
      passItems.push(
        createActionItem("Pass", "过", "pass", "Esc", "escape", action),
      );
      continue;
    }
    if (typeof action !== "object" || action === null) {
      continue;
    }

    if ("Ron" in action) {
      const tile = action.Ron.tile;
      ronItems.push(
        createActionItem(`Ron:${tile}`, "点炮胡", "hu", "H", "h", action, tile),
      );
    } else if ("RobKongHu" in action) {
      const tile = action.RobKongHu.tile;
      robKongItems.push(
        createActionItem(
          `RobKongHu:${tile}`,
          "抢杠胡",
          "hu",
          "H",
          "h",
          action,
          tile,
        ),
      );
    } else if ("ConcealedKong" in action) {
      const tile = action.ConcealedKong.tile;
      concealedKongItems.push(
        createActionItem(
          `ConcealedKong:${tile}`,
          "暗杠",
          "kong",
          "G",
          "g",
          action,
          tile,
        ),
      );
    } else if ("AddedKong" in action) {
      const tile = action.AddedKong.tile;
      addedKongItems.push(
        createActionItem(
          `AddedKong:${tile}`,
          "补杠",
          "kong",
          "G",
          "g",
          action,
          tile,
        ),
      );
    } else if ("ExposedKong" in action) {
      const tile = action.ExposedKong.tile;
      exposedKongItems.push(
        createActionItem(
          `ExposedKong:${tile}`,
          "明杠",
          "kong",
          "G",
          "g",
          action,
          tile,
        ),
      );
    } else if ("Pon" in action) {
      const tile = action.Pon.tile;
      ponItems.push(
        createActionItem(`Pon:${tile}`, "碰", "pon", "P", "p", action, tile),
      );
    }
    // `Discard` is handled directly on hand tiles; `Chi` is never rendered when `allow_chi === false`.
  }

  return [
    ...tsumoItems,
    ...ronItems,
    ...robKongItems,
    ...concealedKongItems,
    ...addedKongItems,
    ...exposedKongItems,
    ...ponItems,
    ...passItems,
  ];
}

/**
 * Clamps canvas devicePixelRatio to `[1, 2]` to avoid oversized Safari backing stores.
 */
export function computeCanvasResolution(dpr: number): number {
  if (!Number.isFinite(dpr) || dpr <= 1) {
    return 1;
  }
  return Math.min(dpr, 2);
}

/**
 * Reads `ResizeObserverEntry.contentBoxSize` (never `devicePixelContentBoxSize`, which Safari omits)
 * with a `contentRect` fallback.
 */
export function extractContentBoxSize(entry: {
  contentBoxSize?:
    | ReadonlyArray<{ inlineSize: number; blockSize: number }>
    | { inlineSize: number; blockSize: number };
  contentRect?: { width: number; height: number };
}): CanvasSize {
  const box = entry.contentBoxSize;
  if (box) {
    const first = Array.isArray(box)
      ? box[0]
      : (box as { inlineSize: number; blockSize: number });
    if (first && Number.isFinite(first.inlineSize) && Number.isFinite(first.blockSize)) {
      return {
        width: Math.max(1, Math.round(first.inlineSize)),
        height: Math.max(1, Math.round(first.blockSize)),
      };
    }
  }
  const rect = entry.contentRect;
  if (rect && Number.isFinite(rect.width) && Number.isFinite(rect.height)) {
    return {
      width: Math.max(1, Math.round(rect.width)),
      height: Math.max(1, Math.round(rect.height)),
    };
  }
  return { width: 1, height: 1 };
}

/**
 * Resolves a keyboard shortcut (`H`/`G`/`P`/`Esc`) to an unambiguous `PlayerAction`,
 * suppressing shortcuts when disabled, a modal is open, or an input element has focus.
 */
export function resolveActionBarShortcut(
  items: readonly ActionBarItem[],
  rawKey: string,
  ctx: ShortcutContext,
): PlayerAction | null {
  if (ctx.disabled || ctx.hasModalOpen || ctx.isInputFocused || items.length === 0) {
    return null;
  }
  const key = rawKey.toLowerCase();
  const candidates = items.filter((item) => item.shortcutKey === key);
  return candidates.length === 1 ? candidates[0].action : null;
}

/**
 * Builds the compact Scheme C corner seat badge view model.
 */
export function buildSeatBadgeModel(
  seat: Seat,
  seatView: PublicSeatView,
  seatInfo: SeatInfo,
  viewer: Seat,
  dealer: Seat,
  activeSeat: Seat | null,
): SeatBadgeModel {
  const role = seatRelativeRole(viewer, seat);
  const rawName = seatInfo.nickname || (seatInfo.kind === "Ai" ? "AI" : role);
  const name = rawName.length > 7 ? `${rawName.slice(0, 6)}…` : rawName;
  return {
    wind: seatShortWind(seat),
    isDealer: dealer === seat,
    isActive: activeSeat === seat,
    isOffline: seatInfo.kind === "Human" && !seatInfo.connected,
    name,
    scoreText: formatScoreDelta(seatView.score),
  };
}
