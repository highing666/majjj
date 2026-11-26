import type {
  ActiveBanterVoiceState,
  AudioContextStateLike,
} from "../audio/audioTypes";
import type { HumanVoicePackId } from "../audio/voicePackSchema";
import type { DevicePreferences } from "../preferences/devicePreferences";
import type { ActiveBanterBubble } from "../presentation/presentationQueue";
import type { Seat } from "../protocol/generated/Seat";
import type { TtsStatusNoticeState } from "../session/reducer";

export interface BanterVisibilityInput {
  readonly preferences: Pick<
    DevicePreferences,
    | "hide_banter_bubble_with_voice"
    | "muted"
    | "master_volume"
    | "voice_volume"
    | "banter_voice_enabled"
  >;
  readonly audioState: AudioContextStateLike | "uninitialized";
  readonly ttsStatusNotice: TtsStatusNoticeState | null;
}

/**
 * Returns `true` ONLY when the user enabled `hide_banter_bubble_with_voice` AND
 * realtime `BanterVoice` is genuinely audible (not muted, volumes > 0, banter voice
 * enabled, Safari AudioContext running, and LAN TTS service available).
 *
 * Flipping ANY single condition immediately returns `false` so text bubbles
 * auto-restore with zero flicker.
 */
export function shouldHideBanterBubbles(input: BanterVisibilityInput): boolean {
  const { preferences, audioState, ttsStatusNotice } = input;
  if (!preferences.hide_banter_bubble_with_voice) {
    return false;
  }
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
 * while keeping human `快捷互动` bubbles visible.
 */
export function shouldShowBubbleText(
  bubble: ActiveBanterBubble,
  hideAiBanterText: boolean,
): boolean {
  if (bubble.kind === "quick_emote") {
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
