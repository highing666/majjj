import type { PresentationScheduler } from "../presentation/presentationQueue";
import type { QuickEmote } from "../protocol/generated/QuickEmote";
import { QUICK_EMOTE_COOLDOWN_MS } from "../protocol/personas";

export interface QuickEmoteCooldownState {
  readonly onCooldown: boolean;
  readonly remainingMs: number;
  readonly lastEmote: QuickEmote | null;
}

export interface QuickEmoteCooldownOptions {
  cooldownMs?: number;
  tickMs?: number;
  now?: () => number;
  scheduler?: PresentationScheduler;
  onSend: (emote: QuickEmote) => boolean;
}

export type QuickEmoteCooldownListener = (
  state: QuickEmoteCooldownState,
) => void;

export interface QuickEmoteCooldownController {
  getState(): QuickEmoteCooldownState;
  subscribe(listener: QuickEmoteCooldownListener): () => void;
  trigger(emote: QuickEmote): boolean;
  reset(): void;
  dispose(): void;
}

const DEFAULT_TICK_MS = 200;

const defaultScheduler: PresentationScheduler = {
  setTimeout: (cb, ms) => globalThis.setTimeout(cb, ms),
  clearTimeout: (handle) =>
    globalThis.clearTimeout(handle as ReturnType<typeof setTimeout>),
};

/**
 * Pure client-side quick-emote cooldown controller mirroring the server's 4-second
 * per-seat cooldown so rapid taps show live countdown feedback without spamming WS.
 */
export function createQuickEmoteCooldown(
  options: QuickEmoteCooldownOptions,
): QuickEmoteCooldownController {
  const cooldownMs = Math.max(1, options.cooldownMs ?? QUICK_EMOTE_COOLDOWN_MS);
  const tickMs = Math.max(25, options.tickMs ?? DEFAULT_TICK_MS);
  const now = options.now ?? (() => Date.now());
  const scheduler = options.scheduler ?? defaultScheduler;
  const onSend = options.onSend;

  let expiresAtMs: number | null = null;
  let lastEmote: QuickEmote | null = null;
  let timerHandle: unknown | null = null;
  let disposed = false;

  const listeners = new Set<QuickEmoteCooldownListener>();

  function computeState(): QuickEmoteCooldownState {
    if (expiresAtMs === null) {
      return {
        onCooldown: false,
        remainingMs: 0,
        lastEmote,
      };
    }
    const remaining = Math.max(0, expiresAtMs - now());
    if (remaining <= 0) {
      return {
        onCooldown: false,
        remainingMs: 0,
        lastEmote,
      };
    }
    return {
      onCooldown: true,
      remainingMs: remaining,
      lastEmote,
    };
  }

  function emit(): void {
    const snapshot = computeState();
    for (const listener of listeners) {
      listener(snapshot);
    }
  }

  function clearTimer(): void {
    if (timerHandle !== null) {
      scheduler.clearTimeout(timerHandle);
      timerHandle = null;
    }
  }

  function scheduleNextTick(): void {
    clearTimer();
    if (disposed || expiresAtMs === null) {
      return;
    }
    const remaining = expiresAtMs - now();
    if (remaining <= 0) {
      expiresAtMs = null;
      emit();
      return;
    }
    const delay = Math.min(tickMs, remaining);
    timerHandle = scheduler.setTimeout(() => {
      timerHandle = null;
      if (disposed) {
        return;
      }
      if (expiresAtMs !== null && expiresAtMs - now() <= 0) {
        expiresAtMs = null;
        emit();
        return;
      }
      emit();
      scheduleNextTick();
    }, delay);
  }

  return {
    getState: computeState,

    subscribe(listener: QuickEmoteCooldownListener): () => void {
      listeners.add(listener);
      listener(computeState());
      return () => {
        listeners.delete(listener);
      };
    },

    trigger(emote: QuickEmote): boolean {
      if (disposed) {
        return false;
      }
      if (expiresAtMs !== null && expiresAtMs - now() > 0) {
        return false;
      }
      const sent = onSend(emote);
      if (!sent) {
        return false;
      }
      lastEmote = emote;
      expiresAtMs = now() + cooldownMs;
      emit();
      scheduleNextTick();
      return true;
    },

    reset(): void {
      clearTimer();
      expiresAtMs = null;
      lastEmote = null;
      emit();
    },

    dispose(): void {
      if (disposed) {
        return;
      }
      disposed = true;
      clearTimer();
      listeners.clear();
    },
  };
}

export function formatCooldownSeconds(remainingMs: number): string {
  if (remainingMs <= 0) {
    return "";
  }
  return `${Math.ceil(remainingMs / 1000)}s`;
}
