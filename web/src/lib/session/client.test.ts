import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClientMessage } from "../protocol/generated/ClientMessage";
import type { LobbyConfig } from "../protocol/generated/LobbyConfig";
import type { PlayerView } from "../protocol/generated/PlayerView";
import type { PublicSeatView } from "../protocol/generated/PublicSeatView";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { ServerMessage } from "../protocol/generated/ServerMessage";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import { createTableClient, type WebSocketLike } from "./client";
import { createMemoryStorage } from "./storage";

const ALL_SEATS: [Seat, Seat, Seat, Seat] = ["East", "South", "West", "North"];

class MockWebSocket implements WebSocketLike {
  readyState = 0; // CONNECTING
  readonly sent: string[] = [];
  closeCalls = 0;

  private readonly openListeners = new Set<() => void>();
  private readonly messageListeners = new Set<
    (ev: { data: unknown }) => void
  >();
  private readonly closeListeners = new Set<() => void>();
  private readonly errorListeners = new Set<() => void>();

  constructor(readonly url: string) {}

  addEventListener(type: "open", listener: () => void): void;
  addEventListener(
    type: "message",
    listener: (ev: { data: unknown }) => void,
  ): void;
  addEventListener(type: "close", listener: () => void): void;
  addEventListener(type: "error", listener: () => void): void;
  addEventListener(type: string, listener: (...args: never[]) => void): void {
    switch (type) {
      case "open":
        this.openListeners.add(listener as () => void);
        break;
      case "message":
        this.messageListeners.add(listener as (ev: { data: unknown }) => void);
        break;
      case "close":
        this.closeListeners.add(listener as () => void);
        break;
      case "error":
        this.errorListeners.add(listener as () => void);
        break;
    }
  }

  removeEventListener(type: "open", listener: () => void): void;
  removeEventListener(
    type: "message",
    listener: (ev: { data: unknown }) => void,
  ): void;
  removeEventListener(type: "close", listener: () => void): void;
  removeEventListener(type: "error", listener: () => void): void;
  removeEventListener(
    type: string,
    listener: (...args: never[]) => void,
  ): void {
    switch (type) {
      case "open":
        this.openListeners.delete(listener as () => void);
        break;
      case "message":
        this.messageListeners.delete(
          listener as (ev: { data: unknown }) => void,
        );
        break;
      case "close":
        this.closeListeners.delete(listener as () => void);
        break;
      case "error":
        this.errorListeners.delete(listener as () => void);
        break;
    }
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.closeCalls += 1;
    if (this.readyState !== 3) {
      this.readyState = 3;
      for (const listener of this.closeListeners) {
        listener();
      }
    }
  }

  emitOpen(): void {
    this.readyState = 1;
    for (const listener of this.openListeners) {
      listener();
    }
  }

  emitServerMessage(msg: ServerMessage): void {
    const payload = { data: JSON.stringify(msg) };
    for (const listener of this.messageListeners) {
      listener(payload);
    }
  }

  emitClose(): void {
    this.readyState = 3;
    for (const listener of this.closeListeners) {
      listener();
    }
  }

  parsedSent(): ClientMessage[] {
    return this.sent.map((raw) => JSON.parse(raw) as ClientMessage);
  }
}

function makeLobbyConfig(): LobbyConfig {
  return {
    mode: "EastOnly",
    seating: "Random",
    ai_difficulty: "Strong",
    pace: "Standard",
  };
}

function makeSeats(): [SeatInfo, SeatInfo, SeatInfo, SeatInfo] {
  return ALL_SEATS.map((seat) => ({
    seat,
    kind: "Empty",
    nickname: "",
    connected: false,
    ai_assisted: false,
    ai_difficulty: null,
    ai_style: null,
    voice_pack_id: null,
  })) as [SeatInfo, SeatInfo, SeatInfo, SeatInfo];
}

function makePlayerView(seq: number): PlayerView {
  return {
    viewer: "East",
    mode: "EastOnly",
    rule_profile: {
      name: "推倒胡",
      allow_chi: false,
      allow_four_of_kind_seven_pairs: true,
      allow_multi_ron: true,
      fan_cap: 8,
      wall_size: 136,
      dead_wall_size: 14,
      starting_scores_east_only: [0, 0, 0, 0],
      starting_scores_free_table: [0, 0, 0, 0],
    },
    phase: "Playing",
    hand: {
      closed_tiles: [0, 1, 2, 9, 10, 11, 18, 19, 20, 27, 27, 31, 31],
      drawn_tile: 5,
    },
    seats: [
      {
        seat: "East",
        score: 0,
        hand_count: 14,
        river: [],
        melds: [],
        last_draw_or_meld_seq: 0,
      },
      {
        seat: "South",
        score: 0,
        hand_count: 13,
        river: [],
        melds: [],
        last_draw_or_meld_seq: 0,
      },
      {
        seat: "West",
        score: 0,
        hand_count: 13,
        river: [],
        melds: [],
        last_draw_or_meld_seq: 0,
      },
      {
        seat: "North",
        score: 0,
        hand_count: 13,
        river: [],
        melds: [],
        last_draw_or_meld_seq: 0,
      },
    ] satisfies [PublicSeatView, PublicSeatView, PublicSeatView, PublicSeatView],
    wall_remaining: 69,
    turn: {
      wind_round: "East",
      hand_number: 1,
      dealer_streak: 0,
      dealer: "East",
      active_seat: "East",
    },
    legal_actions: [{ Discard: { tile: 5, from_drawn: true } }],
    last_settlement: null,
    ready_seats: null,
    match_summary: null,
    seq,
  };
}

function makeSnapshot(overrides: Partial<TableSnapshot> = {}): TableSnapshot {
  return {
    table_id: "tbl-1",
    phase: "Lobby",
    config: makeLobbyConfig(),
    pace: "Standard",
    seats: makeSeats(),
    your_seat: null,
    seat_token: "tok-1",
    player_view: null,
    pending_destructive_op: null,
    seq: 0,
    server_version: "0.1.0",
    lan_urls: {
      preferred_ipv4_url: "http://192.168.1.10:8789",
      ipv4_urls: ["http://192.168.1.10:8789"],
      local_hostname_url: "http://Mac.local:8789",
    },
    host_is_loopback: true,
    ...overrides,
  };
}

describe("TableClient", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("sends Hello on open, persists seat credential from TableSnapshot, and restores it on reconnect", () => {
    const rawStorage = createMemoryStorage();
    const sockets: MockWebSocket[] = [];
    let actionCounter = 0;

    const client = createTableClient({
      wsUrl: "ws://127.0.0.1:8789/ws",
      storage: rawStorage,
      createWebSocket: (url) => {
        const ws = new MockWebSocket(url);
        sockets.push(ws);
        return ws;
      },
      idGenerator: () => `id-${++actionCounter}`,
    });

    client.connect();
    expect(sockets).toHaveLength(1);
    sockets[0].emitOpen();
    expect(sockets[0].parsedSent()).toEqual([
      {
        Hello: {
          client_id: "id-1",
          nickname: "",
          table_id: null,
          seat_token: null,
        },
      },
    ]);

    sockets[0].emitServerMessage({
      TableSnapshot: makeSnapshot({ table_id: "tbl-9", seat_token: "tok-9" }),
    });

    // Drop connection -> reconnects after 300ms with persisted table_id and seat_token
    sockets[0].emitClose();
    vi.advanceTimersByTime(299);
    expect(sockets).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(sockets).toHaveLength(2);
    sockets[1].emitOpen();
    expect(sockets[1].parsedSent()).toEqual([
      {
        Hello: {
          client_id: "id-1",
          nickname: "",
          table_id: "tbl-9",
          seat_token: "tok-9",
        },
      },
    ]);

    client.dispose();
  });

  it("enforces local input lock on submitAction until ActionRejected or EventBatch unlocks it", () => {
    const sockets: MockWebSocket[] = [];
    let seqId = 0;

    const client = createTableClient({
      wsUrl: "ws://127.0.0.1:8789/ws",
      storage: createMemoryStorage(),
      createWebSocket: (url) => {
        const ws = new MockWebSocket(url);
        sockets.push(ws);
        return ws;
      },
      idGenerator: () => `act-${++seqId}`,
    });

    client.connect();
    sockets[0].emitOpen();
    sockets[0].emitServerMessage({
      TableSnapshot: makeSnapshot({
        phase: "InMatch",
        your_seat: "East",
        seq: 4,
        player_view: makePlayerView(4),
      }),
    });

    const firstAccepted = client.submitAction({
      Discard: { tile: 5, from_drawn: true },
    });
    const duplicateBlocked = client.submitAction({
      Discard: { tile: 0, from_drawn: false },
    });

    expect(firstAccepted).toBe(true);
    expect(duplicateBlocked).toBe(false);
    expect(sockets[0].parsedSent()).toHaveLength(2); // Hello + 1 SubmitAction
    expect(sockets[0].parsedSent()[1]).toEqual({
      SubmitAction: {
        expected_seq: 4,
        client_action_id: "act-2",
        action: { Discard: { tile: 5, from_drawn: true } },
      },
    });
    expect(client.getState().pendingActionId).toBe("act-2");

    // ActionRejected unlocks input and allows next submission
    sockets[0].emitServerMessage({
      ActionRejected: { reason: "invalid tile", current_seq: 4 },
    });
    expect(client.getState().pendingActionId).toBeNull();
    expect(client.getState().lastRejectedReason).toBe("invalid tile");

    const retryAccepted = client.submitAction({
      Discard: { tile: 5, from_drawn: true },
    });
    expect(retryAccepted).toBe(true);
    expect(sockets[0].parsedSent()).toHaveLength(3);

    client.dispose();
  });

  it("stops auto-reconnect permanently on SessionReplaced", () => {
    const sockets: MockWebSocket[] = [];
    const client = createTableClient({
      wsUrl: "ws://127.0.0.1:8789/ws",
      storage: createMemoryStorage(),
      createWebSocket: (url) => {
        const ws = new MockWebSocket(url);
        sockets.push(ws);
        return ws;
      },
    });

    client.connect();
    sockets[0].emitOpen();
    sockets[0].emitServerMessage("SessionReplaced");

    expect(client.getState().replacedByNewTab).toBe(true);
    expect(client.getState().connectionStatus).toBe("disconnected");

    vi.advanceTimersByTime(60_000);
    expect(sockets).toHaveLength(1);

    // Even explicit connect() stays disabled after SessionReplaced
    client.connect();
    expect(sockets).toHaveLength(1);
    client.dispose();
  });

  it("applies 300ms..5000ms exponential backoff on disconnects and resets after TableSnapshot", () => {
    const sockets: MockWebSocket[] = [];
    const client = createTableClient({
      wsUrl: "ws://127.0.0.1:8789/ws",
      storage: createMemoryStorage(),
      createWebSocket: (url) => {
        const ws = new MockWebSocket(url);
        sockets.push(ws);
        return ws;
      },
    });

    client.connect();
    const expectedDelays = [300, 600, 1200, 2400, 4800, 5000, 5000];
    for (let i = 0; i < expectedDelays.length; i += 1) {
      sockets[i].emitClose();
      vi.advanceTimersByTime(expectedDelays[i] - 1);
      expect(sockets).toHaveLength(i + 1);
      vi.advanceTimersByTime(1);
      expect(sockets).toHaveLength(i + 2);
    }

    // Successful snapshot resets backoff back to 300ms
    const latest = sockets[sockets.length - 1];
    latest.emitOpen();
    latest.emitServerMessage({ TableSnapshot: makeSnapshot() });
    latest.emitClose();

    const countBefore = sockets.length;
    vi.advanceTimersByTime(299);
    expect(sockets).toHaveLength(countBefore);
    vi.advanceTimersByTime(1);
    expect(sockets).toHaveLength(countBefore + 1);

    client.dispose();
  });

  it("sends ClientLeaving on pagehide, RequestSnapshot on seq gap, and lobby/nickname updates", () => {
    const sockets: MockWebSocket[] = [];
    const client = createTableClient({
      wsUrl: "ws://127.0.0.1:8789/ws",
      storage: createMemoryStorage(),
      createWebSocket: (url) => {
        const ws = new MockWebSocket(url);
        sockets.push(ws);
        return ws;
      },
      idGenerator: () => "client-1",
    });

    client.connect();
    sockets[0].emitOpen();
    sockets[0].emitServerMessage({
      TableSnapshot: makeSnapshot({
        phase: "InMatch",
        your_seat: "East",
        seq: 2,
        player_view: makePlayerView(2),
      }),
    });

    // seq gap triggers automatic RequestSnapshot
    sockets[0].emitServerMessage({
      EventBatch: {
        from_seq: 6,
        to_seq: 6,
        events: [
          {
            seq: 6,
            kind: {
              TileDiscarded: { seat: "South", tile: 1, is_tsumogiri: true },
            },
          },
        ],
        updated_legal_actions: [],
      },
    });
    expect(sockets[0].parsedSent()).toContain("RequestSnapshot");

    // Lobby commands & nickname edit
    client.updateNickname(" 小明 ");
    client.joinSeat("South");
    client.updateLobbyConfig({
      mode: "FreeTable",
      seating: "FixedOpposite",
      ai_difficulty: "Intermediate",
      pace: "Fast",
    });
    client.handlePageHide();

    const sent = sockets[0].parsedSent();
    expect(sent).toContainEqual({
      Hello: {
        client_id: "client-1",
        nickname: "小明",
        table_id: "tbl-1",
        seat_token: "tok-1",
      },
    });
    expect(sent).toContainEqual({ JoinSeat: { seat: "South" } });
    expect(sent).toContainEqual({
      UpdateLobbyConfig: {
        mode: "FreeTable",
        seating: "FixedOpposite",
        ai_difficulty: "Intermediate",
        pace: "Fast",
      },
    });
    expect(sent[sent.length - 1]).toBe("ClientLeaving");

    vi.advanceTimersByTime(10_000);
    expect(sockets).toHaveLength(1);
    client.dispose();
  });
});
