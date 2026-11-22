import { Container, Graphics, Rectangle, Sprite, Text } from "pixi.js";
import type { PublicSeatView } from "../protocol/generated/PublicSeatView";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import type { Tile } from "../protocol/generated/Tile";
import type { PendingAddedKong } from "../session/reducer";
import { computeHubLayout, computeTableLayout, type TableLayout } from "./layout";
import type { TileAtlas } from "./tileAtlas";
import { formatCompactRoundTitle, seatShortWind } from "./tiles";
import {
  buildMeldRenderItems,
  buildVisibleRiverRows,
  findLegalDiscardAction,
  formatScoreDelta,
  getRelativeSeats,
  seatRelativeRole,
} from "./viewModel";

const FELT_BG = 0x14282e;
const FELT_INNER = 0x18333b;
const FELT_RIM = 0x2f5a64;
const BRASS = 0xd4a857;
const HUB_BG = 0x0f2026;
const TEXT_LIGHT = 0xf5eedf;
const TEXT_MUTED = 0xa9c0c4;
const ACTIVE_GLOW = 0xf5be58;
const PENDING_AMBER = 0xf09d38;

export interface TableRenderParams {
  snapshot: TableSnapshot;
  pendingAddedKong: PendingAddedKong | null;
  inputLocked: boolean;
}

export interface TableRendererCallbacks {
  onDiscard: (tile: Tile, fromDrawn: boolean) => void;
}

function disposeChildren(container: Container): void {
  const children = container.removeChildren();
  for (const child of children) {
    if (child instanceof Container && child.children.length > 0) {
      disposeChildren(child);
    }
    if (child instanceof Sprite) {
      child.destroy({ texture: false });
    } else if (child instanceof Text) {
      child.destroy();
    } else {
      child.destroy({ children: false });
    }
  }
}

function createTileSprite(
  atlas: TileAtlas,
  tile: Tile | null,
  width: number,
  height: number,
  rotation = 0,
): Sprite {
  const texture = tile === null ? atlas.backTexture : atlas.getTileTexture(tile);
  const sprite = new Sprite(texture);
  sprite.anchor.set(0.5);
  sprite.width = width;
  sprite.height = height;
  sprite.rotation = rotation;
  return sprite;
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
 * Manages the PixiJS v8 scene layers for the 2D skeleton mahjong table.
 */
export class TableSceneRenderer {
  private readonly root: Container;
  private readonly feltLayer = new Container();
  private readonly hubLayer = new Container();
  private readonly riverLayer = new Container();
  private readonly meldLayer = new Container();
  private readonly opponentHandLayer = new Container();
  private readonly badgeLayer = new Container();
  private readonly viewerHandLayer = new Container();
  private readonly atlas: TileAtlas;
  private readonly callbacks: TableRendererCallbacks;

  private width = 800;
  private height = 600;
  private lastParams: TableRenderParams | null = null;

  constructor(
    stage: Container,
    atlas: TileAtlas,
    callbacks: TableRendererCallbacks,
  ) {
    this.root = stage;
    this.atlas = atlas;
    this.callbacks = callbacks;

    this.root.addChild(
      this.feltLayer,
      this.hubLayer,
      this.riverLayer,
      this.meldLayer,
      this.opponentHandLayer,
      this.badgeLayer,
      this.viewerHandLayer,
    );
  }

  resize(width: number, height: number): void {
    this.width = Math.max(320, Math.round(width));
    this.height = Math.max(280, Math.round(height));
    if (this.lastParams !== null) {
      this.render(this.lastParams);
    }
  }

  render(params: TableRenderParams): void {
    this.lastParams = params;
    const view = params.snapshot.player_view;
    if (!view) {
      this.clearAll();
      return;
    }

    const layout = computeTableLayout(this.width, this.height);
    const relative = getRelativeSeats(view.viewer);
    const seatByName = new Map<Seat, PublicSeatView>();
    for (const s of view.seats) {
      seatByName.set(s.seat, s);
    }
    const infoByName = new Map<Seat, SeatInfo>();
    for (const info of params.snapshot.seats) {
      infoByName.set(info.seat, info);
    }

    this.drawFelt(layout);
    this.drawHub(layout, view.turn, view.wall_remaining, relative);
    this.drawRivers(layout, relative, seatByName, findLatestRiverSeq(view.seats));
    this.drawMelds(layout, relative, seatByName, params.pendingAddedKong);
    this.drawOpponentHands(layout, relative, seatByName);
    this.drawSeatBadges(
      layout,
      relative,
      seatByName,
      infoByName,
      view.viewer,
      view.turn.dealer,
      view.turn.active_seat,
    );
    this.drawViewerHand(layout, params);
  }

  destroy(): void {
    this.clearAll();
    this.root.removeChildren();
  }

  private clearAll(): void {
    disposeChildren(this.feltLayer);
    disposeChildren(this.hubLayer);
    disposeChildren(this.riverLayer);
    disposeChildren(this.meldLayer);
    disposeChildren(this.opponentHandLayer);
    disposeChildren(this.badgeLayer);
    disposeChildren(this.viewerHandLayer);
  }

  private drawFelt(layout: TableLayout): void {
    disposeChildren(this.feltLayer);
    const bg = new Graphics();
    bg.rect(0, 0, layout.width, layout.height).fill({ color: FELT_BG });

    const pad = 8;
    bg.roundRect(pad, pad, layout.width - pad * 2, layout.height - pad * 2, 18)
      .fill({ color: FELT_INNER })
      .stroke({ width: 1.5, color: FELT_RIM });

    // Subtle warm central mat highlight
    const matW = Math.min(layout.width - 40, layout.hub.width + layout.riverTileW * 16);
    const matH = Math.min(layout.height - 40, layout.hub.height + layout.riverTileH * 8);
    bg.roundRect(
      layout.cx - matW / 2,
      layout.cy - matH / 2,
      matW,
      matH,
      20,
    ).stroke({ width: 1, color: 0x264b54, alpha: 0.7 });

    this.feltLayer.addChild(bg);
  }

  private drawHub(
    layout: TableLayout,
    turn: NonNullable<TableSnapshot["player_view"]>["turn"],
    wallRemaining: number,
    relative: ReturnType<typeof getRelativeSeats>,
  ): void {
    disposeChildren(this.hubLayer);
    const { hub } = layout;
    const hubLayout = computeHubLayout(hub);
    const centerX = hub.x + hub.width / 2;

    const card = new Graphics();
    card
      .roundRect(hub.x, hub.y, hub.width, hub.height, 12)
      .fill({ color: HUB_BG })
      .stroke({ width: 1.5, color: BRASS, alpha: 0.85 });
    this.hubLayer.addChild(card);

    if (hubLayout.showRoundLine) {
      const roundText = new Text({
        text: formatCompactRoundTitle(turn),
        style: {
          fontFamily:
            '-apple-system, BlinkMacSystemFont, "PingFang SC", "Noto Sans CJK SC", sans-serif',
          fontSize: hubLayout.roundFontSize,
          fontWeight: "700",
          fill: BRASS,
        },
      });
      roundText.anchor.set(0.5);
      roundText.position.set(centerX, hubLayout.roundY);
      fitTextToWidth(roundText, hubLayout.inner.width);
      this.hubLayer.addChild(roundText);
    }

    const wallText = new Text({
      text: `余 ${wallRemaining}`,
      style: {
        fontFamily:
          '-apple-system, BlinkMacSystemFont, "PingFang SC", "Noto Sans CJK SC", sans-serif',
        fontSize: hubLayout.wallFontSize,
        fontWeight: "700",
        fill: TEXT_LIGHT,
      },
    });
    wallText.anchor.set(0.5);
    wallText.position.set(centerX, hubLayout.wallY);
    fitTextToWidth(wallText, hubLayout.inner.width);
    this.hubLayer.addChild(wallText);

    // Directional wind ticks around hub edges
    const edgeDirs: Array<{ seat: Seat; x: number; y: number }> = [
      { seat: relative.bottom, ...hubLayout.winds.bottom },
      { seat: relative.top, ...hubLayout.winds.top },
      { seat: relative.left, ...hubLayout.winds.left },
      { seat: relative.right, ...hubLayout.winds.right },
    ];
    for (const dir of edgeDirs) {
      const isActive = turn.active_seat === dir.seat;
      const label = new Text({
        text: seatShortWind(dir.seat),
        style: {
          fontFamily:
            '-apple-system, BlinkMacSystemFont, "PingFang SC", "Noto Sans CJK SC", sans-serif',
          fontSize: hubLayout.windFontSize,
          fontWeight: isActive ? "800" : "600",
          fill: isActive ? ACTIVE_GLOW : TEXT_MUTED,
        },
      });
      label.anchor.set(0.5);
      label.position.set(dir.x, dir.y);
      this.hubLayer.addChild(label);
    }
  }

  private drawRivers(
    layout: TableLayout,
    relative: ReturnType<typeof getRelativeSeats>,
    seatByName: Map<Seat, PublicSeatView>,
    latestSeq: number,
  ): void {
    disposeChildren(this.riverLayer);
    const { riverTileW: tw, riverTileH: th, riverGap: gap } = layout;

    const placeTile = (
      tile: Tile,
      x: number,
      y: number,
      rotation: number,
      isTsumogiri: boolean,
      isLatest: boolean,
    ) => {
      const sprite = createTileSprite(this.atlas, tile, tw, th, rotation);
      sprite.position.set(x, y);
      // Subtly dim 摸切 (`is_tsumogiri: true`) vs 手切 (`is_tsumogiri: false`)
      sprite.alpha = isTsumogiri ? 0.72 : 1;
      sprite.tint = isTsumogiri ? 0xd8e2e2 : 0xffffff;
      this.riverLayer.addChild(sprite);

      if (isLatest) {
        const ring = new Graphics();
        const rw = rotation === 0 || Math.abs(rotation - Math.PI) < 0.01 ? tw : th;
        const rh = rotation === 0 || Math.abs(rotation - Math.PI) < 0.01 ? th : tw;
        ring
          .roundRect(x - rw / 2 - 1, y - rh / 2 - 1, rw + 2, rh + 2, 3)
          .stroke({ width: 2, color: ACTIVE_GLOW });
        this.riverLayer.addChild(ring);
      }
    };

    // Bottom river: row 0 closest to hub, growing downward, left-to-right
    const bottomRows = buildVisibleRiverRows(
      seatByName.get(relative.bottom)?.river ?? [],
      6,
    );
    bottomRows.forEach((row, rIdx) => {
      row.forEach((entry, cIdx) => {
        const x = layout.rivers.bottom.x + cIdx * (tw + gap) + tw / 2;
        const y = layout.rivers.bottom.y + rIdx * (th + gap) + th / 2;
        placeTile(
          entry.tile,
          x,
          y,
          0,
          entry.is_tsumogiri,
          entry.seq === latestSeq,
        );
      });
    });

    // Top river: row 0 closest to hub (bottom of top river rect), growing upward
    const topRows = buildVisibleRiverRows(
      seatByName.get(relative.top)?.river ?? [],
      6,
    );
    topRows.forEach((row, rIdx) => {
      row.forEach((entry, cIdx) => {
        const x =
          layout.rivers.top.x +
          layout.rivers.top.width -
          cIdx * (tw + gap) -
          tw / 2;
        const y =
          layout.rivers.top.y +
          layout.rivers.top.height -
          rIdx * (th + gap) -
          th / 2;
        placeTile(
          entry.tile,
          x,
          y,
          Math.PI,
          entry.is_tsumogiri,
          entry.seq === latestSeq,
        );
      });
    });

    // Left river: row 0 closest to hub (right side of left river rect), growing leftward
    const leftRows = buildVisibleRiverRows(
      seatByName.get(relative.left)?.river ?? [],
      6,
    );
    leftRows.forEach((row, rIdx) => {
      row.forEach((entry, cIdx) => {
        const x =
          layout.rivers.left.x +
          layout.rivers.left.width -
          rIdx * (th + gap) -
          th / 2;
        const y = layout.rivers.left.y + cIdx * (tw + gap) + tw / 2;
        placeTile(
          entry.tile,
          x,
          y,
          Math.PI / 2,
          entry.is_tsumogiri,
          entry.seq === latestSeq,
        );
      });
    });

    // Right river: row 0 closest to hub (left side of right river rect), growing rightward
    const rightRows = buildVisibleRiverRows(
      seatByName.get(relative.right)?.river ?? [],
      6,
    );
    rightRows.forEach((row, rIdx) => {
      row.forEach((entry, cIdx) => {
        const x = layout.rivers.right.x + rIdx * (th + gap) + th / 2;
        const y =
          layout.rivers.right.y +
          layout.rivers.right.height -
          cIdx * (tw + gap) -
          tw / 2;
        placeTile(
          entry.tile,
          x,
          y,
          -Math.PI / 2,
          entry.is_tsumogiri,
          entry.seq === latestSeq,
        );
      });
    });
  }

  private drawMelds(
    layout: TableLayout,
    relative: ReturnType<typeof getRelativeSeats>,
    seatByName: Map<Seat, PublicSeatView>,
    pendingAddedKong: PendingAddedKong | null,
  ): void {
    disposeChildren(this.meldLayer);
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
          const sprite = createTileSprite(
            this.atlas,
            slot.faceUp ? slot.tile : null,
            mw,
            mh,
            rotation,
          );
          sprite.position.set(cx, cy);
          this.meldLayer.addChild(sprite);
        });

        if (item.pendingAddedKong && pendingAddedKong !== null) {
          const secondAlong = start + (mw + tileGap) + mw / 2;
          const hoverX =
            axis === "horizontal"
              ? secondAlong
              : fixedCoord + (rotation > 0 ? mh * 0.32 : -mh * 0.32);
          const hoverY =
            axis === "horizontal" ? fixedCoord - mh * 0.28 : secondAlong;
          const pendingSprite = createTileSprite(
            this.atlas,
            pendingAddedKong.tile,
            mw,
            mh,
            rotation,
          );
          pendingSprite.position.set(hoverX, hoverY);
          this.meldLayer.addChild(pendingSprite);

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
      // Bottom viewer melds: anchored bottom-right above hand, growing leftward
      {
        seat: relative.bottom,
        axis: "horizontal",
        edgeAnchor: layout.melds.bottom.x,
        fixedCoord: layout.melds.bottom.y,
        growLeft: true,
        rotation: 0,
      },
      // Top opponent melds: anchored top-right below top hand, growing leftward
      {
        seat: relative.top,
        axis: "horizontal",
        edgeAnchor: layout.melds.top.x,
        fixedCoord: layout.melds.top.y,
        growLeft: true,
        rotation: Math.PI,
      },
      // Left opponent melds: growing downward
      {
        seat: relative.left,
        axis: "vertical",
        edgeAnchor: layout.melds.left.y,
        fixedCoord: layout.melds.left.x + mh / 2,
        rotation: Math.PI / 2,
      },
      // Right opponent melds: growing downward
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
    disposeChildren(this.opponentHandLayer);
    const { oppHandTileW: ow, oppHandTileH: oh } = layout;
    const gap = 1;

    // Top opponent
    const topCount = seatByName.get(relative.top)?.hand_count ?? 0;
    const topSpan = topCount * ow + Math.max(0, topCount - 1) * gap;
    const topStartX = layout.cx - topSpan / 2;
    const topY = layout.topMargin + oh / 2;
    for (let i = 0; i < topCount; i += 1) {
      const sprite = createTileSprite(this.atlas, null, ow, oh, Math.PI);
      sprite.position.set(topStartX + i * (ow + gap) + ow / 2, topY);
      this.opponentHandLayer.addChild(sprite);
    }

    // Left and right opponents
    const sideTileW = Math.max(12, Math.round(ow * 0.86));
    const sideTileH = Math.round(sideTileW * 1.38);
    const sideHands = [
      {
        seat: relative.left,
        x: layout.margin + sideTileH / 2,
        rotation: Math.PI / 2,
      },
      {
        seat: relative.right,
        x: layout.width - layout.margin - sideTileH / 2,
        rotation: -Math.PI / 2,
      },
    ];
    for (const side of sideHands) {
      const count = seatByName.get(side.seat)?.hand_count ?? 0;
      const span = count * sideTileW + Math.max(0, count - 1) * gap;
      const startY = layout.cy - span / 2;
      for (let i = 0; i < count; i += 1) {
        const sprite = createTileSprite(
          this.atlas,
          null,
          sideTileW,
          sideTileH,
          side.rotation,
        );
        sprite.position.set(
          side.x,
          startY + i * (sideTileW + gap) + sideTileW / 2,
        );
        this.opponentHandLayer.addChild(sprite);
      }
    }
  }

  private drawSeatBadges(
    layout: TableLayout,
    relative: ReturnType<typeof getRelativeSeats>,
    seatByName: Map<Seat, PublicSeatView>,
    infoByName: Map<Seat, SeatInfo>,
    viewer: Seat,
    dealer: Seat,
    activeSeat: Seat | null,
  ): void {
    disposeChildren(this.badgeLayer);

    const drawBadge = (
      seat: Seat,
      x: number,
      y: number,
      alignRight: boolean,
    ) => {
      const seatView = seatByName.get(seat);
      const seatInfo = infoByName.get(seat);
      if (!seatView || !seatInfo) {
        return;
      }

      const isActive = activeSeat === seat;
      const isDealer = dealer === seat;
      const wind = seatShortWind(seat);
      const role = seatRelativeRole(viewer, seat);
      const rawName = seatInfo.nickname || (seatInfo.kind === "Ai" ? "AI" : role);
      const shortName = rawName.length > 7 ? `${rawName.slice(0, 6)}…` : rawName;
      const offlineTag =
        seatInfo.kind === "Human" && !seatInfo.connected ? "·离线" : "";
      const dealerTag = isDealer ? "庄" : "";

      const line = `${dealerTag ? `[${dealerTag}] ` : ""}${wind}·${shortName}${offlineTag}  ${formatScoreDelta(seatView.score)}`;
      const badgeW = layout.isWide ? 156 : 122;
      const badgeH = 26;
      const bx = alignRight ? x - badgeW : x;

      const box = new Graphics();
      box
        .roundRect(bx, y, badgeW, badgeH, 8)
        .fill({ color: isActive ? 0x224852 : 0x102127, alpha: 0.92 })
        .stroke({
          width: isActive ? 2 : 1,
          color: isActive ? ACTIVE_GLOW : 0x35626c,
        });
      this.badgeLayer.addChild(box);

      const text = new Text({
        text: line,
        style: {
          fontFamily:
            '-apple-system, BlinkMacSystemFont, "PingFang SC", "Noto Sans CJK SC", sans-serif',
          fontSize: layout.isWide ? 12 : 10,
          fontWeight: isActive ? "700" : "600",
          fill: isActive ? ACTIVE_GLOW : TEXT_LIGHT,
        },
      });
      text.anchor.set(0.5);
      text.position.set(bx + badgeW / 2, y + badgeH / 2);
      this.badgeLayer.addChild(text);
    };

    drawBadge(relative.bottom, layout.badges.bottom.x, layout.badges.bottom.y, false);
    drawBadge(relative.top, layout.badges.top.x, layout.badges.top.y, false);
    drawBadge(relative.left, layout.badges.left.x, layout.badges.left.y, false);
    drawBadge(relative.right, layout.badges.right.x, layout.badges.right.y, true);
  }

  private drawViewerHand(layout: TableLayout, params: TableRenderParams): void {
    disposeChildren(this.viewerHandLayer);
    const view = params.snapshot.player_view;
    if (!view) {
      return;
    }

    const { closed_tiles, drawn_tile } = view.hand;
    const {
      handTileW: tw,
      handTileH: th,
      handGap: gap,
      drawnGap,
      bottomHandY: baseY,
    } = layout;

    const hasDrawn = drawn_tile !== null;
    const totalCount = closed_tiles.length + (hasDrawn ? 1 : 0);
    if (totalCount === 0) {
      return;
    }

    const closedW =
      closed_tiles.length * tw + Math.max(0, closed_tiles.length - 1) * gap;
    const fullW = closedW + (hasDrawn ? drawnGap + tw : 0);
    const startX = Math.round(layout.cx - fullW / 2);

    const addInteractiveHandTile = (
      tile: Tile,
      fromDrawn: boolean,
      x: number,
      y: number,
    ) => {
      const legal =
        !params.inputLocked &&
        findLegalDiscardAction(view.legal_actions, tile, fromDrawn) !== null;

      const sprite = createTileSprite(this.atlas, tile, tw, th, 0);
      sprite.position.set(x + tw / 2, y + th / 2);

      if (legal) {
        sprite.eventMode = "static";
        sprite.cursor = "pointer";
        sprite.hitArea = new Rectangle(
          -tw / 2,
          -th / 2 - 10,
          tw,
          th + 20,
        );

        const restY = y + th / 2;
        sprite.on("pointerover", () => {
          sprite.y = restY - 8;
          sprite.tint = 0xfff3d4;
        });
        sprite.on("pointerout", () => {
          sprite.y = restY;
          sprite.tint = 0xffffff;
        });
        sprite.on("pointertap", () => {
          this.callbacks.onDiscard(tile, fromDrawn);
        });
      } else {
        sprite.eventMode = "none";
        if (params.inputLocked) {
          sprite.alpha = 0.85;
        }
      }

      this.viewerHandLayer.addChild(sprite);
    };

    closed_tiles.forEach((tile, idx) => {
      const x = startX + idx * (tw + gap);
      addInteractiveHandTile(tile, false, x, baseY);
    });

    if (drawn_tile !== null) {
      const drawnX = startX + closedW + drawnGap;
      // Subtle highlight frame behind newly drawn tile
      const marker = new Graphics();
      marker
        .roundRect(drawnX - 2, baseY - 2, tw + 4, th + 4, 6)
        .stroke({ width: 1.5, color: BRASS, alpha: 0.7 });
      this.viewerHandLayer.addChild(marker);

      addInteractiveHandTile(drawn_tile, true, drawnX, baseY);
    }
  }
}
