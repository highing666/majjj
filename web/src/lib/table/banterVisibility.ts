import type {
  ActiveBanterVoiceState,
  AudioContextStateLike,
} from "../audio/audioTypes";
import type { HumanVoicePackId } from "../audio/voicePackSchema";
import type { DevicePreferences } from "../preferences/devicePreferences";
import type {
  ActiveBanterBubble,
  ActiveBanterMap,
} from "../presentation/presentationQueue";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import {
  getPersonaMeta,
  getPersonaMoodMeta,
  getQuickEmoteMeta,
} from "../protocol/personas";
import {
  ALL_SEATS,
  formatRelativeSeatDisplay,
  seatIndex,
} from "../protocol/seats";
import type { TtsStatusNoticeState } from "../session/reducer";

export interface BanterVisibilityInput {
  readonly preferences: Pick<
    DevicePreferences,
    | "muted"
    | "master_volume"
    | "voice_volume"
    | "banter_voice_enabled"
  >;
  readonly audioState: AudioContextStateLike | "uninitialized";
  readonly ttsStatusNotice: TtsStatusNoticeState | null;
}

/**
 * Voice-first default visibility policy: returns `true` whenever realtime
 * `BanterVoice` is genuinely audible (not muted, master & voice volumes > 0,
 * banter voice enabled, Safari AudioContext running, and LAN TTS service available).
 *
 * Flipping ANY single condition immediately returns `false` so AI text bubbles
 * auto-restore as fallback with zero flicker.
 */
export function shouldHideBanterBubbles(input: BanterVisibilityInput): boolean {
  const { preferences, audioState, ttsStatusNotice } = input;
  if (
    preferences.muted ||
    preferences.master_volume <= 0 ||
    preferences.voice_volume <= 0
  ) {
    return false;
  }
  if (!preferences.banter_voice_enabled) {
    return false;
  }
  if (audioState !== "running") {
    return false;
  }
  if (ttsStatusNotice?.available === false) {
    return false;
  }
  return true;
}

/**
 * Suppresses regular AI `牌桌台词` text bubbles when `hideAiBanterText` is active,
 * while keeping human `快捷互动` bubbles and per-line `audioFallback` bubbles visible.
 */
export function shouldShowBubbleText(
  bubble: ActiveBanterBubble,
  hideAiBanterText: boolean,
): boolean {
  if (bubble.kind === "quick_emote" || bubble.audioFallback === true) {
    return true;
  }
  return !hideAiBanterText;
}

/**
 * Matches whether a specific comic banter bubble is currently speaking its `BanterVoice`.
 */
export function isBubbleActivelySpeaking(
  bubble: ActiveBanterBubble | null,
  activeBanterVoice: ActiveBanterVoiceState | null,
): boolean {
  return (
    bubble !== null &&
    activeBanterVoice !== null &&
    bubble.kind === "banter" &&
    bubble.seat === activeBanterVoice.seat &&
    bubble.banterId !== null &&
    bubble.banterId === activeBanterVoice.banterId
  );
}

/**
 * Matches whether a given seat's avatar/nameplate is currently speaking `BanterVoice`.
 */
export function isSeatActivelySpeaking(
  seat: Seat,
  activeBanterVoice: ActiveBanterVoiceState | null,
): boolean {
  return activeBanterVoice !== null && activeBanterVoice.seat === seat;
}

export interface PostHandBanterStripInput {
  readonly seats: readonly [SeatInfo, SeatInfo, SeatInfo, SeatInfo];
  readonly yourSeat: Seat | null;
  readonly activeBubbles: ActiveBanterMap;
  readonly activeBanterVoice: ActiveBanterVoiceState | null;
  readonly hideAiBanterText: boolean;
}

export interface PostHandBanterStripItem {
  readonly bubble: ActiveBanterBubble;
  readonly speakerName: string;
  readonly replyTargetName: string | null;
  readonly accentHex: string;
  readonly moodShortLabel: string;
  readonly moodBadgeBg: string;
  readonly moodBadgeFg: string;
  readonly speaking: boolean;
  readonly hideText: boolean;
}

/**
 * Builds ordered settlement/summary post-hand banter strip cards.
 * Regular AI cards collapse to compact avatar + mood + speaking pulse when
 * `hideAiBanterText` is active, and expand body text on fallback or human `quick_emote`.
 */
export function buildPostHandBanterStripItems(
  input: PostHandBanterStripInput,
): PostHandBanterStripItem[] {
  const { seats, yourSeat, activeBubbles, activeBanterVoice, hideAiBanterText } =
    input;
  const active: ActiveBanterBubble[] = [];
  for (const seat of ALL_SEATS) {
    const bubble = activeBubbles[seat];
    if (bubble !== null) {
      active.push(bubble);
    }
  }
  active.sort((a, b) => a.bubbleId - b.bubbleId);

  return active.map((bubble) => {
    const seatInfo = seats[seatIndex(bubble.seat)];
    const replyTargetName =
      bubble.replyToSeat !== null
        ? formatRelativeSeatDisplay(bubble.replyToSeat, yourSeat, seats)
        : null;
    const speaking = isBubbleActivelySpeaking(bubble, activeBanterVoice);
    const hideText = !shouldShowBubbleText(bubble, hideAiBanterText);
    if (
      bubble.kind === "banter" &&
      bubble.personaId !== null &&
      bubble.mood !== null
    ) {
      const personaMeta = getPersonaMeta(bubble.personaId);
      const moodMeta = getPersonaMoodMeta(bubble.mood);
      return {
        bubble,
        speakerName: seatInfo.nickname.trim() || personaMeta.name,
        replyTargetName,
        accentHex: personaMeta.accentHex,
        moodShortLabel: moodMeta.shortLabel,
        moodBadgeBg: moodMeta.badgeBg,
        moodBadgeFg: moodMeta.badgeFg,
        speaking,
        hideText,
      };
    }
    const emoteMeta = bubble.emote ? getQuickEmoteMeta(bubble.emote) : null;
    return {
      bubble,
      speakerName: seatInfo.nickname.trim() || "玩家",
      replyTargetName,
      accentHex: emoteMeta?.accentHex ?? "#F5BE58",
      moodShortLabel: "快捷互动",
      moodBadgeBg: emoteMeta?.accentHex ?? "#F5BE58",
      moodBadgeFg: "#181A20",
      speaking: false,
      hideText: false,
    };
  });
}

export interface HumanVoicePackSyncInput {
  readonly preferredPackId: HumanVoicePackId | null;
  readonly currentSeatVoicePackId: string | null;
  readonly isConnected: boolean;
  readonly yourSeat: Seat | null;
  readonly tableId: string | null;
  readonly lastSyncedKey: string | null;
}

export interface HumanVoicePackSyncTarget {
  readonly packId: HumanVoicePackId;
  readonly syncKey: string;
}

export function buildHumanVoicePackSyncKey(
  tableId: string,
  seat: Seat,
  packId: HumanVoicePackId,
): string {
  return `${tableId}:${seat}:${packId}`;
}

/**
 * Resolves whether the device's persisted `human_voice_pack_id` should be synced
 * to the server via `SetSeatVoicePack`. Returns `null` when the user kept default
 * auto-staggering (`null`), is not seated/connected, already matches the seat, or
 * already sent the same sync key.
 */
export function resolveHumanVoicePackSyncTarget(
  input: HumanVoicePackSyncInput,
): HumanVoicePackSyncTarget | null {
  const {
    preferredPackId,
    currentSeatVoicePackId,
    isConnected,
    yourSeat,
    tableId,
    lastSyncedKey,
  } = input;
  if (!isConnected || preferredPackId === null || yourSeat === null || !tableId) {
    return null;
  }
  if (currentSeatVoicePackId === preferredPackId) {
    return null;
  }
  const syncKey = buildHumanVoicePackSyncKey(tableId, yourSeat, preferredPackId);
  if (lastSyncedKey === syncKey) {
    return null;
  }
  return { packId: preferredPackId, syncKey };
}
