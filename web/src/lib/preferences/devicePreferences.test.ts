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
      muted: false,
      ios_audio_session: "playback",
      confirm_discard: false,
      reduced_effects: false,
    });

    const updated = store.update({
      master_volume: 1.8,
      sfx_volume: -0.4,
      muted: true,
      ios_audio_session: "ambient",
      confirm_discard: true,
      reduced_effects: true,
    });

    expect(updated).toEqual({
      master_volume: 1,
      sfx_volume: 0,
      muted: true,
      ios_audio_session: "ambient",
      confirm_discard: true,
      reduced_effects: true,
    });

    const reloaded = createDevicePreferencesStore(raw, {
      defaultConfirmDiscard: false,
    }).load();
    expect(reloaded).toEqual(updated);
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
      muted: false,
      ios_audio_session: "playback",
      confirm_discard: true,
      reduced_effects: false,
    });

    raw.setItem(
      DEVICE_PREFERENCES_STORAGE_KEY,
      JSON.stringify({
        master_volume: "loud",
        sfx_volume: Number.NaN,
        muted: "yes",
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
      muted: false,
      ios_audio_session: "playback",
      confirm_discard: true,
      reduced_effects: false,
    });
    expect("pace" in sanitized).toBe(false);

    store.update({ muted: true });
    const persisted = JSON.parse(
      raw.getItem(DEVICE_PREFERENCES_STORAGE_KEY) ?? "{}",
    ) as Record<string, unknown>;
    expect("pace" in persisted).toBe(false);

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
});
