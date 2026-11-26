import type { TtsStatusNoticeState } from "../session/reducer";

export type TtsNoticeKind = "degraded" | "recovered";

export interface TtsNoticeViewState {
  readonly visible: boolean;
  readonly kind: TtsNoticeKind | null;
  readonly message: string;
}

export interface TtsNoticeControllerOptions {
  readonly recoveryDurationMs?: number;
}

export type TtsNoticeListener = (state: TtsNoticeViewState) => void;

export interface TtsNoticeController {
  getState(): TtsNoticeViewState;
  subscribe(listener: TtsNoticeListener): () => void;
  applyNotice(notice: TtsStatusNoticeState | null): void;
  dispose(): void;
}

const DEFAULT_RECOVERY_DURATION_MS = 3500;

const HIDDEN_STATE: TtsNoticeViewState = {
  visible: false,
  kind: null,
  message: "",
};

/**
 * Manages the non-blocking TTS degradation & auto-dismiss recovery toast state
 * so brief recovery notices clear cleanly without interrupting gameplay.
 */
export function createTtsNoticeController(
  options: TtsNoticeControllerOptions = {},
): TtsNoticeController {
  const recoveryDurationMs = Math.max(
    1,
    options.recoveryDurationMs ?? DEFAULT_RECOVERY_DURATION_MS,
  );

  let state: TtsNoticeViewState = HIDDEN_STATE;
  let wasDegraded = false;
  let dismissTimer: ReturnType<typeof setTimeout> | null = null;
  let disposed = false;
  const listeners = new Set<TtsNoticeListener>();

  function clearDismissTimer(): void {
    if (dismissTimer !== null) {
      clearTimeout(dismissTimer);
      dismissTimer = null;
    }
  }

  function emit(next: TtsNoticeViewState): void {
    state = next;
    for (const listener of listeners) {
      listener(state);
    }
  }

  return {
    getState(): TtsNoticeViewState {
      return state;
    },

    subscribe(listener: TtsNoticeListener): () => void {
      listeners.add(listener);
      listener(state);
      return () => {
        listeners.delete(listener);
      };
    },

    applyNotice(notice: TtsStatusNoticeState | null): void {
      if (disposed || notice === null) {
        return;
      }

      if (!notice.available) {
        clearDismissTimer();
        wasDegraded = true;
        emit({
          visible: true,
          kind: "degraded",
          message: notice.message,
        });
        return;
      }

      if (!wasDegraded) {
        return;
      }

      wasDegraded = false;
      clearDismissTimer();
      emit({
        visible: true,
        kind: "recovered",
        message: notice.message,
      });
      dismissTimer = setTimeout(() => {
        dismissTimer = null;
        if (!disposed) {
          emit(HIDDEN_STATE);
        }
      }, recoveryDurationMs);
    },

    dispose(): void {
      if (disposed) {
        return;
      }
      disposed = true;
      clearDismissTimer();
      listeners.clear();
    },
  };
}
