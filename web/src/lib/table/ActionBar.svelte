<script lang="ts">
  import type { PlayerAction } from "../protocol/generated/PlayerAction";
  import type { PlayerView } from "../protocol/generated/PlayerView";
  import TileBadge from "./TileBadge.svelte";
  import { buildActionBarItems, resolveActionBarShortcut } from "./viewModel";

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
    const target = event.target as HTMLElement | null;
    const isInputFocused = Boolean(
      target &&
        (target.tagName === "INPUT" ||
          target.tagName === "SELECT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable),
    );
    const matched = resolveActionBarShortcut(items, event.key, {
      disabled,
      hasModalOpen,
      isInputFocused,
    });
    if (matched !== null) {
      event.preventDefault();
      onAction(matched);
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
    padding: 8px 12px;
    border-radius: 14px;
    background: rgba(13, 24, 29, 0.94);
    border: 2px solid #d4a857;
    box-shadow:
      0 4px 0 #181a20,
      0 12px 28px rgba(8, 12, 20, 0.55);
  }

  .action-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 13px;
    border-radius: 9px;
    border: 2px solid #181a20;
    font-weight: 800;
    font-size: 15px;
    color: #fffaf0;
    box-shadow: 0 3px 0 #181a20;
    transition:
      transform 0.08s ease,
      filter 0.12s ease;
  }

  .action-btn:hover:not(:disabled) {
    filter: brightness(1.08);
    transform: translateY(-2px);
  }

  .action-btn:active:not(:disabled) {
    transform: translateY(1px);
  }

  .tone-hu {
    background: linear-gradient(180deg, #d83d2e 0%, #9f2518 100%);
    border-color: #f5be58;
  }

  .tone-kong {
    background: linear-gradient(180deg, #21857d 0%, #155753 100%);
    border-color: #93d0cb;
  }

  .tone-pon {
    background: linear-gradient(180deg, #d48924 0%, #995d12 100%);
    border-color: #f3cc7b;
  }

  .tone-pass {
    background: #2c3e45;
    border-color: #5b747d;
    color: #e4ecef;
  }

  .action-tile-name {
    font-size: 12px;
    opacity: 0.92;
  }

  .action-kbd {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 11px;
    padding: 1px 5px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.34);
    border: 1px solid rgba(255, 255, 255, 0.22);
    color: rgba(255, 250, 240, 0.9);
  }

  @media (max-width: 640px) {
    .action-kbd {
      display: none;
    }

    .action-bar {
      padding: 6px 10px;
      gap: 6px;
    }

    .action-btn {
      padding: 6px 10px;
      font-size: 14px;
    }
  }
</style>
