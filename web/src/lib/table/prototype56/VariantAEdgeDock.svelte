<script lang="ts">
  import feltBrushTex from "../../../assets/textures/felt-brush@2x.png";
  import feltImpastoTex from "../../../assets/textures/felt-impasto@2x.png";
  import type { MockSeatData, MockTableState, RelativeSlot } from "./mockState";
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

  // Responsive geometry for Variant A (orthogonal 4-axis symmetry, 9/row top-bottom, 5/col left-right)
  const metrics = $derived.by(() => {
    const handTileW = Math.max(
      28,
      Math.min(58, Math.floor((width - 140) / 16.5), Math.floor(height * 0.135)),
    );
    const handTileH = Math.round(handTileW * 1.36);

    const topRows = Math.max(1, Math.ceil(table.seats.top.river.length / 9));
    const botRows = Math.max(1, Math.ceil(table.seats.bottom.river.length / 9));
    const maxVerticalRows = Math.max(topRows, botRows, 2);

    const oppTileW = isShort ? 17 : Math.max(20, Math.round(handTileW * 0.45));
    const oppTileH = Math.round(oppTileW * 1.36);
    const sideTileW = isShort ? 14 : Math.max(16, Math.round(oppTileW * 0.85));
    const sideTileH = Math.round(sideTileW * 1.36);

    const reservedH =
      (isShort ? 26 : 38) + // top seat + opp hand
      handTileH +
      (isShort ? 26 : 36) + // turn pill / gap
      (isShort ? 44 : 68) + // hub
      (isShort ? 28 : 42); // margins
    const availRiverH = Math.max(60, height - reservedH);
    const riverTileH = Math.max(
      19,
      Math.min(
        isShort ? 28 : 42,
        Math.floor(availRiverH / (maxVerticalRows * 2 + 0.6)),
      ),
    );
    const riverTileW = Math.round(riverTileH / 1.36);
    const meldTileW = Math.max(18, Math.round(handTileW * 0.52));
    const meldTileH = Math.round(meldTileW * 1.36);
    const hubSize = isShort ? 46 : Math.min(74, Math.round(height * 0.1));

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

  function seatIsActive(slot: RelativeSlot): boolean {
    return table.activeSlot === slot;
  }

  function seatChipClass(seat: MockSeatData): string {
    return seatIsActive(seat.slot) ? "seat-badge seat-badge-active" : "seat-badge";
  }
</script>

<div
  class="stage-a"
  class:is-short={isShort}
  class:reduced-motion={reducedMotion}
  style:--felt-impasto="url({feltImpastoTex})"
  style:--felt-brush="url({feltBrushTex})"
>
  <div class="felt-layers" aria-hidden="true"></div>
  <div class="art-deco-frame" aria-hidden="true"></div>

  <!-- Top-left compact match tag -->
  <div class="corner-top-left">
    <span class="mode-chip">{table.modeLabel}</span>
    <span class="round-chip">{table.roundFull}</span>
  </div>

  <!-- Top-right floating translucent controls -->
  <div class="corner-top-right">
    <button
      class="ctrl-pill"
      type="button"
      onclick={onToggleFullscreen}
      title="沉浸全屏 (快捷键 F)"
    >
      {isFullscreen ? "退出全屏" : "全屏 F"}
    </button>
    <button
      class="ctrl-pill"
      type="button"
      onclick={() => (fastPace = !fastPace)}
    >
      {fastPace ? "快速" : "标准"}
    </button>
    <button class="ctrl-pill" type="button" onclick={onOpenGallery}>
      牌面集
    </button>
    <button class="ctrl-pill" type="button" onclick={onReset}>重置</button>
  </div>

  <!-- Main orthogonal 4-axis layout -->
  <div class="layout-grid">
    <!-- Top: 对门 edge-anchored badge + banter + concealed hand + melds -->
    <section class="top-zone">
      <div class="top-badge-row">
        <div class={seatChipClass(table.seats.top)}>
          {#if showPersonas}
            <span class="avatar-dot" style:background={table.seats.top.avatarAccent}>
              {table.seats.top.avatarText}
            </span>
          {/if}
          <span class="role-pill">{table.seats.top.role}</span>
          {#if table.seats.top.isDealer}
            <span class="dealer-pill">庄</span>
          {/if}
          <span class="seat-name">
            {showPersonas ? table.seats.top.name : "AI 2"}
          </span>
          <span class="seat-score">{table.seats.top.score.toLocaleString()}</span>
        </div>

        {#if showPersonas && table.seats.top.banter}
          <div class="comic-bubble bubble-top">
            <span class="mood-tag">{table.seats.top.moodLabel}</span>
            {table.seats.top.banter}
          </div>
        {/if}
      </div>

      <div class="top-tiles-row">
        <div class="opp-hand-row">
          {#each Array(table.seats.top.concealedCount) as _, i (i)}
            <TraditionalTile
              tile={null}
              variant="A"
              body="back"
              width={metrics.oppTileW}
              height={metrics.oppTileH}
            />
          {/each}
        </div>
        {#if table.seats.top.melds.length > 0}
          <div class="meld-strip">
            {#each table.seats.top.melds as meld, mIdx (mIdx)}
              <div class="meld-group">
                {#each meld.tiles as mt, tIdx (tIdx)}
                  <TraditionalTile
                    tile={mt}
                    variant="A"
                    body={mt === null ? "back" : "laid"}
                    width={metrics.meldTileW}
                    height={metrics.meldTileH}
                  />
                {/each}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </section>

    <!-- Middle row: 上家 (left) + Center Hub & 4 flush sunken wells + 下家 (right) -->
    <section class="middle-zone">
      <!-- Left: 上家 -->
      <aside class="side-seat side-left">
        <div class="side-badge-stack">
          <div class={seatChipClass(table.seats.left)}>
            {#if showPersonas}
              <span class="avatar-dot" style:background={table.seats.left.avatarAccent}>
                {table.seats.left.avatarText}
              </span>
            {/if}
            <span class="role-pill">{table.seats.left.role}</span>
            <span class="seat-name">
              {showPersonas ? table.seats.left.name : "AI 3"}
            </span>
            <span class="seat-score">{table.seats.left.score.toLocaleString()}</span>
          </div>
          {#if showPersonas && table.seats.left.banter}
            <div class="comic-bubble bubble-side">
              <span class="mood-tag">{table.seats.left.moodLabel}</span>
              {table.seats.left.banter}
            </div>
          {/if}
        </div>

        <div class="side-tiles-col">
          {#each Array(table.seats.left.concealedCount) as _, i (i)}
            <div
              class="rot-slot"
              style:width="{metrics.sideTileH}px"
              style:height="{metrics.sideTileW}px"
            >
              <TraditionalTile
                tile={null}
                variant="A"
                body="back"
                width={metrics.sideTileW}
                height={metrics.sideTileH}
                rotation={90}
              />
            </div>
          {/each}
        </div>
      </aside>

      <!-- Center: Flush 4-well River Arena + Square Compass Hub -->
      <div class="center-arena">
        <!-- Top sunken river well (9/row) -->
        <div class="arena-top-row">
          <div
            class="river-well river-well-horiz"
            class:well-active={seatIsActive("top")}
            style:width="{9 * metrics.riverTileW + 8 * 2 + 12}px"
          >
            <div class="river-grid-horiz" style:--cols="9">
              {#each table.seats.top.river as entry (entry.seq)}
                <TraditionalTile
                  tile={entry.tile}
                  variant="A"
                  body="laid"
                  width={metrics.riverTileW}
                  height={metrics.riverTileH}
                  isTsumogiri={entry.isTsumogiri}
                  highlighted={entry.seq === table.latestRiverSeq}
                />
              {/each}
            </div>
          </div>
        </div>

        <!-- Middle row: Left well (5/col) | Center Hub | Right well (5/col) -->
        <div class="arena-mid-row">
          <div
            class="river-well river-well-vert"
            class:well-active={seatIsActive("left")}
            style:height="{5 * metrics.riverTileW + 4 * 2 + 12}px"
          >
            <div class="river-grid-vert" style:--rows="5">
              {#each table.seats.left.river as entry (entry.seq)}
                <div
                  class="rot-slot"
                  style:width="{metrics.riverTileH}px"
                  style:height="{metrics.riverTileW}px"
                >
                  <TraditionalTile
                    tile={entry.tile}
                    variant="A"
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

          <!-- Compact rounded-square center hub with 4-edge directional glow -->
          <div
            class="center-hub"
            style:width="{metrics.hubSize}px"
            style:height="{metrics.hubSize}px"
          >
            <span class="hub-edge edge-top" class:edge-on={seatIsActive("top")}></span>
            <span class="hub-edge edge-right" class:edge-on={seatIsActive("right")}></span>
            <span class="hub-edge edge-bottom" class:edge-on={seatIsActive("bottom")}></span>
            <span class="hub-edge edge-left" class:edge-on={seatIsActive("left")}></span>
            <strong class="hub-round">{table.roundCompact}</strong>
            <span class="hub-wall">余 {table.wallRemaining}</span>
          </div>

          <div
            class="river-well river-well-vert"
            class:well-active={seatIsActive("right")}
            style:height="{5 * metrics.riverTileW + 4 * 2 + 12}px"
          >
            <div class="river-grid-vert" style:--rows="5">
              {#each table.seats.right.river as entry (entry.seq)}
                <div
                  class="rot-slot"
                  style:width="{metrics.riverTileH}px"
                  style:height="{metrics.riverTileW}px"
                >
                  <TraditionalTile
                    tile={entry.tile}
                    variant="A"
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
          </div>
        </div>

        <!-- Bottom sunken river well (9/row) -->
        <div class="arena-bot-row">
          <div
            class="river-well river-well-horiz"
            class:well-active={seatIsActive("bottom")}
            style:width="{9 * metrics.riverTileW + 8 * 2 + 12}px"
          >
            <div class="river-grid-horiz" style:--cols="9">
              {#each table.seats.bottom.river as entry (entry.seq)}
                <TraditionalTile
                  tile={entry.tile}
                  variant="A"
                  body="laid"
                  width={metrics.riverTileW}
                  height={metrics.riverTileH}
                  isTsumogiri={entry.isTsumogiri}
                  highlighted={entry.seq === table.latestRiverSeq}
                />
              {/each}
            </div>
          </div>
        </div>
      </div>

      <!-- Right: 下家 -->
      <aside class="side-seat side-right">
        <div class="side-tiles-col">
          {#each Array(table.seats.right.concealedCount) as _, i (i)}
            <div
              class="rot-slot"
              style:width="{metrics.sideTileH}px"
              style:height="{metrics.sideTileW}px"
            >
              <TraditionalTile
                tile={null}
                variant="A"
                body="back"
                width={metrics.sideTileW}
                height={metrics.sideTileH}
                rotation={-90}
              />
            </div>
          {/each}
        </div>

        <div class="side-badge-stack">
          <div class={seatChipClass(table.seats.right)}>
            {#if showPersonas}
              <span class="avatar-dot" style:background={table.seats.right.avatarAccent}>
                {table.seats.right.avatarText}
              </span>
            {/if}
            <span class="role-pill">{table.seats.right.role}</span>
            <span class="seat-name">
              {showPersonas ? table.seats.right.name : "AI 1"}
            </span>
            <span class="seat-score">{table.seats.right.score.toLocaleString()}</span>
          </div>
          {#if showPersonas && table.seats.right.banter}
            <div class="comic-bubble bubble-side">
              <span class="mood-tag">{table.seats.right.moodLabel}</span>
              {table.seats.right.banter}
            </div>
          {/if}
          {#if table.seats.right.melds.length > 0}
            <div class="meld-strip meld-strip-compact">
              {#each table.seats.right.melds as meld, mIdx (mIdx)}
                <div class="meld-group">
                  {#each meld.tiles as mt, tIdx (tIdx)}
                    <TraditionalTile
                      tile={mt}
                      variant="A"
                      body={mt === null ? "back" : "laid"}
                      width={Math.round(metrics.meldTileW * 0.85)}
                      height={Math.round(metrics.meldTileH * 0.85)}
                    />
                  {/each}
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </aside>
    </section>

    <!-- Bottom: Layer 2 Turn Pill / ActionBar + Layer 1/3 自家 Hand Tray + Edge Seat Badge -->
    <section class="bottom-zone">
      <div class="turn-prompt-slot">
        {#if hasActionPrompt}
          <div class="action-bar-inline" role="toolbar" aria-label="可执行操作">
            {#each table.actionItems as item (item.key)}
              <button
                class="action-btn tone-{item.tone}"
                type="button"
                onclick={() => onActionClick(item.label)}
              >
                {#if item.tile !== null}
                  <TraditionalTile
                    tile={item.tile}
                    variant="A"
                    body="badge"
                    width={isShort ? 18 : 22}
                    height={isShort ? 25 : 30}
                  />
                {/if}
                <span>{item.label}</span>
                {#if item.tileName}
                  <small>{item.tileName}</small>
                {/if}
              </button>
            {/each}
          </div>
        {:else if isViewerTurn}
          <div class="turn-pill-gold">
            ✦ 轮到你出牌{table.viewerDrawn === null ? "（碰后无摸入牌，请择牌打出）" : ""}
          </div>
        {:else}
          <div class="wait-pill-muted">
            等待{table.seats[table.activeSlot].role}出牌…
          </div>
        {/if}
      </div>

      <div class="viewer-dock-row">
        <!-- 自家 edge-anchored badge -->
        <div class={seatChipClass(table.seats.bottom)}>
          {#if showPersonas}
            <span class="avatar-dot" style:background={table.seats.bottom.avatarAccent}>
              {table.seats.bottom.avatarText}
            </span>
          {/if}
          <span class="role-pill">{table.seats.bottom.role}</span>
          {#if table.seats.bottom.isDealer}
            <span class="dealer-pill">庄</span>
          {/if}
          <span class="seat-name">{table.seats.bottom.name}</span>
          <span class="seat-score">{table.seats.bottom.score.toLocaleString()}</span>
        </div>

        <!-- Layer 1 & Layer 3: Glowing / Dimmed Standing Hand Tray -->
        <div
          class="viewer-hand-tray"
          class:tray-active={isViewerTurn}
          class:tray-post-pon={isViewerTurn && table.viewerDrawn === null}
          class:tray-idle={isIdleWait}
        >
          <div class="closed-tiles">
            {#each table.viewerClosed as tile, idx (idx)}
              <TraditionalTile
                {tile}
                variant="A"
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
            <div class="drawn-tile-slot" class:drawn-lifted={isViewerTurn}>
              <TraditionalTile
                tile={table.viewerDrawn}
                variant="A"
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

        {#if table.seats.bottom.melds.length > 0}
          <div class="meld-strip">
            {#each table.seats.bottom.melds as meld, mIdx (mIdx)}
              <div class="meld-group">
                {#each meld.tiles as mt, tIdx (tIdx)}
                  <TraditionalTile
                    tile={mt}
                    variant="A"
                    body="laid"
                    width={metrics.meldTileW}
                    height={metrics.meldTileH}
                  />
                {/each}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </section>
  </div>
</div>

<style>
  .stage-a {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: #14282e;
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
      radial-gradient(ellipse at 50% 48%, rgba(255, 206, 128, 0.14) 0%, rgba(8, 12, 26, 0.56) 76%),
      var(--felt-brush),
      var(--felt-impasto);
    background-size: cover, 420px 420px, 460px 460px;
    pointer-events: none;
  }

  .art-deco-frame {
    position: absolute;
    inset: 8px;
    border-radius: 14px;
    border: 1px solid rgba(212, 168, 87, 0.24);
    box-shadow: inset 0 0 0 1px rgba(212, 168, 87, 0.08);
    pointer-events: none;
  }

  .corner-top-left {
    position: absolute;
    left: 14px;
    top: 10px;
    z-index: 15;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .mode-chip {
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 700;
    background: rgba(212, 168, 87, 0.2);
    border: 1px solid rgba(212, 168, 87, 0.5);
    color: #f5be58;
  }

  .round-chip {
    font-size: 11.5px;
    font-weight: 600;
    color: #e2e8f0;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.6);
  }

  .corner-top-right {
    position: absolute;
    right: 14px;
    top: 10px;
    z-index: 15;
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .ctrl-pill {
    padding: 4px 9px;
    border-radius: 999px;
    border: 1px solid rgba(212, 168, 87, 0.42);
    background: rgba(11, 23, 28, 0.78);
    color: #f5eedf;
    font-size: 11.5px;
    font-weight: 600;
    backdrop-filter: blur(6px);
  }

  .ctrl-pill:hover {
    background: rgba(24, 48, 56, 0.92);
    border-color: #f5be58;
  }

  .layout-grid {
    position: relative;
    z-index: 5;
    width: 100%;
    height: 100%;
    padding: 10px 14px 12px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    align-items: center;
  }

  .is-short .layout-grid {
    padding: 6px 12px 6px;
  }

  .is-short .round-chip {
    display: none;
  }

  .top-zone {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  .top-badge-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .top-tiles-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .opp-hand-row {
    display: flex;
    align-items: center;
    gap: 1.5px;
  }

  .middle-zone {
    width: 100%;
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .side-seat {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .side-badge-stack {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: 175px;
  }

  .is-short .side-badge-stack {
    max-width: 132px;
  }

  .side-tiles-col {
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

  .center-arena {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 5px;
  }

  .arena-mid-row {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
  }

  .river-well {
    padding: 5px;
    border-radius: 8px;
    background: rgba(8, 19, 23, 0.72);
    border: 1px solid rgba(212, 168, 87, 0.36);
    box-shadow: inset 0 2px 6px rgba(0, 0, 0, 0.52);
  }

  .well-active {
    border-color: rgba(245, 190, 88, 0.8);
    box-shadow:
      inset 0 2px 6px rgba(0, 0, 0, 0.52),
      0 0 10px rgba(245, 190, 88, 0.25);
  }

  .river-grid-horiz {
    display: grid;
    grid-template-columns: repeat(var(--cols), auto);
    gap: 2px;
  }

  .river-grid-vert {
    display: grid;
    grid-auto-flow: column;
    grid-template-rows: repeat(var(--rows), auto);
    gap: 2px;
  }

  .center-hub {
    position: relative;
    border-radius: 12px;
    background: #0b181d;
    border: 1.5px solid rgba(212, 168, 87, 0.45);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1px;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.48);
  }

  .hub-round {
    font-family: "Songti SC", "STSong", serif;
    font-size: 14px;
    font-weight: 900;
    color: #f5eedf;
  }

  .hub-wall {
    font-size: 10.5px;
    font-weight: 700;
    color: #cbd5e1;
  }

  .is-short .hub-round {
    font-size: 12px;
  }
  .is-short .hub-wall {
    font-size: 9.5px;
  }

  /* Layer 4: Center Hub directional edge glow */
  .hub-edge {
    position: absolute;
    background: rgba(212, 168, 87, 0.16);
    border-radius: 999px;
    transition: all 0.2s ease;
  }

  .edge-top {
    top: -2px;
    left: 16%;
    right: 16%;
    height: 3px;
  }
  .edge-bottom {
    bottom: -2px;
    left: 16%;
    right: 16%;
    height: 3px;
  }
  .edge-left {
    left: -2px;
    top: 16%;
    bottom: 16%;
    width: 3px;
  }
  .edge-right {
    right: -2px;
    top: 16%;
    bottom: 16%;
    width: 3px;
  }

  .hub-edge.edge-on {
    background: #f5be58;
    box-shadow: 0 0 12px 2px rgba(245, 190, 88, 0.9);
  }
  .edge-top.edge-on,
  .edge-bottom.edge-on {
    height: 4.5px;
    left: 10%;
    right: 10%;
  }
  .edge-left.edge-on,
  .edge-right.edge-on {
    width: 4.5px;
    top: 10%;
    bottom: 10%;
  }

  .seat-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: 9px;
    background: rgba(11, 24, 29, 0.88);
    border: 1.25px solid rgba(212, 168, 87, 0.42);
    font-size: 11.5px;
    white-space: nowrap;
  }

  .seat-badge-active {
    background: #193b44;
    border-color: #f5be58;
    box-shadow: 0 0 12px rgba(245, 190, 88, 0.35);
  }

  .is-short .seat-badge {
    padding: 2px 6px;
    gap: 4px;
    font-size: 10.5px;
  }

  .avatar-dot {
    width: 18px;
    height: 18px;
    border-radius: 999px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 800;
    color: #fff;
    border: 1px solid #181a20;
    flex: 0 0 auto;
  }

  .role-pill {
    padding: 1px 5px;
    border-radius: 5px;
    background: #234954;
    color: #f5eedf;
    font-weight: 800;
    font-size: 10.5px;
  }

  .dealer-pill {
    padding: 1px 4px;
    border-radius: 5px;
    background: #d8513c;
    color: #fff;
    font-weight: 800;
    font-size: 10px;
  }

  .seat-name {
    font-weight: 700;
    max-width: 78px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .seat-score {
    color: #f5be58;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .comic-bubble {
    padding: 5px 8px;
    border-radius: 10px;
    background: #fffdf6;
    color: #181a20;
    border: 2px solid #181a20;
    box-shadow: 0 3px 0 #181a20;
    font-size: 11px;
    font-weight: 700;
    line-height: 1.3;
  }

  .is-short .comic-bubble {
    font-size: 9.5px;
    padding: 3px 6px;
  }

  .mood-tag {
    display: inline-block;
    margin-right: 4px;
    padding: 0 4px;
    border-radius: 4px;
    background: #f5be58;
    font-size: 9.5px;
    font-weight: 800;
  }

  .meld-strip {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .meld-strip-compact {
    flex-wrap: wrap;
  }

  .meld-group {
    display: flex;
    align-items: center;
    gap: 1.5px;
    padding: 2px 3px;
    border-radius: 6px;
    background: rgba(8, 19, 23, 0.56);
    border: 1px solid rgba(212, 168, 87, 0.28);
  }

  .bottom-zone {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
  }

  .turn-prompt-slot {
    min-height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* Layer 2: Compact gold turn pill */
  .turn-pill-gold {
    padding: 3px 14px;
    border-radius: 999px;
    background: linear-gradient(180deg, #ffd176 0%, #eca535 100%);
    color: #181a20;
    border: 1.5px solid #181a20;
    box-shadow: 0 2px 12px rgba(245, 190, 88, 0.5);
    font-size: 12px;
    font-weight: 800;
  }

  .wait-pill-muted {
    padding: 2px 12px;
    border-radius: 999px;
    background: rgba(11, 24, 29, 0.65);
    border: 1px solid rgba(212, 168, 87, 0.25);
    color: #94a3b8;
    font-size: 11px;
    font-weight: 600;
  }

  .action-bar-inline {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-radius: 12px;
    background: rgba(11, 22, 27, 0.94);
    border: 1.5px solid #d4a857;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
  }

  .action-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 9px;
    border-radius: 8px;
    border: 1.5px solid #181a20;
    color: #fff;
    font-size: 12.5px;
    font-weight: 800;
  }

  .action-btn small {
    font-size: 10.5px;
    opacity: 0.9;
  }

  .tone-hu {
    background: #b92822;
    border-color: #f5be58;
  }
  .tone-pon {
    background: #b97517;
  }
  .tone-kong {
    background: #176963;
  }
  .tone-pass {
    background: #33464e;
  }

  .viewer-dock-row {
    display: flex;
    align-items: flex-end;
    justify-content: center;
    gap: 10px;
    max-width: 100%;
  }

  /* Layer 1 & 3: Viewer Hand Tray */
  .viewer-hand-tray {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    padding: 6px 10px;
    border-radius: 14px;
    border: 1.5px solid rgba(212, 168, 87, 0.2);
    background: rgba(8, 19, 23, 0.4);
    transition:
      box-shadow 0.2s ease,
      border-color 0.2s ease,
      filter 0.2s ease;
  }

  .is-short .viewer-hand-tray {
    padding: 4px 8px;
    gap: 8px;
  }

  .viewer-hand-tray.tray-active {
    border-color: rgba(245, 190, 88, 0.9);
    background: rgba(245, 190, 88, 0.1);
    animation: pulse-gold-tray 1.8s ease-in-out infinite;
  }

  .viewer-hand-tray.tray-post-pon {
    box-shadow:
      0 0 22px rgba(245, 190, 88, 0.55),
      inset 0 0 14px rgba(245, 190, 88, 0.25);
  }

  .reduced-motion .viewer-hand-tray.tray-active {
    animation: none;
    box-shadow: 0 0 0 2px #f5be58, 0 0 18px rgba(245, 190, 88, 0.55);
  }

  .viewer-hand-tray.tray-idle {
    filter: brightness(0.84) saturate(0.82);
  }

  @keyframes pulse-gold-tray {
    0%,
    100% {
      box-shadow: 0 0 12px rgba(245, 190, 88, 0.35);
    }
    50% {
      box-shadow: 0 0 26px rgba(245, 190, 88, 0.78);
    }
  }

  .closed-tiles {
    display: flex;
    align-items: flex-end;
    gap: 2.5px;
  }

  .drawn-tile-slot {
    transition: transform 0.18s ease;
  }

  .drawn-tile-slot.drawn-lifted {
    transform: translateY(-4px);
  }
</style>
