import type { IosAudioSessionMode } from "../preferences/devicePreferences";
import type { RelativeRole } from "../presentation/semanticEvents";
import type { Seat } from "../protocol/generated/Seat";
import type {
  VoicePackManifest,
  VoiceSlotId,
  VoiceSpriteSlice,
} from "./voicePackSchema";

export type { VoicePackManifest, VoiceSlotId, VoiceSpriteSlice };

export type SfxSlotId =
  | "hover"
  | "select"
  | "action_prompt"
  | "pass"
  | "draw"
  | "discard_light"
  | "discard_heavy"
  | "meld_pon"
  | "meld_kong"
  | "added_kong_suspense"
  | "win_tier_1"
  | "win_tier_2"
  | "win_tier_3"
  | "draw_game"
  | "confirm";

export const ALL_SFX_SLOTS: readonly SfxSlotId[] = [
  "hover",
  "select",
  "action_prompt",
  "pass",
  "draw",
  "discard_light",
  "discard_heavy",
  "meld_pon",
  "meld_kong",
  "added_kong_suspense",
  "win_tier_1",
  "win_tier_2",
  "win_tier_3",
  "draw_game",
  "confirm",
];

export type SfxSpriteClip = readonly [startSec: number, durationSec: number];

export interface SfxSpriteManifest {
  readonly version: number;
  readonly sampleRate: number;
  readonly duration: number;
  readonly slots: Record<SfxSlotId, readonly SfxSpriteClip[]>;
}

export type AudioContextStateLike =
  | "suspended"
  | "running"
  | "closed"
  | "interrupted";

export interface AudioParamLike {
  value: number;
  setValueAtTime(value: number, startTime: number): unknown;
  linearRampToValueAtTime(value: number, endTime: number): unknown;
  exponentialRampToValueAtTime(value: number, endTime: number): unknown;
  setTargetAtTime(
    target: number,
    startTime: number,
    timeConstant: number,
  ): unknown;
  cancelScheduledValues?(cancelTime: number): unknown;
}

export interface AudioNodeLike {
  connect(destination: AudioNodeLike): AudioNodeLike | void;
  disconnect?(): void;
}

export interface GainNodeLike extends AudioNodeLike {
  readonly gain: AudioParamLike;
}

export interface StereoPannerNodeLike extends AudioNodeLike {
  readonly pan: AudioParamLike;
}

export interface DynamicsCompressorNodeLike extends AudioNodeLike {
  readonly threshold: AudioParamLike;
  readonly knee: AudioParamLike;
  readonly ratio: AudioParamLike;
  readonly attack: AudioParamLike;
  readonly release: AudioParamLike;
}

export interface AudioBufferLike {
  readonly sampleRate: number;
  readonly length: number;
  readonly duration: number;
  getChannelData(channel: number): Float32Array;
}

export interface AudioScheduledSourceNodeLike extends AudioNodeLike {
  start(when?: number, offset?: number, duration?: number): void;
  stop(when?: number): void;
  onended: (() => void) | null;
}

export interface AudioBufferSourceNodeLike extends AudioScheduledSourceNodeLike {
  buffer: AudioBufferLike | null;
  readonly playbackRate: AudioParamLike;
}

export interface OscillatorNodeLike extends AudioScheduledSourceNodeLike {
  type: OscillatorType;
  readonly frequency: AudioParamLike;
}

export interface BiquadFilterNodeLike extends AudioNodeLike {
  type: BiquadFilterType;
  readonly frequency: AudioParamLike;
  readonly Q: AudioParamLike;
}

export interface AudioContextLike {
  readonly state: AudioContextStateLike;
  readonly currentTime: number;
  readonly sampleRate: number;
  readonly destination: AudioNodeLike;
  resume(): Promise<void>;
  close?(): Promise<void>;
  createGain(): GainNodeLike;
  createDynamicsCompressor(): DynamicsCompressorNodeLike;
  createStereoPanner?(): StereoPannerNodeLike;
  createBuffer(
    numberOfChannels: number,
    length: number,
    sampleRate: number,
  ): AudioBufferLike;
  createBufferSource(): AudioBufferSourceNodeLike;
  createOscillator(): OscillatorNodeLike;
  createBiquadFilter(): BiquadFilterNodeLike;
  decodeAudioData(audioData: ArrayBuffer): Promise<AudioBufferLike>;
  addEventListener(type: "statechange", listener: () => void): void;
  removeEventListener(type: "statechange", listener: () => void): void;
}

export interface AudioSessionLike {
  type: string;
}

export type VoActionCategory =
  | "Hu"
  | "RobKong"
  | "Kong"
  | "Pon"
  | "QuickEmote"
  | "Chi"
  | "Discard";

export type VoManifest = VoicePackManifest;

export interface VoTriggerRequest {
  readonly seat: Seat;
  readonly packId: string;
  readonly category: VoActionCategory;
  readonly slot: VoiceSlotId;
}

export type BanterVoicePriority = "highlight" | "normal";

export interface BanterVoiceRequest {
  readonly banterId: string;
  readonly seat: Seat;
  readonly audioUrl: string;
  readonly durationMs: number;
  readonly priority: BanterVoicePriority;
  readonly relativeRole: RelativeRole | null;
}

export interface ActiveBanterVoiceState {
  readonly seat: Seat;
  readonly banterId: string;
}

export type BanterVoiceStateListener = (
  active: ActiveBanterVoiceState | null,
) => void;

export type SfxBackendMode = "uninitialized" | "sprite" | "procedural";

export interface SfxPlaybackOptions {
  readonly pan?: number;
  readonly playbackRate?: number;
}

export interface SfxPlaybackReceipt {
  readonly backend: "sprite" | "procedural";
  readonly slot: SfxSlotId;
  readonly variantIndex: number;
  readonly playbackRate: number;
  readonly pan: number;
}

export interface AudioStatus {
  readonly state: AudioContextStateLike | "uninitialized";
  readonly muted: boolean;
  readonly needsResumeCapsule: boolean;
  readonly sessionType: IosAudioSessionMode;
  readonly masterVolume: number;
  readonly sfxVolume: number;
  readonly voiceVolume: number;
  readonly tileCallVoiceEnabled: boolean;
  readonly banterVoiceEnabled: boolean;
  readonly backendMode: SfxBackendMode;
  readonly hasAudioSessionApi: boolean;
}

export type FetchLike = (
  input: string,
) => Promise<{ ok: boolean; status: number; json(): Promise<unknown>; arrayBuffer(): Promise<ArrayBuffer> }>;
