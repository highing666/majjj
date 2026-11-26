import type { Seat } from "../protocol/generated/Seat";
import type {
  ActiveBanterVoiceState,
  AudioBufferLike,
  AudioBufferSourceNodeLike,
  AudioContextLike,
  BanterVoicePriority,
  BanterVoiceStateListener,
  FetchLike,
  GainNodeLike,
} from "./audioTypes";
import {
  connectWithRelativePan,
  defaultFetch,
  stopSourceQuietly,
} from "./voAudioUtils";
import { rampSfxGain, VO_DUCK_FACTOR } from "./voManager";

export const BANTER_VOICE_MAX_QUEUE_LENGTH = 2;

export interface BanterVoicePlaybackItem {
  readonly banterId: string;
  readonly seat: Seat;
  readonly audioUrl: string;
  readonly durationMs: number;
  readonly priority: BanterVoicePriority;
  readonly pan: number;
}

export interface BanterVoicePlayerOptions {
  readonly fetchFn?: FetchLike;
  /**
   * Optional predicate indicating whether the static `VoicePack` track is
   * currently playing. Coordinates shared 30ms `sfxGain` ducking so restoring
   * happens only when BOTH tracks are idle.
   */
  readonly isExternalVoiceActive?: () => boolean;
}

export interface BanterVoicePlayer {
  isPlaying(): boolean;
  getActive(): ActiveBanterVoiceState | null;
  getPendingBanterIds(): ReadonlySet<string>;
  getQueueLength(): number;
  subscribe(listener: BanterVoiceStateListener): () => void;
  enqueue(
    ctx: AudioContextLike,
    voGain: GainNodeLike,
    sfxGain: GainNodeLike,
    baseSfxVolume: number,
    item: BanterVoicePlaybackItem,
  ): void;
  updateSfxBaseVolume(baseSfxVolume: number): void;
  flush(): void;
  dispose(): void;
}

interface ActiveBanterSlot {
  readonly token: number;
  readonly item: BanterVoicePlaybackItem;
  readonly ctx: AudioContextLike;
  readonly voGain: GainNodeLike;
  readonly sfxGain: GainNodeLike;
  phase: "loading" | "playing";
  source: AudioBufferSourceNodeLike | null;
}

function isAllowedAudioUrl(audioUrl: string): boolean {
  const trimmed = audioUrl.trim();
  return trimmed.length > 0 && trimmed.startsWith("/");
}

function setsEqual(a: ReadonlySet<string>, b: ReadonlySet<string>): boolean {
  if (a.size !== b.size) {
    return false;
  }
  for (const item of a) {
    if (!b.has(item)) {
      return false;
    }
  }
  return true;
}

/**
 * Manages asynchronous same-origin WAV fetching, Web Audio decoding, bounded
 * cross-seat queuing (max 2 pending items with priority-aware eviction),
 * same-seat preemption, relative stereo panning, shared `sfxGain` ducking,
 * and active `(seat, banterId)` + pending queue notifications for realtime `BanterVoice`.
 */
export function createBanterVoicePlayer(
  options: BanterVoicePlayerOptions = {},
): BanterVoicePlayer {
  const fetchFn = options.fetchFn ?? defaultFetch();
  const isExternalVoiceActive = options.isExternalVoiceActive ?? (() => false);

  const pendingQueue: Array<{
    readonly item: BanterVoicePlaybackItem;
    readonly ctx: AudioContextLike;
    readonly voGain: GainNodeLike;
    readonly sfxGain: GainNodeLike;
  }> = [];
  const listeners = new Set<BanterVoiceStateListener>();

  let active: ActiveBanterSlot | null = null;
  let lastNotified: ActiveBanterVoiceState | null = null;
  let lastNotifiedPendingIds: ReadonlySet<string> = new Set();
  let lastCtx: AudioContextLike | null = null;
  let lastSfxGain: GainNodeLike | null = null;
  let currentBaseSfxVolume = 1;
  let nextToken = 1;
  let flushGeneration = 0;
  let disposed = false;

  function computeActiveState(): ActiveBanterVoiceState | null {
    if (active === null || active.phase !== "playing") {
      return null;
    }
    return {
      seat: active.item.seat,
      banterId: active.item.banterId,
    };
  }

  function computePendingBanterIds(): ReadonlySet<string> {
    const ids = new Set<string>();
    if (active !== null && active.phase === "loading") {
      ids.add(active.item.banterId);
    }
    for (const entry of pendingQueue) {
      ids.add(entry.item.banterId);
    }
    return ids;
  }

  function notifyPlaybackState(): void {
    const next = computeActiveState();
    const nextPending = computePendingBanterIds();
    const sameActive =
      (lastNotified === null && next === null) ||
      (lastNotified !== null &&
        next !== null &&
        lastNotified.seat === next.seat &&
        lastNotified.banterId === next.banterId);
    if (sameActive && setsEqual(lastNotifiedPendingIds, nextPending)) {
      return;
    }
    lastNotified = next;
    lastNotifiedPendingIds = nextPending;
    for (const listener of listeners) {
      listener(next, nextPending);
    }
  }

  function removeQueuedForSeat(seat: Seat): void {
    for (let i = pendingQueue.length - 1; i >= 0; i -= 1) {
      if (pendingQueue[i]!.item.seat === seat) {
        pendingQueue.splice(i, 1);
      }
    }
  }

  function enqueueWithEviction(entry: {
    readonly item: BanterVoicePlaybackItem;
    readonly ctx: AudioContextLike;
    readonly voGain: GainNodeLike;
    readonly sfxGain: GainNodeLike;
  }): void {
    if (pendingQueue.length < BANTER_VOICE_MAX_QUEUE_LENGTH) {
      pendingQueue.push(entry);
      return;
    }

    // Queue full: evict oldest ordinary mid-hand banter first, preserving highlight/settlement.
    const oldestNormalIdx = pendingQueue.findIndex(
      (queued) => queued.item.priority === "normal",
    );
    if (oldestNormalIdx !== -1) {
      pendingQueue.splice(oldestNormalIdx, 1);
      pendingQueue.push(entry);
      return;
    }

    // All queued items are highlight/settlement: drop incoming normal, or evict oldest highlight.
    if (entry.item.priority === "normal") {
      return;
    }
    pendingQueue.shift();
    pendingQueue.push(entry);
  }

  function handleSlotFailed(token: number): void {
    if (disposed || active?.token !== token) {
      return;
    }
    active = null;
    drainNext();
    notifyPlaybackState();
  }

  function startSlot(
    ctx: AudioContextLike,
    voGain: GainNodeLike,
    sfxGain: GainNodeLike,
    item: BanterVoicePlaybackItem,
  ): void {
    if (disposed || !fetchFn || !isAllowedAudioUrl(item.audioUrl)) {
      drainNext();
      notifyPlaybackState();
      return;
    }

    const token = nextToken++;
    const gen = flushGeneration;
    lastCtx = ctx;
    lastSfxGain = sfxGain;

    const slot: ActiveBanterSlot = {
      token,
      item,
      ctx,
      voGain,
      sfxGain,
      phase: "loading",
      source: null,
    };
    active = slot;
    notifyPlaybackState();

    void (async () => {
      let buffer: AudioBufferLike;
      try {
        const response = await fetchFn(item.audioUrl);
        if (disposed || gen !== flushGeneration || active?.token !== token) {
          return;
        }
        if (!response.ok) {
          handleSlotFailed(token);
          return;
        }
        const bytes = await response.arrayBuffer();
        if (disposed || gen !== flushGeneration || active?.token !== token) {
          return;
        }
        buffer = await ctx.decodeAudioData(bytes);
        if (disposed || gen !== flushGeneration || active?.token !== token) {
          return;
        }
      } catch {
        handleSlotFailed(token);
        return;
      }

      if (ctx.state !== "running") {
        handleSlotFailed(token);
        return;
      }

      const target = connectWithRelativePan(ctx, voGain, item.pan);

      const source = ctx.createBufferSource();
      source.buffer = buffer;
      source.connect(target);

      const wasExternalDucking = isExternalVoiceActive();
      slot.phase = "playing";
      slot.source = source;

      if (!wasExternalDucking) {
        rampSfxGain(ctx, sfxGain, currentBaseSfxVolume * VO_DUCK_FACTOR);
      }

      source.onended = () => {
        if (active?.token !== token) {
          return;
        }
        active = null;
        if (!disposed && !isExternalVoiceActive()) {
          rampSfxGain(ctx, sfxGain, currentBaseSfxVolume);
        }
        drainNext();
        notifyPlaybackState();
      };

      try {
        source.start(ctx.currentTime);
      } catch {
        active = null;
        if (!wasExternalDucking && !isExternalVoiceActive()) {
          rampSfxGain(ctx, sfxGain, currentBaseSfxVolume);
        }
        drainNext();
        notifyPlaybackState();
        return;
      }

      notifyPlaybackState();
    })();
  }

  function drainNext(): void {
    if (disposed || active !== null) {
      return;
    }
    const next = pendingQueue.shift();
    if (!next) {
      return;
    }
    startSlot(next.ctx, next.voGain, next.sfxGain, next.item);
  }

  function flushInternal(): void {
    flushGeneration += 1;
    pendingQueue.length = 0;
    const prev = active;
    active = null;

    if (prev !== null) {
      const wasPlaying = prev.phase === "playing";
      if (prev.source !== null) {
        stopSourceQuietly(prev.source);
      }
      if (wasPlaying && !disposed && lastCtx && lastSfxGain && !isExternalVoiceActive()) {
        rampSfxGain(lastCtx, lastSfxGain, currentBaseSfxVolume);
      }
    }
    notifyPlaybackState();
  }

  return {
    isPlaying(): boolean {
      return active !== null && active.phase === "playing";
    },

    getActive: computeActiveState,

    getPendingBanterIds: computePendingBanterIds,

    getQueueLength(): number {
      return pendingQueue.length;
    },

    subscribe(listener: BanterVoiceStateListener): () => void {
      listeners.add(listener);
      listener(computeActiveState(), computePendingBanterIds());
      return () => {
        listeners.delete(listener);
      };
    },

    enqueue(
      ctx: AudioContextLike,
      voGain: GainNodeLike,
      sfxGain: GainNodeLike,
      baseSfxVolume: number,
      item: BanterVoicePlaybackItem,
    ): void {
      if (disposed || !isAllowedAudioUrl(item.audioUrl)) {
        return;
      }
      currentBaseSfxVolume = baseSfxVolume;
      lastCtx = ctx;
      lastSfxGain = sfxGain;

      // Always drop stale pending items from the same seat.
      removeQueuedForSeat(item.seat);

      if (active === null) {
        startSlot(ctx, voGain, sfxGain, item);
        return;
      }

      // Same-seat preemption: stop currently active line for that seat immediately.
      if (active.item.seat === item.seat) {
        const prev = active;
        active = null;
        const wasPlaying = prev.phase === "playing";
        if (prev.source !== null) {
          stopSourceQuietly(prev.source);
        }
        if (wasPlaying && !isExternalVoiceActive()) {
          rampSfxGain(prev.ctx, prev.sfxGain, currentBaseSfxVolume);
        }
        startSlot(ctx, voGain, sfxGain, item);
        return;
      }

      // Cross-seat arrival: enqueue with max-2 bounded priority eviction.
      enqueueWithEviction({ item, ctx, voGain, sfxGain });
      notifyPlaybackState();
    },

    updateSfxBaseVolume(baseSfxVolume: number): void {
      currentBaseSfxVolume = baseSfxVolume;
    },

    flush(): void {
      flushInternal();
    },

    dispose(): void {
      if (disposed) {
        return;
      }
      disposed = true;
      flushInternal();
      listeners.clear();
      lastCtx = null;
      lastSfxGain = null;
    },
  };
}
