import type { Tile } from "../../protocol/generated/Tile";

export type PrototypeVariantKey = "A" | "B" | "C";

export type ViewportPreset =
  | "auto"
  | "mac-1440"
  | "mac-1280"
  | "iphone-land"
  | "iphone-port";

export type TurnScenario =
  | "my_draw_13_1"
  | "my_post_pon_13"
  | "my_action_prompt"
  | "wait_right"
  | "wait_top"
  | "wait_left";

export type RiverDensity = "mid" | "late";

export type RelativeSlot = "bottom" | "right" | "top" | "left";

export interface MockRiverEntry {
  tile: Tile;
  isTsumogiri: boolean;
  seq: number;
}

export interface MockMeld {
  kind: "Pon" | "OpenKong" | "ConcealedKong";
  tiles: (Tile | null)[];
}

export interface MockSeatData {
  slot: RelativeSlot;
  role: "自家" | "下家" | "对门" | "上家";
  name: string;
  personaTitle: string;
  avatarText: string;
  avatarAccent: string;
  moodLabel: string;
  banter: string | null;
  score: number;
  isDealer: boolean;
  isHuman: boolean;
  concealedCount: number;
  melds: MockMeld[];
  river: MockRiverEntry[];
}

export interface MockActionItem {
  key: string;
  label: string;
  tone: "hu" | "kong" | "pon" | "pass";
  tile: Tile | null;
  tileName: string | null;
  shortcut: string;
}

export interface MockTableState {
  scenario: TurnScenario;
  density: RiverDensity;
  roundCompact: string;
  roundFull: string;
  modeLabel: string;
  wallRemaining: number;
  activeSlot: RelativeSlot;
  latestRiverSeq: number;
  viewerClosed: Tile[];
  viewerDrawn: Tile | null;
  viewerSelectedIndex: number | null;
  actionItems: MockActionItem[];
  lastActionNote: string;
  seats: Record<RelativeSlot, MockSeatData>;
}

export const VIEWPORT_DIMENSIONS: Record<
  Exclude<ViewportPreset, "auto">,
  { width: number; height: number; label: string }
> = {
  "mac-1440": { width: 1440, height: 900, label: "Mac 宽屏 1440×900" },
  "mac-1280": { width: 1280, height: 720, label: "Mac 1280×720" },
  "iphone-land": { width: 844, height: 390, label: "iPhone 横屏 844×390" },
  "iphone-port": { width: 390, height: 844, label: "iPhone 竖屏 390×844" },
};

export const SCENARIO_LABELS: Record<TurnScenario, string> = {
  my_draw_13_1: "轮到自家 · 摸牌 13+1",
  my_post_pon_13: "轮到自家 · 碰后 13张（无摸入牌）",
  my_action_prompt: "鸣牌/胡动作栏接管",
  wait_right: "等待下家（自家待机）",
  wait_top: "等待对门（自家待机）",
  wait_left: "等待上家（自家待机）",
};

const MID_RIVERS: Record<RelativeSlot, { tile: Tile; tsumogiri?: boolean }[]> = {
  bottom: [
    { tile: 27 },
    { tile: 30, tsumogiri: true },
    { tile: 8 },
    { tile: 17 },
    { tile: 26, tsumogiri: true },
    { tile: 28 },
    { tile: 11 },
    { tile: 19, tsumogiri: true },
  ],
  right: [
    { tile: 29 },
    { tile: 0 },
    { tile: 18, tsumogiri: true },
    { tile: 33 },
    { tile: 7 },
    { tile: 10 },
    { tile: 21, tsumogiri: true },
  ],
  top: [
    { tile: 28 },
    { tile: 27, tsumogiri: true },
    { tile: 1 },
    { tile: 25 },
    { tile: 9 },
    { tile: 12, tsumogiri: true },
    { tile: 23 },
    { tile: 6 },
    { tile: 33 },
  ],
  left: [
    { tile: 30 },
    { tile: 29 },
    { tile: 18 },
    { tile: 2 },
    { tile: 16, tsumogiri: true },
    { tile: 22 },
  ],
};

const LATE_EXTRA: Record<RelativeSlot, { tile: Tile; tsumogiri?: boolean }[]> = {
  bottom: [
    { tile: 20 },
    { tile: 3, tsumogiri: true },
    { tile: 14 },
    { tile: 24 },
    { tile: 15, tsumogiri: true },
    { tile: 5 },
    { tile: 22 },
    { tile: 13 },
    { tile: 2 },
    { tile: 25, tsumogiri: true },
  ],
  right: [
    { tile: 12 },
    { tile: 4, tsumogiri: true },
    { tile: 23 },
    { tile: 16 },
    { tile: 5 },
    { tile: 24, tsumogiri: true },
    { tile: 15 },
    { tile: 3 },
    { tile: 19 },
  ],
  top: [
    { tile: 14 },
    { tile: 24, tsumogiri: true },
    { tile: 4 },
    { tile: 11 },
    { tile: 21 },
    { tile: 7, tsumogiri: true },
    { tile: 16 },
    { tile: 19 },
    { tile: 2 },
  ],
  left: [
    { tile: 13 },
    { tile: 24, tsumogiri: true },
    { tile: 6 },
    { tile: 15 },
    { tile: 21 },
    { tile: 8, tsumogiri: true },
    { tile: 12 },
    { tile: 25 },
    { tile: 3 },
  ],
};

function buildRivers(
  density: RiverDensity,
): { rivers: Record<RelativeSlot, MockRiverEntry[]>; maxSeq: number } {
  let seq = 1;
  const slots: RelativeSlot[] = ["bottom", "right", "top", "left"];
  const combined: Record<RelativeSlot, { tile: Tile; tsumogiri?: boolean }[]> = {
    bottom:
      density === "late"
        ? [...MID_RIVERS.bottom, ...LATE_EXTRA.bottom]
        : [...MID_RIVERS.bottom],
    right:
      density === "late"
        ? [...MID_RIVERS.right, ...LATE_EXTRA.right]
        : [...MID_RIVERS.right],
    top:
      density === "late"
        ? [...MID_RIVERS.top, ...LATE_EXTRA.top]
        : [...MID_RIVERS.top],
    left:
      density === "late"
        ? [...MID_RIVERS.left, ...LATE_EXTRA.left]
        : [...MID_RIVERS.left],
  };

  const maxLen = Math.max(...slots.map((s) => combined[s].length));
  const rivers: Record<RelativeSlot, MockRiverEntry[]> = {
    bottom: [],
    right: [],
    top: [],
    left: [],
  };

  for (let i = 0; i < maxLen; i += 1) {
    for (const slot of slots) {
      const item = combined[slot][i];
      if (item) {
        rivers[slot].push({
          tile: item.tile,
          isTsumogiri: Boolean(item.tsumogiri),
          seq,
        });
        seq += 1;
      }
    }
  }

  return { rivers, maxSeq: seq - 1 };
}

export function createMockTableState(
  scenario: TurnScenario,
  density: RiverDensity,
): MockTableState {
  const { rivers, maxSeq } = buildRivers(density);

  // Showcase all suits + honors cleanly: 一万, 五万, 九万, 一条, 五条, 八条, 九条, 一筒, 五筒, 七筒, 東, 中, 發
  const standard13: Tile[] = [0, 4, 8, 9, 13, 16, 17, 18, 22, 24, 27, 31, 32];
  // Post-pon 10 closed tiles + 八条×3 open pon meld (no drawn tile!)
  const postPon10: Tile[] = [0, 4, 4, 9, 13, 17, 18, 22, 27, 31];

  const isPostPon = scenario === "my_post_pon_13";
  const isPrompt = scenario === "my_action_prompt";

  const activeSlot: RelativeSlot =
    scenario === "wait_right"
      ? "right"
      : scenario === "wait_top"
        ? "top"
        : scenario === "wait_left"
          ? "left"
          : "bottom";

  const viewerClosed = isPostPon ? [...postPon10] : [...standard13];
  const viewerDrawn: Tile | null =
    scenario === "my_draw_13_1" ? 33 : null; // 白板 drawn tile in 13+1

  const actionItems: MockActionItem[] = isPrompt
    ? [
        { key: "hu", label: "胡", tone: "hu", tile: 18, tileName: "一筒", shortcut: "1" },
        { key: "pon", label: "碰", tone: "pon", tile: 4, tileName: "五万", shortcut: "2" },
        { key: "kong", label: "明杠", tone: "kong", tile: 31, tileName: "中", shortcut: "3" },
        { key: "pass", label: "过", tone: "pass", tile: null, tileName: null, shortcut: "Space" },
      ]
    : [];

  return {
    scenario,
    density,
    roundCompact: "东2",
    roundFull: "东风圈 第 2 小局",
    modeLabel: "东风战",
    wallRemaining: density === "late" ? 18 : 42,
    activeSlot,
    latestRiverSeq: maxSeq,
    viewerClosed,
    viewerDrawn,
    viewerSelectedIndex: null,
    actionItems,
    lastActionNote: SCENARIO_LABELS[scenario],
    seats: {
      bottom: {
        slot: "bottom",
        role: "自家",
        name: "阿哲",
        personaTitle: "人类玩家",
        avatarText: "哲",
        avatarAccent: "#d4a857",
        moodLabel: activeSlot === "bottom" ? "思考中" : "观望",
        banter: null,
        score: 26400,
        isDealer: true,
        isHuman: true,
        concealedCount: viewerClosed.length,
        melds: isPostPon
          ? [{ kind: "Pon", tiles: [16, 16, 16] }]
          : [{ kind: "Pon", tiles: [27, 27, 27] }],
        river: rivers.bottom,
      },
      right: {
        slot: "right",
        role: "下家",
        name: "毒舌小辣椒",
        personaTitle: "快攻牌风",
        avatarText: "辣",
        avatarAccent: "#d8513c",
        moodLabel: "拱火",
        banter: "刚拆完就摸白板？这手气我真看酸了！",
        score: 23800,
        isDealer: false,
        isHuman: false,
        concealedCount: 9,
        melds: [{ kind: "ConcealedKong", tiles: [null, null, null, null] }],
        river: rivers.right,
      },
      top: {
        slot: "top",
        role: "对门",
        name: "老谋算盘王",
        personaTitle: "防守大牌",
        avatarText: "王",
        avatarAccent: "#3b82f6",
        moodLabel: "窃喜",
        banter: "留着八条吧，反正你也听不了清一色。",
        score: 25900,
        isDealer: false,
        isHuman: false,
        concealedCount: 10,
        melds: [{ kind: "OpenKong", tiles: [20, 20, 20, 20] }],
        river: rivers.top,
      },
      left: {
        slot: "left",
        role: "上家",
        name: "阴阳茶博士",
        personaTitle: "均衡稳健",
        avatarText: "茶",
        avatarAccent: "#22a07b",
        moodLabel: "挑眉",
        banter: "下家这手切得挺心虚啊，生张一筒都不敢留？",
        score: 23900,
        isDealer: false,
        isHuman: false,
        concealedCount: 10,
        melds: [{ kind: "Pon", tiles: [32, 32, 32] }],
        river: rivers.left,
      },
    },
  };
}

export function getShortTileName(tile: Tile): string {
  const nums = ["一", "二", "三", "四", "五", "六", "七", "八", "九"];
  if (tile >= 0 && tile <= 8) return `${nums[tile]}万`;
  if (tile >= 9 && tile <= 17) return `${nums[tile - 9]}条`;
  if (tile >= 18 && tile <= 26) return `${nums[tile - 18]}筒`;
  const honors = ["東", "南", "西", "北", "中", "發", "白"];
  return honors[tile - 27] ?? "?";
}

export function applyMockDiscard(
  state: MockTableState,
  index: number,
  fromDrawn: boolean,
): MockTableState {
  if (state.activeSlot !== "bottom" || state.actionItems.length > 0) {
    return state;
  }

  let discardedTile: Tile | null = null;
  let nextClosed = [...state.viewerClosed];
  let nextDrawn = state.viewerDrawn;

  if (fromDrawn && state.viewerDrawn !== null) {
    discardedTile = state.viewerDrawn;
    nextDrawn = null;
  } else if (!fromDrawn && index >= 0 && index < nextClosed.length) {
    discardedTile = nextClosed[index];
    nextClosed.splice(index, 1);
    if (nextDrawn !== null) {
      nextClosed.push(nextDrawn);
      nextClosed.sort((a, b) => a - b);
      nextDrawn = null;
    }
  }

  if (discardedTile === null) {
    return state;
  }

  const nextSeq = state.latestRiverSeq + 1;
  const bottomSeat = state.seats.bottom;
  const nextBottomRiver: MockRiverEntry[] = [
    ...bottomSeat.river,
    {
      tile: discardedTile,
      isTsumogiri: fromDrawn,
      seq: nextSeq,
    },
  ];

  return {
    ...state,
    scenario: "wait_right",
    activeSlot: "right",
    latestRiverSeq: nextSeq,
    viewerClosed: nextClosed,
    viewerDrawn: nextDrawn,
    viewerSelectedIndex: null,
    lastActionNote: `自家${fromDrawn ? "摸切" : "手切"} ${getShortTileName(discardedTile)} → 轮到下家`,
    seats: {
      ...state.seats,
      bottom: {
        ...bottomSeat,
        concealedCount: nextClosed.length,
        river: nextBottomRiver,
      },
    },
  };
}
