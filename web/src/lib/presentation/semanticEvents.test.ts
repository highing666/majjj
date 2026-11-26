import { describe, expect, it } from "vitest";
import type { PlayerEvent } from "../protocol/generated/PlayerEvent";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import { ALL_SEATS } from "../protocol/seats";
import {
  createUiActionPromptedEvent,
  mapLocalUiActionToSemanticEvent,
  mapPlayerEventToSemanticEvent,
  type SemanticMappingContext,
} from "./semanticEvents";

function makeSeats(): [SeatInfo, SeatInfo, SeatInfo, SeatInfo] {
  return ALL_SEATS.map((seat, idx) => ({
    seat,
    kind: idx === 0 ? "Human" : "Ai",
    nickname: idx === 0 ? "房主" : `AI ${idx}`,
    connected: true,
    ai_assisted: false,
    ai_difficulty: idx === 0 ? null : "Strong",
    ai_style: idx === 0 ? null : "Balanced",
    voice_pack_id: idx === 1 ? "pack-south" : idx === 2 ? "pack-west" : null,
  })) as [SeatInfo, SeatInfo, SeatInfo, SeatInfo];
}

function makeContext(
  viewer: Seat | null = "East",
  wallRemaining = 24,
): SemanticMappingContext {
  return {
    viewer,
    seats: makeSeats(),
    wallRemaining,
  };
}

describe("semanticEvents", () => {
  it("maps TileDiscarded with relativeRole, isTsumogiri, wallRemaining, and voicePackId", () => {
    const event: PlayerEvent = {
      seq: 12,
      kind: {
        TileDiscarded: {
          seat: "South",
          tile: 15,
          is_tsumogiri: true,
        },
      },
    };

    const mapped = mapPlayerEventToSemanticEvent(event, makeContext("East", 24));
    expect(mapped).toMatchObject({
      type: "TileDiscarded",
      seq: 12,
      seat: "South",
      relativeRole: "下家",
      actorLabel: "下家 · AI 1",
      voicePackId: "pack-south",
      wallRemaining: 24,
      isTsumogiri: true,
      tile: 15,
      totalFan: null,
      winKind: null,
      winners: [],
      meldKind: null,
    });
  });

  it("maps AddedKongAttempted and MeldDeclared with meldKind and kongScoreDeltas", () => {
    const attempt: PlayerEvent = {
      seq: 20,
      kind: {
        AddedKongAttempted: {
          seat: "West",
          tile: 27,
        },
      },
    };
    const mappedAttempt = mapPlayerEventToSemanticEvent(
      attempt,
      makeContext("East", 31),
    );
    expect(mappedAttempt).toMatchObject({
      type: "AddedKongAttempted",
      seat: "West",
      relativeRole: "对门",
      actorLabel: "对门 · AI 2",
      voicePackId: "pack-west",
      wallRemaining: 31,
      meldKind: "AddedKong",
      tile: 27,
    });

    const declared: PlayerEvent = {
      seq: 21,
      kind: {
        MeldDeclared: {
          seat: "North",
          meld: { Pon: { tile: 8, from_seat: "West", seq: 21 } },
          kong_score_deltas: null,
        },
      },
    };
    const mappedMeld = mapPlayerEventToSemanticEvent(
      declared,
      makeContext("East", 31),
    );
    expect(mappedMeld).toMatchObject({
      type: "MeldDeclared",
      seat: "North",
      relativeRole: "上家",
      actorLabel: "上家 · AI 3",
      voicePackId: null,
      meldKind: "Pon",
      tile: 8,
    });
  });

  it("maps HandSettled for single win, multi-ron 一炮多响, and DrawGame", () => {
    const multiRon: PlayerEvent = {
      seq: 55,
      kind: {
        HandSettled: {
          settlement: {
            wind_round: "East",
            hand_number: 1,
            dealer_streak: 0,
            dealer: "East",
            outcome: {
              Win: {
                discarder: "North",
                winning_tile: 18,
                winners: [
                  {
                    seat: "South",
                    win_kind: "Ron",
                    total_fan: 2,
                    base_points: 4,
                    items: [{ code: "DuiDuiHu", label: "对对胡", fan: 2 }],
                  },
                  {
                    seat: "West",
                    win_kind: "Ron",
                    total_fan: 4,
                    base_points: 16,
                    items: [{ code: "QingYiSe", label: "清一色", fan: 4 }],
                  },
                ],
              },
            },
            kong_transfers: [],
            score_deltas: [0, 4, 16, -20],
            new_scores: [0, 4, 16, -20],
            revealed_hands: [
              { seat: "East", closed_tiles: [0], drawn_tile: null, melds: [] },
              { seat: "South", closed_tiles: [1], drawn_tile: null, melds: [] },
              { seat: "West", closed_tiles: [2], drawn_tile: null, melds: [] },
              { seat: "North", closed_tiles: [3], drawn_tile: null, melds: [] },
            ],
          },
        },
      },
    };

    const mappedWin = mapPlayerEventToSemanticEvent(
      multiRon,
      makeContext("East", 12),
    );
    expect(mappedWin).toMatchObject({
      type: "HandSettled",
      seat: "South",
      relativeRole: "下家",
      actorLabel: "下家 · AI 1",
      voicePackId: "pack-south",
      wallRemaining: 12,
      totalFan: 4,
      winKind: "Ron",
      isMultiRon: true,
      isDrawGame: false,
      discarder: "North",
      tile: 18,
    });
    expect(mappedWin.winners).toEqual([
      {
        seat: "South",
        relativeRole: "下家",
        displayName: "下家 · AI 1",
        winKind: "Ron",
        totalFan: 2,
        voicePackId: "pack-south",
      },
      {
        seat: "West",
        relativeRole: "对门",
        displayName: "对门 · AI 2",
        winKind: "Ron",
        totalFan: 4,
        voicePackId: "pack-west",
      },
    ]);

    const viewerlessWin = mapPlayerEventToSemanticEvent(
      multiRon,
      makeContext(null, 12),
    );
    expect(viewerlessWin.actorLabel).toBe("AI 1");
    expect(viewerlessWin.winners.map((w) => w.displayName)).toEqual([
      "AI 1",
      "AI 2",
    ]);

    const drawGame: PlayerEvent = {
      seq: 56,
      kind: {
        HandSettled: {
          settlement: {
            wind_round: "East",
            hand_number: 1,
            dealer_streak: 0,
            dealer: "East",
            outcome: "DrawGame",
            kong_transfers: [],
            score_deltas: [0, 0, 0, 0],
            new_scores: [0, 0, 0, 0],
            revealed_hands: [
              { seat: "East", closed_tiles: [0], drawn_tile: null, melds: [] },
              { seat: "South", closed_tiles: [1], drawn_tile: null, melds: [] },
              { seat: "West", closed_tiles: [2], drawn_tile: null, melds: [] },
              { seat: "North", closed_tiles: [3], drawn_tile: null, melds: [] },
            ],
          },
        },
      },
    };
    const mappedDraw = mapPlayerEventToSemanticEvent(
      drawGame,
      makeContext("East", 0),
    );
    expect(mappedDraw).toMatchObject({
      type: "HandSettled",
      seat: null,
      relativeRole: null,
      actorLabel: null,
      wallRemaining: 0,
      totalFan: null,
      winKind: null,
      winners: [],
      isDrawGame: true,
      isMultiRon: false,
    });
  });

  it("maps local UI actions with viewer context", () => {
    const hover = mapLocalUiActionToSemanticEvent(
      { type: "UiTileHover", tile: 4, fromDrawn: false },
      makeContext("East", 42),
    );
    expect(hover).toMatchObject({
      type: "UiTileHover",
      seat: "East",
      relativeRole: "自家",
      tile: 4,
      fromDrawn: false,
      wallRemaining: 42,
    });

    const select = mapLocalUiActionToSemanticEvent(
      { type: "UiTileSelect", tile: 7, fromDrawn: true },
      makeContext("East", 42),
    );
    expect(select).toMatchObject({
      type: "UiTileSelect",
      seat: "East",
      relativeRole: "自家",
      tile: 7,
      fromDrawn: true,
    });

    const pass = mapLocalUiActionToSemanticEvent(
      { type: "UiPassClicked" },
      makeContext("East", 42),
    );
    expect(pass.type).toBe("UiPassClicked");

    const confirm = mapLocalUiActionToSemanticEvent(
      { type: "UiConfirmNextHand" },
      makeContext("East", 0),
    );
    expect(confirm.type).toBe("UiConfirmNextHand");
  });

  it("emits UiActionPrompted strictly only when local viewer gains newly actionable legal prompt actions", () => {
    const ctx = makeContext("East", 40);

    // Discard-only turn does not trigger UiActionPrompted
    expect(
      createUiActionPromptedEvent(
        [],
        [{ Discard: { tile: 0, from_drawn: false } }],
        "Playing",
        ctx,
      ),
    ).toBeNull();

    // BetweenHands ConfirmNextHand does not trigger UiActionPrompted
    expect(
      createUiActionPromptedEvent([], ["ConfirmNextHand"], "BetweenHands", ctx),
    ).toBeNull();

    // Pass alone does not trigger UiActionPrompted
    expect(
      createUiActionPromptedEvent([], ["Pass"], "Playing", ctx),
    ).toBeNull();

    // Spectator (viewer === null) never triggers UiActionPrompted
    expect(
      createUiActionPromptedEvent(
        [],
        [{ Pon: { tile: 5 } }, "Pass"],
        "Playing",
        makeContext(null, 40),
      ),
    ).toBeNull();

    // Gaining Pon triggers UiActionPrompted
    const firstPrompt = createUiActionPromptedEvent(
      [{ Discard: { tile: 0, from_drawn: false } }],
      [{ Pon: { tile: 5 } }, "Pass"],
      "Playing",
      ctx,
    );
    expect(firstPrompt).toMatchObject({
      type: "UiActionPrompted",
      seat: "East",
      relativeRole: "自家",
      actions: [{ Pon: { tile: 5 } }],
    });

    // Re-receiving the exact same prompt actions does NOT re-emit
    expect(
      createUiActionPromptedEvent(
        [{ Pon: { tile: 5 } }, "Pass"],
        [{ Pon: { tile: 5 } }, "Pass"],
        "Playing",
        ctx,
      ),
    ).toBeNull();

    // Gaining an additional prompt action (e.g., Ron added) emits again
    const upgradedPrompt = createUiActionPromptedEvent(
      [{ Pon: { tile: 5 } }, "Pass"],
      [{ Pon: { tile: 5 } }, { Ron: { tile: 5 } }, "Pass"],
      "Playing",
      ctx,
    );
    expect(upgradedPrompt).toMatchObject({
      type: "UiActionPrompted",
      actions: [{ Pon: { tile: 5 } }, { Ron: { tile: 5 } }],
    });
  });
});
