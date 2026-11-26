import type { Seat } from "../protocol/generated/Seat";
import type { WinKind } from "../protocol/generated/WinKind";
import type { MeldKind } from "../protocol/melds";
import { seatIndex, seatNumberLabel } from "../protocol/seats";
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

export const IMPACT_DURATION_MS = {
  MeldDeclared: 700,
  HandSettledWin: 1100,
  HandSettledDraw: 600,
} as const;

const PILTOVER_SKEW_DEG = -7;

interface MeldImpactSpec {
  kind: PiltoverImpactKind;
  label: string;
  accentColor: string;
  isKong?: boolean;
}

const MELD_IMPACT_SPEC: Record<MeldKind, MeldImpactSpec> = {
  Pon: { kind: "pon", label: "碰！", accentColor: "#E25343" },
  Chi: { kind: "pon", label: "吃！", accentColor: "#E25343" },
  ExposedKong: {
    kind: "kong",
    label: "明杠！",
    accentColor: "#58D1C9",
    isKong: true,
  },
  AddedKong: {
    kind: "kong",
    label: "补杠！",
    accentColor: "#58D1C9",
    isKong: true,
  },
  ConcealedKong: {
    kind: "kong",
    label: "暗杠！",
    accentColor: "#58D1C9",
    isKong: true,
  },
};

const WIN_KIND_SPEC: Record<
  WinKind,
  { kind: PiltoverImpactKind; label: string }
> = {
  RobKongHu: { kind: "rob_kong", label: "抢杠胡！" },
  Tsumo: { kind: "hu", label: "自摸！" },
  Ron: { kind: "hu", label: "胡！" },
};

function joinSublabel(
  parts: ReadonlyArray<string | null | undefined | false>,
): string {
  return parts.filter((part): part is string => Boolean(part)).join(" · ");
}

function resolveActorText(
  label: string | null | undefined,
  role: SemanticGameEvent["relativeRole"],
  seat: Seat | null,
): string | null {
  return label || role || (seat !== null ? seatNumberLabel(seat) : null);
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
    const spec = MELD_IMPACT_SPEC[event.meldKind];
    const actorSeats = event.seat !== null ? [event.seat] : [];
    const actorDelta =
      spec.isKong && event.seat !== null && event.kongScoreDeltas !== null
        ? event.kongScoreDeltas[seatIndex(event.seat)]
        : 0;
    const kongExtra = actorDelta > 0 ? `+${actorDelta} 杠分` : undefined;

    return {
      id,
      kind: spec.kind,
      label: spec.label,
      sublabel: joinSublabel([
        resolveActorText(event.actorLabel, event.relativeRole, event.seat),
        event.tile !== null && getTileLabel(event.tile),
        kongExtra,
      ]),
      accentColor: spec.accentColor,
      speedlines: true,
      skewDeg: PILTOVER_SKEW_DEG,
      dampedShake: !reducedEffects,
      warmFlash: !reducedEffects,
      seats: actorSeats,
      durationMs: options.durationMs ?? IMPACT_DURATION_MS.MeldDeclared,
      skippable: options.skippable ?? false,
    };
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
        durationMs: options.durationMs ?? IMPACT_DURATION_MS.HandSettledDraw,
        skippable,
      };
    }

    const durationMs = options.durationMs ?? IMPACT_DURATION_MS.HandSettledWin;
    const winnerSeats = event.winners.map((w) => w.seat);

    if (event.isMultiRon) {
      const sublabel = joinSublabel([
        `${event.winners.length} 家胡牌`,
        event.totalFan !== null && `最高 ${event.totalFan} 番`,
        event.tile !== null && getTileLabel(event.tile),
      ]);
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
    const winSpec = primary
      ? WIN_KIND_SPEC[primary.winKind]
      : WIN_KIND_SPEC.Ron;
    const sublabel = joinSublabel([
      primary &&
        resolveActorText(
          primary.displayName || event.actorLabel,
          primary.relativeRole,
          primary.seat,
        ),
      primary && `${primary.totalFan} 番`,
      event.tile !== null && getTileLabel(event.tile),
    ]);

    return {
      id,
      kind: winSpec.kind,
      label: winSpec.label,
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
            ? IMPACT_DURATION_MS.HandSettledDraw
            : IMPACT_DURATION_MS.HandSettledWin
          : IMPACT_DURATION_MS.MeldDeclared;
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
