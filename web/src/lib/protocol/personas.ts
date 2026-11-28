import type { AiStyle } from "./generated/AiStyle";
import type { PersonaId } from "./generated/PersonaId";
import type { PersonaMood } from "./generated/PersonaMood";
import type { QuickEmote } from "./generated/QuickEmote";

export const PERSONA_IDS: readonly PersonaId[] = [
  "QuickHothead",
  "TeaSarcasm",
  "StubbornBlamer",
  "ChaosInstigator",
  "TiltedMaster",
] as const;

export const PERSONA_MOODS: readonly PersonaMood[] = [
  "Smug",
  "Mocking",
  "Tilted",
  "Nervous",
  "Plotting",
  "Deadpan",
] as const;

export interface PersonaMeta {
  readonly id: PersonaId;
  readonly name: string;
  readonly defaultStyle: AiStyle;
  readonly styleLabel: string;
  readonly tagline: string;
  readonly initialMood: PersonaMood;
  readonly accentHex: string;
  readonly bgHex: string;
}

export interface PersonaMoodMeta {
  readonly mood: PersonaMood;
  readonly label: string;
  readonly shortLabel: string;
  readonly badgeBg: string;
  readonly badgeFg: string;
}

const STYLE_LABELS: Record<AiStyle, string> = {
  AggressiveFast: "快攻",
  Balanced: "均衡",
  DefensiveBigHand: "防守做大牌",
};

export const PERSONA_CATALOG: Record<PersonaId, PersonaMeta> = {
  QuickHothead: {
    id: "QuickHothead",
    name: "大川",
    defaultStyle: "AggressiveFast",
    styleLabel: STYLE_LABELS.AggressiveFast,
    tagline: "川渝急性子，快攻抢节奏",
    initialMood: "Smug",
    accentHex: "#E85D3F",
    bgHex: "#4D1D17",
  },
  TeaSarcasm: {
    id: "TeaSarcasm",
    name: "小晚",
    defaultStyle: "DefensiveBigHand",
    styleLabel: STYLE_LABELS.DefensiveBigHand,
    tagline: "笑里藏刀，爱记仇翻旧账",
    initialMood: "Mocking",
    accentHex: "#3DB88E",
    bgHex: "#143A32",
  },
  StubbornBlamer: {
    id: "StubbornBlamer",
    name: "大鹏",
    defaultStyle: "AggressiveFast",
    styleLabel: STYLE_LABELS.AggressiveFast,
    tagline: "京片子嘴贫，死磕不服输",
    initialMood: "Deadpan",
    accentHex: "#4B86E8",
    bgHex: "#182C4C",
  },
  ChaosInstigator: {
    id: "ChaosInstigator",
    name: "小桃",
    defaultStyle: "Balanced",
    styleLabel: STYLE_LABELS.Balanced,
    tagline: "网感损友，吃瓜拱火爱补刀",
    initialMood: "Plotting",
    accentHex: "#F0A830",
    bgHex: "#432813",
  },
  TiltedMaster: {
    id: "TiltedMaster",
    name: "老陈",
    defaultStyle: "DefensiveBigHand",
    styleLabel: STYLE_LABELS.DefensiveBigHand,
    tagline: "爱端前辈架子，吃亏秒破防",
    initialMood: "Smug",
    accentHex: "#C9485B",
    bgHex: "#451924",
  },
};

export const PERSONA_LIST: readonly PersonaMeta[] = PERSONA_IDS.map(
  (id) => PERSONA_CATALOG[id],
);

export const PERSONA_MOOD_CATALOG: Record<PersonaMood, PersonaMoodMeta> = {
  Smug: {
    mood: "Smug",
    label: "得意欠揍",
    shortLabel: "得意",
    badgeBg: "#F5BE58",
    badgeFg: "#1A1610",
  },
  Mocking: {
    mood: "Mocking",
    label: "阴阳嘲讽",
    shortLabel: "阴阳",
    badgeBg: "#49BFA0",
    badgeFg: "#0E211C",
  },
  Tilted: {
    mood: "Tilted",
    label: "红温破防",
    shortLabel: "红温",
    badgeBg: "#E64C3C",
    badgeFg: "#FFF6F2",
  },
  Nervous: {
    mood: "Nervous",
    label: "心虚紧绷",
    shortLabel: "心虚",
    badgeBg: "#6AA4E8",
    badgeFg: "#101B2B",
  },
  Plotting: {
    mood: "Plotting",
    label: "阴险憋大招",
    shortLabel: "憋招",
    badgeBg: "#9C6EE6",
    badgeFg: "#FAF5FF",
  },
  Deadpan: {
    mood: "Deadpan",
    label: "无语装淡定",
    shortLabel: "淡定",
    badgeBg: "#8EA0A8",
    badgeFg: "#141D22",
  },
};

export function getPersonaMeta(id: PersonaId): PersonaMeta {
  return PERSONA_CATALOG[id];
}

export function getPersonaMoodMeta(mood: PersonaMood): PersonaMoodMeta {
  return PERSONA_MOOD_CATALOG[mood];
}

export function aiStyleLabel(style: AiStyle): string {
  return STYLE_LABELS[style];
}

export function personaFrameKey(
  personaId: PersonaId,
  mood: PersonaMood,
): string {
  return `persona_${personaId}_${mood}`;
}

export const QUICK_EMOTE_COOLDOWN_MS = 4000;

export const QUICK_EMOTE_IDS: readonly QuickEmote[] = [
  "IsThatIt",
  "HurryUp",
  "SpareMe",
  "NicePlay",
  "DealerRigged",
  "GoingBig",
] as const;

export interface QuickEmoteMeta {
  readonly id: QuickEmote;
  readonly label: string;
  readonly shortLabel: string;
  readonly accentHex: string;
}

export const QUICK_EMOTE_CATALOG: Record<QuickEmote, QuickEmoteMeta> = {
  IsThatIt: {
    id: "IsThatIt",
    label: "就这？",
    shortLabel: "就这？",
    accentHex: "#F5BE58",
  },
  HurryUp: {
    id: "HurryUp",
    label: "搞快点",
    shortLabel: "搞快点",
    accentHex: "#E85D3F",
  },
  SpareMe: {
    id: "SpareMe",
    label: "别点我 / 求放过",
    shortLabel: "求放过",
    accentHex: "#6AA4E8",
  },
  NicePlay: {
    id: "NicePlay",
    label: "666 / 漂亮",
    shortLabel: "666",
    accentHex: "#49BFA0",
  },
  DealerRigged: {
    id: "DealerRigged",
    label: "什么破手气",
    shortLabel: "破手气",
    accentHex: "#C9485B",
  },
  GoingBig: {
    id: "GoingBig",
    label: "这把我要胡大的",
    shortLabel: "胡大的",
    accentHex: "#9C6EE6",
  },
};

export const QUICK_EMOTE_LIST: readonly QuickEmoteMeta[] = QUICK_EMOTE_IDS.map(
  (id) => QUICK_EMOTE_CATALOG[id],
);

export function getQuickEmoteMeta(emote: QuickEmote): QuickEmoteMeta {
  return QUICK_EMOTE_CATALOG[emote];
}
