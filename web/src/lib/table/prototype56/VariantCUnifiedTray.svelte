<script lang="ts">
  import feltBrushTex from "../../../assets/textures/felt-brush@2x.png";
  import feltImpastoTex from "../../../assets/textures/felt-impasto@2x.png";
  import type { MockTableState, RelativeSlot } from "./mockState";
  import TraditionalTile from "./TraditionalTile.svelte";

  interface Props {
    table: MockTableState;
    width: number;
    height: number;
    isFullscreen: boolean;
    reducedMotion: boolean;
    showPersonas: boolean;
    onDiscardTile: (index: number, fromDrawn: boolean) => void;
    onActionClick: (label: string) => void;
    onToggleFullscreen: () => void;
    onOpenGallery: () => void;
    onReset: () => void;
  }

  let {
    table,
    width,
    height,
    isFullscreen,
    reducedMotion,
    showPersonas,
    onDiscardTile,
    onActionClick,
    onToggleFullscreen,
    onOpenGallery,
    onReset,
  }: Props = $props();

  let selectedTile = $state<{ index: number; fromDrawn: boolean } | null>(null);
  let fastPace = $state(false);

  const isShort = $derived(height <= 460);
  const isViewerTurn = $derived(
    table.activeSlot === "bottom" && table.actionItems.length === 0,
  );
  const hasActionPrompt = $derived(table.actionItems.length > 0);
  const isIdleWait = $derived(!isViewerTurn && !hasActionPrompt);

  // Variant C: Physical Auto-Table Unified Brass-Inlaid Tray (8/row top-bottom, 6/col left-right) + right-edge vertical micro-dock
  const metrics = $derived.by(() => {
    const handTileW = Math.max(
      28,
      Math.min(58, Math.floor((width - 170) / 16.2), Math.floor(height * 0.13)),
    );
    const handTileH = Math.round(handTileW * 1.36);

    const topRows = Math.max(1, Math.ceil(table.seats.top.river.length / 8));
    const botRows = Math.max(1, Math.ceil(table.seats.bottom.river.length / 8));
    const maxVerticalRows = Math.max(topRows, botRows, 2);

    const oppTileW = isShort ? 16 : Math.max(20, Math.round(handTileW * 0.44));
    const oppTileH = Math.round(oppTileW * 1.36);
    const sideTileW = isShort ? 13 : Math.max(15, Math.round(oppTileW * 0.85));
    const sideTileH = Math.round(sideTileW * 1.36);

    const reservedH =
      (isShort ? 26 : 36) +
      handTileH +
      (isShort ? 28 : 40) +
      (isShort ? 48 : 66) +
      (isShort ? 30 : 48);
    const availRiverH = Math.max(56, height - reservedH);
    const riverTileH = Math.max(
      18,
      Math.min(
        isShort ? 27 : 39,
        Math.floor(availRiverH / (maxVerticalRows * 2 + 0.8)),
      ),
    );
    const riverTileW = Math.round(riverTileH / 1.36);
    const meldTileW = Math.max(17, Math.round(handTileW * 0.48));
    const meldTileH = Math.round(meldTileW * 1.36);
    const hubSize = isShort ? 48 : Math.min(72, Math.round(height * 0.098));

    return {
      handTileW,
      handTileH,
      oppTileW,
      oppTileH,
      sideTileW,
      sideTileH,
      riverTileW,
      riverTileH,
      meldTileW,
      meldTileH,
      hubSize,
    };
  });

  function handleTileTap(index: number, fromDrawn: boolean): void {
    if (!isViewerTurn) {
      return;
    }
    if (
      selectedTile &&
      selectedTile.index === index &&
      selectedTile.fromDrawn === fromDrawn
    ) {
      selectedTile = null;
      onDiscardTile(index, fromDrawn);
    } else {
      selectedTile = { index, fromDrawn };
    }
  }

  function isActive(slot: RelativeSlot): boolean {
    return table.activeSlot === slot;
  }
</script>

<div
  class="stage-c"
  class:is-short={isShort}
  class:reduced-motion={reducedMotion}
  style:--felt-impasto="url({feltImpastoTex})"
  style:--felt-brush="url({feltBrushTex})"
>
  <div class="felt-layers" aria-hidden="true"></div>

  <!-- Right-edge vertical translucent micro-dock (keeps top edge 100% unobstructed) -->
  <nav class="right-micro-dock" aria-label="对局工具">
    <button
      class="dock-btn"
      type="button"
      onclick={onToggleFullscreen}
      title="沉浸全屏 (F)"
    >
      {isFullscreen ? "退" : "全屏"}
    </button>
    <button
      class="dock-btn"
      type="button"
      onclick={() => (fastPace = !fastPace)}
      title="切换节奏"
    >
      {fastPace ? "快" : "标"}
    </button>
    <button
      class="dock-btn"
      type="button"
      onclick={onOpenGallery}
      title="查看34张牌面"
    >
      牌面
    </button>
    <button class="dock-btn" type="button" onclick={onReset} title="重置局面">
      重置
    </button>
  </nav>

  <div class="table-stack">
    <!-- Top Perimeter Rim: 对门 Rim-Tab + Hand -->
    <section class="rim-top">
      <div class="rim-tab" class:rim-active={isActive("top")}>
        {#if showPersonas}
          <span class="rim-avatar" style:background={table.seats.top.avatarAccent}>
            {table.seats.top.avatarText}
          </span>
        {/if}
        <span class="rim-role">{table.seats.top.role}</span>
        <span class="rim-name">{showPersonas ? table.seats.top.name : "AI 2"}</span>
        <span class="rim-score">{table.seats.top.score.toLocaleString()}</span>
      </div>

      <div class="opp-hand">
        {#each Array(table.seats.top.concealedCount) as _, i (i)}
          <TraditionalTile
            tile={null}
            variant="C"
            body="back"
            width={metrics.oppTileW}
            height={metrics.oppTileH}
          />
        {/each}
      </div>

      {#if showPersonas && table.seats.top.banter}
        <div class="rim-banter">
          <span class="mood-tag">{table.seats.top.moodLabel}</span>
          {table.seats.top.banter}
        </div>
      {/if}
    </section>

    <!-- Middle Perimeter: 上家 Rim + Unified Brass-Inlaid Center Tray + 下家 Rim -->
    <section class="rim-middle">
      <aside class="rim-side">
        <div class="rim-tab rim-tab-col" class:rim-active={isActive("left")}>
          <div class="rim-tab-row">
            {#if showPersonas}
              <span class="rim-avatar" style:background={table.seats.left.avatarAccent}>
                {table.seats.left.avatarText}
              </span>
            {/if}
            <span class="rim-role">{table.seats.left.role}</span>
            <span class="rim-score">{table.seats.left.score.toLocaleString()}</span>
          </div>
          <span class="rim-name">{showPersonas ? table.seats.left.name : "AI 3"}</span>
        </div>

        {#if showPersonas && table.seats.left.banter}
          <div class="rim-banter rim-banter-narrow">
            <span class="mood-tag">{table.seats.left.moodLabel}</span>
            {table.seats.left.banter}
          </div>
        {/if}

        <div class="side-opp-col">
          {#each Array(table.seats.left.concealedCount) as _, i (i)}
            <div
              class="rot-slot"
              style:width="{metrics.sideTileH}px"
              style:height="{metrics.sideTileW}px"
            >
              <TraditionalTile
                tile={null}
                variant="C"
                body="back"
                width={metrics.sideTileW}
                height={metrics.sideTileH}
                rotation={90}
              />
            </div>
          {/each}
        </div>
      </aside>

      <!-- UNIFIED BRASS-INLAID CENTER TRAY (Hub + 4 River Compartments fused in one sunken frame) -->
      <div class="unified-tray">
        <span class="tray-etched-corner">{table.modeLabel}</span>

        <!-- Top Compartment (8/row) with etched ownership label -->
        <div class="tray-cell tray-top" class:cell-active={isActive("top")}>
          <span class="etched-owner owner-top">对门牌河</span>
          <div class="grid-8">
            {#each table.seats.top.river as entry (entry.seq)}
              <TraditionalTile
                tile={entry.tile}
                variant="C"
                body="laid"
                width={metrics.riverTileW}
                height={metrics.riverTileH}
                isTsumogiri={entry.isTsumogiri}
                highlighted={entry.seq === table.latestRiverSeq}
              />
            {/each}
          </div>
        </div>

        <div class="tray-mid-band">
          <!-- Left Compartment (6/col) -->
          <div class="tray-cell tray-left" class:cell-active={isActive("left")}>
            <span class="etched-owner owner-side">上家</span>
            <div class="grid-6-col">
              {#each table.seats.left.river as entry (entry.seq)}
                <div
                  class="rot-slot"
                  style:width="{metrics.riverTileH}px"
                  style:height="{metrics.riverTileW}px"
                >
                  <TraditionalTile
                    tile={entry.tile}
                    variant="C"
                    body="laid"
                    width={metrics.riverTileW}
                    height={metrics.riverTileH}
                    rotation={90}
                    isTsumogiri={entry.isTsumogiri}
                    highlighted={entry.seq === table.latestRiverSeq}
                  />
                </div>
              {/each}
            </div>
          </div>

          <!-- Center Square Inlaid Hub -->
          <div
            class="inlaid-hub"
            style:width="{metrics.hubSize}px"
            style:height="{metrics.hubSize}px"
          >
            <span class="hub-led led-t" class:led-on={isActive("top")}></span>
            <span class="hub-led led-r" class:led-on={isActive("right")}></span>
            <span class="hub-led led-b" class:led-on={isActive("bottom")}></span>
            <span class="hub-led led-l" class:led-on={isActive("left")}></span>
            <strong class="hub-title">{table.roundCompact}</strong>
            <span class="hub-wall">余 {table.wallRemaining}</span>
          </div>

          <!-- Right Compartment (6/col) -->
          <div class="tray-cell tray-right" class:cell-active={isActive("right")}>
            <div class="grid-6-col">
              {#each table.seats.right.river as entry (entry.seq)}
                <div
                  class="rot-slot"
                  style:width="{metrics.riverTileH}px"
                  style:height="{metrics.riverTileW}px"
                >
                  <TraditionalTile
                    tile={entry.tile}
                    variant="C"
                    body="laid"
                    width={metrics.riverTileW}
                    height={metrics.riverTileH}
                    rotation={-90}
                    isTsumogiri={entry.isTsumogiri}
                    highlighted={entry.seq === table.latestRiverSeq}
                  />
                </div>
              {/each}
            </div>
            <span class="etched-owner owner-side">下家</span>
          </div>
        </div>

        <!-- Bottom Compartment (8/row) -->
        <div class="tray-cell tray-bottom" class:cell-active={isActive("bottom")}>
          <span class="etched-owner owner-bot">自家牌河</span>
          <div class="grid-8">
            {#each table.seats.bottom.river as entry (entry.seq)}
              <TraditionalTile
                tile={entry.tile}
                variant="C"
                body="laid"
                width={metrics.riverTileW}
                height={metrics.riverTileH}
                isTsumogiri={entry.isTsumogiri}
                highlighted={entry.seq === table.latestRiverSeq}
              />
            {/each}
          </div>
        </div>

        <!-- Integrated bottom-rim Turn Pill / ActionBar -->
        <div class="tray-bottom-prompt">
          {#if hasActionPrompt}
            <div class="action-bar-rim" role="toolbar" aria-label="可执行操作">
              {#each table.actionItems as item (item.key)}
                <button
                  class="action-btn tone-{item.tone}"
                  type="button"
                  onclick={() => onActionClick(item.label)}
                >
                  {#if item.tile !== null}
                    <TraditionalTile
                      tile={item.tile}
                      variant="C"
                      body="badge"
                      width={isShort ? 18 : 22}
                      height={isShort ? 25 : 30}
                    />
                  {/if}
                  <span>{item.label}</span>
                </button>
              {/each}
            </div>
          {:else if isViewerTurn}
            <div class="rim-turn-pill">
              ▲ 轮到你出牌{table.viewerDrawn === null ? "（碰后请择牌）" : ""}
            </div>
          {:else}
            <div class="rim-wait-pill">等待{table.seats[table.activeSlot].role}</div>
          {/if}
        </div>
      </div>

      <aside class="rim-side rim-side-right">
        <div class="side-opp-col">
          {#each Array(table.seats.right.concealedCount) as _, i (i)}
            <div
              class="rot-slot"
              style:width="{metrics.sideTileH}px"
              style:height="{metrics.sideTileW}px"
            >
              <TraditionalTile
                tile={null}
                variant="C"
                body="back"
                width={metrics.sideTileW}
                height={metrics.sideTileH}
                rotation={-90}
              />
            </div>
          {/each}
        </div>

        <div class="right-meta-stack">
          <div class="rim-tab rim-tab-col" class:rim-active={isActive("right")}>
            <div class="rim-tab-row">
              {#if showPersonas}
                <span class="rim-avatar" style:background={table.seats.right.avatarAccent}>
                  {table.seats.right.avatarText}
                </span>
              {/if}
              <span class="rim-role">{table.seats.right.role}</span>
              <span class="rim-score">{table.seats.right.score.toLocaleString()}</span>
            </div>
            <span class="rim-name">{showPersonas ? table.seats.right.name : "AI 1"}</span>
          </div>

          {#if showPersonas && table.seats.right.banter}
            <div class="rim-banter rim-banter-narrow">
              <span class="mood-tag">{table.seats.right.moodLabel}</span>
              {table.seats.right.banter}
            </div>
          {/if}
        </div>
      </aside>
    </section>

    <!-- Bottom Rim: 自家 Rim-Tab + Illuminated Precision Floor-Rail + Standing Hand -->
    <section class="rim-bottom">
      <div class="rim-tab" class:rim-active={isActive("bottom")}>
        {#if showPersonas}
          <span class="rim-avatar" style:background={table.seats.bottom.avatarAccent}>
            {table.seats.bottom.avatarText}
          </span>
        {/if}
        <span class="rim-role">{table.seats.bottom.role}</span>
        {#if table.seats.bottom.isDealer}
          <span class="dealer-pill">庄</span>
        {/if}
        <span class="rim-name">{table.seats.bottom.name}</span>
        <span class="rim-score">{table.seats.bottom.score.toLocaleString()}</span>
      </div>

      <div class="hand-rail-wrap">
        <div
          class="hand-tray-c"
          class:tray-c-active={isViewerTurn}
          class:tray-c-idle={isIdleWait}
        >
          <div class="closed-row">
            {#each table.viewerClosed as tile, idx (idx)}
              <TraditionalTile
                {tile}
                variant="C"
                body="standing"
                width={metrics.handTileW}
                height={metrics.handTileH}
                interactive={isViewerTurn}
                selected={selectedTile?.fromDrawn === false && selectedTile.index === idx}
                highlighted={isViewerTurn && table.viewerDrawn === null}
                dimmed={isIdleWait}
                onclick={() => handleTileTap(idx, false)}
              />
            {/each}
          </div>

          {#if table.viewerDrawn !== null}
            <div class="drawn-slot" class:drawn-lifted={isViewerTurn}>
              <TraditionalTile
                tile={table.viewerDrawn}
                variant="C"
                body="standing"
                width={metrics.handTileW}
                height={metrics.handTileH}
                interactive={isViewerTurn}
                selected={selectedTile?.fromDrawn === true}
                highlighted={isViewerTurn}
                dimmed={isIdleWait}
                onclick={() => handleTileTap(0, true)}
              />
            </div>
          {/if}
        </div>
        <div class="floor-rail" class:floor-rail-on={isViewerTurn}></div>
      </div>

      {#if table.seats.bottom.melds.length > 0}
        <div class="meld-strip">
          {#each table.seats.bottom.melds as meld, mIdx (mIdx)}
            <div class="meld-group">
              {#each meld.tiles as mt, tIdx (tIdx)}
                <TraditionalTile
                  tile={mt}
                  variant="C"
                  body="laid"
                  width={metrics.meldTileW}
                  height={metrics.meldTileH}
                />
              {/each}
            </div>
          {/each}
        </div>
      {/if}
    </section>
  </div>
</div>

<style>
  .stage-c {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: #102026;
    color: #f5eedf;
    font-family:
      -apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB",
      sans-serif;
    user-select: none;
  }

  .felt-layers {
    position: absolute;
    inset: 0;
    background-image:
      radial-gradient(circle at 50% 48%, rgba(212, 168, 87, 0.12) 0%, rgba(6, 11, 20, 0.65) 80%),
      var(--felt-brush),
      var(--felt-impasto);
    background-size: cover, 420px 420px, 460px 460px;
    pointer-events: none;
  }

  /* Right-edge vertical translucent micro-dock */
  .right-micro-dock {
    position: absolute;
    right: 8px;
    top: 50%;
    transform: translateY(-50%);
    z-index: 20;
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 5px 4px;
    border-radius: 12px;
    background: rgba(8, 18, 22, 0.8);
    border: 1px solid rgba(212, 168, 87, 0.38);
    backdrop-filter: blur(6px);
  }

  .dock-btn {
    padding: 6px 5px;
    border-radius: 8px;
    border: 1px solid rgba(212, 168, 87, 0.25);
    background: rgba(255, 255, 255, 0.05);
    color: #f5eedf;
    font-size: 10.5px;
    font-weight: 700;
    line-height: 1.1;
  }

  .dock-btn:hover {
    border-color: #f5be58;
    color: #f5be58;
  }

  .table-stack {
    position: relative;
    z-index: 5;
    width: 100%;
    height: 100%;
    padding: 8px 50px 8px 14px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    align-items: center;
  }

  .is-short .table-stack {
    padding: 5px 44px 5px 10px;
  }

  .rim-top {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
  }

  .opp-hand {
    display: flex;
    align-items: center;
    gap: 1.5px;
  }

  .rim-middle {
    width: 100%;
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .rim-side {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .right-meta-stack {
    display: flex;
    flex-direction: column;
    gap: 6px;
    align-items: flex-end;
  }

  .side-opp-col {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1px;
  }

  .rot-slot {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
  }

  /* Unified Brass-Inlaid Center Tray */
  .unified-tray {
    position: relative;
    padding: 8px 10px 14px;
    border-radius: 16px;
    background: linear-gradient(180deg, rgba(9, 20, 24, 0.88) 0%, rgba(7, 15, 19, 0.94) 100%);
    border: 2px solid rgba(212, 168, 87, 0.56);
    box-shadow:
      inset 0 4px 16px rgba(0, 0, 0, 0.62),
      0 8px 26px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  .is-short .unified-tray {
    padding: 5px 7px 11px;
    gap: 3px;
  }

  .tray-etched-corner {
    position: absolute;
    left: 10px;
    top: 6px;
    font-size: 9.5px;
    font-weight: 800;
    color: rgba(212, 168, 87, 0.72);
    letter-spacing: 0.05em;
  }

  .tray-cell {
    position: relative;
    padding: 4px 6px;
    border-radius: 9px;
    border: 1px solid rgba(212, 168, 87, 0.24);
    background: rgba(14, 30, 36, 0.55);
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .tray-top,
  .tray-bottom {
    flex-direction: column;
    gap: 3px;
  }

  .cell-active {
    border-color: rgba(245, 190, 88, 0.85);
    background: rgba(245, 190, 88, 0.09);
  }

  .etched-owner {
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.08em;
    color: rgba(212, 168, 87, 0.78);
  }

  .owner-side {
    writing-mode: vertical-rl;
  }

  .grid-8 {
    display: grid;
    grid-template-columns: repeat(8, auto);
    gap: 2px;
  }

  .tray-mid-band {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
  }

  .grid-6-col {
    display: grid;
    grid-auto-flow: column;
    grid-template-rows: repeat(6, auto);
    gap: 2px;
  }

  .inlaid-hub {
    position: relative;
    border-radius: 10px;
    background: #081216;
    border: 1.5px solid rgba(212, 168, 87, 0.55);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }

  .hub-title {
    font-family: "Songti SC", "STSong", serif;
    font-size: 13.5px;
    font-weight: 900;
  }

  .hub-wall {
    font-size: 10px;
    color: #cbd5e1;
    font-weight: 700;
  }

  .hub-led {
    position: absolute;
    background: rgba(212, 168, 87, 0.16);
  }
  .led-t { top: -2px; left: 15%; right: 15%; height: 3px; }
  .led-b { bottom: -2px; left: 15%; right: 15%; height: 3px; }
  .led-l { left: -2px; top: 15%; bottom: 15%; width: 3px; }
  .led-r { right: -2px; top: 15%; bottom: 15%; width: 3px; }

  .hub-led.led-on {
    background: #f5be58;
    box-shadow: 0 0 11px 2px rgba(245, 190, 88, 0.95);
  }
  .led-t.led-on,
  .led-b.led-on {
    height: 4.5px;
    left: 6%;
    right: 6%;
  }
  .led-l.led-on,
  .led-r.led-on {
    width: 4.5px;
    top: 6%;
    bottom: 6%;
  }

  .tray-bottom-prompt {
    position: absolute;
    left: 50%;
    bottom: -13px;
    transform: translateX(-50%);
    z-index: 12;
    white-space: nowrap;
  }

  .rim-turn-pill {
    padding: 2px 14px;
    border-radius: 999px;
    background: linear-gradient(180deg, #ffe08a 0%, #eca535 100%);
    color: #181a20;
    border: 1.5px solid #181a20;
    font-size: 11.5px;
    font-weight: 900;
    box-shadow: 0 2px 12px rgba(245, 190, 88, 0.55);
  }

  .rim-wait-pill {
    padding: 2px 11px;
    border-radius: 999px;
    background: #0d1c21;
    border: 1px solid rgba(212, 168, 87, 0.35);
    color: #94a3b8;
    font-size: 10.5px;
    font-weight: 700;
  }

  .action-bar-rim {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: 999px;
    background: #0c181d;
    border: 1.5px solid #f5be58;
  }

  .action-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 999px;
    border: 1.5px solid #181a20;
    color: #fff;
    font-size: 11.5px;
    font-weight: 800;
  }

  .tone-hu { background: #b92822; border-color: #f5be58; }
  .tone-pon { background: #b97517; }
  .tone-kong { background: #176963; }
  .tone-pass { background: #33464e; }

  .rim-tab {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: 8px;
    background: rgba(9, 20, 24, 0.85);
    border: 1.25px solid rgba(212, 168, 87, 0.42);
    font-size: 11px;
    white-space: nowrap;
  }

  .rim-tab-col {
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
  }

  .rim-tab-row {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .rim-active {
    background: #183a44;
    border-color: #f5be58;
    box-shadow: 0 0 12px rgba(245, 190, 88, 0.4);
  }

  .rim-avatar {
    width: 17px;
    height: 17px;
    border-radius: 999px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 800;
    color: #fff;
  }

  .rim-role {
    padding: 1px 4px;
    border-radius: 4px;
    background: #234954;
    font-weight: 800;
    font-size: 10px;
  }

  .dealer-pill {
    padding: 1px 4px;
    border-radius: 4px;
    background: #d8513c;
    color: #fff;
    font-weight: 800;
    font-size: 10px;
  }

  .rim-name {
    font-weight: 700;
    max-width: 82px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .rim-score {
    color: #f5be58;
    font-weight: 800;
    font-variant-numeric: tabular-nums;
  }

  .rim-banter {
    padding: 4px 7px;
    border-radius: 9px;
    background: #fffdf6;
    color: #181a20;
    border: 2px solid #181a20;
    box-shadow: 0 2px 0 #181a20;
    font-size: 10.5px;
    font-weight: 700;
    line-height: 1.25;
  }

  .rim-banter-narrow {
    max-width: 138px;
  }

  .is-short .rim-banter {
    font-size: 9px;
    padding: 2.5px 5px;
  }

  .mood-tag {
    display: inline-block;
    margin-right: 4px;
    padding: 0 4px;
    border-radius: 4px;
    background: #f5be58;
    font-size: 9px;
    font-weight: 800;
  }

  .rim-bottom {
    width: 100%;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    gap: 10px;
  }

  .hand-rail-wrap {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
  }

  .hand-tray-c {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    padding: 5px 9px;
    border-radius: 12px;
    border: 1.5px solid rgba(212, 168, 87, 0.25);
    background: rgba(8, 18, 22, 0.45);
    transition: all 0.2s ease;
  }

  .tray-c-active {
    border-color: #f5be58;
    background: rgba(245, 190, 88, 0.12);
    animation: rail-pulse 1.8s ease-in-out infinite;
  }

  .reduced-motion .tray-c-active {
    animation: none;
    box-shadow: 0 0 0 2px #f5be58;
  }

  .tray-c-idle {
    filter: brightness(0.8) saturate(0.75);
  }

  @keyframes rail-pulse {
    0%, 100% { box-shadow: 0 0 10px rgba(245, 190, 88, 0.35); }
    50% { box-shadow: 0 0 22px rgba(245, 190, 88, 0.78); }
  }

  .floor-rail {
    width: 92%;
    height: 3px;
    border-radius: 999px;
    background: rgba(212, 168, 87, 0.2);
    transition: all 0.2s ease;
  }

  .floor-rail-on {
    height: 4px;
    background: #f5be58;
    box-shadow: 0 0 12px rgba(245, 190, 88, 0.9);
  }

  .closed-row {
    display: flex;
    align-items: flex-end;
    gap: 2.5px;
  }

  .drawn-slot.drawn-lifted {
    transform: translateY(-4px);
  }

  .meld-strip {
    display: flex;
    gap: 6px;
  }

  .meld-group {
    display: flex;
    gap: 1.5px;
    padding: 2px 3px;
    border-radius: 6px;
    background: rgba(8, 18, 22, 0.6);
    border: 1px solid rgba(212, 168, 87, 0.3);
  }
</style>
