import { Container, Graphics, Text, Texture } from "pixi.js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import { computeBottomRiverSlotPoint, computeTableLayout } from "./layout";
import { TableSceneRenderer } from "./tableRenderer";
import {
  createSpyTileAtlas,
  installCanvasDocumentStub,
  makePlayerView,
  makePointerEvent,
  makePublicSeatView,
  makeSeatInfo,
  makeTableSnapshot,
} from "./testFixtures";

function makeSnapshot(overrides: Partial<TableSnapshot> = {}): TableSnapshot {
  return makeTableSnapshot({
    table_id: "tbl-72",
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
    seq: 70,
    player_view: makePlayerView({
      seats: [
        makePublicSeatView("East", {
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
        }),
        makePublicSeatView("South", {
          score: -4,
          river: Array.from({ length: 5 }, (_, idx) => ({
            tile: 9 + idx,
            is_tsumogiri: false,
            claimed_by: null,
            seq: 30 + idx,
          })),
          last_draw_or_meld_seq: 30,
        }),
        makePublicSeatView("West", {
          score: -8,
          river: Array.from({ length: 11 }, (_, idx) => ({
            tile: 18 + (idx % 9),
            is_tsumogiri: false,
            claimed_by: null,
            seq: 40 + idx,
          })),
          melds: [{ ConcealedKong: { tile: null, seq: 55 } }],
          last_draw_or_meld_seq: 40,
        }),
        makePublicSeatView("North", {
          score: 0,
          river: Array.from({ length: 6 }, (_, idx) => ({
            tile: idx,
            is_tsumogiri: true,
            claimed_by: null,
            seq: 60 + idx,
          })),
          last_draw_or_meld_seq: 60,
        }),
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
      seq: 70,
    }),
    ...overrides,
  });
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
    const spy = createSpyTileAtlas();
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
    const spy = createSpyTileAtlas();
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
    expect(layout!.riverGrid.left).toEqual({ perLine: 6, maxLines: 2 });
    expect(layout!.riverGrid.right).toEqual({ perLine: 6, maxLines: 2 });

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
    const spy = createSpyTileAtlas();
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
      computeTableLayout(1280, 720, {
        bottomMeldTileCount: 7,
        bottomMeldGroupCount: 2,
        viewerClosedCount: 13,
        viewerHasDrawn: true,
      }),
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
    const spy = createSpyTileAtlas();
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

  it("renders frameless non-dimming hand and clean turn cues across own turn, post-pon, opponent turn, inputLocked, and reducedEffects", () => {
    const spy = createSpyTileAtlas();
    const standingTextureSet = new Set(spy.standingTextures.values());
    const stage = new Container();
    const onLayoutChange = vi.fn();
    const renderer = new TableSceneRenderer(stage, spy.atlas, {
      onDiscard: () => {},
      onLayoutChange,
    });

    const feltLayer = stage.children[0] as Container;
    const viewerHandLayer = stage.children[7] as Container;

    const bannedFrameLabels = new Set([
      "turn-tray-idle",
      "turn-tray-active",
      "turn-tray-breath",
      "post-pon-tile-highlight",
      "drawn-tile-glow",
    ]);

    const expectFramelessAndFullBrightness = (expectedTileCount: number) => {
      const graphics = viewerHandLayer.children.filter(
        (c): c is Graphics => c instanceof Graphics,
      );
      expect(graphics.filter((g) => bannedFrameLabels.has(g.label))).toHaveLength(
        0,
      );
      // At rest (no tile selected), zero framing/marker Graphics are visible in viewerHandLayer.
      expect(graphics.filter((g) => g.visible)).toHaveLength(0);

      const tileSprites = viewerHandLayer.children.filter(
        (c) => "texture" in c && standingTextureSet.has(c.texture as Texture),
      );
      expect(tileSprites).toHaveLength(expectedTileCount);
      for (const sprite of tileSprites) {
        expect(sprite.tint).toBe(0xffffff);
        expect(sprite.alpha).toBe(1);
      }
      return tileSprites;
    };

    const hasStageWash = () =>
      feltLayer.children.some(
        (c) => c instanceof Graphics && c.label === "turn-stage-wash",
      );

    // 1. discard_drawn (desktop): drawn gap + 5px lift + ▼ caret + stage wash, zero hand frames, safe tick
    renderer.resize(1280, 720);
    renderer.render({
      snapshot: makeSnapshot(),
      pendingAddedKong: null,
      inputLocked: false,
      reducedEffects: false,
    });

    expect(onLayoutChange).toHaveBeenCalled();
    const latestLayout =
      onLayoutChange.mock.calls[onLayoutChange.mock.calls.length - 1][0];
    expect(latestLayout.turnBarRect.width).toBeGreaterThan(0);
    expect(hasStageWash()).toBe(true);
    expect(collectTexts(viewerHandLayer)).toContain("▼");

    const desktopSprites = expectFramelessAndFullBrightness(14);
    const firstClosedSprite = desktopSprites[0];
    const secondClosedSprite = desktopSprites[1];
    const lastClosedSprite = desktopSprites[12];
    const drawnTileSprite = desktopSprites[13];
    expect(firstClosedSprite.y - drawnTileSprite.y).toBe(5);
    const closedPitch = secondClosedSprite.x - firstClosedSprite.x;
    expect(drawnTileSprite.x - lastClosedSprite.x).toBeGreaterThan(closedPitch);

    renderer.tick(425);
    expectFramelessAndFullBrightness(14);

    // 2. discard_drawn (short landscape): drawn lift is 3px + ▼ caret, zero hand frames
    renderer.resize(844, 390);
    renderer.render({
      snapshot: makeSnapshot(),
      pendingAddedKong: null,
      inputLocked: false,
      reducedEffects: false,
    });
    expect(renderer.getLayout()!.isShortLandscape).toBe(true);
    expect(hasStageWash()).toBe(true);
    expect(collectTexts(viewerHandLayer)).toContain("▼");
    const shortSprites = expectFramelessAndFullBrightness(14);
    expect(shortSprites[0].y - shortSprites[13].y).toBe(3);

    // 3. reducedEffects: static cues, zero hand frames, tick remains a safe no-op
    renderer.resize(1280, 720);
    renderer.render({
      snapshot: makeSnapshot(),
      pendingAddedKong: null,
      inputLocked: false,
      reducedEffects: true,
    });
    expect(hasStageWash()).toBe(true);
    expect(collectTexts(viewerHandLayer)).toContain("▼");
    renderer.tick(425);
    expectFramelessAndFullBrightness(14);

    // 4. discard_post_pon (drawn_tile: null): no ▼, no per-tile gold boxes, keeps stage wash
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
    expect(hasStageWash()).toBe(true);
    expect(collectTexts(viewerHandLayer)).not.toContain("▼");
    expectFramelessAndFullBrightness(10);

    // 5. idle (waiting on opponent): no ▼, no stage wash, zero tray/marker frames, 100% brightness
    const idleSnap = makeSnapshot();
    idleSnap.player_view!.turn.active_seat = "South";
    idleSnap.player_view!.legal_actions = [];
    renderer.render({
      snapshot: idleSnap,
      pendingAddedKong: null,
      inputLocked: false,
      reducedEffects: false,
    });
    expect(hasStageWash()).toBe(false);
    expect(collectTexts(viewerHandLayer)).not.toContain("▼");
    expectFramelessAndFullBrightness(14);

    // 6. inputLocked (e.g., during animation lock on own turn): no ▼, no stage wash, 100% brightness
    renderer.render({
      snapshot: makeSnapshot(),
      pendingAddedKong: null,
      inputLocked: true,
      reducedEffects: false,
    });
    expect(hasStageWash()).toBe(false);
    expect(collectTexts(viewerHandLayer)).not.toContain("▼");
    expectFramelessAndFullBrightness(14);

    renderer.destroy();
    spy.atlas.destroy();
  });

  it("dynamically recomputes layout from viewer melds/hand count, notifies onLayoutChange, and keeps ghost slot aligned", () => {
    const spy = createSpyTileAtlas();
    const stage = new Container();
    const onLayoutChange = vi.fn();
    const renderer = new TableSceneRenderer(stage, spy.atlas, {
      onDiscard: () => {},
      onLayoutChange,
    });

    renderer.resize(1512, 982);
    onLayoutChange.mockClear();

    // 0 melds, 13+1 hand
    const snap0 = makeSnapshot();
    snap0.player_view!.seats[0].melds = [];
    snap0.player_view!.hand.closed_tiles = [0, 1, 2, 3, 4, 9, 10, 11, 18, 19, 20, 27, 28];
    snap0.player_view!.hand.drawn_tile = 5;
    renderer.render({
      snapshot: snap0,
      pendingAddedKong: null,
      inputLocked: false,
    });
    expect(onLayoutChange).toHaveBeenCalledTimes(0); // Matches initial 0-meld resize layout
    const layout0 = renderer.getLayout()!;
    expect(layout0.handTileW).toBe(74);
    expect(layout0.viewerHandRect).toEqual(
      computeTableLayout(1512, 982, {
        bottomMeldTileCount: 0,
        bottomMeldGroupCount: 0,
        viewerClosedCount: 13,
        viewerHasDrawn: true,
      }).viewerHandRect,
    );

    // 2 melds (Pon 3 + ConcealedKong 4 = 7 tiles), 7+1 hand at SAME viewport size
    const snap2 = makeSnapshot();
    snap2.player_view!.hand.closed_tiles = [0, 1, 2, 9, 10, 11, 18];
    snap2.player_view!.hand.drawn_tile = 5;
    renderer.render({
      snapshot: snap2,
      pendingAddedKong: null,
      inputLocked: false,
    });
    expect(onLayoutChange).toHaveBeenCalledTimes(1);
    const layout2 = renderer.getLayout()!;
    expect(layout2).toBe(onLayoutChange.mock.calls[0][0]);
    expect(layout2.viewerHandRect).toEqual(
      computeTableLayout(1512, 982, {
        bottomMeldTileCount: 7,
        bottomMeldGroupCount: 2,
        viewerClosedCount: 7,
        viewerHasDrawn: true,
      }).viewerHandRect,
    );
    expect(layout2.viewerHandRect.width).toBeLessThan(layout0.viewerHandRect.width);

    // Re-rendering identical state does not fire redundant onLayoutChange
    renderer.render({
      snapshot: snap2,
      pendingAddedKong: null,
      inputLocked: false,
    });
    expect(onLayoutChange).toHaveBeenCalledTimes(1);

    // 4 kongs (16 tiles), 1+1 hand at SAME viewport size
    const snap4 = makeSnapshot();
    snap4.player_view!.seats[0].melds = [
      { ExposedKong: { tile: 27, from_seat: "South", seq: 20 } },
      { ConcealedKong: { tile: 31, seq: 21 } },
      { ExposedKong: { tile: 28, from_seat: "West", seq: 22 } },
      { AddedKong: { tile: 29, from_seat: "North", seq: 23 } },
    ];
    snap4.player_view!.hand.closed_tiles = [0];
    snap4.player_view!.hand.drawn_tile = 5;
    renderer.render({
      snapshot: snap4,
      pendingAddedKong: null,
      inputLocked: false,
    });
    expect(onLayoutChange).toHaveBeenCalledTimes(2);
    const layout4 = renderer.getLayout()!;
    expect(layout4).toBe(onLayoutChange.mock.calls[1][0]);
    expect(layout4.viewerHandRect).toEqual(
      computeTableLayout(1512, 982, {
        bottomMeldTileCount: 16,
        bottomMeldGroupCount: 4,
        viewerClosedCount: 1,
        viewerHasDrawn: true,
      }).viewerHandRect,
    );

    // Discard ghost landing remains aligned to bottom 10-per-row river slot #12
    const expectedGhost = computeBottomRiverSlotPoint(layout4, 12);
    expect(expectedGhost).toEqual(computeBottomRiverSlotPoint(layout0, 12));
    const viewerHandLayer = stage.children[7] as Container;
    const drawnSprite = viewerHandLayer.children.find(
      (c) => "texture" in c && c.texture === spy.standingTextures.get(5),
    );
    expect(drawnSprite).toBeDefined();
    drawnSprite!.emit("pointerover", makePointerEvent());
    const ghostSprite = renderer.ghostLayer.children[renderer.ghostLayer.children.length - 1];
    expect(ghostSprite.x).toBeCloseTo(expectedGhost.x, 1);
    expect(ghostSprite.y).toBeCloseTo(expectedGhost.y, 1);

    renderer.destroy();
    spy.atlas.destroy();
  });
});
