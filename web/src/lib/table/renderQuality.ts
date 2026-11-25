import type { PresentationEffectsBus } from "../presentation/effectsBus";

export interface MediaQueryListLike {
  matches: boolean;
  addEventListener?: (
    type: string,
    listener: (event: { matches: boolean }) => void,
  ) => void;
  removeEventListener?: (
    type: string,
    listener: (event: { matches: boolean }) => void,
  ) => void;
  addListener?: (listener: (event: { matches: boolean }) => void) => void;
  removeListener?: (listener: (event: { matches: boolean }) => void) => void;
}

export interface VisibilityTargetLike {
  addEventListener(type: string, listener: () => void): void;
  removeEventListener(type: string, listener: () => void): void;
}

export interface RenderQualityState {
  degraded: boolean;
  paperGrainEnabled: boolean;
  reducedEffects: boolean;
  prefersReducedMotion: boolean;
  manualReducedEffects: boolean;
  lowFpsDegraded: boolean;
  hidden: boolean;
  lastFps: number | null;
}

export interface RenderQualityOptions {
  readonly effectsBus?: PresentationEffectsBus;
  readonly initialManualReducedEffects?: boolean;
  readonly matchMedia?: (query: string) => MediaQueryListLike | null;
  readonly documentTarget?: VisibilityTargetLike | null;
  readonly getHidden?: () => boolean;
  readonly now?: () => number;
  readonly fpsThreshold?: number;
  readonly consecutiveLowFpsMs?: number;
  readonly sampleWindowMs?: number;
}

export interface RenderQualityController {
  getState(): RenderQualityState;
  subscribe(listener: (state: RenderQualityState) => void): () => void;
  setManualReducedEffects(reduced: boolean): void;
  setHidden(hidden: boolean): void;
  resetSamplingWindow(): void;
  recordFrame(nowMs?: number): void;
  dispose(): void;
}

const DEFAULT_FPS_THRESHOLD = 30;
const DEFAULT_CONSECUTIVE_LOW_FPS_MS = 3000;
const DEFAULT_SAMPLE_WINDOW_MS = 500;

function defaultMatchMedia(query: string): MediaQueryListLike | null {
  if (typeof window !== "undefined" && typeof window.matchMedia === "function") {
    return window.matchMedia(query);
  }
  return null;
}

function defaultDocumentTarget(): VisibilityTargetLike | null {
  return typeof document !== "undefined" ? document : null;
}

function defaultGetHidden(): boolean {
  return typeof document !== "undefined" ? document.hidden : false;
}

function defaultNow(): number {
  return typeof performance !== "undefined" ? performance.now() : Date.now();
}

/**
 * Coordinates the 3-source render quality degradation policy:
 * (1) `prefers-reduced-motion: reduce`, (2) manual `reduced_effects` preference,
 * and (3) rolling FPS `< 30 fps` for 3 consecutive seconds (paused while hidden).
 */
export function createRenderQualityController(
  options: RenderQualityOptions = {},
): RenderQualityController {
  const fpsThreshold = options.fpsThreshold ?? DEFAULT_FPS_THRESHOLD;
  const consecutiveLowFpsMs =
    options.consecutiveLowFpsMs ?? DEFAULT_CONSECUTIVE_LOW_FPS_MS;
  const sampleWindowMs = options.sampleWindowMs ?? DEFAULT_SAMPLE_WINDOW_MS;
  const nowFn = options.now ?? defaultNow;
  const getHiddenFn = options.getHidden ?? defaultGetHidden;
  const matchMediaFn = options.matchMedia ?? defaultMatchMedia;
  const documentTarget =
    options.documentTarget === undefined
      ? defaultDocumentTarget()
      : options.documentTarget;

  const mediaQuery = matchMediaFn("(prefers-reduced-motion: reduce)");
  let prefersReducedMotion = Boolean(mediaQuery?.matches);
  let manualReducedEffects = Boolean(options.initialManualReducedEffects);
  let lowFpsDegraded = false;
  let hidden = getHiddenFn();
  let lastFps: number | null = null;

  let windowStartMs: number | null = null;
  let windowFrames = 0;
  let lowFpsStreakMs = 0;
  let disposed = false;

  const listeners = new Set<(state: RenderQualityState) => void>();

  function buildState(): RenderQualityState {
    const degraded =
      prefersReducedMotion || manualReducedEffects || lowFpsDegraded;
    return {
      degraded,
      paperGrainEnabled: !degraded,
      reducedEffects: degraded,
      prefersReducedMotion,
      manualReducedEffects,
      lowFpsDegraded,
      hidden,
      lastFps,
    };
  }

  let current = buildState();
  options.effectsBus?.setReducedEffects(current.reducedEffects);

  function syncAndNotify(forceNotify = false): void {
    const next = buildState();
    const changed =
      forceNotify ||
      next.degraded !== current.degraded ||
      next.prefersReducedMotion !== current.prefersReducedMotion ||
      next.manualReducedEffects !== current.manualReducedEffects ||
      next.lowFpsDegraded !== current.lowFpsDegraded ||
      next.hidden !== current.hidden;
    current = next;
    options.effectsBus?.setReducedEffects(current.reducedEffects);
    if (changed) {
      for (const listener of listeners) {
        listener({ ...current });
      }
    }
  }

  const onMediaChange = (event: { matches: boolean }) => {
    if (disposed) {
      return;
    }
    prefersReducedMotion = Boolean(event.matches);
    syncAndNotify();
  };

  if (mediaQuery) {
    if (typeof mediaQuery.addEventListener === "function") {
      mediaQuery.addEventListener("change", onMediaChange);
    } else if (typeof mediaQuery.addListener === "function") {
      mediaQuery.addListener(onMediaChange);
    }
  }

  function applyHidden(nextHidden: boolean): void {
    if (hidden === nextHidden) {
      return;
    }
    hidden = nextHidden;
    // Reset the active sampling window so background pauses never count as low FPS.
    windowStartMs = null;
    windowFrames = 0;
    syncAndNotify();
  }

  const onVisibilityChange = () => {
    if (disposed) {
      return;
    }
    applyHidden(getHiddenFn());
  };

  documentTarget?.addEventListener("visibilitychange", onVisibilityChange);

  return {
    getState(): RenderQualityState {
      return { ...current };
    },

    subscribe(listener: (state: RenderQualityState) => void): () => void {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },

    setManualReducedEffects(reduced: boolean): void {
      if (disposed || manualReducedEffects === reduced) {
        return;
      }
      manualReducedEffects = reduced;
      syncAndNotify();
    },

    setHidden(nextHidden: boolean): void {
      if (disposed) {
        return;
      }
      applyHidden(nextHidden);
    },

    resetSamplingWindow(): void {
      if (disposed) {
        return;
      }
      windowStartMs = null;
      windowFrames = 0;
      lowFpsStreakMs = 0;
    },

    recordFrame(nowMs = nowFn()): void {
      if (disposed || hidden || !Number.isFinite(nowMs)) {
        return;
      }
      if (windowStartMs === null || nowMs < windowStartMs) {
        windowStartMs = nowMs;
        windowFrames = 0;
        return;
      }

      windowFrames += 1;
      const elapsedMs = nowMs - windowStartMs;
      if (elapsedMs < sampleWindowMs) {
        return;
      }

      const fps = (windowFrames * 1000) / elapsedMs;
      lastFps = fps;
      windowStartMs = nowMs;
      windowFrames = 0;

      if (fps < fpsThreshold) {
        lowFpsStreakMs += elapsedMs;
        if (lowFpsStreakMs >= consecutiveLowFpsMs && !lowFpsDegraded) {
          lowFpsDegraded = true;
          syncAndNotify();
        }
      } else {
        lowFpsStreakMs = 0;
      }
    },

    dispose(): void {
      if (disposed) {
        return;
      }
      disposed = true;
      if (mediaQuery) {
        if (typeof mediaQuery.removeEventListener === "function") {
          mediaQuery.removeEventListener("change", onMediaChange);
        } else if (typeof mediaQuery.removeListener === "function") {
          mediaQuery.removeListener(onMediaChange);
        }
      }
      documentTarget?.removeEventListener("visibilitychange", onVisibilityChange);
      listeners.clear();
    },
  };
}
