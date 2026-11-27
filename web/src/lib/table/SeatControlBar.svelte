<script lang="ts">
  import type { Seat } from "../protocol/generated/Seat";
  import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
  import {
    buildOfflinePauseBanner,
    buildSeatControlItems,
    shouldShowReclaimControl,
  } from "./seatControl";

  interface Props {
    snapshot: TableSnapshot;
    disabled?: boolean;
    floating?: boolean;
    onToggleAiAssist: (seat: Seat, enabled: boolean) => void;
  }

  let {
    snapshot,
    disabled = false,
    floating = false,
    onToggleAiAssist,
  }: Props = $props();

  const offlineBanner = $derived(buildOfflinePauseBanner(snapshot));
  const showReclaim = $derived(shouldShowReclaimControl(snapshot));
  const controlItems = $derived(buildSeatControlItems(snapshot));
  const hasVisibleControls = $derived(
    offlineBanner !== null || showReclaim || controlItems.length > 0,
  );
</script>

{#if hasVisibleControls}
  <section
    class="seat-control-stack"
    class:seat-control-floating={floating}
    aria-label="座次与托管控制"
  >
    {#if showReclaim && snapshot.your_seat !== null}
      {@const yourSeat = snapshot.your_seat}
      <div class="banner banner-warn reclaim-banner" role="alert">
        <div>
          <strong>当前本座正处于临时 AI 托管中</strong>
          <div>托管期间由 AI 自动代打；点击右侧按钮立即取消托管并收回控制权。</div>
        </div>
        <button
          class="btn btn-primary"
          type="button"
          {disabled}
          onclick={() => onToggleAiAssist(yourSeat, false)}
        >
          收回控制权
        </button>
      </div>
    {/if}

    {#if offlineBanner !== null}
      <div class="banner banner-warn offline-banner" role="status">
        <span>{offlineBanner}</span>
      </div>
    {/if}

    {#if controlItems.length > 0}
      <div class="seat-control-bar">
        {#each controlItems as item (item.seat)}
          <div class="seat-chip">
            <span class="seat-chip-name">{item.displayName}</span>
            <span class="tag tag-offline">离线</span>
            {#if item.aiAssisted}
              <span class="tag tag-self">AI 托管中</span>
            {/if}
            <button
              class="btn btn-chip"
              class:btn-jade={!item.aiAssisted}
              type="button"
              {disabled}
              onclick={() => onToggleAiAssist(item.seat, item.nextEnabled)}
            >
              {item.actionLabel}
            </button>
          </div>
        {/each}
      </div>
    {/if}
  </section>
{/if}

<style>
  .seat-control-stack {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .reclaim-banner,
  .offline-banner {
    padding: 8px 12px;
    font-size: 13px;
  }

  .seat-control-bar {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }

  .seat-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-radius: 10px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    font-size: 12px;
  }

  .seat-chip-name {
    font-weight: 600;
  }

  .btn-chip {
    padding: 4px 10px;
    font-size: 12px;
  }

  .seat-control-floating {
    align-items: center;
    gap: 4px;
  }

  .seat-control-floating .seat-control-bar {
    justify-content: center;
    gap: 6px;
  }

  .seat-control-floating .seat-chip {
    padding: 2px 8px;
    border-radius: 999px;
    background: rgba(9, 20, 24, 0.76);
    border: 1px solid rgba(212, 168, 87, 0.38);
    color: #f5eedf;
    font-size: 11px;
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
  }

  .seat-control-floating .btn-chip {
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 11px;
    border-color: rgba(255, 255, 255, 0.22);
    background: rgba(255, 255, 255, 0.08);
    color: #f5eedf;
  }

  .seat-control-floating .btn-chip.btn-jade {
    background: rgba(19, 116, 73, 0.85);
    border-color: rgba(147, 208, 203, 0.45);
    color: #fff;
  }

  .seat-control-floating .reclaim-banner,
  .seat-control-floating .offline-banner {
    padding: 5px 10px;
    border-radius: 10px;
    background: rgba(16, 32, 38, 0.9);
    border: 1px solid rgba(245, 190, 88, 0.7);
    color: #fef3c7;
    font-size: 11px;
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
  }
</style>
