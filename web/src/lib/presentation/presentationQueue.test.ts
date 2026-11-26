import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { HandSettlement } from "../protocol/generated/HandSettlement";
import type { LobbyConfig } from "../protocol/generated/LobbyConfig";
import type { PlayerView } from "../protocol/generated/PlayerView";
import type { PublicSeatView } from "../protocol/generated/PublicSeatView";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { ServerMessage } from "../protocol/generated/ServerMessage";
import type { TablePace } from "../protocol/generated/TablePace";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import { ALL_SEATS } from "../protocol/seats";
import { createPresentationEffectsBus } from "./effectsBus";
import {
  createPresentationQueue,
  type PresentationState,
} from "./presentationQueue";
import type { SemanticGameEvent } from "./semanticEvents";

function makeLobbyConfig(pace: TablePace = "Standard"): LobbyConfig {
  return {
    mode: "EastOnly",
    seating: "Random",
    ai_difficulty: "Strong",
    pace,
  };
}

function makeSeats(): [SeatInfo, SeatInfo, SeatInfo, SeatInfo] {
  return ALL_SEATS.map((seat, idx) => ({
    seat,
    kind: idx === 0 ? "Human" : "Ai",
    nickname: idx === 0 ? "房主" : `AI ${idx}`,
    connected: true,
    ai_assisted: false,
    ai_difficulty: idx === 0 ? null : "Strong",
    ai_style: idx === 0 ? null : "Balanced",
    persona_id: null,
    mood: null,
    voice_pack_id: idx === 1 ? "vo-south" : null,
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
      fan_cap: 4,
      wall_size: 136,
      dead_wall_size: 14,
      starting_scores_east_only: [0, 0, 0, 0],
      starting_scores_free_table: [0, 0, 0, 0],
    },
    phase: "Playing",
    hand: {
      closed_tiles: [0, 1, 2, 9, 10, 11, 18, 19, 20, 27, 27, 31, 31],
      drawn_tile: null,
    },
    seats: ALL_SEATS.map((seat) => ({
      seat,
      score: 0,
      hand_count: 13,
      river: [],
      melds: [],
      last_draw_or_meld_seq: 0,
    })) as unknown as [
      PublicSeatView,
      PublicSeatView,
      PublicSeatView,
      PublicSeatView,
    ],
    wall_remaining: 69,
    turn: {
      wind_round: "East",
      hand_number: 1,
      dealer_streak: 0,
      dealer: "East",
      active_seat: "East",
    },
    legal_actions: [],
    last_settlement: null,
    ready_seats: null,
    match_summary: null,
    seq: 1,
    ...overrides,
  };
}

function makeSnapshot(
  seq = 1,
  pace: TablePace = "Standard",
  viewOverrides: Partial<PlayerView> = {},
): TableSnapshot {
  return {
    table_id: "table-1",
    server_version: "0.1.0",
    seq,
    phase: "InMatch",
    config: makeLobbyConfig(pace),
    pace,
    your_seat: "East",
    seat_token: "tok-east",
    seats: makeSeats(),
    player_view: makePlayerView({ seq, ...viewOverrides }),
    pending_destructive_op: null,
    lan_urls: {
      preferred_ipv4_url: null,
      ipv4_urls: [],
      local_hostname_url: null,
    },
    host_is_loopback: true,
  };
}

function makeSettlement(
  outcome: HandSettlement["outcome"],
  scoreDeltas: [number, number, number, number] = [0, 0, 0, 0],
): HandSettlement {
  return {
    wind_round: "East",
    hand_number: 1,
    dealer_streak: 0,
    dealer: "East",
    outcome,
    kong_transfers: [],
    score_deltas: scoreDeltas,
    new_scores: scoreDeltas,
    revealed_hands: ALL_SEATS.map((seat) => ({
      seat,
      closed_tiles: [0, 1, 2],
      drawn_tile: null,
      melds: [],
    })) as unknown as HandSettlement["revealed_hands"],
  };
}

describe("PresentationQueue", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("plays EventBatch items serially at Standard (1.0x) and Fast (0.6x) pace", () => {
    const queue = createPresentationQueue();
    const states: PresentationState[] = [];
    queue.subscribe((s) => states.push(s));

    queue.enqueueServerMessage({ TableSnapshot: makeSnapshot(1, "Standard") });
    expect(queue.getState().snapshot?.seq).toBe(1);
    expect(queue.getState().isPlaying).toBe(false);

    // Feed draw (140ms) + discard (180ms) in one batch
    queue.enqueueServerMessage({
      EventBatch: {
        from_seq: 2,
        to_seq: 3,
        events: [
          {
            seq: 2,
            kind: {
              TileDrawn: {
                seat: "South",
                tile: null,
                is_replacement: false,
                wall_remaining: 68,
              },
            },
          },
          {
            seq: 3,
            kind: {
              TileDiscarded: { seat: "South", tile: 5, is_tsumogiri: false },
            },
          },
        ],
        updated_legal_actions: [{ Pon: { tile: 5 } }, "Pass"],
      },
    });

    // At t = 0: first item (TileDrawn) is presented, second is still queued
    expect(queue.getState().snapshot?.seq).toBe(2);
    expect(queue.getState().snapshot?.player_view?.wall_remaining).toBe(68);
    expect(queue.getState().snapshot?.player_view?.legal_actions).toEqual([]);
    expect(queue.getState().isPlaying).toBe(true);
    expect(queue.getState().queueLength).toBe(1);

    // At t = 139ms: still on TileDrawn
    vi.advanceTimersByTime(139);
    expect(queue.getState().snapshot?.seq).toBe(2);

    // At t = 140ms: advances to TileDiscarded (180ms) and applies final legal_actions
    vi.advanceTimersByTime(1);
    expect(queue.getState().snapshot?.seq).toBe(3);
    expect(queue.getState().snapshot?.player_view?.seats[1].river).toHaveLength(
      1,
    );
    expect(queue.getState().snapshot?.player_view?.legal_actions).toEqual([
      { Pon: { tile: 5 } },
      "Pass",
    ]);
    expect(queue.getState().isPlaying).toBe(true);

    // At t = 140 + 180ms: queue finishes
    vi.advanceTimersByTime(180);
    expect(queue.getState().isPlaying).toBe(false);
    expect(queue.getState().queueLength).toBe(0);

    // Switch pace to Fast (0.6x) via LobbyUpdated
    queue.enqueueServerMessage({
      LobbyUpdated: {
        phase: "InMatch",
        config: makeLobbyConfig("Fast"),
        seats: makeSeats(),
      },
    });
    expect(queue.getState().pace).toBe("Fast");

    queue.enqueueServerMessage({
      EventBatch: {
        from_seq: 4,
        to_seq: 4,
        events: [
          {
            seq: 4,
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
    // 140 * 0.6 = 84ms
    vi.advanceTimersByTime(83);
    expect(queue.getState().isPlaying).toBe(true);
    vi.advanceTimersByTime(1);
    expect(queue.getState().isPlaying).toBe(false);

    queue.dispose();
  });

  it("accelerates backlog >= 3 regular items by 1.6x and hard-cuts on TableSnapshot", () => {
    const queue = createPresentationQueue();
    queue.enqueueServerMessage({ TableSnapshot: makeSnapshot(1, "Standard") });

    // Enqueue 4 regular items (backlog >= 3 -> first two run at /1.6: 88ms, 113ms)
    queue.enqueueServerMessage({
      EventBatch: {
        from_seq: 2,
        to_seq: 5,
        events: [
          {
            seq: 2,
            kind: {
              TileDrawn: {
                seat: "South",
                tile: null,
                is_replacement: false,
                wall_remaining: 68,
              },
            },
          },
          {
            seq: 3,
            kind: {
              TileDiscarded: { seat: "South", tile: 1, is_tsumogiri: true },
            },
          },
          {
            seq: 4,
            kind: {
              TileDrawn: {
                seat: "West",
                tile: null,
                is_replacement: false,
                wall_remaining: 67,
              },
            },
          },
          {
            seq: 5,
            kind: {
              TileDiscarded: { seat: "West", tile: 2, is_tsumogiri: false },
            },
          },
        ],
        updated_legal_actions: [],
      },
    });

    // Item 1 (TileDrawn): round(140 / 1.6) = 88ms
    expect(queue.getState().snapshot?.seq).toBe(2);
    vi.advanceTimersByTime(87);
    expect(queue.getState().snapshot?.seq).toBe(2);
    vi.advanceTimersByTime(1);
    expect(queue.getState().snapshot?.seq).toBe(3);

    // Item 2 (TileDiscarded, still 3 items were waiting before pop): round(180 / 1.6) = 113ms
    vi.advanceTimersByTime(113);
    expect(queue.getState().snapshot?.seq).toBe(4);

    // TableSnapshot arrives mid-backlog -> immediately clears queue, cancels active timer, hard-cuts
    queue.enqueueServerMessage({
      TableSnapshot: makeSnapshot(20, "Standard", { wall_remaining: 50 }),
    });
    expect(queue.getState().snapshot?.seq).toBe(20);
    expect(queue.getState().snapshot?.player_view?.wall_remaining).toBe(50);
    expect(queue.getState().queueLength).toBe(0);
    expect(queue.getState().isPlaying).toBe(false);
    expect(queue.getState().activeImpact).toBeNull();

    vi.advanceTimersByTime(2000);
    expect(queue.getState().snapshot?.seq).toBe(20);
    queue.dispose();
  });

  it("handles two-stage AddedKongAttempted -> RobKongHu vs AddedKong without premature impact or score jump", () => {
    const effectsBus = createPresentationEffectsBus();
    const queue = createPresentationQueue({ effectsBus });
    const semanticEvents: SemanticGameEvent[] = [];
    queue.onSemanticEvent((ev) => semanticEvents.push(ev));

    const ponView = makePlayerView({
      seq: 10,
      seats: ALL_SEATS.map((seat) => ({
        seat,
        score: 0,
        hand_count: seat === "West" ? 11 : 13,
        river: [],
        melds:
          seat === "West"
            ? [{ Pon: { tile: 27, from_seat: "South", seq: 6 } }]
            : [],
        last_draw_or_meld_seq: 0,
      })) as unknown as PlayerView["seats"],
    });
    queue.enqueueServerMessage({
      TableSnapshot: {
        ...makeSnapshot(10, "Standard"),
        player_view: ponView,
      },
    });

    // Stage 1: AddedKongAttempted -> pendingAddedKong hover + metallic suspense semantic event, NO big kong impact
    queue.enqueueServerMessage({
      EventBatch: {
        from_seq: 11,
        to_seq: 11,
        events: [
          { seq: 11, kind: { AddedKongAttempted: { seat: "West", tile: 27 } } },
        ],
        updated_legal_actions: [{ RobKongHu: { tile: 27 } }, "Pass"],
      },
    });

    expect(queue.getState().pendingAddedKong).toEqual({
      seat: "West",
      tile: 27,
    });
    expect(queue.getState().activeImpact).toBeNull();
    expect(
      queue.getState().snapshot?.player_view?.seats.map((s) => s.score),
    ).toEqual([0, 0, 0, 0]);
    expect(
      semanticEvents.some(
        (ev) => ev.type === "AddedKongAttempted" && ev.meldKind === "AddedKong",
      ),
    ).toBe(true);

    // Branch A: RobKongHu settles immediately with "抢杠胡！"
    queue.enqueueServerMessage({
      EventBatch: {
        from_seq: 12,
        to_seq: 12,
        events: [
          {
            seq: 12,
            kind: {
              HandSettled: {
                settlement: makeSettlement(
                  {
                    Win: {
                      discarder: "West",
                      winning_tile: 27,
                      winners: [
                        {
                          seat: "East",
                          win_kind: "RobKongHu",
                          total_fan: 2,
                          base_points: 4,
                          items: [
                            { code: "RobKongHu", label: "抢杠胡", fan: 1 },
                          ],
                        },
                      ],
                    },
                  },
                  [12, 0, -12, 0],
                ),
              },
            },
          },
        ],
        updated_legal_actions: ["ConfirmNextHand"],
      },
    });

    // Advance past AddedKongAttempted 180ms beat
    vi.advanceTimersByTime(180);
    expect(queue.getState().pendingAddedKong).toBeNull();
    expect(queue.getState().activeImpact?.label).toBe("抢杠胡！");
    expect(queue.getState().settlementVisible).toBe(false);

    // Now test Branch B on a fresh snapshot: AddedKongAttempted -> MeldDeclared(AddedKong) -> "补杠！" impact (700ms)
    queue.enqueueServerMessage({
      TableSnapshot: {
        ...makeSnapshot(20, "Standard"),
        player_view: { ...ponView, seq: 20 },
      },
    });
    queue.enqueueServerMessage({
      EventBatch: {
        from_seq: 21,
        to_seq: 21,
        events: [
          { seq: 21, kind: { AddedKongAttempted: { seat: "West", tile: 27 } } },
        ],
        updated_legal_actions: [],
      },
    });
    vi.advanceTimersByTime(180);
    queue.enqueueServerMessage({
      EventBatch: {
        from_seq: 22,
        to_seq: 22,
        events: [
          {
            seq: 22,
            kind: {
              MeldDeclared: {
                seat: "West",
                meld: { AddedKong: { tile: 27, from_seat: "South", seq: 22 } },
                kong_score_deltas: [-1, -1, 3, -1],
              },
            },
          },
        ],
        updated_legal_actions: [],
      },
    });
    expect(queue.getState().pendingAddedKong).toBeNull();
    expect(queue.getState().activeImpact).toMatchObject({
      kind: "kong",
      label: "补杠！",
      seats: ["West"],
    });
    vi.advanceTimersByTime(700);
    expect(queue.getState().activeImpact).toBeNull();

    queue.dispose();
  });

  it("combines 一炮多响 into a single impact, gates settlement modal until win/荒庄 impact finishes, and supports skipActiveImpact()", () => {
    const queue = createPresentationQueue();
    queue.enqueueServerMessage({ TableSnapshot: makeSnapshot(1, "Standard") });

    // Multi-ron HandSettled -> 1100ms impact, settlementVisible = false until impact completes
    const multiRonBatch: ServerMessage = {
      EventBatch: {
        from_seq: 2,
        to_seq: 2,
        events: [
          {
            seq: 2,
            kind: {
              HandSettled: {
                settlement: makeSettlement(
                  {
                    Win: {
                      discarder: "East",
                      winning_tile: 5,
                      winners: [
                        {
                          seat: "South",
                          win_kind: "Ron",
                          total_fan: 2,
                          base_points: 4,
                          items: [{ code: "DuiDui", label: "对对胡", fan: 2 }],
                        },
                        {
                          seat: "North",
                          win_kind: "Ron",
                          total_fan: 4,
                          base_points: 16,
                          items: [{ code: "QingYiSe", label: "清一色", fan: 4 }],
                        },
                      ],
                    },
                  },
                  [-20, 4, 0, 16],
                ),
              },
            },
          },
        ],
        updated_legal_actions: ["ConfirmNextHand"],
      },
    };
    queue.enqueueServerMessage(multiRonBatch);

    expect(queue.getState().activeImpact).toMatchObject({
      kind: "multi_ron",
      label: "一炮多响！",
      seats: ["South", "North"],
      skippable: true,
    });
    expect(queue.getState().settlementVisible).toBe(false);

    // Wait 1099ms -> still playing impact, modal still gated
    vi.advanceTimersByTime(1099);
    expect(queue.getState().settlementVisible).toBe(false);
    expect(queue.getState().activeImpact).not.toBeNull();

    // At 1100ms -> impact clears, settlementVisible = true
    vi.advanceTimersByTime(1);
    expect(queue.getState().activeImpact).toBeNull();
    expect(queue.getState().settlementVisible).toBe(true);

    // Now test skipActiveImpact() on another win
    queue.enqueueServerMessage({ TableSnapshot: makeSnapshot(10, "Standard") });
    expect(queue.getState().settlementVisible).toBe(false);
    queue.enqueueServerMessage({
      EventBatch: {
        from_seq: 11,
        to_seq: 11,
        events: [
          {
            seq: 11,
            kind: {
              HandSettled: {
                settlement: makeSettlement("DrawGame"),
              },
            },
          },
        ],
        updated_legal_actions: ["ConfirmNextHand"],
      },
    });

    expect(queue.getState().activeImpact).toMatchObject({
      kind: "draw_game",
      label: "荒庄",
      skippable: true,
    });
    expect(queue.getState().settlementVisible).toBe(false);

    // Tap/click skip immediately clears impact and shows settlement
    expect(queue.skipActiveImpact()).toBe(true);
    expect(queue.getState().activeImpact).toBeNull();
    expect(queue.getState().settlementVisible).toBe(true);
    expect(queue.getState().isPlaying).toBe(false);

    // NextHandConfirmed during BetweenHands keeps settlementVisible = true
    queue.enqueueServerMessage({
      EventBatch: {
        from_seq: 12,
        to_seq: 12,
        events: [
          {
            seq: 12,
            kind: { NextHandConfirmed: { seat: "South" } },
          },
        ],
        updated_legal_actions: ["ConfirmNextHand"],
      },
    });
    expect(queue.getState().settlementVisible).toBe(true);
    expect(queue.getState().snapshot?.player_view?.ready_seats).toEqual([
      false,
      true,
      false,
      false,
    ]);

    queue.dispose();
  });

  it("plays final-hand HandSettled impact while keeping presentedSnapshot.phase at InMatch before transitioning to Finished on MatchFinished or skipActiveImpact()", () => {
    const queue = createPresentationQueue();
    const states: PresentationState[] = [];
    queue.subscribe((s) => states.push(s));
    queue.enqueueServerMessage({ TableSnapshot: makeSnapshot(1, "Standard") });

    const summary = {
      mode: "EastOnly" as const,
      hands_played: 4,
      initial_scores: [0, 0, 0, 0] as [number, number, number, number],
      final_scores: [24, -8, -8, -8] as [number, number, number, number],
      net_deltas: [24, -8, -8, -8] as [number, number, number, number],
    };

    // EastOnly final hand: HandSettled + MatchFinished arrive in the same EventBatch
    queue.enqueueServerMessage({
      EventBatch: {
        from_seq: 2,
        to_seq: 3,
        events: [
          {
            seq: 2,
            kind: {
              HandSettled: {
                settlement: makeSettlement(
                  {
                    Win: {
                      discarder: null,
                      winning_tile: 31,
                      winners: [
                        {
                          seat: "East",
                          win_kind: "Tsumo",
                          total_fan: 4,
                          base_points: 16,
                          items: [{ code: "QingYiSe", label: "清一色", fan: 4 }],
                        },
                      ],
                    },
                  },
                  [24, -8, -8, -8],
                ),
              },
            },
          },
          {
            seq: 3,
            kind: {
              MatchFinished: { summary },
            },
          },
        ],
        updated_legal_actions: [],
      },
    });

    // Impact must play first while presented snapshot remains InMatch (TableCanvas + ComicImpactOverlay stay mounted)
    expect(queue.getState().snapshot?.phase).toBe("InMatch");
    expect(queue.getState().snapshot?.player_view?.phase).toBe("BetweenHands");
    expect(queue.getState().activeImpact).toMatchObject({
      kind: "hu",
      label: "自摸！",
      skippable: true,
    });
    expect(queue.getState().settlementVisible).toBe(false);
    expect(queue.getState().isPlaying).toBe(true);

    vi.advanceTimersByTime(1099);
    expect(queue.getState().snapshot?.phase).toBe("InMatch");
    expect(queue.getState().activeImpact).not.toBeNull();

    // Once impact finishes, MatchFinished applies and transitions to Finished
    vi.advanceTimersByTime(1);
    expect(queue.getState().activeImpact).toBeNull();
    expect(queue.getState().snapshot?.phase).toBe("Finished");
    expect(queue.getState().snapshot?.player_view?.phase).toBe("MatchFinished");
    expect(queue.getState().snapshot?.player_view?.match_summary).toEqual(summary);
    expect(queue.getState().settlementVisible).toBe(false);
    expect(queue.getState().isPlaying).toBe(false);

    // Also verify skipActiveImpact() on DrawGame + MatchFinished transitions cleanly without flashing settlementVisible
    queue.enqueueServerMessage({ TableSnapshot: makeSnapshot(10, "Standard") });
    states.length = 0;
    queue.enqueueServerMessage({
      EventBatch: {
        from_seq: 11,
        to_seq: 12,
        events: [
          {
            seq: 11,
            kind: { HandSettled: { settlement: makeSettlement("DrawGame") } },
          },
          {
            seq: 12,
            kind: { MatchFinished: { summary } },
          },
        ],
        updated_legal_actions: [],
      },
    });

    expect(queue.getState().snapshot?.phase).toBe("InMatch");
    expect(queue.getState().activeImpact).toMatchObject({
      kind: "draw_game",
      label: "荒庄",
      skippable: true,
    });

    expect(queue.skipActiveImpact()).toBe(true);
    expect(queue.getState().activeImpact).toBeNull();
    expect(queue.getState().snapshot?.phase).toBe("Finished");
    expect(queue.getState().snapshot?.player_view?.phase).toBe("MatchFinished");
    expect(queue.getState().settlementVisible).toBe(false);
    expect(queue.getState().isPlaying).toBe(false);
    expect(states.some((s) => s.settlementVisible)).toBe(false);

    queue.dispose();
  });

  it("emits UiActionPrompted strictly when viewer gains newly actionable legal prompt actions", () => {
    const queue = createPresentationQueue();
    const semanticEvents: SemanticGameEvent[] = [];
    queue.onSemanticEvent((ev) => semanticEvents.push(ev));

    queue.enqueueServerMessage({
      TableSnapshot: makeSnapshot(1, "Standard", {
        legal_actions: [{ Discard: { tile: 0, from_drawn: false } }],
      }),
    });
    expect(
      semanticEvents.filter((ev) => ev.type === "UiActionPrompted"),
    ).toHaveLength(0);

    // Opponent discards a tile that viewer can Pon
    queue.enqueueServerMessage({
      EventBatch: {
        from_seq: 2,
        to_seq: 2,
        events: [
          {
            seq: 2,
            kind: {
              TileDiscarded: { seat: "South", tile: 5, is_tsumogiri: false },
            },
          },
        ],
        updated_legal_actions: [{ Pon: { tile: 5 } }, "Pass"],
      },
    });
    expect(
      semanticEvents.filter((ev) => ev.type === "UiActionPrompted"),
    ).toHaveLength(1);

    // Empty batch with identical legal_actions must NOT re-emit UiActionPrompted
    vi.advanceTimersByTime(180);
    queue.enqueueServerMessage({
      EventBatch: {
        from_seq: 2,
        to_seq: 2,
        events: [],
        updated_legal_actions: [{ Pon: { tile: 5 } }, "Pass"],
      },
    });
    expect(
      semanticEvents.filter((ev) => ev.type === "UiActionPrompted"),
    ).toHaveLength(1);

    queue.dispose();
  });
});
