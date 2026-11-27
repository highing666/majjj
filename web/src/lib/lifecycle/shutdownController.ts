import type { TableSnapshot } from "../protocol/generated/TableSnapshot";

export type ShutdownPhase =
  | "idle"
  | "confirming"
  | "submitting"
  | "stopped"
  | "error";

export interface ShutdownResponseLike {
  ok: boolean;
  status: number;
}

export interface ShutdownRequestInit {
  method?: string;
  headers?: Record<string, string>;
}

export type ShutdownFetch = (
  input: string,
  init?: ShutdownRequestInit,
) => Promise<ShutdownResponseLike>;

export interface ShutdownState {
  visible: boolean;
  phase: ShutdownPhase;
  errorMessage: string | null;
}

export interface ShutdownControllerOptions {
  fetchFn?: ShutdownFetch;
  onShutdownSuccess?: () => void;
}

export type ShutdownListener = (state: ShutdownState) => void;

export interface ShutdownController {
  getState(): ShutdownState;
  subscribe(listener: ShutdownListener): () => void;
  syncHostIsLoopback(hostIsLoopback: boolean): void;
  requestConfirm(): void;
  cancelConfirm(): void;
  confirmShutdown(): Promise<boolean>;
}

export function canShowShutdownControl(
  snapshot: Pick<TableSnapshot, "host_is_loopback"> | null,
): boolean {
  return snapshot?.host_is_loopback === true;
}

function defaultFetch(
  input: string,
  init?: ShutdownRequestInit,
): Promise<ShutdownResponseLike> {
  return fetch(input, init);
}

/**
 * Pure TypeScript state controller for loopback-only `POST /api/shutdown`.
 */
export function createShutdownController(
  options: ShutdownControllerOptions = {},
): ShutdownController {
  const fetchFn = options.fetchFn ?? defaultFetch;
  const onShutdownSuccess = options.onShutdownSuccess;
  const listeners = new Set<ShutdownListener>();

  let hostIsLoopback = false;
  let phase: ShutdownPhase = "idle";
  let errorMessage: string | null = null;

  function snapshotState(): ShutdownState {
    return {
      visible: hostIsLoopback && phase !== "stopped",
      phase,
      errorMessage,
    };
  }

  function emit(): void {
    const next = snapshotState();
    for (const listener of listeners) {
      listener(next);
    }
  }

  return {
    getState: snapshotState,
    subscribe(listener) {
      listeners.add(listener);
      listener(snapshotState());
      return () => {
        listeners.delete(listener);
      };
    },
    syncHostIsLoopback(nextLoopback) {
      if (phase === "stopped") {
        return;
      }
      if (hostIsLoopback === nextLoopback) {
        return;
      }
      hostIsLoopback = nextLoopback;
      if (!hostIsLoopback) {
        phase = "idle";
        errorMessage = null;
      }
      emit();
    },
    requestConfirm() {
      if (!hostIsLoopback || phase === "submitting" || phase === "stopped") {
        return;
      }
      phase = "confirming";
      errorMessage = null;
      emit();
    },
    cancelConfirm() {
      if (phase === "confirming" || phase === "error") {
        phase = "idle";
        errorMessage = null;
        emit();
      }
    },
    async confirmShutdown() {
      if (
        !hostIsLoopback ||
        (phase !== "confirming" && phase !== "error")
      ) {
        return false;
      }
      phase = "submitting";
      errorMessage = null;
      emit();

      try {
        const res = await fetchFn("/api/shutdown", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
        });
        if (res.ok) {
          phase = "stopped";
          errorMessage = null;
          emit();
          onShutdownSuccess?.();
          return true;
        }
        phase = "error";
        errorMessage =
          res.status === 403
            ? "关闭本机服务被拒绝（HTTP 403）：仅支持本机回环桌面模式。"
            : `关闭本机服务失败（HTTP ${res.status}）。`;
        emit();
        return false;
      } catch {
        phase = "error";
        errorMessage = "无法连接本机服务，请检查服务是否仍在运行。";
        emit();
        return false;
      }
    },
  };
}
