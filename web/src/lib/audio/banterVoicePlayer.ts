import type { Seat } from "../protocol/generated/Seat";
import type {
  ActiveBanterVoiceState,
  AudioBufferLike,
  AudioBufferSourceNodeLike,
  AudioContextLike,
  AudioNodeLike,
  BanterVoicePriority,
  BanterVoiceStateListener,
  FetchLike,
  GainNodeLike,
} from "./audioTypes";
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

function defaultFetch(): FetchLike | null {
  if (typeof globalThis.fetch !== "function") {
    return null;
  }
  return (input: string) => globalThis.fetch(input);
}

function isAllowedAudioUrl(audioUrl: string): boolean {
  const trimmed = audioUrl.trim();
  return trimmed.length > 0 && trimmed.startsWith("/");
}

function stopSourceQuietly(source: AudioBufferSourceNodeLike): void {
  source.onended = null;
  try {
    source.stop();
  } catch {
    // Ignore already-stopped source errors.
  }
}

/**
 * Manages asynchronous same-origin WAV fetching, Web Audio decoding, bounded
 * cross-seat queuing (max 2 pending items with priority-aware eviction),
 * same-seat preemption, relative stereo panning, shared `sfxGain` ducking,
 * and active `(seat, banterId)` state notifications for realtime `BanterVoice`.
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

  function notifySpeakingState(): void {
    const next = computeActiveState();
    const same =
      (lastNotified === null && next === null) ||
      (lastNotified !== null &&
        next !== null &&
        lastNotified.seat === next.seat &&
        lastNotified.banterId === next.banterId);
    if (same) {
      return;
    }
    lastNotified = next;
    for (const listener of listeners) {
      listener(next);
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
    notifySpeakingState();
    drainNext();
  }

  function startSlot(
    ctx: AudioContextLike,
    voGain: GainNodeLike,
    sfxGain: GainNodeLike,
    item: BanterVoicePlaybackItem,
  ): void {
    if (disposed || !fetchFn || !isAllowedAudioUrl(item.audioUrl)) {
      drainNext();
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

      let target: AudioNodeLike = voGain;
      if (item.pan !== 0 && typeof ctx.createStereoPanner === "function") {
        const panner = ctx.createStereoPanner();
        panner.pan.value = Math.max(-1, Math.min(1, item.pan));
        panner.connect(voGain);
        target = panner;
      }

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
        notifySpeakingState();
        if (!disposed && !isExternalVoiceActive()) {
          rampSfxGain(ctx, sfxGain, currentBaseSfxVolume);
        }
        drainNext();
      };

      try {
        source.start(ctx.currentTime);
      } catch {
        active = null;
        notifySpeakingState();
        if (!wasExternalDucking && !isExternalVoiceActive()) {
          rampSfxGain(ctx, sfxGain, currentBaseSfxVolume);
        }
        drainNext();
        return;
      }

      notifySpeakingState();
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
      if (wasPlaying) {
        notifySpeakingState();
        if (!disposed && lastCtx && lastSfxGain && !isExternalVoiceActive()) {
          rampSfxGain(lastCtx, lastSfxGain, currentBaseSfxVolume);
        }
      }
    }
  }

  return {
    isPlaying(): boolean {
      return active !== null && active.phase === "playing";
    },

    getActive: computeActiveState,

    getQueueLength(): number {
      return pendingQueue.length;
    },

    subscribe(listener: BanterVoiceStateListener): () => void {
      listeners.add(listener);
      listener(computeActiveState());
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
        if (wasPlaying) {
          notifySpeakingState();
          if (!isExternalVoiceActive()) {
            rampSfxGain(prev.ctx, prev.sfxGain, currentBaseSfxVolume);
          }
        }
        startSlot(ctx, voGain, sfxGain, item);
        return;
      }

      // Cross-seat arrival: enqueue with max-2 bounded priority eviction.
      enqueueWithEviction({ item, ctx, voGain, sfxGain });
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
