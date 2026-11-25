import { describe, expect, it, vi } from "vitest";
import {
  createWebGLRecoveryController,
  type WebGLCanvasTargetLike,
  type WebGLContextEventLike,
  type WebGLRecoveryState,
} from "./webglRecovery";

function createFakeCanvasTarget(): WebGLCanvasTargetLike & {
  emit: (
    type: "webglcontextlost" | "webglcontextrestored",
    event?: WebGLContextEventLike,
  ) => void;
  listenerCount: (type: string) => number;
} {
  const listeners = new Map<
    string,
    Set<(event: WebGLContextEventLike) => void>
  >();
  return {
    addEventListener(type, listener) {
      let set = listeners.get(type);
      if (!set) {
        set = new Set();
        listeners.set(type, set);
      }
      set.add(listener);
    },
    removeEventListener(type, listener) {
      listeners.get(type)?.delete(listener);
    },
    emit(type, event = {}) {
      for (const fn of listeners.get(type) ?? []) {
        fn(event);
      }
    },
    listenerCount(type) {
      return listeners.get(type)?.size ?? 0;
    },
  };
}

async function flushMicrotasks(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe("WebGLRecoveryController", () => {
  it("calls preventDefault on webglcontextlost and rebuilds atlas/textures/filter/scene on webglcontextrestored", async () => {
    const canvas = createFakeCanvasTarget();
    const rebuildResources = vi.fn(async () => {});
    const states: WebGLRecoveryState[] = [];

    const controller = createWebGLRecoveryController({
      rebuildResources,
    });
    controller.subscribe((s) => states.push(s));
    const unbind = controller.bindCanvas(canvas);

    expect(controller.getState()).toMatchObject({
      status: "active",
      restoreCount: 0,
      needsRefresh: false,
      errorMessage: null,
    });

    const preventDefault = vi.fn();
    canvas.emit("webglcontextlost", { preventDefault });

    expect(preventDefault).toHaveBeenCalledTimes(1);
    expect(controller.getState()).toMatchObject({
      status: "context_lost",
      needsRefresh: false,
    });

    canvas.emit("webglcontextrestored");
    await flushMicrotasks();

    expect(rebuildResources).toHaveBeenCalledTimes(1);
    expect(controller.getState()).toMatchObject({
      status: "active",
      restoreCount: 1,
      needsRefresh: false,
      errorMessage: null,
    });
    expect(states.map((s) => s.status)).toEqual([
      "context_lost",
      "restoring",
      "active",
    ]);

    unbind();
    expect(canvas.listenerCount("webglcontextlost")).toBe(0);
    expect(canvas.listenerCount("webglcontextrestored")).toBe(0);
    controller.dispose();
  });

  it("exposes unrecoverable refresh banner state when resource rebuild fails", async () => {
    const canvas = createFakeCanvasTarget();
    const rebuildResources = vi.fn(async () => {
      throw new Error("GPU context permanently lost");
    });

    const controller = createWebGLRecoveryController({ rebuildResources });
    controller.bindCanvas(canvas);

    canvas.emit("webglcontextlost", { preventDefault: vi.fn() });
    canvas.emit("webglcontextrestored");
    await flushMicrotasks();

    const state = controller.getState();
    expect(state.status).toBe("unrecoverable");
    expect(state.needsRefresh).toBe(true);
    expect(state.errorMessage).toContain("刷新");
    expect(state.errorMessage).toContain("TableSnapshot");

    controller.dispose();
  });

  it("ignores stale restoration completion if context is lost again mid-rebuild", async () => {
    let resolveRebuild!: () => void;
    const rebuildResources = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveRebuild = resolve;
        }),
    );

    const controller = createWebGLRecoveryController({ rebuildResources });
    const preventDefault = vi.fn();

    await controller.handleContextLost({ preventDefault });
    const restorePromise = controller.handleContextRestored();
    expect(controller.getState().status).toBe("restoring");

    // Context is lost again before the async atlas rebuild completes
    controller.handleContextLost({ preventDefault });
    expect(controller.getState().status).toBe("context_lost");

    resolveRebuild();
    await restorePromise;

    expect(controller.getState().status).toBe("context_lost");
    expect(controller.getState().restoreCount).toBe(0);

    controller.dispose();
  });

  it("exposes unrecoverable banner state when initial WebGL boot fails", () => {
    const controller = createWebGLRecoveryController({
      rebuildResources: async () => {},
    });

    controller.reportUnrecoverable("WebGL2 不可用");
    const state = controller.getState();
    expect(state.status).toBe("unrecoverable");
    expect(state.needsRefresh).toBe(true);
    expect(state.errorMessage).toContain("WebGL2 不可用");
    expect(state.errorMessage).toContain("刷新");

    controller.dispose();
  });
});
