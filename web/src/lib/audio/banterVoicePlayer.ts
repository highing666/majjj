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
  FetchResponseLike,
  GainNodeLike,
} from "./audioTypes";
import {
  connectWithRelativePan,
  defaultFetch,
  stopSourceQuietly,
} from "./voAudioUtils";
import { rampSfxGain, VO_DUCK_FACTOR } from "./voManager";

export const BANTER_VOICE_MAX_QUEUE_LENGTH = 2;
export const DEFAULT_BANTER_SAMPLE_RATE = 24000;
export const BANTER_STREAM_SAFETY_LEAD_SEC = 0.02;
export const BANTER_STREAM_CONTINUATION_LEAD_SEC = 0.005;
export const BANTER_EDGE_FADE_SEC = 0.003;

const MIN_VALID_SAMPLE_RATE = 8000;
const MAX_VALID_SAMPLE_RATE = 192000;

export interface BanterVoicePlaybackItem {
  readonly banterId: string;
  readonly seat: Seat;
  readonly audioUrl: string;
  readonly sampleRate?: number;
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
  readonly abortController: AbortController;
  phase: "loading" | "playing";
  slotGain: GainNodeLike | null;
  panTarget: AudioNodeLike | null;
  readonly sources: Set<AudioBufferSourceNodeLike>;
  streamEnded: boolean;
  nextStartTime: number;
  firstFadeInEnd: number;
  reader: ReadableStreamDefaultReader<Uint8Array> | null;
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

function resolveSampleRate(
  response: FetchResponseLike,
  fallbackSampleRate?: number,
): number {
  const headerVal =
    response.headers?.get("X-Audio-Sample-Rate") ??
    response.headers?.get("x-audio-sample-rate");
  if (headerVal) {
    const parsed = Number.parseInt(headerVal.trim(), 10);
    if (
      Number.isFinite(parsed) &&
      parsed >= MIN_VALID_SAMPLE_RATE &&
      parsed <= MAX_VALID_SAMPLE_RATE
    ) {
      return parsed;
    }
  }
  if (
    typeof fallbackSampleRate === "number" &&
    Number.isFinite(fallbackSampleRate) &&
    fallbackSampleRate >= MIN_VALID_SAMPLE_RATE &&
    fallbackSampleRate <= MAX_VALID_SAMPLE_RATE
  ) {
    return Math.round(fallbackSampleRate);
  }
  return DEFAULT_BANTER_SAMPLE_RATE;
}

function pcm16LeToAudioBuffer(
  ctx: AudioContextLike,
  evenBytes: Uint8Array,
  sampleRate: number,
): AudioBufferLike {
  const sampleCount = evenBytes.byteLength >> 1;
  const buffer = ctx.createBuffer(1, sampleCount, sampleRate);
  const channel = buffer.getChannelData(0);
  const view = new DataView(
    evenBytes.buffer,
    evenBytes.byteOffset,
    evenBytes.byteLength,
  );
  for (let i = 0; i < sampleCount; i += 1) {
    channel[i] = view.getInt16(i * 2, true) / 32768;
  }
  return buffer;
}

/**
 * Manages incremental same-origin Raw PCM16 `ReadableStream` playback via Web Audio
 * `AudioBufferSourceNode` sample-accurate scheduling (zero HTTPS / `AudioWorklet`
 * dependency), odd-byte chunk alignment, ~3ms edge fades, bounded cross-seat queuing
 * (max 2 pending items with priority-aware eviction), same-seat preemption with
 * `AbortController`, relative stereo panning, shared 30ms `sfxGain` ducking, and
 * active `(seat, banterId)` + pending queue notifications for realtime `BanterVoice`.
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

  function disconnectSlotGraph(slot: ActiveBanterSlot): void {
    slot.slotGain?.disconnect?.();
    if (slot.panTarget !== null && slot.panTarget !== slot.voGain) {
      slot.panTarget.disconnect?.();
    }
    slot.slotGain = null;
    slot.panTarget = null;
  }

  function stopSlotResources(slot: ActiveBanterSlot): void {
    slot.abortController.abort();
    if (slot.reader !== null) {
      const reader = slot.reader;
      slot.reader = null;
      void reader.cancel().catch(() => {});
    }
    for (const source of slot.sources) {
      stopSourceQuietly(source);
    }
    slot.sources.clear();
    disconnectSlotGraph(slot);
  }

  function scheduleTailFadeOut(slot: ActiveBanterSlot): void {
    if (slot.slotGain === null) {
      return;
    }
    const endTime = slot.nextStartTime;
    const now = slot.ctx.currentTime;
    if (endTime <= now) {
      return;
    }
    const fadeStart = Math.max(
      now,
      slot.firstFadeInEnd,
      endTime - BANTER_EDGE_FADE_SEC,
    );
    if (fadeStart < endTime) {
      slot.slotGain.gain.setValueAtTime(1, fadeStart);
      slot.slotGain.gain.linearRampToValueAtTime(0, endTime);
    }
  }

  function completeActiveSlot(slot: ActiveBanterSlot): void {
    if (active?.token !== slot.token) {
      return;
    }
    active = null;
    disconnectSlotGraph(slot);
    if (!disposed && !isExternalVoiceActive()) {
      rampSfxGain(slot.ctx, slot.sfxGain, currentBaseSfxVolume);
    }
    drainNext();
    notifyPlaybackState();
  }

  function finishActiveStreamTail(slot: ActiveBanterSlot): void {
    if (disposed || active?.token !== slot.token) {
      return;
    }
    slot.streamEnded = true;
    scheduleTailFadeOut(slot);
    if (slot.sources.size === 0) {
      completeActiveSlot(slot);
    }
  }

  function handlePreFirstChunkFailure(token: number): void {
    if (disposed || active?.token !== token) {
      return;
    }
    const failed = active;
    active = null;
    stopSlotResources(failed);
    drainNext();
    notifyPlaybackState();
  }

  function schedulePcmChunk(
    slot: ActiveBanterSlot,
    evenBytes: Uint8Array,
    sampleRate: number,
  ): boolean {
    const buffer = pcm16LeToAudioBuffer(slot.ctx, evenBytes, sampleRate);
    const isFirstChunk = slot.phase === "loading";

    let startAt: number;
    if (isFirstChunk) {
      const panTarget = connectWithRelativePan(
        slot.ctx,
        slot.voGain,
        slot.item.pan,
      );
      const slotGain = slot.ctx.createGain();
      slotGain.connect(panTarget);

      startAt = slot.ctx.currentTime + BANTER_STREAM_SAFETY_LEAD_SEC;
      const fadeInDur = Math.min(BANTER_EDGE_FADE_SEC, buffer.duration * 0.5);
      slotGain.gain.setValueAtTime(0, startAt);
      slotGain.gain.linearRampToValueAtTime(1, startAt + fadeInDur);

      slot.panTarget = panTarget;
      slot.slotGain = slotGain;
      slot.firstFadeInEnd = startAt + fadeInDur;
    } else {
      startAt = Math.max(
        slot.nextStartTime,
        slot.ctx.currentTime + BANTER_STREAM_CONTINUATION_LEAD_SEC,
      );
    }

    const source = slot.ctx.createBufferSource();
    source.buffer = buffer;
    source.connect(slot.slotGain!);
    slot.sources.add(source);

    source.onended = () => {
      slot.sources.delete(source);
      if (active?.token !== slot.token) {
        return;
      }
      if (slot.streamEnded && slot.sources.size === 0) {
        completeActiveSlot(slot);
      }
    };

    try {
      source.start(startAt);
    } catch {
      slot.sources.delete(source);
      if (isFirstChunk) {
        handlePreFirstChunkFailure(slot.token);
      } else {
        finishActiveStreamTail(slot);
      }
      return false;
    }

    slot.nextStartTime = startAt + buffer.duration;

    if (isFirstChunk) {
      slot.phase = "playing";
      if (!isExternalVoiceActive()) {
        rampSfxGain(slot.ctx, slot.sfxGain, currentBaseSfxVolume * VO_DUCK_FACTOR);
      }
      notifyPlaybackState();
    }

    return true;
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
      abortController: new AbortController(),
      phase: "loading",
      slotGain: null,
      panTarget: null,
      sources: new Set(),
      streamEnded: false,
      nextStartTime: 0,
      firstFadeInEnd: 0,
      reader: null,
    };
    active = slot;
    notifyPlaybackState();

    const isStillCurrent = (): boolean =>
      !disposed && gen === flushGeneration && active?.token === token;

    void (async () => {
      let response: FetchResponseLike;
      try {
        response = await fetchFn(item.audioUrl, {
          signal: slot.abortController.signal,
        });
      } catch {
        if (isStillCurrent()) {
          handlePreFirstChunkFailure(token);
        }
        return;
      }

      if (!isStillCurrent()) {
        return;
      }
      if (!response.ok || !response.body) {
        handlePreFirstChunkFailure(token);
        return;
      }

      const sampleRate = resolveSampleRate(response, item.sampleRate);
      const reader = response.body.getReader();
      slot.reader = reader;

      let pendingByte: number | null = null;
      const consumeEvenBytes = (chunk: Uint8Array): Uint8Array | null => {
        if (chunk.byteLength === 0) {
          return null;
        }
        let combined: Uint8Array;
        if (pendingByte !== null) {
          combined = new Uint8Array(chunk.byteLength + 1);
          combined[0] = pendingByte;
          combined.set(chunk, 1);
          pendingByte = null;
        } else {
          combined = chunk;
        }
        if (combined.byteLength < 2) {
          pendingByte = combined[0]!;
          return null;
        }
        if ((combined.byteLength & 1) === 1) {
          pendingByte = combined[combined.byteLength - 1]!;
          return combined.subarray(0, combined.byteLength - 1);
        }
        return combined;
      };

      try {
        while (true) {
          const { done, value } = await reader.read();
          if (!isStillCurrent()) {
            void reader.cancel().catch(() => {});
            return;
          }
          if (done) {
            break;
          }
          if (!value || value.byteLength === 0) {
            continue;
          }
          const evenBytes = consumeEvenBytes(value);
          if (!evenBytes || evenBytes.byteLength === 0) {
            continue;
          }
          if (ctx.state !== "running") {
            void reader.cancel().catch(() => {});
            slot.reader = null;
            if (slot.phase === "loading") {
              handlePreFirstChunkFailure(token);
            } else {
              finishActiveStreamTail(slot);
            }
            return;
          }
          if (!schedulePcmChunk(slot, evenBytes, sampleRate)) {
            void reader.cancel().catch(() => {});
            slot.reader = null;
            return;
          }
        }
      } catch {
        slot.reader = null;
        if (!isStillCurrent()) {
          return;
        }
        if (slot.phase === "loading") {
          handlePreFirstChunkFailure(token);
        } else {
          finishActiveStreamTail(slot);
        }
        return;
      }

      slot.reader = null;
      if (!isStillCurrent()) {
        return;
      }
      if (slot.phase === "loading") {
        // Stream closed with 0 playable PCM samples.
        handlePreFirstChunkFailure(token);
        return;
      }
      finishActiveStreamTail(slot);
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
      stopSlotResources(prev);
      if (
        wasPlaying &&
        !disposed &&
        lastCtx &&
        lastSfxGain &&
        !isExternalVoiceActive()
      ) {
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

      // Same-seat preemption: abort fetch & stop all scheduled sources immediately.
      if (active.item.seat === item.seat) {
        const prev = active;
        active = null;
        const wasPlaying = prev.phase === "playing";
        stopSlotResources(prev);
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
