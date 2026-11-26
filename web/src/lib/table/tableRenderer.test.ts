import { Container, type FederatedPointerEvent, Text, Texture } from "pixi.js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import { computeBottomRiverSlotPoint, computeTableLayout } from "./layout";
import { TableSceneRenderer } from "./tableRenderer";
import type { TileAtlas } from "./tileAtlas";

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

function createStubAtlas(): TileAtlas {
  return {
    width: 1152,
    height: 960,
    backTexture: Texture.EMPTY,
    shadowTexture: Texture.EMPTY,
    feltImpastoTexture: Texture.EMPTY,
    feltBrushTexture: Texture.EMPTY,
    getTileTexture: () => Texture.EMPTY,
    destroy: () => {},
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

describe("TableSceneRenderer Scheme B integration", () => {
  let restoreDoc: (() => void) | null = null;

  beforeEach(() => {
    restoreDoc = installCanvasDocumentStub();
  });

  afterEach(() => {
    restoreDoc?.();
    restoreDoc = null;
  });

  it("renders Scheme B theater layout, ultra-flat pill center seal, and wind-free relative badges", () => {
    const stage = new Container();
    const onDiscard = vi.fn();
    const renderer = new TableSceneRenderer(stage, createStubAtlas(), {
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
  });

  it("aligns discard ghost preview with 10-per-row bottom river slot and commits desktop click discard", () => {
    const stage = new Container();
    const onDiscard = vi.fn();
    const renderer = new TableSceneRenderer(stage, createStubAtlas(), {
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

    // Find the interactive drawn tile sprite in viewerHandLayer
    const viewerHandLayer = stage.children.find((child) =>
      child.children.some((grand) => grand.eventMode === "static"),
    ) as Container | undefined;
    expect(viewerHandLayer).toBeDefined();

    const interactiveSprites = viewerHandLayer!.children.filter(
      (c) => c.eventMode === "static",
    );
    expect(interactiveSprites.length).toBeGreaterThan(0);

    const targetSprite = interactiveSprites[interactiveSprites.length - 1];
    targetSprite.emit("pointerover", makePointerEvent());

    // Ghost preview sprite should be positioned at slot 12 (row 1, col 2 of 10-per-row river)
    const ghostChildren = renderer.ghostLayer.children;
    const ghostSprite = ghostChildren[ghostChildren.length - 1];
    expect(ghostSprite.x).toBeCloseTo(expectedSlot.x, 1);
    expect(ghostSprite.y).toBeCloseTo(expectedSlot.y, 1);

    // Clicking commits discard
    targetSprite.emit(
      "pointerdown",
      makePointerEvent(targetSprite.x, targetSprite.y),
    );
    stage.emit("pointerup", makePointerEvent(targetSprite.x, targetSprite.y));

    expect(onDiscard).toHaveBeenCalledTimes(1);
    expect(onDiscard).toHaveBeenCalledWith(5, true);

    renderer.destroy();
  });

  it("supports touch tap-twice, swipe-up cancel snap-back, and narrow portrait fallback", () => {
    const stage = new Container();
    const onDiscard = vi.fn();
    const renderer = new TableSceneRenderer(stage, createStubAtlas(), {
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

    const viewerHandLayer = stage.children.find((child) =>
      child.children.some((grand) => grand.eventMode === "static"),
    ) as Container;
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
  });
});
