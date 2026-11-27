import type { DiscardEntry } from "../protocol/generated/DiscardEntry";
import type { MatchSummary } from "../protocol/generated/MatchSummary";
import type { MeldView } from "../protocol/generated/MeldView";
import type { PersonaId } from "../protocol/generated/PersonaId";
import type { PersonaMood } from "../protocol/generated/PersonaMood";
import type { PlayerAction } from "../protocol/generated/PlayerAction";
import type { PlayerView } from "../protocol/generated/PlayerView";
import type { PublicSeatView } from "../protocol/generated/PublicSeatView";
import type { RevealedMeld } from "../protocol/generated/RevealedMeld";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import type { Tile } from "../protocol/generated/Tile";
import { parseMeld, type MeldKind } from "../protocol/melds";
import { getPersonaMeta, getPersonaMoodMeta } from "../protocol/personas";
import {
  ALL_SEATS,
  formatSeatlessDisplay,
  getRelativeSeats,
  resolveSeatNickname,
  seatIndex,
  seatNumberLabel,
  seatRelativeRole,
  seatRelativeSlot,
  type RelativeRole,
  type RelativeSlot,
} from "../protocol/seats";
import type { PendingAddedKong } from "../session/reducer";
import { getTileLabel } from "./tiles";

export {
  getRelativeSeats,
  seatRelativeSlot,
  type RelativeSlot,
};

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
  role: RelativeRole;
  isDealer: boolean;
  isActive: boolean;
  isOffline: boolean;
  isAiAssisted: boolean;
  name: string;
  scoreText: string;
  personaId: PersonaId | null;
  mood: PersonaMood | null;
}

export type OpponentPocketSlot = "top" | "left" | "right";

export interface TablePersonaPocketModel {
  slot: OpponentPocketSlot;
  seat: Seat;
  personaId: PersonaId;
  mood: PersonaMood;
  name: string;
  styleLabel: string;
  moodLabel: string;
  moodShortLabel: string;
  accentHex: string;
  badgeBg: string;
  badgeFg: string;
  isActive: boolean;
}

export type TablePersonaPocketMap = Record<
  OpponentPocketSlot,
  TablePersonaPocketModel | null
>;

export interface ShortcutContext {
  disabled: boolean;
  hasModalOpen: boolean;
  isInputFocused: boolean;
  suppressEscape?: boolean;
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

export type TurnCueMode =
  | "idle"
  | "action_prompt"
  | "discard_drawn"
  | "discard_post_pon";

export interface TurnCueState {
  mode: TurnCueMode;
  activeSlot: RelativeSlot | null;
  showStageWash: boolean;
  animateBreath: boolean;
  highlightDrawnTile: boolean;
  highlightClosedHand: boolean;
  turnPillText: string | null;
  waitHintText: string | null;
  dimViewerHand: boolean;
  centerEdgeGlowSlot: RelativeSlot | null;
}

export interface DeriveTurnCueOptions {
  playerView: PlayerView | null;
  pendingAddedKong?: PendingAddedKong | null;
  inputLocked?: boolean;
  reducedEffects?: boolean;
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
 * Builds ranked standings from `MatchSummary` and `SeatInfo[4]`, projecting
 * viewer-relative roles when `viewer !== null` and seatless names otherwise.
 */
export function buildMatchStandings(
  summary: MatchSummary,
  seats: readonly [SeatInfo, SeatInfo, SeatInfo, SeatInfo],
  viewer: Seat | null = null,
): MatchStandingRow[] {
  const rows = ALL_SEATS.map((seat, idx) => {
    const info = seats[idx];
    const seatName = viewer !== null ? seatRelativeRole(viewer, seat) : "";
    const nickname =
      viewer !== null
        ? resolveSeatNickname(seat, seats)
        : formatSeatlessDisplay(seat, seats);
    return {
      rank: 1,
      seat,
      seatName,
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
  if (ctx.suppressEscape && key === "escape") {
    return null;
  }
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
  const rawName =
    seatInfo.nickname.trim() ||
    (seatInfo.kind === "Ai" ? "AI" : seatNumberLabel(seat));
  const name = rawName.length > 7 ? `${rawName.slice(0, 6)}…` : rawName;
  return {
    role,
    isDealer: dealer === seat,
    isActive: activeSeat === seat,
    isOffline: seatInfo.kind === "Human" && !seatInfo.connected,
    isAiAssisted: seatInfo.kind === "Human" && seatInfo.ai_assisted,
    name,
    scoreText: formatScoreDelta(seatView.score),
    personaId: seatInfo.kind === "Ai" ? seatInfo.persona_id : null,
    mood: seatInfo.kind === "Ai" ? seatInfo.mood : null,
  };
}

/**
 * Projects AI seats occupying the `top`, `left`, and `right` opponent slots into
 * `TablePersonaPocketModel` entries anchored inside `TableLayout.personaPockets`.
 */
export function buildTablePersonaPocketModels(
  snapshot: TableSnapshot,
  playerView: PlayerView,
): TablePersonaPocketMap {
  const relative = getRelativeSeats(playerView.viewer);
  const activeSeat = playerView.turn.active_seat;

  const projectSlot = (
    slot: OpponentPocketSlot,
  ): TablePersonaPocketModel | null => {
    const seat = relative[slot];
    const seatInfo = snapshot.seats[seatIndex(seat)];
    if (seatInfo.kind !== "Ai" || seatInfo.persona_id === null) {
      return null;
    }
    const personaMeta = getPersonaMeta(seatInfo.persona_id);
    const mood = seatInfo.mood ?? personaMeta.initialMood;
    const moodMeta = getPersonaMoodMeta(mood);
    return {
      slot,
      seat,
      personaId: seatInfo.persona_id,
      mood,
      name: seatInfo.nickname.trim() || personaMeta.name,
      styleLabel: personaMeta.styleLabel,
      moodLabel: moodMeta.label,
      moodShortLabel: moodMeta.shortLabel,
      accentHex: personaMeta.accentHex,
      badgeBg: moodMeta.badgeBg,
      badgeFg: moodMeta.badgeFg,
      isActive: activeSeat === seat,
    };
  };

  return {
    top: projectSlot("top"),
    left: projectSlot("left"),
    right: projectSlot("right"),
  };
}

function hasLegalDiscardAction(legalActions: readonly PlayerAction[]): boolean {
  return legalActions.some(
    (action) =>
      typeof action === "object" && action !== null && "Discard" in action,
  );
}

/**
 * Derives the 4-layer turn cue state from the current `PlayerView`, pending added kong,
 * input lock, and reduced-effects policy. Never derives or exposes any tile-advice hints.
 */
export function deriveTurnCueState(
  options: DeriveTurnCueOptions,
): TurnCueState {
  const {
    playerView,
    pendingAddedKong = null,
    inputLocked = false,
    reducedEffects = false,
  } = options;

  if (!playerView || playerView.phase !== "Playing") {
    return {
      mode: "idle",
      activeSlot: null,
      showStageWash: false,
      animateBreath: false,
      highlightDrawnTile: false,
      highlightClosedHand: false,
      turnPillText: null,
      waitHintText: null,
      dimViewerHand: false,
      centerEdgeGlowSlot: null,
    };
  }

  const activeSeat = playerView.turn.active_seat;
  const activeSlot =
    activeSeat !== null ? seatRelativeSlot(playerView.viewer, activeSeat) : null;
  const centerEdgeGlowSlot = activeSlot;

  if (!inputLocked && buildActionBarItems(playerView).length > 0) {
    return {
      mode: "action_prompt",
      activeSlot,
      showStageWash: true,
      animateBreath: !reducedEffects,
      highlightDrawnTile: false,
      highlightClosedHand: false,
      turnPillText: null,
      waitHintText: null,
      dimViewerHand: false,
      centerEdgeGlowSlot,
    };
  }

  const canDiscardTurn =
    !inputLocked &&
    activeSeat === playerView.viewer &&
    hasLegalDiscardAction(playerView.legal_actions);

  if (canDiscardTurn) {
    const isDrawnTurn = playerView.hand.drawn_tile !== null;
    return {
      mode: isDrawnTurn ? "discard_drawn" : "discard_post_pon",
      activeSlot,
      showStageWash: true,
      animateBreath: !reducedEffects,
      highlightDrawnTile: isDrawnTurn,
      highlightClosedHand: !isDrawnTurn,
      turnPillText: isDrawnTurn
        ? "轮到你出牌"
        : "轮到你出牌 · 碰后请择牌打出",
      waitHintText: null,
      dimViewerHand: false,
      centerEdgeGlowSlot,
    };
  }

  const waitHintText =
    pendingAddedKong !== null
      ? "补杠待确认…"
      : activeSeat !== null && activeSeat !== playerView.viewer
        ? `等待${seatRelativeRole(playerView.viewer, activeSeat)}行动…`
        : null;

  return {
    mode: "idle",
    activeSlot,
    showStageWash: false,
    animateBreath: false,
    highlightDrawnTile: false,
    highlightClosedHand: false,
    turnPillText: null,
    waitHintText,
    dimViewerHand: false,
    centerEdgeGlowSlot,
  };
}
