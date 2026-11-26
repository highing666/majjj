import { Container, Texture } from "pixi.js";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import type { Tile } from "../protocol/generated/Tile";
import { TableSceneRenderer } from "./tableRenderer";
import type { TileAtlas, TileBodyKind } from "./tileAtlas";

function createSpyAtlas() {
  const standingTextures = new Map<Tile, Texture>();
  const laidTextures = new Map<Tile, Texture>();
  const backTexture = new Texture();
  const laidBackTexture = new Texture();
  const shadowTexture = new Texture();
  const feltImpastoTexture = new Texture();
  const feltBrushTexture = new Texture();
  const calls: Array<{ tile: Tile; body: TileBodyKind | undefined }> = [];

  for (let t = 0; t < 34; t += 1) {
    standingTextures.set(t, new Texture());
    laidTextures.set(t, new Texture());
  }

  const atlas: TileAtlas = {
    width: 1440,
    height: 1536,
    backTexture,
    laidBackTexture,
    shadowTexture,
    feltImpastoTexture,
    feltBrushTexture,
    getTileTexture(tile: Tile, body?: TileBodyKind): Texture {
      calls.push({ tile, body });
      if (body === "laid") {
        return laidTextures.get(tile) ?? laidBackTexture;
      }
      return standingTextures.get(tile) ?? backTexture;
    },
    destroy() {
      for (const tex of standingTextures.values()) tex.destroy(false);
      for (const tex of laidTextures.values()) tex.destroy(false);
      backTexture.destroy(false);
      laidBackTexture.destroy(false);
      shadowTexture.destroy(false);
      feltImpastoTexture.destroy(false);
      feltBrushTexture.destroy(false);
    },
  };

  return {
    atlas,
    calls,
    standingTextures,
    laidTextures,
    backTexture,
    laidBackTexture,
  };
}

function makeSnapshot(): TableSnapshot {
  return {
    table_id: "t1",
    phase: "InMatch",
    config: {
      mode: "EastOnly",
      seating: "Random",
      ai_difficulty: "Strong",
      pace: "Standard",
    },
    pace: "Standard",
    seats: [
      { seat: "East", kind: "Human", nickname: "玩家甲", connected: true, ai_assisted: false, ai_difficulty: null, ai_style: null, voice_pack_id: null },
      { seat: "South", kind: "Ai", nickname: "AI乙", connected: true, ai_assisted: false, ai_difficulty: "Strong", ai_style: "Balanced", voice_pack_id: null },
      { seat: "West", kind: "Ai", nickname: "AI丙", connected: true, ai_assisted: false, ai_difficulty: "Strong", ai_style: "Balanced", voice_pack_id: null },
      { seat: "North", kind: "Ai", nickname: "AI丁", connected: true, ai_assisted: false, ai_difficulty: "Strong", ai_style: "Balanced", voice_pack_id: null },
    ],
    your_seat: "East",
    seat_token: "tok",
    pending_destructive_op: null,
    seq: 2,
    server_version: "0.1.0",
    lan_urls: { preferred_ipv4_url: null, ipv4_urls: [], local_hostname_url: null },
    host_is_loopback: true,
    player_view: {
      viewer: "East",
      mode: "EastOnly",
      rule_profile: {
        name: "推倒胡",
        allow_chi: false,
        allow_four_of_kind_seven_pairs: true,
        allow_multi_ron: true,
        fan_cap: 8,
        wall_size: 136,
        dead_wall_size: 14,
        starting_scores_east_only: [0, 0, 0, 0],
        starting_scores_free_table: [0, 0, 0, 0],
      },
      phase: "Playing",
      hand: {
        closed_tiles: [0, 1, 2, 9, 10, 11, 18, 19, 20, 27],
        drawn_tile: 31,
      },
      seats: [
        {
          seat: "East",
          score: 0,
          hand_count: 11,
          river: [{ tile: 4, seq: 1, is_tsumogiri: false, claimed_by: null }],
          melds: [{ Pon: { tile: 33, from_seat: "South", seq: 1 } }],
          last_draw_or_meld_seq: 1,
        },
        {
          seat: "South",
          score: 0,
          hand_count: 10,
          river: [{ tile: 14, seq: 2, is_tsumogiri: true, claimed_by: null }],
          melds: [{ ConcealedKong: { tile: null, seq: 2 } }],
          last_draw_or_meld_seq: 2,
        },
        {
          seat: "West",
          score: 0,
          hand_count: 13,
          river: [],
          melds: [],
          last_draw_or_meld_seq: 0,
        },
        {
          seat: "North",
          score: 0,
          hand_count: 13,
          river: [],
          melds: [],
          last_draw_or_meld_seq: 0,
        },
      ],
      wall_remaining: 50,
      turn: {
        wind_round: "East",
        hand_number: 1,
        dealer_streak: 0,
        dealer: "East",
        active_seat: "East",
      },
      legal_actions: [{ Discard: { tile: 31, from_drawn: true } }],
      last_settlement: null,
      ready_seats: null,
      match_summary: null,
      seq: 2,
    },
  };
}

function collectLayerTextures(layer: Container): Set<Texture> {
  const out = new Set<Texture>();
  for (const child of layer.children) {
    if ("texture" in child && child.texture instanceof Texture) {
      out.add(child.texture);
    }
  }
  return out;
}

describe("TableSceneRenderer standing vs laid tile body selection", () => {
  const prevDocument = globalThis.document;
  const prevCtx2d = (globalThis as { CanvasRenderingContext2D?: unknown })
    .CanvasRenderingContext2D;

  beforeEach(() => {
    (
      globalThis as unknown as { CanvasRenderingContext2D: unknown }
    ).CanvasRenderingContext2D = function CanvasRenderingContext2D() {};
    const noop = () => {};
    const fakeCtx = {
      save: noop,
      restore: noop,
      beginPath: noop,
      closePath: noop,
      rect: noop,
      fill: noop,
      stroke: noop,
      fillRect: noop,
      clearRect: noop,
      fillText: noop,
      strokeText: noop,
      scale: noop,
      translate: noop,
      setTransform: noop,
      measureText: () => ({
        width: 24,
        actualBoundingBoxAscent: 10,
        actualBoundingBoxDescent: 3,
        fontBoundingBoxAscent: 12,
        fontBoundingBoxDescent: 4,
      }),
      getImageData: () => ({ data: new Uint8ClampedArray(4) }),
      fillStyle: "",
      strokeStyle: "",
      font: "",
    };
    (globalThis as unknown as { document: unknown }).document = {
      createElement: () => ({
        width: 0,
        height: 0,
        getContext: () => fakeCtx,
      }),
    };
  });

  afterEach(() => {
    if (prevDocument === undefined) {
      delete (globalThis as { document?: Document }).document;
    } else {
      globalThis.document = prevDocument;
    }
    if (prevCtx2d === undefined) {
      delete (globalThis as { CanvasRenderingContext2D?: unknown })
        .CanvasRenderingContext2D;
    } else {
      (
        globalThis as unknown as { CanvasRenderingContext2D?: unknown }
      ).CanvasRenderingContext2D = prevCtx2d;
    }
  });

  it("renders viewer hand with standing textures, rivers and melds with laid textures, and opponent concealed kongs with laidBackTexture", () => {
    const spy = createSpyAtlas();
    const stage = new Container();
    const renderer = new TableSceneRenderer(stage, spy.atlas, {
      onDiscard: () => {},
    });

    renderer.resize(1280, 720);
    renderer.render({
      snapshot: makeSnapshot(),
      pendingAddedKong: { seat: "East", tile: 33 },
      inputLocked: false,
    });

    // Layers in TableSceneRenderer order:
    // 0 felt, 1 centerSeal, 2 river, 3 ghost, 4 meld, 5 opponentHand, 6 badge, 7 viewerHand, 8 effects
    const riverTextures = collectLayerTextures(stage.children[2] as Container);
    const meldTextures = collectLayerTextures(stage.children[4] as Container);
    const opponentHandTextures = collectLayerTextures(stage.children[5] as Container);
    const viewerHandTextures = collectLayerTextures(stage.children[7] as Container);

    // Rivers use laid tile textures (tile 4 & tile 14)
    expect(riverTextures.has(spy.laidTextures.get(4)!)).toBe(true);
    expect(riverTextures.has(spy.laidTextures.get(14)!)).toBe(true);
    expect(riverTextures.has(spy.standingTextures.get(4)!)).toBe(false);
    expect(riverTextures.has(spy.standingTextures.get(14)!)).toBe(false);

    // Melds use laid tile textures (Pon of 33 + pendingAddedKong 33) and laidBackTexture for concealed kong
    expect(meldTextures.has(spy.laidTextures.get(33)!)).toBe(true);
    expect(meldTextures.has(spy.laidBackTexture)).toBe(true);
    expect(meldTextures.has(spy.standingTextures.get(33)!)).toBe(false);
    expect(meldTextures.has(spy.backTexture)).toBe(false);

    // Opponent concealed hands use standing backTexture
    expect(opponentHandTextures.has(spy.backTexture)).toBe(true);
    expect(opponentHandTextures.has(spy.laidBackTexture)).toBe(false);

    // Viewer hand uses standing tile textures (e.g. closed 0, drawn 31)
    expect(viewerHandTextures.has(spy.standingTextures.get(0)!)).toBe(true);
    expect(viewerHandTextures.has(spy.standingTextures.get(31)!)).toBe(true);
    expect(viewerHandTextures.has(spy.laidTextures.get(0)!)).toBe(false);
    expect(viewerHandTextures.has(spy.laidTextures.get(31)!)).toBe(false);

    // Hovering legal drawn tile renders laid ghost preview in ghostLayer
    const viewerHandLayer = stage.children[7] as Container;
    const drawnSprite = viewerHandLayer.children.find(
      (child) =>
        "texture" in child &&
        child.texture === spy.standingTextures.get(31),
    );
    expect(drawnSprite).toBeDefined();
    drawnSprite!.emit("pointerover", {} as never);
    const ghostTextures = collectLayerTextures(stage.children[3] as Container);
    expect(ghostTextures.has(spy.laidTextures.get(31)!)).toBe(true);
    expect(ghostTextures.has(spy.standingTextures.get(31)!)).toBe(false);

    renderer.destroy();
    spy.atlas.destroy();
  });
});
