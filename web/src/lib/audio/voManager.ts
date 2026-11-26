import type { Seat } from "../protocol/generated/Seat";
import type {
  AudioBufferLike,
  AudioBufferSourceNodeLike,
  AudioContextLike,
  FetchLike,
  GainNodeLike,
  VoActionCategory,
  VoicePackManifest,
  VoTriggerRequest,
} from "./audioTypes";
import {
  connectWithRelativePan,
  defaultFetch,
  stopSourceQuietly,
} from "./voAudioUtils";
import {
  isValidVoiceSlice,
  parseVoicePackManifest,
  voicePackManifestUrl,
} from "./voicePackSchema";

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
  /**
   * Optional predicate indicating whether the realtime `BanterVoice` track is
   * currently playing. When `true`, `Discard` tile calls yield silently and
   * `sfxGain` ducking remains held until both tracks finish.
   */
  readonly isExternalVoiceActive?: () => boolean;
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

function resolveSpriteAudioUrl(packId: string, audioPath: string): string {
  if (audioPath.startsWith("/") || /^https?:\/\//.test(audioPath)) {
    return audioPath;
  }
  return `/assets/vo/${encodeURIComponent(packId)}/${audioPath}`;
}

function canPreempt(
  active: ActiveVo | null,
  hasActiveEmote: boolean,
  incomingSeat: Seat,
  incomingCategory: VoActionCategory,
  externalVoiceActive: boolean,
): boolean {
  // Human QuickEmote plays immediately on its dedicated concurrent channel without dropping.
  if (incomingCategory === "QuickEmote") {
    return true;
  }
  // Tile discard calls never interrupt any currently playing voice (static, emote, or BanterVoice).
  if (incomingCategory === "Discard") {
    return active === null && !hasActiveEmote && !externalVoiceActive;
  }
  if (active === null) {
    return true;
  }
  if (active.seat === incomingSeat) {
    return true;
  }
  return VO_PRIORITY[incomingCategory] > VO_PRIORITY[active.category];
}

export function rampSfxGain(
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
  const isExternalVoiceActive = options.isExternalVoiceActive ?? (() => false);

  // Map preserves insertion order: oldest = first key, newest = last key.
  const packCache = new Map<string, CachedPack>();
  const inflightPacks = new Map<string, Promise<CachedPack | null>>();

  let activeVo: ActiveVo | null = null;
  const activeEmotes = new Map<Seat, ActiveVo>();
  let nextToken = 1;
  let currentBaseSfxVolume = 1;
  const seatRequestSeq = new Map<string, number>();
  let disposed = false;

  function hasAnyActiveVo(): boolean {
    return activeVo !== null || activeEmotes.size > 0;
  }

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
        const parsed = parseVoicePackManifest(await manifestResponse.json());
        if (!parsed) {
          return null;
        }

        const audioUrl = resolveSpriteAudioUrl(packId, parsed.audio);
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
          manifest: parsed,
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

  function playSlice(
    ctx: AudioContextLike,
    voGain: GainNodeLike,
    sfxGain: GainNodeLike,
    pack: CachedPack,
    request: VoTriggerRequest,
    pan: number,
  ): boolean {
    const slice = pack.manifest.sprites[request.slot];
    if (!isValidVoiceSlice(slice)) {
      return false;
    }
    const externalActive = isExternalVoiceActive();
    if (
      !canPreempt(
        activeVo,
        activeEmotes.size > 0,
        request.seat,
        request.category,
        externalActive,
      )
    ) {
      return false;
    }

    const [startSec, durationSec] = slice;
    const isEmote = request.category === "QuickEmote";
    const wasDucking = hasAnyActiveVo() || externalActive;

    if (isEmote) {
      // QuickEmote interrupts a low-priority tile discard call, but never cuts off Pon/Kong/RobKong/Hu.
      if (activeVo !== null && activeVo.category === "Discard") {
        stopSourceQuietly(activeVo.source);
        activeVo = null;
      }
      const prevSameSeatEmote = activeEmotes.get(request.seat);
      if (prevSameSeatEmote !== undefined) {
        stopSourceQuietly(prevSameSeatEmote.source);
        activeEmotes.delete(request.seat);
      }
    } else if (activeVo !== null) {
      stopSourceQuietly(activeVo.source);
      activeVo = null;
    }

    const target = connectWithRelativePan(ctx, voGain, pan);

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
    if (isEmote) {
      activeEmotes.set(request.seat, entry);
    } else {
      activeVo = entry;
    }

    if (!wasDucking) {
      rampSfxGain(ctx, sfxGain, currentBaseSfxVolume * VO_DUCK_FACTOR);
    }

    source.onended = () => {
      if (isEmote) {
        if (activeEmotes.get(request.seat)?.token !== token) {
          return;
        }
        activeEmotes.delete(request.seat);
      } else {
        if (activeVo?.token !== token) {
          return;
        }
        activeVo = null;
      }
      if (!disposed && !hasAnyActiveVo() && !isExternalVoiceActive()) {
        rampSfxGain(ctx, sfxGain, currentBaseSfxVolume);
      }
    };

    try {
      source.start(ctx.currentTime, startSec, durationSec);
      return true;
    } catch {
      if (isEmote) {
        if (activeEmotes.get(request.seat)?.token === token) {
          activeEmotes.delete(request.seat);
        }
      } else if (activeVo?.token === token) {
        activeVo = null;
      }
      if (!wasDucking && !hasAnyActiveVo() && !isExternalVoiceActive()) {
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
      return hasAnyActiveVo();
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
      if (
        !canPreempt(
          activeVo,
          activeEmotes.size > 0,
          request.seat,
          request.category,
          isExternalVoiceActive(),
        )
      ) {
        return false;
      }

      const cached = packCache.get(request.packId);
      if (cached) {
        touchLru(request.packId, cached);
        return playSlice(ctx, voGain, sfxGain, cached, request, pan);
      }

      const seqKey = `${request.seat}:${request.category === "QuickEmote" ? "emote" : "main"}`;
      const mySeatSeq = (seatRequestSeq.get(seqKey) ?? 0) + 1;
      seatRequestSeq.set(seqKey, mySeatSeq);
      const pack = await loadPack(ctx, request.packId);
      if (disposed || !pack || seatRequestSeq.get(seqKey) !== mySeatSeq) {
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
      const ducked = hasAnyActiveVo() || isExternalVoiceActive();
      const target = ducked ? baseSfxVolume * VO_DUCK_FACTOR : baseSfxVolume;
      sfxGain.gain.setTargetAtTime(target, ctx.currentTime, 0.015);
    },

    stopActive(
      ctx?: AudioContextLike,
      sfxGain?: GainNodeLike,
      baseSfxVolume = 1,
    ): void {
      const hadActive = hasAnyActiveVo();
      if (activeVo !== null) {
        stopSourceQuietly(activeVo.source);
        activeVo = null;
      }
      for (const emote of activeEmotes.values()) {
        stopSourceQuietly(emote.source);
      }
      activeEmotes.clear();
      if (hadActive && ctx && sfxGain && !isExternalVoiceActive()) {
        rampSfxGain(ctx, sfxGain, baseSfxVolume);
      }
    },

    dispose(): void {
      disposed = true;
      if (activeVo !== null) {
        stopSourceQuietly(activeVo.source);
        activeVo = null;
      }
      for (const emote of activeEmotes.values()) {
        stopSourceQuietly(emote.source);
      }
      activeEmotes.clear();
      packCache.clear();
      inflightPacks.clear();
      seatRequestSeq.clear();
    },
  };
}
