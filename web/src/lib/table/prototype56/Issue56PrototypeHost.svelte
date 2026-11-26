<!-- Three structurally distinct landscape table & traditional Chinese tile variants for #56, switchable via ?variant= on /. -->
<script lang="ts">
  import { onMount } from "svelte";
  import PrototypeSwitcher, {
    type PrototypeVariantOption,
  } from "../../ui/PrototypeSwitcher.svelte";
  import {
    applyMockDiscard,
    createMockTableState,
    SCENARIO_LABELS,
    VIEWPORT_DIMENSIONS,
    type MockTableState,
    type PrototypeVariantKey,
    type RiverDensity,
    type TurnScenario,
    type ViewportPreset,
  } from "./mockState";
  import RotateOverlay from "./RotateOverlay.svelte";
  import TileGalleryModal from "./TileGalleryModal.svelte";
  import VariantAEdgeDock from "./VariantAEdgeDock.svelte";
  import VariantBWideTheater from "./VariantBWideTheater.svelte";
  import VariantCUnifiedTray from "./VariantCUnifiedTray.svelte";

  const VARIANTS: readonly PrototypeVariantOption[] = [
    { key: "A", label: "四边贴座 · 玉骨工笔" },
    { key: "B", label: "双翼剧场 · 厚涂国漫" },
    { key: "C", label: "中盘联河 · 金石碑刻" },
  ];

  function parseInitialVariant(): PrototypeVariantKey {
    if (typeof window === "undefined") {
      return "A";
    }
    const raw = new URLSearchParams(window.location.search)
      .get("variant")
      ?.toUpperCase();
    if (raw === "A" || raw === "B" || raw === "C") {
      return raw;
    }
    return "A";
  }

  let currentVariant = $state<PrototypeVariantKey>(parseInitialVariant());
  let viewportPreset = $state<ViewportPreset>("auto");
  let scenario = $state<TurnScenario>("my_draw_13_1");
  let density = $state<RiverDensity>("mid");
  let showPersonas = $state(true);
  let reducedMotion = $state(false);
  let workbenchOpen = $state(true);
  let galleryOpen = $state(false);
  let portraitBypassed = $state(false);
  let isFullscreen = $state(false);
  let emulatedFullscreen = $state(false);
  let windowWidth = $state(
    typeof window !== "undefined" ? window.innerWidth : 1280,
  );
  let windowHeight = $state(
    typeof window !== "undefined" ? window.innerHeight : 720,
  );
  let tableState = $state<MockTableState>(
    createMockTableState("my_draw_13_1", "mid"),
  );

  function selectVariant(key: string): void {
    if (key !== "A" && key !== "B" && key !== "C") {
      return;
    }
    currentVariant = key;
    if (typeof window !== "undefined") {
      const url = new URL(window.location.href);
      url.searchParams.set("variant", key);
      window.history.replaceState(null, "", url.toString());
    }
  }

  function applyScenario(nextScenario: TurnScenario): void {
    scenario = nextScenario;
    tableState = createMockTableState(nextScenario, density);
  }

  function applyDensity(nextDensity: RiverDensity): void {
    density = nextDensity;
    tableState = createMockTableState(scenario, nextDensity);
  }

  function resetTable(): void {
    tableState = createMockTableState(scenario, density);
  }

  function handleDiscardTile(index: number, fromDrawn: boolean): void {
    tableState = applyMockDiscard(tableState, index, fromDrawn);
    scenario = tableState.scenario;
  }

  function handleActionClick(label: string): void {
    const nextScenario: TurnScenario =
      label === "碰" ? "my_post_pon_13" : "my_draw_13_1";
    scenario = nextScenario;
    const base = createMockTableState(nextScenario, density);
    tableState = {
      ...base,
      lastActionNote: `点击动作「${label}」→ 已切换演示状态`,
    };
  }

  async function toggleFullscreen(): Promise<void> {
    if (viewportPreset === "iphone-land" || viewportPreset === "iphone-port") {
      emulatedFullscreen = !emulatedFullscreen;
      isFullscreen = emulatedFullscreen;
      return;
    }
    if (typeof document === "undefined") {
      return;
    }
    if (document.fullscreenElement) {
      await document.exitFullscreen().catch(() => {});
      emulatedFullscreen = false;
      isFullscreen = false;
      return;
    }
    if (document.documentElement.requestFullscreen) {
      try {
        await document.documentElement.requestFullscreen();
        isFullscreen = true;
      } catch {
        emulatedFullscreen = !emulatedFullscreen;
        isFullscreen = emulatedFullscreen;
      }
    } else {
      emulatedFullscreen = !emulatedFullscreen;
      isFullscreen = emulatedFullscreen;
    }
  }

  function handleGlobalKeyDown(event: KeyboardEvent): void {
    const target = event.target as HTMLElement | null;
    if (
      target &&
      (target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.tagName === "SELECT" ||
        target.isContentEditable)
    ) {
      return;
    }
    if (
      !event.altKey &&
      !event.ctrlKey &&
      !event.metaKey &&
      (event.key === "f" || event.key === "F")
    ) {
      event.preventDefault();
      void toggleFullscreen();
    } else if (event.key === "Escape" && emulatedFullscreen) {
      emulatedFullscreen = false;
      isFullscreen = false;
    }
  }

  onMount(() => {
    const onResize = () => {
      windowWidth = window.innerWidth;
      windowHeight = window.innerHeight;
    };
    const onFullscreenChange = () => {
      isFullscreen = Boolean(document.fullscreenElement) || emulatedFullscreen;
    };
    window.addEventListener("resize", onResize);
    window.addEventListener("keydown", handleGlobalKeyDown);
    document.addEventListener("fullscreenchange", onFullscreenChange);
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("keydown", handleGlobalKeyDown);
      document.removeEventListener("fullscreenchange", onFullscreenChange);
    };
  });

  // Target simulated viewport size & CSS scale-to-fit inside the browser window
  const targetDimensions = $derived.by(() => {
    if (viewportPreset === "auto") {
      return {
        width: Math.max(320, windowWidth),
        height: Math.max(280, windowHeight),
        label: `自适应 ${windowWidth}×${windowHeight}`,
      };
    }
    return VIEWPORT_DIMENSIONS[viewportPreset];
  });

  const frameScale = $derived.by(() => {
    if (viewportPreset === "auto") {
      return 1;
    }
    const padW = 36;
    const padH = workbenchOpen ? 164 : 88;
    const availW = Math.max(320, windowWidth - padW);
    const availH = Math.max(240, windowHeight - padH);
    return Math.min(
      1,
      availW / targetDimensions.width,
      availH / targetDimensions.height,
    );
  });

  const isNarrowPortrait = $derived(
    targetDimensions.height > targetDimensions.width &&
      targetDimensions.width <= 520,
  );

  $effect(() => {
    if (!isNarrowPortrait) {
      portraitBypassed = false;
    }
  });
</script>

<div class="proto-root" class:proto-auto-mode={viewportPreset === "auto"}>
  <!-- Collapsible Top-Left Prototype Workbench -->
  <aside
    class="workbench"
    class:workbench-collapsed={!workbenchOpen}
    aria-label="原型控制与状态面板"
  >
    <div class="wb-header">
      <button
        class="wb-toggle"
        type="button"
        onclick={() => (workbenchOpen = !workbenchOpen)}
      >
        {workbenchOpen ? "收起 #56 调试台 ▾" : "展开 #56 调试台 ▸"}
      </button>
      <span class="wb-state-pill">
        方案 {currentVariant} · {targetDimensions.label} · {SCENARIO_LABELS[tableState.scenario]} · 余 {tableState.wallRemaining} 张
      </span>
    </div>

    {#if workbenchOpen}
      <div class="wb-controls">
        <div class="wb-group">
          <span class="wb-label">模拟视口</span>
          <div class="wb-pills">
            {#each [
              ["auto", "自适应"],
              ["mac-1440", "Mac 1440×900"],
              ["mac-1280", "Mac 1280×720"],
              ["iphone-land", "iPhone 横 844×390"],
              ["iphone-port", "iPhone 竖 390×844"],
            ] as [key, text] (key)}
              <button
                type="button"
                class="wb-pill"
                class:wb-pill-on={viewportPreset === key}
                onclick={() => (viewportPreset = key as ViewportPreset)}
              >
                {text}
              </button>
            {/each}
          </div>
        </div>

        <div class="wb-group">
          <span class="wb-label">回合状态</span>
          <div class="wb-pills">
            {#each [
              ["my_draw_13_1", "摸牌 13+1"],
              ["my_post_pon_13", "碰后 13张"],
              ["my_action_prompt", "胡/碰动作栏"],
              ["wait_right", "等下家"],
              ["wait_top", "等对门"],
              ["wait_left", "等上家"],
            ] as [key, text] (key)}
              <button
                type="button"
                class="wb-pill"
                class:wb-pill-on={tableState.scenario === key}
                onclick={() => applyScenario(key as TurnScenario)}
              >
                {text}
              </button>
            {/each}
          </div>
        </div>

        <div class="wb-group">
          <span class="wb-label">牌河与选项</span>
          <div class="wb-pills">
            <button
              type="button"
              class="wb-pill"
              class:wb-pill-on={density === "mid"}
              onclick={() => applyDensity("mid")}
            >
              中盘牌河
            </button>
            <button
              type="button"
              class="wb-pill"
              class:wb-pill-on={density === "late"}
              onclick={() => applyDensity("late")}
            >
              残局满河
            </button>
            <button
              type="button"
              class="wb-pill"
              class:wb-pill-on={showPersonas}
              onclick={() => (showPersonas = !showPersonas)}
            >
              #63 牌友气泡
            </button>
            <button
              type="button"
              class="wb-pill"
              class:wb-pill-on={reducedMotion}
              onclick={() => (reducedMotion = !reducedMotion)}
            >
              减弱动效
            </button>
            <button
              type="button"
              class="wb-pill wb-pill-gold"
              onclick={() => (galleryOpen = true)}
            >
              34张牌面 &amp; AI样张对比
            </button>
            <button type="button" class="wb-pill" onclick={resetTable}>
              重置局面
            </button>
          </div>
        </div>

        <div class="wb-readout">
          <span><strong>最近状态：</strong>{tableState.lastActionNote}</span>
          <span><strong>全屏状态：</strong>{isFullscreen ? "沉浸全屏中 (F/Esc 退出)" : "标准 100dvh 铺满"}</span>
          <span><strong>交互提示：</strong>轮到自家时，双击手牌即可模拟打出落河</span>
        </div>
      </div>
    {/if}
  </aside>

  <!-- Table Stage / Simulated Device Viewport -->
  <div class="stage-canvas-wrap">
    <div
      class="device-frame"
      class:device-bezel={viewportPreset !== "auto"}
      class:device-emulated-fs={emulatedFullscreen}
      style:width="{targetDimensions.width}px"
      style:height="{targetDimensions.height}px"
      style:transform={viewportPreset === "auto"
        ? "none"
        : `scale(${frameScale})`}
    >
      {#if currentVariant === "A"}
        <VariantAEdgeDock
          table={tableState}
          width={targetDimensions.width}
          height={targetDimensions.height}
          {isFullscreen}
          {reducedMotion}
          {showPersonas}
          onDiscardTile={handleDiscardTile}
          onActionClick={handleActionClick}
          onToggleFullscreen={() => void toggleFullscreen()}
          onOpenGallery={() => (galleryOpen = true)}
          onReset={resetTable}
        />
      {:else if currentVariant === "B"}
        <VariantBWideTheater
          table={tableState}
          width={targetDimensions.width}
          height={targetDimensions.height}
          {isFullscreen}
          {reducedMotion}
          {showPersonas}
          onDiscardTile={handleDiscardTile}
          onActionClick={handleActionClick}
          onToggleFullscreen={() => void toggleFullscreen()}
          onOpenGallery={() => (galleryOpen = true)}
          onReset={resetTable}
        />
      {:else}
        <VariantCUnifiedTray
          table={tableState}
          width={targetDimensions.width}
          height={targetDimensions.height}
          {isFullscreen}
          {reducedMotion}
          {showPersonas}
          onDiscardTile={handleDiscardTile}
          onActionClick={handleActionClick}
          onToggleFullscreen={() => void toggleFullscreen()}
          onOpenGallery={() => (galleryOpen = true)}
          onReset={resetTable}
        />
      {/if}

      <RotateOverlay
        visible={isNarrowPortrait && !portraitBypassed}
        {reducedMotion}
        onBypass={() => (portraitBypassed = true)}
      />

      {#if isNarrowPortrait && portraitBypassed}
        <button
          class="restore-rotate-chip"
          type="button"
          onclick={() => (portraitBypassed = false)}
        >
          恢复横屏提示
        </button>
      {/if}
    </div>
  </div>

  <TileGalleryModal
    open={galleryOpen}
    activeVariant={currentVariant}
    onSelectVariant={(v) => selectVariant(v)}
    onClose={() => (galleryOpen = false)}
  />

  <PrototypeSwitcher
    variants={VARIANTS}
    current={currentVariant}
    compact={viewportPreset === "iphone-land"}
    onSelect={selectVariant}
  />
</div>

<style>
  .proto-root {
    position: fixed;
    inset: 0;
    overflow: hidden;
    background: #081115;
    display: flex;
    flex-direction: column;
    font-family:
      -apple-system, BlinkMacSystemFont, "PingFang SC", "Helvetica Neue",
      sans-serif;
  }

  .workbench {
    position: relative;
    z-index: 60;
    margin: 8px 10px 0;
    padding: 7px 10px;
    border-radius: 12px;
    background: rgba(15, 23, 42, 0.92);
    border: 1px solid rgba(148, 163, 184, 0.35);
    color: #f1f5f9;
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .proto-auto-mode .workbench {
    position: fixed;
    left: 10px;
    top: 40px;
    max-width: min(760px, calc(100vw - 20px));
    margin: 0;
  }

  .workbench-collapsed {
    width: fit-content;
    padding: 4px 9px;
  }

  .wb-header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .wb-toggle {
    padding: 3px 9px;
    border-radius: 7px;
    border: 1px solid rgba(56, 189, 248, 0.5);
    background: rgba(56, 189, 248, 0.16);
    color: #bae6fd;
    font-size: 11.5px;
    font-weight: 700;
  }

  .wb-state-pill {
    font-size: 11.5px;
    color: #cbd5e1;
    font-weight: 600;
  }

  .wb-controls {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .wb-group {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .wb-label {
    font-size: 11px;
    font-weight: 700;
    color: #94a3b8;
    min-width: 64px;
  }

  .wb-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }

  .wb-pill {
    padding: 3px 8px;
    border-radius: 6px;
    border: 1px solid rgba(148, 163, 184, 0.3);
    background: rgba(30, 41, 59, 0.85);
    color: #e2e8f0;
    font-size: 11px;
    font-weight: 600;
  }

  .wb-pill:hover {
    border-color: #38bdf8;
  }

  .wb-pill-on {
    background: #0284c7;
    border-color: #7dd3fc;
    color: #fff;
  }

  .wb-pill-gold {
    background: rgba(245, 190, 88, 0.2);
    border-color: #f5be58;
    color: #fde68a;
  }

  .wb-readout {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    font-size: 11px;
    color: #cbd5e1;
    padding-top: 3px;
    border-top: 1px solid rgba(148, 163, 184, 0.2);
  }

  .stage-canvas-wrap {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  .device-frame {
    position: relative;
    transform-origin: center center;
    overflow: hidden;
  }

  .device-bezel {
    border-radius: 18px;
    border: 2px solid rgba(212, 168, 87, 0.55);
    box-shadow: 0 22px 60px rgba(0, 0, 0, 0.72);
  }

  .device-emulated-fs {
    box-shadow:
      0 0 0 4px rgba(245, 190, 88, 0.55),
      0 22px 60px rgba(0, 0, 0, 0.85);
  }

  .restore-rotate-chip {
    position: absolute;
    left: 12px;
    bottom: 12px;
    z-index: 40;
    padding: 4px 10px;
    border-radius: 999px;
    border: 1px solid rgba(245, 190, 88, 0.55);
    background: rgba(15, 23, 42, 0.88);
    color: #fde68a;
    font-size: 11px;
    font-weight: 700;
  }
</style>
