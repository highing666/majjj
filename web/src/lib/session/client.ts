import type { ClientMessage } from "../protocol/generated/ClientMessage";
import type { LobbyConfig } from "../protocol/generated/LobbyConfig";
import type { PlayerAction } from "../protocol/generated/PlayerAction";
import type { Seat } from "../protocol/generated/Seat";
import type { ServerMessage } from "../protocol/generated/ServerMessage";
import type { TablePace } from "../protocol/generated/TablePace";
import {
  createInitialSessionState,
  reduceServerMessage,
  type SessionState,
} from "./reducer";
import {
  createMemoryStorage,
  createSessionStorage,
  generateUuid,
  type PersistedIdentity,
  type StorageLike,
} from "./storage";

const WS_READY_CONNECTING = 0;
const WS_READY_OPEN = 1;
const BASE_BACKOFF_MS = 300;
const MAX_BACKOFF_MS = 5000;

export interface WebSocketLike {
  readonly readyState: number;
  send(data: string): void;
  close(code?: number, reason?: string): void;
  addEventListener(type: "open", listener: () => void): void;
  addEventListener(
    type: "message",
    listener: (ev: { data: unknown }) => void,
  ): void;
  addEventListener(type: "close", listener: () => void): void;
  addEventListener(type: "error", listener: () => void): void;
  removeEventListener(type: "open", listener: () => void): void;
  removeEventListener(
    type: "message",
    listener: (ev: { data: unknown }) => void,
  ): void;
  removeEventListener(type: "close", listener: () => void): void;
  removeEventListener(type: "error", listener: () => void): void;
}

export interface WindowEventsLike {
  addEventListener(type: string, listener: (ev: unknown) => void): void;
  removeEventListener(type: string, listener: (ev: unknown) => void): void;
}

export interface TableClientOptions {
  wsUrl?: string;
  storage?: StorageLike;
  createWebSocket?: (url: string) => WebSocketLike;
  idGenerator?: () => string;
  windowEvents?: WindowEventsLike | null;
}

export type SessionListener = (state: SessionState) => void;
export type ServerMessageListener = (message: ServerMessage) => void;

export interface TableClient {
  getState(): SessionState;
  getIdentity(): PersistedIdentity;
  subscribe(listener: SessionListener): () => void;
  onServerMessage(listener: ServerMessageListener): () => void;
  connect(): void;
  dispose(): void;
  handlePageHide(): void;
  updateNickname(nickname: string): void;
  joinSeat(seat: Seat | null): boolean;
  leaveSeat(): boolean;
  updateLobbyConfig(config: LobbyConfig): boolean;
  setPace(pace: TablePace): boolean;
  startMatch(): boolean;
  returnToLobby(): boolean;
  requestSnapshot(): boolean;
  sendPing(tsMs?: number): boolean;
  submitAction(action: PlayerAction): boolean;
  requestEndFreeTable(): boolean;
  requestDisbandMatch(): boolean;
  confirmDestructiveOp(op_id: string, accept: boolean): boolean;
}

export function defaultWsUrl(): string {
  if (typeof window === "undefined" || !window.location) {
    return "ws://127.0.0.1:8789/ws";
  }
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}/ws`;
}

export function resolveDefaultStorage(): StorageLike {
  if (typeof window !== "undefined") {
    try {
      return window.localStorage;
    } catch {
      // Fallback in restricted environments.
    }
  }
  return createMemoryStorage();
}

function defaultWebSocketFactory(url: string): WebSocketLike {
  return new WebSocket(url);
}

/**
 * Pure TypeScript WebSocket session client wrapping `reducer.ts` and `storage.ts`.
 */
export function createTableClient(
  options: TableClientOptions = {},
): TableClient {
  const wsUrl = options.wsUrl ?? defaultWsUrl();
  const idGenerator = options.idGenerator ?? generateUuid;
  const sessionStorage = createSessionStorage(
    options.storage ?? resolveDefaultStorage(),
    idGenerator,
  );
  const createWebSocket = options.createWebSocket ?? defaultWebSocketFactory;
  const windowEvents =
    options.windowEvents !== undefined
      ? options.windowEvents
      : typeof window !== "undefined"
        ? window
        : null;

  let state = createInitialSessionState();
  let socket: WebSocketLike | null = null;
  let detachSocketListeners: (() => void) | null = null;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let reconnectAttempt = 0;
  let reconnectDisabled = false;
  let disposed = false;
  let pausedForPageHide = false;

  const listeners = new Set<SessionListener>();
  const serverMessageListeners = new Set<ServerMessageListener>();

  function emitState(): void {
    for (const listener of listeners) {
      listener(state);
    }
  }

  function emitServerMessage(message: ServerMessage): void {
    for (const listener of serverMessageListeners) {
      listener(message);
    }
  }

  function setState(next: SessionState): void {
    state = next;
    emitState();
  }

  function clearReconnectTimer(): void {
    if (reconnectTimer !== null) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  }

  function closeActiveSocket(): void {
    if (detachSocketListeners !== null) {
      detachSocketListeners();
      detachSocketListeners = null;
    }
    if (socket !== null) {
      const active = socket;
      socket = null;
      active.close();
    }
  }

  function sendRaw(message: ClientMessage): boolean {
    if (socket === null || socket.readyState !== WS_READY_OPEN) {
      return false;
    }
    socket.send(JSON.stringify(message));
    return true;
  }

  function sendHello(): boolean {
    const identity = sessionStorage.loadIdentity();
    return sendRaw({
      Hello: {
        client_id: identity.client_id,
        nickname: identity.nickname,
        table_id: identity.table_id,
        seat_token: identity.seat_token,
      },
    });
  }

  function scheduleReconnect(): void {
    if (
      disposed ||
      reconnectDisabled ||
      pausedForPageHide ||
      reconnectTimer !== null
    ) {
      return;
    }
    const delayMs = Math.min(
      MAX_BACKOFF_MS,
      BASE_BACKOFF_MS * 2 ** reconnectAttempt,
    );
    reconnectAttempt += 1;
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, delayMs);
  }

  function handleServerText(rawData: unknown): void {
    if (typeof rawData !== "string") {
      return;
    }
    let parsed: ServerMessage;
    try {
      parsed = JSON.parse(rawData) as ServerMessage;
    } catch {
      return;
    }

    if (typeof parsed === "object" && parsed !== null && "TableSnapshot" in parsed) {
      reconnectAttempt = 0;
    }

    const reduced = reduceServerMessage(state, parsed);
    let nextState = reduced.state;

    let shouldStopReconnect = false;
    for (const effect of reduced.effects) {
      switch (effect.type) {
        case "persist_credential":
          sessionStorage.saveSeatCredential(effect.table_id, effect.seat_token);
          break;
        case "send":
          sendRaw(effect.message);
          break;
        case "stop_reconnect":
          shouldStopReconnect = true;
          break;
      }
    }

    if (shouldStopReconnect) {
      reconnectDisabled = true;
      clearReconnectTimer();
      nextState = {
        ...nextState,
        connectionStatus: "disconnected",
      };
      setState(nextState);
      emitServerMessage(parsed);
      closeActiveSocket();
      return;
    }

    setState(nextState);
    emitServerMessage(parsed);
  }

  function connect(): void {
    if (disposed || reconnectDisabled) {
      return;
    }
    if (
      socket !== null &&
      (socket.readyState === WS_READY_CONNECTING ||
        socket.readyState === WS_READY_OPEN)
    ) {
      return;
    }

    pausedForPageHide = false;
    clearReconnectTimer();
    setState({
      ...state,
      connectionStatus: "connecting",
    });

    const ws = createWebSocket(wsUrl);
    socket = ws;

    const onOpen = () => {
      if (socket !== ws || disposed || reconnectDisabled) {
        return;
      }
      setState({
        ...state,
        connectionStatus: "connected",
      });
      sendHello();
    };

    const onMessage = (event: { data: unknown }) => {
      if (socket !== ws || disposed) {
        return;
      }
      handleServerText(event.data);
    };

    const onClose = () => {
      if (socket !== ws) {
        return;
      }
      if (detachSocketListeners !== null) {
        detachSocketListeners();
        detachSocketListeners = null;
      }
      socket = null;
      setState({
        ...state,
        connectionStatus: "disconnected",
      });
      scheduleReconnect();
    };

    const onError = () => {
      // Allow the subsequent `close` event to drive state & backoff.
    };

    ws.addEventListener("open", onOpen);
    ws.addEventListener("message", onMessage);
    ws.addEventListener("close", onClose);
    ws.addEventListener("error", onError);

    detachSocketListeners = () => {
      ws.removeEventListener("open", onOpen);
      ws.removeEventListener("message", onMessage);
      ws.removeEventListener("close", onClose);
      ws.removeEventListener("error", onError);
    };
  }

  function handlePageHide(): void {
    pausedForPageHide = true;
    clearReconnectTimer();
    if (socket !== null && socket.readyState === WS_READY_OPEN) {
      sendRaw("ClientLeaving");
    }
  }

  function handlePageShow(): void {
    if (!pausedForPageHide || disposed || reconnectDisabled) {
      return;
    }
    pausedForPageHide = false;
    connect();
  }

  if (windowEvents !== null) {
    windowEvents.addEventListener("pagehide", handlePageHide);
    windowEvents.addEventListener("pageshow", handlePageShow);
  }

  return {
    getState(): SessionState {
      return state;
    },

    getIdentity(): PersistedIdentity {
      return sessionStorage.loadIdentity();
    },

    subscribe(listener: SessionListener): () => void {
      listeners.add(listener);
      listener(state);
      return () => {
        listeners.delete(listener);
      };
    },

    onServerMessage(listener: ServerMessageListener): () => void {
      serverMessageListeners.add(listener);
      return () => {
        serverMessageListeners.delete(listener);
      };
    },

    connect,

    dispose(): void {
      if (disposed) {
        return;
      }
      disposed = true;
      clearReconnectTimer();
      if (windowEvents !== null) {
        windowEvents.removeEventListener("pagehide", handlePageHide);
        windowEvents.removeEventListener("pageshow", handlePageShow);
      }
      closeActiveSocket();
      listeners.clear();
      serverMessageListeners.clear();
    },

    handlePageHide,

    updateNickname(nickname: string): void {
      sessionStorage.saveNickname(nickname);
      if (socket !== null && socket.readyState === WS_READY_OPEN) {
        sendHello();
      }
    },

    joinSeat(seat: Seat | null): boolean {
      return sendRaw({ JoinSeat: { seat } });
    },

    leaveSeat(): boolean {
      return sendRaw("LeaveSeat");
    },

    updateLobbyConfig(config: LobbyConfig): boolean {
      return sendRaw({ UpdateLobbyConfig: config });
    },

    setPace(pace: TablePace): boolean {
      return sendRaw({ SetPace: { pace } });
    },

    startMatch(): boolean {
      return sendRaw("StartMatch");
    },

    returnToLobby(): boolean {
      return sendRaw("ReturnToLobby");
    },

    requestSnapshot(): boolean {
      return sendRaw("RequestSnapshot");
    },

    sendPing(tsMs: number = Date.now()): boolean {
      return sendRaw({ ClientPing: { ts_ms: tsMs } });
    },

    submitAction(action: PlayerAction): boolean {
      if (
        state.pendingActionId !== null ||
        state.snapshot === null ||
        state.snapshot.player_view === null
      ) {
        return false;
      }
      const actionId = idGenerator();
      setState({
        ...state,
        pendingActionId: actionId,
        lastRejectedReason: null,
        lastErrorNotice: null,
      });
      const sent = sendRaw({
        SubmitAction: {
          expected_seq: state.snapshot.seq,
          client_action_id: actionId,
          action,
        },
      });
      if (!sent) {
        setState({
          ...state,
          pendingActionId: null,
        });
      }
      return sent;
    },

    requestEndFreeTable(): boolean {
      return sendRaw("RequestEndFreeTable");
    },

    requestDisbandMatch(): boolean {
      return sendRaw("RequestDisbandMatch");
    },

    confirmDestructiveOp(op_id: string, accept: boolean): boolean {
      return sendRaw({ ConfirmDestructiveOp: { op_id, accept } });
    },
  };
}
