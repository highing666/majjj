// PROTOTYPE (throwaway for #78): Three structurally distinct traditional tile & layout variants
// switchable via `?variant=a|b|c` directly on the existing Majjj table host route (Sub-shape A).

import protoAtlasAJson from "../../../assets/atlas/prototype-tiles-a@2x.json";
import protoAtlasAPngUrl from "../../../assets/atlas/prototype-tiles-a@2x.png";
import protoAtlasBJson from "../../../assets/atlas/prototype-tiles-b@2x.json";
import protoAtlasBPngUrl from "../../../assets/atlas/prototype-tiles-b@2x.png";
import protoAtlasCJson from "../../../assets/atlas/prototype-tiles-c@2x.json";
import protoAtlasCPngUrl from "../../../assets/atlas/prototype-tiles-c@2x.png";
import type { TileBadgeDimensions, TileBadgeSize } from "../tileBadgeAtlas";

export interface PrototypeSpritesheetMeta {
  readonly frames: Record<
    string,
    {
      readonly frame: {
        readonly x: number;
        readonly y: number;
        readonly w: number;
        readonly h: number;
      };
    }
  >;
  readonly meta: {
    readonly size?: {
      readonly w: number;
      readonly h: number;
    };
  };
}

export type PrototypeVariantId = "a" | "b" | "c";

export type PrototypeSceneId =
  | "self_draw"
  | "other_turn"
  | "post_pon"
  | "action_bar"
  | "gallery_34"
  | "settlement_badge"
  | "live";

export interface PrototypeVariantMeta {
  readonly id: PrototypeVariantId;
  readonly code: string;
  readonly name: string;
  readonly shortName: string;
  readonly tagline: string;
  readonly materialNote: string;
  readonly fontNote: string;
  readonly whiteDragonNote: string;
  readonly layoutNote: string;
  readonly atlasPngUrl: string;
  readonly atlasMetaJson: PrototypeSpritesheetMeta;
}

export const PROTOTYPE_VARIANT_ORDER: readonly PrototypeVariantId[] = [
  "a",
  "b",
  "c",
] as const;

export const PROTOTYPE_VARIANTS: Record<
  PrototypeVariantId,
  PrototypeVariantMeta
> = {
  a: {
    id: "a",
    code: "A",
    name: "国标机麻·厚玉雕版",
    shortName: "厚玉雕版",
    tagline: "亮奶白厚机麻块 + 浓翠绿背 + 魏碑深雕 + 回纹蓝框白板",
    materialNote: "亮奶白厚块 (#F8F3E8) · 明显翠绿厚背与高光斜边",
    fontNote: "WeibeiTC-Bold（魏碑粗体阴刻）",
    whiteDragonNote: "双线回纹蓝框白板",
    layoutNote: "均衡剧场 · 底部拓宽手牌 (上限88px) · 10列牌河 (上限46px)",
    atlasPngUrl: protoAtlasAPngUrl,
    atlasMetaJson: protoAtlasAJson as unknown as PrototypeSpritesheetMeta,
  },
  b: {
    id: "b",
    code: "B",
    name: "古法骨牌·暖象牙手刻",
    shortName: "古法骨牌",
    tagline: "暖象牙骨润圆角 + 深松翠背 + 行楷手刻 + 卷角复古蓝框白板",
    materialNote: "暖古象牙 (#F3E6CC) · 圆润骨牌倒角与深松翠背",
    fontNote: "STXingkaiTC-Bold（华文行楷粗体手刻）",
    whiteDragonNote: "卷角虚线复古蓝框白板",
    layoutNote: "宽阔中场 · 窄翼大牌河 9列 (上限48px) · 舒展间距手牌",
    atlasPngUrl: protoAtlasBPngUrl,
    atlasMetaJson: protoAtlasBJson as unknown as PrototypeSpritesheetMeta,
  },
  c: {
    id: "c",
    code: "C",
    name: "精雕棋牌·极速高辨识",
    shortName: "极速高辨识",
    tagline: "收窄边框放大牌心 + 超粗楷体高对比 + 手机横屏优先大牌河",
    materialNote: "高白瓷感 (#FCFBF7) · 轻薄立体边释放最大牌面占比",
    fontNote: "STKaitiTC-Black（华文楷体黑体高对比）",
    whiteDragonNote: "粗线切角高辨识蓝框白板",
    layoutNote: "竞技大牌 · 极限宽手牌 (上限92px) · 8列大牌河 (上限50px)",
    atlasPngUrl: protoAtlasCPngUrl,
    atlasMetaJson: protoAtlasCJson as unknown as PrototypeSpritesheetMeta,
  },
};

export interface PrototypeSceneMeta {
  readonly id: PrototypeSceneId;
  readonly label: string;
  readonly shortLabel: string;
  readonly hint: string;
}

export const PROTOTYPE_SCENES: readonly PrototypeSceneMeta[] = [
  {
    id: "self_draw",
    label: "自家回合摸牌",
    shortLabel: "摸牌",
    hint: "无底框/无金框 · 暖光铺底 + 金色胶囊 + 摸牌抬起5px + ▼箭头",
  },
  {
    id: "other_turn",
    label: "非自家回合",
    shortLabel: "非自家",
    hint: "手牌保持100%原色亮度（绝不压暗、绝不加框）",
  },
  {
    id: "post_pon",
    label: "碰后出牌",
    shortLabel: "碰后",
    hint: "已取消单张金框，仅保留碰后金色胶囊提示与底部鸣牌联动",
  },
  {
    id: "action_bar",
    label: "鸣牌操作条",
    shortLabel: "鸣牌",
    hint: "碰/明杠/过操作条保持紧凑 xs TileBadge，手牌无遮挡",
  },
  {
    id: "gallery_34",
    label: "全34张牌面一览",
    shortLabel: "34张",
    hint: "立牌（手牌）与倒牌（牌河/鸣牌）双体3D细节同屏细看",
  },
  {
    id: "settlement_badge",
    label: "结算TileBadge",
    shortLabel: "结算",
    hint: "结算弹窗 + xs(22×30) / sm(30×41) / md(38×52) 尺寸对比",
  },
];

/**
 * Enlarged `#78` TileBadge sizes: keeps ActionBar `xs` compact while bumping
 * settlement/history `sm` and `md` by ~15–20%.
 */
export const PROTOTYPE_TILE_BADGE_DIMENSIONS: Record<
  TileBadgeSize,
  TileBadgeDimensions
> = {
  xs: { width: 22, height: 30 },
  sm: { width: 30, height: 41 },
  md: { width: 38, height: 52 },
};

export function normalizePrototypeVariant(
  raw: string | null | undefined,
): PrototypeVariantId | null {
  if (!raw) {
    return null;
  }
  const lower = raw.trim().toLowerCase();
  if (lower === "a" || lower === "b" || lower === "c") {
    return lower;
  }
  return null;
}

export function readPrototypeVariantFromUrl(): PrototypeVariantId {
  if (typeof window === "undefined") {
    return "a";
  }
  const params = new URLSearchParams(window.location.search);
  return normalizePrototypeVariant(params.get("variant")) ?? "a";
}

export function readPrototypeSceneFromUrl(): PrototypeSceneId | null {
  if (typeof window === "undefined") {
    return null;
  }
  const params = new URLSearchParams(window.location.search);
  const raw = params.get("scene")?.trim().toLowerCase();
  if (
    raw === "self_draw" ||
    raw === "other_turn" ||
    raw === "post_pon" ||
    raw === "action_bar" ||
    raw === "gallery_34" ||
    raw === "settlement_badge" ||
    raw === "live"
  ) {
    return raw;
  }
  return null;
}

export function syncPrototypeUrlParams(
  variant: PrototypeVariantId,
  scene?: PrototypeSceneId,
): void {
  if (typeof window === "undefined") {
    return;
  }
  const url = new URL(window.location.href);
  url.searchParams.set("variant", variant);
  if (scene) {
    url.searchParams.set("scene", scene);
  }
  window.history.replaceState(null, "", url.toString());
}

export function cyclePrototypeVariant(
  current: PrototypeVariantId,
  delta: 1 | -1,
): PrototypeVariantId {
  const idx = PROTOTYPE_VARIANT_ORDER.indexOf(current);
  const nextIdx =
    (idx + delta + PROTOTYPE_VARIANT_ORDER.length) %
    PROTOTYPE_VARIANT_ORDER.length;
  return PROTOTYPE_VARIANT_ORDER[nextIdx];
}
