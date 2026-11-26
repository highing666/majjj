import { describe, expect, it, vi } from "vitest";
import {
  createViewportDisplayController,
  type FullscreenDocumentLike,
  type ViewportKeyEventLike,
  type ViewportWindowLike,
} from "./viewportDisplay";

interface FakeHostOptions {
  width?: number;
  height?: number;
  supportMode?: "standard" | "webkit" | "unsupported" | "rejecting" | "throwing";
}

function createFakeHost(options: FakeHostOptions = {}) {
  let width = options.width ?? 1280;
  let height = options.height ?? 720;
  const supportMode = options.supportMode ?? "standard";

  const docListeners = new Map<string, Set<(event?: unknown) => void>>();
  const winListeners = new Map<string, Set<(event?: unknown) => void>>();

  let nativeEl: unknown = null;
  let nowMs = 1000;

  const docEl: {
    requestFullscreen?: () => Promise<void>;
    webkitRequestFullscreen?: () => Promise<void>;
  } = {};

  const requestCalls = vi.fn(async () => {
    if (supportMode === "rejecting") {
      throw new Error("Fullscreen rejected");
    }
    nativeEl = docEl;
    emitDoc("fullscreenchange");
  });

  const webkitRequestCalls = vi.fn(async () => {
    nativeEl = docEl;
    emitDoc("webkitfullscreenchange");
  });

  if (supportMode === "standard" || supportMode === "rejecting") {
    docEl.requestFullscreen = requestCalls;
  } else if (supportMode === "throwing") {
    docEl.requestFullscreen = () => {
      throw new Error("Sync fullscreen error");
    };
  } else if (supportMode === "webkit") {
    docEl.webkitRequestFullscreen = webkitRequestCalls;
  }

  const exitCalls = vi.fn(async () => {
    nativeEl = null;
    emitDoc("fullscreenchange");
  });

  const webkitExitCalls = vi.fn(async () => {
    nativeEl = null;
    emitDoc("webkitfullscreenchange");
  });

  function emitDoc(type: string, event?: unknown) {
    for (const fn of docListeners.get(type) ?? []) {
      fn(event);
    }
  }

  function emitWin(type: string, event?: unknown) {
    for (const fn of winListeners.get(type) ?? []) {
      fn(event);
    }
  }

  const documentTarget: FullscreenDocumentLike = {
    get fullscreenElement() {
      return supportMode === "webkit" ? undefined : nativeEl;
    },
    get webkitFullscreenElement() {
      return supportMode === "webkit" ? nativeEl : undefined;
    },
    documentElement: docEl,
    exitFullscreen:
      supportMode === "standard" || supportMode === "rejecting"
        ? exitCalls
        : undefined,
    webkitExitFullscreen:
      supportMode === "webkit" ? webkitExitCalls : undefined,
    addEventListener(type: string, listener: (event?: unknown) => void) {
      let set = docListeners.get(type);
      if (!set) {
        set = new Set();
        docListeners.set(type, set);
      }
      set.add(listener);
    },
    removeEventListener(type: string, listener: (event?: unknown) => void) {
      docListeners.get(type)?.delete(listener);
    },
  };

  const windowTarget: ViewportWindowLike = {
    get innerWidth() {
      return width;
    },
    get innerHeight() {
      return height;
    },
    addEventListener(type: string, listener: (event?: unknown) => void) {
      let set = winListeners.get(type);
      if (!set) {
        set = new Set();
        winListeners.set(type, set);
      }
      set.add(listener);
    },
    removeEventListener(type: string, listener: (event?: unknown) => void) {
      winListeners.get(type)?.delete(listener);
    },
  };

  return {
    documentTarget,
    windowTarget,
    requestCalls,
    webkitRequestCalls,
    exitCalls,
    webkitExitCalls,
    now: () => nowMs,
    advanceMs(delta: number) {
      nowMs += delta;
    },
    resize(nextW: number, nextH: number) {
      width = nextW;
      height = nextH;
      emitWin("resize");
    },
    pressKey(event: ViewportKeyEventLike) {
      emitWin("keydown", event);
    },
    simulateNativeEscapeExit() {
      nativeEl = null;
      emitDoc(supportMode === "webkit" ? "webkitfullscreenchange" : "fullscreenchange");
    },
    listenerCounts() {
      let docCount = 0;
      for (const set of docListeners.values()) docCount += set.size;
      let winCount = 0;
      for (const set of winListeners.values()) winCount += set.size;
      return { docCount, winCount };
    },
  };
}

async function flushMicrotasks(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe("ViewportDisplayController", () => {
  it("toggles native fullscreen and syncs on external fullscreenchange (Esc exit) without triggering ActionBar Pass", async () => {
    const host = createFakeHost({ width: 1280, height: 720, supportMode: "standard" });
    const controller = createViewportDisplayController({
      documentTarget: host.documentTarget,
      windowTarget: host.windowTarget,
      now: host.now,
    });
    controller.syncPhase("InMatch", true);

    expect(controller.getState().nativeFullscreenSupported).toBe(true);
    expect(controller.getState().isFullscreen).toBe(false);
    expect(controller.shouldSuppressActionBarEscape()).toBe(false);

    await controller.toggleFullscreen();
    expect(host.requestCalls).toHaveBeenCalledTimes(1);
    expect(controller.getState().nativeFullscreen).toBe(true);
    expect(controller.getState().isFullscreen).toBe(true);
    // While native fullscreen is active, Escape must be suppressed from ActionBar Pass
    expect(controller.shouldSuppressActionBarEscape()).toBe(true);

    // Simulate browser Esc exit via fullscreenchange
    host.simulateNativeEscapeExit();
    expect(controller.getState().nativeFullscreen).toBe(false);
    expect(controller.getState().isFullscreen).toBe(false);
    // Immediately after Esc exits native fullscreen, ActionBar Pass remains suppressed briefly
    expect(controller.shouldSuppressActionBarEscape()).toBe(true);

    // After grace period expires, normal ActionBar Esc Pass works again
    host.advanceMs(500);
    expect(controller.shouldSuppressActionBarEscape()).toBe(false);

    // Button toggle exits cleanly too
    await controller.toggleFullscreen();
    expect(controller.getState().isFullscreen).toBe(true);
    await controller.toggleFullscreen();
    expect(host.exitCalls).toHaveBeenCalledTimes(1);
    expect(controller.getState().isFullscreen).toBe(false);

    controller.dispose();
    expect(host.listenerCounts()).toEqual({ docCount: 0, winCount: 0 });
  });

  it("supports webkitRequestFullscreen and falls back to 100dvh viewport immersive when unsupported or throwing", async () => {
    // 1. WebKit fullscreen host
    const webkitHost = createFakeHost({ supportMode: "webkit" });
    const webkitCtrl = createViewportDisplayController({
      documentTarget: webkitHost.documentTarget,
      windowTarget: webkitHost.windowTarget,
    });
    webkitCtrl.syncPhase("InMatch", true);
    await webkitCtrl.toggleFullscreen();
    expect(webkitHost.webkitRequestCalls).toHaveBeenCalledTimes(1);
    expect(webkitCtrl.getState().nativeFullscreen).toBe(true);
    await webkitCtrl.toggleFullscreen();
    expect(webkitHost.webkitExitCalls).toHaveBeenCalledTimes(1);
    expect(webkitCtrl.getState().isFullscreen).toBe(false);
    webkitCtrl.dispose();

    // 2. Unsupported (e.g., iPhone Safari) -> smooth viewport immersive toggle
    const iosHost = createFakeHost({ width: 844, height: 390, supportMode: "unsupported" });
    const iosCtrl = createViewportDisplayController({
      documentTarget: iosHost.documentTarget,
      windowTarget: iosHost.windowTarget,
    });
    iosCtrl.syncPhase("InMatch", true);
    expect(iosCtrl.getState().nativeFullscreenSupported).toBe(false);
    await expect(iosCtrl.toggleFullscreen()).resolves.toBeUndefined();
    expect(iosCtrl.getState().viewportImmersive).toBe(true);
    expect(iosCtrl.getState().isFullscreen).toBe(true);
    await iosCtrl.toggleFullscreen();
    expect(iosCtrl.getState().viewportImmersive).toBe(false);
    expect(iosCtrl.getState().isFullscreen).toBe(false);
    iosCtrl.dispose();

    // 3. Rejecting or sync-throwing requestFullscreen -> smooth fallback without throwing
    for (const mode of ["rejecting", "throwing"] as const) {
      const errHost = createFakeHost({ supportMode: mode });
      const errCtrl = createViewportDisplayController({
        documentTarget: errHost.documentTarget,
        windowTarget: errHost.windowTarget,
      });
      errCtrl.syncPhase("InMatch", true);
      await expect(errCtrl.toggleFullscreen()).resolves.toBeUndefined();
      expect(errCtrl.getState().nativeFullscreen).toBe(false);
      expect(errCtrl.getState().viewportImmersive).toBe(true);
      expect(errCtrl.getState().isFullscreen).toBe(true);
      errCtrl.dispose();
    }
  });

  it("routes F shortcut ONLY in InMatch without modals, input focus, or modifier keys", async () => {
    const host = createFakeHost({ width: 1280, height: 720, supportMode: "standard" });
    const controller = createViewportDisplayController({
      documentTarget: host.documentTarget,
      windowTarget: host.windowTarget,
    });

    const preventDefault = vi.fn();

    // Lobby: ignored
    controller.syncPhase("Lobby", true);
    host.pressKey({ key: "f", preventDefault });
    await flushMicrotasks();
    expect(host.requestCalls).not.toHaveBeenCalled();

    // Finished: ignored
    controller.syncPhase("Finished", true);
    host.pressKey({ key: "F", preventDefault });
    await flushMicrotasks();
    expect(host.requestCalls).not.toHaveBeenCalled();

    // InMatch but unseated observer: ignored
    controller.syncPhase("InMatch", false);
    host.pressKey({ key: "f", preventDefault });
    await flushMicrotasks();
    expect(host.requestCalls).not.toHaveBeenCalled();

    // InMatch seated, modal open: ignored
    controller.syncPhase("InMatch", true);
    controller.setModalOpen(true);
    host.pressKey({ key: "f", preventDefault });
    await flushMicrotasks();
    expect(host.requestCalls).not.toHaveBeenCalled();
    controller.setModalOpen(false);

    // Input focused (INPUT / TEXTAREA / SELECT / contentEditable): ignored
    for (const target of [
      { tagName: "INPUT" },
      { tagName: "TEXTAREA" },
      { tagName: "SELECT" },
      { tagName: "DIV", isContentEditable: true },
    ]) {
      host.pressKey({ key: "f", target, preventDefault });
      await flushMicrotasks();
    }
    expect(host.requestCalls).not.toHaveBeenCalled();

    // Modifier keys (ctrl / meta / alt): ignored
    for (const mods of [
      { ctrlKey: true },
      { metaKey: true },
      { altKey: true },
    ]) {
      host.pressKey({ key: "f", ...mods, preventDefault });
      await flushMicrotasks();
    }
    expect(host.requestCalls).not.toHaveBeenCalled();

    // ActionBar keys H/G/P/Escape: ignored by viewportDisplay
    for (const key of ["h", "g", "p", "Escape"]) {
      host.pressKey({ key, preventDefault });
      await flushMicrotasks();
    }
    expect(host.requestCalls).not.toHaveBeenCalled();
    expect(preventDefault).not.toHaveBeenCalled();

    // Valid F press in InMatch -> toggles fullscreen
    host.pressKey({ key: "f", preventDefault });
    await flushMicrotasks();
    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(host.requestCalls).toHaveBeenCalledTimes(1);
    expect(controller.getState().isFullscreen).toBe(true);

    // Second F press -> exits fullscreen
    host.pressKey({ key: "F", preventDefault });
    await flushMicrotasks();
    expect(host.exitCalls).toHaveBeenCalledTimes(1);
    expect(controller.getState().isFullscreen).toBe(false);

    controller.dispose();
  });

  it("computes isShortLandscape and isNarrowPortrait, shows RotateOverlay only in seated InMatch, and resets bypass on landscape rotation", () => {
    const host = createFakeHost({ width: 390, height: 844, supportMode: "unsupported" });
    const controller = createViewportDisplayController({
      documentTarget: host.documentTarget,
      windowTarget: host.windowTarget,
    });

    // Narrow portrait in Lobby & Finished -> never shows rotate overlay
    expect(controller.getState().isNarrowPortrait).toBe(true);
    expect(controller.getState().isShortLandscape).toBe(false);
    controller.syncPhase("Lobby", true);
    expect(controller.getState().showRotateOverlay).toBe(false);

    controller.syncPhase("Finished", true);
    expect(controller.getState().showRotateOverlay).toBe(false);

    // Unseated InMatch -> no rotate overlay
    controller.syncPhase("InMatch", false);
    expect(controller.getState().showRotateOverlay).toBe(false);

    // Seated InMatch in narrow portrait -> shows rotate overlay
    controller.syncPhase("InMatch", true);
    expect(controller.getState().showRotateOverlay).toBe(true);

    // Bypass portrait -> hides rotate overlay for current session
    controller.bypassPortrait();
    expect(controller.getState().portraitBypassed).toBe(true);
    expect(controller.getState().showRotateOverlay).toBe(false);

    // Rotate to iPhone landscape (844x390) -> isShortLandscape=true, auto-resets portraitBypassed
    host.resize(844, 390);
    expect(controller.getState().isShortLandscape).toBe(true);
    expect(controller.getState().isNarrowPortrait).toBe(false);
    expect(controller.getState().portraitBypassed).toBe(false);
    expect(controller.getState().showRotateOverlay).toBe(false);

    // Rotate back to narrow portrait -> rotate overlay appears again
    host.resize(390, 844);
    expect(controller.getState().isShortLandscape).toBe(false);
    expect(controller.getState().isNarrowPortrait).toBe(true);
    expect(controller.getState().showRotateOverlay).toBe(true);

    // Wide portrait (>= 600px, e.g. iPad 768x1024) is NOT narrow portrait
    host.resize(768, 1024);
    expect(controller.getState().isNarrowPortrait).toBe(false);
    expect(controller.getState().showRotateOverlay).toBe(false);

    controller.dispose();
  });
});
