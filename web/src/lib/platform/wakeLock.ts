import type { TablePhase } from "../protocol/generated/TablePhase";

export interface WakeLockSentinelLike {
  readonly released?: boolean;
  release(): Promise<void>;
  addEventListener?: (type: string, listener: () => void) => void;
  removeEventListener?: (type: string, listener: () => void) => void;
}

export interface WakeLockLike {
  request(type: "screen"): Promise<WakeLockSentinelLike>;
}

export interface VisibilityDocumentLike {
  addEventListener(type: string, listener: () => void): void;
  removeEventListener(type: string, listener: () => void): void;
}

export interface WakeLockControllerOptions {
  readonly wakeLock?: WakeLockLike | null;
  readonly documentTarget?: VisibilityDocumentLike | null;
  readonly getVisibilityState?: () => DocumentVisibilityState;
}

export interface WakeLockController {
  syncPhase(phase: TablePhase | null): void;
  setMatchActive(active: boolean): void;
  isMatchActive(): boolean;
  isHeld(): boolean;
  dispose(): void;
}

function defaultWakeLock(): WakeLockLike | null {
  if (typeof navigator === "undefined") {
    return null;
  }
  const nav = navigator as unknown as { wakeLock?: WakeLockLike };
  return nav.wakeLock ?? null;
}

function defaultDocumentTarget(): VisibilityDocumentLike | null {
  return typeof document !== "undefined" ? document : null;
}

function defaultVisibilityState(): DocumentVisibilityState {
  return typeof document !== "undefined" ? document.visibilityState : "visible";
}

/**
 * Manages the Screen Wake Lock API during `InMatch` gameplay:
 * acquires on `InMatch`, re-acquires when returning `visible`,
 * releases on `Lobby`/`Finished`/hidden/dispose, and silently falls back when unsupported or denied.
 */
export function createWakeLockController(
  options: WakeLockControllerOptions = {},
): WakeLockController {
  const wakeLockApi =
    options.wakeLock === undefined ? defaultWakeLock() : options.wakeLock;
  const documentTarget =
    options.documentTarget === undefined
      ? defaultDocumentTarget()
      : options.documentTarget;
  const getVisibilityState =
    options.getVisibilityState ?? defaultVisibilityState;

  let matchActive = false;
  let disposed = false;
  let requestInFlight = false;
  let generation = 0;
  let sentinel: WakeLockSentinelLike | null = null;

  const onSentinelRelease = () => {
    sentinel = null;
  };

  function detachSentinelListener(target: WakeLockSentinelLike): void {
    target.removeEventListener?.("release", onSentinelRelease);
  }

  function releaseCurrentSentinel(): void {
    const current = sentinel;
    sentinel = null;
    if (!current) {
      return;
    }
    detachSentinelListener(current);
    try {
      void current.release().catch(() => {});
    } catch {
      // Silent fallback if sentinel release throws synchronously.
    }
  }

  function shouldHoldLock(): boolean {
    return (
      !disposed &&
      matchActive &&
      getVisibilityState() === "visible" &&
      wakeLockApi !== null
    );
  }

  function maybeAcquire(): void {
    if (!shouldHoldLock() || !wakeLockApi) {
      return;
    }
    if (sentinel !== null && !sentinel.released) {
      return;
    }
    if (requestInFlight) {
      return;
    }

    const requestGen = generation;
    requestInFlight = true;
    try {
      void wakeLockApi
        .request("screen")
        .then((acquired) => {
          requestInFlight = false;
          if (requestGen !== generation || !shouldHoldLock()) {
            try {
              void acquired.release().catch(() => {});
            } catch {
              // Silent fallback.
            }
            return;
          }
          sentinel = acquired;
          acquired.addEventListener?.("release", onSentinelRelease);
        })
        .catch(() => {
          requestInFlight = false;
        });
    } catch {
      requestInFlight = false;
    }
  }

  function reconcile(): void {
    if (!shouldHoldLock()) {
      generation += 1;
      releaseCurrentSentinel();
      return;
    }
    maybeAcquire();
  }

  const onVisibilityChange = () => {
    if (disposed) {
      return;
    }
    reconcile();
  };

  documentTarget?.addEventListener("visibilitychange", onVisibilityChange);

  return {
    syncPhase(phase: TablePhase | null): void {
      this.setMatchActive(phase === "InMatch");
    },

    setMatchActive(active: boolean): void {
      if (disposed || matchActive === active) {
        return;
      }
      matchActive = active;
      reconcile();
    },

    isMatchActive(): boolean {
      return matchActive;
    },

    isHeld(): boolean {
      return sentinel !== null && !sentinel.released;
    },

    dispose(): void {
      if (disposed) {
        return;
      }
      disposed = true;
      matchActive = false;
      generation += 1;
      documentTarget?.removeEventListener(
        "visibilitychange",
        onVisibilityChange,
      );
      releaseCurrentSentinel();
    },
  };
}
