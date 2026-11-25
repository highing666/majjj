import type { PlayerAction } from "../protocol/generated/PlayerAction";
import type { PlayerEvent } from "../protocol/generated/PlayerEvent";
import type { PublicPhase } from "../protocol/generated/PublicPhase";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { Tile } from "../protocol/generated/Tile";
import type { WinKind } from "../protocol/generated/WinKind";
import { parseMeld, type MeldKind } from "../protocol/melds";
import { seatIndex } from "../protocol/seats";
import { seatRelativeRole } from "../table/viewModel";

export type RelativeRole = "自家" | "下家" | "对家" | "上家";

export type SemanticGameEventType =
  | "MatchStarted"
  | "HandStarted"
  | "TileDrawn"
  | "TileDiscarded"
  | "AddedKongAttempted"
  | "MeldDeclared"
  | "HandSettled"
  | "NextHandConfirmed"
  | "MatchFinished"
  | "UiTileHover"
  | "UiTileSelect"
  | "UiActionPrompted"
  | "UiPassClicked"
  | "UiConfirmNextHand";

export interface SemanticWinnerInfo {
  seat: Seat;
  relativeRole: RelativeRole | null;
  winKind: WinKind;
  totalFan: number;
  voicePackId: string | null;
}

export interface SemanticGameEvent {
  type: SemanticGameEventType;
  seq: number | null;
  seat: Seat | null;
  relativeRole: RelativeRole | null;
  voicePackId: string | null;
  wallRemaining: number;
  isTsumogiri: boolean;
  totalFan: number | null;
  winKind: WinKind | null;
  winners: readonly SemanticWinnerInfo[];
  meldKind: MeldKind | null;
  tile: Tile | null;
  isReplacement: boolean;
  isDrawGame: boolean;
  isMultiRon: boolean;
  discarder: Seat | null;
  kongScoreDeltas: readonly [number, number, number, number] | null;
  fromDrawn: boolean | null;
  actions: readonly PlayerAction[];
}

export type LocalUiAction =
  | { type: "UiTileHover"; tile: Tile; fromDrawn: boolean }
  | { type: "UiTileSelect"; tile: Tile; fromDrawn: boolean }
  | { type: "UiActionPrompted"; actions: readonly PlayerAction[] }
  | { type: "UiPassClicked" }
  | { type: "UiConfirmNextHand" };

export interface SemanticMappingContext {
  viewer: Seat | null;
  seats: readonly [SeatInfo, SeatInfo, SeatInfo, SeatInfo] | null;
  wallRemaining: number;
}

function resolveRole(
  viewer: Seat | null,
  seat: Seat | null,
): RelativeRole | null {
  if (viewer === null || seat === null) {
    return null;
  }
  return seatRelativeRole(viewer, seat);
}

function resolveVoicePackId(
  seats: readonly [SeatInfo, SeatInfo, SeatInfo, SeatInfo] | null,
  seat: Seat | null,
): string | null {
  if (seats === null || seat === null) {
    return null;
  }
  return seats[seatIndex(seat)]?.voice_pack_id ?? null;
}

function makeBaseEvent(
  type: SemanticGameEventType,
  context: SemanticMappingContext,
  seat: Seat | null,
  overrides: Partial<SemanticGameEvent> = {},
): SemanticGameEvent {
  return {
    type,
    seq: null,
    seat,
    relativeRole: resolveRole(context.viewer, seat),
    voicePackId: resolveVoicePackId(context.seats, seat),
    wallRemaining: context.wallRemaining,
    isTsumogiri: false,
    totalFan: null,
    winKind: null,
    winners: [],
    meldKind: null,
    tile: null,
    isReplacement: false,
    isDrawGame: false,
    isMultiRon: false,
    discarder: null,
    kongScoreDeltas: null,
    fromDrawn: null,
    actions: [],
    ...overrides,
  };
}

/**
 * Pure mapping layer turning a protocol `PlayerEvent` and public table context
 * into a normalized `SemanticGameEvent` for audio and visual buses.
 */
export function mapPlayerEventToSemanticEvent(
  event: PlayerEvent,
  context: SemanticMappingContext,
): SemanticGameEvent {
  const { kind, seq } = event;

  if ("MatchStarted" in kind) {
    return makeBaseEvent(
      "MatchStarted",
      context,
      kind.MatchStarted.initial_dealer,
      { seq },
    );
  }

  if ("HandStarted" in kind) {
    return makeBaseEvent("HandStarted", context, kind.HandStarted.dealer, {
      seq,
      wallRemaining: kind.HandStarted.wall_remaining,
    });
  }

  if ("TileDrawn" in kind) {
    const { seat, tile, is_replacement, wall_remaining } = kind.TileDrawn;
    return makeBaseEvent("TileDrawn", context, seat, {
      seq,
      tile,
      isReplacement: is_replacement,
      wallRemaining: wall_remaining,
    });
  }

  if ("TileDiscarded" in kind) {
    const { seat, tile, is_tsumogiri } = kind.TileDiscarded;
    return makeBaseEvent("TileDiscarded", context, seat, {
      seq,
      tile,
      isTsumogiri: is_tsumogiri,
    });
  }

  if ("AddedKongAttempted" in kind) {
    const { seat, tile } = kind.AddedKongAttempted;
    return makeBaseEvent("AddedKongAttempted", context, seat, {
      seq,
      tile,
      meldKind: "AddedKong",
    });
  }

  if ("MeldDeclared" in kind) {
    const { seat, meld, kong_score_deltas } = kind.MeldDeclared;
    const parsed = parseMeld(meld);
    const tile =
      parsed.kind === "Chi" ? parsed.calledTile : parsed.tile;
    return makeBaseEvent("MeldDeclared", context, seat, {
      seq,
      meldKind: parsed.kind,
      tile,
      kongScoreDeltas: kong_score_deltas,
    });
  }

  if ("HandSettled" in kind) {
    const { settlement } = kind.HandSettled;
    if (settlement.outcome === "DrawGame") {
      return makeBaseEvent("HandSettled", context, null, {
        seq,
        isDrawGame: true,
      });
    }

    const { winners, discarder, winning_tile } = settlement.outcome.Win;
    const semanticWinners: SemanticWinnerInfo[] = winners.map((winner) => ({
      seat: winner.seat,
      relativeRole: resolveRole(context.viewer, winner.seat),
      winKind: winner.win_kind,
      totalFan: winner.total_fan,
      voicePackId: resolveVoicePackId(context.seats, winner.seat),
    }));
    const primary = semanticWinners[0] ?? null;
    const maxFan = semanticWinners.reduce(
      (max, winner) => Math.max(max, winner.totalFan),
      0,
    );

    return makeBaseEvent(
      "HandSettled",
      context,
      primary?.seat ?? null,
      {
        seq,
        tile: winning_tile,
        discarder,
        winKind: primary?.winKind ?? null,
        totalFan: semanticWinners.length > 0 ? maxFan : null,
        winners: semanticWinners,
        isMultiRon: semanticWinners.length > 1,
        isDrawGame: false,
      },
    );
  }

  if ("NextHandConfirmed" in kind) {
    return makeBaseEvent(
      "NextHandConfirmed",
      context,
      kind.NextHandConfirmed.seat,
      { seq },
    );
  }

  return makeBaseEvent("MatchFinished", context, null, { seq });
}

/**
 * Pure mapping layer turning local UI interactions into `SemanticGameEvent` values.
 */
export function mapLocalUiActionToSemanticEvent(
  action: LocalUiAction,
  context: SemanticMappingContext,
): SemanticGameEvent {
  switch (action.type) {
    case "UiTileHover":
      return makeBaseEvent("UiTileHover", context, context.viewer, {
        tile: action.tile,
        fromDrawn: action.fromDrawn,
      });
    case "UiTileSelect":
      return makeBaseEvent("UiTileSelect", context, context.viewer, {
        tile: action.tile,
        fromDrawn: action.fromDrawn,
      });
    case "UiActionPrompted":
      return makeBaseEvent("UiActionPrompted", context, context.viewer, {
        actions: [...action.actions],
      });
    case "UiPassClicked":
      return makeBaseEvent("UiPassClicked", context, context.viewer);
    case "UiConfirmNextHand":
      return makeBaseEvent("UiConfirmNextHand", context, context.viewer);
  }
}

function isActionablePromptAction(action: PlayerAction): boolean {
  if (action === "Tsumo") {
    return true;
  }
  if (typeof action !== "object" || action === null) {
    return false;
  }
  return (
    "Ron" in action ||
    "RobKongHu" in action ||
    "Pon" in action ||
    "ExposedKong" in action ||
    "AddedKong" in action ||
    "ConcealedKong" in action ||
    "Chi" in action
  );
}

/**
 * Extracts non-trivial prompt actions (`Tsumo`, `Ron`, `RobKongHu`, `Pon`, kongs, `Chi`),
 * excluding `Discard`, `Pass`, `ConfirmNextHand`, and `EndMatch`.
 */
export function extractActionablePromptActions(
  actions: readonly PlayerAction[],
): PlayerAction[] {
  return actions.filter(isActionablePromptAction);
}

function serializeActionKey(action: PlayerAction): string {
  return typeof action === "string" ? action : JSON.stringify(action);
}

/**
 * Returns the current actionable prompt actions strictly when the local viewer
 * gains at least one newly actionable prompt action in `Playing` phase; otherwise `null`.
 */
export function detectNewlyActionablePrompt(
  prevActions: readonly PlayerAction[],
  nextActions: readonly PlayerAction[],
  nextPhase: PublicPhase | null,
): PlayerAction[] | null {
  if (nextPhase !== "Playing") {
    return null;
  }
  const nextPrompt = extractActionablePromptActions(nextActions);
  if (nextPrompt.length === 0) {
    return null;
  }
  const prevKeys = new Set(
    extractActionablePromptActions(prevActions).map(serializeActionKey),
  );
  const hasNewPrompt = nextPrompt.some(
    (action) => !prevKeys.has(serializeActionKey(action)),
  );
  return hasNewPrompt ? nextPrompt : null;
}

/**
 * Emits a `UiActionPrompted` semantic event strictly when the local viewer
 * (non-spectator) gains newly actionable legal prompt actions.
 */
export function createUiActionPromptedEvent(
  prevActions: readonly PlayerAction[],
  nextActions: readonly PlayerAction[],
  nextPhase: PublicPhase | null,
  context: SemanticMappingContext,
): SemanticGameEvent | null {
  if (context.viewer === null) {
    return null;
  }
  const newlyActionable = detectNewlyActionablePrompt(
    prevActions,
    nextActions,
    nextPhase,
  );
  if (newlyActionable === null) {
    return null;
  }
  return mapLocalUiActionToSemanticEvent(
    { type: "UiActionPrompted", actions: newlyActionable },
    context,
  );
}
