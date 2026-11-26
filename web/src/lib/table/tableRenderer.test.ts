import { Container, type FederatedPointerEvent, Graphics, Text, Texture } from "pixi.js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import type { Tile } from "../protocol/generated/Tile";
import { computeBottomRiverSlotPoint, computeTableLayout } from "./layout";
import { TableSceneRenderer } from "./tableRenderer";
import type { TileAtlas, TileBodyKind } from "./tileAtlas";

function installCanvasDocumentStub(): () => void {
  const prevDocument = globalThis.document;
  const prevCtx2D = (globalThis as { CanvasRenderingContext2D?: unknown })
    .CanvasRenderingContext2D;
  const noop = () => {};
  const fakeCtx = {
    save: noop,
    restore: noop,
    scale: noop,
    translate: noop,
    beginPath: noop,
    closePath: noop,
    rect: noop,
    fill: noop,
    stroke: noop,
    clearRect: noop,
    fillRect: noop,
    fillText: noop,
    strokeText: noop,
    setTransform: noop,
    getImageData: () => ({ data: new Uint8ClampedArray(4) }),
    measureText: (text: string) => ({
      width: Math.max(8, text.length * 8),
      actualBoundingBoxAscent: 8,
      actualBoundingBoxDescent: 2,
      fontBoundingBoxAscent: 9,
      fontBoundingBoxDescent: 3,
      actualBoundingBoxLeft: 0,
      actualBoundingBoxRight: Math.max(8, text.length * 8),
    }),
    fillStyle: "",
    strokeStyle: "",
    lineWidth: 1,
    font: "",
    textAlign: "left",
    textBaseline: "alphabetic",
  };
  (globalThis as { CanvasRenderingContext2D?: unknown }).CanvasRenderingContext2D =
    function CanvasRenderingContext2D() {};
  (globalThis as unknown as { document: unknown }).document = {
    createElement: () => ({
      width: 256,
      height: 256,
      style: {},
      getContext: () => fakeCtx,
    }),
  };
  return () => {
    if (prevDocument === undefined) {
      delete (globalThis as { document?: Document }).document;
    } else {
      globalThis.document = prevDocument;
    }
    if (prevCtx2D === undefined) {
      delete (globalThis as { CanvasRenderingContext2D?: unknown })
        .CanvasRenderingContext2D;
    } else {
      (globalThis as { CanvasRenderingContext2D?: unknown }).CanvasRenderingContext2D =
        prevCtx2D;
    }
  };
}

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

function makePointerEvent(x = 0, y = 0): FederatedPointerEvent {
  return {
    global: { x, y },
    stopPropagation: () => {},
  } as unknown as FederatedPointerEvent;
}

function makeSeatInfo(
  seat: Seat,
  nickname: string,
  overrides: Partial<SeatInfo> = {},
): SeatInfo {
  return {
    seat,
    kind: seat === "East" ? "Human" : "Ai",
    nickname,
    connected: true,
    ai_assisted: false,
    ai_difficulty: null,
    ai_style: null,
    voice_pack_id: null,
    ...overrides,
  };
}

function makeSnapshot(overrides: Partial<TableSnapshot> = {}): TableSnapshot {
  return {
    table_id: "tbl-72",
    phase: "InMatch",
    config: {
      mode: "EastOnly",
      seating: "KeepLobbySeats",
      ai_difficulty: "Intermediate",
      pace: "Standard",
    },
    pace: "Standard",
    seats: [
      makeSeatInfo("East", "阿东"),
      makeSeatInfo("South", "小南"),
      makeSeatInfo("West", "阿西", {
        kind: "Human",
        connected: false,
        ai_assisted: true,
      }),
      makeSeatInfo("North", "北北"),
    ],
    your_seat: "East",
    seat_token: "tok-east",
    pending_destructive_op: null,
    seq: 70,
    server_version: "0.1.0",
    lan_urls: {
      preferred_ipv4_url: null,
      ipv4_urls: [],
      local_hostname_url: null,
    },
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
        closed_tiles: [0, 1, 2, 9, 10, 11, 18, 19, 20, 27, 27, 31, 31],
        drawn_tile: 5,
      },
      seats: [
        {
          seat: "East",
          score: 12,
          hand_count: 14,
          river: Array.from({ length: 12 }, (_, idx) => ({
            tile: idx % 9,
            is_tsumogiri: idx % 3 === 0,
            claimed_by: null,
            seq: idx + 1,
          })),
          melds: [
            { Pon: { tile: 27, from_seat: "South", seq: 20 } },
            { ConcealedKong: { tile: 31, seq: 21 } },
          ],
          last_draw_or_meld_seq: 22,
        },
        {
          seat: "South",
          score: -4,
          hand_count: 13,
          river: Array.from({ length: 5 }, (_, idx) => ({
            tile: 9 + idx,
            is_tsumogiri: false,
            claimed_by: null,
            seq: 30 + idx,
          })),
          melds: [],
          last_draw_or_meld_seq: 30,
        },
        {
          seat: "West",
          score: -8,
          hand_count: 13,
          river: Array.from({ length: 11 }, (_, idx) => ({
            tile: 18 + (idx % 9),
            is_tsumogiri: false,
            claimed_by: null,
            seq: 40 + idx,
          })),
          melds: [{ ConcealedKong: { tile: null, seq: 55 } }],
          last_draw_or_meld_seq: 40,
        },
        {
          seat: "North",
          score: 0,
          hand_count: 13,
          river: Array.from({ length: 6 }, (_, idx) => ({
            tile: idx,
            is_tsumogiri: true,
            claimed_by: null,
            seq: 60 + idx,
          })),
          melds: [],
          last_draw_or_meld_seq: 60,
        },
      ],
      wall_remaining: 42,
      turn: {
        wind_round: "East",
        hand_number: 2,
        dealer_streak: 0,
        dealer: "East",
        active_seat: "East",
      },
      legal_actions: [
        { Discard: { tile: 5, from_drawn: true } },
        { Discard: { tile: 0, from_drawn: false } },
      ],
      last_settlement: null,
      ready_seats: null,
      match_summary: null,
      seq: 70,
    },
    ...overrides,
  };
}

function collectTexts(container: Container): string[] {
  const out: string[] = [];
  for (const child of container.children) {
    if (child instanceof Text) {
      out.push(child.text);
    }
    if (child instanceof Container && child.children.length > 0) {
      out.push(...collectTexts(child));
    }
  }
  return out;
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

describe("TableSceneRenderer standing vs laid bodies & Scheme B integration", () => {
  let restoreDoc: (() => void) | null = null;

  beforeEach(() => {
    restoreDoc = installCanvasDocumentStub();
  });

  afterEach(() => {
    restoreDoc?.();
    restoreDoc = null;
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
      pendingAddedKong: { seat: "East", tile: 27 },
      inputLocked: false,
    });

    // Layers in TableSceneRenderer order:
    // 0 felt, 1 centerSeal, 2 river, 3 ghost, 4 meld, 5 opponentHand, 6 badge, 7 viewerHand, 8 effects
    const riverTextures = collectLayerTextures(stage.children[2] as Container);
    const meldTextures = collectLayerTextures(stage.children[4] as Container);
    const opponentHandTextures = collectLayerTextures(stage.children[5] as Container);
    const viewerHandTextures = collectLayerTextures(stage.children[7] as Container);

    // Rivers use laid tile textures (tile 4 from bottom river, tile 13 from right river)
    expect(riverTextures.has(spy.laidTextures.get(4)!)).toBe(true);
    expect(riverTextures.has(spy.laidTextures.get(13)!)).toBe(true);
    expect(riverTextures.has(spy.standingTextures.get(4)!)).toBe(false);
    expect(riverTextures.has(spy.standingTextures.get(13)!)).toBe(false);

    // Melds use laid tile textures (Pon of 27 + pendingAddedKong 27 + ConcealedKong 31) and laidBackTexture
    expect(meldTextures.has(spy.laidTextures.get(27)!)).toBe(true);
    expect(meldTextures.has(spy.laidTextures.get(31)!)).toBe(true);
    expect(meldTextures.has(spy.laidBackTexture)).toBe(true);
    expect(meldTextures.has(spy.standingTextures.get(27)!)).toBe(false);
    expect(meldTextures.has(spy.backTexture)).toBe(false);

    // Opponent concealed hands use standing backTexture
    expect(opponentHandTextures.has(spy.backTexture)).toBe(true);
    expect(opponentHandTextures.has(spy.laidBackTexture)).toBe(false);

    // Viewer hand uses standing tile textures (e.g. closed 0, drawn 5)
    expect(viewerHandTextures.has(spy.standingTextures.get(0)!)).toBe(true);
    expect(viewerHandTextures.has(spy.standingTextures.get(5)!)).toBe(true);
    expect(viewerHandTextures.has(spy.laidTextures.get(0)!)).toBe(false);
    expect(viewerHandTextures.has(spy.laidTextures.get(5)!)).toBe(false);

    // Hovering legal drawn tile renders laid ghost preview in ghostLayer
    const viewerHandLayer = stage.children[7] as Container;
    const drawnSprite = viewerHandLayer.children.find(
      (child) =>
        "texture" in child && child.texture === spy.standingTextures.get(5),
    );
    expect(drawnSprite).toBeDefined();
    drawnSprite!.emit("pointerover", makePointerEvent());
    const ghostTextures = collectLayerTextures(stage.children[3] as Container);
    expect(ghostTextures.has(spy.laidTextures.get(5)!)).toBe(true);
    expect(ghostTextures.has(spy.standingTextures.get(5)!)).toBe(false);

    renderer.destroy();
    spy.atlas.destroy();
  });

  it("renders Scheme B theater layout, ultra-flat pill center seal, and wind-free relative badges", () => {
    const spy = createSpyAtlas();
    const stage = new Container();
    const onDiscard = vi.fn();
    const renderer = new TableSceneRenderer(stage, spy.atlas, {
      onDiscard,
    });

    renderer.resize(1440, 900);
    renderer.render({
      snapshot: makeSnapshot(),
      pendingAddedKong: { seat: "East", tile: 27 },
      inputLocked: false,
    });

    const layout = renderer.getLayout();
    expect(layout).not.toBeNull();
    expect(layout!.isWide).toBe(true);
    expect(layout!.riverGrid.bottom.perLine).toBe(10);
    expect(layout!.riverGrid.left.perLine).toBe(4);

    const texts = collectTexts(stage).join(" | ");
    expect(texts).toContain("东2");
    expect(texts).toContain("牌墙余 42");
    expect(texts).toContain("自家");
    expect(texts).toContain("下家");
    expect(texts).toContain("对门");
    expect(texts).toContain("上家");
    expect(texts).toContain("庄");
    expect(texts).toContain("离线托管");

    renderer.destroy();
    spy.atlas.destroy();
  });

  it("aligns discard ghost preview with 10-per-row bottom river slot and commits desktop click discard", () => {
    const spy = createSpyAtlas();
    const stage = new Container();
    const onDiscard = vi.fn();
    const renderer = new TableSceneRenderer(stage, spy.atlas, {
      onDiscard,
    });

    renderer.resize(1280, 720);
    renderer.render({
      snapshot: makeSnapshot(),
      pendingAddedKong: null,
      inputLocked: false,
      confirmDiscard: false,
    });

    const layout = renderer.getLayout()!;
    const expectedSlot = computeBottomRiverSlotPoint(layout, 12);
    const expectedSameAsCompute = computeBottomRiverSlotPoint(
      computeTableLayout(1280, 720),
      12,
    );
    expect(expectedSlot).toEqual(expectedSameAsCompute);

    const viewerHandLayer = stage.children[7] as Container;
    const interactiveSprites = viewerHandLayer.children.filter(
      (c) => c.eventMode === "static",
    );
    expect(interactiveSprites.length).toBeGreaterThan(0);

    const targetSprite = interactiveSprites[interactiveSprites.length - 1];
    targetSprite.emit("pointerover", makePointerEvent());

    const ghostChildren = renderer.ghostLayer.children;
    const ghostSprite = ghostChildren[ghostChildren.length - 1];
    expect(ghostSprite.x).toBeCloseTo(expectedSlot.x, 1);
    expect(ghostSprite.y).toBeCloseTo(expectedSlot.y, 1);

    targetSprite.emit(
      "pointerdown",
      makePointerEvent(targetSprite.x, targetSprite.y),
    );
    stage.emit("pointerup", makePointerEvent(targetSprite.x, targetSprite.y));

    expect(onDiscard).toHaveBeenCalledTimes(1);
    expect(onDiscard).toHaveBeenCalledWith(5, true);

    renderer.destroy();
    spy.atlas.destroy();
  });

  it("supports touch tap-twice, swipe-up cancel snap-back, and narrow portrait fallback", () => {
    const spy = createSpyAtlas();
    const stage = new Container();
    const onDiscard = vi.fn();
    const renderer = new TableSceneRenderer(stage, spy.atlas, {
      onDiscard,
    });

    renderer.resize(390, 844);
    renderer.render({
      snapshot: makeSnapshot(),
      pendingAddedKong: { seat: "East", tile: 27 },
      inputLocked: false,
      confirmDiscard: true,
    });

    expect(renderer.getLayout()!.isWide).toBe(false);

    const viewerHandLayer = stage.children[7] as Container;
    const interactiveSprites = viewerHandLayer.children.filter(
      (c) => c.eventMode === "static",
    );
    const tileSprite = interactiveSprites[0];
    const restY = tileSprite.y;

    // First tap selects and shows ghost preview
    tileSprite.emit("pointerdown", makePointerEvent(tileSprite.x, tileSprite.y));
    stage.emit("pointerup", makePointerEvent(tileSprite.x, tileSprite.y));
    expect(onDiscard).not.toHaveBeenCalled();
    expect(renderer.ghostLayer.children.length).toBeGreaterThan(0);

    // Small drag below swipe threshold cancels and snaps back
    tileSprite.emit("pointerdown", makePointerEvent(tileSprite.x, restY));
    stage.emit("globalpointermove", makePointerEvent(tileSprite.x, restY - 12));
    stage.emit("pointerup", makePointerEvent(tileSprite.x, restY - 12));
    expect(onDiscard).not.toHaveBeenCalled();
    expect(tileSprite.y).toBe(restY);

    // Tap twice commits discard
    tileSprite.emit("pointerdown", makePointerEvent(tileSprite.x, restY));
    stage.emit("pointerup", makePointerEvent(tileSprite.x, restY));
    tileSprite.emit("pointerdown", makePointerEvent(tileSprite.x, restY));
    stage.emit("pointerup", makePointerEvent(tileSprite.x, restY));
    expect(onDiscard).toHaveBeenCalledTimes(1);
    expect(onDiscard).toHaveBeenCalledWith(0, false);

    renderer.destroy();
    spy.atlas.destroy();
  });

  it("renders 4-layer turn cues: drawn lift/caret vs post-pon 13-tile highlight, hand dimming, reducedEffects static glow, and onLayoutChange", () => {
    const spy = createSpyAtlas();
    const stage = new Container();
    const onLayoutChange = vi.fn();
    const renderer = new TableSceneRenderer(stage, spy.atlas, {
      onDiscard: () => {},
      onLayoutChange,
    });

    renderer.resize(1280, 720);

    // 1. discard_drawn: drawn tile lifted with ▼ indicator, full brightness, breathing animates on tick
    renderer.render({
      snapshot: makeSnapshot(),
      pendingAddedKong: null,
      inputLocked: false,
      reducedEffects: false,
    });

    expect(onLayoutChange).toHaveBeenCalled();
    const latestLayout = onLayoutChange.mock.calls[onLayoutChange.mock.calls.length - 1][0];
    expect(latestLayout.turnBarRect.width).toBeGreaterThan(0);

    const viewerHandLayer = stage.children[7] as Container;
    expect(collectTexts(viewerHandLayer)).toContain("▼");

    const closedTileSprite = viewerHandLayer.children.find(
      (c) => "texture" in c && c.texture === spy.standingTextures.get(0),
    )!;
    const drawnTileSprite = viewerHandLayer.children.find(
      (c) => "texture" in c && c.texture === spy.standingTextures.get(5),
    )!;
    expect(drawnTileSprite.y).toBeLessThan(closedTileSprite.y);
    expect(closedTileSprite.tint).toBe(0xffffff);
    expect(closedTileSprite.alpha).toBe(1);

    const trayGlow = viewerHandLayer.children.find(
      (c) => c instanceof Graphics && c.label === "turn-tray-breath",
    );
    expect(trayGlow).toBeDefined();
    const initialGlowAlpha = trayGlow!.alpha;
    renderer.tick(425);
    expect(trayGlow!.alpha).not.toBe(initialGlowAlpha);

    // 2. reducedEffects: static highlight does not animate on tick
    renderer.render({
      snapshot: makeSnapshot(),
      pendingAddedKong: null,
      inputLocked: false,
      reducedEffects: true,
    });
    const staticGlow = viewerHandLayer.children.find(
      (c) => c instanceof Graphics && c.label === "turn-tray-breath",
    );
    expect(staticGlow).toBeDefined();
    const staticAlpha = staticGlow!.alpha;
    renderer.tick(425);
    expect(staticGlow!.alpha).toBe(staticAlpha);

    // 3. discard_post_pon (drawn_tile: null): no ▼ indicator, closed hand highlighted uniformly
    const postPonSnap = makeSnapshot();
    postPonSnap.player_view!.hand.closed_tiles = [0, 1, 2, 9, 10, 11, 18, 19, 20, 27];
    postPonSnap.player_view!.hand.drawn_tile = null;
    postPonSnap.player_view!.legal_actions = [{ Discard: { tile: 0, from_drawn: false } }];
    renderer.render({
      snapshot: postPonSnap,
      pendingAddedKong: null,
      inputLocked: false,
      reducedEffects: false,
    });
    expect(collectTexts(viewerHandLayer)).not.toContain("▼");
    const postPonHighlights = viewerHandLayer.children.filter(
      (c) => c instanceof Graphics && c.label === "post-pon-tile-highlight",
    );
    expect(postPonHighlights).toHaveLength(10);

    // 4. idle (waiting on opponent): Layer 3 dims hand, then snaps back on own turn
    const idleSnap = makeSnapshot();
    idleSnap.player_view!.turn.active_seat = "South";
    idleSnap.player_view!.legal_actions = [];
    renderer.render({
      snapshot: idleSnap,
      pendingAddedKong: null,
      inputLocked: false,
      reducedEffects: false,
    });
    expect(collectTexts(viewerHandLayer)).not.toContain("▼");
    const dimmedSprite = viewerHandLayer.children.find(
      (c) => "texture" in c && c.texture === spy.standingTextures.get(0),
    )!;
    expect(dimmedSprite.tint).not.toBe(0xffffff);
    expect(dimmedSprite.alpha).toBeLessThan(1);

    renderer.destroy();
    spy.atlas.destroy();
  });
});
