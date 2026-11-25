import type { Seat } from "../protocol/generated/Seat";
import { seatIndex, seatLabel } from "../protocol/seats";
import { getTileLabel } from "../table/tiles";
import type { SemanticGameEvent } from "./semanticEvents";

export type PiltoverImpactKind =
  | "pon"
  | "kong"
  | "hu"
  | "rob_kong"
  | "multi_ron"
  | "draw_game";

export interface PiltoverImpactFrame {
  id: number;
  kind: PiltoverImpactKind;
  label: string;
  sublabel: string;
  accentColor: string;
  speedlines: boolean;
  skewDeg: number;
  dampedShake: boolean;
  warmFlash: boolean;
  seats: readonly Seat[];
  durationMs: number;
  skippable: boolean;
}

export interface EmitEffectOptions {
  durationMs?: number;
  skippable?: boolean;
}

export type EffectsBusMessage =
  | {
      type: "semantic_event";
      event: SemanticGameEvent;
      durationMs: number;
      skippable: boolean;
      reducedEffects: boolean;
    }
  | { type: "clear" }
  | { type: "reduced_effects"; reducedEffects: boolean };

export type EffectsBusSubscriber = (message: EffectsBusMessage) => void;

export interface PresentationEffectsBus {
  subscribe(subscriber: EffectsBusSubscriber): () => void;
  emit(event: SemanticGameEvent, options?: EmitEffectOptions): void;
  clear(): void;
  setReducedEffects(reduced: boolean): void;
  getReducedEffects(): boolean;
  dispose(): void;
}

export interface PiltoverImpactCallbacks {
  onImpact: (frame: PiltoverImpactFrame) => void;
  onClear?: () => void;
  onReducedEffectsChange?: (reduced: boolean) => void;
}

const DEFAULT_MELD_DURATION_MS = 700;
const DEFAULT_WIN_DURATION_MS = 1100;
const DEFAULT_DRAW_GAME_DURATION_MS = 600;
const PILTOVER_SKEW_DEG = -7;

function formatActorSublabel(
  event: SemanticGameEvent,
  extra?: string,
): string {
  const parts: string[] = [];
  if (event.seat !== null) {
    parts.push(seatLabel(event.seat));
  }
  if (event.tile !== null) {
    parts.push(getTileLabel(event.tile));
  }
  if (extra) {
    parts.push(extra);
  }
  return parts.join(" · ");
}

/**
 * Pure mapper producing a v1 Piltover comic impact frame descriptor from a
 * `SemanticGameEvent`, or `null` when the event has no comic burst (such as `AddedKongAttempted`).
 */
export function buildPiltoverImpactFrame(
  event: SemanticGameEvent,
  options: {
    id: number;
    reducedEffects: boolean;
    durationMs?: number;
    skippable?: boolean;
  },
): PiltoverImpactFrame | null {
  const { id, reducedEffects } = options;

  if (event.type === "MeldDeclared" && event.meldKind !== null) {
    const durationMs = options.durationMs ?? DEFAULT_MELD_DURATION_MS;
    const skippable = options.skippable ?? false;
    const actorSeats = event.seat !== null ? [event.seat] : [];
    const actorDelta =
      event.seat !== null && event.kongScoreDeltas !== null
        ? event.kongScoreDeltas[seatIndex(event.seat)]
        : 0;
    const kongExtra = actorDelta > 0 ? `+${actorDelta} 杠分` : undefined;

    switch (event.meldKind) {
      case "Pon":
        return {
          id,
          kind: "pon",
          label: "碰！",
          sublabel: formatActorSublabel(event),
          accentColor: "#E25343",
          speedlines: true,
          skewDeg: PILTOVER_SKEW_DEG,
          dampedShake: !reducedEffects,
          warmFlash: !reducedEffects,
          seats: actorSeats,
          durationMs,
          skippable,
        };
      case "ExposedKong":
      case "AddedKong":
      case "ConcealedKong": {
        const label =
          event.meldKind === "ExposedKong"
            ? "明杠！"
            : event.meldKind === "AddedKong"
              ? "补杠！"
              : "暗杠！";
        return {
          id,
          kind: "kong",
          label,
          sublabel: formatActorSublabel(event, kongExtra),
          accentColor: "#58D1C9",
          speedlines: true,
          skewDeg: PILTOVER_SKEW_DEG,
          dampedShake: !reducedEffects,
          warmFlash: !reducedEffects,
          seats: actorSeats,
          durationMs,
          skippable,
        };
      }
      case "Chi":
        return {
          id,
          kind: "pon",
          label: "吃！",
          sublabel: formatActorSublabel(event),
          accentColor: "#E25343",
          speedlines: true,
          skewDeg: PILTOVER_SKEW_DEG,
          dampedShake: !reducedEffects,
          warmFlash: !reducedEffects,
          seats: actorSeats,
          durationMs,
          skippable,
        };
    }
  }

  if (event.type === "HandSettled") {
    const skippable = options.skippable ?? true;
    if (event.isDrawGame) {
      return {
        id,
        kind: "draw_game",
        label: "荒庄",
        sublabel: "牌墙已空",
        accentColor: "#9FB2B6",
        speedlines: false,
        skewDeg: -4,
        dampedShake: false,
        warmFlash: false,
        seats: [],
        durationMs: options.durationMs ?? DEFAULT_DRAW_GAME_DURATION_MS,
        skippable,
      };
    }

    const durationMs = options.durationMs ?? DEFAULT_WIN_DURATION_MS;
    const winnerSeats = event.winners.map((w) => w.seat);

    if (event.isMultiRon) {
      const fanText =
        event.totalFan !== null ? `最高 ${event.totalFan} 番` : "";
      const sublabel = [
        `${event.winners.length} 家胡牌`,
        fanText,
        event.tile !== null ? getTileLabel(event.tile) : "",
      ]
        .filter(Boolean)
        .join(" · ");
      return {
        id,
        kind: "multi_ron",
        label: "一炮多响！",
        sublabel,
        accentColor: "#F3A93B",
        speedlines: true,
        skewDeg: PILTOVER_SKEW_DEG,
        dampedShake: !reducedEffects,
        warmFlash: !reducedEffects,
        seats: winnerSeats,
        durationMs,
        skippable,
      };
    }

    const primary = event.winners[0];
    const isRobKong = primary?.winKind === "RobKongHu";
    const isTsumo = primary?.winKind === "Tsumo";
    const label = isRobKong ? "抢杠胡！" : isTsumo ? "自摸！" : "胡！";
    const subParts: string[] = [];
    if (primary) {
      subParts.push(seatLabel(primary.seat));
      subParts.push(`${primary.totalFan} 番`);
    }
    if (event.tile !== null) {
      subParts.push(getTileLabel(event.tile));
    }

    return {
      id,
      kind: isRobKong ? "rob_kong" : "hu",
      label,
      sublabel: subParts.join(" · "),
      accentColor: "#F3A93B",
      speedlines: true,
      skewDeg: PILTOVER_SKEW_DEG,
      dampedShake: !reducedEffects,
      warmFlash: !reducedEffects,
      seats: winnerSeats,
      durationMs,
      skippable,
    };
  }

  return null;
}

/**
 * Registers the v1 Piltover comic impact subscriber on `bus`.
 */
export function registerPiltoverImpactSubscriber(
  bus: PresentationEffectsBus,
  callbacks: PiltoverImpactCallbacks,
): () => void {
  let nextId = 1;
  return bus.subscribe((message) => {
    switch (message.type) {
      case "semantic_event": {
        const frame = buildPiltoverImpactFrame(message.event, {
          id: nextId,
          reducedEffects: message.reducedEffects,
          durationMs: message.durationMs,
          skippable: message.skippable,
        });
        if (frame !== null) {
          nextId += 1;
          callbacks.onImpact(frame);
        }
        break;
      }
      case "clear":
        callbacks.onClear?.();
        break;
      case "reduced_effects":
        callbacks.onReducedEffectsChange?.(message.reducedEffects);
        break;
    }
  });
}

/**
 * Creates a pub/sub `PresentationEffectsBus` supporting multiple subscribers
 * (v1 Piltover impact frames and v2 Cut-in / big-fan hooks).
 */
export function createPresentationEffectsBus(
  initialReducedEffects = false,
): PresentationEffectsBus {
  let reducedEffects = initialReducedEffects;
  const subscribers = new Set<EffectsBusSubscriber>();

  function publish(message: EffectsBusMessage): void {
    for (const subscriber of subscribers) {
      subscriber(message);
    }
  }

  return {
    subscribe(subscriber: EffectsBusSubscriber): () => void {
      subscribers.add(subscriber);
      return () => {
        subscribers.delete(subscriber);
      };
    },

    emit(event: SemanticGameEvent, options: EmitEffectOptions = {}): void {
      const defaultDuration =
        event.type === "HandSettled"
          ? event.isDrawGame
            ? DEFAULT_DRAW_GAME_DURATION_MS
            : DEFAULT_WIN_DURATION_MS
          : DEFAULT_MELD_DURATION_MS;
      publish({
        type: "semantic_event",
        event,
        durationMs: options.durationMs ?? defaultDuration,
        skippable: options.skippable ?? event.type === "HandSettled",
        reducedEffects,
      });
    },

    clear(): void {
      publish({ type: "clear" });
    },

    setReducedEffects(reduced: boolean): void {
      if (reducedEffects === reduced) {
        return;
      }
      reducedEffects = reduced;
      publish({ type: "reduced_effects", reducedEffects });
    },

    getReducedEffects(): boolean {
      return reducedEffects;
    },

    dispose(): void {
      subscribers.clear();
    },
  };
}
