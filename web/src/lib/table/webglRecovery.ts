export type WebGLRecoveryStatus =
  | "active"
  | "context_lost"
  | "restoring"
  | "unrecoverable";

export interface WebGLRecoveryState {
  status: WebGLRecoveryStatus;
  restoreCount: number;
  needsRefresh: boolean;
  errorMessage: string | null;
}

export interface WebGLContextEventLike {
  preventDefault?: () => void;
}

export interface WebGLCanvasTargetLike {
  addEventListener(
    type: string,
    listener: (event: WebGLContextEventLike) => void,
  ): void;
  removeEventListener(
    type: string,
    listener: (event: WebGLContextEventLike) => void,
  ): void;
}

export interface WebGLRecoveryOptions {
  /** Rebuilds `@2x` atlas, felt textures, `PaperGrainFilter`, and re-renders latest scene. */
  readonly rebuildResources: () => Promise<void>;
  /** Maximum consecutive/total automatic restorations before prompting user refresh. */
  readonly maxRestores?: number;
}

export interface WebGLRecoveryController {
  getState(): WebGLRecoveryState;
  subscribe(listener: (state: WebGLRecoveryState) => void): () => void;
  bindCanvas(canvas: WebGLCanvasTargetLike): () => void;
  handleContextLost(event?: WebGLContextEventLike): void;
  handleContextRestored(): Promise<void>;
  reportUnrecoverable(detail?: string): void;
  dispose(): void;
}

const DEFAULT_MAX_RESTORES = 5;

function formatUnrecoverableMessage(detail?: string): string {
  const prefix = detail ? `${detail}：` : "";
  return `${prefix}WebGL 渲染上下文无法恢复，请刷新页面以通过全量 TableSnapshot 无损续打当前手牌。`;
}

/**
 * Manages `webglcontextlost` / `webglcontextrestored` self-healing and unrecoverable
 * error banner state so players can refresh and resume losslessly via `TableSnapshot`.
 */
export function createWebGLRecoveryController(
  options: WebGLRecoveryOptions,
): WebGLRecoveryController {
  const maxRestores = options.maxRestores ?? DEFAULT_MAX_RESTORES;
  const listeners = new Set<(state: WebGLRecoveryState) => void>();

  let disposed = false;
  let epoch = 0;
  let state: WebGLRecoveryState = {
    status: "active",
    restoreCount: 0,
    needsRefresh: false,
    errorMessage: null,
  };

  function updateState(next: WebGLRecoveryState): void {
    state = next;
    for (const listener of listeners) {
      listener({ ...state });
    }
  }

  function transitionUnrecoverable(detail?: string): void {
    updateState({
      status: "unrecoverable",
      restoreCount: state.restoreCount,
      needsRefresh: true,
      errorMessage: formatUnrecoverableMessage(detail),
    });
  }

  function handleContextLost(event?: WebGLContextEventLike): void {
    event?.preventDefault?.();
    if (disposed || state.status === "unrecoverable") {
      return;
    }
    epoch += 1;
    updateState({
      status: "context_lost",
      restoreCount: state.restoreCount,
      needsRefresh: false,
      errorMessage: null,
    });
  }

  async function handleContextRestored(): Promise<void> {
    if (disposed || state.status === "unrecoverable") {
      return;
    }
    if (state.restoreCount >= maxRestores) {
      transitionUnrecoverable("WebGL 上下文反复丢失");
      return;
    }

    const restoreEpoch = epoch;
    updateState({
      status: "restoring",
      restoreCount: state.restoreCount,
      needsRefresh: false,
      errorMessage: null,
    });

    try {
      await options.rebuildResources();
      if (disposed || restoreEpoch !== epoch || state.status !== "restoring") {
        return;
      }
      updateState({
        status: "active",
        restoreCount: state.restoreCount + 1,
        needsRefresh: false,
        errorMessage: null,
      });
    } catch (err) {
      if (disposed || restoreEpoch !== epoch) {
        return;
      }
      const detail = err instanceof Error ? err.message : undefined;
      transitionUnrecoverable(detail);
    }
  }

  return {
    getState(): WebGLRecoveryState {
      return { ...state };
    },

    subscribe(listener: (next: WebGLRecoveryState) => void): () => void {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },

    bindCanvas(canvas: WebGLCanvasTargetLike): () => void {
      const onLost = (event: WebGLContextEventLike) => {
        handleContextLost(event);
      };
      const onRestored = () => {
        void handleContextRestored();
      };
      canvas.addEventListener("webglcontextlost", onLost);
      canvas.addEventListener("webglcontextrestored", onRestored);
      return () => {
        canvas.removeEventListener("webglcontextlost", onLost);
        canvas.removeEventListener("webglcontextrestored", onRestored);
      };
    },

    handleContextLost,
    handleContextRestored,

    reportUnrecoverable(detail?: string): void {
      if (disposed) {
        return;
      }
      epoch += 1;
      transitionUnrecoverable(detail);
    },

    dispose(): void {
      if (disposed) {
        return;
      }
      disposed = true;
      epoch += 1;
      listeners.clear();
    },
  };
}
