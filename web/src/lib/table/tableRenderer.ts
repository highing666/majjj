import {
  Container,
  type FederatedPointerEvent,
  Graphics,
  Rectangle,
  Sprite,
  Text,
} from "pixi.js";
import type { PublicSeatView } from "../protocol/generated/PublicSeatView";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import type { Tile } from "../protocol/generated/Tile";
import type { PendingAddedKong } from "../session/reducer";
import {
  createDiscardInteractionController,
  type DiscardInteractionController,
  type DiscardInteractionState,
  type HandTileSlot,
} from "./discardInteraction";
import {
  BRASS,
  disposeContainerChildren,
  renderFeltBackdrop,
} from "./feltBackdrop";
import {
  computeBottomRiverSlotPoint,
  computeCenterSealLayout,
  computeOpponentHandSlots,
  computeRiverSlotPoint,
  computeTableLayout,
  computeViewerHandSlots,
  type Rect,
  type TableLayout,
} from "./layout";
import {
  createPaperGrainFilter,
  type PaperGrainFilter,
} from "./paperGrainFilter";
import type { TileAtlas } from "./tileAtlas";
import { seatShortWind } from "./tiles";
import {
  buildMeldRenderItems,
  buildSeatBadgeModel,
  buildVisibleRiverRows,
  findLegalDiscardAction,
  getRelativeSeats,
} from "./viewModel";

const SEAL_BG = 0x0d1b20;
const TEXT_LIGHT = 0xf5eedf;
const TEXT_MUTED = 0xcfdad9;
const ACTIVE_GLOW = 0xf5be58;
const PENDING_AMBER = 0xf09d38;
const DEALER_CORAL = 0xd8513c;
const INK_DARK = 0x181a20;

const UI_FONT =
  '-apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Noto Sans CJK SC", sans-serif';
const SERIF_FONT =
  '"Songti SC", "STSong", "Noto Serif CJK SC", "PingFang SC", serif';

export interface TableRenderParams {
  snapshot: TableSnapshot;
  pendingAddedKong: PendingAddedKong | null;
  inputLocked: boolean;
  confirmDiscard?: boolean;
  paperGrainEnabled?: boolean;
}

export interface TableRendererCallbacks {
  onDiscard: (tile: Tile, fromDrawn: boolean) => void;
  onTileHover?: (tile: Tile, fromDrawn: boolean) => void;
  onTileSelect?: (tile: Tile, fromDrawn: boolean) => void;
  onTileUnhover?: () => void;
}

interface ShadowedTilePair {
  sprite: Sprite;
  shadow: Sprite;
}

interface ViewerHandVisual {
  slot: HandTileSlot;
  sprite: Sprite;
  shadow: Sprite;
  selectRing: Graphics;
  restX: number;
  restY: number;
  width: number;
  height: number;
}

function addShadowedTilePair(
  layer: Container,
  atlas: TileAtlas,
  tile: Tile | null,
  x: number,
  y: number,
  width: number,
  height: number,
  rotation = 0,
  shadowOffsetY = 2,
): ShadowedTilePair {
  const shadow = new Sprite(atlas.shadowTexture);
  shadow.anchor.set(0.5);
  shadow.width = width;
  shadow.height = height;
  shadow.rotation = rotation;
  shadow.position.set(x, y + shadowOffsetY);
  shadow.alpha = 0.65;
  layer.addChild(shadow);

  const texture = tile === null ? atlas.backTexture : atlas.getTileTexture(tile);
  const sprite = new Sprite(texture);
  sprite.anchor.set(0.5);
  sprite.width = width;
  sprite.height = height;
  sprite.rotation = rotation;
  sprite.position.set(x, y);
  layer.addChild(sprite);
  return { sprite, shadow };
}

function addShadowedTileSprite(
  layer: Container,
  atlas: TileAtlas,
  tile: Tile | null,
  x: number,
  y: number,
  width: number,
  height: number,
  rotation = 0,
  shadowOffsetY = 2,
): Sprite {
  return addShadowedTilePair(
    layer,
    atlas,
    tile,
    x,
    y,
    width,
    height,
    rotation,
    shadowOffsetY,
  ).sprite;
}

function findLatestRiverSeq(seats: readonly PublicSeatView[]): number {
  let maxSeq = 0;
  for (const seat of seats) {
    for (const entry of seat.river) {
      if (entry.claimed_by === null && entry.seq > maxSeq) {
        maxSeq = entry.seq;
      }
    }
  }
  return maxSeq;
}

function fitTextToWidth(label: Text, maxWidth: number): void {
  if (maxWidth > 0 && label.width > maxWidth && label.width > 0) {
    label.scale.set(maxWidth / label.width);
  }
}

/**
 * Manages the PixiJS v8 scene layers for the Piltover Stage (Scheme A) + Four-Corner Minimal (Scheme C) table.
 * Reserves dedicated `ghostLayer` (#42) and `effectsLayer` (#43) containers.
 */
export class TableSceneRenderer {
  private readonly root: Container;
  private readonly feltLayer = new Container();
  private readonly centerSealLayer = new Container();
  private readonly riverLayer = new Container();
  readonly ghostLayer = new Container();
  private readonly meldLayer = new Container();
  private readonly opponentHandLayer = new Container();
  private readonly badgeLayer = new Container();
  private readonly viewerHandLayer = new Container();
  readonly effectsLayer = new Container();
  private readonly atlas: TileAtlas;
  private readonly callbacks: TableRendererCallbacks;

  private width = 800;
  private height = 600;
  private lastParams: TableRenderParams | null = null;
  private lastLayout: TableLayout | null = null;
  private bottomVisibleRiverCount = 0;
  private viewerHandVisuals: ViewerHandVisual[] = [];
  private readonly lastDragOffsets = new Map<string, { dx: number; dy: number }>();
  private snapBackRaf: number | null = null;
  private suppressInteractionApply = false;
  private readonly interaction: DiscardInteractionController;
  private readonly paperGrainFilter: PaperGrainFilter | null;

  private readonly onStagePointerDown = () => {
    this.interaction.tapBlankArea();
  };

  private readonly onStagePointerMove = (e: FederatedPointerEvent) => {
    this.interaction.pointerMove(e.global.x, e.global.y, performance.now());
  };

  private readonly onStagePointerUp = (e: FederatedPointerEvent) => {
    this.interaction.pointerUp(e.global.x, e.global.y, performance.now());
  };

  private readonly onStagePointerCancel = () => {
    this.interaction.pointerCancel();
  };

  constructor(
    stage: Container,
    atlas: TileAtlas,
    callbacks: TableRendererCallbacks,
  ) {
    this.root = stage;
    this.atlas = atlas;
    this.callbacks = callbacks;

    this.interaction = createDiscardInteractionController({
      confirmDiscard: false,
      onCommitDiscard: (tile, fromDrawn) => {
        this.callbacks.onDiscard(tile, fromDrawn);
      },
      onTileHover: (tile, fromDrawn) => {
        this.callbacks.onTileHover?.(tile, fromDrawn);
      },
      onTileSelect: (tile, fromDrawn) => {
        this.callbacks.onTileSelect?.(tile, fromDrawn);
      },
      onStateChange: (state) => {
        if (!this.suppressInteractionApply) {
          this.applyInteractionState(state);
        }
      },
    });

    this.root.eventMode = "static";
    this.root.hitArea = new Rectangle(0, 0, this.width, this.height);
    this.root.filterArea = new Rectangle(0, 0, this.width, this.height);
    this.paperGrainFilter =
      typeof document !== "undefined" ? createPaperGrainFilter() : null;
    if (this.paperGrainFilter) {
      this.root.filters = [this.paperGrainFilter];
    }
    this.root.on("pointerdown", this.onStagePointerDown);
    this.root.on("globalpointermove", this.onStagePointerMove);
    this.root.on("pointerup", this.onStagePointerUp);
    this.root.on("pointerupoutside", this.onStagePointerUp);
    this.root.on("pointercancel", this.onStagePointerCancel);

    this.root.addChild(
      this.feltLayer,
      this.centerSealLayer,
      this.riverLayer,
      this.ghostLayer,
      this.meldLayer,
      this.opponentHandLayer,
      this.badgeLayer,
      this.viewerHandLayer,
      this.effectsLayer,
    );
  }

  getLayout(): TableLayout | null {
    return this.lastLayout;
  }

  setPaperGrainEnabled(enabled: boolean): void {
    if (this.paperGrainFilter) {
      this.paperGrainFilter.enabled = enabled;
    }
  }

  resize(width: number, height: number): void {
    this.width = Math.max(320, Math.round(width));
    this.height = Math.max(280, Math.round(height));
    const bounds = new Rectangle(0, 0, this.width, this.height);
    this.root.hitArea = bounds;
    this.root.filterArea = bounds;
    if (this.lastParams !== null) {
      this.render(this.lastParams);
    }
  }

  render(params: TableRenderParams): void {
    this.lastParams = params;
    if (params.paperGrainEnabled !== undefined) {
      this.setPaperGrainEnabled(params.paperGrainEnabled);
    }
    const view = params.snapshot.player_view;
    if (!view) {
      this.clearAll(true);
      return;
    }

    const layout = computeTableLayout(this.width, this.height);
    this.lastLayout = layout;
    const relative = getRelativeSeats(view.viewer);
    const seatByName = new Map<Seat, PublicSeatView>();
    for (const s of view.seats) {
      seatByName.set(s.seat, s);
    }
    const infoByName = new Map<Seat, SeatInfo>();
    for (const info of params.snapshot.seats) {
      infoByName.set(info.seat, info);
    }

    const bottomRiver = seatByName.get(relative.bottom)?.river ?? [];
    this.bottomVisibleRiverCount = bottomRiver.filter(
      (entry) => entry.claimed_by === null,
    ).length;

    renderFeltBackdrop(this.feltLayer, layout, this.atlas);
    this.drawCenterSeal(layout, view.turn, view.wall_remaining);
    this.drawRivers(layout, relative, seatByName, findLatestRiverSeq(view.seats));
    this.drawMelds(layout, relative, seatByName, params.pendingAddedKong);
    this.drawOpponentHands(layout, relative, seatByName);
    this.drawCornerBadges(
      layout,
      relative,
      seatByName,
      infoByName,
      view.viewer,
      view.turn.dealer,
      view.turn.active_seat,
    );
    const legalSlotIds = this.drawViewerHand(layout, params);

    this.suppressInteractionApply = true;
    try {
      this.interaction.setConfirmDiscard(params.confirmDiscard ?? false);
      this.interaction.setInputLocked(params.inputLocked);
      this.interaction.syncContext({
        canDiscard: legalSlotIds.size > 0 && !params.inputLocked,
        legalSlotIds,
      });
    } finally {
      this.suppressInteractionApply = false;
    }

    this.applyInteractionState(this.interaction.getState());
  }

  destroy(): void {
    this.cancelSnapBackAnimation();
    this.root.off("pointerdown", this.onStagePointerDown);
    this.root.off("globalpointermove", this.onStagePointerMove);
    this.root.off("pointerup", this.onStagePointerUp);
    this.root.off("pointerupoutside", this.onStagePointerUp);
    this.root.off("pointercancel", this.onStagePointerCancel);
    this.root.filters = [];
    this.root.filterArea = undefined;
    this.paperGrainFilter?.destroy();
    this.clearAll(true);
    this.root.removeChildren();
  }

  private clearAll(includeAuxiliaryLayers: boolean): void {
    this.cancelSnapBackAnimation();
    this.viewerHandVisuals = [];
    disposeContainerChildren(this.feltLayer);
    disposeContainerChildren(this.centerSealLayer);
    disposeContainerChildren(this.riverLayer);
    disposeContainerChildren(this.meldLayer);
    disposeContainerChildren(this.opponentHandLayer);
    disposeContainerChildren(this.badgeLayer);
    disposeContainerChildren(this.viewerHandLayer);
    if (includeAuxiliaryLayers) {
      disposeContainerChildren(this.ghostLayer);
      disposeContainerChildren(this.effectsLayer);
    }
  }

  private drawCenterSeal(
    layout: TableLayout,
    turn: NonNullable<TableSnapshot["player_view"]>["turn"],
    wallRemaining: number,
  ): void {
    disposeContainerChildren(this.centerSealLayer);
    const { centerSeal } = layout;
    const sealLayout = computeCenterSealLayout(centerSeal);
    const cx = centerSeal.x + centerSeal.width / 2;
    const cy = centerSeal.y + centerSeal.height / 2;
    const radius = centerSeal.width / 2;

    const ring = new Graphics();
    ring
      .circle(cx, cy, radius)
      .fill({ color: SEAL_BG, alpha: 0.88 })
      .stroke({ width: 2, color: BRASS, alpha: 0.78 });
    ring
      .circle(cx, cy, Math.max(4, radius - 4))
      .stroke({ width: 1, color: BRASS, alpha: 0.28 });
    this.centerSealLayer.addChild(ring);

    const activeSeat = turn.active_seat ?? turn.dealer;
    const windText = new Text({
      text: seatShortWind(activeSeat),
      style: {
        fontFamily: SERIF_FONT,
        fontSize: sealLayout.windFontSize,
        fontWeight: "900",
        fill: ACTIVE_GLOW,
      },
    });
    windText.anchor.set(0.5);
    windText.position.set(cx, sealLayout.windY);
    this.centerSealLayer.addChild(windText);

    const wallText = new Text({
      text: `余 ${wallRemaining}`,
      style: {
        fontFamily: UI_FONT,
        fontSize: sealLayout.wallFontSize,
        fontWeight: "700",
        fill: TEXT_MUTED,
      },
    });
    wallText.anchor.set(0.5);
    wallText.position.set(cx, sealLayout.wallY);
    fitTextToWidth(wallText, centerSeal.width - 10);
    this.centerSealLayer.addChild(wallText);
  }

  private drawRivers(
    layout: TableLayout,
    relative: ReturnType<typeof getRelativeSeats>,
    seatByName: Map<Seat, PublicSeatView>,
    latestSeq: number,
  ): void {
    disposeContainerChildren(this.riverLayer);
    const { riverTileW: tw, riverTileH: th } = layout;

    const placeRiverTile = (
      tile: Tile,
      x: number,
      y: number,
      rotation: number,
      isTsumogiri: boolean,
      isLatest: boolean,
    ) => {
      const sprite = addShadowedTileSprite(
        this.riverLayer,
        this.atlas,
        tile,
        x,
        y,
        tw,
        th,
        rotation,
        1.5,
      );
      const isHorizontal = rotation === 0 || Math.abs(rotation - Math.PI) < 0.01;
      const rw = isHorizontal ? tw : th;
      const rh = isHorizontal ? th : tw;

      if (isTsumogiri) {
        sprite.alpha = 0.84;
        sprite.tint = 0xd2dada;

        const dot = new Graphics();
        dot
          .circle(x + rw * 0.28, y - rh * 0.28, Math.max(1.5, tw * 0.08))
          .fill({ color: INK_DARK, alpha: 0.42 });
        this.riverLayer.addChild(dot);
      }

      if (isLatest) {
        const ring = new Graphics();
        ring
          .roundRect(x - rw / 2 - 1.5, y - rh / 2 - 1.5, rw + 3, rh + 3, 4)
          .stroke({ width: 2, color: ACTIVE_GLOW });
        this.riverLayer.addChild(ring);
      }
    };

    for (const side of ["bottom", "top", "left", "right"] as const) {
      const rows = buildVisibleRiverRows(
        seatByName.get(relative[side])?.river ?? [],
        6,
      );
      rows.forEach((row, rIdx) => {
        row.forEach((entry, cIdx) => {
          const slot = computeRiverSlotPoint(layout, side, rIdx * 6 + cIdx);
          placeRiverTile(
            entry.tile,
            slot.x,
            slot.y,
            slot.rotation,
            entry.is_tsumogiri,
            entry.seq === latestSeq,
          );
        });
      });
    }
  }

  private drawMelds(
    layout: TableLayout,
    relative: ReturnType<typeof getRelativeSeats>,
    seatByName: Map<Seat, PublicSeatView>,
    pendingAddedKong: PendingAddedKong | null,
  ): void {
    disposeContainerChildren(this.meldLayer);
    const { meldTileW: mw, meldTileH: mh } = layout;
    const tileGap = 1;
    const groupGap = 8;

    interface MeldGroupPlacement {
      seat: Seat;
      axis: "horizontal" | "vertical";
      edgeAnchor: number;
      fixedCoord: number;
      growLeft?: boolean;
      rotation: number;
    }

    const drawMeldSequence = (placement: MeldGroupPlacement) => {
      const {
        seat,
        axis,
        edgeAnchor,
        fixedCoord,
        growLeft = false,
        rotation,
      } = placement;
      const seatView = seatByName.get(seat);
      if (!seatView) {
        return;
      }
      const items = buildMeldRenderItems(
        seatView.melds,
        pendingAddedKong,
        seat,
      );
      let cursor = edgeAnchor;
      for (const item of items) {
        const groupLen =
          item.tiles.length * mw + (item.tiles.length - 1) * tileGap;
        const start =
          axis === "horizontal" && growLeft ? cursor - groupLen : cursor;

        item.tiles.forEach((slot, idx) => {
          const along = start + idx * (mw + tileGap) + mw / 2;
          const cx = axis === "horizontal" ? along : fixedCoord;
          const cy = axis === "horizontal" ? fixedCoord + mh / 2 : along;
          addShadowedTileSprite(
            this.meldLayer,
            this.atlas,
            slot.faceUp ? slot.tile : null,
            cx,
            cy,
            mw,
            mh,
            rotation,
            1.5,
          );
        });

        if (item.pendingAddedKong && pendingAddedKong !== null) {
          const secondAlong = start + (mw + tileGap) + mw / 2;
          const hoverX =
            axis === "horizontal"
              ? secondAlong
              : fixedCoord + (rotation > 0 ? mh * 0.32 : -mh * 0.32);
          const hoverY =
            axis === "horizontal" ? fixedCoord - mh * 0.28 : secondAlong;
          addShadowedTileSprite(
            this.meldLayer,
            this.atlas,
            pendingAddedKong.tile,
            hoverX,
            hoverY,
            mw,
            mh,
            rotation,
            1.5,
          );

          const boxW = axis === "horizontal" ? mw : mh;
          const boxH = axis === "horizontal" ? mh : mw;
          const outline = new Graphics();
          outline
            .roundRect(
              hoverX - boxW / 2 - 2,
              hoverY - boxH / 2 - 2,
              boxW + 4,
              boxH + 4,
              4,
            )
            .stroke({ width: 2, color: PENDING_AMBER });
          this.meldLayer.addChild(outline);
        }

        cursor =
          axis === "horizontal" && growLeft
            ? start - groupGap
            : start + groupLen + groupGap;
      }
    };

    const placements: MeldGroupPlacement[] = [
      {
        seat: relative.bottom,
        axis: "horizontal",
        edgeAnchor: layout.melds.bottom.x,
        fixedCoord: layout.melds.bottom.y,
        growLeft: true,
        rotation: 0,
      },
      {
        seat: relative.top,
        axis: "horizontal",
        edgeAnchor: layout.melds.top.x,
        fixedCoord: layout.melds.top.y,
        growLeft: true,
        rotation: Math.PI,
      },
      {
        seat: relative.left,
        axis: "vertical",
        edgeAnchor: layout.melds.left.y,
        fixedCoord: layout.melds.left.x + mh / 2,
        rotation: Math.PI / 2,
      },
      {
        seat: relative.right,
        axis: "vertical",
        edgeAnchor: layout.melds.right.y,
        fixedCoord: layout.melds.right.x - mh / 2,
        rotation: -Math.PI / 2,
      },
    ];
    for (const placement of placements) {
      drawMeldSequence(placement);
    }
  }

  private drawOpponentHands(
    layout: TableLayout,
    relative: ReturnType<typeof getRelativeSeats>,
    seatByName: Map<Seat, PublicSeatView>,
  ): void {
    disposeContainerChildren(this.opponentHandLayer);
    for (const side of ["top", "left", "right"] as const) {
      const count = seatByName.get(relative[side])?.hand_count ?? 0;
      for (const slot of computeOpponentHandSlots(layout, side, count)) {
        addShadowedTileSprite(
          this.opponentHandLayer,
          this.atlas,
          null,
          slot.x,
          slot.y,
          slot.width,
          slot.height,
          slot.rotation,
          1.5,
        );
      }
    }
  }

  private drawCornerBadges(
    layout: TableLayout,
    relative: ReturnType<typeof getRelativeSeats>,
    seatByName: Map<Seat, PublicSeatView>,
    infoByName: Map<Seat, SeatInfo>,
    viewer: Seat,
    dealer: Seat,
    activeSeat: Seat | null,
  ): void {
    disposeContainerChildren(this.badgeLayer);

    const drawBadge = (seat: Seat, rect: Rect) => {
      const seatView = seatByName.get(seat);
      const seatInfo = infoByName.get(seat);
      if (!seatView || !seatInfo) {
        return;
      }

      const model = buildSeatBadgeModel(
        seat,
        seatView,
        seatInfo,
        viewer,
        dealer,
        activeSeat,
      );
      const padX = layout.isWide ? 7 : 5;
      const box = new Graphics();
      box
        .roundRect(rect.x, rect.y, rect.width, rect.height, 8)
        .fill({
          color: model.isActive ? 0x1b3b44 : SEAL_BG,
          alpha: 0.9,
        })
        .stroke({
          width: model.isActive ? 2 : 1.25,
          color: model.isActive ? ACTIVE_GLOW : 0x8a6d3b,
          alpha: model.isActive ? 1 : 0.82,
        });
      this.badgeLayer.addChild(box);

      const pillSize = rect.height - 8;
      const pill = new Graphics();
      pill
        .roundRect(rect.x + padX, rect.y + 4, pillSize, pillSize, 5)
        .fill({ color: model.isDealer ? DEALER_CORAL : 0x264852 });
      this.badgeLayer.addChild(pill);

      const windLabel = new Text({
        text: model.isDealer ? `${model.wind}庄` : model.wind,
        style: {
          fontFamily: UI_FONT,
          fontSize: model.isDealer
            ? layout.isWide
              ? 10
              : 8
            : layout.isWide
              ? 11
              : 9,
          fontWeight: "800",
          fill: TEXT_LIGHT,
        },
      });
      windLabel.anchor.set(0.5);
      windLabel.position.set(
        rect.x + padX + pillSize / 2,
        rect.y + rect.height / 2,
      );
      fitTextToWidth(windLabel, pillSize - 2);
      this.badgeLayer.addChild(windLabel);

      const scoreText = new Text({
        text: model.scoreText,
        style: {
          fontFamily: UI_FONT,
          fontSize: layout.isWide ? 12 : 10,
          fontWeight: "800",
          fill: model.isActive ? ACTIVE_GLOW : BRASS,
        },
      });
      scoreText.anchor.set(1, 0.5);
      scoreText.position.set(
        rect.x + rect.width - padX,
        rect.y + rect.height / 2,
      );
      this.badgeLayer.addChild(scoreText);

      const nameStart = rect.x + padX + pillSize + 6;
      const nameMaxW = Math.max(
        20,
        rect.x + rect.width - padX - scoreText.width - 6 - nameStart,
      );
      const nameLabel = new Text({
        text: model.isOffline ? `${model.name}·离线` : model.name,
        style: {
          fontFamily: UI_FONT,
          fontSize: layout.isWide ? 12 : 10,
          fontWeight: model.isActive ? "700" : "600",
          fill: model.isOffline ? PENDING_AMBER : TEXT_LIGHT,
        },
      });
      nameLabel.anchor.set(0, 0.5);
      nameLabel.position.set(nameStart, rect.y + rect.height / 2);
      fitTextToWidth(nameLabel, nameMaxW);
      this.badgeLayer.addChild(nameLabel);
    };

    drawBadge(relative.top, layout.badges.top);
    drawBadge(relative.right, layout.badges.right);
    drawBadge(relative.left, layout.badges.left);
    drawBadge(relative.bottom, layout.badges.bottom);
  }

  private drawViewerHand(
    layout: TableLayout,
    params: TableRenderParams,
  ): Set<string> {
    this.cancelSnapBackAnimation();
    this.viewerHandVisuals = [];
    disposeContainerChildren(this.viewerHandLayer);
    const legalSlotIds = new Set<string>();
    const view = params.snapshot.player_view;
    if (!view) {
      return legalSlotIds;
    }

    const { closed_tiles, drawn_tile } = view.hand;
    const hasDrawn = drawn_tile !== null;
    if (closed_tiles.length === 0 && !hasDrawn) {
      return legalSlotIds;
    }

    const handSlots = computeViewerHandSlots(
      layout,
      closed_tiles.length,
      hasDrawn,
    );

    const addInteractiveHandTile = (
      slotId: string,
      tile: Tile,
      fromDrawn: boolean,
      geom: (typeof handSlots.closed)[number],
    ) => {
      const legal =
        !params.inputLocked &&
        findLegalDiscardAction(view.legal_actions, tile, fromDrawn) !== null;
      const slot: HandTileSlot = { slotId, tile, fromDrawn, legal };
      if (legal) {
        legalSlotIds.add(slotId);
      }

      const { x, y, restX, restY, width: tw, height: th } = geom;
      const { sprite, shadow } = addShadowedTilePair(
        this.viewerHandLayer,
        this.atlas,
        tile,
        restX,
        restY,
        tw,
        th,
        0,
        3,
      );

      const selectRing = new Graphics();
      selectRing
        .roundRect(x - 2, y - 14, tw + 4, th + 4, 6)
        .stroke({ width: 2.2, color: ACTIVE_GLOW });
      selectRing.visible = false;
      this.viewerHandLayer.addChild(selectRing);

      this.viewerHandVisuals.push({
        slot,
        sprite,
        shadow,
        selectRing,
        restX,
        restY,
        width: tw,
        height: th,
      });

      if (legal) {
        sprite.eventMode = "static";
        sprite.cursor = "pointer";
        sprite.hitArea = new Rectangle(
          -tw / 2,
          -th / 2 - 12,
          tw,
          th + 24,
        );

        sprite.on("pointerover", () => {
          this.interaction.pointerEnter(slot);
        });
        sprite.on("pointerout", () => {
          this.interaction.pointerLeave(slot.slotId);
          this.callbacks.onTileUnhover?.();
        });
        sprite.on("pointerdown", (e: FederatedPointerEvent) => {
          e.stopPropagation();
          this.interaction.pointerDown(
            slot,
            e.global.x,
            e.global.y,
            performance.now(),
          );
        });
      } else {
        sprite.eventMode = "none";
        if (params.inputLocked) {
          sprite.alpha = 0.86;
        }
      }
    };

    closed_tiles.forEach((tile, idx) => {
      addInteractiveHandTile(
        `closed-${idx}`,
        tile,
        false,
        handSlots.closed[idx],
      );
    });

    if (drawn_tile !== null && handSlots.drawn !== null) {
      const { x: drawnX, y: baseY, width: tw, height: th } = handSlots.drawn;
      const marker = new Graphics();
      marker
        .roundRect(drawnX - 2, baseY - 2, tw + 4, th + 4, 6)
        .stroke({ width: 1.5, color: BRASS, alpha: 0.72 });
      this.viewerHandLayer.addChild(marker);

      addInteractiveHandTile("drawn", drawn_tile, true, handSlots.drawn);
    }

    return legalSlotIds;
  }

  private cancelSnapBackAnimation(): void {
    if (this.snapBackRaf !== null && typeof cancelAnimationFrame === "function") {
      cancelAnimationFrame(this.snapBackRaf);
    }
    this.snapBackRaf = null;
  }

  private startSnapBackAnimation(
    visual: ViewerHandVisual,
    fromDx: number,
    fromDy: number,
  ): void {
    this.cancelSnapBackAnimation();
    if (typeof requestAnimationFrame !== "function") {
      visual.sprite.position.set(visual.restX, visual.restY);
      return;
    }
    const durationMs = 120;
    const startMs = performance.now();
    const step = (nowMs: number) => {
      const t = Math.min(1, (nowMs - startMs) / durationMs);
      const ease = 1 - (1 - t) * (1 - t);
      visual.sprite.position.set(
        visual.restX + fromDx * (1 - ease),
        visual.restY + fromDy * (1 - ease),
      );
      if (t < 1) {
        this.snapBackRaf = requestAnimationFrame(step);
      } else {
        this.snapBackRaf = null;
        visual.sprite.position.set(visual.restX, visual.restY);
      }
    };
    this.snapBackRaf = requestAnimationFrame(step);
  }

  private applyInteractionState(state: DiscardInteractionState): void {
    if (state.drag !== null) {
      this.cancelSnapBackAnimation();
      this.lastDragOffsets.set(state.drag.slotId, {
        dx: state.drag.dx,
        dy: state.drag.dy,
      });
    }

    for (const v of this.viewerHandVisuals) {
      const id = v.slot.slotId;
      const isDragging = state.drag?.slotId === id;
      const isLifting = state.liftingSlotId === id;
      const isSelected = state.selectedSlotId === id;
      const isHovered = state.hoveredSlotId === id;

      v.sprite.width = v.width;
      v.sprite.height = v.height;
      v.sprite.rotation = 0;
      v.sprite.tint = 0xffffff;
      v.sprite.alpha = state.locked && !isLifting ? 0.86 : 1;
      v.shadow.position.set(v.restX, v.restY + 3);
      v.shadow.alpha = 0.65;
      v.selectRing.visible = false;

      if (isDragging && state.drag) {
        v.sprite.position.set(v.restX + state.drag.dx, v.restY + state.drag.dy);
        v.sprite.width = v.width * 1.06;
        v.sprite.height = v.height * 1.06;
        v.sprite.rotation = Math.max(-0.12, Math.min(0.12, state.drag.dx * 0.002));
        v.sprite.tint = state.drag.readyToCommit ? 0xffe7a3 : 0xfff3d4;
        v.shadow.alpha = 0.28;
        continue;
      }

      if (isLifting) {
        v.sprite.position.set(v.restX, v.restY - 18);
        v.sprite.tint = 0xfff3d4;
        v.sprite.alpha = 0.72;
        v.shadow.alpha = 0.3;
        continue;
      }

      if (isSelected || isHovered) {
        v.sprite.position.set(v.restX, v.restY - 12);
        v.sprite.tint = 0xfff3d4;
        v.shadow.alpha = 0.42;
        v.selectRing.visible = isSelected;
        continue;
      }

      if (state.snapBackSlotId === id) {
        const prev = this.lastDragOffsets.get(id);
        this.lastDragOffsets.delete(id);
        if (prev && (Math.abs(prev.dx) > 1 || Math.abs(prev.dy) > 1)) {
          this.startSnapBackAnimation(v, prev.dx, prev.dy);
          continue;
        }
      }

      v.sprite.position.set(v.restX, v.restY);
    }

    if (state.drag === null && state.snapBackSlotId === null) {
      this.lastDragOffsets.clear();
    }

    this.drawGhostPreview(state);
  }

  private drawGhostPreview(state: DiscardInteractionState): void {
    disposeContainerChildren(this.ghostLayer);
    const layout = this.lastLayout;
    if (!layout) {
      return;
    }

    if (state.drag !== null) {
      const guideY = layout.bottomHandY - 36;
      const guideHalfW = Math.min(180, layout.width * 0.3);
      const guide = new Graphics();
      const color = state.drag.readyToCommit ? ACTIVE_GLOW : BRASS;
      const alpha = state.drag.readyToCommit ? 0.92 : 0.48;
      guide
        .moveTo(layout.cx - guideHalfW, guideY + 12)
        .quadraticCurveTo(layout.cx, guideY - 16, layout.cx + guideHalfW, guideY + 12)
        .stroke({ width: state.drag.readyToCommit ? 3 : 2, color, alpha });
      this.ghostLayer.addChild(guide);
    }

    if (state.ghostTile === null) {
      return;
    }

    const pt = computeBottomRiverSlotPoint(layout, this.bottomVisibleRiverCount);
    const { riverTileW: tw, riverTileH: th } = layout;

    const frame = new Graphics();
    frame
      .roundRect(pt.x - tw / 2 - 2, pt.y - th / 2 - 2, tw + 4, th + 4, 4)
      .fill({ color: ACTIVE_GLOW, alpha: 0.14 })
      .stroke({ width: 1.8, color: ACTIVE_GLOW, alpha: 0.86 });
    this.ghostLayer.addChild(frame);

    const ghostSprite = new Sprite(this.atlas.getTileTexture(state.ghostTile));
    ghostSprite.anchor.set(0.5);
    ghostSprite.width = tw;
    ghostSprite.height = th;
    ghostSprite.position.set(pt.x, pt.y);
    ghostSprite.alpha = 0.56;
    ghostSprite.tint = 0xfff3d4;
    this.ghostLayer.addChild(ghostSprite);
  }
}
