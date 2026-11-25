import { describe, expect, it, vi } from "vitest";
import type {
  AudioBufferLike,
  AudioBufferSourceNodeLike,
  AudioContextLike,
  AudioNodeLike,
  AudioParamLike,
  BiquadFilterNodeLike,
  DynamicsCompressorNodeLike,
  FetchLike,
  GainNodeLike,
  OscillatorNodeLike,
  VoManifest,
} from "./audioTypes";
import { createVoManager, VO_DUCK_FACTOR, VO_DUCK_TIME_SEC } from "./voManager";

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

function createFakeVoAudioEnv() {
  let now = 2.0;
  const destination: AudioNodeLike = { connect: vi.fn() };
  const sources: AudioBufferSourceNodeLike[] = [];

  const ctx: AudioContextLike = {
    state: "running",
    get currentTime() {
      return now;
    },
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
    createBuffer: (_ch, length, sampleRate): AudioBufferLike => ({
      sampleRate,
      length,
      duration: length / sampleRate,
      getChannelData: () => new Float32Array(length),
    }),
    createBufferSource: (): AudioBufferSourceNodeLike => {
      const node: AudioBufferSourceNodeLike = {
        buffer: null,
        playbackRate: makeFakeParam(1),
        onended: null,
        connect: vi.fn(),
        start: vi.fn(),
        stop: vi.fn(),
      };
      sources.push(node);
      return node;
    },
    createOscillator: (): OscillatorNodeLike => ({
      type: "sine",
      frequency: makeFakeParam(440),
      onended: null,
      connect: vi.fn(),
      start: vi.fn(),
      stop: vi.fn(),
    }),
    createBiquadFilter: (): BiquadFilterNodeLike => ({
      type: "lowpass",
      frequency: makeFakeParam(1000),
      Q: makeFakeParam(1),
      connect: vi.fn(),
    }),
    decodeAudioData: vi.fn(async () => ({
      sampleRate: 44100,
      length: 44100,
      duration: 1,
      getChannelData: () => new Float32Array(44100),
    })),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  };

  const sfxGain = ctx.createGain();
  const voGain = ctx.createGain();

  return {
    ctx,
    sfxGain,
    voGain,
    sources,
    advanceTime(sec: number) {
      now += sec;
    },
  };
}

function makePackFetch(): { fetchFn: FetchLike; requestedUrls: string[] } {
  const requestedUrls: string[] = [];
  const fetchFn: FetchLike = vi.fn(async (url: string) => {
    requestedUrls.push(url);
    const match = url.match(/^\/assets\/vo\/([^/]+)\/manifest\.json$/);
    if (match) {
      const packId = match[1]!;
      const manifest: VoManifest = {
        pack_id: packId,
        version: 2,
        clips: {
          Hu: ["hu.m4a"],
          RobKong: ["robkong.m4a"],
          Kong: ["kong.m4a"],
          Pon: ["pon.m4a"],
          Chi: ["chi.m4a"],
          Discard: ["discard.m4a"],
        },
      };
      return {
        ok: true,
        status: 200,
        json: async () => manifest,
        arrayBuffer: async () => new ArrayBuffer(0),
      };
    }
    if (url.endsWith(".m4a")) {
      return {
        ok: true,
        status: 200,
        json: async () => ({}),
        arrayBuffer: async () => new ArrayBuffer(16),
      };
    }
    return {
      ok: false,
      status: 404,
      json: async () => ({}),
      arrayBuffer: async () => new ArrayBuffer(0),
    };
  });
  return { fetchFn, requestedUrls };
}

describe("VoManager (v2 VO Pack contract)", () => {
  it("loads /assets/vo/<pack_id>/manifest.json + mono .m4a and enforces 4-pack LRU eviction", async () => {
    const env = createFakeVoAudioEnv();
    const { fetchFn, requestedUrls } = makePackFetch();
    const vo = createVoManager({ fetchFn, random: () => 0 });

    await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
      seat: "East",
      packId: "pack-1",
      category: "Discard",
    });
    expect(requestedUrls).toContain("/assets/vo/pack-1/manifest.json");
    expect(requestedUrls).toContain("/assets/vo/pack-1/discard.m4a");

    await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
      seat: "South",
      packId: "pack-2",
      category: "Pon",
    });
    await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
      seat: "West",
      packId: "pack-3",
      category: "Kong",
    });
    await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
      seat: "North",
      packId: "pack-4",
      category: "Hu",
    });

    expect(vo.getCachedPackIds()).toEqual(["pack-1", "pack-2", "pack-3", "pack-4"]);

    // Touch pack-1 again so pack-2 becomes the LRU entry
    await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
      seat: "North",
      packId: "pack-1",
      category: "Hu",
    });
    expect(vo.getCachedPackIds()).toEqual(["pack-2", "pack-3", "pack-4", "pack-1"]);

    // Loading a 5th pack evicts pack-2
    await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
      seat: "North",
      packId: "pack-5",
      category: "Hu",
    });
    expect(vo.getCachedPackIds()).toEqual(["pack-3", "pack-4", "pack-1", "pack-5"]);
  });

  it("interrupts active VO from the same seat regardless of priority", async () => {
    const env = createFakeVoAudioEnv();
    const { fetchFn } = makePackFetch();
    const vo = createVoManager({ fetchFn, random: () => 0 });

    await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
      seat: "East",
      packId: "pack-1",
      category: "Pon",
    });
    expect(env.sources).toHaveLength(1);
    const first = env.sources[0]!;

    // Same seat plays lower-priority Discard -> interrupts Pon
    const played = await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
      seat: "East",
      packId: "pack-1",
      category: "Discard",
    });
    expect(played).toBe(true);
    expect(first.stop).toHaveBeenCalledTimes(1);
    expect(env.sources).toHaveLength(2);
  });

  it("enforces cross-seat priority Hu > RobKong > Kong > Pon > Chi > Discard", async () => {
    const env = createFakeVoAudioEnv();
    const { fetchFn } = makePackFetch();
    const vo = createVoManager({ fetchFn, random: () => 0 });

    // Seat East plays Discard (lowest)
    await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
      seat: "East",
      packId: "pack-1",
      category: "Discard",
    });

    // Seat South plays Chi -> interrupts Discard
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "South",
        packId: "pack-1",
        category: "Chi",
      }),
    ).toBe(true);
    expect(env.sources[0]!.stop).toHaveBeenCalledTimes(1);

    // Seat West tries Discard -> suppressed by active Chi
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "West",
        packId: "pack-1",
        category: "Discard",
      }),
    ).toBe(false);

    // Seat West plays Pon -> interrupts Chi
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "West",
        packId: "pack-1",
        category: "Pon",
      }),
    ).toBe(true);

    // Seat North plays Kong -> interrupts Pon
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "North",
        packId: "pack-1",
        category: "Kong",
      }),
    ).toBe(true);

    // Seat East plays RobKong -> interrupts Kong
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "East",
        packId: "pack-1",
        category: "RobKong",
      }),
    ).toBe(true);

    // Seat South plays Hu -> interrupts RobKong
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "South",
        packId: "pack-1",
        category: "Hu",
      }),
    ).toBe(true);

    // Seat North tries RobKong while Hu is playing -> suppressed
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "North",
        packId: "pack-1",
        category: "RobKong",
      }),
    ).toBe(false);
  });

  it("smoothly ducks sfxGain to 0.45 over 30ms during VO and restores when VO finishes", async () => {
    const env = createFakeVoAudioEnv();
    const { fetchFn } = makePackFetch();
    const vo = createVoManager({ fetchFn, random: () => 0 });

    await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 0.8, {
      seat: "East",
      packId: "pack-1",
      category: "Hu",
    });

    expect(VO_DUCK_FACTOR).toBe(0.45);
    expect(VO_DUCK_TIME_SEC).toBe(0.03);
    expect(env.sfxGain.gain.linearRampToValueAtTime).toHaveBeenCalledWith(
      0.8 * 0.45,
      2.0 + 0.03,
    );
    expect(env.sfxGain.gain.value).toBeCloseTo(0.8 * 0.45, 5);

    env.advanceTime(0.9);
    // Simulate active VO finishing
    env.sources[0]!.onended?.();

    expect(env.sfxGain.gain.linearRampToValueAtTime).toHaveBeenLastCalledWith(
      0.8,
      2.9 + 0.03,
    );
    expect(env.sfxGain.gain.value).toBeCloseTo(0.8, 5);
  });

  it("handles missing VO manifest gracefully without throwing or leaving sfxGain ducked", async () => {
    const env = createFakeVoAudioEnv();
    const fetchFn: FetchLike = vi.fn(async () => ({
      ok: false,
      status: 404,
      json: async () => ({}),
      arrayBuffer: async () => new ArrayBuffer(0),
    }));
    const vo = createVoManager({ fetchFn });

    const played = await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
      seat: "East",
      packId: "missing-pack",
      category: "Hu",
    });

    expect(played).toBe(false);
    expect(env.sfxGain.gain.value).toBe(1.0);
  });
});
