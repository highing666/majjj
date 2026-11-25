import { describe, expect, it, vi } from "vitest";
import sfxManifestJson from "../../assets/audio/sfx-sprite.json";
import {
  ALL_SFX_SLOTS,
  type AudioBufferLike,
  type AudioBufferSourceNodeLike,
  type AudioContextLike,
  type AudioNodeLike,
  type AudioParamLike,
  type BiquadFilterNodeLike,
  type DynamicsCompressorNodeLike,
  type FetchLike,
  type GainNodeLike,
  type OscillatorNodeLike,
  type SfxSpriteManifest,
  type StereoPannerNodeLike,
} from "./audioTypes";
import { createSfxBackend } from "./sfxBackend";

function makeFakeParam(initial = 0): AudioParamLike {
  const param: AudioParamLike = {
    value: initial,
    setValueAtTime: vi.fn((v: number) => {
      param.value = v;
    }),
    linearRampToValueAtTime: vi.fn((v: number) => {
      param.value = v;
    }),
    exponentialRampToValueAtTime: vi.fn((v: number) => {
      param.value = v;
    }),
    setTargetAtTime: vi.fn((v: number) => {
      param.value = v;
    }),
    cancelScheduledValues: vi.fn(),
  };
  return param;
}

function createFakeSfxContext(opts?: {
  decodeShouldReject?: boolean;
}): {
  ctx: AudioContextLike;
  destination: AudioNodeLike;
  bufferSources: Array<{
    node: AudioBufferSourceNodeLike;
    connectedTo: AudioNodeLike[];
    startArgs: Array<[number?, number?, number?]>;
  }>;
  oscillators: Array<{
    node: OscillatorNodeLike;
    connectedTo: AudioNodeLike[];
  }>;
  panners: Array<{ node: StereoPannerNodeLike; panValue: number }>;
} {
  const destination: AudioNodeLike = { connect: vi.fn() };
  const bufferSources: Array<{
    node: AudioBufferSourceNodeLike;
    connectedTo: AudioNodeLike[];
    startArgs: Array<[number?, number?, number?]>;
  }> = [];
  const oscillators: Array<{
    node: OscillatorNodeLike;
    connectedTo: AudioNodeLike[];
  }> = [];
  const panners: Array<{ node: StereoPannerNodeLike; panValue: number }> = [];

  const ctx: AudioContextLike = {
    state: "running",
    currentTime: 1.5,
    sampleRate: 44100,
    destination,
    resume: vi.fn(async () => {}),
    createGain: (): GainNodeLike => ({
      gain: makeFakeParam(1),
      connect: vi.fn(),
    }),
    createDynamicsCompressor: (): DynamicsCompressorNodeLike => ({
      threshold: makeFakeParam(-10),
      knee: makeFakeParam(18),
      ratio: makeFakeParam(6),
      attack: makeFakeParam(0.002),
      release: makeFakeParam(0.12),
      connect: vi.fn(),
    }),
    createStereoPanner: (): StereoPannerNodeLike => {
      const panParam = makeFakeParam(0);
      const entry = {
        node: {
          pan: panParam,
          connect: vi.fn(),
        },
        get panValue() {
          return panParam.value;
        },
      };
      panners.push(entry);
      return entry.node;
    },
    createBuffer: (_ch, length, sampleRate): AudioBufferLike => ({
      sampleRate,
      length,
      duration: length / sampleRate,
      getChannelData: () => new Float32Array(length),
    }),
    createBufferSource: (): AudioBufferSourceNodeLike => {
      const connectedTo: AudioNodeLike[] = [];
      const startArgs: Array<[number?, number?, number?]> = [];
      const node: AudioBufferSourceNodeLike = {
        buffer: null,
        playbackRate: makeFakeParam(1),
        onended: null,
        connect: vi.fn((dest: AudioNodeLike) => {
          connectedTo.push(dest);
          return dest;
        }),
        start: vi.fn((when?: number, offset?: number, duration?: number) => {
          startArgs.push([when, offset, duration]);
        }),
        stop: vi.fn(),
      };
      bufferSources.push({ node, connectedTo, startArgs });
      return node;
    },
    createOscillator: (): OscillatorNodeLike => {
      const connectedTo: AudioNodeLike[] = [];
      const node: OscillatorNodeLike = {
        type: "sine",
        frequency: makeFakeParam(440),
        onended: null,
        connect: vi.fn((dest: AudioNodeLike) => {
          connectedTo.push(dest);
          return dest;
        }),
        start: vi.fn(),
        stop: vi.fn(),
      };
      oscillators.push({ node, connectedTo });
      return node;
    },
    createBiquadFilter: (): BiquadFilterNodeLike => ({
      type: "bandpass",
      frequency: makeFakeParam(1000),
      Q: makeFakeParam(1),
      connect: vi.fn(),
    }),
    decodeAudioData: vi.fn(async () => {
      if (opts?.decodeShouldReject) {
        throw new Error("decode failed");
      }
      return {
        sampleRate: 44100,
        length: 44100 * 9,
        duration: 9,
        getChannelData: () => new Float32Array(44100 * 9),
      };
    }),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  };

  return { ctx, destination, bufferSources, oscillators, panners };
}

describe("SfxBackend + ProceduralFallback", () => {
  it("bundles a valid sfx-sprite.json covering all 15 semantic slots with variants on core tile actions", () => {
    const manifest = sfxManifestJson as unknown as SfxSpriteManifest;
    expect(manifest.duration).toBeGreaterThan(0);
    expect(manifest.duration).toBeLessThan(15);
    for (const slot of ALL_SFX_SLOTS) {
      expect(manifest.slots[slot]).toBeDefined();
      expect(manifest.slots[slot].length).toBeGreaterThanOrEqual(1);
    }
    expect(manifest.slots.draw.length).toBe(3);
    expect(manifest.slots.discard_light.length).toBe(3);
    expect(manifest.slots.discard_heavy.length).toBe(3);
    expect(manifest.slots.meld_pon.length).toBeGreaterThanOrEqual(2);
  });

  it("loads AudioSprite, selects random variant, applies +-4% playbackRate jitter, and pans via StereoPannerNode", async () => {
    const fake = createFakeSfxContext();
    const fetchFn: FetchLike = vi.fn(async () => ({
      ok: true,
      status: 200,
      json: async () => ({}),
      arrayBuffer: async () => new ArrayBuffer(16),
    }));

    // Deterministic RNG sequence: variant index ~0.8 (3rd variant of 3), jitter ~0.0 (-4%) then 0.999 (+4%)
    const rngValues = [0.8, 0.0, 0.1, 1.0];
    let rngCursor = 0;
    const backend = createSfxBackend({
      spriteUrl: "/assets/sfx-sprite.m4a",
      manifest: sfxManifestJson as unknown as SfxSpriteManifest,
      fetchFn,
      random: () => rngValues[rngCursor++ % rngValues.length]!,
    });

    const mode = await backend.ensureLoaded(fake.ctx);
    expect(mode).toBe("sprite");
    expect(backend.getMode()).toBe("sprite");

    const r1 = backend.play(fake.ctx, fake.destination, "discard_light", {
      pan: -0.35,
    });
    expect(r1.backend).toBe("sprite");
    expect(r1.variantIndex).toBe(2);
    expect(r1.playbackRate).toBeCloseTo(0.96, 5);
    expect(r1.pan).toBeCloseTo(-0.35, 5);

    const src1 = fake.bufferSources[0]!;
    expect(src1.node.playbackRate.value).toBeCloseTo(0.96, 5);
    const clip2 = (sfxManifestJson as unknown as SfxSpriteManifest).slots
      .discard_light[2]!;
    expect(src1.startArgs[0]).toEqual([1.5, clip2[0], clip2[1]]);
    expect(fake.panners[0]!.panValue).toBeCloseTo(-0.35, 5);

    const r2 = backend.play(fake.ctx, fake.destination, "discard_light", {
      pan: 0.35,
    });
    expect(r2.variantIndex).toBe(0);
    expect(r2.playbackRate).toBeCloseTo(1.04, 5);
    expect(fake.panners[1]!.panValue).toBeCloseTo(0.35, 5);
  });

  it("falls back 1:1 to ProceduralFallback with zero uncaught errors when sprite fetch returns 404", async () => {
    const fake = createFakeSfxContext();
    const fetchFn: FetchLike = vi.fn(async () => ({
      ok: false,
      status: 404,
      json: async () => ({}),
      arrayBuffer: async () => new ArrayBuffer(0),
    }));

    const backend = createSfxBackend({
      spriteUrl: "/missing.m4a",
      manifest: sfxManifestJson as unknown as SfxSpriteManifest,
      fetchFn,
    });

    const mode = await backend.ensureLoaded(fake.ctx);
    expect(mode).toBe("procedural");
    expect(backend.getMode()).toBe("procedural");

    for (const slot of ALL_SFX_SLOTS) {
      const receipt = backend.play(fake.ctx, fake.destination, slot);
      expect(receipt.backend).toBe("procedural");
      expect(receipt.slot).toBe(slot);
    }
    expect(fake.oscillators.length).toBeGreaterThan(ALL_SFX_SLOTS.length);
  });

  it("falls back 1:1 to ProceduralFallback when decodeAudioData rejects", async () => {
    const fake = createFakeSfxContext({ decodeShouldReject: true });
    const fetchFn: FetchLike = vi.fn(async () => ({
      ok: true,
      status: 200,
      json: async () => ({}),
      arrayBuffer: async () => new ArrayBuffer(8),
    }));

    const backend = createSfxBackend({
      spriteUrl: "/corrupt.m4a",
      manifest: sfxManifestJson as unknown as SfxSpriteManifest,
      fetchFn,
    });

    const mode = await backend.ensureLoaded(fake.ctx);
    expect(mode).toBe("procedural");

    const receipt = backend.play(fake.ctx, fake.destination, "win_tier_3", {
      pan: 0,
    });
    expect(receipt.backend).toBe("procedural");
    expect(fake.oscillators.length).toBeGreaterThan(0);
  });
});
