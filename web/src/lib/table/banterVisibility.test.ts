import { describe, expect, it } from "vitest";
import { defaultDevicePreferences } from "../preferences/devicePreferences";
import type {
  ActiveBanterBubble,
  ActiveBanterMap,
} from "../presentation/presentationQueue";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import {
  buildPostHandBanterStripItems,
  isBubbleActivelySpeaking,
  isSeatActivelySpeaking,
  resolveHumanVoicePackSyncTarget,
  shouldHideBanterBubbles,
  shouldShowBubbleText,
} from "./banterVisibility";

function makeAiBubble(overrides: Partial<ActiveBanterBubble> = {}): ActiveBanterBubble {
  return {
    kind: "banter",
    bubbleId: 1,
    banterId: "b-1",
    priority: "normal",
    seat: "South",
    personaId: "QuickHothead",
    mood: "Tilted",
    emote: null,
    text: "这手怎么老摸不来！",
    eventSeq: 10,
    replyToSeat: null,
    ...overrides,
  };
}

function makeQuickEmoteBubble(
  overrides: Partial<ActiveBanterBubble> = {},
): ActiveBanterBubble {
  return {
    kind: "quick_emote",
    bubbleId: 2,
    banterId: null,
    priority: "normal",
    seat: "East",
    personaId: null,
    mood: null,
    emote: "IsThatIt",
    text: "就这？",
    eventSeq: 11,
    replyToSeat: null,
    ...overrides,
  };
}

function makeSeats(): [SeatInfo, SeatInfo, SeatInfo, SeatInfo] {
  return [
    {
      seat: "East",
      kind: "Human",
      nickname: "小明",
      ai_difficulty: null,
      ai_style: null,
      persona_id: null,
      mood: null,
      connected: true,
      ai_assisted: false,
      voice_pack_id: "human_male",
    },
    {
      seat: "South",
      kind: "Ai",
      nickname: "大川",
      ai_difficulty: "Strong",
      ai_style: "AggressiveFast",
      persona_id: "QuickHothead",
      mood: "Tilted",
      connected: true,
      ai_assisted: false,
      voice_pack_id: "quick_hothead",
    },
    {
      seat: "West",
      kind: "Ai",
      nickname: "小晚",
      ai_difficulty: "Strong",
      ai_style: "Balanced",
      persona_id: "TeaSarcasm",
      mood: "Mocking",
      connected: true,
      ai_assisted: false,
      voice_pack_id: "tea_sarcasm",
    },
    {
      seat: "North",
      kind: "Ai",
      nickname: "老周",
      ai_difficulty: "Strong",
      ai_style: "DefensiveBigHand",
      persona_id: "StubbornBlamer",
      mood: "Deadpan",
      connected: true,
      ai_assisted: false,
      voice_pack_id: "stubborn_blamer",
    },
  ];
}

describe("banterVisibility", () => {
  it("hides AI banter text by default whenever audible, banter voice enabled, AudioContext running, and TTS available, and restores immediately on any fallback condition", () => {
    const basePrefs = defaultDevicePreferences(false);

    // Default audible state -> hide regular AI banter text bubbles (voice-first)
    expect(
      shouldHideBanterBubbles({
        preferences: basePrefs,
        audioState: "running",
        ttsStatusNotice: null,
      }),
    ).toBe(true);
    expect(
      shouldHideBanterBubbles({
        preferences: basePrefs,
        audioState: "running",
        ttsStatusNotice: { available: true, message: "语音服务已恢复" },
      }),
    ).toBe(true);

    // 1. Device muted or master_volume <= 0 or voice_volume <= 0 -> auto-fallback to visible bubbles
    expect(
      shouldHideBanterBubbles({
        preferences: { ...basePrefs, muted: true },
        audioState: "running",
        ttsStatusNotice: null,
      }),
    ).toBe(false);
    expect(
      shouldHideBanterBubbles({
        preferences: { ...basePrefs, master_volume: 0 },
        audioState: "running",
        ttsStatusNotice: null,
      }),
    ).toBe(false);
    expect(
      shouldHideBanterBubbles({
        preferences: { ...basePrefs, voice_volume: 0 },
        audioState: "running",
        ttsStatusNotice: null,
      }),
    ).toBe(false);

    // 2. banter_voice_enabled === false -> auto-fallback to visible bubbles
    expect(
      shouldHideBanterBubbles({
        preferences: { ...basePrefs, banter_voice_enabled: false },
        audioState: "running",
        ttsStatusNotice: null,
      }),
    ).toBe(false);

    // 3. Safari AudioContext not running (uninitialized / suspended / interrupted / closed) -> auto-fallback
    for (const audioState of [
      "uninitialized",
      "suspended",
      "interrupted",
      "closed",
    ] as const) {
      expect(
        shouldHideBanterBubbles({
          preferences: basePrefs,
          audioState,
          ttsStatusNotice: null,
        }),
        `audioState=${audioState} must auto-restore bubbles`,
      ).toBe(false);
    }

    // 4. TTS service degraded or --no-tts -> auto-fallback to visible bubbles
    expect(
      shouldHideBanterBubbles({
        preferences: basePrefs,
        audioState: "running",
        ttsStatusNotice: {
          available: false,
          message: "局域网语音服务暂时不可用",
        },
      }),
    ).toBe(false);

    // 5. Returning to normal audible state immediately restores hidden mode
    expect(
      shouldHideBanterBubbles({
        preferences: { ...basePrefs, muted: false, banter_voice_enabled: true },
        audioState: "running",
        ttsStatusNotice: { available: true, message: "语音服务已恢复" },
      }),
    ).toBe(true);
  });

  it("suppresses only AI banter text bubbles when hidden while keeping human quick_emote and audioFallback bubbles visible", () => {
    const aiBubble = makeAiBubble();
    const fallbackAiBubble = makeAiBubble({ banterId: "b-2", audioFallback: true });
    const emoteBubble = makeQuickEmoteBubble();

    expect(shouldShowBubbleText(aiBubble, false)).toBe(true);
    expect(shouldShowBubbleText(aiBubble, true)).toBe(false);
    expect(shouldShowBubbleText(fallbackAiBubble, false)).toBe(true);
    expect(shouldShowBubbleText(fallbackAiBubble, true)).toBe(true);
    expect(shouldShowBubbleText(emoteBubble, false)).toBe(true);
    expect(shouldShowBubbleText(emoteBubble, true)).toBe(true);
  });

  it("renders settlement/summary PostHandBanterStrip cards compactly when audible and expands text on fallback, audioFallback, or human quick_emote", () => {
    const seats = makeSeats();
    const aiRegular = makeAiBubble({
      bubbleId: 1,
      seat: "South",
      banterId: "b-settle-1",
      mood: "Tilted",
      text: "这手点炮点得我心疼！",
    });
    const aiFallback = makeAiBubble({
      bubbleId: 2,
      seat: "West",
      banterId: "b-settle-2",
      personaId: "TeaSarcasm",
      mood: "Mocking",
      text: "哟，这都能胡？",
      audioFallback: true,
      replyToSeat: "South",
    });
    const humanEmote = makeQuickEmoteBubble({
      bubbleId: 3,
      seat: "East",
      text: "就这？",
    });

    const activeBubbles: ActiveBanterMap = {
      East: humanEmote,
      South: aiRegular,
      West: aiFallback,
      North: null,
    };

    // Audible (hideAiBanterText=true): regular AI is compact with speaking pulse;
    // per-line audioFallback and human quick_emote expand text.
    const audibleItems = buildPostHandBanterStripItems({
      seats,
      yourSeat: "East",
      activeBubbles,
      activeBanterVoice: { seat: "South", banterId: "b-settle-1" },
      hideAiBanterText: true,
    });
    expect(audibleItems).toHaveLength(3);
    expect(audibleItems[0]).toMatchObject({
      speakerName: "大川",
      speaking: true,
      hideText: true,
      replyTargetName: null,
    });
    expect(audibleItems[1]).toMatchObject({
      speakerName: "小晚",
      speaking: false,
      hideText: false,
      replyTargetName: "下家 · 大川",
    });
    expect(audibleItems[2]).toMatchObject({
      speakerName: "小明",
      speaking: false,
      hideText: false,
      moodShortLabel: "快捷互动",
    });

    // Silent/fallback (hideAiBanterText=false): regular AI card automatically expands text
    const fallbackItems = buildPostHandBanterStripItems({
      seats,
      yourSeat: "East",
      activeBubbles,
      activeBanterVoice: null,
      hideAiBanterText: false,
    });
    expect(fallbackItems[0]).toMatchObject({
      speakerName: "大川",
      speaking: false,
      hideText: false,
    });
    expect(fallbackItems[1]?.hideText).toBe(false);
    expect(fallbackItems[2]?.hideText).toBe(false);
  });

  it("matches active speaking state strictly by seat and banterId for bubbles and by seat for avatar pockets", () => {
    const bubble = makeAiBubble({ seat: "South", banterId: "b-42" });
    const emoteBubble = makeQuickEmoteBubble({ seat: "South" });

    expect(
      isBubbleActivelySpeaking(bubble, { seat: "South", banterId: "b-42" }),
    ).toBe(true);
    expect(
      isBubbleActivelySpeaking(bubble, { seat: "South", banterId: "b-99" }),
    ).toBe(false);
    expect(
      isBubbleActivelySpeaking(bubble, { seat: "West", banterId: "b-42" }),
    ).toBe(false);
    expect(isBubbleActivelySpeaking(bubble, null)).toBe(false);
    expect(
      isBubbleActivelySpeaking(emoteBubble, { seat: "South", banterId: "b-42" }),
    ).toBe(false);

    expect(
      isSeatActivelySpeaking("South", { seat: "South", banterId: "b-42" }),
    ).toBe(true);
    expect(
      isSeatActivelySpeaking("West", { seat: "South", banterId: "b-42" }),
    ).toBe(false);
    expect(isSeatActivelySpeaking("South", null)).toBe(false);
  });

  it("resolves preferred human voice pack sync target only when seated, connected, and differing from current seat pack", () => {
    // Default null preference preserves server auto-staggering
    expect(
      resolveHumanVoicePackSyncTarget({
        preferredPackId: null,
        currentSeatVoicePackId: "human_male",
        isConnected: true,
        yourSeat: "East",
        tableId: "table-1",
        lastSyncedKey: null,
      }),
    ).toBeNull();

    // Explicit preference differing from server seat assignment -> syncs with dedupe key
    expect(
      resolveHumanVoicePackSyncTarget({
        preferredPackId: "human_female",
        currentSeatVoicePackId: "human_male",
        isConnected: true,
        yourSeat: "East",
        tableId: "table-1",
        lastSyncedKey: null,
      }),
    ).toEqual({
      packId: "human_female",
      syncKey: "table-1:East:human_female",
    });

    // Already matching current seat pack -> no redundant send
    expect(
      resolveHumanVoicePackSyncTarget({
        preferredPackId: "human_female",
        currentSeatVoicePackId: "human_female",
        isConnected: true,
        yourSeat: "East",
        tableId: "table-1",
        lastSyncedKey: null,
      }),
    ).toBeNull();

    // Already sent for same (table, seat, pack) -> no loop while waiting for server echo
    expect(
      resolveHumanVoicePackSyncTarget({
        preferredPackId: "human_female",
        currentSeatVoicePackId: "human_male",
        isConnected: true,
        yourSeat: "East",
        tableId: "table-1",
        lastSyncedKey: "table-1:East:human_female",
      }),
    ).toBeNull();

    // Disconnected or unseated -> skip sync
    expect(
      resolveHumanVoicePackSyncTarget({
        preferredPackId: "human_female",
        currentSeatVoicePackId: "human_male",
        isConnected: false,
        yourSeat: "East",
        tableId: "table-1",
        lastSyncedKey: null,
      }),
    ).toBeNull();
    expect(
      resolveHumanVoicePackSyncTarget({
        preferredPackId: "human_female",
        currentSeatVoicePackId: null,
        isConnected: true,
        yourSeat: null,
        tableId: "table-1",
        lastSyncedKey: null,
      }),
    ).toBeNull();
  });
});
