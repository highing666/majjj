<script lang="ts">
  import { Application } from "pixi.js";
  import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
  import type { Tile } from "../protocol/generated/Tile";
  import type { PendingAddedKong } from "../session/reducer";
  import { TableSceneRenderer, type TableRenderParams } from "./tableRenderer";
  import type { TableLayout } from "./layout";
  import { loadTileAtlas, type TileAtlas } from "./tileAtlas";
  import { computeCanvasResolution, extractContentBoxSize } from "./viewModel";
  import {
    createWebGLRecoveryController,
    type WebGLRecoveryState,
  } from "./webglRecovery";

  interface Props {
    snapshot: TableSnapshot;
    pendingAddedKong: PendingAddedKong | null;
    inputLocked: boolean;
    confirmDiscard?: boolean;
    paperGrainEnabled?: boolean;
    reducedEffects?: boolean;
    onDiscard: (tile: Tile, fromDrawn: boolean) => void;
    onTileHover?: (tile: Tile, fromDrawn: boolean) => void;
    onTileSelect?: (tile: Tile, fromDrawn: boolean) => void;
    onLayoutChange?: (layout: TableLayout) => void;
    onFrameTick?: () => void;
    onWebGLRecoveryChange?: (state: WebGLRecoveryState) => void;
    onRefresh?: () => void;
  }

  let {
    snapshot,
    pendingAddedKong,
    inputLocked,
    confirmDiscard = false,
    paperGrainEnabled = true,
    reducedEffects = false,
    onDiscard,
    onTileHover,
    onTileSelect,
    onLayoutChange,
    onFrameTick,
    onWebGLRecoveryChange,
    onRefresh,
  }: Props = $props();

  let hostEl = $state<HTMLDivElement | null>(null);
  let sceneRenderer = $state<TableSceneRenderer | null>(null);
  let recoveryState = $state<WebGLRecoveryState>({
    status: "active",
    restoreCount: 0,
    needsRefresh: false,
    errorMessage: null,
  });

  const renderParams = $derived<TableRenderParams>({
    snapshot,
    pendingAddedKong,
    inputLocked,
    confirmDiscard,
    paperGrainEnabled,
    reducedEffects,
  });

  let latestRenderParams: TableRenderParams | null = null;

  function triggerRefresh(): void {
    if (onRefresh) {
      onRefresh();
    } else if (typeof window !== "undefined") {
      window.location.reload();
    }
  }

  $effect(() => {
    const container = hostEl;
    if (!container) {
      return;
    }

    let disposed = false;
    let app: Application | null = null;
    let atlas: TileAtlas | null = null;
    let renderer: TableSceneRenderer | null = null;
    let resizeObserver: ResizeObserver | null = null;
    let unbindCanvasRecovery: (() => void) | null = null;
    let currentWidth = 640;
    let currentHeight = 480;

    const createRendererForAtlas = (
      pixiApp: Application,
      loadedAtlas: TileAtlas,
      width: number,
      height: number,
    ): TableSceneRenderer => {
      const nextRenderer = new TableSceneRenderer(pixiApp.stage, loadedAtlas, {
        onDiscard: (tile, fromDrawn) => {
          onDiscard(tile, fromDrawn);
        },
        onTileHover: (tile, fromDrawn) => {
          onTileHover?.(tile, fromDrawn);
        },
        onTileSelect: (tile, fromDrawn) => {
          onTileSelect?.(tile, fromDrawn);
        },
        onLayoutChange: (layout) => {
          onLayoutChange?.(layout);
        },
      });
      nextRenderer.resize(width, height);
      if (latestRenderParams !== null) {
        nextRenderer.render(latestRenderParams);
      }
      return nextRenderer;
    };

    const recovery = createWebGLRecoveryController({
      rebuildResources: async () => {
        if (disposed || !app) {
          throw new Error("牌桌画布已销毁");
        }
        const prevRenderer = renderer;
        const prevAtlas = atlas;
        renderer = null;
        atlas = null;
        sceneRenderer = null;
        prevRenderer?.destroy();
        prevAtlas?.destroy();

        const rebuiltAtlas = await loadTileAtlas();
        if (disposed || !app) {
          rebuiltAtlas.destroy();
          return;
        }
        atlas = rebuiltAtlas;
        const rect = container.getBoundingClientRect();
        currentWidth = Math.max(320, Math.round(rect.width || currentWidth));
        currentHeight = Math.max(280, Math.round(rect.height || currentHeight));
        const rebuiltRenderer = createRendererForAtlas(
          app,
          rebuiltAtlas,
          currentWidth,
          currentHeight,
        );
        renderer = rebuiltRenderer;
        sceneRenderer = rebuiltRenderer;
        if (!document.hidden) {
          app.ticker.start();
        }
      },
    });

    const syncRecoveryState = (next: WebGLRecoveryState) => {
      recoveryState = next;
      onWebGLRecoveryChange?.(next);
    };
    syncRecoveryState(recovery.getState());
    const unsubscribeRecovery = recovery.subscribe((next) => {
      if (next.status === "context_lost") {
        app?.ticker.stop();
      }
      syncRecoveryState(next);
    });

    const handleVisibilityChange = () => {
      if (!app) {
        return;
      }
      if (document.hidden) {
        app.ticker.stop();
      } else if (recovery.getState().status === "active") {
        app.ticker.start();
      }
    };

    const handleTickerFrame = (ticker: { deltaMS: number }) => {
      if (!disposed && !document.hidden) {
        renderer?.tick(ticker.deltaMS);
        onFrameTick?.();
      }
    };

    const boot = async () => {
      try {
        const rect = container.getBoundingClientRect();
        currentWidth = Math.max(320, Math.round(rect.width || 640));
        currentHeight = Math.max(280, Math.round(rect.height || 480));
        const resolution = computeCanvasResolution(
          window.devicePixelRatio || 1,
        );

        const nextApp = new Application();
        await nextApp.init({
          preference: "webgl",
          resolution,
          autoDensity: true,
          antialias: true,
          backgroundColor: 0x14282e,
          width: currentWidth,
          height: currentHeight,
        });

        if (disposed) {
          nextApp.destroy(true);
          return;
        }

        const loadedAtlas = await loadTileAtlas();
        if (disposed) {
          loadedAtlas.destroy();
          nextApp.destroy(true);
          return;
        }

        app = nextApp;
        atlas = loadedAtlas;
        const canvasEl = nextApp.canvas;
        canvasEl.style.width = "100%";
        canvasEl.style.height = "100%";
        canvasEl.style.display = "block";
        canvasEl.style.touchAction = "none";
        container.replaceChildren(canvasEl);

        renderer = createRendererForAtlas(
          nextApp,
          loadedAtlas,
          currentWidth,
          currentHeight,
        );
        sceneRenderer = renderer;

        nextApp.ticker.add(handleTickerFrame);
        if (document.hidden) {
          nextApp.ticker.stop();
        }

        if (typeof ResizeObserver !== "undefined") {
          resizeObserver = new ResizeObserver((entries) => {
            if (disposed || !app || !renderer) {
              return;
            }
            for (const entry of entries) {
              const size = extractContentBoxSize(entry);
              currentWidth = Math.max(320, size.width);
              currentHeight = Math.max(280, size.height);
              app.renderer.resize(currentWidth, currentHeight);
              renderer.resize(currentWidth, currentHeight);
            }
          });
          resizeObserver.observe(container);
        }

        document.addEventListener("visibilitychange", handleVisibilityChange);
        unbindCanvasRecovery = recovery.bindCanvas(canvasEl);
      } catch (err) {
        if (!disposed) {
          const detail =
            err instanceof Error
              ? err.message
              : "WebGL2 牌桌画布初始化失败";
          recovery.reportUnrecoverable(detail);
        }
      }
    };

    void boot();

    return () => {
      disposed = true;
      sceneRenderer = null;
      document.removeEventListener("visibilitychange", handleVisibilityChange);
      unbindCanvasRecovery?.();
      unsubscribeRecovery();
      recovery.dispose();
      if (resizeObserver) {
        resizeObserver.disconnect();
      }
      if (app) {
        app.ticker.remove(handleTickerFrame);
      }
      if (renderer) {
        renderer.destroy();
      }
      if (atlas) {
        atlas.destroy();
      }
      if (app) {
        app.destroy(true);
      }
      container.replaceChildren();
    };
  });

  $effect(() => {
    const params = renderParams;
    latestRenderParams = params;
    if (sceneRenderer) {
      sceneRenderer.render(params);
    }
  });
</script>

<div class="table-canvas-host" bind:this={hostEl}>
  {#if recoveryState.needsRefresh}
    <div class="canvas-fallback" role="alert">
      <strong>2D 牌桌渲染已中断</strong>
      <span>{recoveryState.errorMessage}</span>
      <button class="btn btn-primary" type="button" onclick={triggerRefresh}>
        刷新页面
      </button>
    </div>
  {:else if recoveryState.status === "context_lost" || recoveryState.status === "restoring"}
    <div class="canvas-status" role="status">
      正在重建牌桌画布纹理，请稍候…
    </div>
  {/if}
</div>

<style>
  .table-canvas-host {
    position: relative;
    width: 100%;
    height: 100%;
    min-height: 0;
    border-radius: 0;
    overflow: hidden;
    background: #14282e;
    border: none;
    box-shadow: inset 0 0 24px rgba(8, 12, 26, 0.55);
    user-select: none;
    -webkit-user-select: none;
  }

  .canvas-fallback {
    position: absolute;
    inset: 0;
    z-index: 5;
    background: rgba(16, 32, 38, 0.94);
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 20px;
    color: #f5eedf;
    text-align: center;
  }

  .canvas-status {
    position: absolute;
    inset: 0;
    z-index: 4;
    background: rgba(16, 32, 38, 0.72);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    color: #f5eedf;
    font-weight: 600;
    text-align: center;
  }
</style>
