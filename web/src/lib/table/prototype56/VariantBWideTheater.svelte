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

  // Variant B: Widescreen 3-Column Theater -> maximizes 自家 hand height with slim stadium hub & 10/row rivers
  const metrics = $derived.by(() => {
    const wingW = isShort ? 154 : Math.max(188, Math.min(256, Math.round(width * 0.19)));
    const centerW = Math.max(420, width - wingW * 2 - 36);

    const handTileW = Math.max(
      34,
      Math.min(64, Math.floor((centerW - 44) / 14.8), Math.floor(height * 0.15)),
    );
    const handTileH = Math.round(handTileW * 1.36);

    const topRows = Math.max(1, Math.ceil(table.seats.top.river.length / 10));
    const botRows = Math.max(1, Math.ceil(table.seats.bottom.river.length / 10));
    const maxVerticalRows = Math.max(topRows, botRows, 2);

    const oppTileW = isShort ? 16 : Math.max(20, Math.round(handTileW * 0.42));
    const oppTileH = Math.round(oppTileW * 1.36);
    const sideTileW = isShort ? 13 : Math.max(15, Math.round(oppTileW * 0.85));
    const sideTileH = Math.round(sideTileW * 1.36);

    const reservedH =
      (isShort ? 26 : 38) + // top opp hand
      handTileH +
      (isShort ? 26 : 34) + // slim turn bar
      (isShort ? 28 : 38) + // ultra-slim stadium hub
      (isShort ? 22 : 34); // paddings
    const availRiverH = Math.max(58, height - reservedH);
    const riverTileH = Math.max(
      20,
      Math.min(
        isShort ? 29 : 44,
        Math.floor(availRiverH / (maxVerticalRows * 2 + 0.5)),
        Math.floor(((centerW - 40) / 10) * 1.36),
      ),
    );
    const riverTileW = Math.round(riverTileH / 1.36);
    const meldTileW = Math.max(18, Math.round(handTileW * 0.48));
    const meldTileH = Math.round(meldTileW * 1.36);

    return {
      wingW,
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
  class="stage-b"
  class:is-short={isShort}
  class:reduced-motion={reducedMotion}
  style:--felt-impasto="url({feltImpastoTex})"
  style:--felt-brush="url({feltBrushTex})"
  style:--wing-w="{metrics.wingW}px"
>
  <div class="felt-layers" aria-hidden="true"></div>
  <!-- Layer 1 theatrical stage spotlight wash for 自家 -->
  <div
    class="stage-floor-wash"
    class:wash-on={isViewerTurn}
    aria-hidden="true"
  ></div>

  <!-- Zero top header bar: pure floating corner glass controls -->
  <div class="glass-top-left">
    <span class="glass-pill">{table.modeLabel} · {table.roundCompact}</span>
  </div>

  <div class="glass-top-right">
    <button class="glass-btn" type="button" onclick={onToggleFullscreen}>
      {isFullscreen ? "退出" : "全屏 F"}
    </button>
    <button class="glass-btn" type="button" onclick={() => (fastPace = !fastPace)}>
      {fastPace ? "极速" : "标准"}
    </button>
    <button class="glass-btn" type="button" onclick={onOpenGallery}>牌面</button>
    <button class="glass-btn" type="button" onclick={onReset}>重置</button>
  </div>

  <!-- 3-Column Widescreen Theater -->
  <div class="theater-grid">
    <!-- Left Wing: 上家 Persona Theater + 上家 River (4/col) + 自家 Dashboard -->
    <aside class="wing left-wing">
      <div class="persona-pocket" class:pocket-active={isActive("left")}>
        <div class="pocket-head">
          {#if showPersonas}
            <span class="portrait-badge" style:background={table.seats.left.avatarAccent}>
              {table.seats.left.avatarText}
            </span>
          {/if}
          <div class="pocket-meta">
            <div class="pocket-title-row">
              <span class="role-chip">{table.seats.left.role}</span>
              <strong>{showPersonas ? table.seats.left.name : "AI 3"}</strong>
            </div>
            <span class="pocket-score">{table.seats.left.score.toLocaleString()}</span>
          </div>
        </div>
        {#if showPersonas && table.seats.left.banter}
          <div class="banter-card">
            <span class="mood-pill">{table.seats.left.moodLabel}</span>
            {table.seats.left.banter}
          </div>
        {/if}
      </div>

      <div class="side-play-cluster">
        <div class="opp-col">
          {#each Array(table.seats.left.concealedCount) as _, i (i)}
            <div
              class="rot-slot"
              style:width="{metrics.sideTileH}px"
              style:height="{metrics.sideTileW}px"
            >
              <TraditionalTile
                tile={null}
                variant="B"
                body="back"
                width={metrics.sideTileW}
                height={metrics.sideTileH}
                rotation={90}
              />
            </div>
          {/each}
        </div>

        <div class="side-river-trough" class:trough-active={isActive("left")}>
          <div class="side-river-grid" style:--rows="4">
            {#each table.seats.left.river as entry (entry.seq)}
              <div
                class="rot-slot"
                style:width="{metrics.riverTileH}px"
                style:height="{metrics.riverTileW}px"
              >
                <TraditionalTile
                  tile={entry.tile}
                  variant="B"
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
      </div>

      <!-- Bottom-left dedicated 自家 dashboard so center hand spans freely -->
      <div class="viewer-dashboard" class:pocket-active={isActive("bottom")}>
        <div class="pocket-head">
          {#if showPersonas}
            <span class="portrait-badge" style:background={table.seats.bottom.avatarAccent}>
              {table.seats.bottom.avatarText}
            </span>
          {/if}
          <div class="pocket-meta">
            <div class="pocket-title-row">
              <span class="role-chip">{table.seats.bottom.role}</span>
              {#if table.seats.bottom.isDealer}
                <span class="dealer-tag">庄</span>
              {/if}
              <strong>{table.seats.bottom.name}</strong>
            </div>
            <span class="pocket-score">{table.seats.bottom.score.toLocaleString()}</span>
          </div>
        </div>
      </div>
    </aside>

    <!-- Center Stage: 对门 Hand -> 对门 Wide River (10/row) -> Slim Stadium Hub -> 自家 Wide River (10/row) -> Oversized Hand -->
    <main class="center-stage">
      <!-- Top seat compact strip -->
      <div class="top-theater-band">
        <div class="top-inline-badge" class:pocket-active={isActive("top")}>
          {#if showPersonas}
            <span class="mini-avatar" style:background={table.seats.top.avatarAccent}>
              {table.seats.top.avatarText}
            </span>
          {/if}
          <span class="role-chip">{table.seats.top.role}</span>
          <strong>{showPersonas ? table.seats.top.name : "AI 2"}</strong>
          <span class="pocket-score">{table.seats.top.score.toLocaleString()}</span>
        </div>

        <div class="opp-row">
          {#each Array(table.seats.top.concealedCount) as _, i (i)}
            <TraditionalTile
              tile={null}
              variant="B"
              body="back"
              width={metrics.oppTileW}
              height={metrics.oppTileH}
            />
          {/each}
        </div>

        {#if showPersonas && table.seats.top.banter}
          <div class="banter-inline">
            <span class="mood-pill">{table.seats.top.moodLabel}</span>
            {table.seats.top.banter}
          </div>
        {/if}
      </div>

      <!-- Top Wide River Trough (10/row) -->
      <div
        class="wide-river-trough"
        class:trough-active={isActive("top")}
        style:width="{10 * metrics.riverTileW + 9 * 2 + 14}px"
      >
        <div class="wide-river-grid" style:--cols="10">
          {#each table.seats.top.river as entry (entry.seq)}
            <TraditionalTile
              tile={entry.tile}
              variant="B"
              body="laid"
              width={metrics.riverTileW}
              height={metrics.riverTileH}
              isTsumogiri={entry.isTsumogiri}
              highlighted={entry.seq === table.latestRiverSeq}
            />
          {/each}
        </div>
      </div>

      <!-- Ultra-Slim Horizontal Stadium Center Hub -->
      <div class="stadium-hub">
        <span class="led-bar led-top" class:led-on={isActive("top")}></span>
        <span class="led-bar led-left" class:led-on={isActive("left")}></span>
        <span class="led-bar led-right" class:led-on={isActive("right")}></span>
        <span class="led-bar led-bottom" class:led-on={isActive("bottom")}></span>
        <span class="stadium-round">{table.roundCompact}</span>
        <span class="stadium-sep">·</span>
        <span class="stadium-wall">牌墙余 {table.wallRemaining}</span>
      </div>

      <!-- Bottom Wide River Trough (10/row) -->
      <div
        class="wide-river-trough"
        class:trough-active={isActive("bottom")}
        style:width="{10 * metrics.riverTileW + 9 * 2 + 14}px"
      >
        <div class="wide-river-grid" style:--cols="10">
          {#each table.seats.bottom.river as entry (entry.seq)}
            <TraditionalTile
              tile={entry.tile}
              variant="B"
              body="laid"
              width={metrics.riverTileW}
              height={metrics.riverTileH}
              isTsumogiri={entry.isTsumogiri}
              highlighted={entry.seq === table.latestRiverSeq}
            />
          {/each}
        </div>
      </div>

      <!-- Slim docked gold turn bar / ActionBar + Oversized 自家 Hand -->
      <div class="bottom-stage-dock">
        <div class="turn-bar-slot">
          {#if hasActionPrompt}
            <div class="action-bar-theater" role="toolbar" aria-label="可执行操作">
              {#each table.actionItems as item (item.key)}
                <button
                  class="action-btn tone-{item.tone}"
                  type="button"
                  onclick={() => onActionClick(item.label)}
                >
                  {#if item.tile !== null}
                    <TraditionalTile
                      tile={item.tile}
                      variant="B"
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
            <div class="turn-bar-gold">
              ✦ 轮到你出牌{table.viewerDrawn === null ? " · 碰后请择牌打出" : ""}
            </div>
          {:else}
            <div class="turn-bar-wait">
              等待{table.seats[table.activeSlot].role}行动…
            </div>
          {/if}
        </div>

        <div
          class="oversized-hand-stage"
          class:hand-stage-active={isViewerTurn}
          class:hand-stage-idle={isIdleWait}
        >
          <div class="closed-row">
            {#each table.viewerClosed as tile, idx (idx)}
              <TraditionalTile
                {tile}
                variant="B"
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
              {#if isViewerTurn}
                <span class="drawn-caret" aria-hidden="true">▼</span>
              {/if}
              <TraditionalTile
                tile={table.viewerDrawn}
                variant="B"
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
      </div>
    </main>

    <!-- Right Wing: 下家 Persona Theater + 下家 River (4/col) + Compact Meld Bay -->
    <aside class="wing right-wing">
      <div class="persona-pocket" class:pocket-active={isActive("right")}>
        <div class="pocket-head">
          {#if showPersonas}
            <span class="portrait-badge" style:background={table.seats.right.avatarAccent}>
              {table.seats.right.avatarText}
            </span>
          {/if}
          <div class="pocket-meta">
            <div class="pocket-title-row">
              <span class="role-chip">{table.seats.right.role}</span>
              <strong>{showPersonas ? table.seats.right.name : "AI 1"}</strong>
            </div>
            <span class="pocket-score">{table.seats.right.score.toLocaleString()}</span>
          </div>
        </div>
        {#if showPersonas && table.seats.right.banter}
          <div class="banter-card">
            <span class="mood-pill">{table.seats.right.moodLabel}</span>
            {table.seats.right.banter}
          </div>
        {/if}
      </div>

      <div class="side-play-cluster side-play-right">
        <div class="side-river-trough" class:trough-active={isActive("right")}>
          <div class="side-river-grid" style:--rows="4">
            {#each table.seats.right.river as entry (entry.seq)}
              <div
                class="rot-slot"
                style:width="{metrics.riverTileH}px"
                style:height="{metrics.riverTileW}px"
              >
                <TraditionalTile
                  tile={entry.tile}
                  variant="B"
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

        <div class="opp-col">
          {#each Array(table.seats.right.concealedCount) as _, i (i)}
            <div
              class="rot-slot"
              style:width="{metrics.sideTileH}px"
              style:height="{metrics.sideTileW}px"
            >
              <TraditionalTile
                tile={null}
                variant="B"
                body="back"
                width={metrics.sideTileW}
                height={metrics.sideTileH}
                rotation={-90}
              />
            </div>
          {/each}
        </div>
      </div>

      <!-- Compact bottom-right meld bay for 自家 -->
      <div class="meld-bay">
        <span class="meld-bay-label">自家鸣牌</span>
        <div class="meld-bay-items">
          {#each table.seats.bottom.melds as meld, mIdx (mIdx)}
            <div class="meld-group">
              {#each meld.tiles as mt, tIdx (tIdx)}
                <TraditionalTile
                  tile={mt}
                  variant="B"
                  body="laid"
                  width={metrics.meldTileW}
                  height={metrics.meldTileH}
                />
              {/each}
            </div>
          {/each}
        </div>
      </div>
    </aside>
  </div>
</div>

<style>
  .stage-b {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: #11242a;
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
      radial-gradient(ellipse at 50% 42%, rgba(255, 206, 128, 0.14) 0%, rgba(8, 12, 26, 0.62) 78%),
      var(--felt-brush),
      var(--felt-impasto);
    background-size: cover, 420px 420px, 460px 460px;
    pointer-events: none;
  }

  .stage-floor-wash {
    position: absolute;
    left: 14%;
    right: 14%;
    bottom: -36px;
    height: 170px;
    border-radius: 50%;
    background: radial-gradient(ellipse, rgba(245, 190, 88, 0.34) 0%, transparent 72%);
    opacity: 0;
    transition: opacity 0.25s ease;
    pointer-events: none;
  }

  .stage-floor-wash.wash-on {
    opacity: 1;
  }

  .glass-top-left {
    position: absolute;
    left: 12px;
    top: 8px;
    z-index: 15;
  }

  .glass-pill {
    display: inline-block;
    padding: 3px 10px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 700;
    background: rgba(9, 20, 24, 0.72);
    border: 1px solid rgba(212, 168, 87, 0.42);
    color: #f5be58;
    backdrop-filter: blur(6px);
  }

  .glass-top-right {
    position: absolute;
    right: 12px;
    top: 8px;
    z-index: 15;
    display: flex;
    gap: 5px;
  }

  .glass-btn {
    padding: 3px 9px;
    border-radius: 999px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    background: rgba(9, 20, 24, 0.68);
    color: #f5eedf;
    font-size: 11px;
    font-weight: 600;
    backdrop-filter: blur(6px);
  }

  .glass-btn:hover {
    border-color: #f5be58;
    color: #f5be58;
  }

  .theater-grid {
    position: relative;
    z-index: 5;
    width: 100%;
    height: 100%;
    padding: 36px 12px 10px;
    display: grid;
    grid-template-columns: var(--wing-w) 1fr var(--wing-w);
    gap: 10px;
  }

  .is-short .theater-grid {
    padding: 30px 10px 6px;
    gap: 6px;
  }

  .wing {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 6px;
    min-width: 0;
  }

  .persona-pocket,
  .viewer-dashboard {
    padding: 6px 8px;
    border-radius: 11px;
    background: rgba(9, 20, 24, 0.76);
    border: 1.5px solid rgba(212, 168, 87, 0.3);
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .is-short .persona-pocket,
  .is-short .viewer-dashboard {
    padding: 4px 6px;
    gap: 3px;
  }

  .pocket-active {
    border-color: #f5be58;
    background: rgba(24, 56, 65, 0.9);
    box-shadow: 0 0 14px rgba(245, 190, 88, 0.38);
  }

  .pocket-head {
    display: flex;
    align-items: center;
    gap: 7px;
  }

  .portrait-badge {
    width: 28px;
    height: 28px;
    border-radius: 9px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 13px;
    font-weight: 900;
    color: #fff;
    border: 2px solid #181a20;
    box-shadow: 0 2px 0 #181a20;
    flex: 0 0 auto;
  }

  .is-short .portrait-badge {
    width: 22px;
    height: 22px;
    font-size: 11px;
  }

  .pocket-meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
    font-size: 11.5px;
  }

  .pocket-title-row {
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }

  .pocket-title-row strong {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .role-chip {
    padding: 1px 5px;
    border-radius: 4px;
    background: #234954;
    font-size: 10px;
    font-weight: 800;
  }

  .dealer-tag {
    padding: 1px 4px;
    border-radius: 4px;
    background: #d8513c;
    color: #fff;
    font-size: 10px;
    font-weight: 800;
  }

  .pocket-score {
    color: #f5be58;
    font-weight: 800;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .banter-card,
  .banter-inline {
    padding: 5px 7px;
    border-radius: 9px;
    background: #fffdf6;
    color: #181a20;
    border: 2px solid #181a20;
    box-shadow: 0 2.5px 0 #181a20;
    font-size: 10.5px;
    font-weight: 700;
    line-height: 1.28;
  }

  .is-short .banter-card,
  .is-short .banter-inline {
    font-size: 9px;
    padding: 3px 5px;
  }

  .mood-pill {
    display: inline-block;
    margin-right: 4px;
    padding: 0 4px;
    border-radius: 4px;
    background: #f5be58;
    font-size: 9px;
    font-weight: 800;
  }

  .side-play-cluster {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .side-play-right {
    justify-content: flex-end;
  }

  .opp-col {
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

  .side-river-trough,
  .wide-river-trough {
    padding: 5px 6px;
    border-radius: 9px;
    background: rgba(7, 16, 20, 0.76);
    border: 1px solid rgba(212, 168, 87, 0.34);
    box-shadow: inset 0 2px 7px rgba(0, 0, 0, 0.55);
  }

  .trough-active {
    border-color: rgba(245, 190, 88, 0.85);
    box-shadow:
      inset 0 2px 7px rgba(0, 0, 0, 0.55),
      0 0 10px rgba(245, 190, 88, 0.28);
  }

  .side-river-grid {
    display: grid;
    grid-auto-flow: column;
    grid-template-rows: repeat(var(--rows), auto);
    gap: 2px;
  }

  .wide-river-grid {
    display: grid;
    grid-template-columns: repeat(var(--cols), auto);
    gap: 2px;
  }

  .meld-bay {
    padding: 5px 7px;
    border-radius: 10px;
    background: rgba(9, 20, 24, 0.68);
    border: 1px solid rgba(212, 168, 87, 0.25);
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: flex-end;
  }

  .meld-bay-label {
    font-size: 10px;
    color: #94a3b8;
    font-weight: 700;
  }

  .meld-bay-items {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .meld-group {
    display: flex;
    gap: 1.5px;
  }

  .center-stage {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: space-between;
    min-width: 0;
  }

  .top-theater-band {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .top-inline-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: 9px;
    background: rgba(9, 20, 24, 0.8);
    border: 1px solid rgba(212, 168, 87, 0.36);
    font-size: 11px;
  }

  .mini-avatar {
    width: 16px;
    height: 16px;
    border-radius: 999px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 9.5px;
    font-weight: 800;
    color: #fff;
  }

  .opp-row {
    display: flex;
    align-items: center;
    gap: 1.5px;
  }

  /* Slim Stadium Hub */
  .stadium-hub {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 5px 18px;
    border-radius: 999px;
    background: #0a171c;
    border: 1.5px solid rgba(212, 168, 87, 0.45);
    box-shadow: 0 3px 12px rgba(0, 0, 0, 0.48);
  }

  .is-short .stadium-hub {
    padding: 3px 14px;
  }

  .stadium-round {
    font-family: "Songti SC", "STSong", serif;
    font-size: 13px;
    font-weight: 900;
  }

  .stadium-wall {
    font-size: 11px;
    font-weight: 700;
    color: #cbd5e1;
  }

  .led-bar {
    position: absolute;
    background: rgba(212, 168, 87, 0.16);
    border-radius: 999px;
  }

  .led-top {
    top: -2px;
    left: 22%;
    right: 22%;
    height: 3px;
  }
  .led-bottom {
    bottom: -2px;
    left: 22%;
    right: 22%;
    height: 3px;
  }
  .led-left {
    left: -3px;
    top: 25%;
    bottom: 25%;
    width: 4px;
  }
  .led-right {
    right: -3px;
    top: 25%;
    bottom: 25%;
    width: 4px;
  }

  .led-bar.led-on {
    background: #f5be58;
    box-shadow: 0 0 12px 2px rgba(245, 190, 88, 0.95);
  }
  .led-top.led-on,
  .led-bottom.led-on {
    left: 10%;
    right: 10%;
    height: 4.5px;
  }

  .bottom-stage-dock {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  .turn-bar-slot {
    min-height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .turn-bar-gold {
    padding: 3px 16px;
    border-radius: 999px;
    background: linear-gradient(180deg, #ffe08a 0%, #eca535 100%);
    color: #181a20;
    border: 1.5px solid #181a20;
    font-size: 11.5px;
    font-weight: 900;
    box-shadow: 0 2px 14px rgba(245, 190, 88, 0.58);
  }

  .turn-bar-wait {
    font-size: 11px;
    color: #94a3b8;
    font-weight: 600;
  }

  .action-bar-theater {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 12px;
    background: rgba(11, 22, 27, 0.95);
    border: 1.5px solid #f5be58;
  }

  .action-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 9px;
    border-radius: 8px;
    border: 1.5px solid #181a20;
    color: #fff;
    font-size: 12px;
    font-weight: 800;
  }

  .tone-hu { background: #b92822; border-color: #f5be58; }
  .tone-pon { background: #b97517; }
  .tone-kong { background: #176963; }
  .tone-pass { background: #33464e; }

  .oversized-hand-stage {
    display: flex;
    align-items: flex-end;
    gap: 14px;
    padding: 6px 12px;
    border-radius: 15px;
    border: 2px solid rgba(212, 168, 87, 0.24);
    background: rgba(8, 18, 22, 0.45);
    transition: all 0.2s ease;
  }

  .is-short .oversized-hand-stage {
    padding: 4px 8px;
    gap: 10px;
  }

  .hand-stage-active {
    border-color: #f5be58;
    background: rgba(245, 190, 88, 0.14);
    animation: theater-glow 1.7s ease-in-out infinite;
  }

  .reduced-motion .hand-stage-active {
    animation: none;
    box-shadow: 0 0 0 2px #f5be58, 0 0 20px rgba(245, 190, 88, 0.6);
  }

  .hand-stage-idle {
    filter: brightness(0.72) saturate(0.72);
  }

  @keyframes theater-glow {
    0%, 100% { box-shadow: 0 0 12px rgba(245, 190, 88, 0.4); }
    50% { box-shadow: 0 0 28px rgba(245, 190, 88, 0.85); }
  }

  .closed-row {
    display: flex;
    align-items: flex-end;
    gap: 2.5px;
  }

  .drawn-slot {
    position: relative;
    transition: transform 0.18s ease;
  }

  .drawn-slot.drawn-lifted {
    transform: translateY(-5px);
  }

  .drawn-caret {
    position: absolute;
    left: 50%;
    top: -13px;
    transform: translateX(-50%);
    font-size: 10px;
    color: #f5be58;
    text-shadow: 0 1px 4px #000;
  }
</style>
