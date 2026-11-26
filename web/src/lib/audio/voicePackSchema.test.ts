import { describe, expect, it } from "vitest";
import { PERSONA_IDS, QUICK_EMOTE_IDS } from "../protocol/personas";
import {
  AI_VOICE_PACK_IDS,
  ALL_VOICE_PACK_IDS,
  ALL_VOICE_SLOTS,
  HUMAN_VOICE_PACK_CATALOG,
  HUMAN_VOICE_PACK_IDS,
  VOICE_ACTION_SLOTS,
  VOICE_EMOTE_SLOTS,
  VOICE_SLOT_SPEECH_TEXT,
  VOICE_TILE_SLOTS,
  isHumanVoicePackId,
  isVoicePackId,
  meldKindToVoiceSlot,
  personaToVoicePackId,
  quickEmoteToVoiceSlot,
  tileToVoiceSlot,
  voicePackManifestUrl,
  winKindToVoiceSlot,
} from "./voicePackSchema";

describe("voicePackSchema (canonical 7-pack / 46-slot VoicePack contract)", () => {
  it("defines 7 distinct canonical VoicePack IDs (5 AI + 2 human) matching Rust PersonaId", () => {
    expect(AI_VOICE_PACK_IDS).toEqual([
      "quick_hothead",
      "tea_sarcasm",
      "stubborn_blamer",
      "chaos_instigator",
      "tilted_master",
    ]);
    expect(HUMAN_VOICE_PACK_IDS).toEqual(["human_male", "human_female"]);
    expect(ALL_VOICE_PACK_IDS).toHaveLength(7);
    expect(new Set(ALL_VOICE_PACK_IDS).size).toBe(7);

    expect(PERSONA_IDS.map(personaToVoicePackId)).toEqual(AI_VOICE_PACK_IDS);

    expect(isHumanVoicePackId("human_male")).toBe(true);
    expect(isHumanVoicePackId("human_female")).toBe(true);
    expect(isHumanVoicePackId("quick_hothead")).toBe(false);
    expect(isHumanVoicePackId(null)).toBe(false);

    expect(isVoicePackId("quick_hothead")).toBe(true);
    expect(isVoicePackId("human_female")).toBe(true);
    expect(isVoicePackId("unknown")).toBe(false);

    expect(HUMAN_VOICE_PACK_CATALOG.human_male.label).toBe("清亮男声");
    expect(HUMAN_VOICE_PACK_CATALOG.human_female.label).toBe("利落女声");
    expect(voicePackManifestUrl("quick_hothead")).toBe(
      "/assets/vo/quick_hothead/manifest.json",
    );
  });

  it("defines exact 46 canonical slots: 34 tiles + 6 action calls + 6 quick emotes with spoken Chinese text", () => {
    expect(VOICE_TILE_SLOTS).toHaveLength(34);
    expect(VOICE_ACTION_SLOTS).toEqual([
      "pong",
      "kong",
      "concealed_kong",
      "zimo",
      "ron",
      "rob_kong",
    ]);
    expect(VOICE_EMOTE_SLOTS).toEqual([
      "emote_is_that_it",
      "emote_hurry_up",
      "emote_spare_me",
      "emote_nice_play",
      "emote_dealer_rigged",
      "emote_going_big",
    ]);
    expect(ALL_VOICE_SLOTS).toHaveLength(46);
    expect(new Set(ALL_VOICE_SLOTS).size).toBe(46);

    // Every canonical slot has non-empty spoken Chinese text for offline generation (#84).
    for (const slot of ALL_VOICE_SLOTS) {
      expect(VOICE_SLOT_SPEECH_TEXT[slot]).toBeTruthy();
      expect(VOICE_SLOT_SPEECH_TEXT[slot].trim().length).toBeGreaterThan(0);
    }

    // Spot-check spoken tile names including full honor names (东风/红中/白板).
    expect(VOICE_SLOT_SPEECH_TEXT.m1).toBe("一万");
    expect(VOICE_SLOT_SPEECH_TEXT.m9).toBe("九万");
    expect(VOICE_SLOT_SPEECH_TEXT.s1).toBe("一条");
    expect(VOICE_SLOT_SPEECH_TEXT.s9).toBe("九条");
    expect(VOICE_SLOT_SPEECH_TEXT.p1).toBe("一筒");
    expect(VOICE_SLOT_SPEECH_TEXT.p9).toBe("九筒");
    expect(VOICE_SLOT_SPEECH_TEXT.z1).toBe("东风");
    expect(VOICE_SLOT_SPEECH_TEXT.z2).toBe("南风");
    expect(VOICE_SLOT_SPEECH_TEXT.z3).toBe("西风");
    expect(VOICE_SLOT_SPEECH_TEXT.z4).toBe("北风");
    expect(VOICE_SLOT_SPEECH_TEXT.z5).toBe("红中");
    expect(VOICE_SLOT_SPEECH_TEXT.z6).toBe("发财");
    expect(VOICE_SLOT_SPEECH_TEXT.z7).toBe("白板");
  });

  it("maps Tile (0..33), MeldKind, WinKind, and QuickEmote bijectively to canonical VoiceSlotId", () => {
    const mappedTiles = Array.from({ length: 34 }, (_, i) => tileToVoiceSlot(i));
    expect(mappedTiles).toEqual(VOICE_TILE_SLOTS);
    expect(tileToVoiceSlot(0)).toBe("m1");
    expect(tileToVoiceSlot(8)).toBe("m9");
    expect(tileToVoiceSlot(9)).toBe("s1");
    expect(tileToVoiceSlot(17)).toBe("s9");
    expect(tileToVoiceSlot(18)).toBe("p1");
    expect(tileToVoiceSlot(26)).toBe("p9");
    expect(tileToVoiceSlot(27)).toBe("z1");
    expect(tileToVoiceSlot(31)).toBe("z5");
    expect(tileToVoiceSlot(33)).toBe("z7");
    expect(tileToVoiceSlot(-1)).toBeNull();
    expect(tileToVoiceSlot(34)).toBeNull();

    expect(meldKindToVoiceSlot("Pon")).toBe("pong");
    expect(meldKindToVoiceSlot("ExposedKong")).toBe("kong");
    expect(meldKindToVoiceSlot("AddedKong")).toBe("kong");
    expect(meldKindToVoiceSlot("ConcealedKong")).toBe("concealed_kong");
    expect(meldKindToVoiceSlot("Chi")).toBeNull();

    expect(winKindToVoiceSlot("Tsumo")).toBe("zimo");
    expect(winKindToVoiceSlot("Ron")).toBe("ron");
    expect(winKindToVoiceSlot("RobKongHu")).toBe("rob_kong");

    expect(QUICK_EMOTE_IDS.map(quickEmoteToVoiceSlot)).toEqual(VOICE_EMOTE_SLOTS);
  });
});
