import { describe, expect, it } from "vitest";
import type { PlayerEvent } from "../protocol/generated/PlayerEvent";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import { ALL_SEATS } from "../protocol/seats";
import {
  buildPiltoverImpactFrame,
  createPresentationEffectsBus,
  registerPiltoverImpactSubscriber,
  type EffectsBusMessage,
  type PiltoverImpactFrame,
} from "./effectsBus";
import {
  mapPlayerEventToSemanticEvent,
  type SemanticGameEvent,
  type SemanticMappingContext,
} from "./semanticEvents";

function makeSeats(): [SeatInfo, SeatInfo, SeatInfo, SeatInfo] {
  return ALL_SEATS.map((seat, idx) => ({
    seat,
    kind: idx === 0 ? "Human" : "Ai",
    nickname: `玩家 ${idx + 1}`,
    connected: true,
    ai_assisted: false,
    ai_difficulty: idx === 0 ? null : "Strong",
    ai_style: idx === 0 ? null : "Balanced",
    voice_pack_id: idx === 2 ? "arcane-hero-2" : null,
  })) as [SeatInfo, SeatInfo, SeatInfo, SeatInfo];
}

const ctx: SemanticMappingContext = {
  viewer: "East",
  seats: makeSeats(),
  wallRemaining: 30,
};

function toSemantic(event: PlayerEvent): SemanticGameEvent {
  return mapPlayerEventToSemanticEvent(event, ctx);
}

describe("PresentationEffectsBus", () => {
  it("supports multiple subscribers including v1 Piltover impact frame and v2 Cut-in / big-fan hook", () => {
    const bus = createPresentationEffectsBus();
    const piltoverFrames: PiltoverImpactFrame[] = [];
    let clearCount = 0;

    const unsubPiltover = registerPiltoverImpactSubscriber(bus, {
      onImpact: (frame) => piltoverFrames.push(frame),
      onClear: () => {
        clearCount += 1;
      },
    });

    const cutIns: Array<{ fan: number | null; voicePackId: string | null }> =
      [];
    const unsubV2 = bus.subscribe((msg: EffectsBusMessage) => {
      if (
        msg.type === "semantic_event" &&
        msg.event.type === "HandSettled" &&
        (msg.event.totalFan ?? 0) >= 4
      ) {
        cutIns.push({
          fan: msg.event.totalFan,
          voicePackId: msg.event.voicePackId,
        });
      }
    });

    // AddedKongAttempted must NOT trigger a big kong impact frame
    bus.emit(
      toSemantic({
        seq: 10,
        kind: { AddedKongAttempted: { seat: "West", tile: 27 } },
      }),
    );
    expect(piltoverFrames).toHaveLength(0);

    // MeldDeclared(AddedKong) triggers 补杠！ Piltover frame
    bus.emit(
      toSemantic({
        seq: 11,
        kind: {
          MeldDeclared: {
            seat: "West",
            meld: { AddedKong: { tile: 27, from_seat: "South", seq: 11 } },
            kong_score_deltas: [-1, -1, 3, -1],
          },
        },
      }),
      { durationMs: 700, skippable: false },
    );
    expect(piltoverFrames).toHaveLength(1);
    expect(piltoverFrames[0]).toMatchObject({
      kind: "kong",
      label: "补杠！",
      sublabel: "对门 · 玩家 3 · 东 · +3 杠分",
      speedlines: true,
      skewDeg: -7,
      dampedShake: true,
      warmFlash: true,
      seats: ["West"],
      durationMs: 700,
      skippable: false,
    });

    // Big-fan HandSettled triggers both Piltover frame and v2 Cut-in subscriber
    bus.emit(
      toSemantic({
        seq: 12,
        kind: {
          HandSettled: {
            settlement: {
              wind_round: "East",
              hand_number: 1,
              dealer_streak: 0,
              dealer: "East",
              outcome: {
                Win: {
                  discarder: null,
                  winning_tile: 31,
                  winners: [
                    {
                      seat: "West",
                      win_kind: "Tsumo",
                      total_fan: 4,
                      base_points: 16,
                      items: [{ code: "QingYiSe", label: "清一色", fan: 4 }],
                    },
                  ],
                },
              },
              kong_transfers: [],
              score_deltas: [-8, -8, 24, -8],
              new_scores: [-8, -8, 24, -8],
              revealed_hands: [
                { seat: "East", closed_tiles: [0], drawn_tile: null, melds: [] },
                { seat: "South", closed_tiles: [1], drawn_tile: null, melds: [] },
                { seat: "West", closed_tiles: [2], drawn_tile: null, melds: [] },
                { seat: "North", closed_tiles: [3], drawn_tile: null, melds: [] },
              ],
            },
          },
        },
      }),
      { durationMs: 1100, skippable: true },
    );
    expect(piltoverFrames).toHaveLength(2);
    expect(piltoverFrames[1]).toMatchObject({
      kind: "hu",
      label: "自摸！",
      sublabel: "对门 · 玩家 3 · 4 番 · 中",
      seats: ["West"],
      skippable: true,
    });
    expect(cutIns).toEqual([{ fan: 4, voicePackId: "arcane-hero-2" }]);

    bus.clear();
    expect(clearCount).toBe(1);

    unsubPiltover();
    unsubV2();
    bus.emit(
      toSemantic({
        seq: 13,
        kind: {
          MeldDeclared: {
            seat: "South",
            meld: { Pon: { tile: 0, from_seat: "East", seq: 13 } },
            kong_score_deltas: null,
          },
        },
      }),
    );
    expect(piltoverFrames).toHaveLength(2);
  });

  it("respects setReducedEffects(true) to suppress damped shake and warm flash while preserving stamp", () => {
    const bus = createPresentationEffectsBus();
    const frames: PiltoverImpactFrame[] = [];
    registerPiltoverImpactSubscriber(bus, {
      onImpact: (frame) => frames.push(frame),
    });

    bus.setReducedEffects(true);
    expect(bus.getReducedEffects()).toBe(true);

    bus.emit(
      toSemantic({
        seq: 20,
        kind: {
          MeldDeclared: {
            seat: "South",
            meld: { Pon: { tile: 12, from_seat: "East", seq: 20 } },
            kong_score_deltas: null,
          },
        },
      }),
    );

    expect(frames).toHaveLength(1);
    expect(frames[0]).toMatchObject({
      kind: "pon",
      label: "碰！",
      sublabel: "下家 · 玩家 2 · 四条",
      speedlines: true,
      dampedShake: false,
      warmFlash: false,
      seats: ["South"],
    });
  });

  it("formats 一炮多响 single combined stamp, 抢杠胡！, and 荒庄 frames", () => {
    const multiRon = toSemantic({
      seq: 30,
      kind: {
        HandSettled: {
          settlement: {
            wind_round: "East",
            hand_number: 1,
            dealer_streak: 0,
            dealer: "East",
            outcome: {
              Win: {
                discarder: "East",
                winning_tile: 9,
                winners: [
                  {
                    seat: "South",
                    win_kind: "Ron",
                    total_fan: 1,
                    base_points: 2,
                    items: [{ code: "DuanYao", label: "断幺", fan: 1 }],
                  },
                  {
                    seat: "North",
                    win_kind: "Ron",
                    total_fan: 2,
                    base_points: 4,
                    items: [{ code: "PengPeng", label: "碰碰胡", fan: 2 }],
                  },
                ],
              },
            },
            kong_transfers: [],
            score_deltas: [-6, 2, 0, 4],
            new_scores: [-6, 2, 0, 4],
            revealed_hands: [
              { seat: "East", closed_tiles: [0], drawn_tile: null, melds: [] },
              { seat: "South", closed_tiles: [1], drawn_tile: null, melds: [] },
              { seat: "West", closed_tiles: [2], drawn_tile: null, melds: [] },
              { seat: "North", closed_tiles: [3], drawn_tile: null, melds: [] },
            ],
          },
        },
      },
    });
    const multiFrame = buildPiltoverImpactFrame(multiRon, {
      id: 1,
      reducedEffects: false,
    });
    expect(multiFrame).toMatchObject({
      kind: "multi_ron",
      label: "一炮多响！",
      seats: ["South", "North"],
      speedlines: true,
      dampedShake: true,
      warmFlash: true,
    });

    const robKong = toSemantic({
      seq: 31,
      kind: {
        HandSettled: {
          settlement: {
            wind_round: "East",
            hand_number: 1,
            dealer_streak: 0,
            dealer: "East",
            outcome: {
              Win: {
                discarder: "West",
                winning_tile: 27,
                winners: [
                  {
                    seat: "East",
                    win_kind: "RobKongHu",
                    total_fan: 2,
                    base_points: 4,
                    items: [{ code: "RobKongHu", label: "抢杠胡", fan: 1 }],
                  },
                ],
              },
            },
            kong_transfers: [],
            score_deltas: [12, 0, -12, 0],
            new_scores: [12, 0, -12, 0],
            revealed_hands: [
              { seat: "East", closed_tiles: [0], drawn_tile: null, melds: [] },
              { seat: "South", closed_tiles: [1], drawn_tile: null, melds: [] },
              { seat: "West", closed_tiles: [2], drawn_tile: null, melds: [] },
              { seat: "North", closed_tiles: [3], drawn_tile: null, melds: [] },
            ],
          },
        },
      },
    });
    const robFrame = buildPiltoverImpactFrame(robKong, {
      id: 2,
      reducedEffects: false,
    });
    expect(robFrame).toMatchObject({
      kind: "rob_kong",
      label: "抢杠胡！",
      sublabel: "自家 · 玩家 1 · 2 番 · 东",
      seats: ["East"],
    });

    const viewerlessWin = mapPlayerEventToSemanticEvent(
      {
        seq: 33,
        kind: {
          HandSettled: {
            settlement: {
              wind_round: "East",
              hand_number: 1,
              dealer_streak: 0,
              dealer: "East",
              outcome: {
                Win: {
                  discarder: "West",
                  winning_tile: 27,
                  winners: [
                    {
                      seat: "East",
                      win_kind: "Ron",
                      total_fan: 2,
                      base_points: 4,
                      items: [{ code: "DuiDuiHu", label: "对对胡", fan: 2 }],
                    },
                  ],
                },
              },
              kong_transfers: [],
              score_deltas: [12, 0, -12, 0],
              new_scores: [12, 0, -12, 0],
              revealed_hands: [
                { seat: "East", closed_tiles: [0], drawn_tile: null, melds: [] },
                { seat: "South", closed_tiles: [1], drawn_tile: null, melds: [] },
                { seat: "West", closed_tiles: [2], drawn_tile: null, melds: [] },
                { seat: "North", closed_tiles: [3], drawn_tile: null, melds: [] },
              ],
            },
          },
        },
      },
      { viewer: null, seats: null, wallRemaining: 30 },
    );
    const viewerlessFrame = buildPiltoverImpactFrame(viewerlessWin, {
      id: 4,
      reducedEffects: false,
    });
    expect(viewerlessFrame?.sublabel).toBe("1号位 · 2 番 · 东");
    expect(viewerlessFrame?.sublabel).not.toMatch(/东座|南座|西座|北座/);

    const drawGame = toSemantic({
      seq: 32,
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
    });
    const drawFrame = buildPiltoverImpactFrame(drawGame, {
      id: 3,
      reducedEffects: false,
    });
    expect(drawFrame).toMatchObject({
      kind: "draw_game",
      label: "荒庄",
      speedlines: false,
      dampedShake: false,
      warmFlash: false,
      seats: [],
    });
  });
});
