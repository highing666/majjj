import { describe, expect, it } from "vitest";
import type { DiscardEntry } from "../protocol/generated/DiscardEntry";
import type { MeldView } from "../protocol/generated/MeldView";
import type { PlayerView } from "../protocol/generated/PlayerView";
import type { PublicSeatView } from "../protocol/generated/PublicSeatView";
import type { Seat } from "../protocol/generated/Seat";
import {
  matchModeLabel,
  patchLobbyConfig,
} from "../lobby/lobbyConfig";
import {
  formatCompactRoundTitle,
  formatRoundTitle,
  getDragonKind,
  getSuitUnit,
  getTileLabel,
  getTileRank,
  getTileSuit,
  getTileTone,
} from "./tiles";
import {
  buildActionBarItems,
  buildMatchStandings,
  buildMeldRenderItems,
  buildRevealedMeldItems,
  buildSeatBadgeModel,
  buildVisibleRiverRows,
  computeCanvasResolution,
  extractContentBoxSize,
  findLegalDiscardAction,
  formatScoreDelta,
  getRelativeSeats,
  resolveActionBarShortcut,
  seatRelativeSlot,
} from "./viewModel";

function makePlayerView(overrides: Partial<PlayerView> = {}): PlayerView {
  const emptySeat = (seat: Seat): PublicSeatView => ({
    seat,
    score: 0,
    hand_count: 13,
    river: [],
    melds: [],
    last_draw_or_meld_seq: 0,
  });

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
      { ...emptySeat("East"), hand_count: 14 },
      emptySeat("South"),
      emptySeat("West"),
      emptySeat("North"),
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
    seq: 2,
    ...overrides,
  };
}

describe("tile metadata helpers", () => {
  it("maps all 34 tiles (0..33) to suit, rank, and canonical labels", () => {
    expect(getTileSuit(0)).toBe("Man");
    expect(getTileRank(0)).toBe(1);
    expect(getTileLabel(0)).toBe("一万");
    expect(getTileLabel(8)).toBe("九万");

    expect(getTileSuit(9)).toBe("Bamboo");
    expect(getTileRank(9)).toBe(1);
    expect(getTileLabel(9)).toBe("一条");
    expect(getTileLabel(17)).toBe("九条");

    expect(getTileSuit(18)).toBe("Dot");
    expect(getTileRank(26)).toBe(9);
    expect(getTileLabel(18)).toBe("一筒");
    expect(getTileLabel(26)).toBe("九筒");

    expect(getTileSuit(27)).toBe("Wind");
    expect(getTileRank(27)).toBeNull();
    expect([27, 28, 29, 30].map(getTileLabel)).toEqual([
      "东",
      "南",
      "西",
      "北",
    ]);

    expect(getTileSuit(31)).toBe("Dragon");
    expect(getTileRank(33)).toBeNull();
    expect([31, 32, 33].map(getTileLabel)).toEqual(["中", "发", "白"]);
    expect([31, 32, 33].map(getDragonKind)).toEqual(["Red", "Green", "White"]);
    expect(getDragonKind(0)).toBeNull();
    expect([getSuitUnit("Man"), getSuitUnit("Bamboo"), getSuitUnit("Dot"), getSuitUnit("Dragon")]).toEqual([
      "万",
      "条",
      "筒",
      "",
    ]);
    expect([0, 9, 18, 27, 31, 32, 33].map(getTileTone)).toEqual([
      "man",
      "bamboo",
      "dot",
      "ink",
      "red",
      "bamboo",
      "dot",
    ]);
  });

  it("formats round titles with GLOSSARY terms", () => {
    expect(
      formatRoundTitle({
        wind_round: "East",
        hand_number: 3,
        dealer_streak: 0,
        dealer: "West",
        active_seat: "West",
      }),
    ).toBe("东风圈 第 3 小局");
    expect(
      formatRoundTitle({
        wind_round: "East",
        hand_number: 2,
        dealer_streak: 2,
        dealer: "South",
        active_seat: "South",
      }),
    ).toBe("东风圈 第 2 小局 · 连庄 2");
    expect(
      formatCompactRoundTitle({
        wind_round: "East",
        hand_number: 3,
        dealer_streak: 0,
        dealer: "West",
        active_seat: "West",
      }),
    ).toBe("东3");
    expect(
      formatCompactRoundTitle({
        wind_round: "South",
        hand_number: 2,
        dealer_streak: 2,
        dealer: "South",
        active_seat: "South",
      }),
    ).toBe("南2·连2");
  });
});

describe("table viewModel helpers", () => {
  it("computes counter-clockwise relative seat positions for each viewer", () => {
    expect(getRelativeSeats("East")).toEqual({
      bottom: "East",
      right: "South",
      top: "West",
      left: "North",
    });
    expect(getRelativeSeats("West")).toEqual({
      bottom: "West",
      right: "North",
      top: "East",
      left: "South",
    });
    expect(seatRelativeSlot("West", "West")).toBe("bottom");
    expect(seatRelativeSlot("West", "North")).toBe("right");
    expect(seatRelativeSlot("West", "East")).toBe("top");
    expect(seatRelativeSlot("West", "South")).toBe("left");
  });

  it("omits claimed discards and chunks rivers strictly into 6-tile rows", () => {
    const river: DiscardEntry[] = [
      { tile: 0, is_tsumogiri: false, claimed_by: null, seq: 1 },
      { tile: 1, is_tsumogiri: true, claimed_by: "South", seq: 2 },
      { tile: 2, is_tsumogiri: false, claimed_by: null, seq: 3 },
      { tile: 3, is_tsumogiri: true, claimed_by: null, seq: 4 },
      { tile: 4, is_tsumogiri: false, claimed_by: null, seq: 5 },
      { tile: 5, is_tsumogiri: false, claimed_by: null, seq: 6 },
      { tile: 6, is_tsumogiri: true, claimed_by: null, seq: 7 },
      { tile: 7, is_tsumogiri: false, claimed_by: null, seq: 8 },
    ];

    const rows = buildVisibleRiverRows(river);
    expect(rows).toHaveLength(2);
    expect(rows[0].map((e) => e.tile)).toEqual([0, 2, 3, 4, 5, 6]);
    expect(rows[0][2].is_tsumogiri).toBe(true);
    expect(rows[1].map((e) => e.tile)).toEqual([7]);
  });

  it("projects meld tiles with viewer vs opponent ConcealedKong and pendingAddedKong", () => {
    const melds: MeldView[] = [
      { Pon: { tile: 27, from_seat: "South", seq: 6 } },
      { ConcealedKong: { tile: 31, seq: 9 } },
      { ConcealedKong: { tile: null, seq: 12 } },
    ];

    const items = buildMeldRenderItems(melds, { seat: "East", tile: 27 }, "East");
    expect(items).toHaveLength(3);
    expect(items[0]).toMatchObject({
      kind: "Pon",
      pendingAddedKong: true,
    });
    expect(items[0].tiles).toEqual([
      { tile: 27, faceUp: true },
      { tile: 27, faceUp: true },
      { tile: 27, faceUp: true },
    ]);
    // Viewer concealed kong exposes the tile face-up in 3-back-1-face layout
    expect(items[1].tiles).toEqual([
      { tile: null, faceUp: false },
      { tile: null, faceUp: false },
      { tile: 31, faceUp: true },
      { tile: null, faceUp: false },
    ]);
    // Opponent concealed kong renders 4 face-down backs
    expect(items[2].tiles).toEqual([
      { tile: null, faceUp: false },
      { tile: null, faceUp: false },
      { tile: null, faceUp: false },
      { tile: null, faceUp: false },
    ]);
  });

  it("matches legal Discard actions strictly by tile and from_drawn", () => {
    const legal = [
      { Discard: { tile: 5, from_drawn: false } },
      { Discard: { tile: 5, from_drawn: true } },
    ];
    expect(findLegalDiscardAction(legal, 5, true)).toEqual({
      Discard: { tile: 5, from_drawn: true },
    });
    expect(findLegalDiscardAction(legal, 5, false)).toEqual({
      Discard: { tile: 5, from_drawn: false },
    });
    expect(findLegalDiscardAction(legal, 9, false)).toBeNull();
  });

  it("builds ActionBar items strictly from legal_actions and never renders Chi in Tuidaohu", () => {
    const view = makePlayerView({
      legal_actions: [
        { Discard: { tile: 5, from_drawn: true } },
        "Pass",
        { Pon: { tile: 27 } },
        { ExposedKong: { tile: 27 } },
        { AddedKong: { tile: 9 } },
        { ConcealedKong: { tile: 31 } },
        { RobKongHu: { tile: 28 } },
        { Ron: { tile: 18 } },
        "Tsumo",
        { Chi: { tile: 1, with: [0, 2] } },
      ],
    });

    const items = buildActionBarItems(view);
    expect(items.map((item) => item.label)).toEqual([
      "自摸",
      "点炮胡",
      "抢杠胡",
      "暗杠",
      "补杠",
      "明杠",
      "碰",
      "过",
    ]);
    expect(items.map((item) => item.shortcutKey)).toEqual([
      "h",
      "h",
      "h",
      "g",
      "g",
      "g",
      "p",
      "escape",
    ]);
    expect(items.some((item) => item.label.includes("吃"))).toBe(false);

    expect(matchModeLabel("EastOnly")).toBe("东风战");
    expect(matchModeLabel("FreeTable")).toBe("自由桌");
    expect(
      patchLobbyConfig(
        {
          mode: "EastOnly",
          seating: "Random",
          ai_difficulty: "Strong",
          pace: "Standard",
        },
        { mode: "FreeTable", pace: "Fast" },
      ),
    ).toEqual({
      mode: "FreeTable",
      seating: "Random",
      ai_difficulty: "Strong",
      pace: "Fast",
    });

    // BetweenHands actions are excluded from in-play ActionBar
    const betweenHands = makePlayerView({
      phase: "BetweenHands",
      legal_actions: ["ConfirmNextHand", "EndMatch"],
    });
    expect(buildActionBarItems(betweenHands)).toEqual([]);
  });

  it("computes clamped DPR <= 2 and extracts ResizeObserver contentBoxSize safely", () => {
    expect(computeCanvasResolution(1)).toBe(1);
    expect(computeCanvasResolution(2)).toBe(2);
    expect(computeCanvasResolution(3)).toBe(2);
    expect(computeCanvasResolution(0)).toBe(1);

    expect(
      extractContentBoxSize({
        contentBoxSize: [{ inlineSize: 960.4, blockSize: 640.8 }],
      }),
    ).toEqual({ width: 960, height: 641 });

    expect(
      extractContentBoxSize({
        contentBoxSize: { inlineSize: 390, blockSize: 520 },
      }),
    ).toEqual({ width: 390, height: 520 });

    expect(
      extractContentBoxSize({
        contentRect: { width: 412, height: 600 },
      }),
    ).toEqual({ width: 412, height: 600 });
  });

  it("formats revealed melds (including revealed ConcealedKong), score deltas, and MatchSummary standings", () => {
    const revealed = buildRevealedMeldItems([
      { Pon: { tile: 10, from_seat: "East", seq: 5 } },
      { ConcealedKong: { tile: 32, seq: 8 } },
    ]);
    expect(revealed).toHaveLength(2);
    expect(revealed[1].kind).toBe("ConcealedKong");
    expect(revealed[1].tiles).toEqual([
      { tile: null, faceUp: false },
      { tile: null, faceUp: false },
      { tile: 32, faceUp: true },
      { tile: null, faceUp: false },
    ]);

    expect(formatScoreDelta(12)).toBe("+12");
    expect(formatScoreDelta(0)).toBe("0");
    expect(formatScoreDelta(-6)).toBe("-6");

    const seats = (["East", "South", "West", "North"] as const).map(
      (seat, idx) => ({
        seat,
        kind: idx === 0 ? ("Human" as const) : ("Ai" as const),
        nickname: idx === 0 ? "小明" : `AI ${idx}`,
        connected: true,
        ai_assisted: false,
        ai_difficulty: null,
        ai_style: null,
        voice_pack_id: null,
      }),
    ) as [
      {
        seat: Seat;
        kind: "Human";
        nickname: string;
        connected: boolean;
        ai_assisted: boolean;
        ai_difficulty: null;
        ai_style: null;
        voice_pack_id: null;
      },
      {
        seat: Seat;
        kind: "Ai";
        nickname: string;
        connected: boolean;
        ai_assisted: boolean;
        ai_difficulty: null;
        ai_style: null;
        voice_pack_id: null;
      },
      {
        seat: Seat;
        kind: "Ai";
        nickname: string;
        connected: boolean;
        ai_assisted: boolean;
        ai_difficulty: null;
        ai_style: null;
        voice_pack_id: null;
      },
      {
        seat: Seat;
        kind: "Ai";
        nickname: string;
        connected: boolean;
        ai_assisted: boolean;
        ai_difficulty: null;
        ai_style: null;
        voice_pack_id: null;
      },
    ];

    const standings = buildMatchStandings(
      {
        mode: "EastOnly",
        hands_played: 4,
        initial_scores: [0, 0, 0, 0],
        final_scores: [-4, 12, 0, -8],
        net_deltas: [-4, 12, 0, -8],
      },
      seats,
    );
    expect(standings.map((row) => row.seat)).toEqual([
      "South",
      "West",
      "East",
      "North",
    ]);
    expect(standings[0]).toMatchObject({
      rank: 1,
      seatName: "",
      nickname: "AI 1",
      finalScore: 12,
      netDelta: 12,
    });
    expect(standings[2]).toMatchObject({
      rank: 3,
      seatName: "",
      nickname: "小明",
    });

    const seatedStandings = buildMatchStandings(
      {
        mode: "EastOnly",
        hands_played: 4,
        initial_scores: [0, 0, 0, 0],
        final_scores: [-4, 12, 0, -8],
        net_deltas: [-4, 12, 0, -8],
      },
      seats,
      "East",
    );
    expect(seatedStandings[0]).toMatchObject({
      rank: 1,
      seat: "South",
      seatName: "下家",
      nickname: "AI 1",
    });
    expect(seatedStandings[2]).toMatchObject({
      rank: 3,
      seat: "East",
      seatName: "自家",
      nickname: "小明",
    });
    for (const row of [...standings, ...seatedStandings]) {
      expect(row.seatName).not.toMatch(/东座|南座|西座|北座/);
      expect(row.nickname).not.toMatch(/东座|南座|西座|北座/);
    }
  });

  it("resolves H/G/P/Esc ActionBar shortcuts only when unobstructed and unambiguous", () => {
    const view = makePlayerView({
      legal_actions: [{ Pon: { tile: 27 } }, "Pass"],
    });
    const items = buildActionBarItems(view);

    expect(
      resolveActionBarShortcut(items, "p", {
        disabled: false,
        hasModalOpen: false,
        isInputFocused: false,
      }),
    ).toEqual({ Pon: { tile: 27 } });

    expect(
      resolveActionBarShortcut(items, "Escape", {
        disabled: false,
        hasModalOpen: false,
        isInputFocused: false,
      }),
    ).toBe("Pass");

    // Disabled when modal open, input focused, or ActionBar disabled
    expect(
      resolveActionBarShortcut(items, "p", {
        disabled: false,
        hasModalOpen: true,
        isInputFocused: false,
      }),
    ).toBeNull();
    expect(
      resolveActionBarShortcut(items, "p", {
        disabled: false,
        hasModalOpen: false,
        isInputFocused: true,
      }),
    ).toBeNull();
    expect(
      resolveActionBarShortcut(items, "Escape", {
        disabled: true,
        hasModalOpen: false,
        isInputFocused: false,
      }),
    ).toBeNull();

    // Ambiguous multiple kong actions do not fire blindly on G
    const multiKongItems = buildActionBarItems(
      makePlayerView({
        legal_actions: [
          { ConcealedKong: { tile: 31 } },
          { AddedKong: { tile: 9 } },
          "Pass",
        ],
      }),
    );
    expect(
      resolveActionBarShortcut(multiKongItems, "g", {
        disabled: false,
        hasModalOpen: false,
        isInputFocused: false,
      }),
    ).toBeNull();
  });

  it("builds corner seat badge view models with 2-char relative role, dealer, active, offline, and score tags", () => {
    const model = buildSeatBadgeModel(
      "South",
      {
        seat: "South",
        score: 8,
        hand_count: 13,
        river: [],
        melds: [],
        last_draw_or_meld_seq: 0,
      },
      {
        seat: "South",
        kind: "Human",
        nickname: "小红",
        connected: false,
        ai_assisted: false,
        ai_difficulty: null,
        ai_style: null,
        voice_pack_id: null,
      },
      "East",
      "South",
      "South",
    );

    expect(model).toEqual({
      role: "下家",
      isDealer: true,
      isActive: true,
      isOffline: true,
      isAiAssisted: false,
      name: "小红",
      scoreText: "+8",
    });
    expect("wind" in model).toBe(false);

    const assistedModel = buildSeatBadgeModel(
      "South",
      {
        seat: "South",
        score: 8,
        hand_count: 13,
        river: [],
        melds: [],
        last_draw_or_meld_seq: 0,
      },
      {
        seat: "South",
        kind: "Human",
        nickname: "小红",
        connected: false,
        ai_assisted: true,
        ai_difficulty: "Strong",
        ai_style: "Balanced",
        voice_pack_id: null,
      },
      "East",
      "South",
      "South",
    );
    expect(assistedModel.isAiAssisted).toBe(true);
    expect(assistedModel.isOffline).toBe(true);
    expect(assistedModel.role).toBe("下家");

    const emptyHumanModel = buildSeatBadgeModel(
      "South",
      {
        seat: "South",
        score: 0,
        hand_count: 13,
        river: [],
        melds: [],
        last_draw_or_meld_seq: 0,
      },
      {
        seat: "South",
        kind: "Human",
        nickname: "",
        connected: true,
        ai_assisted: false,
        ai_difficulty: null,
        ai_style: null,
        voice_pack_id: null,
      },
      "East",
      "East",
      null,
    );
    expect(emptyHumanModel.role).toBe("下家");
    expect(emptyHumanModel.name).toBe("2号位");

    const whitespaceHumanModel = buildSeatBadgeModel(
      "West",
      {
        seat: "West",
        score: 0,
        hand_count: 13,
        river: [],
        melds: [],
        last_draw_or_meld_seq: 0,
      },
      {
        seat: "West",
        kind: "Human",
        nickname: "   ",
        connected: true,
        ai_assisted: false,
        ai_difficulty: null,
        ai_style: null,
        voice_pack_id: null,
      },
      "East",
      "East",
      null,
    );
    expect(whitespaceHumanModel.role).toBe("对门");
    expect(whitespaceHumanModel.name).toBe("3号位");

    const whitespaceAiModel = buildSeatBadgeModel(
      "North",
      {
        seat: "North",
        score: 0,
        hand_count: 13,
        river: [],
        melds: [],
        last_draw_or_meld_seq: 0,
      },
      {
        seat: "North",
        kind: "Ai",
        nickname: "   ",
        connected: true,
        ai_assisted: false,
        ai_difficulty: "Strong",
        ai_style: "Balanced",
        voice_pack_id: null,
      },
      "East",
      "East",
      null,
    );
    expect(whitespaceAiModel.name).toBe("AI");
  });

  it("assigns full 2-char counter-clockwise relative roles with zero wind characters across all 4 viewer perspectives", () => {
    const seats: Seat[] = ["East", "South", "West", "North"];
    const expectedByViewer: Record<Seat, Record<Seat, string>> = {
      East: { East: "自家", South: "下家", West: "对门", North: "上家" },
      South: { South: "自家", West: "下家", North: "对门", East: "上家" },
      West: { West: "自家", North: "下家", East: "对门", South: "上家" },
      North: { North: "自家", East: "下家", South: "对门", West: "上家" },
    };

    for (const viewer of seats) {
      for (const seat of seats) {
        const badge = buildSeatBadgeModel(
          seat,
          {
            seat,
            score: -4,
            hand_count: 13,
            river: [],
            melds: [],
            last_draw_or_meld_seq: 0,
          },
          {
            seat,
            kind: "Ai",
            nickname: "",
            connected: true,
            ai_assisted: false,
            ai_difficulty: "Strong",
            ai_style: "Balanced",
            voice_pack_id: null,
          },
          viewer,
          "East",
          "West",
        );

        expect(badge.role).toBe(expectedByViewer[viewer][seat]);
        expect(badge.role).toHaveLength(2);
        expect(badge.role).not.toMatch(/[东南西北]/);
        expect(badge.isDealer).toBe(seat === "East");
        expect(badge.isActive).toBe(seat === "West");
        expect(badge.isOffline).toBe(false);
        expect(badge.isAiAssisted).toBe(false);
        expect(badge.name).toBe("AI");
        expect(badge.scoreText).toBe("-4");
      }
    }
  });
});
