import type { Seat } from "../protocol/generated/Seat";
import type {
  AudioBufferLike,
  AudioBufferSourceNodeLike,
  AudioContextLike,
  AudioNodeLike,
  FetchLike,
  GainNodeLike,
  VoActionCategory,
  VoicePackManifest,
  VoiceSpriteSlice,
  VoTriggerRequest,
} from "./audioTypes";
import { voicePackManifestUrl } from "./voicePackSchema";

export const VO_MAX_CACHED_PACKS = 4;
export const VO_DUCK_FACTOR = 0.45;
export const VO_DUCK_TIME_SEC = 0.03;

const VO_PRIORITY: Record<VoActionCategory, number> = {
  Hu: 6,
  RobKong: 5,
  Kong: 4,
  Pon: 3,
  QuickEmote: 2,
  Chi: 2,
  Discard: 1,
};

export interface VoManagerOptions {
  readonly fetchFn?: FetchLike;
}

export interface VoManager {
  getCachedPackIds(): string[];
  isDucking(): boolean;
  preloadPack(ctx: AudioContextLike, packId: string): Promise<boolean>;
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
  readonly manifest: VoicePackManifest;
  readonly buffer: AudioBufferLike;
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

function resolveSpriteAudioUrl(packId: string, audioPath: string): string {
  if (audioPath.startsWith("/") || /^https?:\/\//.test(audioPath)) {
    return audioPath;
  }
  return `/assets/vo/${encodeURIComponent(packId)}/${audioPath}`;
}

function isValidSlice(slice: unknown): slice is VoiceSpriteSlice {
  return (
    Array.isArray(slice) &&
    slice.length === 2 &&
    typeof slice[0] === "number" &&
    typeof slice[1] === "number" &&
    Number.isFinite(slice[0]) &&
    Number.isFinite(slice[1]) &&
    slice[0] >= 0 &&
    slice[1] > 0
  );
}

function canPreempt(
  active: ActiveVo | null,
  incomingSeat: Seat,
  incomingCategory: VoActionCategory,
): boolean {
  if (active === null) {
    return true;
  }
  // Tile discard calls never interrupt any currently playing voice.
  if (incomingCategory === "Discard") {
    return false;
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
 * Manages offline static `VoicePack` (`角色语音包`) AudioSprites
 * (`/assets/vo/<pack_id>/manifest.json` + single mono `.m4a`), 4-pack LRU decoded
 * buffer cache, zero-delay slice playback, priority arbitration, relative stereo
 * panning, and 30ms `sfxGain` ducking.
 */
export function createVoManager(options: VoManagerOptions = {}): VoManager {
  const fetchFn = options.fetchFn ?? defaultFetch();

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

  async function loadPack(
    ctx: AudioContextLike,
    packId: string,
  ): Promise<CachedPack | null> {
    const existing = packCache.get(packId);
    if (existing) {
      touchLru(packId, existing);
      return existing;
    }

    const inflight = inflightPacks.get(packId);
    if (inflight) {
      return inflight;
    }

    if (!fetchFn || disposed) {
      return null;
    }

    const promise = (async (): Promise<CachedPack | null> => {
      try {
        const manifestResponse = await fetchFn(voicePackManifestUrl(packId));
        if (!manifestResponse.ok || disposed) {
          return null;
        }
        const parsed = (await manifestResponse.json()) as Partial<VoicePackManifest> | null;
        if (
          !parsed ||
          typeof parsed !== "object" ||
          typeof parsed.audio !== "string" ||
          parsed.audio.trim().length === 0 ||
          typeof parsed.sprites !== "object" ||
          parsed.sprites === null
        ) {
          return null;
        }

        const audioUrl = resolveSpriteAudioUrl(packId, parsed.audio.trim());
        const audioResponse = await fetchFn(audioUrl);
        if (!audioResponse.ok || disposed) {
          return null;
        }
        const bytes = await audioResponse.arrayBuffer();
        if (disposed) {
          return null;
        }
        const buffer = await ctx.decodeAudioData(bytes);
        if (disposed) {
          return null;
        }

        const entry: CachedPack = {
          manifest: {
            pack_id: parsed.pack_id ?? packId,
            version: typeof parsed.version === "number" ? parsed.version : 1,
            audio: parsed.audio.trim(),
            sample_rate:
              typeof parsed.sample_rate === "number"
                ? parsed.sample_rate
                : buffer.sampleRate,
            duration:
              typeof parsed.duration === "number"
                ? parsed.duration
                : buffer.duration,
            sprites: parsed.sprites,
          },
          buffer,
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

  function stopSourceQuietly(vo: ActiveVo): void {
    vo.source.onended = null;
    try {
      vo.source.stop();
    } catch {
      // Ignore already-stopped source errors.
    }
  }

  function playSlice(
    ctx: AudioContextLike,
    voGain: GainNodeLike,
    sfxGain: GainNodeLike,
    pack: CachedPack,
    request: VoTriggerRequest,
    pan: number,
  ): boolean {
    const slice = pack.manifest.sprites[request.slot];
    if (!isValidSlice(slice)) {
      return false;
    }
    if (!canPreempt(activeVo, request.seat, request.category)) {
      return false;
    }

    const [startSec, durationSec] = slice;
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
    source.buffer = pack.buffer;
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

    try {
      source.start(ctx.currentTime, startSec, durationSec);
      return true;
    } catch {
      activeVo = null;
      if (!wasDucking) {
        rampSfxGain(ctx, sfxGain, currentBaseSfxVolume);
      }
      return false;
    }
  }

  return {
    getCachedPackIds(): string[] {
      return [...packCache.keys()];
    },

    isDucking(): boolean {
      return activeVo !== null;
    },

    async preloadPack(ctx: AudioContextLike, packId: string): Promise<boolean> {
      if (disposed || !packId) {
        return false;
      }
      const pack = await loadPack(ctx, packId);
      return pack !== null;
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

      const cached = packCache.get(request.packId);
      if (cached) {
        touchLru(request.packId, cached);
        return playSlice(ctx, voGain, sfxGain, cached, request, pan);
      }

      const mySeatSeq = (seatRequestSeq[request.seat] ?? 0) + 1;
      seatRequestSeq[request.seat] = mySeatSeq;
      const pack = await loadPack(ctx, request.packId);
      if (disposed || !pack || seatRequestSeq[request.seat] !== mySeatSeq) {
        return false;
      }

      return playSlice(ctx, voGain, sfxGain, pack, request, pan);
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
