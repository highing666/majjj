import type { PersonaId } from "../protocol/generated/PersonaId";
import type { PersonaMood } from "../protocol/generated/PersonaMood";
import type { PlayerAction } from "../protocol/generated/PlayerAction";
import type { PlayerEvent } from "../protocol/generated/PlayerEvent";
import type { QuickEmote } from "../protocol/generated/QuickEmote";
import type { Seat } from "../protocol/generated/Seat";
import type { ServerMessage } from "../protocol/generated/ServerMessage";
import type { TablePace } from "../protocol/generated/TablePace";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import { ALL_SEATS } from "../protocol/seats";
import {
  createInitialSessionState,
  reduceServerMessage,
  type PendingAddedKong,
  type SessionState,
} from "../session/reducer";
import {
  IMPACT_DURATION_MS,
  createPresentationEffectsBus,
  registerPiltoverImpactSubscriber,
  type PiltoverImpactFrame,
  type PresentationEffectsBus,
} from "./effectsBus";
import {
  createUiActionPromptedEvent,
  mapLocalUiActionToSemanticEvent,
  mapPlayerEventToSemanticEvent,
  mapQuickEmoteBroadcastToSemanticEvent,
  type LocalUiAction,
  type SemanticGameEvent,
  type SemanticMappingContext,
} from "./semanticEvents";

export interface PresentationScheduler {
  setTimeout(callback: () => void, delayMs: number): unknown;
  clearTimeout(handle: unknown): void;
}

export interface ActiveBanterBubble {
  kind: "banter" | "quick_emote";
  bubbleId: number;
  seat: Seat;
  personaId: PersonaId | null;
  mood: PersonaMood | null;
  emote: QuickEmote | null;
  text: string;
  eventSeq: number;
  replyToSeat: Seat | null;
}

export type ActiveBanterMap = Record<Seat, ActiveBanterBubble | null>;

export interface PresentationState {
  snapshot: TableSnapshot | null;
  pendingAddedKong: PendingAddedKong | null;
  activeImpact: PiltoverImpactFrame | null;
  activeBubbles: ActiveBanterMap;
  settlementVisible: boolean;
  queueLength: number;
  isPlaying: boolean;
  pace: TablePace;
}

export interface PresentationQueueOptions {
  effectsBus?: PresentationEffectsBus;
  scheduler?: PresentationScheduler;
  banterDurationMs?: number;
}

export type PresentationStateListener = (state: PresentationState) => void;
export type SemanticEventListener = (event: SemanticGameEvent) => void;

export interface PresentationQueue {
  getState(): PresentationState;
  subscribe(listener: PresentationStateListener): () => void;
  onSemanticEvent(listener: SemanticEventListener): () => void;
  enqueueServerMessage(message: ServerMessage): void;
  emitLocalUiAction(action: LocalUiAction): void;
  skipActiveImpact(): boolean;
  dispose(): void;
}

type QueueItem =
  | {
      kind: "event";
      event: PlayerEvent;
      updatedLegalActions: readonly PlayerAction[];
      applyLegalActions: boolean;
    }
  | {
      kind: "legal_sync";
      fromSeq: number;
      toSeq: number;
      updatedLegalActions: readonly PlayerAction[];
    };

const PACE_MULTIPLIER: Record<TablePace, number> = {
  Standard: 1.0,
  Fast: 0.6,
};

const BACKLOG_ACCEL_THRESHOLD = 3;
const BACKLOG_ACCEL_FACTOR = 1.6;
const DEFAULT_BANTER_DURATION_MS = 4500;

function createEmptyBanterMap(): ActiveBanterMap {
  return {
    East: null,
    South: null,
    West: null,
    North: null,
  };
}

const REGULAR_EVENT_DURATION_MS: Record<string, number> = {
  TileDrawn: 140,
  TileDiscarded: 180,
  AddedKongAttempted: 180,
  MeldDeclared: IMPACT_DURATION_MS.MeldDeclared,
};

const defaultScheduler: PresentationScheduler = {
  setTimeout: (cb, ms) => globalThis.setTimeout(cb, ms),
  clearTimeout: (handle) =>
    globalThis.clearTimeout(handle as ReturnType<typeof setTimeout>),
};

function isRegularQueueItem(item: QueueItem): boolean {
  if (item.kind !== "event") {
    return false;
  }
  const kindKey = Object.keys(item.event.kind)[0];
  return kindKey !== undefined && kindKey in REGULAR_EVENT_DURATION_MS;
}

function resolveBaseDurationMs(event: PlayerEvent): number {
  const { kind } = event;
  if ("HandSettled" in kind) {
    return kind.HandSettled.settlement.outcome === "DrawGame"
      ? IMPACT_DURATION_MS.HandSettledDraw
      : IMPACT_DURATION_MS.HandSettledWin;
  }
  const kindKey = Object.keys(kind)[0];
  return (kindKey !== undefined ? REGULAR_EVENT_DURATION_MS[kindKey] : undefined) ?? 0;
}

function shouldShowSettlement(snapshot: TableSnapshot | null): boolean {
  return (
    snapshot !== null &&
    snapshot.phase === "InMatch" &&
    snapshot.player_view?.phase === "BetweenHands" &&
    snapshot.player_view.last_settlement !== null
  );
}

function buildSemanticContext(
  session: SessionState,
): SemanticMappingContext {
  const snapshot = session.snapshot;
  return {
    viewer: snapshot?.your_seat ?? null,
    seats: snapshot?.seats ?? null,
    wallRemaining: snapshot?.player_view?.wall_remaining ?? 0,
  };
}

/**
 * Serial presentation queue separating authoritative `SessionState` from paced
 * visual/audio presentation (`PresentationState`).
 */
export function createPresentationQueue(
  options: PresentationQueueOptions = {},
): PresentationQueue {
  const effectsBus = options.effectsBus ?? createPresentationEffectsBus();
  const scheduler = options.scheduler ?? defaultScheduler;
  const banterDurationMs = options.banterDurationMs ?? DEFAULT_BANTER_DURATION_MS;

  let sessionState = createInitialSessionState();
  let activeImpact: PiltoverImpactFrame | null = null;
  let activeBubbles: ActiveBanterMap = createEmptyBanterMap();
  const bubbleTimers: Partial<Record<Seat, unknown>> = {};
  let nextBubbleId = 1;
  let settlementVisible = false;
  let pace: TablePace = "Standard";
  let activeTimer: unknown | null = null;
  let activeItem: QueueItem | null = null;
  let disposed = false;

  const queuedItems: QueueItem[] = [];
  const stateListeners = new Set<PresentationStateListener>();
  const semanticListeners = new Set<SemanticEventListener>();

  const unsubPiltover = registerPiltoverImpactSubscriber(effectsBus, {
    onImpact: (frame) => {
      activeImpact = frame;
    },
    onClear: () => {
      activeImpact = null;
    },
    onReducedEffectsChange: (reduced) => {
      if (activeImpact !== null && reduced) {
        activeImpact = {
          ...activeImpact,
          dampedShake: false,
          warmFlash: false,
        };
        emitState();
      }
    },
  });

  function snapshotPresentationState(): PresentationState {
    return {
      snapshot: sessionState.snapshot,
      pendingAddedKong: sessionState.pendingAddedKong,
      activeImpact,
      activeBubbles: { ...activeBubbles },
      settlementVisible,
      queueLength: queuedItems.length,
      isPlaying: activeTimer !== null,
      pace,
    };
  }

  function clearSeatBubble(seat: Seat): void {
    const timer = bubbleTimers[seat];
    if (timer !== undefined) {
      scheduler.clearTimeout(timer);
      delete bubbleTimers[seat];
    }
    if (activeBubbles[seat] !== null) {
      activeBubbles = { ...activeBubbles, [seat]: null };
    }
  }

  function clearAllBubbles(): void {
    for (const seat of ALL_SEATS) {
      clearSeatBubble(seat);
    }
  }

  function pruneBubblesForSeats(seats: TableSnapshot["seats"]): void {
    for (const info of seats) {
      const bubble = activeBubbles[info.seat];
      if (bubble === null) {
        continue;
      }
      if (
        (bubble.kind === "banter" && info.kind !== "Ai") ||
        (bubble.kind === "quick_emote" && info.kind !== "Human")
      ) {
        clearSeatBubble(info.seat);
      }
    }
  }

  function emitState(): void {
    const state = snapshotPresentationState();
    for (const listener of stateListeners) {
      listener(state);
    }
  }

  function emitSemantic(event: SemanticGameEvent): void {
    for (const listener of semanticListeners) {
      listener(event);
    }
  }

  function cancelActiveTimer(): void {
    if (activeTimer !== null) {
      scheduler.clearTimeout(activeTimer);
      activeTimer = null;
    }
  }

  function clearQueueAndEffects(): void {
    cancelActiveTimer();
    queuedItems.length = 0;
    activeItem = null;
    if (activeImpact !== null) {
      activeImpact = null;
    }
    effectsBus.clear();
  }

  function maybeEmitPrompted(
    prevView: NonNullable<SessionState["snapshot"]>["player_view"],
    nextSession: SessionState,
  ): void {
    const nextView = nextSession.snapshot?.player_view ?? null;
    const prompted = createUiActionPromptedEvent(
      prevView?.legal_actions ?? [],
      nextView?.legal_actions ?? [],
      nextView?.phase ?? null,
      buildSemanticContext(nextSession),
    );
    if (prompted !== null) {
      emitSemantic(prompted);
      effectsBus.emit(prompted);
    }
  }

  function applyLegalSyncItem(
    item: Extract<QueueItem, { kind: "legal_sync" }>,
  ): void {
    const prevView = sessionState.snapshot?.player_view ?? null;
    const reduced = reduceServerMessage(sessionState, {
      EventBatch: {
        from_seq: item.fromSeq,
        to_seq: item.toSeq,
        events: [],
        updated_legal_actions: [...item.updatedLegalActions],
      },
    });
    if (reduced.effects.some((e) => e.type === "send")) {
      return;
    }
    sessionState = reduced.state;
    maybeEmitPrompted(prevView, sessionState);
  }

  function applyEventItem(
    item: Extract<QueueItem, { kind: "event" }>,
    durationMs: number,
  ): boolean {
    const prevView = sessionState.snapshot?.player_view ?? null;
    const reduced = reduceServerMessage(sessionState, {
      EventBatch: {
        from_seq: item.event.seq,
        to_seq: item.event.seq,
        events: [item.event],
        updated_legal_actions: item.applyLegalActions
          ? [...item.updatedLegalActions]
          : [],
      },
    });
    if (reduced.effects.some((e) => e.type === "send")) {
      return false;
    }

    sessionState = reduced.state;
    if ("HandStarted" in item.event.kind) {
      clearAllBubbles();
    }
    if ("HandSettled" in item.event.kind) {
      settlementVisible = false;
    } else if (!shouldShowSettlement(sessionState.snapshot)) {
      settlementVisible = false;
    }

    const semanticEvent = mapPlayerEventToSemanticEvent(
      item.event,
      buildSemanticContext(sessionState),
    );
    emitSemantic(semanticEvent);
    effectsBus.emit(semanticEvent, {
      durationMs,
      skippable: "HandSettled" in item.event.kind,
    });
    maybeEmitPrompted(prevView, sessionState);
    return true;
  }

  function completeCurrentItem(): void {
    cancelActiveTimer();
    const finished = activeItem;
    activeItem = null;

    if (activeImpact !== null) {
      activeImpact = null;
      effectsBus.clear();
    }

    if (
      finished?.kind === "event" &&
      "HandSettled" in finished.event.kind &&
      shouldShowSettlement(sessionState.snapshot)
    ) {
      settlementVisible = true;
    }

    drainQueue();
  }

  function drainQueue(): void {
    if (disposed || activeTimer !== null) {
      return;
    }

    while (queuedItems.length > 0) {
      const regularBacklog = queuedItems.filter(isRegularQueueItem).length;
      const next = queuedItems.shift()!;

      if (next.kind === "legal_sync") {
        applyLegalSyncItem(next);
        continue;
      }

      const baseDurationMs = resolveBaseDurationMs(next.event);
      const accelerate =
        isRegularQueueItem(next) && regularBacklog >= BACKLOG_ACCEL_THRESHOLD;
      const speedFactor = accelerate ? BACKLOG_ACCEL_FACTOR : 1.0;
      const durationMs =
        baseDurationMs > 0
          ? Math.max(
              1,
              Math.round(
                (baseDurationMs * PACE_MULTIPLIER[pace]) / speedFactor,
              ),
            )
          : 0;

      const applied = applyEventItem(next, durationMs);
      if (!applied) {
        continue;
      }

      if (durationMs > 0) {
        activeItem = next;
        activeTimer = scheduler.setTimeout(() => {
          completeCurrentItem();
        }, durationMs);
        emitState();
        return;
      }

      if (
        "HandSettled" in next.event.kind &&
        shouldShowSettlement(sessionState.snapshot)
      ) {
        settlementVisible = true;
      }
    }

    emitState();
  }

  function handleSnapshot(
    message: Extract<ServerMessage, { TableSnapshot: unknown }>,
  ): void {
    const prevSnapshot = sessionState.snapshot;
    const prevView = prevSnapshot?.player_view ?? null;
    clearQueueAndEffects();
    const nextSnap = message.TableSnapshot;
    const phaseReset =
      nextSnap.phase === "Lobby" ||
      (prevSnapshot !== null &&
        (prevSnapshot.table_id !== nextSnap.table_id ||
          (prevSnapshot.phase !== nextSnap.phase &&
            !(prevSnapshot.phase === "InMatch" && nextSnap.phase === "Finished"))));
    if (phaseReset) {
      clearAllBubbles();
    } else {
      pruneBubblesForSeats(nextSnap.seats);
    }
    const reduced = reduceServerMessage(sessionState, message);
    sessionState = reduced.state;
    pace = nextSnap.pace;
    settlementVisible = shouldShowSettlement(sessionState.snapshot);
    maybeEmitPrompted(prevView, sessionState);
    emitState();
  }

  function handleLobbyUpdated(
    message: Extract<ServerMessage, { LobbyUpdated: unknown }>,
  ): void {
    pace = message.LobbyUpdated.config.pace;
    if (message.LobbyUpdated.phase === "Lobby") {
      clearQueueAndEffects();
      clearAllBubbles();
      settlementVisible = false;
    } else {
      pruneBubblesForSeats(message.LobbyUpdated.seats);
    }
    const reduced = reduceServerMessage(sessionState, message);
    if (!reduced.effects.some((e) => e.type === "send")) {
      sessionState = reduced.state;
    }
    emitState();
  }

  function scheduleSeatBubble(bubble: ActiveBanterBubble): void {
    const seat = bubble.seat;
    clearSeatBubble(seat);
    activeBubbles = {
      ...activeBubbles,
      [seat]: bubble,
    };
    const effectiveDurationMs = Math.max(
      1,
      Math.round(banterDurationMs * (pace === "Fast" ? 0.78 : 1.0)),
    );
    const id = bubble.bubbleId;
    bubbleTimers[seat] = scheduler.setTimeout(() => {
      delete bubbleTimers[seat];
      if (!disposed && activeBubbles[seat]?.bubbleId === id) {
        activeBubbles = { ...activeBubbles, [seat]: null };
        emitState();
      }
    }, effectiveDurationMs);
    emitState();
  }

  function handleTableBanter(
    message: Extract<ServerMessage, { TableBanter: unknown }>,
  ): void {
    const { seat, persona_id, mood, text, event_seq, reply_to_seat } =
      message.TableBanter;
    const trimmed = text.trim();
    if (trimmed.length === 0) {
      return;
    }
    const snap = sessionState.snapshot;
    if (snap !== null) {
      const seatInfo = snap.seats.find((entry) => entry.seat === seat);
      if (seatInfo && seatInfo.kind !== "Ai") {
        return;
      }
    }

    const reduced = reduceServerMessage(sessionState, message);
    sessionState = reduced.state;

    scheduleSeatBubble({
      kind: "banter",
      bubbleId: nextBubbleId++,
      seat,
      personaId: persona_id,
      mood,
      emote: null,
      text: trimmed,
      eventSeq: event_seq,
      replyToSeat: reply_to_seat,
    });
  }

  function handleQuickEmoteBroadcast(
    message: Extract<ServerMessage, { QuickEmoteBroadcast: unknown }>,
  ): void {
    const { seat, emote, text } = message.QuickEmoteBroadcast;
    const trimmed = text.trim();
    if (trimmed.length === 0) {
      return;
    }
    const snap = sessionState.snapshot;
    if (snap !== null) {
      const seatInfo = snap.seats.find((entry) => entry.seat === seat);
      if (seatInfo && seatInfo.kind !== "Human") {
        return;
      }
    }

    const reduced = reduceServerMessage(sessionState, message);
    sessionState = reduced.state;

    emitSemantic(
      mapQuickEmoteBroadcastToSemanticEvent(
        { seat, emote, text: trimmed },
        buildSemanticContext(sessionState),
      ),
    );

    scheduleSeatBubble({
      kind: "quick_emote",
      bubbleId: nextBubbleId++,
      seat,
      personaId: null,
      mood: null,
      emote,
      text: trimmed,
      eventSeq: snap?.seq ?? 0,
      replyToSeat: null,
    });
  }

  function handleEventBatch(
    message: Extract<ServerMessage, { EventBatch: unknown }>,
  ): void {
    const { from_seq, to_seq, events, updated_legal_actions } =
      message.EventBatch;

    if (events.length === 0) {
      queuedItems.push({
        kind: "legal_sync",
        fromSeq: from_seq,
        toSeq: to_seq,
        updatedLegalActions: updated_legal_actions,
      });
      drainQueue();
      return;
    }

    events.forEach((event, idx) => {
      const isLast = idx === events.length - 1;
      queuedItems.push({
        kind: "event",
        event,
        updatedLegalActions: isLast ? updated_legal_actions : [],
        applyLegalActions: isLast,
      });
    });
    drainQueue();
  }

  return {
    getState: snapshotPresentationState,

    subscribe(listener: PresentationStateListener): () => void {
      stateListeners.add(listener);
      listener(snapshotPresentationState());
      return () => {
        stateListeners.delete(listener);
      };
    },

    onSemanticEvent(listener: SemanticEventListener): () => void {
      semanticListeners.add(listener);
      return () => {
        semanticListeners.delete(listener);
      };
    },

    enqueueServerMessage(message: ServerMessage): void {
      if (disposed || typeof message !== "object" || message === null) {
        return;
      }
      if ("TableSnapshot" in message) {
        handleSnapshot(message);
        return;
      }
      if ("LobbyUpdated" in message) {
        handleLobbyUpdated(message);
        return;
      }
      if ("EventBatch" in message) {
        handleEventBatch(message);
        return;
      }
      if ("TableBanter" in message) {
        handleTableBanter(message);
        return;
      }
      if ("QuickEmoteBroadcast" in message) {
        handleQuickEmoteBroadcast(message);
      }
    },

    emitLocalUiAction(action: LocalUiAction): void {
      if (disposed) {
        return;
      }
      const event = mapLocalUiActionToSemanticEvent(
        action,
        buildSemanticContext(sessionState),
      );
      emitSemantic(event);
      effectsBus.emit(event);
    },

    skipActiveImpact(): boolean {
      if (disposed || activeImpact === null || !activeImpact.skippable) {
        return false;
      }
      completeCurrentItem();
      return true;
    },

    dispose(): void {
      if (disposed) {
        return;
      }
      disposed = true;
      clearQueueAndEffects();
      clearAllBubbles();
      unsubPiltover();
      stateListeners.clear();
      semanticListeners.clear();
    },
  };
}
