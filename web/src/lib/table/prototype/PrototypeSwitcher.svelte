<!-- PROTOTYPE (throwaway for #78): Floating dev-only variant & scene switcher (`← / →`). -->
<script lang="ts">
  import type { TableLayout } from "../layout";
  import {
    cyclePrototypeVariant,
    PROTOTYPE_SCENES,
    PROTOTYPE_VARIANTS,
    type PrototypeSceneId,
    type PrototypeVariantId,
  } from "./variants";

  interface Props {
    currentVariant: PrototypeVariantId;
    currentScene: PrototypeSceneId;
    hasLiveMatch: boolean;
    tableLayout: TableLayout | null;
    lastActionNote?: string | null;
    onSelectVariant: (variant: PrototypeVariantId) => void;
    onSelectScene: (scene: PrototypeSceneId) => void;
  }

  let {
    currentVariant,
    currentScene,
    hasLiveMatch,
    tableLayout,
    lastActionNote = null,
    onSelectVariant,
    onSelectScene,
  }: Props = $props();

  let dockTop = $state(true);
  let collapsed = $state(
    typeof window !== "undefined" ? window.innerHeight <= 460 : false,
  );
  let showSpecPopover = $state(false);

  const meta = $derived(PROTOTYPE_VARIANTS[currentVariant]);
  const sceneMeta = $derived(
    PROTOTYPE_SCENES.find((s) => s.id === currentScene) ?? null,
  );

  const metricsSummary = $derived.by(() => {
    if (!tableLayout) {
      return "等待画布测量…";
    }
    return `${tableLayout.width}×${tableLayout.height} · 手牌 ${tableLayout.handTileW}×${tableLayout.handTileH} · 牌河 ${tableLayout.riverTileW}×${tableLayout.riverTileH} · 鸣牌 ${tableLayout.meldTileW}×${tableLayout.meldTileH}`;
  });

  function stepVariant(delta: 1 | -1): void {
    onSelectVariant(cyclePrototypeVariant(currentVariant, delta));
  }

  function handleKeyDown(event: KeyboardEvent): void {
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
    if (event.metaKey || event.ctrlKey || event.altKey) {
      return;
    }
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      stepVariant(-1);
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      stepVariant(1);
    }
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if import.meta.env.DEV}
  <aside
    class="proto-dock"
    class:proto-dock-top={dockTop}
    class:proto-dock-collapsed={collapsed}
    aria-label="原型方案切换栏"
  >
    {#if collapsed}
      <div class="proto-collapsed-pill">
        <button
          class="proto-arrow"
          type="button"
          title="上一个方案 (←)"
          aria-label="上一个方案"
          onclick={() => stepVariant(-1)}
        >
          ←
        </button>
        <button
          class="proto-expand-btn"
          type="button"
          onclick={() => (collapsed = false)}
        >
          <span class="proto-tag">#{78} 原型</span>
          <strong>{meta.code} · {meta.shortName}</strong>
          <span class="proto-caret">展开</span>
        </button>
        <button
          class="proto-arrow"
          type="button"
          title="下一个方案 (→)"
          aria-label="下一个方案"
          onclick={() => stepVariant(1)}
        >
          →
        </button>
      </div>
    {:else}
      <div class="proto-bar">
        <div class="proto-row-main">
          <div class="proto-variant-ctrl">
            <button
              class="proto-arrow"
              type="button"
              title="上一个方案 (←)"
              aria-label="上一个方案"
              onclick={() => stepVariant(-1)}
            >
              ←
            </button>
            <button
              class="proto-label-btn"
              type="button"
              title="点击查看当前变体参数说明"
              onclick={() => (showSpecPopover = !showSpecPopover)}
            >
              <span class="proto-badge">{meta.code}</span>
              <span class="proto-name">{meta.name}</span>
              <span class="proto-kbd">←/→</span>
            </button>
            <button
              class="proto-arrow"
              type="button"
              title="下一个方案 (→)"
              aria-label="下一个方案"
              onclick={() => stepVariant(1)}
            >
              →
            </button>
          </div>

          <div class="proto-scenes" role="tablist" aria-label="原型预览状态">
            {#each PROTOTYPE_SCENES as s (s.id)}
              <button
                class="proto-scene-btn"
                class:proto-scene-active={currentScene === s.id}
                type="button"
                role="tab"
                aria-selected={currentScene === s.id}
                title={s.label}
                onclick={() => onSelectScene(s.id)}
              >
                {s.shortLabel}
              </button>
            {/each}
            {#if hasLiveMatch}
              <button
                class="proto-scene-btn proto-scene-live"
                class:proto-scene-active={currentScene === "live"}
                type="button"
                role="tab"
                aria-selected={currentScene === "live"}
                onclick={() => onSelectScene("live")}
              >
                实时对局
              </button>
            {/if}
          </div>

          <div class="proto-util">
            <button
              class="proto-util-btn"
              type="button"
              title={dockTop ? "停靠到底部" : "停靠到顶部（避免遮挡手牌）"}
              onclick={() => (dockTop = !dockTop)}
            >
              {dockTop ? "置底" : "置顶"}
            </button>
            <button
              class="proto-util-btn"
              type="button"
              title="收起切换栏"
              onclick={() => (collapsed = true)}
            >
              收起
            </button>
          </div>
        </div>

        <div class="proto-row-meta">
          <span class="proto-metrics">{metricsSummary}</span>
          {#if lastActionNote}
            <span class="proto-action-note">{lastActionNote}</span>
          {:else if sceneMeta}
            <span class="proto-hint">{sceneMeta.hint}</span>
          {/if}
        </div>

        {#if showSpecPopover}
          <div class="proto-spec-box">
            <div><strong>材质：</strong>{meta.materialNote}</div>
            <div><strong>字刻：</strong>{meta.fontNote}</div>
            <div><strong>白板：</strong>{meta.whiteDragonNote}</div>
            <div><strong>布局：</strong>{meta.layoutNote}</div>
          </div>
        {/if}
      </div>
    {/if}
  </aside>
{/if}

<style>
  .proto-dock {
    position: fixed;
    left: 50%;
    bottom: max(10px, env(safe-area-inset-bottom));
    transform: translateX(-50%);
    z-index: 1200;
    max-width: calc(100vw - 16px);
    font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", sans-serif;
    pointer-events: none;
  }

  .proto-dock-top {
    bottom: auto;
    top: max(4px, env(safe-area-inset-top));
  }

  .proto-dock > :global(*) {
    pointer-events: auto;
  }

  .proto-collapsed-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px;
    border-radius: 999px;
    background: rgba(15, 23, 42, 0.92);
    border: 1.5px solid rgba(245, 190, 88, 0.78);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    color: #f8fafc;
  }

  .proto-expand-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: 0;
    color: #f8fafc;
    font-size: 12px;
    cursor: pointer;
    padding: 2px 6px;
  }

  .proto-tag {
    padding: 1px 6px;
    border-radius: 999px;
    background: rgba(245, 190, 88, 0.22);
    color: #fcd34d;
    font-size: 10px;
    font-weight: 800;
  }

  .proto-caret {
    font-size: 10px;
    color: #94a3b8;
  }

  .proto-bar {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 4px 9px;
    border-radius: 12px;
    background: rgba(12, 18, 28, 0.92);
    border: 1.5px solid rgba(245, 190, 88, 0.72);
    box-shadow:
      0 8px 24px rgba(0, 0, 0, 0.52),
      0 0 0 1px rgba(255, 255, 255, 0.08) inset;
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    color: #f8fafc;
  }

  .proto-row-main {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 6px 10px;
  }

  .proto-variant-ctrl {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .proto-arrow {
    width: 26px;
    height: 26px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.22);
    background: rgba(255, 255, 255, 0.08);
    color: #f8fafc;
    font-size: 13px;
    font-weight: 800;
    cursor: pointer;
    transition: background 0.12s ease, border-color 0.12s ease;
  }

  .proto-arrow:hover {
    background: rgba(245, 190, 88, 0.24);
    border-color: #f5be58;
    color: #fef3c7;
  }

  .proto-label-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 9px;
    border-radius: 8px;
    border: 1px solid rgba(245, 190, 88, 0.36);
    background: rgba(245, 190, 88, 0.12);
    color: #fef3c7;
    cursor: pointer;
    font-size: 12px;
    font-weight: 700;
    white-space: nowrap;
  }

  .proto-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 19px;
    height: 19px;
    border-radius: 5px;
    background: #f5be58;
    color: #111827;
    font-size: 11px;
    font-weight: 900;
  }

  .proto-name {
    font-size: 12px;
    font-weight: 800;
  }

  .proto-kbd {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 10px;
    padding: 1px 4px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.35);
    color: #cbd5e1;
  }

  .proto-scenes {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 3px;
  }

  .proto-scene-btn {
    padding: 3px 8px;
    border-radius: 999px;
    border: 1px solid rgba(255, 255, 255, 0.16);
    background: rgba(255, 255, 255, 0.05);
    color: #cbd5e1;
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
    white-space: nowrap;
    transition: all 0.12s ease;
  }

  .proto-scene-btn:hover {
    border-color: rgba(245, 190, 88, 0.55);
    color: #fff;
  }

  .proto-scene-active {
    background: #f5be58;
    border-color: #fde68a;
    color: #111827;
  }

  .proto-scene-live {
    border-color: rgba(52, 211, 153, 0.5);
    color: #a7f3d0;
  }

  .proto-scene-live.proto-scene-active {
    background: #10b981;
    border-color: #6ee7b7;
    color: #052e16;
  }

  .proto-util {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .proto-util-btn {
    padding: 3px 7px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.18);
    background: transparent;
    color: #94a3b8;
    font-size: 10.5px;
    font-weight: 700;
    cursor: pointer;
    white-space: nowrap;
  }

  .proto-util-btn:hover {
    color: #f8fafc;
    border-color: rgba(255, 255, 255, 0.4);
  }

  .proto-row-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 4px 12px;
    font-size: 10.5px;
    line-height: 1.25;
    border-top: 1px solid rgba(255, 255, 255, 0.08);
    padding-top: 4px;
  }

  .proto-metrics {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    color: #93c5fd;
    font-weight: 600;
  }

  .proto-hint {
    color: #cbd5e1;
  }

  .proto-action-note {
    color: #fcd34d;
    font-weight: 700;
  }

  .proto-spec-box {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 4px 14px;
    padding: 6px 8px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.05);
    font-size: 11px;
    color: #e2e8f0;
  }

  @media (max-height: 460px) {
    .proto-bar {
      padding: 4px 7px;
      gap: 3px;
      border-radius: 10px;
    }

    .proto-arrow {
      width: 22px;
      height: 22px;
      font-size: 11px;
    }

    .proto-label-btn {
      padding: 2px 6px;
      font-size: 11px;
    }

    .proto-kbd,
    .proto-hint {
      display: none;
    }

    .proto-scene-btn {
      padding: 2px 6px;
      font-size: 10px;
    }

    .proto-row-meta {
      padding-top: 2px;
      font-size: 9.5px;
    }

    .proto-spec-box {
      grid-template-columns: 1fr;
    }
  }
</style>
