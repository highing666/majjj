import { describe, expect, it, vi } from "vitest";
import type {
  ActiveBanterVoiceState,
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
} from "./audioTypes";
import {
  BANTER_VOICE_MAX_QUEUE_LENGTH,
  createBanterVoicePlayer,
} from "./banterVoicePlayer";
import { VO_DUCK_FACTOR, VO_DUCK_TIME_SEC } from "./voManager";

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

function createFakeBanterAudioEnv(opts?: {
  decodeShouldRejectFor?: Set<number>;
}) {
  let now = 3.0;
  let decodeCount = 0;
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
      decodeCount += 1;
      if (opts?.decodeShouldRejectFor?.has(decodeCount)) {
        throw new Error("corrupt wav");
      }
      return {
        sampleRate: 24000,
        length: 24000,
        duration: 1.0,
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

function makeOkWavFetch(): { fetchFn: FetchLike; requestedUrls: string[] } {
  const requestedUrls: string[] = [];
  const fetchFn: FetchLike = vi.fn(async (url: string) => {
    requestedUrls.push(url);
    return {
      ok: true,
      status: 200,
      json: async () => ({}),
      arrayBuffer: async () => new ArrayBuffer(64),
    };
  });
  return { fetchFn, requestedUrls };
}

describe("BanterVoicePlayer", () => {
  it("fetches same-origin WAV, decodes via AudioContext, applies pan & 30ms ducking, and notifies active speaking state", async () => {
    const env = createFakeBanterAudioEnv();
    const { fetchFn, requestedUrls } = makeOkWavFetch();
    const player = createBanterVoicePlayer({ fetchFn });

    const observedStates: Array<ActiveBanterVoiceState | null> = [];
    player.subscribe((active) => observedStates.push(active));
    expect(observedStates).toEqual([null]);

    player.enqueue(env.ctx, env.voGain, env.sfxGain, 0.8, {
      banterId: "b-1",
      seat: "South",
      audioUrl: "/api/banter-audio/b-1",
      durationMs: 1200,
      priority: "normal",
      pan: 0.35,
    });

    await vi.waitFor(() => {
      expect(env.sources).toHaveLength(1);
    });

    expect(requestedUrls).toEqual(["/api/banter-audio/b-1"]);
    expect(env.panners[0]!.pan.value).toBeCloseTo(0.35, 5);
    expect(env.sfxGain.gain.linearRampToValueAtTime).toHaveBeenCalledWith(
      0.8 * VO_DUCK_FACTOR,
      3.0 + VO_DUCK_TIME_SEC,
    );
    expect(env.sfxGain.gain.value).toBeCloseTo(0.8 * 0.45, 5);
    expect(player.isPlaying()).toBe(true);
    expect(player.getActive()).toEqual({ seat: "South", banterId: "b-1" });
    expect(observedStates.at(-1)).toEqual({ seat: "South", banterId: "b-1" });

    // Finish playback -> restores sfxGain and notifies null
    env.advanceTime(1.2);
    env.sources[0]!.node.onended?.();
    expect(player.isPlaying()).toBe(false);
    expect(player.getActive()).toBeNull();
    expect(observedStates.at(-1)).toBeNull();
    expect(env.sfxGain.gain.linearRampToValueAtTime).toHaveBeenLastCalledWith(
      0.8,
      4.2 + VO_DUCK_TIME_SEC,
    );
    expect(env.sfxGain.gain.value).toBeCloseTo(0.8, 5);

    player.dispose();
  });

  it("silently skips HTTP 404, network errors, and decode failures while advancing to queued items", async () => {
    const env = createFakeBanterAudioEnv({
      decodeShouldRejectFor: new Set([1]),
    });
    const fetchFn: FetchLike = vi.fn(async (url: string) => {
      if (url.endsWith("/b-404")) {
        return {
          ok: false,
          status: 404,
          json: async () => ({}),
          arrayBuffer: async () => new ArrayBuffer(0),
        };
      }
      if (url.endsWith("/b-net")) {
        throw new Error("LAN unreachable");
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({}),
        arrayBuffer: async () => new ArrayBuffer(32),
      };
    });
    const player = createBanterVoicePlayer({ fetchFn });

    // Enqueue 404 -> network error -> corrupt decode -> valid ok item
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-404",
      seat: "South",
      audioUrl: "/api/banter-audio/b-404",
      durationMs: 1000,
      priority: "normal",
      pan: 0.35,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-net",
      seat: "West",
      audioUrl: "/api/banter-audio/b-net",
      durationMs: 1000,
      priority: "normal",
      pan: 0,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-corrupt",
      seat: "North",
      audioUrl: "/api/banter-audio/b-corrupt",
      durationMs: 1000,
      priority: "highlight",
      pan: -0.35,
    });

    // Wait for first three failures to drain cleanly without throwing or ducking
    await vi.waitFor(() => {
      expect(fetchFn).toHaveBeenCalledTimes(3);
    });
    await Promise.resolve();
    expect(env.sources).toHaveLength(0);
    expect(player.isPlaying()).toBe(false);
    expect(env.sfxGain.gain.value).toBe(1.0);

    // Subsequent valid item plays cleanly
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-ok",
      seat: "South",
      audioUrl: "/api/banter-audio/b-ok",
      durationMs: 1000,
      priority: "normal",
      pan: 0.35,
    });
    await vi.waitFor(() => {
      expect(env.sources).toHaveLength(1);
    });
    expect(player.getActive()).toEqual({ seat: "South", banterId: "b-ok" });

    player.dispose();
  });

  it("preempts active same-seat BanterVoice immediately, drops queued items for that seat, and preserves other seats", async () => {
    const env = createFakeBanterAudioEnv();
    const { fetchFn } = makeOkWavFetch();
    const player = createBanterVoicePlayer({ fetchFn });

    // South starts playing b-1
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-1",
      seat: "South",
      audioUrl: "/api/banter-audio/b-1",
      durationMs: 1500,
      priority: "normal",
      pan: 0.35,
    });
    await vi.waitFor(() => expect(env.sources).toHaveLength(1));

    // West queues b-2, North queues b-3
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-2",
      seat: "West",
      audioUrl: "/api/banter-audio/b-2",
      durationMs: 1200,
      priority: "normal",
      pan: 0,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-3",
      seat: "North",
      audioUrl: "/api/banter-audio/b-3",
      durationMs: 1200,
      priority: "normal",
      pan: -0.35,
    });

    // West sends a newer b-4 while South is still playing -> drops West's old b-2 from queue, keeps North's b-3!
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-4",
      seat: "West",
      audioUrl: "/api/banter-audio/b-4",
      durationMs: 1200,
      priority: "normal",
      pan: 0,
    });
    expect(env.sources[0]!.node.stop).not.toHaveBeenCalled();

    // South sends a newer b-5 -> immediately stops South's active b-1 and starts b-5!
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-5",
      seat: "South",
      audioUrl: "/api/banter-audio/b-5",
      durationMs: 1200,
      priority: "normal",
      pan: 0.35,
    });
    expect(env.sources[0]!.node.stop).toHaveBeenCalledTimes(1);
    await vi.waitFor(() => expect(env.sources).toHaveLength(2));
    expect(player.getActive()).toEqual({ seat: "South", banterId: "b-5" });

    // Finish b-5 -> North's b-3 plays next (West's old b-2 was dropped!)
    env.sources[1]!.node.onended?.();
    await vi.waitFor(() => expect(env.sources).toHaveLength(3));
    expect(player.getActive()).toEqual({ seat: "North", banterId: "b-3" });

    // Finish b-3 -> West's new b-4 plays last
    env.sources[2]!.node.onended?.();
    await vi.waitFor(() => expect(env.sources).toHaveLength(4));
    expect(player.getActive()).toEqual({ seat: "West", banterId: "b-4" });

    player.dispose();
  });

  it("caps cross-seat pending queue at 2 and evicts oldest normal banter while preserving highlight/settlement banter", async () => {
    expect(BANTER_VOICE_MAX_QUEUE_LENGTH).toBe(2);
    const env = createFakeBanterAudioEnv();
    const { fetchFn } = makeOkWavFetch();
    const player = createBanterVoicePlayer({ fetchFn });

    // 1. Active South (b-1), queued: [West normal (b-2), North highlight (b-3)]
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-1",
      seat: "South",
      audioUrl: "/api/banter-audio/b-1",
      durationMs: 1000,
      priority: "normal",
      pan: 0.35,
    });
    await vi.waitFor(() => expect(env.sources).toHaveLength(1));

    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-2",
      seat: "West",
      audioUrl: "/api/banter-audio/b-2",
      durationMs: 1000,
      priority: "normal",
      pan: 0,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-3",
      seat: "North",
      audioUrl: "/api/banter-audio/b-3",
      durationMs: 1000,
      priority: "highlight",
      pan: -0.35,
    });

    // Queue full (2): East normal (b-4) arrives -> evicts oldest normal (b-2), preserves North highlight (b-3)
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-4",
      seat: "East",
      audioUrl: "/api/banter-audio/b-4",
      durationMs: 1000,
      priority: "normal",
      pan: 0,
    });

    // Now queue is [North highlight (b-3), East normal (b-4)].
    // West highlight (b-5) arrives -> evicts East normal (b-4), leaving [North highlight (b-3), West highlight (b-5)].
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-5",
      seat: "West",
      audioUrl: "/api/banter-audio/b-5",
      durationMs: 1000,
      priority: "highlight",
      pan: 0,
    });

    // All queued items are highlight now! Incoming East normal (b-6) must be dropped!
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-6",
      seat: "East",
      audioUrl: "/api/banter-audio/b-6",
      durationMs: 1000,
      priority: "normal",
      pan: 0,
    });

    // Drain b-1 -> expect North highlight (b-3), then West highlight (b-5), then idle!
    env.sources[0]!.node.onended?.();
    await vi.waitFor(() => expect(env.sources).toHaveLength(2));
    expect(player.getActive()).toEqual({ seat: "North", banterId: "b-3" });

    // While b-3 is playing, queue has [West highlight (b-5)]. Add East highlight (b-7) -> queue full with 2 highlights [b-5, b-7].
    // Then South highlight (b-8) arrives -> evicts oldest highlight (b-5), keeping [b-7, b-8]!
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-7",
      seat: "East",
      audioUrl: "/api/banter-audio/b-7",
      durationMs: 1000,
      priority: "highlight",
      pan: 0,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-8",
      seat: "South",
      audioUrl: "/api/banter-audio/b-8",
      durationMs: 1000,
      priority: "highlight",
      pan: 0.35,
    });

    env.sources[1]!.node.onended?.();
    await vi.waitFor(() => expect(env.sources).toHaveLength(3));
    expect(player.getActive()).toEqual({ seat: "East", banterId: "b-7" });

    env.sources[2]!.node.onended?.();
    await vi.waitFor(() => expect(env.sources).toHaveLength(4));
    expect(player.getActive()).toEqual({ seat: "South", banterId: "b-8" });

    env.sources[3]!.node.onended?.();
    expect(player.getActive()).toBeNull();
    expect(player.getQueueLength()).toBe(0);

    player.dispose();
  });

  it("stops playback, clears queue, ignores stale in-flight fetches, and restores sfxGain on flush()", async () => {
    const env = createFakeBanterAudioEnv();
    let resolveInflight!: () => void;
    const fetchFn: FetchLike = vi.fn(
      () =>
        new Promise<Awaited<ReturnType<FetchLike>>>((resolve) => {
          resolveInflight = () =>
            resolve({
              ok: true,
              status: 200,
              json: async () => ({}),
              arrayBuffer: async () => new ArrayBuffer(32),
            });
        }),
    );
    const player = createBanterVoicePlayer({ fetchFn });

    // 1. Flush while fetch is in flight -> late resolution must NOT start playback!
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 0.9, {
      banterId: "b-inflight",
      seat: "South",
      audioUrl: "/api/banter-audio/b-inflight",
      durationMs: 1000,
      priority: "normal",
      pan: 0.35,
    });
    player.flush();
    resolveInflight();
    await Promise.resolve();
    await Promise.resolve();
    expect(env.sources).toHaveLength(0);
    expect(player.isPlaying()).toBe(false);

    // 2. Flush while playing with queued item -> stops source, clears queue, restores sfxGain
    const { fetchFn: okFetch } = makeOkWavFetch();
    const player2 = createBanterVoicePlayer({ fetchFn: okFetch });
    const events: Array<ActiveBanterVoiceState | null> = [];
    player2.subscribe((s) => events.push(s));

    player2.enqueue(env.ctx, env.voGain, env.sfxGain, 0.9, {
      banterId: "b-active",
      seat: "South",
      audioUrl: "/api/banter-audio/b-active",
      durationMs: 1500,
      priority: "highlight",
      pan: 0.35,
    });
    await vi.waitFor(() => expect(env.sources).toHaveLength(1));
    player2.enqueue(env.ctx, env.voGain, env.sfxGain, 0.9, {
      banterId: "b-queued",
      seat: "West",
      audioUrl: "/api/banter-audio/b-queued",
      durationMs: 1500,
      priority: "highlight",
      pan: 0,
    });
    expect(env.sfxGain.gain.value).toBeCloseTo(0.9 * 0.45, 5);

    env.advanceTime(0.5);
    player2.flush();
    expect(env.sources[0]!.node.stop).toHaveBeenCalledTimes(1);
    expect(player2.isPlaying()).toBe(false);
    expect(player2.getActive()).toBeNull();
    expect(player2.getQueueLength()).toBe(0);
    expect(events.at(-1)).toBeNull();
    expect(env.sfxGain.gain.linearRampToValueAtTime).toHaveBeenLastCalledWith(
      0.9,
      3.5 + VO_DUCK_TIME_SEC,
    );

    player.dispose();
    player2.dispose();
  });

  it("holds sfxGain ducking when external static VO is still active upon BanterVoice completion", async () => {
    const env = createFakeBanterAudioEnv();
    const { fetchFn } = makeOkWavFetch();
    let externalActive = true;
    const player = createBanterVoicePlayer({
      fetchFn,
      isExternalVoiceActive: () => externalActive,
    });

    env.sfxGain.gain.value = 0.8 * 0.45;
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 0.8, {
      banterId: "b-ext",
      seat: "North",
      audioUrl: "/api/banter-audio/b-ext",
      durationMs: 1000,
      priority: "normal",
      pan: -0.35,
    });
    await vi.waitFor(() => expect(env.sources).toHaveLength(1));

    // Banter ends while static VO is still active -> must NOT un-duck sfxGain
    env.sources[0]!.node.onended?.();
    expect(env.sfxGain.gain.value).toBeCloseTo(0.8 * 0.45, 5);

    externalActive = false;
    player.dispose();
  });

  it("publishes pendingBanterIds across loading, queued cross-seat items, eviction, and flush", async () => {
    const env = createFakeBanterAudioEnv();
    const { fetchFn } = makeOkWavFetch();
    const player = createBanterVoicePlayer({ fetchFn });

    const snapshots: Array<{
      active: ActiveBanterVoiceState | null;
      pending: string[];
    }> = [];
    player.subscribe((active, pendingIds) => {
      snapshots.push({ active, pending: [...pendingIds].sort() });
    });
    expect(snapshots).toEqual([{ active: null, pending: [] }]);

    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-1",
      seat: "South",
      audioUrl: "/api/banter-audio/b-1",
      durationMs: 1200,
      priority: "normal",
      pan: 0.35,
    });
    expect([...player.getPendingBanterIds()]).toEqual(["b-1"]);

    await vi.waitFor(() => expect(env.sources).toHaveLength(1));
    expect(player.getActive()).toEqual({ seat: "South", banterId: "b-1" });
    expect([...player.getPendingBanterIds()]).toEqual([]);

    // Queue West (b-2) and North (b-3) while South plays
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-2",
      seat: "West",
      audioUrl: "/api/banter-audio/b-2",
      durationMs: 1000,
      priority: "normal",
      pan: 0,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-3",
      seat: "North",
      audioUrl: "/api/banter-audio/b-3",
      durationMs: 1000,
      priority: "highlight",
      pan: -0.35,
    });
    expect([...player.getPendingBanterIds()].sort()).toEqual(["b-2", "b-3"]);
    expect(snapshots.at(-1)).toEqual({
      active: { seat: "South", banterId: "b-1" },
      pending: ["b-2", "b-3"],
    });

    // Queue full: East normal (b-4) evicts b-2 -> pending updates to [b-3, b-4]
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-4",
      seat: "East",
      audioUrl: "/api/banter-audio/b-4",
      durationMs: 1000,
      priority: "normal",
      pan: 0,
    });
    expect([...player.getPendingBanterIds()].sort()).toEqual(["b-3", "b-4"]);

    // Flush clears both active and pendingBanterIds and notifies subscribers
    player.flush();
    expect(player.getActive()).toBeNull();
    expect([...player.getPendingBanterIds()]).toEqual([]);
    expect(snapshots.at(-1)).toEqual({ active: null, pending: [] });

    player.dispose();
  });
});
