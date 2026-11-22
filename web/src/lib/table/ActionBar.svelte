<script lang="ts">
  import type { PlayerAction } from "../protocol/generated/PlayerAction";
  import type { PlayerView } from "../protocol/generated/PlayerView";
  import TileBadge from "./TileBadge.svelte";
  import { buildActionBarItems, type ActionBarItem } from "./viewModel";

  interface Props {
    playerView: PlayerView;
    disabled?: boolean;
    hasModalOpen?: boolean;
    onAction: (action: PlayerAction) => void;
  }

  let {
    playerView,
    disabled = false,
    hasModalOpen = false,
    onAction,
  }: Props = $props();

  const items = $derived(buildActionBarItems(playerView));

  function handleKeyDown(event: KeyboardEvent): void {
    if (disabled || hasModalOpen || items.length === 0) {
      return;
    }
    const target = event.target as HTMLElement | null;
    if (
      target &&
      (target.tagName === "INPUT" ||
        target.tagName === "SELECT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable)
    ) {
      return;
    }

    const key = event.key.toLowerCase();
    let candidates: ActionBarItem[] = [];
    if (key === "h") {
      candidates = items.filter((i) => i.tone === "hu");
    } else if (key === "g") {
      candidates = items.filter((i) => i.tone === "kong");
    } else if (key === "p") {
      candidates = items.filter((i) => i.tone === "pon");
    } else if (event.key === "Escape") {
      candidates = items.filter((i) => i.tone === "pass");
    }

    if (candidates.length === 1) {
      event.preventDefault();
      onAction(candidates[0].action);
    }
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if items.length > 0}
  <div class="action-bar" role="toolbar" aria-label="可执行操作">
    {#each items as item (item.key)}
      <button
        class="action-btn tone-{item.tone}"
        type="button"
        {disabled}
        onclick={() => onAction(item.action)}
      >
        {#if item.tile !== null}
          <TileBadge tile={item.tile} size="xs" />
        {/if}
        <span class="action-label">{item.label}</span>
        {#if item.tileLabel}
          <span class="action-tile-name">{item.tileLabel}</span>
        {/if}
        <kbd class="action-kbd">{item.shortcut}</kbd>
      </button>
    {/each}
  </div>
{/if}

<style>
  .action-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 10px 14px;
    border-radius: 14px;
    background: rgba(15, 32, 38, 0.94);
    border: 1.5px solid #d4a857;
    box-shadow: 0 10px 26px rgba(10, 20, 24, 0.35);
  }

  .action-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border-radius: 10px;
    border: 1.5px solid #181a20;
    font-weight: 700;
    font-size: 15px;
    color: #fff;
    transition:
      transform 0.08s ease,
      filter 0.12s ease;
  }

  .action-btn:hover:not(:disabled) {
    filter: brightness(1.08);
    transform: translateY(-1px);
  }

  .action-btn:active:not(:disabled) {
    transform: translateY(1px);
  }

  .tone-hu {
    background: linear-gradient(180deg, #cb4028 0%, #9b2916 100%);
    border-color: #f5be58;
  }

  .tone-kong {
    background: linear-gradient(180deg, #2b6f72 0%, #1c4e50 100%);
    border-color: #8ccac8;
  }

  .tone-pon {
    background: linear-gradient(180deg, #b87826 0%, #8d5814 100%);
    border-color: #f2cb79;
  }

  .tone-pass {
    background: #33484f;
    border-color: #5b747d;
    color: #e4ecef;
  }

  .action-tile-name {
    font-size: 12px;
    opacity: 0.9;
  }

  .action-kbd {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
    padding: 1px 5px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.28);
    color: rgba(255, 255, 255, 0.85);
  }

  @media (max-width: 640px) {
    .action-kbd {
      display: none;
    }

    .action-btn {
      padding: 8px 12px;
      font-size: 14px;
    }
  }
</style>
