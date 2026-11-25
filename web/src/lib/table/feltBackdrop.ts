import { Container, Graphics, Sprite, Text, TilingSprite } from "pixi.js";
import type { TableLayout } from "./layout";
import type { TileAtlas } from "./tileAtlas";

export const FELT_BG = 0x14282e;
export const SPOTLIGHT_AMBER = 0xffce80;
export const VIGNETTE_INDIGO = 0x080c1a;
export const BRASS = 0xd4a857;

/**
 * Safely destroys all display-object children of a Pixi container while preserving shared atlas textures.
 */
export function disposeContainerChildren(container: Container): void {
  const children = container.removeChildren();
  for (const child of children) {
    if (child instanceof Container && child.children.length > 0) {
      disposeContainerChildren(child);
    }
    if (child instanceof Sprite || child instanceof TilingSprite) {
      child.destroy({ texture: false });
    } else if (child instanceof Text) {
      child.destroy();
    } else {
      child.destroy({ children: false });
    }
  }
}

function drawArtDecoCorner(
  g: Graphics,
  x: number,
  y: number,
  sx: number,
  sy: number,
  arm: number,
): void {
  g.moveTo(x, y + sy * arm)
    .lineTo(x, y)
    .lineTo(x + sx * arm, y)
    .stroke({ width: 2, color: BRASS, alpha: 0.56 });

  const inset = 6;
  const innerArm = Math.round(arm * 0.52);
  g.moveTo(x + sx * inset, y + sy * (inset + innerArm))
    .lineTo(x + sx * inset, y + sy * inset)
    .lineTo(x + sx * (inset + innerArm), y + sy * inset)
    .stroke({ width: 1.2, color: BRASS, alpha: 0.36 });

  const px = x + sx * (inset + 4);
  const py = y + sy * (inset + 4);
  g.poly([px, py - 3, px + 3, py, px, py + 3, px - 3, py]).fill({
    color: BRASS,
    alpha: 0.5,
  });
}

/**
 * Renders the Piltover Stage impasto felt, warm amber spotlight, indigo edge vignette,
 * and brass Art Deco corner inlays into `feltLayer`.
 */
export function renderFeltBackdrop(
  feltLayer: Container,
  layout: TableLayout,
  atlas: TileAtlas,
): void {
  disposeContainerChildren(feltLayer);

  const baseBg = new Graphics();
  baseBg.rect(0, 0, layout.width, layout.height).fill({ color: FELT_BG });
  feltLayer.addChild(baseBg);

  const impastoTile = new TilingSprite({
    texture: atlas.feltImpastoTexture,
    width: layout.width,
    height: layout.height,
    tileScale: { x: 0.75, y: 0.75 },
  });
  feltLayer.addChild(impastoTile);

  const brushOverlay = new TilingSprite({
    texture: atlas.feltBrushTexture,
    width: layout.width,
    height: layout.height,
    tileScale: { x: 0.85, y: 0.85 },
    alpha: 0.52,
  });
  feltLayer.addChild(brushOverlay);

  const overlay = new Graphics();
  const rx = Math.max(140, layout.width * 0.42);
  const ry = Math.max(110, layout.height * 0.4);
  for (const scale of [1.0, 0.72, 0.46]) {
    overlay
      .ellipse(layout.cx, layout.cy, rx * scale, ry * scale)
      .fill({ color: SPOTLIGHT_AMBER, alpha: 0.042 });
  }

  const vigSteps = [
    { inset: 0, width: 22, alpha: 0.36 },
    { inset: 12, width: 28, alpha: 0.22 },
    { inset: 28, width: 34, alpha: 0.1 },
  ];
  for (const v of vigSteps) {
    overlay
      .rect(
        v.inset,
        v.inset,
        Math.max(1, layout.width - v.inset * 2),
        Math.max(1, layout.height - v.inset * 2),
      )
      .stroke({ width: v.width, color: VIGNETTE_INDIGO, alpha: v.alpha });
  }

  const pad = layout.isWide ? 10 : 6;
  const arm = layout.isWide ? 36 : 22;
  overlay
    .roundRect(
      pad - 2,
      pad - 2,
      layout.width - (pad - 2) * 2,
      layout.height - (pad - 2) * 2,
      12,
    )
    .stroke({ width: 1, color: BRASS, alpha: 0.16 });

  drawArtDecoCorner(overlay, pad, pad, 1, 1, arm);
  drawArtDecoCorner(overlay, layout.width - pad, pad, -1, 1, arm);
  drawArtDecoCorner(overlay, pad, layout.height - pad, 1, -1, arm);
  drawArtDecoCorner(
    overlay,
    layout.width - pad,
    layout.height - pad,
    -1,
    -1,
    arm,
  );

  feltLayer.addChild(overlay);
}
