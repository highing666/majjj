import type { PersonaId } from "../protocol/generated/PersonaId";
import type { QuickEmote } from "../protocol/generated/QuickEmote";
import type { Tile } from "../protocol/generated/Tile";
import type { WinKind } from "../protocol/generated/WinKind";
import type { MeldKind } from "../protocol/melds";

/**
 * Five canonical AI `VoicePack` (`角色语音包`) identifiers mapped 1:1 from `PersonaId`.
 */
export const AI_VOICE_PACK_IDS = [
  "quick_hothead",
  "tea_sarcasm",
  "stubborn_blamer",
  "chaos_instigator",
  "tilted_master",
] as const;

export type AiVoicePackId = (typeof AI_VOICE_PACK_IDS)[number];

/**
 * Two canonical selectable human `VoicePack` (`角色语音包`) identifiers.
 */
export const HUMAN_VOICE_PACK_IDS = ["human_male", "human_female"] as const;

export type HumanVoicePackId = (typeof HUMAN_VOICE_PACK_IDS)[number];

/**
 * All seven built-in offline `VoicePack` (`角色语音包`) identifiers.
 */
export const ALL_VOICE_PACK_IDS = [
  ...AI_VOICE_PACK_IDS,
  ...HUMAN_VOICE_PACK_IDS,
] as const;

export type VoicePackId = (typeof ALL_VOICE_PACK_IDS)[number];

export interface HumanVoicePackMeta {
  readonly id: HumanVoicePackId;
  readonly label: string;
  readonly shortLabel: string;
  readonly description: string;
}

export const HUMAN_VOICE_PACK_CATALOG: Record<
  HumanVoicePackId,
  HumanVoicePackMeta
> = {
  human_male: {
    id: "human_male",
    label: "清亮男声",
    shortLabel: "男声",
    description: "清亮爽朗青年男声",
  },
  human_female: {
    id: "human_female",
    label: "利落女声",
    shortLabel: "女声",
    description: "利落清脆青年女声",
  },
};

export const HUMAN_VOICE_PACK_LIST: readonly HumanVoicePackMeta[] =
  HUMAN_VOICE_PACK_IDS.map((id) => HUMAN_VOICE_PACK_CATALOG[id]);

export const PERSONA_VOICE_PACK_ID: Record<PersonaId, AiVoicePackId> = {
  QuickHothead: "quick_hothead",
  TeaSarcasm: "tea_sarcasm",
  StubbornBlamer: "stubborn_blamer",
  ChaosInstigator: "chaos_instigator",
  TiltedMaster: "tilted_master",
};

/**
 * Canonical 34 standard mahjong tile slots ordered identically to `Tile` (`0..=33`):
 * `m1..m9` (0..8 万), `s1..s9` (9..17 条), `p1..p9` (18..26 筒), `z1..z7` (27..33 东南西北中发白).
 */
export const VOICE_TILE_SLOTS = [
  "m1",
  "m2",
  "m3",
  "m4",
  "m5",
  "m6",
  "m7",
  "m8",
  "m9",
  "s1",
  "s2",
  "s3",
  "s4",
  "s5",
  "s6",
  "s7",
  "s8",
  "s9",
  "p1",
  "p2",
  "p3",
  "p4",
  "p5",
  "p6",
  "p7",
  "p8",
  "p9",
  "z1",
  "z2",
  "z3",
  "z4",
  "z5",
  "z6",
  "z7",
] as const;

export type VoiceTileSlotId = (typeof VOICE_TILE_SLOTS)[number];

/**
 * Canonical 6 meld & win action call slots.
 */
export const VOICE_ACTION_SLOTS = [
  "pong",
  "kong",
  "concealed_kong",
  "zimo",
  "ron",
  "rob_kong",
] as const;

export type VoiceActionSlotId = (typeof VOICE_ACTION_SLOTS)[number];

/**
 * Canonical 6 human quick-emote (`快捷互动`) slots matching `QuickEmote`.
 */
export const VOICE_EMOTE_SLOTS = [
  "emote_is_that_it",
  "emote_hurry_up",
  "emote_spare_me",
  "emote_nice_play",
  "emote_dealer_rigged",
  "emote_going_big",
] as const;

export type VoiceEmoteSlotId = (typeof VOICE_EMOTE_SLOTS)[number];

/**
 * All 46 canonical `VoicePack` AudioSprite slot identifiers
 * (34 tiles + 6 action calls + 6 quick emotes).
 */
export const ALL_VOICE_SLOTS = [
  ...VOICE_TILE_SLOTS,
  ...VOICE_ACTION_SLOTS,
  ...VOICE_EMOTE_SLOTS,
] as const;

export type VoiceSlotId = (typeof ALL_VOICE_SLOTS)[number];

/**
 * Authoritative spoken Chinese text for every canonical `VoiceSlotId`
 * (single-sourced for offline VoicePack generation & verification).
 */
export const VOICE_SLOT_SPEECH_TEXT: Record<VoiceSlotId, string> = {
  m1: "一万",
  m2: "二万",
  m3: "三万",
  m4: "四万",
  m5: "五万",
  m6: "六万",
  m7: "七万",
  m8: "八万",
  m9: "九万",
  s1: "一条",
  s2: "二条",
  s3: "三条",
  s4: "四条",
  s5: "五条",
  s6: "六条",
  s7: "七条",
  s8: "八条",
  s9: "九条",
  p1: "一筒",
  p2: "二筒",
  p3: "三筒",
  p4: "四筒",
  p5: "五筒",
  p6: "六筒",
  p7: "七筒",
  p8: "八筒",
  p9: "九筒",
  z1: "东风",
  z2: "南风",
  z3: "西风",
  z4: "北风",
  z5: "红中",
  z6: "发财",
  z7: "白板",
  pong: "碰！",
  kong: "杠！",
  concealed_kong: "暗杠！",
  zimo: "自摸！",
  ron: "胡了！",
  rob_kong: "抢杠胡！",
  emote_is_that_it: "就这？",
  emote_hurry_up: "搞快点",
  emote_spare_me: "别点我，求放过",
  emote_nice_play: "666，漂亮",
  emote_dealer_rigged: "什么破手气",
  emote_going_big: "这把我要胡大的",
};

/**
 * Single AudioSprite slice `[startSec, durationSec]` in seconds.
 */
export type VoiceSpriteSlice = readonly [
  startSec: number,
  durationSec: number,
];

/**
 * Runtime manifest schema served at `/assets/vo/<voice_pack_id>/manifest.json`.
 */
export interface VoicePackManifest {
  readonly pack_id: string;
  readonly version: number;
  readonly audio: string;
  readonly sample_rate: number;
  readonly duration: number;
  readonly sprites: Partial<Record<VoiceSlotId, VoiceSpriteSlice>>;
}

const HUMAN_VOICE_PACK_SET: ReadonlySet<string> = new Set(HUMAN_VOICE_PACK_IDS);
const ALL_VOICE_PACK_SET: ReadonlySet<string> = new Set(ALL_VOICE_PACK_IDS);
const ALL_VOICE_SLOT_SET: ReadonlySet<string> = new Set(ALL_VOICE_SLOTS);

export function isVoiceSlotId(value: string): value is VoiceSlotId {
  return ALL_VOICE_SLOT_SET.has(value);
}

export function isValidVoiceSlice(slice: unknown): slice is VoiceSpriteSlice {
  return (
    Array.isArray(slice) &&
    slice.length === 2 &&
    typeof slice[0] === "number" &&
    typeof slice[1] === "number" &&
    Number.isFinite(slice[0]) &&
    Number.isFinite(slice[1]) &&
    slice[0] >= 0 &&
    slice[1] > 0
  );
}

/**
 * Validates and normalizes an untrusted JSON object against the runtime
 * `VoicePackManifest` contract. Returns `null` if any required field or
 * sprite slice entry is malformed.
 */
export function parseVoicePackManifest(raw: unknown): VoicePackManifest | null {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    return null;
  }
  const obj = raw as Record<string, unknown>;
  const packId = typeof obj.pack_id === "string" ? obj.pack_id.trim() : "";
  if (packId.length === 0) {
    return null;
  }
  const version = obj.version;
  if (typeof version !== "number" || !Number.isInteger(version) || version < 1) {
    return null;
  }
  const audio = typeof obj.audio === "string" ? obj.audio.trim() : "";
  if (audio.length === 0) {
    return null;
  }
  const sampleRate = obj.sample_rate;
  if (typeof sampleRate !== "number" || !Number.isFinite(sampleRate) || sampleRate <= 0) {
    return null;
  }
  const duration = obj.duration;
  if (typeof duration !== "number" || !Number.isFinite(duration) || duration <= 0) {
    return null;
  }
  const rawSprites = obj.sprites;
  if (!rawSprites || typeof rawSprites !== "object" || Array.isArray(rawSprites)) {
    return null;
  }

  const sprites: Partial<Record<VoiceSlotId, VoiceSpriteSlice>> = {};
  for (const [key, value] of Object.entries(rawSprites as Record<string, unknown>)) {
    if (!isVoiceSlotId(key)) {
      return null;
    }
    if (!isValidVoiceSlice(value)) {
      return null;
    }
    sprites[key] = [value[0], value[1]];
  }

  return {
    pack_id: packId,
    version,
    audio,
    sample_rate: sampleRate,
    duration,
    sprites,
  };
}

const QUICK_EMOTE_SLOT_MAP: Record<QuickEmote, VoiceEmoteSlotId> = {
  IsThatIt: "emote_is_that_it",
  HurryUp: "emote_hurry_up",
  SpareMe: "emote_spare_me",
  NicePlay: "emote_nice_play",
  DealerRigged: "emote_dealer_rigged",
  GoingBig: "emote_going_big",
};

export function isHumanVoicePackId(
  value: string | null | undefined,
): value is HumanVoicePackId {
  return typeof value === "string" && HUMAN_VOICE_PACK_SET.has(value);
}

export function isVoicePackId(
  value: string | null | undefined,
): value is VoicePackId {
  return typeof value === "string" && ALL_VOICE_PACK_SET.has(value);
}

export function personaToVoicePackId(personaId: PersonaId): AiVoicePackId {
  return PERSONA_VOICE_PACK_ID[personaId];
}

export function voicePackManifestUrl(packId: string): string {
  return `/assets/vo/${encodeURIComponent(packId)}/manifest.json`;
}

export function tileToVoiceSlot(tile: Tile): VoiceTileSlotId | null {
  if (!Number.isInteger(tile) || tile < 0 || tile >= VOICE_TILE_SLOTS.length) {
    return null;
  }
  return VOICE_TILE_SLOTS[tile] ?? null;
}

export function meldKindToVoiceSlot(kind: MeldKind): VoiceActionSlotId | null {
  switch (kind) {
    case "Pon":
      return "pong";
    case "ExposedKong":
    case "AddedKong":
      return "kong";
    case "ConcealedKong":
      return "concealed_kong";
    case "Chi":
      return null;
  }
}

export function winKindToVoiceSlot(kind: WinKind): VoiceActionSlotId {
  switch (kind) {
    case "Tsumo":
      return "zimo";
    case "Ron":
      return "ron";
    case "RobKongHu":
      return "rob_kong";
  }
}

export function quickEmoteToVoiceSlot(emote: QuickEmote): VoiceEmoteSlotId {
  return QUICK_EMOTE_SLOT_MAP[emote];
}

export function getHumanVoicePackMeta(
  packId: string | null | undefined,
): HumanVoicePackMeta | null {
  if (!isHumanVoicePackId(packId)) {
    return null;
  }
  return HUMAN_VOICE_PACK_CATALOG[packId];
}
