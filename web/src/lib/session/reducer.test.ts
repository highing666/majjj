import { describe, expect, it } from "vitest";
import type { LobbyConfig } from "../protocol/generated/LobbyConfig";
import type { PlayerView } from "../protocol/generated/PlayerView";
import type { PublicSeatView } from "../protocol/generated/PublicSeatView";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import { ALL_SEATS } from "../protocol/seats";
import {
  createInitialSessionState,
  reduceServerMessage,
  type SessionState,
} from "./reducer";

function makeLobbyConfig(overrides: Partial<LobbyConfig> = {}): LobbyConfig {
  return {
    mode: "EastOnly",
    seating: "Random",
    ai_difficulty: "Strong",
    pace: "Standard",
    ...overrides,
  };
}

function makeLobbySeats(): [SeatInfo, SeatInfo, SeatInfo, SeatInfo] {
  return ALL_SEATS.map((seat) => ({
    seat,
    kind: "Empty",
    nickname: "",
    connected: false,
    ai_assisted: false,
    ai_difficulty: null,
    ai_style: null,
    persona_id: null,
    mood: null,
    voice_pack_id: null,
  })) as [SeatInfo, SeatInfo, SeatInfo, SeatInfo];
}

function makePlayerView(overrides: Partial<PlayerView> = {}): PlayerView {
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
    seq: 2,
    ...overrides,
  };
}

function makeSnapshot(overrides: Partial<TableSnapshot> = {}): TableSnapshot {
  return {
    table_id: "table-1",
    phase: "Lobby",
    config: makeLobbyConfig(),
    pace: "Standard",
    slow_think_nudge_enabled: true,
    seats: makeLobbySeats(),
    your_seat: null,
    seat_token: "token-1",
    player_view: null,
    pending_destructive_op: null,
    seq: 0,
    server_version: "0.1.0",
    lan_urls: {
      preferred_ipv4_url: "http://192.168.1.20:8789",
      ipv4_urls: ["http://192.168.1.20:8789"],
      local_hostname_url: "http://MacBook.local:8789",
    },
    host_is_loopback: true,
    client_is_remote: false,
    ...overrides,
  };
}

describe("session reducer — snapshot, lobby sync, and session lifecycle", () => {
  it("replaces snapshot completely, clears pending locks, and emits credential persistence", () => {
    const initial: SessionState = {
      ...createInitialSessionState(),
      pendingActionId: "act-stale",
      pendingAddedKong: { seat: "South", tile: 4 },
      lastRejectedReason: "stale seq",
    };

    const view = makePlayerView({ seq: 12 });
    const snapshot = makeSnapshot({
      phase: "InMatch",
      your_seat: "East",
      seat_token: "tok-99",
      player_view: view,
      seq: 12,
    });

    const result = reduceServerMessage(initial, { TableSnapshot: snapshot });

    expect(result.state.snapshot).toEqual(snapshot);
    expect(result.state.pendingActionId).toBeNull();
    expect(result.state.pendingAddedKong).toBeNull();
    expect(result.state.lastRejectedReason).toBeNull();
    expect(result.effects).toEqual([
      {
        type: "persist_credential",
        table_id: "table-1",
        seat_token: "tok-99",
      },
    ]);
  });

  it("synchronizes lobby config, pace, and seats on LobbyUpdated without dropping local metadata", () => {
    const start = reduceServerMessage(createInitialSessionState(), {
      TableSnapshot: makeSnapshot({ your_seat: "South" }),
    }).state;

    const updatedSeats = makeLobbySeats();
    updatedSeats[1] = {
      seat: "South",
      kind: "Human",
      nickname: "小明",
      connected: true,
      ai_assisted: false,
      ai_difficulty: null,
      ai_style: null,
      persona_id: null,
      mood: null,
      voice_pack_id: null,
    };
    updatedSeats[2] = {
      ...updatedSeats[2],
      persona_id: "TeaSarcasm",
      mood: null,
    };
    const nextConfig = makeLobbyConfig({
      mode: "FreeTable",
      seating: "FixedOpposite",
      ai_difficulty: "Intermediate",
      pace: "Fast",
    });

    const result = reduceServerMessage(start, {
      LobbyUpdated: {
        phase: "Lobby",
        config: nextConfig,
        seats: updatedSeats,
        slow_think_nudge_enabled: false,
      },
    });

    expect(result.state.snapshot?.config).toEqual(nextConfig);
    expect(result.state.snapshot?.pace).toBe("Fast");
    expect(result.state.snapshot?.slow_think_nudge_enabled).toBe(false);
    expect(result.state.snapshot?.seats[1].nickname).toBe("小明");
    expect(result.state.snapshot?.seats[2].persona_id).toBe("TeaSarcasm");
    expect(result.state.snapshot?.your_seat).toBe("South");
    expect(result.state.snapshot?.lan_urls.preferred_ipv4_url).toBe(
      "http://192.168.1.20:8789",
    );
  });

  it("marks replacedByNewTab and emits stop_reconnect on SessionReplaced", () => {
    const start: SessionState = {
      ...createInitialSessionState(),
      pendingActionId: "act-1",
    };

    const result = reduceServerMessage(start, "SessionReplaced");

    expect(result.state.replacedByNewTab).toBe(true);
    expect(result.state.pendingActionId).toBeNull();
    expect(result.effects).toEqual([{ type: "stop_reconnect" }]);
  });
});

describe("session reducer — EventBatch folding, gap recovery, and ActionRejected", () => {
  it("folds normal incremental events: draw, 手切/摸切 discard sorting, meld claiming without mutating scores", () => {
    let state = reduceServerMessage(createInitialSessionState(), {
      TableSnapshot: makeSnapshot({
        phase: "InMatch",
        your_seat: "East",
        seq: 2,
        player_view: makePlayerView({
          seq: 2,
          hand: {
            closed_tiles: [0, 1, 2, 9, 10, 11, 18, 19, 20, 27, 27, 31, 31],
            drawn_tile: 5,
          },
        }),
      }),
    }).state;
    state = { ...state, pendingActionId: "act-discard" };

    // Viewer 手切 discards tile 1 (is_tsumogiri: false) -> drawn_tile 5 folds into sorted closed_tiles
    let res = reduceServerMessage(state, {
      EventBatch: {
        from_seq: 3,
        to_seq: 4,
        events: [
          {
            seq: 3,
            kind: {
              TileDiscarded: { seat: "East", tile: 1, is_tsumogiri: false },
            },
          },
          {
            seq: 4,
            kind: {
              TileDrawn: {
                seat: "South",
                tile: null,
                is_replacement: false,
                wall_remaining: 68,
              },
            },
          },
        ],
        updated_legal_actions: [],
      },
    });
    state = res.state;
    expect(state.pendingActionId).toBeNull();
    expect(state.snapshot?.seq).toBe(4);
    expect(state.snapshot?.player_view?.hand).toEqual({
      closed_tiles: [0, 2, 5, 9, 10, 11, 18, 19, 20, 27, 27, 31, 31],
      drawn_tile: null,
    });
    expect(state.snapshot?.player_view?.seats[0].hand_count).toBe(13);
    expect(state.snapshot?.player_view?.seats[0].river).toEqual([
      { tile: 1, is_tsumogiri: false, claimed_by: null, seq: 3 },
    ]);
    expect(state.snapshot?.player_view?.seats[1].hand_count).toBe(14);
    expect(state.snapshot?.player_view?.seats[1].last_draw_or_meld_seq).toBe(4);
    expect(state.snapshot?.player_view?.wall_remaining).toBe(68);
    expect(state.snapshot?.player_view?.turn.active_seat).toBe("South");

    // South 摸切 discards tile 27; East declares ExposedKong on 27 without premature score mutation
    res = reduceServerMessage(state, {
      EventBatch: {
        from_seq: 5,
        to_seq: 7,
        events: [
          {
            seq: 5,
            kind: {
              TileDiscarded: { seat: "South", tile: 27, is_tsumogiri: true },
            },
          },
          {
            seq: 6,
            kind: {
              MeldDeclared: {
                seat: "East",
                meld: {
                  Pon: { tile: 27, from_seat: "South", seq: 6 },
                },
                kong_score_deltas: null,
              },
            },
          },
          {
            seq: 7,
            kind: {
              TileDrawn: {
                seat: "West",
                tile: null,
                is_replacement: false,
                wall_remaining: 67,
              },
            },
          },
        ],
        updated_legal_actions: [],
      },
    });
    state = res.state;
    expect(state.snapshot?.player_view?.seats[1].river[0].claimed_by).toBe(
      "East",
    );
    expect(state.snapshot?.player_view?.seats[0].melds).toEqual([
      { Pon: { tile: 27, from_seat: "South", seq: 6 } },
    ]);
    expect(state.snapshot?.player_view?.hand.closed_tiles).toEqual([
      0, 2, 5, 9, 10, 11, 18, 19, 20, 31, 31,
    ]);
    expect(state.snapshot?.player_view?.seats[0].hand_count).toBe(11);

    // ConcealedKong with kong_score_deltas must NOT prematurely mutate seat scores before HandSettled
    res = reduceServerMessage(state, {
      EventBatch: {
        from_seq: 8,
        to_seq: 9,
        events: [
          {
            seq: 8,
            kind: {
              MeldDeclared: {
                seat: "West",
                meld: { ConcealedKong: { tile: null, seq: 8 } },
                kong_score_deltas: [-2, -2, 6, -2],
              },
            },
          },
          {
            seq: 9,
            kind: {
              TileDrawn: {
                seat: "West",
                tile: null,
                is_replacement: true,
                wall_remaining: 66,
              },
            },
          },
        ],
        updated_legal_actions: [],
      },
    });
    state = res.state;
    expect(state.snapshot?.player_view?.seats[2].hand_count).toBe(11);
    expect(state.snapshot?.player_view?.seats.map((s) => s.score)).toEqual([
      0, 0, 0, 0,
    ]);
  });

  it("tracks AddedKongAttempted pending state and clears it on MeldDeclared upgrade or HandSettled", () => {
    let state = reduceServerMessage(createInitialSessionState(), {
      TableSnapshot: makeSnapshot({
        phase: "InMatch",
        your_seat: "East",
        seq: 10,
        player_view: makePlayerView({
          seq: 10,
          hand: {
            closed_tiles: [0, 2, 5, 9, 10, 11, 18, 19, 20, 31],
            drawn_tile: 27,
          },
          seats: [
            {
              seat: "East",
              score: 0,
              hand_count: 11,
              river: [],
              melds: [{ Pon: { tile: 27, from_seat: "South", seq: 6 } }],
              last_draw_or_meld_seq: 10,
            },
            {
              seat: "South",
              score: 0,
              hand_count: 13,
              river: [],
              melds: [],
              last_draw_or_meld_seq: 10,
            },
            {
              seat: "West",
              score: 0,
              hand_count: 13,
              river: [],
              melds: [],
              last_draw_or_meld_seq: 10,
            },
            {
              seat: "North",
              score: 0,
              hand_count: 13,
              river: [],
              melds: [],
              last_draw_or_meld_seq: 10,
            },
          ] satisfies [
            PublicSeatView,
            PublicSeatView,
            PublicSeatView,
            PublicSeatView,
          ],
        }),
      }),
    }).state;

    // Step 1: AddedKongAttempted marks pendingAddedKong without mutating melds or scores
    let res = reduceServerMessage(state, {
      EventBatch: {
        from_seq: 11,
        to_seq: 11,
        events: [
          {
            seq: 11,
            kind: { AddedKongAttempted: { seat: "East", tile: 27 } },
          },
        ],
        updated_legal_actions: [],
      },
    });
    state = res.state;
    expect(state.pendingAddedKong).toEqual({ seat: "East", tile: 27 });
    expect(state.snapshot?.player_view?.seats[0].melds).toEqual([
      { Pon: { tile: 27, from_seat: "South", seq: 6 } },
    ]);
    expect(state.snapshot?.player_view?.seats[0].hand_count).toBe(11);

    // Step 2: MeldDeclared upgrades Pon -> AddedKong and clears pendingAddedKong
    res = reduceServerMessage(state, {
      EventBatch: {
        from_seq: 12,
        to_seq: 12,
        events: [
          {
            seq: 12,
            kind: {
              MeldDeclared: {
                seat: "East",
                meld: { AddedKong: { tile: 27, from_seat: "South", seq: 12 } },
                kong_score_deltas: [-1, 3, -1, -1],
              },
            },
          },
        ],
        updated_legal_actions: [],
      },
    });
    state = res.state;
    expect(state.pendingAddedKong).toBeNull();
    expect(state.snapshot?.player_view?.seats[0].melds).toEqual([
      { AddedKong: { tile: 27, from_seat: "South", seq: 12 } },
    ]);
    expect(state.snapshot?.player_view?.hand.drawn_tile).toBeNull();
    expect(state.snapshot?.player_view?.seats[0].hand_count).toBe(10);
    expect(state.snapshot?.player_view?.seats[0].score).toBe(0);
  });

  it("folds HandSettled and MatchFinished in the same batch and transitions phase to Finished", () => {
    const start = reduceServerMessage(createInitialSessionState(), {
      TableSnapshot: makeSnapshot({
        phase: "InMatch",
        your_seat: "East",
        seq: 20,
        player_view: makePlayerView({ seq: 20 }),
      }),
    }).state;

    const settlement = {
      wind_round: "East" as const,
      hand_number: 4,
      dealer_streak: 0,
      dealer: "North" as const,
      outcome: "DrawGame" as const,
      kong_transfers: [],
      score_deltas: [0, 0, 0, 0] as [number, number, number, number],
      new_scores: [4, -4, 2, -2] as [number, number, number, number],
      revealed_hands: ALL_SEATS.map((seat) => ({
        seat,
        closed_tiles: [0, 1, 2],
        drawn_tile: null,
        melds: [],
      })) as [
        {
          seat: Seat;
          closed_tiles: number[];
          drawn_tile: number | null;
          melds: [];
        },
        {
          seat: Seat;
          closed_tiles: number[];
          drawn_tile: number | null;
          melds: [];
        },
        {
          seat: Seat;
          closed_tiles: number[];
          drawn_tile: number | null;
          melds: [];
        },
        {
          seat: Seat;
          closed_tiles: number[];
          drawn_tile: number | null;
          melds: [];
        },
      ],
    };

    const summary = {
      mode: "EastOnly" as const,
      hands_played: 4,
      initial_scores: [0, 0, 0, 0] as [number, number, number, number],
      final_scores: [4, -4, 2, -2] as [number, number, number, number],
      net_deltas: [4, -4, 2, -2] as [number, number, number, number],
    };

    const res = reduceServerMessage(
      { ...start, pendingAddedKong: { seat: "North", tile: 3 } },
      {
        EventBatch: {
          from_seq: 21,
          to_seq: 22,
          events: [
            { seq: 21, kind: { HandSettled: { settlement } } },
            { seq: 22, kind: { MatchFinished: { summary } } },
          ],
          updated_legal_actions: [],
        },
      },
    );

    expect(res.state.pendingAddedKong).toBeNull();
    expect(res.state.snapshot?.phase).toBe("Finished");
    expect(res.state.snapshot?.player_view?.phase).toBe("MatchFinished");
    expect(res.state.snapshot?.player_view?.last_settlement).toEqual(
      settlement,
    );
    expect(res.state.snapshot?.player_view?.match_summary).toEqual(summary);
    expect(
      res.state.snapshot?.player_view?.seats.map((s) => s.score),
    ).toEqual([4, -4, 2, -2]);
  });

  it("detects seq gaps on EventBatch and emits RequestSnapshot without corrupting state", () => {
    const start = reduceServerMessage(createInitialSessionState(), {
      TableSnapshot: makeSnapshot({
        phase: "InMatch",
        your_seat: "East",
        seq: 5,
        player_view: makePlayerView({ seq: 5 }),
      }),
    }).state;

    const res = reduceServerMessage(start, {
      EventBatch: {
        from_seq: 8,
        to_seq: 9,
        events: [
          {
            seq: 8,
            kind: {
              TileDiscarded: { seat: "West", tile: 12, is_tsumogiri: true },
            },
          },
          {
            seq: 9,
            kind: {
              TileDrawn: {
                seat: "North",
                tile: null,
                is_replacement: false,
                wall_remaining: 50,
              },
            },
          },
        ],
        updated_legal_actions: ["Pass"],
      },
    });

    expect(res.effects).toEqual([{ type: "send", message: "RequestSnapshot" }]);
    expect(res.state.snapshot?.seq).toBe(5);
    expect(res.state.snapshot?.player_view?.seq).toBe(5);
    expect(res.state.snapshot?.player_view?.seats[2].river).toEqual([]);
  });

  it("unlocks pendingActionId and updates legal_actions on empty-event same-seq EventBatch", () => {
    const start: SessionState = {
      ...reduceServerMessage(createInitialSessionState(), {
        TableSnapshot: makeSnapshot({
          phase: "InMatch",
          your_seat: "South",
          seq: 10,
          player_view: makePlayerView({
            viewer: "South",
            seq: 10,
            legal_actions: [{ Pon: { tile: 9 } }, "Pass"],
          }),
        }),
      }).state,
      pendingActionId: "act-pass-1",
    };

    const res = reduceServerMessage(start, {
      EventBatch: {
        from_seq: 10,
        to_seq: 10,
        events: [],
        updated_legal_actions: [],
      },
    });

    expect(res.effects).toEqual([]);
    expect(res.state.pendingActionId).toBeNull();
    expect(res.state.snapshot?.seq).toBe(10);
    expect(res.state.snapshot?.player_view?.legal_actions).toEqual([]);
  });

  it("unlocks pendingActionId on ActionRejected and emits RequestSnapshot only when seq differs", () => {
    const start: SessionState = {
      ...reduceServerMessage(createInitialSessionState(), {
        TableSnapshot: makeSnapshot({
          phase: "InMatch",
          your_seat: "East",
          seq: 10,
          player_view: makePlayerView({ seq: 10 }),
        }),
      }).state,
      pendingActionId: "act-1",
    };

    const sameSeq = reduceServerMessage(start, {
      ActionRejected: { reason: "illegal action", current_seq: 10 },
    });
    expect(sameSeq.state.pendingActionId).toBeNull();
    expect(sameSeq.state.lastRejectedReason).toBe("illegal action");
    expect(sameSeq.effects).toEqual([]);

    const staleSeq = reduceServerMessage(start, {
      ActionRejected: { reason: "stale seq", current_seq: 12 },
    });
    expect(staleSeq.state.pendingActionId).toBeNull();
    expect(staleSeq.state.lastRejectedReason).toBe("stale seq");
    expect(staleSeq.effects).toEqual([
      { type: "send", message: "RequestSnapshot" },
    ]);
  });

  it("clears pendingActionId on ErrorNotice so unexpected server errors never deadlock input", () => {
    const start: SessionState = {
      ...reduceServerMessage(createInitialSessionState(), {
        TableSnapshot: makeSnapshot({
          phase: "InMatch",
          your_seat: "East",
          seq: 10,
          player_view: makePlayerView({ seq: 10 }),
        }),
      }).state,
      pendingActionId: "act-pending-1",
    };

    const res = reduceServerMessage(start, {
      ErrorNotice: { message: "can only submit actions during an active match" },
    });

    expect(res.state.pendingActionId).toBeNull();
    expect(res.state.lastErrorNotice).toBe(
      "can only submit actions during an active match",
    );
    expect(res.effects).toEqual([]);
  });

  it("clears pendingAddedKong on HandSettled and folds NextHandConfirmed and HandStarted", () => {
    const start = reduceServerMessage(createInitialSessionState(), {
      TableSnapshot: makeSnapshot({
        phase: "InMatch",
        your_seat: "East",
        seq: 14,
        player_view: makePlayerView({ seq: 14 }),
      }),
    }).state;

    const afterAttempt = reduceServerMessage(start, {
      EventBatch: {
        from_seq: 15,
        to_seq: 15,
        events: [
          { seq: 15, kind: { AddedKongAttempted: { seat: "East", tile: 27 } } },
        ],
        updated_legal_actions: [],
      },
    }).state;
    expect(afterAttempt.pendingAddedKong).toEqual({ seat: "East", tile: 27 });

    const settlement = {
      wind_round: "East" as const,
      hand_number: 1,
      dealer_streak: 0,
      dealer: "East" as const,
      outcome: "DrawGame" as const,
      kong_transfers: [],
      score_deltas: [-4, 4, 0, 0] as [number, number, number, number],
      new_scores: [-4, 4, 0, 0] as [number, number, number, number],
      revealed_hands: ALL_SEATS.map((seat) => ({
        seat,
        closed_tiles: [0, 1, 2],
        drawn_tile: null,
        melds: [],
      })) as [
        {
          seat: Seat;
          closed_tiles: number[];
          drawn_tile: number | null;
          melds: [];
        },
        {
          seat: Seat;
          closed_tiles: number[];
          drawn_tile: number | null;
          melds: [];
        },
        {
          seat: Seat;
          closed_tiles: number[];
          drawn_tile: number | null;
          melds: [];
        },
        {
          seat: Seat;
          closed_tiles: number[];
          drawn_tile: number | null;
          melds: [];
        },
      ],
    };

    const settled = reduceServerMessage(afterAttempt, {
      EventBatch: {
        from_seq: 16,
        to_seq: 16,
        events: [{ seq: 16, kind: { HandSettled: { settlement } } }],
        updated_legal_actions: ["ConfirmNextHand"],
      },
    }).state;
    expect(settled.pendingAddedKong).toBeNull();
    expect(settled.snapshot?.player_view?.phase).toBe("BetweenHands");
    expect(settled.snapshot?.player_view?.ready_seats).toEqual([
      false,
      false,
      false,
      false,
    ]);

    const confirmed = reduceServerMessage(settled, {
      EventBatch: {
        from_seq: 17,
        to_seq: 17,
        events: [{ seq: 17, kind: { NextHandConfirmed: { seat: "South" } } }],
        updated_legal_actions: ["ConfirmNextHand"],
      },
    }).state;
    expect(confirmed.snapshot?.player_view?.ready_seats).toEqual([
      false,
      true,
      false,
      false,
    ]);

    const nextHand = reduceServerMessage(confirmed, {
      EventBatch: {
        from_seq: 18,
        to_seq: 19,
        events: [
          { seq: 18, kind: { NextHandConfirmed: { seat: "East" } } },
          {
            seq: 19,
            kind: {
              HandStarted: {
                wind_round: "East",
                hand_number: 2,
                dealer_streak: 0,
                dealer: "South",
                hand: {
                  closed_tiles: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13],
                  drawn_tile: null,
                },
                wall_remaining: 70,
                scores: [-4, 4, 0, 0],
              },
            },
          },
        ],
        updated_legal_actions: [],
      },
    }).state;
    expect(nextHand.snapshot?.player_view?.phase).toBe("Playing");
    expect(nextHand.snapshot?.player_view?.turn.hand_number).toBe(2);
    expect(nextHand.snapshot?.player_view?.turn.dealer).toBe("South");
    expect(nextHand.snapshot?.player_view?.ready_seats).toBeNull();
    expect(nextHand.snapshot?.player_view?.last_settlement).toBeNull();
    expect(nextHand.snapshot?.player_view?.seats[1].hand_count).toBe(14);
    expect(nextHand.snapshot?.player_view?.seats[0].hand_count).toBe(13);
  });

  it("preserves player_view and pending_destructive_op on LobbyUpdated during InMatch (e.g. SetPace)", () => {
    const view = makePlayerView({ seq: 8 });
    const pendingOp = {
      op_id: "op-disband-1",
      kind: "DisbandMatch" as const,
      requested_by: "East" as const,
      required_seats: ["East" as const, "South" as const],
      confirmed_seats: ["East" as const],
    };
    const start: SessionState = {
      ...reduceServerMessage(createInitialSessionState(), {
        TableSnapshot: makeSnapshot({
          phase: "InMatch",
          your_seat: "East",
          seq: 8,
          player_view: view,
          pending_destructive_op: pendingOp,
        }),
      }).state,
      pendingAddedKong: { seat: "East", tile: 27 },
    };

    const res = reduceServerMessage(start, {
      LobbyUpdated: {
        phase: "InMatch",
        config: makeLobbyConfig({ pace: "Fast" }),
        seats: makeLobbySeats(),
        slow_think_nudge_enabled: false,
      },
    });

    expect(res.state.snapshot?.pace).toBe("Fast");
    expect(res.state.snapshot?.config.pace).toBe("Fast");
    expect(res.state.snapshot?.slow_think_nudge_enabled).toBe(false);
    expect(res.state.snapshot?.player_view).toEqual(view);
    expect(res.state.snapshot?.pending_destructive_op).toEqual(pendingOp);
    expect(res.state.pendingAddedKong).toEqual({ seat: "East", tile: 27 });
  });

  it("handles RobKongHu HandSettled after AddedKongAttempted without upgrading Pon meld", () => {
    const start = reduceServerMessage(createInitialSessionState(), {
      TableSnapshot: makeSnapshot({
        phase: "InMatch",
        your_seat: "South",
        seq: 11,
        player_view: makePlayerView({
          viewer: "South",
          seq: 11,
          seats: [
            {
              seat: "East",
              score: 0,
              hand_count: 11,
              river: [],
              melds: [{ Pon: { tile: 27, from_seat: "West", seq: 6 } }],
              last_draw_or_meld_seq: 10,
            },
            {
              seat: "South",
              score: 0,
              hand_count: 13,
              river: [],
              melds: [],
              last_draw_or_meld_seq: 9,
            },
            {
              seat: "West",
              score: 0,
              hand_count: 13,
              river: [],
              melds: [],
              last_draw_or_meld_seq: 7,
            },
            {
              seat: "North",
              score: 0,
              hand_count: 13,
              river: [],
              melds: [],
              last_draw_or_meld_seq: 8,
            },
          ],
        }),
      }),
    }).state;

    const pending = reduceServerMessage(start, {
      EventBatch: {
        from_seq: 12,
        to_seq: 12,
        events: [
          { seq: 12, kind: { AddedKongAttempted: { seat: "East", tile: 27 } } },
        ],
        updated_legal_actions: [{ RobKongHu: { tile: 27 } }, "Pass"],
      },
    }).state;

    expect(pending.pendingAddedKong).toEqual({ seat: "East", tile: 27 });
    expect(pending.snapshot?.player_view?.legal_actions).toEqual([
      { RobKongHu: { tile: 27 } },
      "Pass",
    ]);

    const settled = reduceServerMessage(pending, {
      EventBatch: {
        from_seq: 13,
        to_seq: 13,
        events: [
          {
            seq: 13,
            kind: {
              HandSettled: {
                settlement: {
                  wind_round: "East",
                  hand_number: 1,
                  dealer_streak: 0,
                  dealer: "East",
                  outcome: {
                    Win: {
                      winners: [
                        {
                          seat: "South",
                          win_kind: "RobKongHu",
                          items: [{ code: "rob_kong_hu", label: "抢杠胡", fan: 1 }],
                          total_fan: 1,
                          base_points: 2,
                        },
                      ],
                      discarder: "East",
                      winning_tile: 27,
                    },
                  },
                  kong_transfers: [],
                  score_deltas: [-6, 6, 0, 0],
                  new_scores: [-6, 6, 0, 0],
                  revealed_hands: ALL_SEATS.map((seat) => ({
                    seat,
                    closed_tiles: [0, 1, 2],
                    drawn_tile: null,
                    melds:
                      seat === "East"
                        ? [{ Pon: { tile: 27, from_seat: "West", seq: 6 } }]
                        : [],
                  })) as [
                    {
                      seat: Seat;
                      closed_tiles: number[];
                      drawn_tile: number | null;
                      melds: [{ Pon: { tile: number; from_seat: Seat; seq: number } }];
                    },
                    {
                      seat: Seat;
                      closed_tiles: number[];
                      drawn_tile: number | null;
                      melds: [];
                    },
                    {
                      seat: Seat;
                      closed_tiles: number[];
                      drawn_tile: number | null;
                      melds: [];
                    },
                    {
                      seat: Seat;
                      closed_tiles: number[];
                      drawn_tile: number | null;
                      melds: [];
                    },
                  ],
                },
              },
            },
          },
        ],
        updated_legal_actions: ["ConfirmNextHand"],
      },
    }).state;

    expect(settled.pendingAddedKong).toBeNull();
    expect(settled.snapshot?.player_view?.phase).toBe("BetweenHands");
    expect(settled.snapshot?.player_view?.seats[0].melds).toEqual([
      { Pon: { tile: 27, from_seat: "West", seq: 6 } },
    ]);
    expect(settled.snapshot?.player_view?.seats.map((s) => s.score)).toEqual([
      -6, 6, 0, 0,
    ]);
  });

  it("handles PendingDestructiveOp snapshot lifecycle and Finished -> ReturnToLobby / StartMatch transitions", () => {
    const pendingOp = {
      op_id: "op-end-1",
      kind: "EndFreeTable" as const,
      requested_by: "East" as const,
      required_seats: ["East" as const, "South" as const],
      confirmed_seats: ["East" as const],
    };

    // PendingDestructiveOp arrives via TableSnapshot
    const withPending = reduceServerMessage(createInitialSessionState(), {
      TableSnapshot: makeSnapshot({
        phase: "InMatch",
        your_seat: "South",
        seq: 25,
        player_view: makePlayerView({
          viewer: "South",
          mode: "FreeTable",
          phase: "BetweenHands",
          seq: 25,
        }),
        pending_destructive_op: pendingOp,
      }),
    }).state;
    expect(withPending.snapshot?.pending_destructive_op).toEqual(pendingOp);

    // Rejection clears pending_destructive_op via TableSnapshot
    const cleared = reduceServerMessage(withPending, {
      TableSnapshot: makeSnapshot({
        phase: "InMatch",
        your_seat: "South",
        seq: 25,
        player_view: makePlayerView({
          viewer: "South",
          mode: "FreeTable",
          phase: "BetweenHands",
          seq: 25,
        }),
        pending_destructive_op: null,
      }),
    }).state;
    expect(cleared.snapshot?.pending_destructive_op).toBeNull();

    // Finished state transitions cleanly to Lobby on ReturnToLobby snapshot
    const finished = reduceServerMessage(cleared, {
      TableSnapshot: makeSnapshot({
        phase: "Finished",
        your_seat: "South",
        seq: 26,
        player_view: makePlayerView({
          viewer: "South",
          phase: "MatchFinished",
          seq: 26,
          match_summary: {
            mode: "FreeTable",
            hands_played: 2,
            initial_scores: [0, 0, 0, 0],
            final_scores: [8, -8, 0, 0],
            net_deltas: [8, -8, 0, 0],
          },
        }),
      }),
    }).state;
    expect(finished.snapshot?.phase).toBe("Finished");

    const returnedToLobby = reduceServerMessage(finished, {
      TableSnapshot: makeSnapshot({
        phase: "Lobby",
        your_seat: "South",
        seq: 0,
        player_view: null,
      }),
    }).state;
    expect(returnedToLobby.snapshot?.phase).toBe("Lobby");
    expect(returnedToLobby.snapshot?.player_view).toBeNull();

    // Peer LobbyUpdated in Finished updates config/pace while preserving match_summary
    const updatedConfigInFinished = reduceServerMessage(finished, {
      LobbyUpdated: {
        phase: "Finished",
        config: makeLobbyConfig({
          mode: "EastOnly",
          seating: "FixedOpposite",
          ai_difficulty: "Intermediate",
          pace: "Fast",
        }),
        seats: makeLobbySeats(),
        slow_think_nudge_enabled: true,
      },
    }).state;
    expect(updatedConfigInFinished.snapshot?.phase).toBe("Finished");
    expect(updatedConfigInFinished.snapshot?.config).toEqual({
      mode: "EastOnly",
      seating: "FixedOpposite",
      ai_difficulty: "Intermediate",
      pace: "Fast",
    });
    expect(updatedConfigInFinished.snapshot?.pace).toBe("Fast");
    expect(
      updatedConfigInFinished.snapshot?.player_view?.match_summary,
    ).toEqual({
      mode: "FreeTable",
      hands_played: 2,
      initial_scores: [0, 0, 0, 0],
      final_scores: [8, -8, 0, 0],
      net_deltas: [8, -8, 0, 0],
    });

    // Finished -> StartMatch directly transitions back to InMatch
    const restarted = reduceServerMessage(finished, {
      TableSnapshot: makeSnapshot({
        phase: "InMatch",
        your_seat: "East",
        seq: 2,
        player_view: makePlayerView({ seq: 2 }),
      }),
    }).state;
    expect(restarted.snapshot?.phase).toBe("InMatch");
    expect(restarted.snapshot?.player_view?.phase).toBe("Playing");
    expect(restarted.snapshot?.player_view?.match_summary).toBeNull();
  });
});

describe("session reducer — TableBanter and LlmStatusNotice", () => {
  it("updates AI seat mood live on TableBanter and ignores non-AI or mismatched persona seats", () => {
    const seats = makeLobbySeats();
    seats[0] = {
      ...seats[0],
      kind: "Human",
      nickname: "房主",
      connected: true,
    };
    seats[1] = {
      ...seats[1],
      kind: "Ai",
      nickname: "大川",
      connected: true,
      ai_difficulty: "Strong",
      ai_style: "AggressiveFast",
      persona_id: "QuickHothead",
      mood: "Smug",
    };

    const inMatch = reduceServerMessage(createInitialSessionState(), {
      TableSnapshot: makeSnapshot({
        phase: "InMatch",
        your_seat: "East",
        seats,
        player_view: makePlayerView(),
        seq: 4,
      }),
    }).state;

    // Matching AI seat updates mood to Tilted
    const updated = reduceServerMessage(inMatch, {
      TableBanter: {
        banter_id: "b-1",
        seat: "South",
        persona_id: "QuickHothead",
        mood: "Tilted",
        text: "谁让你碰我牌的，急眼了！",
        event_seq: 5,
        reply_to_seat: null,
      },
    }).state;
    expect(updated.snapshot?.seats[1].mood).toBe("Tilted");

    // Human seat banter is ignored (no mood leaked onto human seat)
    const ignoredHuman = reduceServerMessage(updated, {
      TableBanter: {
        banter_id: "b-2",
        seat: "East",
        persona_id: "TeaSarcasm",
        mood: "Mocking",
        text: "不该写入人类座位",
        event_seq: 6,
        reply_to_seat: null,
      },
    }).state;
    expect(ignoredHuman.snapshot?.seats[0].mood).toBeNull();
  });

  it("updates AI seat mood silently on PersonaMoodUpdated and ignores non-AI or mismatched persona seats", () => {
    const seats = makeLobbySeats();
    seats[0] = {
      ...seats[0],
      kind: "Human",
      nickname: "房主",
      connected: true,
    };
    seats[1] = {
      ...seats[1],
      kind: "Ai",
      nickname: "大川",
      connected: true,
      ai_difficulty: "Strong",
      ai_style: "AggressiveFast",
      persona_id: "QuickHothead",
      mood: "Smug",
    };
    seats[3] = {
      ...seats[3],
      kind: "Ai",
      nickname: "无角色AI",
      connected: true,
      ai_difficulty: "Strong",
      ai_style: "Balanced",
      persona_id: null,
      mood: "Deadpan",
    };

    const inMatch = reduceServerMessage(createInitialSessionState(), {
      TableSnapshot: makeSnapshot({
        phase: "InMatch",
        your_seat: "East",
        seats,
        player_view: makePlayerView(),
        seq: 4,
      }),
    }).state;

    // 1. Matching AI seat updates mood to Plotting with zero effects
    const updated = reduceServerMessage(inMatch, {
      PersonaMoodUpdated: {
        seat: "South",
        persona_id: "QuickHothead",
        mood: "Plotting",
      },
    });
    expect(updated.effects).toEqual([]);
    expect(updated.state.snapshot?.seats[1].mood).toBe("Plotting");
    expect(updated.state.snapshot?.seats[0].mood).toBeNull();
    expect(updated.state.snapshot?.seats[2].mood).toBeNull();

    // 2. Identical mood is a reference-preserving no-op
    const noop = reduceServerMessage(updated.state, {
      PersonaMoodUpdated: {
        seat: "South",
        persona_id: "QuickHothead",
        mood: "Plotting",
      },
    });
    expect(noop.state).toBe(updated.state);
    expect(noop.effects).toEqual([]);

    // 3. Human seat is ignored
    const ignoredHuman = reduceServerMessage(updated.state, {
      PersonaMoodUpdated: {
        seat: "East",
        persona_id: "QuickHothead",
        mood: "Mocking",
      },
    });
    expect(ignoredHuman.state).toBe(updated.state);
    expect(ignoredHuman.state.snapshot?.seats[0].mood).toBeNull();

    // 4. Empty seat is ignored
    const ignoredEmpty = reduceServerMessage(updated.state, {
      PersonaMoodUpdated: {
        seat: "West",
        persona_id: "TeaSarcasm",
        mood: "Tilted",
      },
    });
    expect(ignoredEmpty.state).toBe(updated.state);
    expect(ignoredEmpty.state.snapshot?.seats[2].mood).toBeNull();

    // 5. Mismatched persona_id on AI seat is ignored
    const ignoredMismatch = reduceServerMessage(updated.state, {
      PersonaMoodUpdated: {
        seat: "South",
        persona_id: "TeaSarcasm",
        mood: "Tilted",
      },
    });
    expect(ignoredMismatch.state).toBe(updated.state);
    expect(ignoredMismatch.state.snapshot?.seats[1].mood).toBe("Plotting");

    // 6. AI seat without matching persona_id (null) is ignored
    const ignoredNullPersona = reduceServerMessage(updated.state, {
      PersonaMoodUpdated: {
        seat: "North",
        persona_id: "ChaosInstigator",
        mood: "Nervous",
      },
    });
    expect(ignoredNullPersona.state).toBe(updated.state);
    expect(ignoredNullPersona.state.snapshot?.seats[3].mood).toBe("Deadpan");
  });

  it("tracks LlmStatusNotice outage and recovery cleanly without clearing snapshot", () => {
    const base = reduceServerMessage(createInitialSessionState(), {
      TableSnapshot: makeSnapshot({ phase: "InMatch", your_seat: "East" }),
    }).state;

    const degraded = reduceServerMessage(base, {
      LlmStatusNotice: {
        available: false,
        message: "局域网推理服务异常：请求超时",
      },
    }).state;
    expect(degraded.llmStatusNotice).toEqual({
      available: false,
      message: "局域网推理服务异常：请求超时",
    });
    expect(degraded.snapshot?.phase).toBe("InMatch");

    const recovered = reduceServerMessage(degraded, {
      LlmStatusNotice: {
        available: true,
        message: "局域网推理服务已恢复连接",
      },
    }).state;
    expect(recovered.llmStatusNotice).toEqual({
      available: true,
      message: "局域网推理服务已恢复连接",
    });
  });

  it("tracks TtsStatusNotice outage and recovery without clearing snapshot, and passes through TableBanterAudioReady", () => {
    const base = reduceServerMessage(createInitialSessionState(), {
      TableSnapshot: makeSnapshot({ phase: "InMatch", your_seat: "East" }),
    }).state;
    expect(base.ttsStatusNotice).toBeNull();

    const degraded = reduceServerMessage(base, {
      TtsStatusNotice: {
        available: false,
        message: "局域网台词语音服务暂不可用，已切换为离线语音包与文字气泡",
      },
    });
    expect(degraded.state.ttsStatusNotice).toEqual({
      available: false,
      message: "局域网台词语音服务暂不可用，已切换为离线语音包与文字气泡",
    });
    expect(degraded.state.snapshot?.phase).toBe("InMatch");
    expect(degraded.effects).toEqual([]);

    const afterAudioReady = reduceServerMessage(degraded.state, {
      TableBanterAudioReady: {
        banter_id: "b-1",
        seat: "South",
        audio_url: "/api/banter-audio/b-1",
        duration_ms: 1450,
      },
    });
    expect(afterAudioReady.state).toEqual(degraded.state);
    expect(afterAudioReady.effects).toEqual([]);

    const afterAudioFallback = reduceServerMessage(degraded.state, {
      TableBanterAudioFallback: {
        banter_id: "b-2",
        seat: "West",
      },
    });
    expect(afterAudioFallback.state).toEqual(degraded.state);
    expect(afterAudioFallback.effects).toEqual([]);

    const recovered = reduceServerMessage(degraded.state, {
      TtsStatusNotice: {
        available: true,
        message: "局域网台词语音服务已恢复连接",
      },
    });
    expect(recovered.state.ttsStatusNotice).toEqual({
      available: true,
      message: "局域网台词语音服务已恢复连接",
    });
    expect(recovered.effects).toEqual([]);
  });

  it("handles QuickEmoteBroadcast cleanly and clears stale cooldown rejection when viewer sends successfully", () => {
    const base: SessionState = {
      ...reduceServerMessage(createInitialSessionState(), {
        TableSnapshot: makeSnapshot({
          phase: "InMatch",
          your_seat: "East",
          slow_think_nudge_enabled: false,
        }),
      }).state,
      lastRejectedReason: "快捷互动冷却中，请稍后再试",
    };

    // Peer quick emote leaves viewer's rejection notice intact.
    const peerRes = reduceServerMessage(base, {
      QuickEmoteBroadcast: {
        seat: "South",
        emote: "NicePlay",
        text: "666 / 漂亮",
      },
    });
    expect(peerRes.state.lastRejectedReason).toBe("快捷互动冷却中，请稍后再试");
    expect(peerRes.effects).toEqual([]);

    // Viewer's own successful quick emote clears stale rejection notice.
    const ownRes = reduceServerMessage(base, {
      QuickEmoteBroadcast: {
        seat: "East",
        emote: "IsThatIt",
        text: "就这？",
      },
    });
    expect(ownRes.state.lastRejectedReason).toBeNull();
    expect(ownRes.state.snapshot?.slow_think_nudge_enabled).toBe(false);
    expect(ownRes.effects).toEqual([]);
  });

  it("records remoteAccessDisabledMessage and emits stop_reconnect on RemoteAccessDisabled", () => {
    const start = reduceServerMessage(createInitialSessionState(), {
      TableSnapshot: makeSnapshot({
        host_is_loopback: false,
        client_is_remote: true,
        lan_urls: {
          preferred_ipv4_url: null,
          ipv4_urls: [],
          local_hostname_url: null,
        },
      }),
    }).state;

    const res = reduceServerMessage(
      { ...start, pendingActionId: "act-pending" },
      {
        RemoteAccessDisabled: {
          message: "宿主已关闭远程联机，连接已断开",
        },
      },
    );
    expect(res.state.remoteAccessDisabledMessage).toBe(
      "宿主已关闭远程联机，连接已断开",
    );
    expect(res.state.pendingActionId).toBeNull();
    expect(res.effects).toEqual([{ type: "stop_reconnect" }]);
  });
});


