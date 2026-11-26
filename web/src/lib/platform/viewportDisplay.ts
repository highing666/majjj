import type { TablePhase } from "../protocol/generated/TablePhase";

const SHORT_LANDSCAPE_MAX_HEIGHT = 460;
const NARROW_PORTRAIT_MAX_WIDTH = 600;
const NATIVE_EXIT_ESCAPE_GRACE_MS = 400;

export interface KeyEventTargetLike {
  readonly tagName?: string;
  readonly isContentEditable?: boolean;
}

export interface FullscreenElementLike {
  requestFullscreen?: () => Promise<void> | void;
  webkitRequestFullscreen?: () => Promise<void> | void;
}

export interface FullscreenDocumentLike {
  readonly fullscreenElement?: unknown;
  readonly webkitFullscreenElement?: unknown;
  readonly documentElement?: FullscreenElementLike | null;
  readonly activeElement?: KeyEventTargetLike | null;
  exitFullscreen?: () => Promise<void> | void;
  webkitExitFullscreen?: () => Promise<void> | void;
  addEventListener(type: string, listener: (event?: unknown) => void): void;
  removeEventListener(type: string, listener: (event?: unknown) => void): void;
}

export interface ViewportWindowLike {
  readonly innerWidth: number;
  readonly innerHeight: number;
  addEventListener(type: string, listener: (event?: unknown) => void): void;
  removeEventListener(type: string, listener: (event?: unknown) => void): void;
}

export interface ViewportKeyEventLike {
  readonly key: string;
  readonly ctrlKey?: boolean;
  readonly metaKey?: boolean;
  readonly altKey?: boolean;
  readonly shiftKey?: boolean;
  readonly repeat?: boolean;
  readonly target?: KeyEventTargetLike | null;
  preventDefault?: () => void;
}

export interface ViewportDisplayState {
  width: number;
  height: number;
  isShortLandscape: boolean;
  isNarrowPortrait: boolean;
  phase: TablePhase | null;
  isSeated: boolean;
  hasModalOpen: boolean;
  nativeFullscreenSupported: boolean;
  nativeFullscreen: boolean;
  viewportImmersive: boolean;
  isFullscreen: boolean;
  portraitBypassed: boolean;
  showRotateOverlay: boolean;
}

export interface ViewportDisplayControllerOptions {
  readonly documentTarget?: FullscreenDocumentLike | null;
  readonly windowTarget?: ViewportWindowLike | null;
  readonly now?: () => number;
}

export interface ViewportDisplayController {
  getState(): ViewportDisplayState;
  subscribe(listener: (state: ViewportDisplayState) => void): () => void;
  syncPhase(phase: TablePhase | null, isSeated?: boolean): void;
  setModalOpen(open: boolean): void;
  setViewportSize(width: number, height: number): void;
  toggleFullscreen(): Promise<void>;
  exitFullscreen(): Promise<void>;
  bypassPortrait(): void;
  handleKeyDown(event: ViewportKeyEventLike): boolean;
  shouldSuppressActionBarEscape(): boolean;
  dispose(): void;
}

function defaultDocumentTarget(): FullscreenDocumentLike | null {
  return typeof document !== "undefined"
    ? (document as unknown as FullscreenDocumentLike)
    : null;
}

function defaultWindowTarget(): ViewportWindowLike | null {
  return typeof window !== "undefined"
    ? (window as unknown as ViewportWindowLike)
    : null;
}

function defaultNow(): number {
  return typeof performance !== "undefined" ? performance.now() : Date.now();
}

function isEditableElement(target: KeyEventTargetLike | null | undefined): boolean {
  if (!target) {
    return false;
  }
  const tag = target.tagName?.toUpperCase();
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    Boolean(target.isContentEditable)
  );
}

function normalizeDim(value: number | undefined, fallback: number): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
    return fallback;
  }
  return Math.round(value);
}

function readNativeFullscreenElement(
  doc: FullscreenDocumentLike | null,
): unknown {
  if (!doc) {
    return null;
  }
  return doc.fullscreenElement ?? doc.webkitFullscreenElement ?? null;
}

function hasNativeFullscreenSupport(
  doc: FullscreenDocumentLike | null,
): boolean {
  const el = doc?.documentElement;
  if (!el) {
    return false;
  }
  return (
    typeof el.requestFullscreen === "function" ||
    typeof el.webkitRequestFullscreen === "function"
  );
}

/**
 * Coordinates fullscreen toggling (native Fullscreen API with `100dvh` viewport fallback),
 * `F` shortcut routing, short-landscape detection, and the portrait rotate overlay state machine.
 */
export function createViewportDisplayController(
  options: ViewportDisplayControllerOptions = {},
): ViewportDisplayController {
  const documentTarget =
    options.documentTarget === undefined
      ? defaultDocumentTarget()
      : options.documentTarget;
  const windowTarget =
    options.windowTarget === undefined
      ? defaultWindowTarget()
      : options.windowTarget;
  const nowFn = options.now ?? defaultNow;

  let width = normalizeDim(windowTarget?.innerWidth, 1280);
  let height = normalizeDim(windowTarget?.innerHeight, 720);
  let phase: TablePhase | null = null;
  let isSeated = false;
  let hasModalOpen = false;
  let nativeFullscreen = Boolean(readNativeFullscreenElement(documentTarget));
  let viewportImmersive = false;
  let portraitBypassed = false;
  let lastNativeExitAtMs: number | null = null;
  let programmaticNativeExit = false;
  let disposed = false;

  const listeners = new Set<(state: ViewportDisplayState) => void>();

  function buildState(): ViewportDisplayState {
    const isShortLandscape = height <= SHORT_LANDSCAPE_MAX_HEIGHT;
    const isNarrowPortrait = width < height && width < NARROW_PORTRAIT_MAX_WIDTH;
    const isFullscreen = nativeFullscreen || viewportImmersive;
    const showRotateOverlay =
      phase === "InMatch" && isSeated && isNarrowPortrait && !portraitBypassed;
    return {
      width,
      height,
      isShortLandscape,
      isNarrowPortrait,
      phase,
      isSeated,
      hasModalOpen,
      nativeFullscreenSupported: hasNativeFullscreenSupport(documentTarget),
      nativeFullscreen,
      viewportImmersive,
      isFullscreen,
      portraitBypassed,
      showRotateOverlay,
    };
  }

  let current = buildState();

  function statesEqual(a: ViewportDisplayState, b: ViewportDisplayState): boolean {
    return (
      a.width === b.width &&
      a.height === b.height &&
      a.isShortLandscape === b.isShortLandscape &&
      a.isNarrowPortrait === b.isNarrowPortrait &&
      a.phase === b.phase &&
      a.isSeated === b.isSeated &&
      a.hasModalOpen === b.hasModalOpen &&
      a.nativeFullscreenSupported === b.nativeFullscreenSupported &&
      a.nativeFullscreen === b.nativeFullscreen &&
      a.viewportImmersive === b.viewportImmersive &&
      a.isFullscreen === b.isFullscreen &&
      a.portraitBypassed === b.portraitBypassed &&
      a.showRotateOverlay === b.showRotateOverlay
    );
  }

  function syncAndNotify(): void {
    const next = buildState();
    if (statesEqual(current, next)) {
      return;
    }
    current = next;
    for (const listener of listeners) {
      listener({ ...current });
    }
  }

  function applyViewportSize(nextWidth: number, nextHeight: number): void {
    width = normalizeDim(nextWidth, width);
    height = normalizeDim(nextHeight, height);
    const isNarrowPortrait = width < height && width < NARROW_PORTRAIT_MAX_WIDTH;
    if (!isNarrowPortrait && portraitBypassed) {
      portraitBypassed = false;
    }
    syncAndNotify();
  }

  async function exitNativeFullscreen(): Promise<void> {
    if (!documentTarget) {
      nativeFullscreen = false;
      return;
    }
    programmaticNativeExit = true;
    try {
      if (typeof documentTarget.exitFullscreen === "function") {
        await documentTarget.exitFullscreen();
      } else if (typeof documentTarget.webkitExitFullscreen === "function") {
        await documentTarget.webkitExitFullscreen();
      }
    } catch {
      // Silently ignore host exit rejections.
    } finally {
      programmaticNativeExit = false;
    }
    nativeFullscreen = Boolean(readNativeFullscreenElement(documentTarget));
  }

  async function enterFullscreen(): Promise<void> {
    const docEl = documentTarget?.documentElement;
    const requestFn =
      typeof docEl?.requestFullscreen === "function"
        ? () => docEl.requestFullscreen!()
        : typeof docEl?.webkitRequestFullscreen === "function"
          ? () => docEl.webkitRequestFullscreen!()
          : null;

    if (!requestFn) {
      viewportImmersive = true;
      syncAndNotify();
      return;
    }

    try {
      await requestFn();
      const hasNative = Boolean(readNativeFullscreenElement(documentTarget));
      if (hasNative) {
        nativeFullscreen = true;
        viewportImmersive = false;
      } else {
        // Host resolved without entering native fullscreen -> use 100dvh fallback.
        nativeFullscreen = false;
        viewportImmersive = true;
      }
    } catch {
      nativeFullscreen = Boolean(readNativeFullscreenElement(documentTarget));
      if (!nativeFullscreen) {
        viewportImmersive = true;
      }
    }
    syncAndNotify();
  }

  async function exitFullscreen(): Promise<void> {
    if (disposed) {
      return;
    }
    if (nativeFullscreen || readNativeFullscreenElement(documentTarget)) {
      await exitNativeFullscreen();
    }
    viewportImmersive = false;
    syncAndNotify();
  }

  async function toggleFullscreen(): Promise<void> {
    if (disposed) {
      return;
    }
    if (
      nativeFullscreen ||
      viewportImmersive ||
      Boolean(readNativeFullscreenElement(documentTarget))
    ) {
      await exitFullscreen();
    } else {
      await enterFullscreen();
    }
  }

  function handleKeyDown(event: ViewportKeyEventLike): boolean {
    if (disposed) {
      return false;
    }
    if (
      phase !== "InMatch" ||
      !isSeated ||
      hasModalOpen ||
      event.repeat ||
      event.ctrlKey ||
      event.metaKey ||
      event.altKey
    ) {
      return false;
    }
    if (
      isEditableElement(event.target) ||
      isEditableElement(documentTarget?.activeElement)
    ) {
      return false;
    }
    if (event.key === "f" || event.key === "F") {
      event.preventDefault?.();
      void toggleFullscreen();
      return true;
    }
    return false;
  }

  const onFullscreenChange = () => {
    if (disposed) {
      return;
    }
    const wasNative = nativeFullscreen;
    const hasNative = Boolean(readNativeFullscreenElement(documentTarget));
    nativeFullscreen = hasNative;
    if (wasNative && !hasNative && !programmaticNativeExit) {
      lastNativeExitAtMs = nowFn();
    }
    if (!hasNative && !viewportImmersive) {
      // Ensure state stays synced when native fullscreen is exited externally.
      syncAndNotify();
      return;
    }
    if (hasNative) {
      viewportImmersive = false;
    }
    syncAndNotify();
  };

  const onWindowResize = () => {
    if (disposed || !windowTarget) {
      return;
    }
    applyViewportSize(windowTarget.innerWidth, windowTarget.innerHeight);
  };

  const onWindowKeyDown = (rawEvent?: unknown) => {
    if (!rawEvent || typeof rawEvent !== "object") {
      return;
    }
    handleKeyDown(rawEvent as ViewportKeyEventLike);
  };

  documentTarget?.addEventListener("fullscreenchange", onFullscreenChange);
  documentTarget?.addEventListener("webkitfullscreenchange", onFullscreenChange);
  windowTarget?.addEventListener("resize", onWindowResize);
  windowTarget?.addEventListener("orientationchange", onWindowResize);
  windowTarget?.addEventListener("keydown", onWindowKeyDown);

  return {
    getState(): ViewportDisplayState {
      return { ...current };
    },

    subscribe(listener: (state: ViewportDisplayState) => void): () => void {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },

    syncPhase(nextPhase: TablePhase | null, nextSeated = true): void {
      if (disposed) {
        return;
      }
      phase = nextPhase;
      isSeated = nextSeated;
      if (phase !== "InMatch" || !isSeated) {
        viewportImmersive = false;
      }
      syncAndNotify();
    },

    setModalOpen(open: boolean): void {
      if (disposed) {
        return;
      }
      hasModalOpen = open;
      syncAndNotify();
    },

    setViewportSize(nextWidth: number, nextHeight: number): void {
      if (disposed) {
        return;
      }
      applyViewportSize(nextWidth, nextHeight);
    },

    toggleFullscreen,
    exitFullscreen,

    bypassPortrait(): void {
      if (disposed) {
        return;
      }
      portraitBypassed = true;
      syncAndNotify();
    },

    handleKeyDown,

    shouldSuppressActionBarEscape(): boolean {
      if (nativeFullscreen || Boolean(readNativeFullscreenElement(documentTarget))) {
        return true;
      }
      if (lastNativeExitAtMs !== null) {
        return nowFn() - lastNativeExitAtMs <= NATIVE_EXIT_ESCAPE_GRACE_MS;
      }
      return false;
    },

    dispose(): void {
      if (disposed) {
        return;
      }
      disposed = true;
      documentTarget?.removeEventListener(
        "fullscreenchange",
        onFullscreenChange,
      );
      documentTarget?.removeEventListener(
        "webkitfullscreenchange",
        onFullscreenChange,
      );
      windowTarget?.removeEventListener("resize", onWindowResize);
      windowTarget?.removeEventListener("orientationchange", onWindowResize);
      windowTarget?.removeEventListener("keydown", onWindowKeyDown);
      listeners.clear();
    },
  };
}
