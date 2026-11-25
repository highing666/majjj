import type { Seat } from "../protocol/generated/Seat";
import type {
  AudioBufferLike,
  AudioBufferSourceNodeLike,
  AudioContextLike,
  AudioNodeLike,
  FetchLike,
  GainNodeLike,
  VoActionCategory,
  VoManifest,
  VoTriggerRequest,
} from "./audioTypes";

export const VO_MAX_CACHED_PACKS = 4;
export const VO_DUCK_FACTOR = 0.45;
export const VO_DUCK_TIME_SEC = 0.03;

const VO_PRIORITY: Record<VoActionCategory, number> = {
  Hu: 6,
  RobKong: 5,
  Kong: 4,
  Pon: 3,
  Chi: 2,
  Discard: 1,
};

export interface VoManagerOptions {
  readonly fetchFn?: FetchLike;
  readonly random?: () => number;
}

export interface VoManager {
  getCachedPackIds(): string[];
  isDucking(): boolean;
  triggerVo(
    ctx: AudioContextLike,
    voGain: GainNodeLike,
    sfxGain: GainNodeLike,
    baseSfxVolume: number,
    request: VoTriggerRequest,
    pan?: number,
  ): Promise<boolean>;
  updateSfxBaseVolume(
    ctx: AudioContextLike,
    sfxGain: GainNodeLike,
    baseSfxVolume: number,
  ): void;
  stopActive(
    ctx?: AudioContextLike,
    sfxGain?: GainNodeLike,
    baseSfxVolume?: number,
  ): void;
  dispose(): void;
}

interface CachedPack {
  readonly manifest: VoManifest;
  readonly clips: Map<string, AudioBufferLike>;
}

interface ActiveVo {
  readonly token: number;
  readonly seat: Seat;
  readonly category: VoActionCategory;
  readonly source: AudioBufferSourceNodeLike;
}

function defaultFetch(): FetchLike | null {
  if (typeof globalThis.fetch !== "function") {
    return null;
  }
  return (input: string) => globalThis.fetch(input);
}

function resolveClipUrl(packId: string, clipPath: string): string {
  if (clipPath.startsWith("/") || /^https?:\/\//.test(clipPath)) {
    return clipPath;
  }
  return `/assets/vo/${encodeURIComponent(packId)}/${clipPath}`;
}

function canPreempt(
  active: ActiveVo | null,
  incomingSeat: Seat,
  incomingCategory: VoActionCategory,
): boolean {
  if (active === null) {
    return true;
  }
  if (active.seat === incomingSeat) {
    return true;
  }
  return VO_PRIORITY[incomingCategory] > VO_PRIORITY[active.category];
}

function rampSfxGain(
  ctx: AudioContextLike,
  sfxGain: GainNodeLike,
  targetValue: number,
): void {
  const now = ctx.currentTime;
  sfxGain.gain.cancelScheduledValues?.(now);
  sfxGain.gain.setValueAtTime(sfxGain.gain.value, now);
  sfxGain.gain.linearRampToValueAtTime(targetValue, now + VO_DUCK_TIME_SEC);
}

/**
 * Manages v2 character VO packs (`/assets/vo/<pack_id>/manifest.json`),
 * 4-pack LRU decoded buffer cache, same-seat interruption, cross-seat priority
 * (`Hu > RobKong > Kong > Pon > Chi > Discard`), and 30ms `sfxGain` ducking.
 */
export function createVoManager(options: VoManagerOptions = {}): VoManager {
  const fetchFn = options.fetchFn ?? defaultFetch();
  const random = options.random ?? (() => Math.random());

  // Map preserves insertion order: oldest = first key, newest = last key.
  const packCache = new Map<string, CachedPack>();
  const inflightPacks = new Map<string, Promise<CachedPack | null>>();

  let activeVo: ActiveVo | null = null;
  let nextToken = 1;
  let currentBaseSfxVolume = 1;
  const seatRequestSeq: Partial<Record<Seat, number>> = {};
  let disposed = false;

  function touchLru(packId: string, entry: CachedPack): void {
    packCache.delete(packId);
    packCache.set(packId, entry);
    while (packCache.size > VO_MAX_CACHED_PACKS) {
      const oldestKey = packCache.keys().next().value;
      if (oldestKey === undefined) {
        break;
      }
      packCache.delete(oldestKey);
    }
  }

  async function loadPack(packId: string): Promise<CachedPack | null> {
    const existing = packCache.get(packId);
    if (existing) {
      touchLru(packId, existing);
      return existing;
    }

    const inflight = inflightPacks.get(packId);
    if (inflight) {
      return inflight;
    }

    if (!fetchFn) {
      return null;
    }

    const promise = (async (): Promise<CachedPack | null> => {
      try {
        const manifestUrl = `/assets/vo/${encodeURIComponent(packId)}/manifest.json`;
        const response = await fetchFn(manifestUrl);
        if (!response.ok) {
          return null;
        }
        const parsed = (await response.json()) as Partial<VoManifest> | null;
        if (
          !parsed ||
          typeof parsed !== "object" ||
          typeof parsed.clips !== "object" ||
          parsed.clips === null
        ) {
          return null;
        }
        const entry: CachedPack = {
          manifest: {
            pack_id: parsed.pack_id ?? packId,
            version: typeof parsed.version === "number" ? parsed.version : 2,
            clips: parsed.clips,
          },
          clips: new Map<string, AudioBufferLike>(),
        };
        touchLru(packId, entry);
        return entry;
      } catch {
        return null;
      } finally {
        inflightPacks.delete(packId);
      }
    })();

    inflightPacks.set(packId, promise);
    return promise;
  }

  async function loadClipBuffer(
    ctx: AudioContextLike,
    pack: CachedPack,
    packId: string,
    clipPath: string,
  ): Promise<AudioBufferLike | null> {
    const cached = pack.clips.get(clipPath);
    if (cached) {
      return cached;
    }
    if (!fetchFn) {
      return null;
    }
    try {
      const response = await fetchFn(resolveClipUrl(packId, clipPath));
      if (!response.ok) {
        return null;
      }
      const bytes = await response.arrayBuffer();
      const buffer = await ctx.decodeAudioData(bytes);
      pack.clips.set(clipPath, buffer);
      return buffer;
    } catch {
      return null;
    }
  }

  function stopSourceQuietly(vo: ActiveVo): void {
    vo.source.onended = null;
    try {
      vo.source.stop();
    } catch {
      // Ignore already-stopped source errors.
    }
  }

  return {
    getCachedPackIds(): string[] {
      return [...packCache.keys()];
    },

    isDucking(): boolean {
      return activeVo !== null;
    },

    async triggerVo(
      ctx: AudioContextLike,
      voGain: GainNodeLike,
      sfxGain: GainNodeLike,
      baseSfxVolume: number,
      request: VoTriggerRequest,
      pan = 0,
    ): Promise<boolean> {
      if (disposed || !request.packId) {
        return false;
      }
      currentBaseSfxVolume = baseSfxVolume;
      if (!canPreempt(activeVo, request.seat, request.category)) {
        return false;
      }

      const mySeatSeq = (seatRequestSeq[request.seat] ?? 0) + 1;
      seatRequestSeq[request.seat] = mySeatSeq;
      const pack = await loadPack(request.packId);
      if (disposed || !pack || seatRequestSeq[request.seat] !== mySeatSeq) {
        return false;
      }

      const candidates = pack.manifest.clips[request.category];
      if (!Array.isArray(candidates) || candidates.length === 0) {
        return false;
      }
      const idx = Math.min(
        candidates.length - 1,
        Math.max(0, Math.floor(random() * candidates.length)),
      );
      const clipPath = candidates[idx];
      if (!clipPath) {
        return false;
      }

      const buffer = await loadClipBuffer(ctx, pack, request.packId, clipPath);
      if (disposed || !buffer || seatRequestSeq[request.seat] !== mySeatSeq) {
        return false;
      }

      if (!canPreempt(activeVo, request.seat, request.category)) {
        return false;
      }

      const wasDucking = activeVo !== null;
      if (activeVo !== null) {
        stopSourceQuietly(activeVo);
        activeVo = null;
      }

      let target: AudioNodeLike = voGain;
      if (pan !== 0 && typeof ctx.createStereoPanner === "function") {
        const panner = ctx.createStereoPanner();
        panner.pan.value = Math.max(-1, Math.min(1, pan));
        panner.connect(voGain);
        target = panner;
      }

      const token = nextToken++;
      const source = ctx.createBufferSource();
      source.buffer = buffer;
      source.connect(target);

      const entry: ActiveVo = {
        token,
        seat: request.seat,
        category: request.category,
        source,
      };
      activeVo = entry;

      if (!wasDucking) {
        rampSfxGain(ctx, sfxGain, currentBaseSfxVolume * VO_DUCK_FACTOR);
      }

      source.onended = () => {
        if (activeVo?.token !== token) {
          return;
        }
        activeVo = null;
        if (!disposed) {
          rampSfxGain(ctx, sfxGain, currentBaseSfxVolume);
        }
      };

      source.start(ctx.currentTime);
      return true;
    },

    updateSfxBaseVolume(
      ctx: AudioContextLike,
      sfxGain: GainNodeLike,
      baseSfxVolume: number,
    ): void {
      currentBaseSfxVolume = baseSfxVolume;
      const target =
        activeVo !== null ? baseSfxVolume * VO_DUCK_FACTOR : baseSfxVolume;
      sfxGain.gain.setTargetAtTime(target, ctx.currentTime, 0.015);
    },

    stopActive(
      ctx?: AudioContextLike,
      sfxGain?: GainNodeLike,
      baseSfxVolume = 1,
    ): void {
      if (activeVo !== null) {
        stopSourceQuietly(activeVo);
        activeVo = null;
        if (ctx && sfxGain) {
          rampSfxGain(ctx, sfxGain, baseSfxVolume);
        }
      }
    },

    dispose(): void {
      disposed = true;
      if (activeVo !== null) {
        stopSourceQuietly(activeVo);
        activeVo = null;
      }
      packCache.clear();
      inflightPacks.clear();
    },
  };
}
