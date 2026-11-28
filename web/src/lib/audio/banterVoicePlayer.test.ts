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
  FetchInitLike,
  FetchLike,
  FetchResponseLike,
  GainNodeLike,
  OscillatorNodeLike,
  StereoPannerNodeLike,
} from "./audioTypes";
import {
  BANTER_EDGE_FADE_SEC,
  BANTER_STREAM_SAFETY_LEAD_SEC,
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

function encodeInt16Le(samples: readonly number[]): Uint8Array {
  const bytes = new Uint8Array(samples.length * 2);
  const view = new DataView(bytes.buffer);
  for (let i = 0; i < samples.length; i += 1) {
    view.setInt16(i * 2, samples[i]!, true);
  }
  return bytes;
}

function makeConstantPcm16(sampleCount: number, sampleValue = 8192): Uint8Array {
  const samples = new Array<number>(sampleCount).fill(sampleValue);
  return encodeInt16Le(samples);
}

function makePcmStreamResponse(
  chunks: readonly Uint8Array[],
  headersMap?: Record<string, string>,
): FetchResponseLike {
  const normalized = new Map<string, string>();
  for (const [k, v] of Object.entries(
    headersMap ?? {
      "X-Audio-Format": "pcm_s16le",
      "X-Audio-Sample-Rate": "24000",
      "X-Audio-Channels": "1",
    },
  )) {
    normalized.set(k.toLowerCase(), v);
  }
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const chunk of chunks) {
        controller.enqueue(chunk);
      }
      controller.close();
    },
  });
  return {
    ok: true,
    status: 200,
    headers: {
      get: (name: string) => normalized.get(name.toLowerCase()) ?? null,
    },
    body: stream,
    json: async () => ({}),
    arrayBuffer: async () => new ArrayBuffer(0),
  };
}

function createControlledStream(): {
  stream: ReadableStream<Uint8Array>;
  push(chunk: Uint8Array): void;
  close(): void;
  error(err: unknown): void;
  canceled: () => boolean;
} {
  let controllerRef!: ReadableStreamDefaultController<Uint8Array>;
  let wasCanceled = false;
  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      controllerRef = controller;
    },
    cancel() {
      wasCanceled = true;
    },
  });
  return {
    stream,
    push(chunk: Uint8Array) {
      controllerRef.enqueue(chunk);
    },
    close() {
      controllerRef.close();
    },
    error(err: unknown) {
      controllerRef.error(err);
    },
    canceled: () => wasCanceled,
  };
}

function createFakeBanterAudioEnv() {
  let now = 3.0;
  const destination: AudioNodeLike = { connect: vi.fn() };
  const sources: Array<{
    node: AudioBufferSourceNodeLike;
    startArgs: Array<[number?, number?, number?]>;
    connectedTo: AudioNodeLike[];
  }> = [];
  const panners: StereoPannerNodeLike[] = [];
  const gains: GainNodeLike[] = [];
  const createdBuffers: Array<{
    buffer: AudioBufferLike;
    channelData: Float32Array;
  }> = [];

  const ctx: AudioContextLike = {
    state: "running",
    get currentTime() {
      return now;
    },
    sampleRate: 24000,
    destination,
    resume: vi.fn(async () => {}),
    createGain: (): GainNodeLike => {
      const node: GainNodeLike = {
        gain: makeFakeParam(1),
        connect: vi.fn((dest: AudioNodeLike) => dest),
        disconnect: vi.fn(),
      };
      gains.push(node);
      return node;
    },
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
        disconnect: vi.fn(),
      };
      panners.push(node);
      return node;
    },
    createBuffer: (_ch, length, sampleRate): AudioBufferLike => {
      const channelData = new Float32Array(length);
      const buffer: AudioBufferLike = {
        sampleRate,
        length,
        duration: length / sampleRate,
        getChannelData: () => channelData,
      };
      createdBuffers.push({ buffer, channelData });
      return buffer;
    },
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
      throw new Error("streaming PCM16 player must never call decodeAudioData");
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
    gains,
    createdBuffers,
    advanceTime(sec: number) {
      now += sec;
    },
  };
}

function makeOkPcmFetch(sampleCount = 4800): {
  fetchFn: FetchLike;
  requestedUrls: string[];
  signals: Array<AbortSignal | undefined>;
} {
  const requestedUrls: string[] = [];
  const signals: Array<AbortSignal | undefined> = [];
  const fetchFn: FetchLike = vi.fn(
    async (url: string, init?: FetchInitLike) => {
      requestedUrls.push(url);
      signals.push(init?.signal);
      return makePcmStreamResponse([makeConstantPcm16(sampleCount)]);
    },
  );
  return { fetchFn, requestedUrls, signals };
}

describe("BanterVoicePlayer (streaming Raw PCM16)", () => {
  it("streams multi-chunk Raw PCM16 without decodeAudioData, enters playing on chunk 1 with pan, 30ms ducking, sample-accurate scheduling, and 3ms edge fades", async () => {
    const env = createFakeBanterAudioEnv();
    const controlled = createControlledStream();
    const fetchFn: FetchLike = vi.fn(async () => ({
      ok: true,
      status: 200,
      headers: {
        get: (name: string) =>
          name.toLowerCase() === "x-audio-sample-rate" ? "24000" : null,
      },
      body: controlled.stream,
      json: async () => ({}),
      arrayBuffer: async () => new ArrayBuffer(0),
    }));
    const player = createBanterVoicePlayer({ fetchFn });

    const observedStates: Array<ActiveBanterVoiceState | null> = [];
    player.subscribe((active) => observedStates.push(active));
    expect(observedStates).toEqual([null]);

    player.enqueue(env.ctx, env.voGain, env.sfxGain, 0.8, {
      banterId: "b-1",
      seat: "South",
      audioUrl: "/api/banter-audio/b-1",
      sampleRate: 24000,
      priority: "normal",
      pan: 0.35,
    });

    // Push ONLY chunk 1 (12,000 samples == 0.5s at 24kHz) while stream remains open.
    controlled.push(makeConstantPcm16(12000, 16384));

    await vi.waitFor(() => {
      expect(env.sources).toHaveLength(1);
    });

    // Playing state, pan, and 30ms ducking start immediately on chunk 1 before stream EOF!
    expect(env.ctx.decodeAudioData).not.toHaveBeenCalled();
    expect(env.panners[0]!.pan.value).toBeCloseTo(0.35, 5);
    expect(env.sfxGain.gain.linearRampToValueAtTime).toHaveBeenCalledWith(
      0.8 * VO_DUCK_FACTOR,
      3.0 + VO_DUCK_TIME_SEC,
    );
    expect(player.isPlaying()).toBe(true);
    expect(player.getActive()).toEqual({ seat: "South", banterId: "b-1" });
    expect(observedStates.at(-1)).toEqual({ seat: "South", banterId: "b-1" });

    const chunk1Start = 3.0 + BANTER_STREAM_SAFETY_LEAD_SEC;
    expect(env.sources[0]!.startArgs[0]?.[0]).toBeCloseTo(chunk1Start, 6);
    expect(env.createdBuffers[0]!.buffer.sampleRate).toBe(24000);
    expect(env.createdBuffers[0]!.channelData[0]).toBeCloseTo(0.5, 5);

    // Slot gain has ~3ms fade-in at chunk 1 start.
    const slotGain = env.gains[2]!;
    expect(slotGain.gain.setValueAtTime).toHaveBeenCalledWith(0, chunk1Start);
    expect(slotGain.gain.linearRampToValueAtTime).toHaveBeenCalledWith(
      1,
      chunk1Start + BANTER_EDGE_FADE_SEC,
    );

    // Push chunk 2 (6,000 samples == 0.25s at 24kHz) -> scheduled sample-accurately right after chunk 1!
    controlled.push(makeConstantPcm16(6000, -16384));
    await vi.waitFor(() => {
      expect(env.sources).toHaveLength(2);
    });
    const chunk2Start = chunk1Start + 0.5;
    expect(env.sources[1]!.startArgs[0]?.[0]).toBeCloseTo(chunk2Start, 6);
    expect(env.createdBuffers[1]!.channelData[0]).toBeCloseTo(-0.5, 5);

    // Chunk 1 ends while stream is still open -> stays playing!
    env.advanceTime(0.55);
    env.sources[0]!.node.onended?.();
    expect(player.isPlaying()).toBe(true);
    expect(player.getActive()).toEqual({ seat: "South", banterId: "b-1" });

    // Close stream -> schedules ~3ms fade-out at utterance endTime (chunk2Start + 0.25).
    controlled.close();
    const utteranceEnd = chunk2Start + 0.25;
    await vi.waitFor(() => {
      expect(slotGain.gain.linearRampToValueAtTime).toHaveBeenCalledWith(
        0,
        utteranceEnd,
      );
    });
    expect(slotGain.gain.setValueAtTime).toHaveBeenCalledWith(
      1,
      utteranceEnd - BANTER_EDGE_FADE_SEC,
    );
    // Still playing until last scheduled AudioBufferSourceNode fires onended!
    expect(player.isPlaying()).toBe(true);

    env.advanceTime(0.25);
    env.sources[1]!.node.onended?.();
    expect(player.isPlaying()).toBe(false);
    expect(player.getActive()).toBeNull();
    expect(observedStates.at(-1)).toBeNull();
    expect(env.sfxGain.gain.linearRampToValueAtTime).toHaveBeenLastCalledWith(
      0.8,
      3.8 + VO_DUCK_TIME_SEC,
    );
    expect(env.sfxGain.gain.value).toBeCloseTo(0.8, 5);

    player.dispose();
  });

  it("reassembles arbitrary odd-byte PCM16 chunk boundaries without sample shift and honors X-Audio-Sample-Rate with protocol/24000 fallback", async () => {
    const env = createFakeBanterAudioEnv();
    // 3 exact Int16 LE samples: +16384 (+0.5), -32768 (-1.0), +32767 (~+1.0)
    const fullBytes = encodeInt16Le([16384, -32768, 32767]);
    // Split into odd-length chunks: [1 byte], [3 bytes], [2 bytes]
    const splitChunks = [
      fullBytes.slice(0, 1),
      fullBytes.slice(1, 4),
      fullBytes.slice(4, 6),
    ];

    const fetchFn: FetchLike = vi.fn(async (url: string) => {
      if (url.endsWith("/b-header-sr")) {
        return makePcmStreamResponse(splitChunks, {
          "X-Audio-Sample-Rate": "16000",
        });
      }
      return makePcmStreamResponse([encodeInt16Le([0, 16384])], {});
    });
    const player = createBanterVoicePlayer({ fetchFn });

    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-header-sr",
      seat: "South",
      audioUrl: "/api/banter-audio/b-header-sr",
      sampleRate: 24000,
      priority: "normal",
      pan: 0,
    });

    await vi.waitFor(() => {
      expect(env.sources).toHaveLength(2);
    });

    // Header X-Audio-Sample-Rate (16000) overrides protocol sampleRate (24000).
    expect(env.createdBuffers[0]!.buffer.sampleRate).toBe(16000);
    expect(env.createdBuffers[0]!.channelData).toHaveLength(2);
    expect(env.createdBuffers[0]!.channelData[0]).toBeCloseTo(0.5, 5);
    expect(env.createdBuffers[0]!.channelData[1]).toBeCloseTo(-1.0, 5);

    expect(env.createdBuffers[1]!.buffer.sampleRate).toBe(16000);
    expect(env.createdBuffers[1]!.channelData).toHaveLength(1);
    expect(env.createdBuffers[1]!.channelData[0]).toBeCloseTo(32767 / 32768, 5);

    // Finish first utterance and verify missing header falls back to protocol sampleRate (22050).
    env.sources[0]!.node.onended?.();
    env.sources[1]!.node.onended?.();
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-fallback-sr",
      seat: "West",
      audioUrl: "/api/banter-audio/b-fallback-sr",
      sampleRate: 22050,
      priority: "normal",
      pan: 0,
    });
    await vi.waitFor(() => {
      expect(env.sources).toHaveLength(3);
    });
    expect(env.createdBuffers[2]!.buffer.sampleRate).toBe(22050);

    player.dispose();
  });

  it("skips HTTP 404, network errors, and empty streams cleanly while advancing to queued items", async () => {
    const env = createFakeBanterAudioEnv();
    const fetchFn: FetchLike = vi.fn(async (url: string) => {
      if (url.endsWith("/b-404")) {
        return {
          ok: false,
          status: 404,
          body: null,
          json: async () => ({}),
          arrayBuffer: async () => new ArrayBuffer(0),
        };
      }
      if (url.endsWith("/b-net")) {
        throw new Error("LAN unreachable");
      }
      if (url.endsWith("/b-empty")) {
        return makePcmStreamResponse([]);
      }
      return makePcmStreamResponse([makeConstantPcm16(2400)]);
    });
    const player = createBanterVoicePlayer({ fetchFn });

    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-404",
      seat: "South",
      audioUrl: "/api/banter-audio/b-404",
      sampleRate: 24000,
      priority: "normal",
      pan: 0.35,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-net",
      seat: "West",
      audioUrl: "/api/banter-audio/b-net",
      sampleRate: 24000,
      priority: "normal",
      pan: 0,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-empty",
      seat: "North",
      audioUrl: "/api/banter-audio/b-empty",
      sampleRate: 24000,
      priority: "highlight",
      pan: -0.35,
    });

    await vi.waitFor(() => {
      expect(fetchFn).toHaveBeenCalledTimes(3);
    });
    await vi.waitFor(() => {
      expect(player.getPendingBanterIds().size).toBe(0);
    });
    expect(env.sources).toHaveLength(0);
    expect(player.isPlaying()).toBe(false);
    expect(env.sfxGain.gain.value).toBe(1.0);

    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-ok",
      seat: "South",
      audioUrl: "/api/banter-audio/b-ok",
      sampleRate: 24000,
      priority: "normal",
      pan: 0.35,
    });
    await vi.waitFor(() => {
      expect(env.sources).toHaveLength(1);
    });
    expect(player.getActive()).toEqual({ seat: "South", banterId: "b-ok" });

    player.dispose();
  });

  it("preempts active same-seat BanterVoice immediately (aborting in-flight fetch), drops queued items for that seat, and preserves other seats", async () => {
    const env = createFakeBanterAudioEnv();
    const firstControlled = createControlledStream();
    const signals: Array<AbortSignal | undefined> = [];
    const fetchFn: FetchLike = vi.fn(
      async (url: string, init?: FetchInitLike) => {
        signals.push(init?.signal);
        if (url.endsWith("/b-1")) {
          return {
            ok: true,
            status: 200,
            body: firstControlled.stream,
            json: async () => ({}),
            arrayBuffer: async () => new ArrayBuffer(0),
          };
        }
        return makePcmStreamResponse([makeConstantPcm16(2400)]);
      },
    );
    const player = createBanterVoicePlayer({ fetchFn });

    // South starts streaming b-1 (chunk 1 playing while stream is still open)
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-1",
      seat: "South",
      audioUrl: "/api/banter-audio/b-1",
      sampleRate: 24000,
      priority: "normal",
      pan: 0.35,
    });
    firstControlled.push(makeConstantPcm16(4800));
    await vi.waitFor(() => expect(env.sources).toHaveLength(1));
    expect(signals[0]?.aborted).toBe(false);

    // West queues b-2, North queues b-3
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-2",
      seat: "West",
      audioUrl: "/api/banter-audio/b-2",
      sampleRate: 24000,
      priority: "normal",
      pan: 0,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-3",
      seat: "North",
      audioUrl: "/api/banter-audio/b-3",
      sampleRate: 24000,
      priority: "normal",
      pan: -0.35,
    });

    // West sends newer b-4 -> drops West's old b-2 from queue, keeps North's b-3
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-4",
      seat: "West",
      audioUrl: "/api/banter-audio/b-4",
      sampleRate: 24000,
      priority: "normal",
      pan: 0,
    });
    expect(env.sources[0]!.node.stop).not.toHaveBeenCalled();

    // South sends newer b-5 -> aborts b-1's AbortController, stops b-1 source, starts b-5!
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-5",
      seat: "South",
      audioUrl: "/api/banter-audio/b-5",
      sampleRate: 24000,
      priority: "normal",
      pan: 0.35,
    });
    expect(signals[0]?.aborted).toBe(true);
    expect(env.sources[0]!.node.stop).toHaveBeenCalledTimes(1);
    await vi.waitFor(() => expect(env.sources).toHaveLength(2));
    expect(player.getActive()).toEqual({ seat: "South", banterId: "b-5" });

    // Finish b-5 -> North's b-3 plays next (West's old b-2 was dropped)
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
    const { fetchFn } = makeOkPcmFetch();
    const player = createBanterVoicePlayer({ fetchFn });

    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-1",
      seat: "South",
      audioUrl: "/api/banter-audio/b-1",
      sampleRate: 24000,
      priority: "normal",
      pan: 0.35,
    });
    await vi.waitFor(() => expect(env.sources).toHaveLength(1));

    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-2",
      seat: "West",
      audioUrl: "/api/banter-audio/b-2",
      sampleRate: 24000,
      priority: "normal",
      pan: 0,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-3",
      seat: "North",
      audioUrl: "/api/banter-audio/b-3",
      sampleRate: 24000,
      priority: "highlight",
      pan: -0.35,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-4",
      seat: "East",
      audioUrl: "/api/banter-audio/b-4",
      sampleRate: 24000,
      priority: "normal",
      pan: 0,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-5",
      seat: "West",
      audioUrl: "/api/banter-audio/b-5",
      sampleRate: 24000,
      priority: "highlight",
      pan: 0,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-6",
      seat: "East",
      audioUrl: "/api/banter-audio/b-6",
      sampleRate: 24000,
      priority: "normal",
      pan: 0,
    });

    env.sources[0]!.node.onended?.();
    await vi.waitFor(() => expect(env.sources).toHaveLength(2));
    expect(player.getActive()).toEqual({ seat: "North", banterId: "b-3" });

    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-7",
      seat: "East",
      audioUrl: "/api/banter-audio/b-7",
      sampleRate: 24000,
      priority: "highlight",
      pan: 0,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-8",
      seat: "South",
      audioUrl: "/api/banter-audio/b-8",
      sampleRate: 24000,
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

  it("aborts in-flight fetch via AbortController, stops playback, clears queue, and restores sfxGain on flush()", async () => {
    const env = createFakeBanterAudioEnv();
    let resolveInflight!: () => void;
    let capturedSignal: AbortSignal | undefined;
    const fetchFn: FetchLike = vi.fn(
      (_url: string, init?: FetchInitLike) =>
        new Promise<FetchResponseLike>((resolve) => {
          capturedSignal = init?.signal;
          resolveInflight = () =>
            resolve(makePcmStreamResponse([makeConstantPcm16(2400)]));
        }),
    );
    const player = createBanterVoicePlayer({ fetchFn });

    // 1. Flush while fetch is in flight -> aborts AbortSignal and ignores late resolution
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 0.9, {
      banterId: "b-inflight",
      seat: "South",
      audioUrl: "/api/banter-audio/b-inflight",
      sampleRate: 24000,
      priority: "normal",
      pan: 0.35,
    });
    expect(capturedSignal?.aborted).toBe(false);
    player.flush();
    expect(capturedSignal?.aborted).toBe(true);
    resolveInflight();
    await Promise.resolve();
    await Promise.resolve();
    expect(env.sources).toHaveLength(0);
    expect(player.isPlaying()).toBe(false);

    // 2. Flush while playing with queued item -> stops sources, clears queue, restores sfxGain
    const { fetchFn: okFetch } = makeOkPcmFetch();
    const player2 = createBanterVoicePlayer({ fetchFn: okFetch });
    const events: Array<ActiveBanterVoiceState | null> = [];
    player2.subscribe((s) => events.push(s));

    player2.enqueue(env.ctx, env.voGain, env.sfxGain, 0.9, {
      banterId: "b-active",
      seat: "South",
      audioUrl: "/api/banter-audio/b-active",
      sampleRate: 24000,
      priority: "highlight",
      pan: 0.35,
    });
    await vi.waitFor(() => expect(env.sources).toHaveLength(1));
    player2.enqueue(env.ctx, env.voGain, env.sfxGain, 0.9, {
      banterId: "b-queued",
      seat: "West",
      audioUrl: "/api/banter-audio/b-queued",
      sampleRate: 24000,
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
    const { fetchFn } = makeOkPcmFetch();
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
      sampleRate: 24000,
      priority: "normal",
      pan: -0.35,
    });
    await vi.waitFor(() => expect(env.sources).toHaveLength(1));

    env.sources[0]!.node.onended?.();
    expect(env.sfxGain.gain.value).toBeCloseTo(0.8 * 0.45, 5);

    externalActive = false;
    player.dispose();
  });

  it("publishes pendingBanterIds across loading, queued cross-seat items, eviction, and flush", async () => {
    const env = createFakeBanterAudioEnv();
    const { fetchFn } = makeOkPcmFetch();
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
      sampleRate: 24000,
      priority: "normal",
      pan: 0.35,
    });
    expect([...player.getPendingBanterIds()]).toEqual(["b-1"]);

    await vi.waitFor(() => expect(env.sources).toHaveLength(1));
    expect(player.getActive()).toEqual({ seat: "South", banterId: "b-1" });
    expect([...player.getPendingBanterIds()]).toEqual([]);

    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-2",
      seat: "West",
      audioUrl: "/api/banter-audio/b-2",
      sampleRate: 24000,
      priority: "normal",
      pan: 0,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-3",
      seat: "North",
      audioUrl: "/api/banter-audio/b-3",
      sampleRate: 24000,
      priority: "highlight",
      pan: -0.35,
    });
    expect([...player.getPendingBanterIds()].sort()).toEqual(["b-2", "b-3"]);
    expect(snapshots.at(-1)).toEqual({
      active: { seat: "South", banterId: "b-1" },
      pending: ["b-2", "b-3"],
    });

    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-4",
      seat: "East",
      audioUrl: "/api/banter-audio/b-4",
      sampleRate: 24000,
      priority: "normal",
      pan: 0,
    });
    expect([...player.getPendingBanterIds()].sort()).toEqual(["b-3", "b-4"]);

    player.flush();
    expect(player.getActive()).toBeNull();
    expect([...player.getPendingBanterIds()]).toEqual([]);
    expect(snapshots.at(-1)).toEqual({ active: null, pending: [] });

    player.dispose();
  });

  it("fades out smoothly and advances to next queued utterance when stream errors mid-playback", async () => {
    const env = createFakeBanterAudioEnv();
    const controlled = createControlledStream();
    const fetchFn: FetchLike = vi.fn(async (url: string) => {
      if (url.endsWith("/b-mid-err")) {
        return {
          ok: true,
          status: 200,
          body: controlled.stream,
          json: async () => ({}),
          arrayBuffer: async () => new ArrayBuffer(0),
        };
      }
      return makePcmStreamResponse([makeConstantPcm16(2400)]);
    });
    const player = createBanterVoicePlayer({ fetchFn });

    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-mid-err",
      seat: "South",
      audioUrl: "/api/banter-audio/b-mid-err",
      sampleRate: 24000,
      priority: "normal",
      pan: 0.35,
    });
    player.enqueue(env.ctx, env.voGain, env.sfxGain, 1.0, {
      banterId: "b-next",
      seat: "North",
      audioUrl: "/api/banter-audio/b-next",
      sampleRate: 24000,
      priority: "highlight",
      pan: -0.35,
    });

    controlled.push(makeConstantPcm16(12000));
    await vi.waitFor(() => expect(env.sources).toHaveLength(1));
    expect(player.getActive()).toEqual({ seat: "South", banterId: "b-mid-err" });

    // Stream errors mid-flight after chunk 1 is already playing.
    controlled.error(new Error("upstream reset"));
    const slotGain = env.gains[2]!;
    const endTime = 3.0 + BANTER_STREAM_SAFETY_LEAD_SEC + 0.5;
    await vi.waitFor(() => {
      expect(slotGain.gain.linearRampToValueAtTime).toHaveBeenCalledWith(
        0,
        endTime,
      );
    });

    // Once chunk 1 finishes playing out, next queued utterance starts cleanly!
    env.sources[0]!.node.onended?.();
    await vi.waitFor(() => expect(env.sources).toHaveLength(2));
    expect(player.getActive()).toEqual({ seat: "North", banterId: "b-next" });

    player.dispose();
  });
});
