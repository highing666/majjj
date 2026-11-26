import { describe, expect, it } from "vitest";
import { defaultDevicePreferences } from "../preferences/devicePreferences";
import type { ActiveBanterBubble } from "../presentation/presentationQueue";
import {
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

describe("banterVisibility", () => {
  it("hides AI banter text ONLY when hide_banter_bubble_with_voice is on, audible, banter voice enabled, AudioContext running, and TTS available", () => {
    const basePrefs = {
      ...defaultDevicePreferences(false),
      hide_banter_bubble_with_voice: true,
      muted: false,
      master_volume: 0.85,
      voice_volume: 1,
      banter_voice_enabled: true,
    };

    // All 5 conditions satisfied -> hide regular AI banter text bubbles
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

    // 1. Toggle off -> show bubbles
    expect(
      shouldHideBanterBubbles({
        preferences: { ...basePrefs, hide_banter_bubble_with_voice: false },
        audioState: "running",
        ttsStatusNotice: null,
      }),
    ).toBe(false);

    // 2. Device muted or master_volume === 0 or voice_volume === 0 -> auto-fallback to visible bubbles
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

    // 3. banter_voice_enabled === false -> auto-fallback to visible bubbles
    expect(
      shouldHideBanterBubbles({
        preferences: { ...basePrefs, banter_voice_enabled: false },
        audioState: "running",
        ttsStatusNotice: null,
      }),
    ).toBe(false);

    // 4. Safari AudioContext not running (uninitialized / suspended / interrupted / closed) -> auto-fallback
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

    // 5. TTS service degraded -> auto-fallback to visible bubbles
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
  });

  it("suppresses only AI banter text bubbles when hidden while keeping human quick_emote bubbles visible", () => {
    const aiBubble = makeAiBubble();
    const emoteBubble = makeQuickEmoteBubble();

    expect(shouldShowBubbleText(aiBubble, false)).toBe(true);
    expect(shouldShowBubbleText(aiBubble, true)).toBe(false);
    expect(shouldShowBubbleText(emoteBubble, false)).toBe(true);
    expect(shouldShowBubbleText(emoteBubble, true)).toBe(true);
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
