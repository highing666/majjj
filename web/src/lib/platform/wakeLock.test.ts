import { describe, expect, it, vi } from "vitest";
import {
  createWakeLockController,
  type WakeLockLike,
  type WakeLockSentinelLike,
} from "./wakeLock";

function createFakeSentinel(): WakeLockSentinelLike & {
  emitRelease: () => void;
  releaseCalls: number;
} {
  let released = false;
  const listeners = new Set<() => void>();
  const sentinel = {
    get released() {
      return released;
    },
    releaseCalls: 0,
    async release() {
      sentinel.releaseCalls += 1;
      if (!released) {
        released = true;
        for (const fn of listeners) {
          fn();
        }
      }
    },
    addEventListener(type: string, listener: () => void) {
      if (type === "release") {
        listeners.add(listener);
      }
    },
    removeEventListener(type: string, listener: () => void) {
      if (type === "release") {
        listeners.delete(listener);
      }
    },
    emitRelease() {
      released = true;
      for (const fn of listeners) {
        fn();
      }
    },
  };
  return sentinel;
}

function createFakeVisibilityDoc(initial: DocumentVisibilityState = "visible") {
  let state: DocumentVisibilityState = initial;
  const listeners = new Set<() => void>();
  return {
    target: {
      addEventListener(type: string, fn: () => void) {
        if (type === "visibilitychange") {
          listeners.add(fn);
        }
      },
      removeEventListener(type: string, fn: () => void) {
        if (type === "visibilitychange") {
          listeners.delete(fn);
        }
      },
    },
    getVisibilityState: () => state,
    setVisibility(next: DocumentVisibilityState) {
      state = next;
      for (const fn of listeners) {
        fn();
      }
    },
    listenerCount: () => listeners.size,
  };
}

async function flushMicrotasks(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe("WakeLockController", () => {
  it("requests screen wake lock in InMatch, re-requests on visibilitychange -> visible, and releases on Lobby/Finished/hidden", async () => {
    const sentinels: ReturnType<typeof createFakeSentinel>[] = [];
    const requestMock = vi.fn(async () => {
      const s = createFakeSentinel();
      sentinels.push(s);
      return s;
    });
    const wakeLock: WakeLockLike = { request: requestMock };
    const doc = createFakeVisibilityDoc("visible");

    const controller = createWakeLockController({
      wakeLock,
      documentTarget: doc.target,
      getVisibilityState: doc.getVisibilityState,
    });

    // Lobby does not request wake lock
    controller.syncPhase("Lobby");
    await flushMicrotasks();
    expect(requestMock).not.toHaveBeenCalled();
    expect(controller.isHeld()).toBe(false);

    // Enter InMatch -> requests screen wake lock
    controller.syncPhase("InMatch");
    await flushMicrotasks();
    expect(requestMock).toHaveBeenCalledTimes(1);
    expect(requestMock).toHaveBeenCalledWith("screen");
    expect(controller.isHeld()).toBe(true);

    // Tab goes hidden -> releases sentinel
    doc.setVisibility("hidden");
    await flushMicrotasks();
    expect(sentinels[0]!.releaseCalls).toBe(1);
    expect(controller.isHeld()).toBe(false);

    // Tab returns visible while still InMatch -> re-requests wake lock
    doc.setVisibility("visible");
    await flushMicrotasks();
    expect(requestMock).toHaveBeenCalledTimes(2);
    expect(controller.isHeld()).toBe(true);

    // Match ends -> Finished releases wake lock
    controller.syncPhase("Finished");
    await flushMicrotasks();
    expect(sentinels[1]!.releaseCalls).toBe(1);
    expect(controller.isHeld()).toBe(false);

    // Returning visible in Finished does NOT re-request
    doc.setVisibility("hidden");
    doc.setVisibility("visible");
    await flushMicrotasks();
    expect(requestMock).toHaveBeenCalledTimes(2);

    controller.dispose();
    expect(doc.listenerCount()).toBe(0);
  });

  it("releases a late-resolving sentinel if match phase changed before request resolved", async () => {
    let resolveRequest!: (s: WakeLockSentinelLike) => void;
    const wakeLock: WakeLockLike = {
      request: vi.fn(
        () =>
          new Promise<WakeLockSentinelLike>((resolve) => {
            resolveRequest = resolve;
          }),
      ),
    };
    const doc = createFakeVisibilityDoc("visible");
    const controller = createWakeLockController({
      wakeLock,
      documentTarget: doc.target,
      getVisibilityState: doc.getVisibilityState,
    });

    controller.syncPhase("InMatch");
    controller.syncPhase("Lobby");

    const lateSentinel = createFakeSentinel();
    resolveRequest(lateSentinel);
    await flushMicrotasks();

    expect(lateSentinel.releaseCalls).toBe(1);
    expect(controller.isHeld()).toBe(false);
    controller.dispose();
  });

  it("silently falls back when navigator.wakeLock is missing or request rejects", async () => {
    const doc = createFakeVisibilityDoc("visible");

    // Case 1: API absent
    const absentController = createWakeLockController({
      wakeLock: null,
      documentTarget: doc.target,
      getVisibilityState: doc.getVisibilityState,
    });
    expect(() => absentController.syncPhase("InMatch")).not.toThrow();
    await flushMicrotasks();
    expect(absentController.isHeld()).toBe(false);
    absentController.dispose();

    // Case 2: API rejects (e.g. battery saver / disallowed)
    const rejectingWakeLock: WakeLockLike = {
      request: vi.fn(async () => {
        throw new Error("NotAllowedError");
      }),
    };
    const rejectController = createWakeLockController({
      wakeLock: rejectingWakeLock,
      documentTarget: doc.target,
      getVisibilityState: doc.getVisibilityState,
    });
    expect(() => rejectController.syncPhase("InMatch")).not.toThrow();
    await flushMicrotasks();
    expect(rejectController.isHeld()).toBe(false);
    rejectController.dispose();
  });
});
