<script lang="ts">
  import { Application } from "pixi.js";
  import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
  import type { Tile } from "../protocol/generated/Tile";
  import type { PendingAddedKong } from "../session/reducer";
  import { TableSceneRenderer, type TableRenderParams } from "./tableRenderer";
  import { createTileAtlas, type TileAtlas } from "./tileAtlas";
  import { computeCanvasResolution, extractContentBoxSize } from "./viewModel";

  interface Props {
    snapshot: TableSnapshot;
    pendingAddedKong: PendingAddedKong | null;
    inputLocked: boolean;
    onDiscard: (tile: Tile, fromDrawn: boolean) => void;
  }

  let { snapshot, pendingAddedKong, inputLocked, onDiscard }: Props = $props();

  let hostEl = $state<HTMLDivElement | null>(null);
  let initError = $state<string | null>(null);
  let sceneRenderer = $state<TableSceneRenderer | null>(null);
  const renderParams = $derived<TableRenderParams>({
    snapshot,
    pendingAddedKong,
    inputLocked,
  });

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
    let canvasEl: HTMLCanvasElement | null = null;

    const handleVisibilityChange = () => {
      if (!app) {
        return;
      }
      if (document.hidden) {
        app.ticker.stop();
      } else {
        app.ticker.start();
      }
    };

    const handleContextLost = (event: Event) => {
      event.preventDefault();
    };

    const handleContextRestored = () => {
      if (disposed || !app || !renderer) {
        return;
      }
      renderer.render(renderParams);
    };

    const boot = async () => {
      try {
        const rect = container.getBoundingClientRect();
        const initWidth = Math.max(320, Math.round(rect.width || 640));
        const initHeight = Math.max(280, Math.round(rect.height || 480));
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
          width: initWidth,
          height: initHeight,
        });

        if (disposed) {
          nextApp.destroy(true);
          return;
        }

        app = nextApp;
        canvasEl = nextApp.canvas;
        canvasEl.style.width = "100%";
        canvasEl.style.height = "100%";
        canvasEl.style.display = "block";
        canvasEl.style.touchAction = "manipulation";
        container.replaceChildren(canvasEl);

        atlas = createTileAtlas();
        renderer = new TableSceneRenderer(nextApp.stage, atlas, {
          onDiscard: (tile, fromDrawn) => {
            onDiscard(tile, fromDrawn);
          },
        });
        renderer.resize(initWidth, initHeight);
        renderer.render(renderParams);
        sceneRenderer = renderer;
        initError = null;

        if (typeof ResizeObserver !== "undefined") {
          resizeObserver = new ResizeObserver((entries) => {
            if (disposed || !app || !renderer) {
              return;
            }
            for (const entry of entries) {
              const size = extractContentBoxSize(entry);
              const w = Math.max(320, size.width);
              const h = Math.max(280, size.height);
              app.renderer.resize(w, h);
              renderer.resize(w, h);
            }
          });
          resizeObserver.observe(container);
        }

        document.addEventListener("visibilitychange", handleVisibilityChange);
        canvasEl.addEventListener("webglcontextlost", handleContextLost);
        canvasEl.addEventListener(
          "webglcontextrestored",
          handleContextRestored,
        );
      } catch (err) {
        if (!disposed) {
          initError =
            err instanceof Error
              ? err.message
              : "WebGL2 牌桌画布初始化失败";
        }
      }
    };

    void boot();

    return () => {
      disposed = true;
      sceneRenderer = null;
      document.removeEventListener("visibilitychange", handleVisibilityChange);
      if (canvasEl) {
        canvasEl.removeEventListener("webglcontextlost", handleContextLost);
        canvasEl.removeEventListener(
          "webglcontextrestored",
          handleContextRestored,
        );
      }
      if (resizeObserver) {
        resizeObserver.disconnect();
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
    if (sceneRenderer) {
      sceneRenderer.render(params);
    }
  });
</script>

<div class="table-canvas-host" bind:this={hostEl}>
  {#if initError}
    <div class="canvas-fallback" role="alert">
      <strong>无法启动 2D 牌桌渲染</strong>
      <span>{initError}</span>
    </div>
  {/if}
</div>

<style>
  .table-canvas-host {
    position: relative;
    width: 100%;
    height: clamp(340px, 68dvh, 720px);
    min-height: 320px;
    border-radius: 16px;
    overflow: hidden;
    background: #14282e;
    border: 1px solid #2b545e;
    box-shadow: 0 10px 28px rgba(15, 32, 38, 0.24);
    user-select: none;
    -webkit-user-select: none;
  }

  .canvas-fallback {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 20px;
    color: #f5eedf;
    text-align: center;
  }
</style>
