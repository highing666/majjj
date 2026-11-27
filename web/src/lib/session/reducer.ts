import type { ClientMessage } from "../protocol/generated/ClientMessage";
import type { MeldView } from "../protocol/generated/MeldView";
import type { PlayerEvent } from "../protocol/generated/PlayerEvent";
import type { PlayerHandView } from "../protocol/generated/PlayerHandView";
import type { PlayerView } from "../protocol/generated/PlayerView";
import type { Seat } from "../protocol/generated/Seat";
import type { ServerMessage } from "../protocol/generated/ServerMessage";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import type { Tile } from "../protocol/generated/Tile";
import { parseMeld } from "../protocol/melds";
import { ALL_SEATS, seatIndex } from "../protocol/seats";

export type ConnectionStatus = "disconnected" | "connecting" | "connected";

export interface PendingAddedKong {
  seat: Seat;
  tile: Tile;
}

export interface LlmStatusNoticeState {
  available: boolean;
  message: string;
}

export interface TtsStatusNoticeState {
  available: boolean;
  message: string;
}

export interface SessionState {
  connectionStatus: ConnectionStatus;
  snapshot: TableSnapshot | null;
  pendingActionId: string | null;
  pendingAddedKong: PendingAddedKong | null;
  replacedByNewTab: boolean;
  remoteAccessDisabledMessage: string | null;
  lastRejectedReason: string | null;
  lastErrorNotice: string | null;
  llmStatusNotice: LlmStatusNoticeState | null;
  ttsStatusNotice: TtsStatusNoticeState | null;
  lastPongTsMs: number | null;
}

export type SessionEffect =
  | { type: "send"; message: ClientMessage }
  | { type: "persist_credential"; table_id: string; seat_token: string }
  | { type: "stop_reconnect" };

export interface ReduceResult {
  state: SessionState;
  effects: SessionEffect[];
}

export { seatIndex };

export function createInitialSessionState(): SessionState {
  return {
    connectionStatus: "disconnected",
    snapshot: null,
    pendingActionId: null,
    pendingAddedKong: null,
    replacedByNewTab: false,
    remoteAccessDisabledMessage: null,
    lastRejectedReason: null,
    lastErrorNotice: null,
    llmStatusNotice: null,
    ttsStatusNotice: null,
    lastPongTsMs: null,
  };
}

function clonePlayerView(view: PlayerView): PlayerView {
  return {
    ...view,
    hand: {
      closed_tiles: [...view.hand.closed_tiles],
      drawn_tile: view.hand.drawn_tile,
    },
    seats: view.seats.map((seatView) => ({
      ...seatView,
      river: [...seatView.river],
      melds: [...seatView.melds],
    })) as PlayerView["seats"],
    turn: { ...view.turn },
    legal_actions: [...view.legal_actions],
    ready_seats: view.ready_seats
      ? ([...view.ready_seats] as [boolean, boolean, boolean, boolean])
      : null,
  };
}

function consumeViewerTiles(
  hand: PlayerHandView,
  tiles: readonly Tile[],
): PlayerHandView {
  const closed = [...hand.closed_tiles];
  let drawn = hand.drawn_tile;

  for (const tile of tiles) {
    if (drawn === tile) {
      drawn = null;
    } else {
      const idx = closed.indexOf(tile);
      if (idx !== -1) {
        closed.splice(idx, 1);
      }
    }
  }

  if (drawn !== null) {
    closed.push(drawn);
    closed.sort((a, b) => a - b);
    drawn = null;
  }

  return { closed_tiles: closed, drawn_tile: drawn };
}

function discardViewerTile(
  hand: PlayerHandView,
  tile: Tile,
  isTsumogiri: boolean,
): PlayerHandView {
  if (isTsumogiri) {
    return {
      closed_tiles: [...hand.closed_tiles],
      drawn_tile: null,
    };
  }

  const closed = [...hand.closed_tiles];
  const idx = closed.indexOf(tile);
  if (idx !== -1) {
    closed.splice(idx, 1);
  }
  if (hand.drawn_tile !== null) {
    closed.push(hand.drawn_tile);
    closed.sort((a, b) => a - b);
  }
  return {
    closed_tiles: closed,
    drawn_tile: null,
  };
}

function markRiverClaimed(
  seats: PlayerView["seats"],
  fromSeat: Seat,
  tile: Tile,
  claimer: Seat,
): void {
  const river = seats[seatIndex(fromSeat)].river;
  for (let i = river.length - 1; i >= 0; i -= 1) {
    const entry = river[i];
    if (entry.tile === tile && entry.claimed_by === null) {
      river[i] = { ...entry, claimed_by: claimer };
      return;
    }
  }
}

function applyMeldDeclared(
  view: PlayerView,
  actor: Seat,
  meld: MeldView,
  eventSeq: number,
): void {
  const actorSeat = view.seats[seatIndex(actor)];
  actorSeat.last_draw_or_meld_seq = eventSeq;
  const parsed = parseMeld(meld);

  let handDelta = 0;
  let viewerConsumed: Tile[] = [];

  switch (parsed.kind) {
    case "Pon":
      markRiverClaimed(view.seats, parsed.fromSeat, parsed.tile, actor);
      actorSeat.melds.push(meld);
      handDelta = 2;
      viewerConsumed = [parsed.tile, parsed.tile];
      break;
    case "ExposedKong":
      markRiverClaimed(view.seats, parsed.fromSeat, parsed.tile, actor);
      actorSeat.melds.push(meld);
      handDelta = 3;
      viewerConsumed = [parsed.tile, parsed.tile, parsed.tile];
      break;
    case "Chi": {
      markRiverClaimed(view.seats, parsed.fromSeat, parsed.calledTile, actor);
      actorSeat.melds.push(meld);
      handDelta = 2;
      let skippedCalled = false;
      for (const t of parsed.tiles) {
        if (!skippedCalled && t === parsed.calledTile) {
          skippedCalled = true;
        } else {
          viewerConsumed.push(t);
        }
      }
      break;
    }
    case "AddedKong": {
      const ponIdx = actorSeat.melds.findIndex((existing) => {
        const parsedExisting = parseMeld(existing);
        return parsedExisting.kind === "Pon" && parsedExisting.tile === parsed.tile;
      });
      if (ponIdx !== -1) {
        actorSeat.melds[ponIdx] = meld;
      } else {
        actorSeat.melds.push(meld);
      }
      handDelta = 1;
      viewerConsumed = [parsed.tile];
      break;
    }
    case "ConcealedKong":
      actorSeat.melds.push(meld);
      handDelta = 4;
      if (parsed.tile !== null) {
        viewerConsumed = [parsed.tile, parsed.tile, parsed.tile, parsed.tile];
      }
      break;
  }

  actorSeat.hand_count = Math.max(0, actorSeat.hand_count - handDelta);
  if (actor === view.viewer && viewerConsumed.length > 0) {
    view.hand = consumeViewerTiles(view.hand, viewerConsumed);
  }
}

function applyPlayerEvent(
  snapshot: TableSnapshot,
  view: PlayerView,
  pendingAddedKong: PendingAddedKong | null,
  event: PlayerEvent,
): PendingAddedKong | null {
  const { kind } = event;

  if ("MatchStarted" in kind) {
    const { mode, initial_dealer, initial_scores, rule_profile } =
      kind.MatchStarted;
    snapshot.phase = "InMatch";
    view.mode = mode;
    view.rule_profile = rule_profile;
    view.turn.dealer = initial_dealer;
    for (let i = 0; i < 4; i += 1) {
      view.seats[i].score = initial_scores[i];
    }
    return pendingAddedKong;
  }

  if ("HandStarted" in kind) {
    const {
      wind_round,
      hand_number,
      dealer_streak,
      dealer,
      hand,
      wall_remaining,
      scores,
    } = kind.HandStarted;
    view.phase = "Playing";
    view.turn = {
      wind_round,
      hand_number,
      dealer_streak,
      dealer,
      active_seat: dealer,
    };
    view.hand = {
      closed_tiles: [...hand.closed_tiles],
      drawn_tile: hand.drawn_tile,
    };
    view.wall_remaining = wall_remaining;
    view.last_settlement = null;
    view.ready_seats = null;
    view.match_summary = null;
    for (let i = 0; i < 4; i += 1) {
      view.seats[i] = {
        seat: ALL_SEATS[i],
        score: scores[i],
        hand_count: ALL_SEATS[i] === dealer ? 14 : 13,
        river: [],
        melds: [],
        last_draw_or_meld_seq: 0,
      };
    }
    return null;
  }

  if ("TileDrawn" in kind) {
    const { seat, tile, wall_remaining } = kind.TileDrawn;
    view.wall_remaining = wall_remaining;
    view.turn.active_seat = seat;
    const seatView = view.seats[seatIndex(seat)];
    seatView.hand_count += 1;
    seatView.last_draw_or_meld_seq = event.seq;
    if (seat === view.viewer) {
      view.hand.drawn_tile = tile;
    }
    return pendingAddedKong;
  }

  if ("TileDiscarded" in kind) {
    const { seat, tile, is_tsumogiri } = kind.TileDiscarded;
    const seatView = view.seats[seatIndex(seat)];
    seatView.hand_count = Math.max(0, seatView.hand_count - 1);
    seatView.river.push({
      tile,
      is_tsumogiri,
      claimed_by: null,
      seq: event.seq,
    });
    if (seat === view.viewer) {
      view.hand = discardViewerTile(view.hand, tile, is_tsumogiri);
    }
    return pendingAddedKong;
  }

  if ("AddedKongAttempted" in kind) {
    const { seat, tile } = kind.AddedKongAttempted;
    view.turn.active_seat = seat;
    return { seat, tile };
  }

  if ("MeldDeclared" in kind) {
    const { seat, meld } = kind.MeldDeclared;
    view.turn.active_seat = seat;
    applyMeldDeclared(view, seat, meld, event.seq);
    return null;
  }

  if ("HandSettled" in kind) {
    const { settlement } = kind.HandSettled;
    view.phase = "BetweenHands";
    view.turn.active_seat = null;
    view.last_settlement = settlement;
    view.ready_seats = [false, false, false, false];
    for (let i = 0; i < 4; i += 1) {
      view.seats[i].score = settlement.new_scores[i];
    }
    return null;
  }

  if ("NextHandConfirmed" in kind) {
    const { seat } = kind.NextHandConfirmed;
    if (view.ready_seats !== null) {
      view.ready_seats[seatIndex(seat)] = true;
    }
    return pendingAddedKong;
  }

  if ("MatchFinished" in kind) {
    const { summary } = kind.MatchFinished;
    view.phase = "MatchFinished";
    view.turn.active_seat = null;
    view.ready_seats = null;
    view.match_summary = summary;
    for (let i = 0; i < 4; i += 1) {
      view.seats[i].score = summary.final_scores[i];
    }
    snapshot.phase = "Finished";
    snapshot.pending_destructive_op = null;
    return null;
  }

  return pendingAddedKong;
}

function reduceEventBatch(
  state: SessionState,
  batch: Extract<ServerMessage, { EventBatch: unknown }>["EventBatch"],
): ReduceResult {
  if (state.snapshot === null || state.snapshot.player_view === null) {
    return {
      state: {
        ...state,
        pendingActionId: null,
      },
      effects: [{ type: "send", message: "RequestSnapshot" }],
    };
  }

  const currentSeq = state.snapshot.seq;
  const { from_seq, to_seq, events, updated_legal_actions } = batch;

  if (events.length === 0) {
    if (from_seq > currentSeq + 1 || to_seq > currentSeq) {
      return {
        state,
        effects: [{ type: "send", message: "RequestSnapshot" }],
      };
    }
    if (to_seq < currentSeq) {
      return { state, effects: [] };
    }

    const nextView = clonePlayerView(state.snapshot.player_view);
    nextView.legal_actions = [...updated_legal_actions];
    return {
      state: {
        ...state,
        snapshot: {
          ...state.snapshot,
          player_view: nextView,
        },
        pendingActionId: null,
        lastRejectedReason: null,
        lastErrorNotice: null,
      },
      effects: [],
    };
  }

  if (from_seq > currentSeq + 1) {
    return {
      state,
      effects: [{ type: "send", message: "RequestSnapshot" }],
    };
  }

  if (to_seq <= currentSeq) {
    return {
      state:
        to_seq === currentSeq
          ? { ...state, pendingActionId: null }
          : state,
      effects: [],
    };
  }

  const nextEvents = events.filter((event) => event.seq > currentSeq);
  let expectedSeq = currentSeq + 1;
  for (const event of nextEvents) {
    if (event.seq !== expectedSeq) {
      return {
        state,
        effects: [{ type: "send", message: "RequestSnapshot" }],
      };
    }
    expectedSeq += 1;
  }
  if (expectedSeq - 1 !== to_seq) {
    return {
      state,
      effects: [{ type: "send", message: "RequestSnapshot" }],
    };
  }

  const nextSnapshot: TableSnapshot = { ...state.snapshot };
  const nextView = clonePlayerView(state.snapshot.player_view);
  let nextPendingAddedKong = state.pendingAddedKong;

  for (const event of nextEvents) {
    nextPendingAddedKong = applyPlayerEvent(
      nextSnapshot,
      nextView,
      nextPendingAddedKong,
      event,
    );
  }

  nextView.legal_actions = [...updated_legal_actions];
  nextView.seq = to_seq;
  nextSnapshot.seq = to_seq;
  nextSnapshot.player_view = nextView;

  return {
    state: {
      ...state,
      snapshot: nextSnapshot,
      pendingActionId: null,
      pendingAddedKong: nextPendingAddedKong,
      lastRejectedReason: null,
      lastErrorNotice: null,
    },
    effects: [],
  };
}

export function reduceServerMessage(
  state: SessionState,
  message: ServerMessage,
): ReduceResult {
  if (message === "SessionReplaced") {
    return {
      state: {
        ...state,
        replacedByNewTab: true,
        pendingActionId: null,
      },
      effects: [{ type: "stop_reconnect" }],
    };
  }

  if ("RemoteAccessDisabled" in message) {
    return {
      state: {
        ...state,
        remoteAccessDisabledMessage: message.RemoteAccessDisabled.message,
        pendingActionId: null,
      },
      effects: [{ type: "stop_reconnect" }],
    };
  }

  if ("TableSnapshot" in message) {
    const snapshot = message.TableSnapshot;
    const effects: SessionEffect[] = [];
    if (snapshot.seat_token !== null) {
      effects.push({
        type: "persist_credential",
        table_id: snapshot.table_id,
        seat_token: snapshot.seat_token,
      });
    }
    return {
      state: {
        ...state,
        snapshot,
        pendingActionId: null,
        pendingAddedKong: null,
        lastRejectedReason: null,
        lastErrorNotice: null,
      },
      effects,
    };
  }

  if ("EventBatch" in message) {
    return reduceEventBatch(state, message.EventBatch);
  }

  if ("LobbyUpdated" in message) {
    if (state.snapshot === null) {
      return {
        state,
        effects: [{ type: "send", message: "RequestSnapshot" }],
      };
    }
    const { phase, config, seats, slow_think_nudge_enabled } =
      message.LobbyUpdated;
    const nextSnapshot: TableSnapshot = {
      ...state.snapshot,
      phase,
      config,
      pace: config.pace,
      slow_think_nudge_enabled,
      seats,
      player_view: phase === "Lobby" ? null : state.snapshot.player_view,
      pending_destructive_op:
        phase === "Lobby" ? null : state.snapshot.pending_destructive_op,
    };
    return {
      state: {
        ...state,
        snapshot: nextSnapshot,
        pendingAddedKong: phase === "Lobby" ? null : state.pendingAddedKong,
        lastErrorNotice: null,
      },
      effects: [],
    };
  }

  if ("ActionRejected" in message) {
    const { reason, current_seq } = message.ActionRejected;
    const needsResync =
      state.snapshot === null || state.snapshot.seq !== current_seq;
    return {
      state: {
        ...state,
        pendingActionId: null,
        lastRejectedReason: reason,
      },
      effects: needsResync
        ? [{ type: "send", message: "RequestSnapshot" }]
        : [],
    };
  }

  if ("ErrorNotice" in message) {
    return {
      state: {
        ...state,
        pendingActionId: null,
        lastErrorNotice: message.ErrorNotice.message,
      },
      effects: [],
    };
  }

  if ("TableBanter" in message) {
    if (state.snapshot === null) {
      return { state, effects: [] };
    }
    const { seat, persona_id, mood } = message.TableBanter;
    const idx = seatIndex(seat);
    const currentSeat = state.snapshot.seats[idx];
    if (
      currentSeat.kind !== "Ai" ||
      (currentSeat.persona_id !== null && currentSeat.persona_id !== persona_id)
    ) {
      return { state, effects: [] };
    }
    const nextSeats = state.snapshot.seats.map((entry, i) =>
      i === idx ? { ...entry, persona_id, mood } : entry,
    ) as TableSnapshot["seats"];
    return {
      state: {
        ...state,
        snapshot: {
          ...state.snapshot,
          seats: nextSeats,
        },
      },
      effects: [],
    };
  }

  if ("QuickEmoteBroadcast" in message) {
    if (
      state.snapshot !== null &&
      message.QuickEmoteBroadcast.seat === state.snapshot.your_seat &&
      state.lastRejectedReason !== null
    ) {
      return {
        state: {
          ...state,
          lastRejectedReason: null,
        },
        effects: [],
      };
    }
    return { state, effects: [] };
  }

  if ("LlmStatusNotice" in message) {
    const { available, message: noticeText } = message.LlmStatusNotice;
    return {
      state: {
        ...state,
        llmStatusNotice: {
          available,
          message: noticeText,
        },
      },
      effects: [],
    };
  }

  if ("TtsStatusNotice" in message) {
    const { available, message: noticeText } = message.TtsStatusNotice;
    return {
      state: {
        ...state,
        ttsStatusNotice: {
          available,
          message: noticeText,
        },
      },
      effects: [],
    };
  }

  if ("TableBanterAudioReady" in message) {
    return { state, effects: [] };
  }

  if ("TableBanterAudioFallback" in message) {
    return { state, effects: [] };
  }

  if ("ServerPong" in message) {
    return {
      state: {
        ...state,
        lastPongTsMs: message.ServerPong.ts_ms,
      },
      effects: [],
    };
  }

  if ("ServerPing" in message) {
    return { state, effects: [] };
  }

  return { state, effects: [] };
}
