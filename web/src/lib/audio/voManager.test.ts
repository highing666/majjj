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
  StereoPannerNodeLike,
  VoicePackManifest,
  VoiceSlotId,
  VoiceSpriteSlice,
} from "./audioTypes";
import { ALL_VOICE_SLOTS } from "./voicePackSchema";
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

function createFakeVoAudioEnv(opts?: { decodeShouldReject?: boolean }) {
  let now = 2.0;
  const destination: AudioNodeLike = { connect: vi.fn() };
  const sources: Array<{
    node: AudioBufferSourceNodeLike;
    startArgs: Array<[number?, number?, number?]>;
    connectedTo: AudioNodeLike[];
  }> = [];
  const panners: StereoPannerNodeLike[] = [];

  const ctx: AudioContextLike = {
    state: "running",
    get currentTime() {
      return now;
    },
    sampleRate: 24000,
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
      const node: StereoPannerNodeLike = {
        pan: makeFakeParam(0),
        connect: vi.fn((dest: AudioNodeLike) => dest),
      };
      panners.push(node);
      return node;
    },
    createBuffer: (_ch, length, sampleRate): AudioBufferLike => ({
      sampleRate,
      length,
      duration: length / sampleRate,
      getChannelData: () => new Float32Array(length),
    }),
    createBufferSource: (): AudioBufferSourceNodeLike => {
      const startArgs: Array<[number?, number?, number?]> = [];
      const connectedTo: AudioNodeLike[] = [];
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
      sources.push({ node, startArgs, connectedTo });
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
    decodeAudioData: vi.fn(async () => {
      if (opts?.decodeShouldReject) {
        throw new Error("corrupt m4a");
      }
      return {
        sampleRate: 24000,
        length: 24000 * 46,
        duration: 46,
        getChannelData: () => new Float32Array(24000),
      };
    }),
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
    panners,
    advanceTime(sec: number) {
      now += sec;
    },
  };
}

function makeFullManifest(packId: string): VoicePackManifest {
  const sprites: Partial<Record<VoiceSlotId, VoiceSpriteSlice>> = {};
  ALL_VOICE_SLOTS.forEach((slot, idx) => {
    sprites[slot] = [idx * 0.8, 0.55];
  });
  return {
    pack_id: packId,
    version: 1,
    audio: "sprite.m4a",
    sample_rate: 24000,
    duration: ALL_VOICE_SLOTS.length * 0.8,
    sprites,
  };
}

function makeSpritePackFetch(): { fetchFn: FetchLike; requestedUrls: string[] } {
  const requestedUrls: string[] = [];
  const fetchFn: FetchLike = vi.fn(async (url: string) => {
    requestedUrls.push(url);
    const match = url.match(/^\/assets\/vo\/([^/]+)\/manifest\.json$/);
    if (match) {
      const packId = match[1]!;
      return {
        ok: true,
        status: 200,
        json: async () => makeFullManifest(packId),
        arrayBuffer: async () => new ArrayBuffer(0),
      };
    }
    if (url.endsWith("/sprite.m4a")) {
      return {
        ok: true,
        status: 200,
        json: async () => ({}),
        arrayBuffer: async () => new ArrayBuffer(32),
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

describe("VoManager (AudioSprite VoicePack contract)", () => {
  it("fetches and decodes sprite.m4a once per pack, slices exact [start, duration] across slots, and enforces 4-pack LRU eviction", async () => {
    const env = createFakeVoAudioEnv();
    const { fetchFn, requestedUrls } = makeSpritePackFetch();
    const vo = createVoManager({ fetchFn });

    // Preload quick_hothead once
    expect(await vo.preloadPack(env.ctx, "quick_hothead")).toBe(true);
    expect(requestedUrls).toEqual([
      "/assets/vo/quick_hothead/manifest.json",
      "/assets/vo/quick_hothead/sprite.m4a",
    ]);
    expect(env.ctx.decodeAudioData).toHaveBeenCalledTimes(1);

    // Triggering multiple slots from the same pack reuses the decoded AudioBuffer with zero extra fetch/decode!
    expect(
      await vo.triggerVo(
        env.ctx,
        env.voGain,
        env.sfxGain,
        1.0,
        {
          seat: "East",
          packId: "quick_hothead",
          category: "Discard",
          slot: "m1",
        },
        0,
      ),
    ).toBe(true);
    expect(env.sources[0]!.startArgs[0]).toEqual([2.0, 0, 0.55]);

    env.sources[0]!.node.onended?.();
    env.advanceTime(0.6);

    expect(
      await vo.triggerVo(
        env.ctx,
        env.voGain,
        env.sfxGain,
        1.0,
        {
          seat: "East",
          packId: "quick_hothead",
          category: "Pon",
          slot: "pong",
        },
        -0.35,
      ),
    ).toBe(true);
    const pongIdx = ALL_VOICE_SLOTS.indexOf("pong");
    expect(env.sources[1]!.startArgs[0]).toEqual([2.6, pongIdx * 0.8, 0.55]);
    expect(env.panners[0]!.pan.value).toBeCloseTo(-0.35, 5);
    expect(env.ctx.decodeAudioData).toHaveBeenCalledTimes(1);

    // Load 3 more packs to fill 4-pack LRU cache
    await vo.preloadPack(env.ctx, "tea_sarcasm");
    await vo.preloadPack(env.ctx, "stubborn_blamer");
    await vo.preloadPack(env.ctx, "human_male");
    expect(vo.getCachedPackIds()).toEqual([
      "quick_hothead",
      "tea_sarcasm",
      "stubborn_blamer",
      "human_male",
    ]);

    // Touch quick_hothead again so tea_sarcasm becomes LRU
    env.sources[1]!.node.onended?.();
    await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
      seat: "East",
      packId: "quick_hothead",
      category: "Hu",
      slot: "zimo",
    });
    expect(vo.getCachedPackIds()).toEqual([
      "tea_sarcasm",
      "stubborn_blamer",
      "human_male",
      "quick_hothead",
    ]);

    // Loading a 5th pack evicts tea_sarcasm
    await vo.preloadPack(env.ctx, "human_female");
    expect(vo.getCachedPackIds()).toEqual([
      "stubborn_blamer",
      "human_male",
      "quick_hothead",
      "human_female",
    ]);
  });

  it("never lets Discard interrupt any active voice, while action calls and QuickEmote preempt by priority", async () => {
    const env = createFakeVoAudioEnv();
    const { fetchFn } = makeSpritePackFetch();
    const vo = createVoManager({ fetchFn });

    // East plays Discard (m1)
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "East",
        packId: "human_male",
        category: "Discard",
        slot: "m1",
      }),
    ).toBe(true);

    // Another Discard (same seat OR cross-seat) yields silently without interrupting
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "South",
        packId: "quick_hothead",
        category: "Discard",
        slot: "z5",
      }),
    ).toBe(false);
    expect(env.sources[0]!.node.stop).not.toHaveBeenCalled();

    // QuickEmote interrupts Discard immediately
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "East",
        packId: "human_male",
        category: "QuickEmote",
        slot: "emote_hurry_up",
      }),
    ).toBe(true);
    expect(env.sources[0]!.node.stop).toHaveBeenCalledTimes(1);

    // Same-seat Discard still yields while QuickEmote is playing
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "East",
        packId: "human_male",
        category: "Discard",
        slot: "m2",
      }),
    ).toBe(false);

    // Pon interrupts QuickEmote
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "West",
        packId: "tea_sarcasm",
        category: "Pon",
        slot: "pong",
      }),
    ).toBe(true);

    // Kong interrupts Pon
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "North",
        packId: "stubborn_blamer",
        category: "Kong",
        slot: "concealed_kong",
      }),
    ).toBe(true);

    // RobKong interrupts Kong
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "East",
        packId: "human_male",
        category: "RobKong",
        slot: "rob_kong",
      }),
    ).toBe(true);

    // Hu interrupts RobKong
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "South",
        packId: "quick_hothead",
        category: "Hu",
        slot: "ron",
      }),
    ).toBe(true);

    // RobKong while Hu is playing is suppressed
    expect(
      await vo.triggerVo(env.ctx, env.voGain, env.sfxGain, 1.0, {
        seat: "North",
        packId: "stubborn_blamer",
        category: "RobKong",
        slot: "rob_kong",
      }),
    ).toBe(false);
  });

  it("smoothly ducks sfxGain to 0.45 over 30ms during VO and restores when VO finishes", async () => {
    const env = createFakeVoAudioEnv();
    const { fetchFn } = makeSpritePackFetch();
    const vo = createVoManager({ fetchFn });

    await vo.triggerVo(
      env.ctx,
      env.voGain,
      env.sfxGain,
      0.8,
      {
        seat: "East",
        packId: "human_male",
        category: "Hu",
        slot: "zimo",
      },
      0.35,
    );

    expect(VO_DUCK_FACTOR).toBe(0.45);
    expect(VO_DUCK_TIME_SEC).toBe(0.03);
    expect(env.sfxGain.gain.linearRampToValueAtTime).toHaveBeenCalledWith(
      0.8 * 0.45,
      2.0 + 0.03,
    );
    expect(env.sfxGain.gain.value).toBeCloseTo(0.8 * 0.45, 5);
    expect(env.panners[0]!.pan.value).toBeCloseTo(0.35, 5);

    env.advanceTime(0.9);
    env.sources[0]!.node.onended?.();

    expect(env.sfxGain.gain.linearRampToValueAtTime).toHaveBeenLastCalledWith(
      0.8,
      2.9 + 0.03,
    );
    expect(env.sfxGain.gain.value).toBeCloseTo(0.8, 5);
  });

  it("silently degrades on 404 manifest, 404 audio, corrupt decode, or missing slot without throwing or ducking sfxGain", async () => {
    const env404 = createFakeVoAudioEnv();
    const fetch404: FetchLike = vi.fn(async () => ({
      ok: false,
      status: 404,
      json: async () => ({}),
      arrayBuffer: async () => new ArrayBuffer(0),
    }));
    const vo404 = createVoManager({ fetchFn: fetch404 });
    expect(
      await vo404.triggerVo(env404.ctx, env404.voGain, env404.sfxGain, 1.0, {
        seat: "East",
        packId: "missing-pack",
        category: "Hu",
        slot: "zimo",
      }),
    ).toBe(false);
    expect(env404.sfxGain.gain.value).toBe(1.0);
    expect(vo404.isDucking()).toBe(false);

    // Corrupt audio decode failure
    const envCorrupt = createFakeVoAudioEnv({ decodeShouldReject: true });
    const { fetchFn: okFetch } = makeSpritePackFetch();
    const voCorrupt = createVoManager({ fetchFn: okFetch });
    expect(
      await voCorrupt.triggerVo(
        envCorrupt.ctx,
        envCorrupt.voGain,
        envCorrupt.sfxGain,
        1.0,
        {
          seat: "East",
          packId: "human_male",
          category: "Pon",
          slot: "pong",
        },
      ),
    ).toBe(false);
    expect(envCorrupt.sfxGain.gain.value).toBe(1.0);
    expect(voCorrupt.isDucking()).toBe(false);
  });
});
