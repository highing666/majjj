import { describe, expect, it } from "vitest";
import type { HandRecord } from "../protocol/generated/HandRecord";
import type { MatchRecord } from "../protocol/generated/MatchRecord";
import type { PersonaMemoryResponse } from "../protocol/generated/PersonaMemoryResponse";
import type { PlayerStats } from "../protocol/generated/PlayerStats";
import type { RevealedHand } from "../protocol/generated/RevealedHand";
import type { Seat } from "../protocol/generated/Seat";
import {
  buildHandHistoryRows,
  buildMatchHistoryRows,
  buildPersonaMemoryCards,
  buildPlayerSummaryCards,
  formatAverageScore,
  personaMemoryTriggerLabel,
} from "./historyViewModel";

function revealedHand(seat: Seat): RevealedHand {
  return {
    seat,
    closed_tiles: [0, 1, 2],
    drawn_tile: null,
    melds: [],
  };
}

function dummyRevealedHands(): [
  RevealedHand,
  RevealedHand,
  RevealedHand,
  RevealedHand,
] {
  return [
    revealedHand("East"),
    revealedHand("South"),
    revealedHand("West"),
    revealedHand("North"),
  ];
}

describe("historyViewModel", () => {
  it("formats normalized-nickname player summary cards with cumulative and average scores", () => {
    const players: PlayerStats[] = [
      {
        nickname: "阿东",
        completed_matches: 3,
        settled_hands: 11,
        aborted_hands: 2,
        wins: 4,
        tsumo_wins: 2,
        deal_ins: 1,
        ai_assisted_hands: 3,
        cumulative_net_score: 26,
        average_net_score: 8.6666,
        average_hand_net_score: 2.3636,
      },
      {
        nickname: "阿南",
        completed_matches: 0,
        settled_hands: 2,
        aborted_hands: 2,
        wins: 0,
        tsumo_wins: 0,
        deal_ins: 1,
        ai_assisted_hands: 0,
        cumulative_net_score: -10,
        average_net_score: 0,
        average_hand_net_score: -5,
      },
    ];

    const cards = buildPlayerSummaryCards(players);
    expect(cards).toHaveLength(2);

    expect(cards[0]).toMatchObject({
      nickname: "阿东",
      completedMatches: 3,
      settledHands: 11,
      abortedHands: 2,
      wins: 4,
      tsumoWins: 2,
      dealIns: 1,
      aiAssistedHands: 3,
      cumulativeNetScoreText: "+26",
      cumulativeNetTone: "pos",
      averageNetScoreText: "+8.7",
      averageHandNetScoreText: "+2.4",
    });

    expect(cards[1]).toMatchObject({
      nickname: "阿南",
      completedMatches: 0,
      settledHands: 2,
      abortedHands: 2,
      cumulativeNetScoreText: "-10",
      cumulativeNetTone: "neg",
      averageNetScoreText: "0.0",
      averageHandNetScoreText: "-5.0",
    });

    expect(formatAverageScore(0)).toBe("0.0");
    expect(formatAverageScore(-3.24)).toBe("-3.2");
  });

  it("maps recent match records with mode labels, seat scores, and AI assist tags", () => {
    const matches: MatchRecord[] = [
      {
        schema_version: 1,
        match_id: "m-1",
        timestamp_ms: Date.UTC(2026, 10, 26, 3, 30, 0),
        mode: "EastOnly",
        hands_played: 4,
        initial_scores: [100, 100, 100, 100],
        final_scores: [124, 92, 84, 100],
        net_deltas: [24, -8, -16, 0],
        seats: [
          {
            seat: "East",
            nickname: "阿东",
            seat_kind: "Human",
            assisted_by_ai: false,
            final_score: 124,
            net_delta: 24,
          },
          {
            seat: "South",
            nickname: "阿南",
            seat_kind: "Human",
            assisted_by_ai: true,
            final_score: 92,
            net_delta: -8,
          },
          {
            seat: "West",
            nickname: "AI 3",
            seat_kind: "Ai",
            assisted_by_ai: false,
            final_score: 84,
            net_delta: -16,
          },
          {
            seat: "North",
            nickname: "AI 4",
            seat_kind: "Ai",
            assisted_by_ai: false,
            final_score: 100,
            net_delta: 0,
          },
        ],
      },
    ];

    const rows = buildMatchHistoryRows(matches);
    expect(rows).toHaveLength(1);
    expect(rows[0].modeLabel).toBe("东风战");
    expect(rows[0].handsPlayed).toBe(4);
    expect(rows[0].assistedByAi).toBe(true);
    expect(rows[0].seats[0]).toEqual({
      seat: "East",
      nickname: "阿东",
      isAi: false,
      assistedByAi: false,
      finalScore: 124,
      netDelta: 24,
      netDeltaText: "+24",
      netTone: "pos",
    });
    expect(rows[0].seats[1]).toMatchObject({
      seat: "South",
      nickname: "阿南",
      assistedByAi: true,
      netDeltaText: "-8",
      netTone: "neg",
    });
    expect(rows[0].seats[2]).toMatchObject({
      isAi: true,
      nickname: "AI 3",
      netDeltaText: "-16",
    });

    const fallbackRows = buildMatchHistoryRows([
      {
        ...matches[0],
        match_id: "m-fallback",
        seats: [
          { seat: "East", nickname: "  ", seat_kind: "Human", assisted_by_ai: false, final_score: 100, net_delta: 0 },
          { seat: "South", nickname: "", seat_kind: "Ai", assisted_by_ai: false, final_score: 100, net_delta: 0 },
          { seat: "West", nickname: "", seat_kind: "Human", assisted_by_ai: false, final_score: 100, net_delta: 0 },
          { seat: "North", nickname: "AI北", seat_kind: "Ai", assisted_by_ai: false, final_score: 100, net_delta: 0 },
        ],
      },
    ]);
    expect(fallbackRows[0].seats.map((s) => s.nickname)).toEqual([
      "1号位",
      "AI 2",
      "3号位",
      "AI北",
    ]);
    expect(JSON.stringify(fallbackRows)).not.toMatch(/东座|南座|西座|北座/);
  });

  it("maps hand records with fan breakdown, kong transfers, AI assist, takeover, and aborted flags", () => {
    const hands: HandRecord[] = [
      {
        schema_version: 1,
        match_id: "m-aborted",
        hand_index: 1,
        timestamp_ms: Date.UTC(2026, 10, 26, 4, 0, 0),
        mode: "EastOnly",
        aborted: true,
        seats: [
          {
            seat: "East",
            nickname: "阿东",
            seat_kind: "Human",
            assisted_by_ai: true,
            took_over_ai: false,
          },
          {
            seat: "South",
            nickname: "阿南",
            seat_kind: "Human",
            assisted_by_ai: false,
            took_over_ai: true,
          },
          {
            seat: "West",
            nickname: "AI 3",
            seat_kind: "Ai",
            assisted_by_ai: false,
            took_over_ai: false,
          },
          {
            seat: "North",
            nickname: "AI 4",
            seat_kind: "Ai",
            assisted_by_ai: false,
            took_over_ai: false,
          },
        ],
        settlement: {
          wind_round: "East",
          hand_number: 2,
          dealer_streak: 1,
          dealer: "South",
          outcome: {
            Win: {
              winners: [
                {
                  seat: "East",
                  win_kind: "RobKongHu",
                  items: [
                    { code: "qingyise", label: "清一色", fan: 2 },
                    { code: "qiangganghu", label: "抢杠胡", fan: 1 },
                  ],
                  total_fan: 3,
                  base_points: 8,
                },
              ],
              discarder: "South",
              winning_tile: 0,
            },
          },
          kong_transfers: [
            {
              kind: "ConcealedKong",
              actor: "East",
              from_seat: null,
              tile: 31,
              score_deltas: [6, -2, -2, -2],
            },
          ],
          score_deltas: [30, -26, -2, -2],
          new_scores: [130, 74, 98, 98],
          revealed_hands: dummyRevealedHands(),
        },
      },
      {
        schema_version: 1,
        match_id: "m-draw",
        hand_index: 2,
        timestamp_ms: Date.UTC(2026, 10, 26, 4, 5, 0),
        mode: "FreeTable",
        aborted: false,
        seats: [
          {
            seat: "East",
            nickname: "阿东",
            seat_kind: "Human",
            assisted_by_ai: false,
            took_over_ai: false,
          },
          {
            seat: "South",
            nickname: "AI 2",
            seat_kind: "Ai",
            assisted_by_ai: false,
            took_over_ai: false,
          },
          {
            seat: "West",
            nickname: "AI 3",
            seat_kind: "Ai",
            assisted_by_ai: false,
            took_over_ai: false,
          },
          {
            seat: "North",
            nickname: "AI 4",
            seat_kind: "Ai",
            assisted_by_ai: false,
            took_over_ai: false,
          },
        ],
        settlement: {
          wind_round: "East",
          hand_number: 1,
          dealer_streak: 0,
          dealer: "East",
          outcome: "DrawGame",
          kong_transfers: [],
          score_deltas: [0, 0, 0, 0],
          new_scores: [100, 100, 100, 100],
          revealed_hands: dummyRevealedHands(),
        },
      },
    ];

    const rows = buildHandHistoryRows(hands);
    expect(rows).toHaveLength(2);

    const winRow = rows[0];
    expect(winRow.modeLabel).toBe("东风战");
    expect(winRow.roundTitle).toBe("东风圈 第 2 小局 · 连庄 1");
    expect(winRow.aborted).toBe(true);
    expect(winRow.assistedByAi).toBe(true);
    expect(winRow.outcomeLabel).toBe("抢杠胡");
    expect(winRow.winningTileLabel).toBe("一万");
    expect(winRow.discarderLabel).toBe("阿南");
    expect(winRow.winners).toEqual([
      {
        seat: "East",
        displayName: "阿东",
        winKindLabel: "抢杠胡",
        totalFan: 3,
        fanItems: [
          { code: "qingyise", label: "清一色", fan: 2 },
          { code: "qiangganghu", label: "抢杠胡", fan: 1 },
        ],
      },
    ]);
    expect(winRow.kongTransfers).toEqual([
      {
        kindLabel: "暗杠",
        actorLabel: "阿东",
        fromLabel: null,
        tileLabel: "中",
        actorDeltaText: "+6",
      },
    ]);
    expect(winRow.seatDeltas[0]).toEqual({
      seat: "East",
      nickname: "阿东",
      isAi: false,
      assistedByAi: true,
      tookOverAi: false,
      delta: 30,
      deltaText: "+30",
      tone: "pos",
    });
    expect(winRow.seatDeltas[1]).toMatchObject({
      nickname: "阿南",
      tookOverAi: true,
      deltaText: "-26",
      tone: "neg",
    });

    const fallbackHandRows = buildHandHistoryRows([
      {
        ...hands[0],
        match_id: "m-hand-fallback",
        seats: [
          { seat: "East", nickname: "", seat_kind: "Human", assisted_by_ai: false, took_over_ai: false },
          { seat: "South", nickname: " ", seat_kind: "Human", assisted_by_ai: false, took_over_ai: false },
          { seat: "West", nickname: "", seat_kind: "Ai", assisted_by_ai: false, took_over_ai: false },
          { seat: "North", nickname: "AI 4", seat_kind: "Ai", assisted_by_ai: false, took_over_ai: false },
        ],
        settlement: {
          ...hands[0].settlement,
          kong_transfers: [
            {
              kind: "ExposedKong",
              actor: "West",
              from_seat: "East",
              tile: 31,
              score_deltas: [-3, 0, 3, 0],
            },
          ],
        },
      },
    ]);
    expect(fallbackHandRows[0].discarderLabel).toBe("2号位");
    expect(fallbackHandRows[0].winners[0].displayName).toBe("1号位");
    expect(fallbackHandRows[0].kongTransfers[0]).toMatchObject({
      actorLabel: "AI 3",
      fromLabel: "1号位",
    });
    expect(fallbackHandRows[0].seatDeltas.map((s) => s.nickname)).toEqual([
      "1号位",
      "2号位",
      "AI 3",
      "AI 4",
    ]);
    expect(JSON.stringify(rows)).not.toMatch(/东座|南座|西座|北座/);
    expect(JSON.stringify(fallbackHandRows)).not.toMatch(/东座|南座|西座|北座/);

    const drawRow = rows[1];
    expect(drawRow.modeLabel).toBe("自由桌");
    expect(drawRow.aborted).toBe(false);
    expect(drawRow.assistedByAi).toBe(false);
    expect(drawRow.outcomeLabel).toBe("荒庄");
    expect(drawRow.winningTileLabel).toBeNull();
    expect(drawRow.winners).toEqual([]);
  });

  it("maps persona memory cards with ledger metrics, meld habits, and newest-first grudge notes", () => {
    const response: PersonaMemoryResponse = {
      schema_version: 1,
      total_pairs: 1,
      total_notes: 2,
      personas: [
        {
          persona_id: "TeaSarcasm",
          players: [
            {
              player_nickname: "小王",
              ledger: {
                hands_played: 5,
                persona_wins_against_player: 2,
                player_wins_against_persona: 3,
                persona_dealt_into_player: 1,
                player_dealt_into_persona: 2,
                highest_fan_seen: 6,
                net_score_delta: -18,
                player_pon_count: 4,
                player_kong_count: 2,
              },
              grudge_notes: [
                {
                  summary: "第一条较早恩怨",
                  created_at_ms: Date.UTC(2026, 10, 26, 3, 0, 0),
                  trigger: "HighFan",
                },
                {
                  summary: "最新一条抢杠胡记仇",
                  created_at_ms: Date.UTC(2026, 10, 26, 4, 30, 0),
                  trigger: "RobKongHu",
                },
              ],
              updated_at_ms: Date.UTC(2026, 10, 26, 4, 30, 0),
            },
          ],
        },
        {
          persona_id: "QuickHothead",
          players: [],
        },
      ],
    };

    const cards = buildPersonaMemoryCards(response);
    expect(cards).toHaveLength(2);
    expect(cards[0]).toMatchObject({
      personaId: "TeaSarcasm",
      personaName: "小晚",
      styleLabel: "防守做大牌",
      totalNotes: 2,
    });
    expect(cards[0].players[0]).toMatchObject({
      playerNickname: "小王",
      handsPlayed: 5,
      personaWins: 2,
      playerWins: 3,
      personaDealtIntoPlayer: 1,
      playerDealtIntoPersona: 2,
      highestFanSeen: 6,
      netScoreDelta: -18,
      netScoreDeltaText: "-18",
      netTone: "neg",
      meldHabitText: "碰 4 / 杠 2",
    });
    expect(cards[0].players[0].grudgeNotes.map((n) => n.summary)).toEqual([
      "最新一条抢杠胡记仇",
      "第一条较早恩怨",
    ]);
    expect(cards[0].players[0].grudgeNotes[0].triggerLabel).toBe("抢杠胡");
    expect(cards[0].players[0].grudgeNotes[1].triggerLabel).toBe("大番名场面");
    expect(personaMemoryTriggerLabel("MultiRon")).toBe("一炮多响");
    expect(personaMemoryTriggerLabel("MatchFinished")).toBe("终局复盘");
    expect(cards[1].players).toEqual([]);
  });
});
