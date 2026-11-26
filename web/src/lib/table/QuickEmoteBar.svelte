<script lang="ts">
  import type { QuickEmote } from "../protocol/generated/QuickEmote";
  import { QUICK_EMOTE_LIST } from "../protocol/personas";
  import { computeQuickEmoteDockRect } from "./banterLayout";
  import type { TableLayout } from "./layout";
  import {
    createQuickEmoteCooldown,
    formatCooldownSeconds,
    type QuickEmoteCooldownState,
  } from "./quickEmoteCooldown";

  interface Props {
    tableLayout: TableLayout | null;
    disabled: boolean;
    onSendQuickEmote: (emote: QuickEmote) => boolean;
  }

  let { tableLayout, disabled, onSendQuickEmote }: Props = $props();

  const cooldown = createQuickEmoteCooldown({
    onSend: (emote) => onSendQuickEmote(emote),
  });

  let cooldownState = $state<QuickEmoteCooldownState>(cooldown.getState());

  $effect(() => {
    const unsubscribe = cooldown.subscribe((next) => {
      cooldownState = next;
    });
    return () => {
      unsubscribe();
      cooldown.dispose();
    };
  });

  const isPortrait = $derived(
    tableLayout !== null && tableLayout.height > tableLayout.width,
  );

  const isCompact = $derived(
    tableLayout === null ||
      tableLayout.isShortLandscape ||
      isPortrait ||
      tableLayout.rightWing.width < 190,
  );

  const dockStyle = $derived.by(() => {
    if (!tableLayout) {
      return "";
    }
    const rect = computeQuickEmoteDockRect(tableLayout);
    const leftPct = (rect.x / tableLayout.width) * 100;
    const topPct = (rect.y / tableLayout.height) * 100;
    const widthPct = (rect.width / tableLayout.width) * 100;
    return `left: ${leftPct.toFixed(3)}%; top: ${topPct.toFixed(3)}%; width: ${widthPct.toFixed(3)}%;`;
  });

  const cooldownLabel = $derived(
    formatCooldownSeconds(cooldownState.remainingMs),
  );
  const buttonsDisabled = $derived(disabled || cooldownState.onCooldown);

  function handleClick(emote: QuickEmote): void {
    if (buttonsDisabled) {
      return;
    }
    cooldown.trigger(emote);
  }
</script>

{#if tableLayout}
  <div
    class="quick-emote-dock"
    class:quick-emote-compact={isCompact}
    class:quick-emote-portrait={isPortrait}
    class:quick-emote-cooling={cooldownState.onCooldown}
    style={dockStyle}
    role="group"
    aria-label="快捷互动"
  >
    {#if cooldownState.onCooldown}
      <div class="quick-emote-cooldown-badge" role="status" aria-live="polite">
        冷却 {cooldownLabel}
      </div>
    {/if}
    <div class="quick-emote-grid">
      {#each QUICK_EMOTE_LIST as item (item.id)}
        <button
          class="quick-emote-btn"
          class:quick-emote-btn-last={cooldownState.onCooldown &&
            cooldownState.lastEmote === item.id}
          type="button"
          style="--emote-accent: {item.accentHex};"
          disabled={buttonsDisabled}
          title={item.label}
          aria-label={`快捷互动：${item.label}`}
          onclick={() => handleClick(item.id)}
        >
          <span>{isCompact ? item.shortLabel : item.label}</span>
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .quick-emote-dock {
    position: absolute;
    z-index: 19;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 3px;
    pointer-events: auto;
  }

  .quick-emote-cooldown-badge {
    align-self: flex-end;
    padding: 1px 7px;
    border-radius: 999px;
    background: rgba(9, 20, 24, 0.86);
    border: 1px solid rgba(245, 190, 88, 0.65);
    color: #f5be58;
    font-size: 10px;
    font-weight: 800;
    line-height: 1.3;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }

  .quick-emote-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 4px;
    padding: 4px;
    border-radius: 10px;
    background: rgba(9, 20, 24, 0.72);
    border: 1px solid rgba(212, 168, 87, 0.34);
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.28);
  }

  .quick-emote-portrait .quick-emote-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 3px;
    padding: 3px;
  }

  .quick-emote-btn {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 0;
    padding: 4px 5px;
    border-radius: 7px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-left: 3px solid var(--emote-accent, #f5be58);
    background: rgba(24, 34, 40, 0.88);
    color: #f6efe2;
    font-size: 11px;
    font-weight: 700;
    line-height: 1.15;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    cursor: pointer;
    touch-action: manipulation;
    transition:
      transform 0.1s ease,
      border-color 0.12s ease,
      background-color 0.12s ease,
      opacity 0.12s ease;
  }

  .quick-emote-btn > span {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .quick-emote-btn:hover:not(:disabled) {
    border-color: var(--emote-accent, #f5be58);
    background: rgba(38, 52, 60, 0.96);
    color: #fff8ea;
  }

  .quick-emote-btn:active:not(:disabled) {
    transform: scale(0.96);
  }

  .quick-emote-btn:disabled {
    opacity: 0.48;
    cursor: not-allowed;
  }

  .quick-emote-btn-last:disabled {
    opacity: 0.78;
    border-color: var(--emote-accent, #f5be58);
    background: rgba(212, 168, 87, 0.2);
  }

  .quick-emote-compact .quick-emote-grid {
    gap: 2.5px;
    padding: 3px;
    border-radius: 8px;
  }

  .quick-emote-compact .quick-emote-btn {
    padding: 2.5px 4px;
    font-size: 10px;
    border-radius: 6px;
  }
</style>
