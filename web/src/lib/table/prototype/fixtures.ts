// PROTOTYPE (throwaway for #78): Rich realistic Majjj table snapshots, settlements,
// and banter bubbles for instant previewing across all turn states without playing a full game.

import type { ActiveBanterMap } from "../../presentation/presentationQueue";
import type { DiscardEntry } from "../../protocol/generated/DiscardEntry";
import type { HandSettlement } from "../../protocol/generated/HandSettlement";
import type { PlayerAction } from "../../protocol/generated/PlayerAction";
import type { Seat } from "../../protocol/generated/Seat";
import type { TableSnapshot } from "../../protocol/generated/TableSnapshot";
import type { Tile } from "../../protocol/generated/Tile";
import {
  makeLobbyConfig,
  makePlayerView,
  makePublicSeatView,
  makeSeatInfo,
  makeTableSnapshot,
} from "../testFixtures";
import type { PrototypeSceneId } from "./variants";

function makeDiscards(tiles: Tile[], startSeq: number): DiscardEntry[] {
  return tiles.map((tile, idx) => ({
    tile,
    seq: startSeq + idx,
    is_tsumogiri: idx % 4 === 2,
    claimed_by: null,
  }));
}

function makeAllDiscardActions(closed: Tile[], drawn: Tile | null): PlayerAction[] {
  const seen = new Set<Tile>();
  const actions: PlayerAction[] = [];
  for (const tile of closed) {
    if (!seen.has(tile)) {
      seen.add(tile);
      actions.push({ Discard: { tile, from_drawn: false } });
    }
  }
  if (drawn !== null) {
    actions.push({ Discard: { tile: drawn, from_drawn: true } });
  }
  return actions;
}

const DEMO_SEATS: TableSnapshot["seats"] = [
  makeSeatInfo("East", "阿东", {
    kind: "Human",
    connected: true,
  }),
  makeSeatInfo("South", "急眼快攻哥", {
    kind: "Ai",
    persona_id: "QuickHothead",
    mood: "Smug",
    ai_style: "AggressiveFast",
  }),
  makeSeatInfo("West", "阴阳茶艺师", {
    kind: "Ai",
    persona_id: "TeaSarcasm",
    mood: "Mocking",
    ai_style: "DefensiveBigHand",
  }),
  makeSeatInfo("North", "拱火乐子人", {
    kind: "Ai",
    persona_id: "ChaosInstigator",
    mood: "Plotting",
    ai_style: "Balanced",
  }),
];

// Showcase a rich cross-section of 萬, 条 (including 1条 幺鸡!), 筒 (including 1筒 大花轮!), and 字牌 (中/發/白板!)
const RICH_SELF_CLOSED: Tile[] = [0, 4, 8, 9, 13, 16, 17, 18, 22, 26, 27, 31, 33];
const RICH_SELF_DRAWN: Tile = 32; // 發

const POST_PON_CLOSED: Tile[] = [0, 4, 8, 9, 13, 17, 18, 22, 31, 33];

const DEMO_RIVERS = {
  // Bottom (自家): 12 tiles covering every suit + honors + 白板
  East: makeDiscards([0, 3, 7, 9, 12, 15, 18, 21, 25, 28, 30, 33], 10),
  // Right (下家): 10 tiles (fits cleanly in 2×5 side river on iPhone landscape)
  South: makeDiscards([1, 5, 10, 14, 17, 19, 23, 27, 29, 31], 30),
  // Top (对门): 12 tiles
  West: makeDiscards([2, 6, 8, 11, 13, 16, 20, 24, 26, 28, 32, 33], 50),
  // Left (上家): 10 tiles
  North: makeDiscards([4, 9, 11, 15, 18, 21, 24, 29, 30, 31], 70),
};

const DEMO_SETTLEMENT: HandSettlement = {
  wind_round: "East",
  hand_number: 2,
  dealer_streak: 1,
  dealer: "East",
  outcome: {
    Win: {
      winning_tile: 33, // 白板
      discarder: null,
      winners: [
        {
          seat: "East",
          win_kind: "Tsumo",
          total_fan: 4,
          base_points: 16,
          items: [
            { code: "MENQING_TSUMO", label: "门前清自摸", fan: 1 },
            { code: "HUN_YISE", label: "混一色", fan: 2 },
            { code: "SAN_YUAN_KE", label: "箭刻（中）", fan: 1 },
          ],
        },
      ],
    },
  },
  kong_transfers: [],
  score_deltas: [48, -16, -16, -16],
  new_scores: [64, -12, -24, -28],
  revealed_hands: [
    {
      seat: "East",
      closed_tiles: [0, 1, 2, 4, 5, 6, 7, 8, 31, 31, 31, 33, 33],
      drawn_tile: 33,
      melds: [],
    },
    {
      seat: "South",
      closed_tiles: [9, 10, 11, 13, 14, 15, 18, 19, 20, 27],
      drawn_tile: null,
      melds: [{ Pon: { tile: 17, from_seat: "West", seq: 42 } }],
    },
    {
      seat: "West",
      closed_tiles: [18, 19, 20, 22, 23, 24, 25, 26, 28, 28],
      drawn_tile: null,
      melds: [{ ExposedKong: { tile: 32, from_seat: "North", seq: 46 } }],
    },
    {
      seat: "North",
      closed_tiles: [0, 4, 8, 9, 12, 16, 18, 21, 25, 29, 30, 31, 32],
      drawn_tile: null,
      melds: [],
    },
  ],
};

export interface PrototypePreviewBundle {
  readonly snapshot: TableSnapshot;
  readonly activeBubbles: ActiveBanterMap;
  readonly showSettlementModal: boolean;
}

export function buildPrototypePreview(
  scene: Exclude<PrototypeSceneId, "live">,
): PrototypePreviewBundle {
  const baseConfig = makeLobbyConfig({
    mode: "EastOnly",
    pace: "Standard",
  });

  const makeBubbles = (
    topText?: string,
    leftText?: string,
    rightText?: string,
  ): ActiveBanterMap => ({
    East: null,
    South: rightText
      ? {
          bubbleId: 101,
          banterId: "proto-right",
          priority: "normal",
          seat: "South",
          kind: "banter",
          text: rightText,
          personaId: "QuickHothead",
          mood: "Smug",
          emote: null,
          eventSeq: 90,
          replyToSeat: null,
        }
      : null,
    West: topText
      ? {
          bubbleId: 102,
          banterId: "proto-top",
          priority: "normal",
          seat: "West",
          kind: "banter",
          text: topText,
          personaId: "TeaSarcasm",
          mood: "Mocking",
          emote: null,
          eventSeq: 91,
          replyToSeat: null,
        }
      : null,
    North: leftText
      ? {
          bubbleId: 103,
          banterId: "proto-left",
          priority: "normal",
          seat: "North",
          kind: "banter",
          text: leftText,
          personaId: "ChaosInstigator",
          mood: "Plotting",
          emote: null,
          eventSeq: 92,
          replyToSeat: null,
        }
      : null,
  });

  if (scene === "other_turn") {
    const activeSeat: Seat = "South";
    return {
      snapshot: makeTableSnapshot({
        config: baseConfig,
        seats: DEMO_SEATS,
        player_view: makePlayerView({
          viewer: "East",
          phase: "Playing",
          hand: {
            closed_tiles: RICH_SELF_CLOSED,
            drawn_tile: null,
          },
          seats: [
            makePublicSeatView("East", {
              score: 16,
              hand_count: 13,
              river: DEMO_RIVERS.East,
            }),
            makePublicSeatView("South", {
              score: 4,
              hand_count: 14,
              river: DEMO_RIVERS.South,
            }),
            makePublicSeatView("West", {
              score: -8,
              hand_count: 10,
              river: DEMO_RIVERS.West,
              melds: [{ Pon: { tile: 27, from_seat: "North", seq: 40 } }],
            }),
            makePublicSeatView("North", {
              score: -12,
              hand_count: 13,
              river: DEMO_RIVERS.North,
            }),
          ],
          wall_remaining: 42,
          turn: {
            wind_round: "East",
            hand_number: 2,
            dealer_streak: 1,
            dealer: "East",
            active_seat: activeSeat,
          },
          legal_actions: [],
        }),
      }),
      activeBubbles: makeBubbles(undefined, undefined, "这张牌我可盯半天了，看牌！"),
      showSettlementModal: false,
    };
  }

  if (scene === "post_pon") {
    return {
      snapshot: makeTableSnapshot({
        config: baseConfig,
        seats: DEMO_SEATS,
        player_view: makePlayerView({
          viewer: "East",
          phase: "Playing",
          hand: {
            closed_tiles: POST_PON_CLOSED,
            drawn_tile: null,
          },
          seats: [
            makePublicSeatView("East", {
              score: 16,
              hand_count: POST_PON_CLOSED.length,
              river: DEMO_RIVERS.East,
              melds: [
                { Pon: { tile: 32, from_seat: "South", seq: 81 } },
                { Pon: { tile: 18, from_seat: "West", seq: 84 } },
              ],
            }),
            makePublicSeatView("South", {
              score: 4,
              hand_count: 13,
              river: DEMO_RIVERS.South,
            }),
            makePublicSeatView("West", {
              score: -8,
              hand_count: 10,
              river: DEMO_RIVERS.West,
              melds: [{ Pon: { tile: 27, from_seat: "North", seq: 40 } }],
            }),
            makePublicSeatView("North", {
              score: -12,
              hand_count: 13,
              river: DEMO_RIVERS.North,
            }),
          ],
          wall_remaining: 38,
          turn: {
            wind_round: "East",
            hand_number: 2,
            dealer_streak: 1,
            dealer: "East",
            active_seat: "East",
          },
          legal_actions: makeAllDiscardActions(POST_PON_CLOSED, null),
        }),
      }),
      activeBubbles: makeBubbles("哎呀又碰了，这把火候不小嘛！"),
      showSettlementModal: false,
    };
  }

  if (scene === "action_bar") {
    return {
      snapshot: makeTableSnapshot({
        config: baseConfig,
        seats: DEMO_SEATS,
        player_view: makePlayerView({
          viewer: "East",
          phase: "Playing",
          hand: {
            closed_tiles: [0, 4, 8, 9, 13, 18, 22, 22, 22, 26, 27, 31, 33],
            drawn_tile: null,
          },
          seats: [
            makePublicSeatView("East", {
              score: 16,
              hand_count: 13,
              river: DEMO_RIVERS.East,
            }),
            makePublicSeatView("South", {
              score: 4,
              hand_count: 13,
              river: [
                ...DEMO_RIVERS.South,
                { tile: 22, seq: 99, is_tsumogiri: false, claimed_by: null },
              ],
            }),
            makePublicSeatView("West", {
              score: -8,
              hand_count: 10,
              river: DEMO_RIVERS.West,
              melds: [{ Pon: { tile: 27, from_seat: "North", seq: 40 } }],
            }),
            makePublicSeatView("North", {
              score: -12,
              hand_count: 13,
              river: DEMO_RIVERS.North,
            }),
          ],
          wall_remaining: 41,
          turn: {
            wind_round: "East",
            hand_number: 2,
            dealer_streak: 1,
            dealer: "East",
            active_seat: "South",
          },
          legal_actions: [
            { ExposedKong: { tile: 22 } },
            { Pon: { tile: 22 } },
            "Pass",
          ],
        }),
      }),
      activeBubbles: makeBubbles(undefined, "五筒出来了！快看要不要碰！"),
      showSettlementModal: false,
    };
  }

  if (scene === "settlement_badge") {
    return {
      snapshot: makeTableSnapshot({
        config: baseConfig,
        seats: DEMO_SEATS,
        player_view: makePlayerView({
          viewer: "East",
          phase: "BetweenHands",
          hand: {
            closed_tiles: RICH_SELF_CLOSED,
            drawn_tile: RICH_SELF_DRAWN,
          },
          seats: [
            makePublicSeatView("East", {
              score: 64,
              hand_count: 14,
              river: DEMO_RIVERS.East,
            }),
            makePublicSeatView("South", {
              score: -12,
              hand_count: 10,
              river: DEMO_RIVERS.South,
              melds: [{ Pon: { tile: 17, from_seat: "West", seq: 42 } }],
            }),
            makePublicSeatView("West", {
              score: -24,
              hand_count: 10,
              river: DEMO_RIVERS.West,
              melds: [{ ExposedKong: { tile: 32, from_seat: "North", seq: 46 } }],
            }),
            makePublicSeatView("North", {
              score: -28,
              hand_count: 13,
              river: DEMO_RIVERS.North,
            }),
          ],
          wall_remaining: 24,
          turn: {
            wind_round: "East",
            hand_number: 2,
            dealer_streak: 1,
            dealer: "East",
            active_seat: null,
          },
          legal_actions: ["ConfirmNextHand"],
          last_settlement: DEMO_SETTLEMENT,
          ready_seats: [false, true, true, false],
        }),
      }),
      activeBubbles: makeBubbles("白板都能自摸，这运气没谁了～"),
      showSettlementModal: true,
    };
  }

  // Default: `self_draw` (and backdrop for `gallery_34`)
  return {
    snapshot: makeTableSnapshot({
      config: baseConfig,
      seats: DEMO_SEATS,
      player_view: makePlayerView({
        viewer: "East",
        phase: "Playing",
        hand: {
          closed_tiles: RICH_SELF_CLOSED,
          drawn_tile: RICH_SELF_DRAWN,
        },
        seats: [
          makePublicSeatView("East", {
            score: 16,
            hand_count: 14,
            river: DEMO_RIVERS.East,
          }),
          makePublicSeatView("South", {
            score: 4,
            hand_count: 13,
            river: DEMO_RIVERS.South,
          }),
          makePublicSeatView("West", {
            score: -8,
            hand_count: 10,
            river: DEMO_RIVERS.West,
            melds: [{ Pon: { tile: 27, from_seat: "North", seq: 40 } }],
          }),
          makePublicSeatView("North", {
            score: -12,
            hand_count: 10,
            river: DEMO_RIVERS.North,
            melds: [{ ConcealedKong: { tile: null, seq: 55 } }],
          }),
        ],
        wall_remaining: 42,
        turn: {
          wind_round: "East",
          hand_number: 2,
          dealer_streak: 1,
          dealer: "East",
          active_seat: "East",
        },
        legal_actions: makeAllDiscardActions(RICH_SELF_CLOSED, RICH_SELF_DRAWN),
      }),
    }),
    activeBubbles: makeBubbles("这把牌面不错嘛，别藏着掖着啦～"),
    showSettlementModal: false,
  };
}
