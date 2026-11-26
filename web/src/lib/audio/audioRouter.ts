import {
  defaultDevicePreferences,
  type DevicePreferences,
} from "../preferences/devicePreferences";
import type {
  RelativeRole,
  SemanticGameEvent,
  SemanticGameEventType,
} from "../presentation/semanticEvents";
import type { MeldKind } from "../protocol/melds";
import type {
  AudioContextLike,
  AudioContextStateLike,
  AudioSessionLike,
  AudioStatus,
  DynamicsCompressorNodeLike,
  FetchLike,
  GainNodeLike,
  SfxSlotId,
  VoActionCategory,
  VoTriggerRequest,
} from "./audioTypes";
import { createSfxBackend, type SfxBackend } from "./sfxBackend";
import {
  meldKindToVoiceSlot,
  quickEmoteToVoiceSlot,
  tileToVoiceSlot,
  winKindToVoiceSlot,
  type VoiceSlotId,
} from "./voicePackSchema";
import { createVoManager, type VoManager } from "./voManager";

const PAN_LEFT = -0.35;
const PAN_RIGHT = 0.35;
const HOVER_COOLDOWN_SEC = 0.06;
const GESTURE_EVENTS = ["pointerdown", "touchend", "click", "keydown"] as const;

export interface EventTargetLike {
  addEventListener(
    type: string,
    listener: (event?: unknown) => void,
    options?: boolean | AddEventListenerOptions,
  ): void;
  removeEventListener(
    type: string,
    listener: (event?: unknown) => void,
    options?: boolean | EventListenerOptions,
  ): void;
}

export interface AudioEventRouterOptions {
  readonly initialPreferences?: DevicePreferences;
  readonly createAudioContext?: () => AudioContextLike | null;
  readonly getAudioSession?: () => AudioSessionLike | null;
  readonly documentTarget?: EventTargetLike | null;
  readonly getVisibilityState?: () => DocumentVisibilityState;
  readonly sfxBackend?: SfxBackend;
  readonly voManager?: VoManager;
  readonly fetchFn?: FetchLike;
}

export type AudioStatusListener = (status: AudioStatus) => void;

export interface AudioEventRouter {
  getStatus(): AudioStatus;
  subscribe(listener: AudioStatusListener): () => void;
  applyPreferences(preferences: DevicePreferences): void;
  preloadVoicePacks(packIds: readonly (string | null | undefined)[]): void;
  unlockFromGesture(): boolean;
  bindGestureUnlock(target?: EventTargetLike | null): () => void;
  handleSemanticEvent(event: SemanticGameEvent): void;
  dispose(): void;
}

function defaultCreateAudioContext(): AudioContextLike | null {
  if (typeof window === "undefined") {
    return null;
  }
  const win = window as unknown as {
    AudioContext?: new () => AudioContextLike;
    webkitAudioContext?: new () => AudioContextLike;
  };
  const AudioCtx = win.AudioContext ?? win.webkitAudioContext;
  if (!AudioCtx) {
    return null;
  }
  try {
    return new AudioCtx();
  } catch {
    return null;
  }
}

function defaultGetAudioSession(): AudioSessionLike | null {
  if (typeof navigator === "undefined") {
    return null;
  }
  const nav = navigator as unknown as { audioSession?: AudioSessionLike };
  return nav.audioSession ?? null;
}

function defaultDocumentTarget(): EventTargetLike | null {
  return typeof document !== "undefined" ? document : null;
}

function defaultVisibilityState(): DocumentVisibilityState {
  return typeof document !== "undefined" ? document.visibilityState : "visible";
}

const ROLE_PAN: Partial<Record<RelativeRole, number>> = {
  上家: PAN_LEFT,
  下家: PAN_RIGHT,
};

const STATIC_UI_SFX: Partial<Record<SemanticGameEventType, SfxSlotId>> = {
  UiTileHover: "hover",
  UiTileSelect: "select",
  UiPassClicked: "pass",
  UiConfirmNextHand: "confirm",
  MatchStarted: "confirm",
};

const MELD_SFX_SLOT: Partial<Record<MeldKind, SfxSlotId>> = {
  Pon: "meld_pon",
  ExposedKong: "meld_kong",
  AddedKong: "meld_kong",
  ConcealedKong: "meld_kong",
};

const MELD_VO_CATEGORY: Record<MeldKind, VoActionCategory> = {
  Pon: "Pon",
  Chi: "Chi",
  ExposedKong: "Kong",
  AddedKong: "Kong",
  ConcealedKong: "Kong",
};

function resolvePan(role: RelativeRole | null): number {
  return role !== null ? (ROLE_PAN[role] ?? 0) : 0;
}

function resolveWinTierSlot(totalFan: number | null): SfxSlotId {
  const fan = totalFan ?? 0;
  if (fan <= 1) {
    return "win_tier_1";
  }
  if (fan <= 3) {
    return "win_tier_2";
  }
  return "win_tier_3";
}

function resolveSfxMapping(
  event: SemanticGameEvent,
): { slot: SfxSlotId; pan: number } | null {
  const staticSlot = STATIC_UI_SFX[event.type];
  if (staticSlot) {
    return { slot: staticSlot, pan: 0 };
  }

  const pan = resolvePan(event.relativeRole);
  switch (event.type) {
    case "UiActionPrompted":
      return event.relativeRole === "自家" && event.actions.length > 0
        ? { slot: "action_prompt", pan: 0 }
        : null;
    case "TileDrawn":
      return { slot: "draw", pan };
    case "TileDiscarded": {
      const heavy = !event.isTsumogiri || event.wallRemaining <= 24;
      return { slot: heavy ? "discard_heavy" : "discard_light", pan };
    }
    case "AddedKongAttempted":
      return { slot: "added_kong_suspense", pan };
    case "MeldDeclared": {
      const slot = event.meldKind ? MELD_SFX_SLOT[event.meldKind] : undefined;
      return slot ? { slot, pan } : null;
    }
    case "HandSettled":
      return event.isDrawGame
        ? { slot: "draw_game", pan: 0 }
        : {
            slot: resolveWinTierSlot(event.totalFan),
            pan: event.isMultiRon ? 0 : pan,
          };
    default:
      return null;
  }
}

function resolveVoTrigger(
  event: SemanticGameEvent,
): { category: VoActionCategory; slot: VoiceSlotId } | null {
  switch (event.type) {
    case "TileDiscarded": {
      const slot = event.tile !== null ? tileToVoiceSlot(event.tile) : null;
      return slot !== null ? { category: "Discard", slot } : null;
    }
    case "MeldDeclared": {
      if (!event.meldKind) {
        return null;
      }
      const slot = meldKindToVoiceSlot(event.meldKind);
      return slot !== null
        ? { category: MELD_VO_CATEGORY[event.meldKind], slot }
        : null;
    }
    case "HandSettled": {
      if (event.isDrawGame || event.winKind === null) {
        return null;
      }
      const slot = winKindToVoiceSlot(event.winKind);
      const category: VoActionCategory =
        event.winKind === "RobKongHu" ? "RobKong" : "Hu";
      return { category, slot };
    }
    case "QuickEmoteBroadcast": {
      if (event.quickEmote === null) {
        return null;
      }
      return {
        category: "QuickEmote",
        slot: quickEmoteToVoiceSlot(event.quickEmote),
      };
    }
    default:
      return null;
  }
}

function resolveVoRequest(
  event: SemanticGameEvent,
): { request: VoTriggerRequest; pan: number } | null {
  const resolved = resolveVoTrigger(event);
  if (resolved === null || !event.seat || !event.voicePackId) {
    return null;
  }
  return {
    request: {
      seat: event.seat,
      packId: event.voicePackId,
      category: resolved.category,
      slot: resolved.slot,
    },
    pan: resolvePan(event.relativeRole),
  };
}

/**
 * Coordinates the Web Audio mixing graph (`sfxGain` + `voGain` -> `masterGain` ->
 * `DynamicsCompressorNode` -> `destination`), public-info SFX layering, relative panning,
 * v2 VO pack arbitration/ducking, and Safari `audioSession` / W3C `interrupted` recovery.
 */
export function createAudioEventRouter(
  options: AudioEventRouterOptions = {},
): AudioEventRouter {
  const createCtx = options.createAudioContext ?? defaultCreateAudioContext;
  const getAudioSession = options.getAudioSession ?? defaultGetAudioSession;
  const documentTarget =
    options.documentTarget !== undefined
      ? options.documentTarget
      : defaultDocumentTarget();
  const getVisibilityState =
    options.getVisibilityState ?? defaultVisibilityState;
  const sfxBackend =
    options.sfxBackend ?? createSfxBackend({ fetchFn: options.fetchFn });
  const voManager =
    options.voManager ?? createVoManager({ fetchFn: options.fetchFn });

  let prefs: DevicePreferences = options.initialPreferences
    ? { ...options.initialPreferences }
    : defaultDevicePreferences();
  let ctx: AudioContextLike | null = null;
  let masterGain: GainNodeLike | null = null;
  let sfxGain: GainNodeLike | null = null;
  let voGain: GainNodeLike | null = null;
  let compressor: DynamicsCompressorNodeLike | null = null;
  let lastHoverAt = -Infinity;
  let desiredVoicePacks: string[] = [];
  let disposed = false;

  const listeners = new Set<AudioStatusListener>();

  function applyAudioSession(): void {
    try {
      const session = getAudioSession();
      if (session) {
        session.type = prefs.ios_audio_session;
      }
    } catch {
      // Ignore browsers rejecting audioSession assignment.
    }
  }

  function getContextState(): AudioContextStateLike | "uninitialized" {
    return ctx ? ctx.state : "uninitialized";
  }

  function snapshotStatus(): AudioStatus {
    const state = getContextState();
    return {
      state,
      muted: prefs.muted,
      needsResumeCapsule: !prefs.muted && state !== "running",
      sessionType: prefs.ios_audio_session,
      masterVolume: prefs.master_volume,
      sfxVolume: prefs.sfx_volume,
      backendMode: sfxBackend.getMode(),
      hasAudioSessionApi: getAudioSession() !== null,
    };
  }

  function notify(): void {
    const status = snapshotStatus();
    for (const listener of listeners) {
      listener(status);
    }
  }

  const handleStateChange = (): void => {
    notify();
  };

  const handleVisibilityChange = (): void => {
    if (disposed) {
      return;
    }
    if (
      getVisibilityState() === "visible" &&
      ctx !== null &&
      ctx.state === "suspended"
    ) {
      // Only resume from 'suspended'; never call resume() while W3C 'interrupted'.
      ctx
        .resume()
        .then(() => notify())
        .catch(() => notify());
    }
    notify();
  };

  if (documentTarget) {
    documentTarget.addEventListener("visibilitychange", handleVisibilityChange);
  }

  function syncGainValues(): void {
    if (!ctx || !masterGain || !sfxGain) {
      return;
    }
    const targetMaster = prefs.muted ? 0 : prefs.master_volume;
    masterGain.gain.setTargetAtTime(targetMaster, ctx.currentTime, 0.015);
    voManager.updateSfxBaseVolume(ctx, sfxGain, prefs.sfx_volume);
  }

  function ensureContext(): AudioContextLike | null {
    if (disposed) {
      return null;
    }
    if (ctx !== null) {
      return ctx;
    }
    applyAudioSession();
    const created = createCtx();
    if (!created) {
      return null;
    }
    ctx = created;
    compressor = created.createDynamicsCompressor();
    compressor.threshold.value = -10;
    compressor.knee.value = 18;
    compressor.ratio.value = 6;
    compressor.attack.value = 0.002;
    compressor.release.value = 0.12;

    masterGain = created.createGain();
    masterGain.gain.value = prefs.muted ? 0 : prefs.master_volume;

    sfxGain = created.createGain();
    sfxGain.gain.value = prefs.sfx_volume;

    voGain = created.createGain();
    voGain.gain.value = 1;

    sfxGain.connect(masterGain);
    voGain.connect(masterGain);
    masterGain.connect(compressor);
    compressor.connect(created.destination);

    created.addEventListener("statechange", handleStateChange);
    void sfxBackend.ensureLoaded(created).then(() => {
      if (!disposed) {
        notify();
      }
    });
    for (const packId of desiredVoicePacks) {
      void voManager.preloadPack(created, packId);
    }

    return ctx;
  }

  function playSilentUnlockBuffer(audioCtx: AudioContextLike): void {
    try {
      const silent = audioCtx.createBuffer(1, 1, audioCtx.sampleRate);
      const src = audioCtx.createBufferSource();
      src.buffer = silent;
      src.connect(audioCtx.destination);
      src.start(0);
    } catch {
      // Best-effort iOS Safari gesture unlock.
    }
  }

  // Apply preferred audioSession eagerly on boot for iOS Safari.
  applyAudioSession();

  return {
    getStatus: snapshotStatus,

    subscribe(listener: AudioStatusListener): () => void {
      listeners.add(listener);
      listener(snapshotStatus());
      return () => {
        listeners.delete(listener);
      };
    },

    applyPreferences(nextPreferences: DevicePreferences): void {
      if (disposed) {
        return;
      }
      const sessionChanged =
        prefs.ios_audio_session !== nextPreferences.ios_audio_session;
      prefs = { ...nextPreferences };
      if (sessionChanged) {
        applyAudioSession();
      }
      syncGainValues();
      notify();
    },

    preloadVoicePacks(packIds: readonly (string | null | undefined)[]): void {
      if (disposed) {
        return;
      }
      const unique = new Set<string>();
      for (const raw of packIds) {
        if (typeof raw === "string") {
          const trimmed = raw.trim();
          if (trimmed.length > 0) {
            unique.add(trimmed);
          }
        }
      }
      desiredVoicePacks = [...unique];
      if (ctx !== null) {
        for (const packId of desiredVoicePacks) {
          void voManager.preloadPack(ctx, packId);
        }
      }
    },

    unlockFromGesture(): boolean {
      const audioCtx = ensureContext();
      if (!audioCtx) {
        notify();
        return false;
      }
      if (audioCtx.state === "interrupted" || audioCtx.state === "closed") {
        notify();
        return false;
      }
      if (audioCtx.state === "suspended") {
        audioCtx
          .resume()
          .then(() => notify())
          .catch(() => notify());
        playSilentUnlockBuffer(audioCtx);
      }
      notify();
      return true;
    },

    bindGestureUnlock(
      target: EventTargetLike | null = typeof window !== "undefined"
        ? window
        : null,
    ): () => void {
      if (!target) {
        return () => {};
      }
      const onGesture = (): void => {
        this.unlockFromGesture();
      };
      const listenerOpts = { capture: true, passive: true };
      for (const type of GESTURE_EVENTS) {
        target.addEventListener(type, onGesture, listenerOpts);
      }
      return () => {
        for (const type of GESTURE_EVENTS) {
          target.removeEventListener(type, onGesture, listenerOpts);
        }
      };
    },

    handleSemanticEvent(event: SemanticGameEvent): void {
      if (disposed || prefs.muted) {
        return;
      }
      const sfxMapping = resolveSfxMapping(event);
      const voMapping = resolveVoRequest(event);
      if (sfxMapping === null && voMapping === null) {
        return;
      }

      const audioCtx = ensureContext();
      if (!audioCtx || !sfxGain || !voGain) {
        notify();
        return;
      }
      if (audioCtx.state !== "running") {
        notify();
        return;
      }

      if (sfxMapping !== null) {
        if (sfxMapping.slot === "hover") {
          if (audioCtx.currentTime - lastHoverAt < HOVER_COOLDOWN_SEC) {
            return;
          }
          lastHoverAt = audioCtx.currentTime;
        }
        sfxBackend.play(audioCtx, sfxGain, sfxMapping.slot, {
          pan: sfxMapping.pan,
        });
      }

      if (voMapping !== null) {
        void voManager.triggerVo(
          audioCtx,
          voGain,
          sfxGain,
          prefs.sfx_volume,
          voMapping.request,
          voMapping.pan,
        );
      }
    },

    dispose(): void {
      if (disposed) {
        return;
      }
      disposed = true;
      if (documentTarget) {
        documentTarget.removeEventListener(
          "visibilitychange",
          handleVisibilityChange,
        );
      }
      if (ctx) {
        ctx.removeEventListener("statechange", handleStateChange);
        void ctx.close?.().catch(() => {});
      }
      voManager.dispose();
      sfxBackend.dispose();
      listeners.clear();
      ctx = null;
      masterGain = null;
      sfxGain = null;
      voGain = null;
      compressor = null;
    },
  };
}
