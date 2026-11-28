import { describe, expect, it } from "vitest";
import { createMemoryStorage, type StorageLike } from "../session/storage";
import {
  DEVICE_PREFERENCES_STORAGE_KEY,
  createDevicePreferencesStore,
  detectDefaultConfirmDiscard,
} from "./devicePreferences";

describe("devicePreferences", () => {
  it("detects pointer-aware default for confirm_discard (false for fine hover pointer, true for coarse touch)", () => {
    const macMatchMedia = (query: string) => ({
      matches: query === "(hover: hover) and (pointer: fine)",
    });
    const iphoneMatchMedia = (query: string) => ({
      matches: query === "(pointer: coarse)" || query === "(hover: none)",
    });

    expect(detectDefaultConfirmDiscard(macMatchMedia, 0)).toBe(false);
    expect(detectDefaultConfirmDiscard(iphoneMatchMedia, 5)).toBe(true);
    expect(detectDefaultConfirmDiscard(undefined, 5)).toBe(true);
    expect(detectDefaultConfirmDiscard(undefined, 0)).toBe(false);
  });

  it("loads defaults, clamps out-of-range volumes, and persists updates under majjj.device_preferences", () => {
    const raw = createMemoryStorage();
    const store = createDevicePreferencesStore(raw, {
      defaultConfirmDiscard: false,
    });

    expect(store.load()).toEqual({
      master_volume: 0.85,
      sfx_volume: 1,
      voice_volume: 1,
      muted: false,
      tile_call_voice_enabled: true,
      banter_voice_enabled: true,
      human_voice_pack_id: null,
      ios_audio_session: "playback",
      confirm_discard: false,
      reduced_effects: false,
    });
    expect("hide_banter_bubble_with_voice" in store.load()).toBe(false);

    const updated = store.update({
      master_volume: 1.8,
      sfx_volume: -0.4,
      voice_volume: 1.4,
      muted: true,
      tile_call_voice_enabled: false,
      banter_voice_enabled: false,
      human_voice_pack_id: "human_female",
      ios_audio_session: "ambient",
      confirm_discard: true,
      reduced_effects: true,
    });

    expect(updated).toEqual({
      master_volume: 1,
      sfx_volume: 0,
      voice_volume: 1,
      muted: true,
      tile_call_voice_enabled: false,
      banter_voice_enabled: false,
      human_voice_pack_id: "human_female",
      ios_audio_session: "ambient",
      confirm_discard: true,
      reduced_effects: true,
    });

    const clampedLow = store.update({
      voice_volume: -0.25,
      human_voice_pack_id: "human_male",
    });
    expect(clampedLow.voice_volume).toBe(0);
    expect(clampedLow.human_voice_pack_id).toBe("human_male");

    const reloaded = createDevicePreferencesStore(raw, {
      defaultConfirmDiscard: false,
    }).load();
    expect(reloaded).toEqual(clampedLow);
    expect(raw.getItem(DEVICE_PREFERENCES_STORAGE_KEY)).not.toBeNull();
  });

  it("falls back safely on corrupt JSON, invalid field types, throwing storage, and stays isolated from table pace", () => {
    const raw = createMemoryStorage();
    raw.setItem(DEVICE_PREFERENCES_STORAGE_KEY, "{not-valid-json");

    const store = createDevicePreferencesStore(raw, {
      defaultConfirmDiscard: true,
    });
    expect(store.load()).toEqual({
      master_volume: 0.85,
      sfx_volume: 1,
      voice_volume: 1,
      muted: false,
      tile_call_voice_enabled: true,
      banter_voice_enabled: true,
      human_voice_pack_id: null,
      ios_audio_session: "playback",
      confirm_discard: true,
      reduced_effects: false,
    });

    raw.setItem(
      DEVICE_PREFERENCES_STORAGE_KEY,
      JSON.stringify({
        master_volume: "loud",
        sfx_volume: Number.NaN,
        voice_volume: Number.POSITIVE_INFINITY,
        muted: "yes",
        tile_call_voice_enabled: 0,
        banter_voice_enabled: "false",
        hide_banter_bubble_with_voice: 1,
        human_voice_pack_id: "quick_hothead",
        ios_audio_session: "invalid",
        confirm_discard: 1,
        reduced_effects: null,
        pace: "Fast",
      }),
    );

    const sanitized = store.load();
    expect(sanitized).toEqual({
      master_volume: 0.85,
      sfx_volume: 1,
      voice_volume: 1,
      muted: false,
      tile_call_voice_enabled: true,
      banter_voice_enabled: true,
      human_voice_pack_id: null,
      ios_audio_session: "playback",
      confirm_discard: true,
      reduced_effects: false,
    });
    expect("pace" in sanitized).toBe(false);
    expect("hide_banter_bubble_with_voice" in sanitized).toBe(false);

    store.update({ muted: true, human_voice_pack_id: "human_female" });
    const persisted = JSON.parse(
      raw.getItem(DEVICE_PREFERENCES_STORAGE_KEY) ?? "{}",
    ) as Record<string, unknown>;
    expect("pace" in persisted).toBe(false);
    expect("hide_banter_bubble_with_voice" in persisted).toBe(false);
    expect(persisted.human_voice_pack_id).toBe("human_female");

    const throwingStorage: StorageLike = {
      getItem() {
        throw new Error("SecurityError");
      },
      setItem() {
        throw new Error("QuotaExceededError");
      },
      removeItem() {
        throw new Error("SecurityError");
      },
    };
    const fallbackStore = createDevicePreferencesStore(throwingStorage, {
      defaultConfirmDiscard: false,
    });
    expect(fallbackStore.load().confirm_discard).toBe(false);
    expect(fallbackStore.update({ muted: true }).muted).toBe(true);
    expect(fallbackStore.load().muted).toBe(true);
  });

  it("notifies subscribers on updates and reset", () => {
    const raw = createMemoryStorage();
    const store = createDevicePreferencesStore(raw, {
      defaultConfirmDiscard: false,
    });
    const seen: boolean[] = [];
    const unsubscribe = store.subscribe((prefs) => {
      seen.push(prefs.muted);
    });

    store.update({ muted: true });
    store.reset();
    unsubscribe();
    store.update({ muted: true });

    expect(seen).toEqual([true, false]);
  });

  it("smoothly ignores legacy hide_banter_bubble_with_voice key from existing localStorage", () => {
    for (const legacyValue of [false, true]) {
      const raw = createMemoryStorage();
      raw.setItem(
        DEVICE_PREFERENCES_STORAGE_KEY,
        JSON.stringify({
          master_volume: 0.7,
          sfx_volume: 0.8,
          voice_volume: 0.9,
          muted: false,
          tile_call_voice_enabled: true,
          banter_voice_enabled: true,
          hide_banter_bubble_with_voice: legacyValue,
          human_voice_pack_id: "human_male",
          ios_audio_session: "ambient",
          confirm_discard: true,
          reduced_effects: true,
        }),
      );

      const store = createDevicePreferencesStore(raw, {
        defaultConfirmDiscard: false,
      });
      const loaded = store.load();
      expect(loaded).toEqual({
        master_volume: 0.7,
        sfx_volume: 0.8,
        voice_volume: 0.9,
        muted: false,
        tile_call_voice_enabled: true,
        banter_voice_enabled: true,
        human_voice_pack_id: "human_male",
        ios_audio_session: "ambient",
        confirm_discard: true,
        reduced_effects: true,
      });
      expect("hide_banter_bubble_with_voice" in loaded).toBe(false);

      store.update({ master_volume: 0.75 });
      const persisted = JSON.parse(
        raw.getItem(DEVICE_PREFERENCES_STORAGE_KEY) ?? "{}",
      ) as Record<string, unknown>;
      expect("hide_banter_bubble_with_voice" in persisted).toBe(false);
      expect(persisted.master_volume).toBe(0.75);
    }
  });
});
