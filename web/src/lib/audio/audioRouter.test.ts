import { describe, expect, it, vi } from "vitest";
import { defaultDevicePreferences } from "../preferences/devicePreferences";
import {
  mapLocalUiActionToSemanticEvent,
  mapPlayerEventToSemanticEvent,
  mapQuickEmoteBroadcastToSemanticEvent,
  type SemanticMappingContext,
} from "../presentation/semanticEvents";
import type { PlayerEvent } from "../protocol/generated/PlayerEvent";
import type { RevealedHand } from "../protocol/generated/RevealedHand";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import { QUICK_EMOTE_IDS } from "../protocol/personas";
import { ALL_SEATS } from "../protocol/seats";
import { createAudioEventRouter } from "./audioRouter";
import type {
  ActiveBanterVoiceState,
  AudioBufferLike,
  AudioBufferSourceNodeLike,
  AudioContextLike,
  AudioContextStateLike,
  AudioNodeLike,
  AudioParamLike,
  AudioSessionLike,
  AudioStatus,
  BiquadFilterNodeLike,
  DynamicsCompressorNodeLike,
  FetchLike,
  GainNodeLike,
  OscillatorNodeLike,
  SfxPlaybackReceipt,
  SfxSlotId,
  StereoPannerNodeLike,
  VoicePackManifest,
  VoiceSlotId,
  VoiceSpriteSlice,
} from "./audioTypes";
import { ALL_VOICE_SLOTS } from "./voicePackSchema";

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

class FakeEventTarget {
  private listeners = new Map<string, Set<(event?: unknown) => void>>();

  addEventListener(type: string, fn: (event?: unknown) => void): void {
    let set = this.listeners.get(type);
    if (!set) {
      set = new Set();
      this.listeners.set(type, set);
    }
    set.add(fn);
  }

  removeEventListener(type: string, fn: (event?: unknown) => void): void {
    this.listeners.get(type)?.delete(fn);
  }

  dispatch(type: string): void {
    for (const fn of this.listeners.get(type) ?? []) {
      fn({ type });
    }
  }

  listenerCount(type: string): number {
    return this.listeners.get(type)?.size ?? 0;
  }
}

function createFakeAudioEnv(initialState: AudioContextStateLike = "suspended") {
  let state: AudioContextStateLike = initialState;
  const now = 5.0;
  const stateListeners = new Set<() => void>();
  const destination: AudioNodeLike = { connect: vi.fn() };
  const gainNodes: Array<{ node: GainNodeLike; connectedTo: AudioNodeLike[] }> =
    [];
  const compressors: Array<{
    node: DynamicsCompressorNodeLike;
    connectedTo: AudioNodeLike[];
  }> = [];
  const bufferSources: Array<{
    node: AudioBufferSourceNodeLike;
    connectedTo: AudioNodeLike[];
    createdBufferLength: number | null;
    startArgs: Array<[number?, number?, number?]>;
  }> = [];
  const panners: StereoPannerNodeLike[] = [];

  const ctx: AudioContextLike = {
    get state() {
      return state;
    },
    currentTime: now,
    sampleRate: 44100,
    destination,
    resume: vi.fn(async () => {
      if (state === "suspended") {
        state = "running";
        for (const fn of stateListeners) fn();
      }
    }),
    close: vi.fn(async () => {
      state = "closed";
    }),
    createGain: (): GainNodeLike => {
      const connectedTo: AudioNodeLike[] = [];
      const node: GainNodeLike = {
        gain: makeFakeParam(1),
        connect: vi.fn((dest: AudioNodeLike) => {
          connectedTo.push(dest);
          return dest;
        }),
      };
      gainNodes.push({ node, connectedTo });
      return node;
    },
    createDynamicsCompressor: (): DynamicsCompressorNodeLike => {
      const connectedTo: AudioNodeLike[] = [];
      const node: DynamicsCompressorNodeLike = {
        threshold: makeFakeParam(-10),
        knee: makeFakeParam(18),
        ratio: makeFakeParam(6),
        attack: makeFakeParam(0.002),
        release: makeFakeParam(0.12),
        connect: vi.fn((dest: AudioNodeLike) => {
          connectedTo.push(dest);
          return dest;
        }),
      };
      compressors.push({ node, connectedTo });
      return node;
    },
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
      const connectedTo: AudioNodeLike[] = [];
      const startArgs: Array<[number?, number?, number?]> = [];
      const record = {
        node: null as unknown as AudioBufferSourceNodeLike,
        connectedTo,
        createdBufferLength: null as number | null,
        startArgs,
      };
      const node: AudioBufferSourceNodeLike = {
        get buffer() {
          return internalBuffer;
        },
        set buffer(b: AudioBufferLike | null) {
          internalBuffer = b;
          record.createdBufferLength = b ? b.length : null;
        },
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
      let internalBuffer: AudioBufferLike | null = null;
      record.node = node;
      bufferSources.push(record);
      return node;
    },
    createOscillator: (): OscillatorNodeLike => ({
      type: "sine",
      frequency: makeFakeParam(440),
      onended: null,
      connect: vi.fn((dest: AudioNodeLike) => dest),
      start: vi.fn(),
      stop: vi.fn(),
    }),
    createBiquadFilter: (): BiquadFilterNodeLike => ({
      type: "bandpass",
      frequency: makeFakeParam(1000),
      Q: makeFakeParam(1),
      connect: vi.fn((dest: AudioNodeLike) => dest),
    }),
    decodeAudioData: vi.fn(async () => ({
      sampleRate: 44100,
      length: 44100,
      duration: 1,
      getChannelData: () => new Float32Array(44100),
    })),
    addEventListener: (_type, listener) => {
      stateListeners.add(listener);
    },
    removeEventListener: (_type, listener) => {
      stateListeners.delete(listener);
    },
  };

  const audioSession: AudioSessionLike = { type: "ambient" };
  const docTarget = new FakeEventTarget();
  let visibilityState: DocumentVisibilityState = "visible";

  return {
    ctx,
    destination,
    audioSession,
    docTarget,
    gainNodes,
    compressors,
    bufferSources,
    panners,
    setState(next: AudioContextStateLike) {
      state = next;
      for (const fn of stateListeners) fn();
    },
    setVisibility(next: DocumentVisibilityState) {
      visibilityState = next;
      docTarget.dispatch("visibilitychange");
    },
    getVisibilityState: () => visibilityState,
  };
}

function makeSeats(voicePackSeatEast: string | null = null): [SeatInfo, SeatInfo, SeatInfo, SeatInfo] {
  return ALL_SEATS.map((seat, idx) => ({
    seat,
    kind: idx === 0 ? "Human" : "Ai",
    nickname: `玩家 ${idx + 1}`,
    connected: true,
    ai_assisted: false,
    ai_difficulty: idx === 0 ? null : "Strong",
    ai_style: idx === 0 ? null : "Balanced",
    persona_id: null,
    mood: null,
    voice_pack_id: idx === 0 ? voicePackSeatEast : null,
  })) as [SeatInfo, SeatInfo, SeatInfo, SeatInfo];
}

function emptyRevealedHands(): [RevealedHand, RevealedHand, RevealedHand, RevealedHand] {
  return [
    { seat: "East", closed_tiles: [], drawn_tile: null, melds: [] },
    { seat: "South", closed_tiles: [], drawn_tile: null, melds: [] },
    { seat: "West", closed_tiles: [], drawn_tile: null, melds: [] },
    { seat: "North", closed_tiles: [], drawn_tile: null, melds: [] },
  ];
}

function makeHandSettledEvent(
  seq: number,
  outcome:
    | "DrawGame"
    | {
        Win: {
          winners: Array<{
            seat: "East" | "South" | "West" | "North";
            win_kind: "Tsumo" | "Ron" | "RobKongHu";
            total_fan: number;
          }>;
          discarder: "East" | "South" | "West" | "North" | null;
          winning_tile: number;
        };
      },
): PlayerEvent {
  return {
    seq,
    kind: {
      HandSettled: {
        settlement: {
          wind_round: "East",
          hand_number: 1,
          dealer_streak: 0,
          dealer: "East",
          outcome:
            outcome === "DrawGame"
              ? "DrawGame"
              : {
                  Win: {
                    winners: outcome.Win.winners.map((w) => ({
                      seat: w.seat,
                      win_kind: w.win_kind,
                      items: [],
                      total_fan: w.total_fan,
                      base_points: 1,
                    })),
                    discarder: outcome.Win.discarder,
                    winning_tile: outcome.Win.winning_tile,
                  },
                },
          kong_transfers: [],
          score_deltas: [0, 0, 0, 0],
          new_scores: [0, 0, 0, 0],
          revealed_hands: emptyRevealedHands(),
        },
      },
    },
  };
}

function makeContext(
  wallRemaining = 40,
  voicePackSeatEast: string | null = null,
): SemanticMappingContext {
  return {
    viewer: "East",
    seats: makeSeats(voicePackSeatEast),
    wallRemaining,
  };
}

describe("AudioEventRouter", () => {
  it("builds sfxGain + voGain -> masterGain -> DynamicsCompressorNode -> destination graph and syncs volume/mute", () => {
    const env = createFakeAudioEnv("running");
    const prefs = defaultDevicePreferences(false);
    const router = createAudioEventRouter({
      initialPreferences: { ...prefs, master_volume: 0.7, sfx_volume: 0.5 },
      createAudioContext: () => env.ctx,
      getAudioSession: () => env.audioSession,
      documentTarget: env.docTarget,
      getVisibilityState: env.getVisibilityState,
    });

    router.unlockFromGesture();

    // Gain nodes created in order: masterGain, sfxGain, voGain (or sfxGain, voGain, masterGain)
    expect(env.compressors).toHaveLength(1);
    expect(env.compressors[0]!.connectedTo).toContain(env.destination);
    expect(env.gainNodes).toHaveLength(3);

    const masterRecord = env.gainNodes.find((g) =>
      g.connectedTo.includes(env.compressors[0]!.node),
    );
    expect(masterRecord).toBeDefined();
    expect(masterRecord!.node.gain.value).toBeCloseTo(0.7, 5);

    const subGainRecords = env.gainNodes.filter((g) =>
      g.connectedTo.includes(masterRecord!.node),
    );
    expect(subGainRecords).toHaveLength(2);
    const gainValues = subGainRecords.map((g) => g.node.gain.value).sort();
    expect(gainValues).toEqual([0.5, 1]);

    // Mute sets masterGain to 0; unmute restores 0.7
    router.applyPreferences({ ...prefs, master_volume: 0.7, sfx_volume: 0.5, muted: true });
    expect(masterRecord!.node.gain.value).toBe(0);
    router.applyPreferences({ ...prefs, master_volume: 0.7, sfx_volume: 0.5, muted: false });
    expect(masterRecord!.node.gain.value).toBeCloseTo(0.7, 5);

    router.dispose();
  });

  it("layers SFX by public info (tsumogiri vs hand-cut/endgame wall<=24, 3 win tiers), relative panning (-0.35/0/+0.35), and local-only UiActionPrompted", () => {
    const env = createFakeAudioEnv("running");
    const played: SfxPlaybackReceipt[] = [];
    const fakeBackend = {
      getMode: () => "sprite" as const,
      ensureLoaded: vi.fn(async () => "sprite" as const),
      play: vi.fn(
        (
          _ctx: AudioContextLike,
          _dest: AudioNodeLike,
          slot: SfxSlotId,
          opts?: { pan?: number },
        ): SfxPlaybackReceipt => {
          const receipt: SfxPlaybackReceipt = {
            backend: "sprite",
            slot,
            variantIndex: 0,
            playbackRate: 1,
            pan: opts?.pan ?? 0,
          };
          played.push(receipt);
          return receipt;
        },
      ),
      dispose: vi.fn(),
    };

    const router = createAudioEventRouter({
      initialPreferences: defaultDevicePreferences(false),
      createAudioContext: () => env.ctx,
      sfxBackend: fakeBackend,
      documentTarget: env.docTarget,
      getVisibilityState: env.getVisibilityState,
    });

    router.unlockFromGesture();

    // 1. Tsumogiri in mid-game (wall=40) -> discard_light; South (下家) -> +0.35
    const tsumogiriEvent: PlayerEvent = {
      seq: 1,
      kind: { TileDiscarded: { seat: "South", tile: 0, is_tsumogiri: true } },
    };
    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(tsumogiriEvent, makeContext(40)),
    );
    expect(played.at(-1)).toMatchObject({ slot: "discard_light", pan: 0.35 });

    // 2. Hand-cut (is_tsumogiri=false) in mid-game -> discard_heavy; North (上家) -> -0.35
    const handCutEvent: PlayerEvent = {
      seq: 2,
      kind: { TileDiscarded: { seat: "North", tile: 1, is_tsumogiri: false } },
    };
    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(handCutEvent, makeContext(40)),
    );
    expect(played.at(-1)).toMatchObject({ slot: "discard_heavy", pan: -0.35 });

    // 3. Tsumogiri in endgame (wall <= 24) -> discard_heavy; West (对门) -> 0
    const endgameDiscard: PlayerEvent = {
      seq: 3,
      kind: { TileDiscarded: { seat: "West", tile: 2, is_tsumogiri: true } },
    };
    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(endgameDiscard, makeContext(24)),
    );
    expect(played.at(-1)).toMatchObject({ slot: "discard_heavy", pan: 0 });

    // 4. AddedKongAttempted -> added_kong_suspense; MeldDeclared(Pon) -> meld_pon; MeldDeclared(AddedKong) -> meld_kong
    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(
        { seq: 4, kind: { AddedKongAttempted: { seat: "South", tile: 5 } } },
        makeContext(24),
      ),
    );
    expect(played.at(-1)).toMatchObject({
      slot: "added_kong_suspense",
      pan: 0.35,
    });

    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(
        {
          seq: 5,
          kind: {
            MeldDeclared: {
              seat: "North",
              meld: { Pon: { tile: 5, from_seat: "East", seq: 4 } },
              kong_score_deltas: [0, 0, 0, 0],
            },
          },
        },
        makeContext(24),
      ),
    );
    expect(played.at(-1)).toMatchObject({ slot: "meld_pon", pan: -0.35 });

    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(
        {
          seq: 6,
          kind: {
            MeldDeclared: {
              seat: "East",
              meld: { ConcealedKong: { tile: 9, seq: 6 } },
              kong_score_deltas: [6, -2, -2, -2],
            },
          },
        },
        makeContext(24),
      ),
    );
    expect(played.at(-1)).toMatchObject({ slot: "meld_kong", pan: 0 });

    // 5. Win 3 tiers (0-1 / 2-3 / 4 fan) + DrawGame
    const makeWin = (totalFan: number, seat: "East" | "South" = "East"): PlayerEvent =>
      makeHandSettledEvent(7, {
        Win: {
          winners: [{ seat, win_kind: "Tsumo", total_fan: totalFan }],
          discarder: null,
          winning_tile: 0,
        },
      });

    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(makeWin(1), makeContext(20)),
    );
    expect(played.at(-1)?.slot).toBe("win_tier_1");

    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(makeWin(3, "South"), makeContext(20)),
    );
    expect(played.at(-1)).toMatchObject({ slot: "win_tier_2", pan: 0.35 });

    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(makeWin(4), makeContext(20)),
    );
    expect(played.at(-1)?.slot).toBe("win_tier_3");

    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(
        makeHandSettledEvent(8, "DrawGame"),
        makeContext(0),
      ),
    );
    expect(played.at(-1)?.slot).toBe("draw_game");

    // 6. TileDrawn panning & local UI events (hover/select/pass/confirm)
    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(
        { seq: 9, kind: { TileDrawn: { seat: "North", tile: null, is_replacement: false, wall_remaining: 39 } } },
        makeContext(39),
      ),
    );
    expect(played.at(-1)).toMatchObject({ slot: "draw", pan: -0.35 });

    router.handleSemanticEvent(
      mapLocalUiActionToSemanticEvent({ type: "UiTileHover", tile: 0, fromDrawn: false }, makeContext(39)),
    );
    expect(played.at(-1)?.slot).toBe("hover");

    router.handleSemanticEvent(
      mapLocalUiActionToSemanticEvent({ type: "UiTileSelect", tile: 0, fromDrawn: true }, makeContext(39)),
    );
    expect(played.at(-1)?.slot).toBe("select");

    router.handleSemanticEvent(
      mapLocalUiActionToSemanticEvent({ type: "UiPassClicked" }, makeContext(39)),
    );
    expect(played.at(-1)?.slot).toBe("pass");

    router.handleSemanticEvent(
      mapLocalUiActionToSemanticEvent({ type: "UiConfirmNextHand" }, makeContext(39)),
    );
    expect(played.at(-1)?.slot).toBe("confirm");

    // 7. UiActionPrompted strictly local-only (must belong to viewer and have actions)
    const countBefore = played.length;
    const nonSelfPrompt = {
      ...mapLocalUiActionToSemanticEvent(
        { type: "UiActionPrompted", actions: [{ Pon: { tile: 5 } }] },
        makeContext(20),
      ),
      relativeRole: "下家" as const,
    };
    router.handleSemanticEvent(nonSelfPrompt);
    expect(played.length).toBe(countBefore);

    const selfPrompt = mapLocalUiActionToSemanticEvent(
      { type: "UiActionPrompted", actions: [{ Pon: { tile: 5 } }] },
      makeContext(20),
    );
    router.handleSemanticEvent(selfPrompt);
    expect(played.at(-1)?.slot).toBe("action_prompt");

    router.dispose();
  });

  it("routes all 46 VoicePack slots (34 tiles, 6 actions, 6 quick emotes) with single-decode slicing, relative panning, and 30ms ducking", async () => {
    const env = createFakeAudioEnv("running");
    const sprites: Partial<Record<VoiceSlotId, VoiceSpriteSlice>> = {};
    ALL_VOICE_SLOTS.forEach((slot, idx) => {
      sprites[slot] = [idx * 0.75, 0.45];
    });
    const fetchFn: FetchLike = vi.fn(async (url: string) => {
      const match = url.match(/^\/assets\/vo\/([^/]+)\/manifest\.json$/);
      if (match) {
        const manifest: VoicePackManifest = {
          pack_id: match[1]!,
          version: 1,
          audio: "sprite.m4a",
          sample_rate: 24000,
          duration: ALL_VOICE_SLOTS.length * 0.75,
          sprites,
        };
        return {
          ok: true,
          status: 200,
          json: async () => manifest,
          arrayBuffer: async () => new ArrayBuffer(0),
        };
      }
      if (url.endsWith("/sprite.m4a")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({}),
          arrayBuffer: async () => new ArrayBuffer(24),
        };
      }
      return {
        ok: false,
        status: 404,
        json: async () => ({}),
        arrayBuffer: async () => new ArrayBuffer(0),
      };
    });

    const fakeSfxBackend = {
      getMode: () => "procedural" as const,
      ensureLoaded: vi.fn(async () => "procedural" as const),
      play: vi.fn(() => ({
        backend: "procedural" as const,
        slot: "discard_light" as const,
        variantIndex: 0,
        playbackRate: 1,
        pan: 0,
      })),
      dispose: vi.fn(),
    };

    const router = createAudioEventRouter({
      initialPreferences: { ...defaultDevicePreferences(false), sfx_volume: 1 },
      createAudioContext: () => env.ctx,
      sfxBackend: fakeSfxBackend,
      fetchFn,
      documentTarget: env.docTarget,
      getVisibilityState: env.getVisibilityState,
    });
    router.preloadVoicePacks(["human_male", "quick_hothead"]);
    router.unlockFromGesture();

    // Wait for preloaded packs to finish decoding once each
    await vi.waitFor(() => {
      expect(env.ctx.decodeAudioData).toHaveBeenCalledTimes(2);
    });

    const seats = makeSeats("human_male");
    seats[1] = { ...seats[1], voice_pack_id: "quick_hothead" };
    seats[2] = { ...seats[2], voice_pack_id: "tea_sarcasm" };
    seats[3] = { ...seats[3], voice_pack_id: "stubborn_blamer" };
    const ctx: SemanticMappingContext = {
      viewer: "East",
      seats,
      wallRemaining: 30,
    };

    const finishLatestSource = () => {
      env.bufferSources.at(-1)?.node.onended?.();
    };

    // 1. All 34 discard tiles map to m1..z7 in order
    for (let tile = 0; tile < 34; tile += 1) {
      const countBefore = env.bufferSources.length;
      router.handleSemanticEvent(
        mapPlayerEventToSemanticEvent(
          {
            seq: 100 + tile,
            kind: { TileDiscarded: { seat: "East", tile, is_tsumogiri: true } },
          },
          ctx,
        ),
      );
      await vi.waitFor(() => {
        expect(env.bufferSources.length).toBe(countBefore + 1);
      });
      const expectedSlot = ALL_VOICE_SLOTS[tile]!;
      expect(env.bufferSources.at(-1)!.startArgs[0]).toEqual([
        5.0,
        sprites[expectedSlot]![0],
        sprites[expectedSlot]![1],
      ]);
      finishLatestSource();
    }

    // 2. All 6 meld/win action calls + relative panning (South 下家 +0.35, North 上家 -0.35)
    const actionChecks: Array<{
      event: PlayerEvent;
      expectedSlot: VoiceSlotId;
      expectedPan: number;
    }> = [
      {
        event: {
          seq: 201,
          kind: {
            MeldDeclared: {
              seat: "South",
              meld: { Pon: { tile: 0, from_seat: "East", seq: 201 } },
              kong_score_deltas: null,
            },
          },
        },
        expectedSlot: "pong",
        expectedPan: 0.35,
      },
      {
        event: {
          seq: 202,
          kind: {
            MeldDeclared: {
              seat: "North",
              meld: { ExposedKong: { tile: 1, from_seat: "East", seq: 202 } },
              kong_score_deltas: null,
            },
          },
        },
        expectedSlot: "kong",
        expectedPan: -0.35,
      },
      {
        event: {
          seq: 203,
          kind: {
            MeldDeclared: {
              seat: "South",
              meld: { AddedKong: { tile: 2, from_seat: "East", seq: 203 } },
              kong_score_deltas: null,
            },
          },
        },
        expectedSlot: "kong",
        expectedPan: 0.35,
      },
      {
        event: {
          seq: 204,
          kind: {
            MeldDeclared: {
              seat: "East",
              meld: { ConcealedKong: { tile: 3, seq: 204 } },
              kong_score_deltas: null,
            },
          },
        },
        expectedSlot: "concealed_kong",
        expectedPan: 0,
      },
      {
        event: makeHandSettledEvent(205, {
          Win: {
            winners: [{ seat: "East", win_kind: "Tsumo", total_fan: 2 }],
            discarder: null,
            winning_tile: 0,
          },
        }),
        expectedSlot: "zimo",
        expectedPan: 0,
      },
      {
        event: makeHandSettledEvent(206, {
          Win: {
            winners: [{ seat: "South", win_kind: "Ron", total_fan: 1 }],
            discarder: "East",
            winning_tile: 0,
          },
        }),
        expectedSlot: "ron",
        expectedPan: 0.35,
      },
      {
        event: makeHandSettledEvent(207, {
          Win: {
            winners: [{ seat: "North", win_kind: "RobKongHu", total_fan: 2 }],
            discarder: "East",
            winning_tile: 0,
          },
        }),
        expectedSlot: "rob_kong",
        expectedPan: -0.35,
      },
    ];

    for (const check of actionChecks) {
      const countBefore = env.bufferSources.length;
      const pannersBefore = env.panners.length;
      router.handleSemanticEvent(mapPlayerEventToSemanticEvent(check.event, ctx));
      await vi.waitFor(() => {
        expect(env.bufferSources.length).toBe(countBefore + 1);
      });
      expect(env.bufferSources.at(-1)!.startArgs[0]).toEqual([
        5.0,
        sprites[check.expectedSlot]![0],
        sprites[check.expectedSlot]![1],
      ]);
      if (check.expectedPan !== 0) {
        expect(env.panners.length).toBe(pannersBefore + 1);
        expect(env.panners.at(-1)!.pan.value).toBeCloseTo(check.expectedPan, 5);
      }
      // Verify sfxGain ducked to 0.45 while active
      const sfxGainNode = env.gainNodes[1]!.node;
      expect(sfxGainNode.gain.value).toBeCloseTo(0.45, 5);
      finishLatestSource();
    }

    // 3. All 6 QuickEmotes trigger their canonical emote_* slots
    const expectedEmoteSlots: VoiceSlotId[] = [
      "emote_is_that_it",
      "emote_hurry_up",
      "emote_spare_me",
      "emote_nice_play",
      "emote_dealer_rigged",
      "emote_going_big",
    ];
    for (let i = 0; i < QUICK_EMOTE_IDS.length; i += 1) {
      const countBefore = env.bufferSources.length;
      router.handleSemanticEvent(
        mapQuickEmoteBroadcastToSemanticEvent(
          { seat: "East", emote: QUICK_EMOTE_IDS[i]!, text: "互动" },
          ctx,
        ),
      );
      await vi.waitFor(() => {
        expect(env.bufferSources.length).toBe(countBefore + 1);
      });
      const expectedSlot = expectedEmoteSlots[i]!;
      expect(env.bufferSources.at(-1)!.startArgs[0]).toEqual([
        5.0,
        sprites[expectedSlot]![0],
        sprites[expectedSlot]![1],
      ]);
      finishLatestSource();
    }

    router.dispose();
  });

  it("degrades silently when VoicePack returns 404 while keeping SFX playback intact", async () => {
    const env = createFakeAudioEnv("running");
    const fetch404: FetchLike = vi.fn(async () => ({
      ok: false,
      status: 404,
      json: async () => ({}),
      arrayBuffer: async () => new ArrayBuffer(0),
    }));
    const sfxPlay = vi.fn(() => ({
      backend: "procedural" as const,
      slot: "discard_heavy" as const,
      variantIndex: 0,
      playbackRate: 1,
      pan: 0,
    }));
    const router = createAudioEventRouter({
      initialPreferences: { ...defaultDevicePreferences(false), sfx_volume: 1 },
      createAudioContext: () => env.ctx,
      sfxBackend: {
        getMode: () => "procedural" as const,
        ensureLoaded: vi.fn(async () => "procedural" as const),
        play: sfxPlay,
        dispose: vi.fn(),
      },
      fetchFn: fetch404,
      documentTarget: env.docTarget,
      getVisibilityState: env.getVisibilityState,
    });
    router.unlockFromGesture();

    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(
        { seq: 1, kind: { TileDiscarded: { seat: "East", tile: 0, is_tsumogiri: false } } },
        makeContext(30, "human_male"),
      ),
    );

    await vi.waitFor(() => {
      expect(fetch404).toHaveBeenCalledWith("/assets/vo/human_male/manifest.json");
    });
    expect(sfxPlay).toHaveBeenCalledTimes(1);
    expect(env.gainNodes[1]!.node.gain.value).toBe(1);
    router.dispose();
  });

  it("enforces Safari audioSession, W3C interrupted guard, 1-sample gesture unlock, visibilitychange resume, and needsResumeCapsule", () => {
    const env = createFakeAudioEnv("suspended");
    const gestureTarget = new FakeEventTarget();
    const statuses: AudioStatus[] = [];

    const router = createAudioEventRouter({
      initialPreferences: defaultDevicePreferences(false),
      createAudioContext: () => env.ctx,
      getAudioSession: () => env.audioSession,
      documentTarget: env.docTarget,
      getVisibilityState: env.getVisibilityState,
    });

    router.subscribe((s) => statuses.push(s));

    // Default before first gesture: uninitialized -> needsResumeCapsule is true
    expect(router.getStatus().state).toBe("uninitialized");
    expect(router.getStatus().needsResumeCapsule).toBe(true);

    // Register synchronous gesture unlock on pointerdown/touchend/click/keydown
    const unbind = router.bindGestureUnlock(gestureTarget);
    expect(gestureTarget.listenerCount("pointerdown")).toBe(1);
    expect(gestureTarget.listenerCount("touchend")).toBe(1);
    expect(gestureTarget.listenerCount("click")).toBe(1);
    expect(gestureTarget.listenerCount("keydown")).toBe(1);

    // Synchronous gesture while suspended -> sets audioSession='playback', calls resume(), plays 1-sample silent buffer
    gestureTarget.dispatch("pointerdown");
    expect(env.audioSession.type).toBe("playback");
    expect(env.ctx.resume).toHaveBeenCalledTimes(1);
    expect(
      env.bufferSources.some(
        (s) =>
          s.createdBufferLength === 1 &&
          s.connectedTo.includes(env.destination) &&
          s.startArgs[0]?.[0] === 0,
      ),
    ).toBe(true);
    expect(router.getStatus().state).toBe("running");
    expect(router.getStatus().needsResumeCapsule).toBe(false);

    // Toggle iOS audioSession to ambient
    router.applyPreferences({
      ...defaultDevicePreferences(false),
      ios_audio_session: "ambient",
    });
    expect(env.audioSession.type).toBe("ambient");

    // Transition to W3C 'interrupted' (e.g. Siri/phone call on iPhone)
    env.setState("interrupted");
    expect(router.getStatus().needsResumeCapsule).toBe(true);

    const resumeCallsBeforeInterrupted = (
      env.ctx.resume as ReturnType<typeof vi.fn>
    ).mock.calls.length;

    // MUST NEVER call resume() while state === "interrupted"
    router.unlockFromGesture();
    env.setVisibility("visible");
    router.handleSemanticEvent(
      mapLocalUiActionToSemanticEvent(
        { type: "UiConfirmNextHand" },
        makeContext(30),
      ),
    );
    expect((env.ctx.resume as ReturnType<typeof vi.fn>).mock.calls.length).toBe(
      resumeCallsBeforeInterrupted,
    );

    // Transition to 'suspended', then visibilitychange -> visible resumes
    env.setState("suspended");
    expect(router.getStatus().needsResumeCapsule).toBe(true);
    env.setVisibility("visible");
    expect((env.ctx.resume as ReturnType<typeof vi.fn>).mock.calls.length).toBe(
      resumeCallsBeforeInterrupted + 1,
    );
    expect(router.getStatus().needsResumeCapsule).toBe(false);

    // Muting hides the resume capsule even if suspended
    env.setState("suspended");
    expect(router.getStatus().needsResumeCapsule).toBe(true);
    router.applyPreferences({
      ...defaultDevicePreferences(false),
      muted: true,
    });
    expect(router.getStatus().needsResumeCapsule).toBe(false);

    unbind();
    expect(gestureTarget.listenerCount("pointerdown")).toBe(0);
    expect(gestureTarget.listenerCount("touchend")).toBe(0);
    expect(gestureTarget.listenerCount("click")).toBe(0);
    expect(gestureTarget.listenerCount("keydown")).toBe(0);

    router.dispose();
  });

  it("enforces unified dual-track arbitration: discard yields during BanterVoice, action calls & QuickEmote fire immediately, and shared ducking holds until both tracks finish", async () => {
    const env = createFakeAudioEnv("running");
    const createAudioContext = vi.fn(() => env.ctx);
    const sprites: Partial<Record<VoiceSlotId, VoiceSpriteSlice>> = {};
    ALL_VOICE_SLOTS.forEach((slot, idx) => {
      sprites[slot] = [idx * 0.75, 0.45];
    });
    const fetchFn: FetchLike = vi.fn(async (url: string) => {
      const match = url.match(/^\/assets\/vo\/([^/]+)\/manifest\.json$/);
      if (match) {
        const manifest: VoicePackManifest = {
          pack_id: match[1]!,
          version: 1,
          audio: "sprite.m4a",
          sample_rate: 24000,
          duration: ALL_VOICE_SLOTS.length * 0.75,
          sprites,
        };
        return {
          ok: true,
          status: 200,
          json: async () => manifest,
          arrayBuffer: async () => new ArrayBuffer(0),
        };
      }
      if (url.endsWith("/sprite.m4a") || url.startsWith("/api/banter-audio/")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({}),
          arrayBuffer: async () => new ArrayBuffer(48),
        };
      }
      return {
        ok: false,
        status: 404,
        json: async () => ({}),
        arrayBuffer: async () => new ArrayBuffer(0),
      };
    });
    const fakeSfxBackend = {
      getMode: () => "procedural" as const,
      ensureLoaded: vi.fn(async () => "procedural" as const),
      play: vi.fn(() => ({
        backend: "procedural" as const,
        slot: "discard_light" as const,
        variantIndex: 0,
        playbackRate: 1,
        pan: 0,
      })),
      dispose: vi.fn(),
    };

    const router = createAudioEventRouter({
      initialPreferences: { ...defaultDevicePreferences(false), sfx_volume: 0.8 },
      createAudioContext,
      sfxBackend: fakeSfxBackend,
      fetchFn,
      documentTarget: env.docTarget,
      getVisibilityState: env.getVisibilityState,
    });
    router.preloadVoicePacks(["human_male", "quick_hothead"]);
    router.unlockFromGesture();
    await vi.waitFor(() => {
      expect(env.ctx.decodeAudioData).toHaveBeenCalledTimes(2);
    });

    const seats = makeSeats("human_male");
    seats[1] = { ...seats[1], voice_pack_id: "quick_hothead" };
    const ctx: SemanticMappingContext = { viewer: "East", seats, wallRemaining: 30 };

    const speakingHistory: Array<ActiveBanterVoiceState | null> = [];
    router.onBanterVoiceStateChange((active) => speakingHistory.push(active));
    expect(speakingHistory).toEqual([null]);

    // 1. Start BanterVoice on South (下家 -> pan +0.35)
    router.enqueueBanterVoice({
      banterId: "b-1",
      seat: "South",
      audioUrl: "/api/banter-audio/b-1",
      durationMs: 1400,
      priority: "normal",
      relativeRole: "下家",
    });
    await vi.waitFor(() => {
      expect(env.bufferSources).toHaveLength(1);
    });
    const banterSource = env.bufferSources[0]!.node;
    expect(createAudioContext).toHaveBeenCalledTimes(1);
    expect(env.panners.at(-1)!.pan.value).toBeCloseTo(0.35, 5);
    expect(router.getActiveBanterVoice()).toEqual({ seat: "South", banterId: "b-1" });
    expect(speakingHistory.at(-1)).toEqual({ seat: "South", banterId: "b-1" });
    const sfxGainNode = env.gainNodes[1]!.node;
    expect(sfxGainNode.gain.value).toBeCloseTo(0.8 * 0.45, 5);

    // 2. Discard tile call arrives during BanterVoice -> MUST silently yield (SFX still fires, no new VO source!)
    const sfxCallsBefore = fakeSfxBackend.play.mock.calls.length;
    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(
        { seq: 10, kind: { TileDiscarded: { seat: "East", tile: 0, is_tsumogiri: true } } },
        ctx,
      ),
    );
    await Promise.resolve();
    expect(fakeSfxBackend.play.mock.calls.length).toBe(sfxCallsBefore + 1);
    expect(env.bufferSources).toHaveLength(1);
    expect(banterSource.stop).not.toHaveBeenCalled();

    // 3. Action call (Pon) arrives during BanterVoice -> fires IMMEDIATELY without waiting for BanterVoice!
    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(
        {
          seq: 11,
          kind: {
            MeldDeclared: {
              seat: "South",
              meld: { Pon: { tile: 0, from_seat: "East", seq: 10 } },
              kong_score_deltas: null,
            },
          },
        },
        ctx,
      ),
    );
    await vi.waitFor(() => {
      expect(env.bufferSources).toHaveLength(2);
    });
    const ponSource = env.bufferSources[1]!.node;
    expect(banterSource.stop).not.toHaveBeenCalled();

    // 4. Pon finishes while BanterVoice is STILL playing -> sfxGain stays ducked at 0.8 * 0.45!
    ponSource.onended?.();
    expect(sfxGainNode.gain.value).toBeCloseTo(0.8 * 0.45, 5);

    // 5. Human QuickEmote arrives during BanterVoice -> fires IMMEDIATELY!
    router.handleSemanticEvent(
      mapQuickEmoteBroadcastToSemanticEvent(
        { seat: "East", emote: "IsThatIt", text: "就这？" },
        ctx,
      ),
    );
    await vi.waitFor(() => {
      expect(env.bufferSources).toHaveLength(3);
    });
    const emoteSource = env.bufferSources[2]!.node;

    // 6. BanterVoice finishes while QuickEmote is STILL playing -> sfxGain stays ducked until QuickEmote ends!
    banterSource.onended?.();
    expect(router.getActiveBanterVoice()).toBeNull();
    expect(sfxGainNode.gain.value).toBeCloseTo(0.8 * 0.45, 5);

    // 7. QuickEmote finishes -> BOTH tracks idle -> sfxGain restores cleanly to 0.8!
    emoteSource.onended?.();
    expect(sfxGainNode.gain.value).toBeCloseTo(0.8, 5);

    router.dispose();
  });

  it("flushes BanterVoice and restores sfxGain on HandStarted semantic event and flushBanterVoices()", async () => {
    const env = createFakeAudioEnv("running");
    const fetchFn: FetchLike = vi.fn(async () => ({
      ok: true,
      status: 200,
      json: async () => ({}),
      arrayBuffer: async () => new ArrayBuffer(32),
    }));
    const router = createAudioEventRouter({
      initialPreferences: { ...defaultDevicePreferences(false), sfx_volume: 1.0 },
      createAudioContext: () => env.ctx,
      fetchFn,
      documentTarget: env.docTarget,
      getVisibilityState: env.getVisibilityState,
    });
    router.unlockFromGesture();

    // Enqueue 2 banters (1 active + 1 queued)
    router.enqueueBanterVoice({
      banterId: "b-settle-1",
      seat: "South",
      audioUrl: "/api/banter-audio/b-settle-1",
      durationMs: 2000,
      priority: "highlight",
      relativeRole: "下家",
    });
    router.enqueueBanterVoice({
      banterId: "b-settle-2",
      seat: "North",
      audioUrl: "/api/banter-audio/b-settle-2",
      durationMs: 2000,
      priority: "highlight",
      relativeRole: "上家",
    });
    await vi.waitFor(() => expect(env.bufferSources).toHaveLength(1));
    const sfxGainNode = env.gainNodes[1]!.node;
    expect(sfxGainNode.gain.value).toBeCloseTo(0.45, 5);
    expect(router.getActiveBanterVoice()).toEqual({
      seat: "South",
      banterId: "b-settle-1",
    });

    // HandStarted semantic event stops active banter, clears queue, and restores sfxGain!
    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(
        {
          seq: 20,
          kind: {
            HandStarted: {
              wind_round: "East",
              hand_number: 2,
              dealer_streak: 0,
              dealer: "South",
              hand: { closed_tiles: [0, 1, 2], drawn_tile: null },
              wall_remaining: 70,
              scores: [0, 0, 0, 0],
            },
          },
        },
        makeContext(70),
      ),
    );
    expect(env.bufferSources[0]!.node.stop).toHaveBeenCalledTimes(1);
    expect(router.getActiveBanterVoice()).toBeNull();
    expect(sfxGainNode.gain.value).toBeCloseTo(1.0, 5);
    // Queued b-settle-2 must NOT have started
    await Promise.resolve();
    expect(env.bufferSources).toHaveLength(1);

    // Direct flushBanterVoices() also stops active banter & restores sfxGain
    router.enqueueBanterVoice({
      banterId: "b-lobby-1",
      seat: "West",
      audioUrl: "/api/banter-audio/b-lobby-1",
      durationMs: 1200,
      priority: "normal",
      relativeRole: "对门",
    });
    await vi.waitFor(() => expect(env.bufferSources).toHaveLength(2));
    expect(sfxGainNode.gain.value).toBeCloseTo(0.45, 5);

    router.flushBanterVoices();
    expect(env.bufferSources[1]!.node.stop).toHaveBeenCalledTimes(1);
    expect(router.getActiveBanterVoice()).toBeNull();
    expect(sfxGainNode.gain.value).toBeCloseTo(1.0, 5);

    router.dispose();
  });

  it("supports runtime voice controls (setVoiceVolume, setTileCallVoiceEnabled, setBanterVoiceEnabled) for #87", async () => {
    const env = createFakeAudioEnv("running");
    const sprites: Partial<Record<VoiceSlotId, VoiceSpriteSlice>> = {};
    ALL_VOICE_SLOTS.forEach((slot, idx) => {
      sprites[slot] = [idx * 0.75, 0.45];
    });
    const fetchFn: FetchLike = vi.fn(async (url: string) => {
      if (url.endsWith("/manifest.json")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({
            pack_id: "human_male",
            version: 1,
            audio: "sprite.m4a",
            sample_rate: 24000,
            duration: ALL_VOICE_SLOTS.length * 0.75,
            sprites,
          }),
          arrayBuffer: async () => new ArrayBuffer(0),
        };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({}),
        arrayBuffer: async () => new ArrayBuffer(32),
      };
    });

    const router = createAudioEventRouter({
      initialPreferences: { ...defaultDevicePreferences(false), sfx_volume: 0.9 },
      createAudioContext: () => env.ctx,
      sfxBackend: {
        getMode: () => "procedural" as const,
        ensureLoaded: vi.fn(async () => "procedural" as const),
        play: vi.fn(() => ({
          backend: "procedural" as const,
          slot: "discard_light" as const,
          variantIndex: 0,
          playbackRate: 1,
          pan: 0,
        })),
        dispose: vi.fn(),
      },
      fetchFn,
      documentTarget: env.docTarget,
      getVisibilityState: env.getVisibilityState,
    });
    router.preloadVoicePacks(["human_male"]);
    router.unlockFromGesture();
    await vi.waitFor(() => expect(env.ctx.decodeAudioData).toHaveBeenCalledTimes(1));

    // 1. setVoiceVolume updates voGain without changing sfxGain
    const sfxGainNode = env.gainNodes[1]!.node;
    const voGainNode = env.gainNodes[2]!.node;
    router.setVoiceVolume(0.4);
    expect(voGainNode.gain.value).toBeCloseTo(0.4, 5);
    expect(sfxGainNode.gain.value).toBeCloseTo(0.9, 5);

    // 2. setTileCallVoiceEnabled(false) skips Discard VO only (Pon still plays!)
    const ctx = makeContext(30, "human_male");
    router.setTileCallVoiceEnabled(false);
    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(
        { seq: 1, kind: { TileDiscarded: { seat: "East", tile: 0, is_tsumogiri: true } } },
        ctx,
      ),
    );
    await Promise.resolve();
    expect(env.bufferSources).toHaveLength(0);

    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(
        {
          seq: 2,
          kind: {
            MeldDeclared: {
              seat: "East",
              meld: { Pon: { tile: 0, from_seat: "South", seq: 1 } },
              kong_score_deltas: null,
            },
          },
        },
        ctx,
      ),
    );
    await vi.waitFor(() => expect(env.bufferSources).toHaveLength(1));
    env.bufferSources[0]!.node.onended?.();

    // 3. setBanterVoiceEnabled(false) stops active banter immediately and skips new banter fetches
    router.enqueueBanterVoice({
      banterId: "b-toggle-1",
      seat: "South",
      audioUrl: "/api/banter-audio/b-toggle-1",
      durationMs: 1000,
      priority: "normal",
      relativeRole: "下家",
    });
    await vi.waitFor(() => expect(env.bufferSources).toHaveLength(2));

    router.setBanterVoiceEnabled(false);
    expect(env.bufferSources[1]!.node.stop).toHaveBeenCalledTimes(1);
    expect(router.getActiveBanterVoice()).toBeNull();

    const fetchCountAfterDisable = (fetchFn as ReturnType<typeof vi.fn>).mock.calls.length;
    router.enqueueBanterVoice({
      banterId: "b-toggle-2",
      seat: "West",
      audioUrl: "/api/banter-audio/b-toggle-2",
      durationMs: 1000,
      priority: "highlight",
      relativeRole: "对门",
    });
    await Promise.resolve();
    expect((fetchFn as ReturnType<typeof vi.fn>).mock.calls.length).toBe(
      fetchCountAfterDisable,
    );

    router.dispose();
  });

  it("syncs voice_volume, tile_call_voice_enabled, and banter_voice_enabled via applyPreferences without disturbing sfxGain or Safari interrupted state", async () => {
    const env = createFakeAudioEnv("running");
    const sprites: Partial<Record<VoiceSlotId, VoiceSpriteSlice>> = {};
    ALL_VOICE_SLOTS.forEach((slot, idx) => {
      sprites[slot] = [idx * 0.75, 0.45];
    });
    const fetchFn: FetchLike = vi.fn(async (url: string) => {
      if (url.endsWith("/manifest.json")) {
        return {
          ok: true,
          status: 200,
          json: async () => ({
            pack_id: "human_male",
            version: 1,
            audio: "sprite.m4a",
            sample_rate: 24000,
            duration: ALL_VOICE_SLOTS.length * 0.75,
            sprites,
          }),
          arrayBuffer: async () => new ArrayBuffer(0),
        };
      }
      return {
        ok: true,
        status: 200,
        json: async () => ({}),
        arrayBuffer: async () => new ArrayBuffer(32),
      };
    });

    const initialPrefs = {
      ...defaultDevicePreferences(false),
      sfx_volume: 0.85,
      voice_volume: 0.6,
      tile_call_voice_enabled: true,
      banter_voice_enabled: true,
    };
    const router = createAudioEventRouter({
      initialPreferences: initialPrefs,
      createAudioContext: () => env.ctx,
      getAudioSession: () => env.audioSession,
      sfxBackend: {
        getMode: () => "procedural" as const,
        ensureLoaded: vi.fn(async () => "procedural" as const),
        play: vi.fn(() => ({
          backend: "procedural" as const,
          slot: "discard_light" as const,
          variantIndex: 0,
          playbackRate: 1,
          pan: 0,
        })),
        dispose: vi.fn(),
      },
      fetchFn,
      documentTarget: env.docTarget,
      getVisibilityState: env.getVisibilityState,
    });
    router.preloadVoicePacks(["human_male"]);
    router.unlockFromGesture();
    await vi.waitFor(() => expect(env.ctx.decodeAudioData).toHaveBeenCalledTimes(1));

    const sfxGainNode = env.gainNodes[1]!.node;
    const voGainNode = env.gainNodes[2]!.node;
    expect(voGainNode.gain.value).toBeCloseTo(0.6, 5);
    expect(sfxGainNode.gain.value).toBeCloseTo(0.85, 5);
    expect(router.getStatus()).toMatchObject({
      voiceVolume: 0.6,
      tileCallVoiceEnabled: true,
      banterVoiceEnabled: true,
    });

    // Adjusting voice_volume via applyPreferences smoothly updates voGain ONLY (sfxGain untouched!)
    const sfxSetTargetBefore = (
      sfxGainNode.gain.setTargetAtTime as ReturnType<typeof vi.fn>
    ).mock.calls.length;
    router.applyPreferences({ ...initialPrefs, voice_volume: 0.35 });
    expect(voGainNode.gain.value).toBeCloseTo(0.35, 5);
    expect(sfxGainNode.gain.value).toBeCloseTo(0.85, 5);
    expect(
      (sfxGainNode.gain.setTargetAtTime as ReturnType<typeof vi.fn>).mock.calls
        .length,
    ).toBe(sfxSetTargetBefore);
    expect(router.getStatus().voiceVolume).toBeCloseTo(0.35, 5);

    // Turning off tile_call_voice_enabled via applyPreferences skips discard calls while keeping action calls + BanterVoice
    router.applyPreferences({
      ...initialPrefs,
      voice_volume: 0.35,
      tile_call_voice_enabled: false,
    });
    const ctx = makeContext(30, "human_male");
    router.handleSemanticEvent(
      mapPlayerEventToSemanticEvent(
        { seq: 1, kind: { TileDiscarded: { seat: "East", tile: 0, is_tsumogiri: true } } },
        ctx,
      ),
    );
    await Promise.resolve();
    expect(env.bufferSources).toHaveLength(0);

    router.enqueueBanterVoice({
      banterId: "b-pref-1",
      seat: "South",
      audioUrl: "/api/banter-audio/b-pref-1",
      durationMs: 1200,
      priority: "normal",
      relativeRole: "下家",
    });
    await vi.waitFor(() => expect(env.bufferSources).toHaveLength(1));
    expect(router.getActiveBanterVoice()).toEqual({
      seat: "South",
      banterId: "b-pref-1",
    });

    // Turning off banter_voice_enabled via applyPreferences immediately stops active BanterVoice & restores sfxGain ducking
    router.applyPreferences({
      ...initialPrefs,
      voice_volume: 0.35,
      tile_call_voice_enabled: false,
      banter_voice_enabled: false,
    });
    expect(env.bufferSources[0]!.node.stop).toHaveBeenCalledTimes(1);
    expect(router.getActiveBanterVoice()).toBeNull();
    expect(sfxGainNode.gain.value).toBeCloseTo(0.85, 5);

    // Re-enable banter voice, enter Safari interrupted state, verify applyPreferences never calls resume()
    router.applyPreferences({
      ...initialPrefs,
      ios_audio_session: "ambient",
      banter_voice_enabled: true,
    });
    expect(env.audioSession.type).toBe("ambient");
    env.setState("interrupted");
    const resumeCountBefore = (env.ctx.resume as ReturnType<typeof vi.fn>).mock
      .calls.length;
    router.applyPreferences({
      ...initialPrefs,
      ios_audio_session: "playback",
      voice_volume: 0.9,
      banter_voice_enabled: true,
    });
    expect(env.audioSession.type).toBe("playback");
    expect((env.ctx.resume as ReturnType<typeof vi.fn>).mock.calls.length).toBe(
      resumeCountBefore,
    );
    expect(router.getStatus().needsResumeCapsule).toBe(true);

    // Enqueueing BanterVoice while interrupted does not fetch or play
    const fetchCountWhileInterrupted = (fetchFn as ReturnType<typeof vi.fn>)
      .mock.calls.length;
    router.enqueueBanterVoice({
      banterId: "b-pref-interrupted",
      seat: "West",
      audioUrl: "/api/banter-audio/b-pref-interrupted",
      durationMs: 1000,
      priority: "highlight",
      relativeRole: "对门",
    });
    await Promise.resolve();
    expect((fetchFn as ReturnType<typeof vi.fn>).mock.calls.length).toBe(
      fetchCountWhileInterrupted,
    );

    router.dispose();
  });
});
