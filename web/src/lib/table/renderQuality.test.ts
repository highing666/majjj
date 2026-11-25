import { describe, expect, it } from "vitest";
import { createPresentationEffectsBus } from "../presentation/effectsBus";
import {
  createRenderQualityController,
  type MediaQueryListLike,
  type RenderQualityState,
} from "./renderQuality";

function createFakeMediaQuery(initialMatches: boolean): {
  list: MediaQueryListLike;
  emitChange: (matches: boolean) => void;
  listenerCount: () => number;
} {
  const listeners = new Set<(event: { matches: boolean }) => void>();
  const list: MediaQueryListLike = {
    matches: initialMatches,
    addEventListener(_type, listener) {
      listeners.add(listener);
    },
    removeEventListener(_type, listener) {
      listeners.delete(listener);
    },
  };
  return {
    list,
    emitChange(matches: boolean) {
      list.matches = matches;
      for (const listener of listeners) {
        listener({ matches });
      }
    },
    listenerCount: () => listeners.size,
  };
}

function createFakeVisibilityTarget(initialHidden = false): {
  target: {
    addEventListener(type: string, listener: () => void): void;
    removeEventListener(type: string, listener: () => void): void;
  };
  getHidden: () => boolean;
  setHidden: (hidden: boolean) => void;
  listenerCount: () => number;
} {
  let hidden = initialHidden;
  const listeners = new Set<() => void>();
  return {
    target: {
      addEventListener(type, listener) {
        if (type === "visibilitychange") {
          listeners.add(listener);
        }
      },
      removeEventListener(type, listener) {
        if (type === "visibilitychange") {
          listeners.delete(listener);
        }
      },
    },
    getHidden: () => hidden,
    setHidden(next: boolean) {
      hidden = next;
      for (const listener of listeners) {
        listener();
      }
    },
    listenerCount: () => listeners.size,
  };
}

function simulateFrames(
  controller: ReturnType<typeof createRenderQualityController>,
  startMs: number,
  durationMs: number,
  fps: number,
): number {
  const stepMs = 1000 / fps;
  let t = startMs;
  const endMs = startMs + durationMs;
  while (t <= endMs + 1e-6) {
    controller.recordFrame(t);
    t += stepMs;
  }
  return endMs;
}

describe("RenderQualityController", () => {
  it("activates degradation on prefers-reduced-motion: reduce (initial & live change) and syncs effectsBus", () => {
    const mq = createFakeMediaQuery(true);
    const bus = createPresentationEffectsBus(false);
    const states: RenderQualityState[] = [];

    const controller = createRenderQualityController({
      effectsBus: bus,
      matchMedia: () => mq.list,
    });
    controller.subscribe((s) => states.push(s));

    expect(controller.getState()).toMatchObject({
      degraded: true,
      paperGrainEnabled: false,
      reducedEffects: true,
      prefersReducedMotion: true,
      manualReducedEffects: false,
      lowFpsDegraded: false,
    });
    expect(bus.getReducedEffects()).toBe(true);

    // OS disables reduced-motion -> recovers when manual and FPS are fine
    mq.emitChange(false);
    expect(controller.getState().degraded).toBe(false);
    expect(controller.getState().paperGrainEnabled).toBe(true);
    expect(bus.getReducedEffects()).toBe(false);
    expect(states).toHaveLength(1);

    // OS re-enables reduced-motion
    mq.emitChange(true);
    expect(controller.getState().degraded).toBe(true);
    expect(controller.getState().paperGrainEnabled).toBe(false);
    expect(bus.getReducedEffects()).toBe(true);

    controller.dispose();
    expect(mq.listenerCount()).toBe(0);
  });

  it("toggles degradation immediately on manual reduced_effects preference", () => {
    const mq = createFakeMediaQuery(false);
    const bus = createPresentationEffectsBus(false);
    const controller = createRenderQualityController({
      effectsBus: bus,
      matchMedia: () => mq.list,
      initialManualReducedEffects: false,
    });

    expect(controller.getState().degraded).toBe(false);
    expect(controller.getState().paperGrainEnabled).toBe(true);
    expect(bus.getReducedEffects()).toBe(false);

    controller.setManualReducedEffects(true);
    expect(controller.getState()).toMatchObject({
      degraded: true,
      paperGrainEnabled: false,
      reducedEffects: true,
      manualReducedEffects: true,
    });
    expect(bus.getReducedEffects()).toBe(true);

    controller.setManualReducedEffects(false);
    expect(controller.getState().degraded).toBe(false);
    expect(controller.getState().paperGrainEnabled).toBe(true);
    expect(bus.getReducedEffects()).toBe(false);

    controller.dispose();
  });

  it("triggers FPS degradation only after 3 consecutive seconds < 30 fps (brief hiccups do NOT trigger)", () => {
    const bus = createPresentationEffectsBus(false);
    const controller = createRenderQualityController({
      effectsBus: bus,
      matchMedia: () => createFakeMediaQuery(false).list,
    });

    // Normal 60 fps for 2s
    let t = simulateFrames(controller, 0, 2000, 60);
    expect(controller.getState().lowFpsDegraded).toBe(false);
    expect(controller.getState().degraded).toBe(false);

    // Brief 2s hiccup at 20 fps (< 3s) followed by recovery to 60 fps
    t = simulateFrames(controller, t + 50, 2000, 20);
    expect(controller.getState().lowFpsDegraded).toBe(false);
    t = simulateFrames(controller, t + 16.67, 1500, 60);
    expect(controller.getState().lowFpsDegraded).toBe(false);
    expect(controller.getState().degraded).toBe(false);

    // Sustained < 30 fps (18 fps) for 2s -> still not degraded
    t = simulateFrames(controller, t + 55, 2000, 18);
    expect(controller.getState().lowFpsDegraded).toBe(false);

    // 3rd consecutive second < 30 fps -> triggers degradation & syncs effectsBus
    simulateFrames(controller, t + 55, 1200, 18);
    expect(controller.getState()).toMatchObject({
      degraded: true,
      paperGrainEnabled: false,
      reducedEffects: true,
      lowFpsDegraded: true,
    });
    expect(bus.getReducedEffects()).toBe(true);

    controller.dispose();
  });

  it("pauses FPS sampling when document.hidden is true so background intervals never count as low FPS", () => {
    const bus = createPresentationEffectsBus(false);
    const vis = createFakeVisibilityTarget(false);
    const controller = createRenderQualityController({
      effectsBus: bus,
      matchMedia: () => createFakeMediaQuery(false).list,
      documentTarget: vis.target,
      getHidden: vis.getHidden,
    });

    // 2 seconds of low FPS before hiding
    let t = simulateFrames(controller, 0, 2000, 20);
    expect(controller.getState().lowFpsDegraded).toBe(false);

    // Tab becomes hidden for 15 seconds; stray frames or huge gap after resume must NOT trigger
    vis.setHidden(true);
    expect(controller.getState().hidden).toBe(true);
    controller.recordFrame(t + 5000);
    controller.recordFrame(t + 10000);

    // Return to visible at t + 15000 and run at 60 fps
    vis.setHidden(false);
    expect(controller.getState().hidden).toBe(false);
    expect(controller.getState().lowFpsDegraded).toBe(false);

    simulateFrames(controller, t + 15000, 1500, 60);
    expect(controller.getState().lowFpsDegraded).toBe(false);
    expect(controller.getState().degraded).toBe(false);
    expect(bus.getReducedEffects()).toBe(false);

    controller.dispose();
    expect(vis.listenerCount()).toBe(0);
  });

  it("ignores idle gaps across canvas unmount/remount when resetSamplingWindow is called", () => {
    const controller = createRenderQualityController({
      matchMedia: () => createFakeMediaQuery(false).list,
    });

    const t = simulateFrames(controller, 0, 1000, 60);
    controller.resetSamplingWindow();

    // 10 seconds pass in Lobby/Finished before next match mounts TableCanvas
    simulateFrames(controller, t + 10000, 1000, 60);
    expect(controller.getState().lowFpsDegraded).toBe(false);
    expect(controller.getState().degraded).toBe(false);

    controller.dispose();
  });
});
