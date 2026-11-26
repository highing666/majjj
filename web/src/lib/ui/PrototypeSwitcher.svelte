<script lang="ts">
  import { onMount } from "svelte";

  export interface PrototypeVariantOption {
    key: string;
    label: string;
  }

  interface Props {
    variants: readonly PrototypeVariantOption[];
    current: string;
    onSelect: (key: string) => void;
    compact?: boolean;
  }

  let { variants, current, onSelect, compact = false }: Props = $props();

  const isDev = import.meta.env.DEV;

  const currentIndex = $derived(
    Math.max(
      0,
      variants.findIndex((item) => item.key === current),
    ),
  );
  const currentItem = $derived(variants[currentIndex] ?? variants[0]);

  function step(delta: number): void {
    if (variants.length === 0) {
      return;
    }
    const nextIdx = (currentIndex + delta + variants.length) % variants.length;
    onSelect(variants[nextIdx].key);
  }

  function handleKeyDown(event: KeyboardEvent): void {
    if (!isDev || event.altKey || event.ctrlKey || event.metaKey) {
      return;
    }
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
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      step(-1);
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      step(1);
    }
  }

  onMount(() => {
    if (!isDev) {
      return;
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  });
</script>

{#if isDev && currentItem}
  <nav
    class="proto-switcher"
    class:proto-switcher-compact={compact}
    aria-label="原型方案切换器"
  >
    <button
      class="proto-arrow"
      type="button"
      title="上一方案 (←)"
      aria-label="上一方案"
      onclick={() => step(-1)}
    >
      ←
    </button>

    <div class="proto-label-wrap">
      <span class="proto-badge">PROTOTYPE</span>
      <span class="proto-label">
        {currentItem.key} ({currentItem.label})
      </span>
    </div>

    <button
      class="proto-arrow"
      type="button"
      title="下一方案 (→)"
      aria-label="下一方案"
      onclick={() => step(1)}
    >
      →
    </button>
  </nav>
{/if}

<style>
  .proto-switcher {
    position: fixed;
    left: 50%;
    bottom: max(10px, env(safe-area-inset-bottom));
    transform: translateX(-50%);
    z-index: 9999;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
    border-radius: 999px;
    background: rgba(15, 23, 42, 0.92);
    color: #f8fafc;
    border: 1px solid rgba(148, 163, 184, 0.42);
    box-shadow:
      0 10px 30px rgba(2, 6, 23, 0.45),
      0 0 0 1px rgba(255, 255, 255, 0.08) inset;
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    font-family:
      -apple-system, BlinkMacSystemFont, "PingFang SC", "Helvetica Neue",
      sans-serif;
    user-select: none;
  }

  .proto-switcher-compact {
    bottom: max(6px, env(safe-area-inset-bottom));
    padding: 3px 6px;
    gap: 6px;
  }

  .proto-arrow {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 999px;
    border: 1px solid rgba(148, 163, 184, 0.35);
    background: rgba(30, 41, 59, 0.95);
    color: #f8fafc;
    font-size: 14px;
    font-weight: 700;
    line-height: 1;
    transition:
      background-color 0.14s ease,
      transform 0.08s ease;
  }

  .proto-arrow:hover {
    background: #334155;
  }

  .proto-arrow:active {
    transform: scale(0.94);
  }

  .proto-label-wrap {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 0 4px;
    white-space: nowrap;
  }

  .proto-badge {
    font-size: 10px;
    font-weight: 800;
    letter-spacing: 0.06em;
    padding: 2px 6px;
    border-radius: 999px;
    background: #38bdf8;
    color: #082f49;
  }

  .proto-label {
    font-size: 12.5px;
    font-weight: 600;
    color: #f1f5f9;
  }

  @media (max-height: 440px) {
    .proto-switcher {
      padding: 3px 6px;
      gap: 6px;
    }

    .proto-arrow {
      width: 24px;
      height: 24px;
      font-size: 12px;
    }

    .proto-badge {
      display: none;
    }

    .proto-label {
      font-size: 11.5px;
    }
  }
</style>
