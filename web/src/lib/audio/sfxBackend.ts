import sfxManifestJson from "../../assets/audio/sfx-sprite.json";
import sfxSpriteUrl from "../../assets/audio/sfx-sprite.m4a";
import type {
  AudioBufferLike,
  AudioContextLike,
  AudioNodeLike,
  FetchLike,
  SfxBackendMode,
  SfxPlaybackOptions,
  SfxPlaybackReceipt,
  SfxSlotId,
  SfxSpriteManifest,
} from "./audioTypes";
import { createProceduralSfx, type ProceduralSfx } from "./proceduralSfx";

export interface SfxBackendOptions {
  readonly spriteUrl?: string;
  readonly manifest?: SfxSpriteManifest;
  readonly fetchFn?: FetchLike;
  readonly random?: () => number;
  readonly procedural?: ProceduralSfx;
}

export interface SfxBackend {
  getMode(): SfxBackendMode;
  ensureLoaded(ctx: AudioContextLike): Promise<"sprite" | "procedural">;
  play(
    ctx: AudioContextLike,
    destination: AudioNodeLike,
    slot: SfxSlotId,
    options?: SfxPlaybackOptions,
  ): SfxPlaybackReceipt;
  dispose(): void;
}

const JITTER_SPAN = 0.08; // +-4% around 1.0 => [0.96, 1.04]

function clampPan(pan: number | undefined): number {
  if (typeof pan !== "number" || !Number.isFinite(pan)) {
    return 0;
  }
  return Math.max(-1, Math.min(1, pan));
}

function routeWithPan(
  ctx: AudioContextLike,
  destination: AudioNodeLike,
  pan: number,
): AudioNodeLike {
  if (pan === 0 || typeof ctx.createStereoPanner !== "function") {
    return destination;
  }
  const panner = ctx.createStereoPanner();
  panner.pan.value = pan;
  panner.connect(destination);
  return panner;
}

function defaultFetch(): FetchLike | null {
  if (typeof globalThis.fetch !== "function") {
    return null;
  }
  return (input: string) => globalThis.fetch(input);
}

/**
 * Dual-layer SFX backend: decodes the bundled AAC `.m4a` AudioSprite as primary channel
 * with random variant selection + `+-4%` pitch jitter, transparently falling back 1:1
 * to native Web Audio procedural synthesis on missing asset or decode failure.
 */
export function createSfxBackend(options: SfxBackendOptions = {}): SfxBackend {
  const spriteUrl = options.spriteUrl ?? sfxSpriteUrl;
  const manifest =
    options.manifest ?? (sfxManifestJson as unknown as SfxSpriteManifest);
  const fetchFn = options.fetchFn ?? defaultFetch();
  const random = options.random ?? (() => Math.random());
  const procedural = options.procedural ?? createProceduralSfx();

  let mode: SfxBackendMode = "uninitialized";
  let spriteBuffer: AudioBufferLike | null = null;
  let loadPromise: Promise<"sprite" | "procedural"> | null = null;
  let loadedForCtx: AudioContextLike | null = null;
  let disposed = false;

  async function loadInternal(
    ctx: AudioContextLike,
  ): Promise<"sprite" | "procedural"> {
    if (!fetchFn) {
      mode = "procedural";
      return "procedural";
    }
    try {
      const response = await fetchFn(spriteUrl);
      if (!response.ok) {
        mode = "procedural";
        return "procedural";
      }
      const bytes = await response.arrayBuffer();
      if (disposed) {
        mode = "procedural";
        return "procedural";
      }
      const decoded = await ctx.decodeAudioData(bytes);
      if (disposed) {
        mode = "procedural";
        return "procedural";
      }
      spriteBuffer = decoded;
      mode = "sprite";
      return "sprite";
    } catch {
      spriteBuffer = null;
      mode = "procedural";
      return "procedural";
    }
  }

  return {
    getMode(): SfxBackendMode {
      return mode;
    },

    ensureLoaded(ctx: AudioContextLike): Promise<"sprite" | "procedural"> {
      if (disposed) {
        return Promise.resolve("procedural");
      }
      if (loadedForCtx === ctx && loadPromise !== null) {
        return loadPromise;
      }
      loadedForCtx = ctx;
      spriteBuffer = null;
      loadPromise = loadInternal(ctx);
      return loadPromise;
    },

    play(
      ctx: AudioContextLike,
      destination: AudioNodeLike,
      slot: SfxSlotId,
      playOptions: SfxPlaybackOptions = {},
    ): SfxPlaybackReceipt {
      const pan = clampPan(playOptions.pan);
      const target = routeWithPan(ctx, destination, pan);
      const clips = manifest.slots[slot];

      if (
        mode === "sprite" &&
        spriteBuffer !== null &&
        Array.isArray(clips) &&
        clips.length > 0
      ) {
        const variantIndex = Math.min(
          clips.length - 1,
          Math.max(0, Math.floor(random() * clips.length)),
        );
        const [startSec, durationSec] = clips[variantIndex]!;
        const playbackRate =
          playOptions.playbackRate ?? 1 - JITTER_SPAN / 2 + random() * JITTER_SPAN;

        const source = ctx.createBufferSource();
        source.buffer = spriteBuffer;
        source.playbackRate.value = playbackRate;
        source.connect(target);
        source.start(ctx.currentTime, startSec, durationSec);

        return {
          backend: "sprite",
          slot,
          variantIndex,
          playbackRate,
          pan,
        };
      }

      const playbackRate =
        playOptions.playbackRate ?? 1 - JITTER_SPAN / 2 + random() * JITTER_SPAN;
      procedural.playSlot(ctx, target, slot, playbackRate);
      return {
        backend: "procedural",
        slot,
        variantIndex: 0,
        playbackRate,
        pan,
      };
    },

    dispose(): void {
      disposed = true;
      spriteBuffer = null;
      loadPromise = null;
      loadedForCtx = null;
      procedural.dispose();
    },
  };
}
