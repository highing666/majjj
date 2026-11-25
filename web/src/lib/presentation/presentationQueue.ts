import type { PlayerAction } from "../protocol/generated/PlayerAction";
import type { PlayerEvent } from "../protocol/generated/PlayerEvent";
import type { ServerMessage } from "../protocol/generated/ServerMessage";
import type { TablePace } from "../protocol/generated/TablePace";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
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
  type LocalUiAction,
  type SemanticGameEvent,
  type SemanticMappingContext,
} from "./semanticEvents";

export interface PresentationScheduler {
  setTimeout(callback: () => void, delayMs: number): unknown;
  clearTimeout(handle: unknown): void;
}

export interface PresentationState {
  snapshot: TableSnapshot | null;
  pendingAddedKong: PendingAddedKong | null;
  activeImpact: PiltoverImpactFrame | null;
  settlementVisible: boolean;
  queueLength: number;
  isPlaying: boolean;
  pace: TablePace;
}

export interface PresentationQueueOptions {
  effectsBus?: PresentationEffectsBus;
  scheduler?: PresentationScheduler;
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

  let sessionState = createInitialSessionState();
  let activeImpact: PiltoverImpactFrame | null = null;
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
      settlementVisible,
      queueLength: queuedItems.length,
      isPlaying: activeTimer !== null,
      pace,
    };
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
    const prevView = sessionState.snapshot?.player_view ?? null;
    clearQueueAndEffects();
    const reduced = reduceServerMessage(sessionState, message);
    sessionState = reduced.state;
    pace = message.TableSnapshot.pace;
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
      settlementVisible = false;
    }
    const reduced = reduceServerMessage(sessionState, message);
    if (!reduced.effects.some((e) => e.type === "send")) {
      sessionState = reduced.state;
    }
    emitState();
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
      unsubPiltover();
      stateListeners.clear();
      semanticListeners.clear();
    },
  };
}
