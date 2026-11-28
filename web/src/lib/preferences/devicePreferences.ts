import {
  isHumanVoicePackId,
  type HumanVoicePackId,
} from "../audio/voicePackSchema";
import { createMemoryStorage, type StorageLike } from "../session/storage";

export type IosAudioSessionMode = "playback" | "ambient";

export interface DevicePreferences {
  master_volume: number;
  sfx_volume: number;
  voice_volume: number;
  muted: boolean;
  tile_call_voice_enabled: boolean;
  banter_voice_enabled: boolean;
  human_voice_pack_id: HumanVoicePackId | null;
  ios_audio_session: IosAudioSessionMode;
  confirm_discard: boolean;
  reduced_effects: boolean;
}

export type MatchMediaQuery = (query: string) => { matches: boolean };

export interface DevicePreferencesStoreOptions {
  defaultConfirmDiscard?: boolean;
  matchMedia?: MatchMediaQuery;
  maxTouchPoints?: number;
}

export interface DevicePreferencesStore {
  load(): DevicePreferences;
  save(preferences: DevicePreferences): DevicePreferences;
  update(patch: Partial<DevicePreferences>): DevicePreferences;
  reset(): DevicePreferences;
  subscribe(listener: (preferences: DevicePreferences) => void): () => void;
}

export const DEVICE_PREFERENCES_STORAGE_KEY = "majjj.device_preferences";

const BASE_DEFAULTS: Omit<DevicePreferences, "confirm_discard"> = {
  master_volume: 0.85,
  sfx_volume: 1,
  voice_volume: 1,
  muted: false,
  tile_call_voice_enabled: true,
  banter_voice_enabled: true,
  human_voice_pack_id: null,
  ios_audio_session: "playback",
  reduced_effects: false,
};

function clampVolume(value: unknown, fallback: number): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return fallback;
  }
  return Math.min(1, Math.max(0, value));
}

function normalizeBoolean(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function normalizeIosAudioSession(
  value: unknown,
  fallback: IosAudioSessionMode,
): IosAudioSessionMode {
  return value === "playback" || value === "ambient" ? value : fallback;
}

function normalizeHumanVoicePackId(
  value: unknown,
  fallback: HumanVoicePackId | null,
): HumanVoicePackId | null {
  if (value === null) {
    return null;
  }
  if (typeof value === "string" && isHumanVoicePackId(value)) {
    return value;
  }
  return fallback;
}

/**
 * Determines pointer-aware default for `confirm_discard`:
 * `false` on fine hover pointers (desktop Mac), `true` on coarse/no-hover touch pointers (iPhone).
 */
export function detectDefaultConfirmDiscard(
  matchMedia?: MatchMediaQuery,
  maxTouchPoints?: number,
): boolean {
  const queryFn =
    matchMedia ??
    (typeof window !== "undefined" && typeof window.matchMedia === "function"
      ? (q: string) => window.matchMedia(q)
      : undefined);

  if (queryFn) {
    if (queryFn("(hover: hover) and (pointer: fine)").matches) {
      return false;
    }
    if (queryFn("(pointer: coarse)").matches || queryFn("(hover: none)").matches) {
      return true;
    }
  }

  const touchPoints =
    maxTouchPoints ??
    (typeof navigator !== "undefined" &&
    typeof navigator.maxTouchPoints === "number"
      ? navigator.maxTouchPoints
      : 0);
  return touchPoints > 0;
}

/**
 * Returns a fresh `DevicePreferences` object with pointer-aware defaults.
 */
export function defaultDevicePreferences(
  defaultConfirmDiscard = detectDefaultConfirmDiscard(),
): DevicePreferences {
  return {
    ...BASE_DEFAULTS,
    confirm_discard: defaultConfirmDiscard,
  };
}

/**
 * Sanitizes and clamps an arbitrary input into the exact 10 `DevicePreferences` fields,
 * dropping unknown or retired legacy properties (such as table-level `pace`).
 */
export function normalizeDevicePreferences(
  raw: unknown,
  defaults: DevicePreferences,
): DevicePreferences {
  if (typeof raw !== "object" || raw === null) {
    return { ...defaults };
  }
  const record = raw as Record<string, unknown>;
  return {
    master_volume: clampVolume(record.master_volume, defaults.master_volume),
    sfx_volume: clampVolume(record.sfx_volume, defaults.sfx_volume),
    voice_volume: clampVolume(record.voice_volume, defaults.voice_volume),
    muted: normalizeBoolean(record.muted, defaults.muted),
    tile_call_voice_enabled: normalizeBoolean(
      record.tile_call_voice_enabled,
      defaults.tile_call_voice_enabled,
    ),
    banter_voice_enabled: normalizeBoolean(
      record.banter_voice_enabled,
      defaults.banter_voice_enabled,
    ),
    human_voice_pack_id: normalizeHumanVoicePackId(
      record.human_voice_pack_id,
      defaults.human_voice_pack_id,
    ),
    ios_audio_session: normalizeIosAudioSession(
      record.ios_audio_session,
      defaults.ios_audio_session,
    ),
    confirm_discard: normalizeBoolean(
      record.confirm_discard,
      defaults.confirm_discard,
    ),
    reduced_effects: normalizeBoolean(
      record.reduced_effects,
      defaults.reduced_effects,
    ),
  };
}

function safeReadStorage(storage: StorageLike, key: string): string | null {
  try {
    return storage.getItem(key);
  } catch {
    return null;
  }
}

function safeWriteStorage(
  storage: StorageLike,
  key: string,
  value: string,
): void {
  try {
    storage.setItem(key, value);
  } catch {
    // Ignore storage quota or private-browsing write errors; in-memory state remains authoritative.
  }
}

function resolveBrowserStorage(): StorageLike {
  if (typeof window !== "undefined") {
    try {
      const storage = window.localStorage;
      if (storage) {
        return storage;
      }
    } catch {
      // Ignore SecurityError when localStorage access is blocked.
    }
  }
  return createMemoryStorage();
}

/**
 * Creates a device-local preferences store backed by `localStorage` (`majjj.device_preferences`)
 * with safe fallbacks for missing/corrupt storage and strict isolation from table-level `pace`.
 */
export function createDevicePreferencesStore(
  storage: StorageLike = resolveBrowserStorage(),
  options: DevicePreferencesStoreOptions = {},
): DevicePreferencesStore {
  const defaultConfirm =
    options.defaultConfirmDiscard ??
    detectDefaultConfirmDiscard(options.matchMedia, options.maxTouchPoints);
  const defaults = defaultDevicePreferences(defaultConfirm);
  const listeners = new Set<(preferences: DevicePreferences) => void>();

  function readNormalized(): DevicePreferences {
    const raw = safeReadStorage(storage, DEVICE_PREFERENCES_STORAGE_KEY);
    if (!raw) {
      return { ...defaults };
    }
    try {
      const parsed: unknown = JSON.parse(raw);
      return normalizeDevicePreferences(parsed, defaults);
    } catch {
      return { ...defaults };
    }
  }

  let current = readNormalized();

  function commit(next: DevicePreferences): DevicePreferences {
    current = next;
    safeWriteStorage(
      storage,
      DEVICE_PREFERENCES_STORAGE_KEY,
      JSON.stringify(current),
    );
    for (const listener of listeners) {
      listener({ ...current });
    }
    return { ...current };
  }

  return {
    load(): DevicePreferences {
      const fromStorage = safeReadStorage(
        storage,
        DEVICE_PREFERENCES_STORAGE_KEY,
      );
      if (fromStorage !== null) {
        current = readNormalized();
      }
      return { ...current };
    },

    save(preferences: DevicePreferences): DevicePreferences {
      return commit(normalizeDevicePreferences(preferences, defaults));
    },

    update(patch: Partial<DevicePreferences>): DevicePreferences {
      return commit(
        normalizeDevicePreferences({ ...current, ...patch }, defaults),
      );
    },

    reset(): DevicePreferences {
      return commit({ ...defaults });
    },

    subscribe(listener: (preferences: DevicePreferences) => void): () => void {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

let browserStore: DevicePreferencesStore | null = null;

/**
 * Returns the shared browser singleton `DevicePreferencesStore`.
 */
export function getBrowserDevicePreferencesStore(): DevicePreferencesStore {
  if (browserStore === null) {
    browserStore = createDevicePreferencesStore();
  }
  return browserStore;
}
