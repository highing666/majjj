import type { PublicPhase } from "../protocol/generated/PublicPhase";
import type { Seat } from "../protocol/generated/Seat";
import type { TablePhase } from "../protocol/generated/TablePhase";

/** Delay before showing the subtle AI long-think breathing indicator (ADR-0014). */
export const AI_LONG_THINK_INDICATOR_DELAY_MS = 2500;

export interface AiThinkingObservation {
  readonly phase: TablePhase;
  readonly publicPhase: PublicPhase | null;
  readonly activeSeat: Seat | null;
  readonly activeSeatIsAi: boolean;
  readonly turnSeq: number;
}

export type AiThinkingListener = (thinkingSeat: Seat | null) => void;

export interface AiThinkingIndicatorController {
  getThinkingSeat(): Seat | null;
  subscribe(listener: AiThinkingListener): () => void;
  observe(observation: AiThinkingObservation): void;
  dispose(): void;
}

/**
 * Pure timer-driven controller that surfaces a subtle thinking indicator on an active
 * AI seat once its turn stays open beyond `AI_LONG_THINK_INDICATOR_DELAY_MS` (2.5s),
 * clearing immediately on any turn, seat-kind, or phase transition.
 */
export function createAiThinkingIndicatorController(): AiThinkingIndicatorController {
  const delayMs = AI_LONG_THINK_INDICATOR_DELAY_MS;

  let thinkingSeat: Seat | null = null;
  let trackedKey: string | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let disposed = false;
  const listeners = new Set<AiThinkingListener>();

  function clearTimer(): void {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function setThinkingSeat(next: Seat | null): void {
    if (thinkingSeat === next) {
      return;
    }
    thinkingSeat = next;
    for (const listener of listeners) {
      listener(thinkingSeat);
    }
  }

  return {
    getThinkingSeat(): Seat | null {
      return thinkingSeat;
    },

    subscribe(listener: AiThinkingListener): () => void {
      listeners.add(listener);
      listener(thinkingSeat);
      return () => {
        listeners.delete(listener);
      };
    },

    observe(observation: AiThinkingObservation): void {
      if (disposed) {
        return;
      }
      const eligible =
        observation.phase === "InMatch" &&
        observation.publicPhase === "Playing" &&
        observation.activeSeat !== null &&
        observation.activeSeatIsAi;

      if (!eligible || observation.activeSeat === null) {
        clearTimer();
        trackedKey = null;
        setThinkingSeat(null);
        return;
      }

      const nextKey = `${observation.activeSeat}:${observation.turnSeq}`;
      if (nextKey === trackedKey) {
        return;
      }

      clearTimer();
      trackedKey = nextKey;
      setThinkingSeat(null);

      const targetSeat = observation.activeSeat;
      timer = setTimeout(() => {
        timer = null;
        if (!disposed && trackedKey === nextKey) {
          setThinkingSeat(targetSeat);
        }
      }, delayMs);
    },

    dispose(): void {
      if (disposed) {
        return;
      }
      disposed = true;
      clearTimer();
      trackedKey = null;
      listeners.clear();
    },
  };
}
