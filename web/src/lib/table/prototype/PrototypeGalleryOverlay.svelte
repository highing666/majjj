<!-- PROTOTYPE (throwaway for #78): Full 34-tile close-up showcase (standing + laid 3D bodies)
     plus enlarged TileBadge (`xs` / `sm` / `md`) comparison panel. -->
<script lang="ts">
  import type { Tile } from "../../protocol/generated/Tile";
  import TileBadge from "../TileBadge.svelte";
  import { getTileLabel } from "../tiles";
  import {
    PROTOTYPE_TILE_BADGE_DIMENSIONS,
    PROTOTYPE_VARIANTS,
    type PrototypeVariantId,
  } from "./variants";

  interface Props {
    variant: PrototypeVariantId;
    mode: "gallery_34" | "badge_strip";
    onClose?: () => void;
  }

  let { variant, mode, onClose }: Props = $props();

  let bodyView = $state<"both" | "standing" | "laid">("both");

  const meta = $derived(PROTOTYPE_VARIANTS[variant]);
  const atlasW = $derived(meta.atlasMetaJson.meta.size?.w ?? 1920);
  const atlasH = $derived(meta.atlasMetaJson.meta.size?.h ?? 2048);

  const SUIT_GROUPS: Array<{ title: string; tiles: Tile[] }> = [
    { title: "万子 (1–9萬)", tiles: [0, 1, 2, 3, 4, 5, 6, 7, 8] },
    { title: "条子 (1–9条 · 含幺鸡)", tiles: [9, 10, 11, 12, 13, 14, 15, 16, 17] },
    { title: "筒子 (1–9筒 · 含大花轮)", tiles: [18, 19, 20, 21, 22, 23, 24, 25, 26] },
    { title: "字牌 (东南西北中发白)", tiles: [27, 28, 29, 30, 31, 32, 33] },
  ];

  const SPOTLIGHT_TILES: Tile[] = [0, 4, 8, 9, 13, 16, 18, 22, 27, 31, 32, 33];

  function spriteStyle(frameKey: string, widthPx: number, heightPx: number): string {
    const entry = meta.atlasMetaJson.frames[frameKey];
    if (!entry) {
      return `width:${widthPx}px;height:${heightPx}px;`;
    }
    const f = entry.frame;
    const sx = widthPx / f.w;
    const sy = heightPx / f.h;
    return [
      `width:${widthPx}px`,
      `height:${heightPx}px`,
      `background-image:url("${meta.atlasPngUrl}")`,
      `background-size:${(atlasW * sx).toFixed(2)}px ${(atlasH * sy).toFixed(2)}px`,
      `background-position:${(-f.x * sx).toFixed(2)}px ${(-f.y * sy).toFixed(2)}px`,
    ].join(";");
  }
</script>

{#if mode === "badge_strip"}
  <div class="badge-compare-strip" role="region" aria-label="TileBadge 尺寸对比">
    <span class="strip-title">
      变体 {meta.code} · TileBadge 实测尺寸
    </span>
    <div class="strip-group">
      <span class="strip-label">xs ({PROTOTYPE_TILE_BADGE_DIMENSIONS.xs.width}×{PROTOTYPE_TILE_BADGE_DIMENSIONS.xs.height} · 操作条)</span>
      <TileBadge tile={9} size="xs" />
      <TileBadge tile={18} size="xs" />
      <TileBadge tile={33} size="xs" />
    </div>
    <div class="strip-group">
      <span class="strip-label">sm ({PROTOTYPE_TILE_BADGE_DIMENSIONS.sm.width}×{PROTOTYPE_TILE_BADGE_DIMENSIONS.sm.height} · 结算/历史)</span>
      <TileBadge tile={9} size="sm" />
      <TileBadge tile={18} size="sm" />
      <TileBadge tile={31} size="sm" highlighted />
      <TileBadge tile={33} size="sm" />
    </div>
    <div class="strip-group">
      <span class="strip-label">md ({PROTOTYPE_TILE_BADGE_DIMENSIONS.md.width}×{PROTOTYPE_TILE_BADGE_DIMENSIONS.md.height} · 重点展示)</span>
      <TileBadge tile={4} size="md" />
      <TileBadge tile={9} size="md" />
      <TileBadge tile={18} size="md" />
      <TileBadge tile={33} size="md" highlighted />
      <TileBadge tile={null} faceUp={false} size="md" />
    </div>
  </div>
{:else}
  <div class="gallery-backdrop" role="dialog" aria-modal="true" aria-label="全34张牌面一览">
    <section class="gallery-panel">
      <header class="gallery-head">
        <div>
          <h2>方案 {meta.code} · {meta.name} — 全 34 张牌面与双体 3D 一览</h2>
          <p class="gallery-sub">
            {meta.tagline} · 单帧 184×248 (@2x 1920×2048)
          </p>
        </div>
        <div class="gallery-controls">
          <button
            class="g-btn"
            class:g-btn-active={bodyView === "both"}
            type="button"
            onclick={() => (bodyView = "both")}
          >
            立牌 + 倒牌
          </button>
          <button
            class="g-btn"
            class:g-btn-active={bodyView === "standing"}
            type="button"
            onclick={() => (bodyView = "standing")}
          >
            仅立牌（手牌）
          </button>
          <button
            class="g-btn"
            class:g-btn-active={bodyView === "laid"}
            type="button"
            onclick={() => (bodyView = "laid")}
          >
            仅倒牌（牌河/鸣牌）
          </button>
          {#if onClose}
            <button class="g-btn g-btn-close" type="button" onclick={onClose}>
              返回牌桌
            </button>
          {/if}
        </div>
      </header>

      <div class="spotlight-bar">
        <div class="spotlight-label">重点特写（立牌 76×103 · 模拟 14" MacBook 真实手牌尺寸）</div>
        <div class="spotlight-row">
          {#each SPOTLIGHT_TILES as t (t)}
            <div class="spotlight-item">
              <span class="tile-sprite" style={spriteStyle(`tile_${t}`, 76, 103)}></span>
              <span class="tile-cap">{getTileLabel(t)}</span>
            </div>
          {/each}
          <div class="spotlight-item">
            <span class="tile-sprite" style={spriteStyle("body_back", 76, 103)}></span>
            <span class="tile-cap">立牌背</span>
          </div>
          <div class="spotlight-item">
            <span class="tile-sprite" style={spriteStyle("body_laid_back", 76, 103)}></span>
            <span class="tile-cap">倒牌背</span>
          </div>
        </div>
      </div>

      <div class="suit-sections">
        {#each SUIT_GROUPS as group (group.title)}
          <div class="suit-group">
            <h3>{group.title}</h3>
            <div class="tiles-grid">
              {#each group.tiles as tile (tile)}
                <div class="tile-card">
                  <div class="tile-pair">
                    {#if bodyView === "both" || bodyView === "standing"}
                      <span
                        class="tile-sprite"
                        title={`${getTileLabel(tile)}（立牌）`}
                        style={spriteStyle(`tile_${tile}`, 58, 78)}
                      ></span>
                    {/if}
                    {#if bodyView === "both" || bodyView === "laid"}
                      <span
                        class="tile-sprite"
                        title={`${getTileLabel(tile)}（倒牌）`}
                        style={spriteStyle(`tile_laid_${tile}`, 48, 65)}
                      ></span>
                    {/if}
                  </div>
                  <span class="tile-cap">{getTileLabel(tile)}</span>
                </div>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    </section>
  </div>
{/if}

<style>
  .badge-compare-strip {
    position: fixed;
    top: max(8px, env(safe-area-inset-top));
    left: 50%;
    transform: translateX(-50%);
    z-index: 1150;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 8px 16px;
    max-width: calc(100vw - 24px);
    padding: 8px 14px;
    border-radius: 12px;
    background: rgba(11, 24, 29, 0.94);
    border: 1.5px solid rgba(245, 190, 88, 0.72);
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.55);
    color: #f8fafc;
    font-size: 11px;
  }

  .strip-title {
    font-weight: 800;
    color: #f5be58;
  }

  .strip-group {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  .strip-label {
    color: #cbd5e1;
    font-weight: 600;
    margin-right: 2px;
  }

  .gallery-backdrop {
    position: fixed;
    inset: 0;
    z-index: 950;
    background: rgba(8, 18, 22, 0.88);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    overflow-y: auto;
    padding: 16px 16px 96px;
    display: flex;
    justify-content: center;
    align-items: flex-start;
  }

  .gallery-panel {
    width: 100%;
    max-width: 1320px;
    background: linear-gradient(180deg, #13282f 0%, #0d1d22 100%);
    border: 1.5px solid rgba(212, 168, 87, 0.45);
    border-radius: 18px;
    padding: 16px 20px 22px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6);
    color: #f5eedf;
  }

  .gallery-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    border-bottom: 1px solid rgba(212, 168, 87, 0.22);
    padding-bottom: 12px;
    margin-bottom: 14px;
  }

  .gallery-head h2 {
    margin: 0;
    font-size: 17px;
    color: #fde68a;
  }

  .gallery-sub {
    margin: 4px 0 0;
    font-size: 12px;
    color: #cbd5e1;
  }

  .gallery-controls {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .g-btn {
    padding: 5px 11px;
    border-radius: 999px;
    border: 1px solid rgba(255, 255, 255, 0.22);
    background: rgba(255, 255, 255, 0.06);
    color: #e2e8f0;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }

  .g-btn-active {
    background: #f5be58;
    border-color: #fde68a;
    color: #111827;
  }

  .g-btn-close {
    border-color: rgba(248, 113, 113, 0.55);
    color: #fecaca;
  }

  .spotlight-bar {
    background: rgba(6, 15, 19, 0.55);
    border: 1px solid rgba(212, 168, 87, 0.24);
    border-radius: 12px;
    padding: 10px 14px;
    margin-bottom: 14px;
  }

  .spotlight-label {
    font-size: 11.5px;
    font-weight: 700;
    color: #f5be58;
    margin-bottom: 8px;
  }

  .spotlight-row {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px;
  }

  .spotlight-item {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  .suit-sections {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .suit-group h3 {
    margin: 0 0 8px;
    font-size: 13px;
    color: #93c5fd;
  }

  .tiles-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .tile-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 7px 8px 5px;
    border-radius: 10px;
    background: rgba(7, 17, 21, 0.52);
    border: 1px solid rgba(255, 255, 255, 0.09);
  }

  .tile-pair {
    display: flex;
    align-items: flex-end;
    gap: 5px;
  }

  .tile-sprite {
    display: inline-block;
    flex: 0 0 auto;
    background-repeat: no-repeat;
    filter: drop-shadow(0 3px 5px rgba(0, 0, 0, 0.48));
  }

  .tile-cap {
    font-size: 10.5px;
    color: #cbd5e1;
    font-weight: 600;
  }
</style>
