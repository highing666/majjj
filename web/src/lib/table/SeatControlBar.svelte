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
    onToggleAiAssist: (seat: Seat, enabled: boolean) => void;
  }

  let { snapshot, disabled = false, onToggleAiAssist }: Props = $props();

  const offlineBanner = $derived(buildOfflinePauseBanner(snapshot));
  const showReclaim = $derived(shouldShowReclaimControl(snapshot));
  const controlItems = $derived(buildSeatControlItems(snapshot));
  const hasVisibleControls = $derived(
    offlineBanner !== null || showReclaim || controlItems.length > 0,
  );
</script>

{#if hasVisibleControls}
  <section class="seat-control-stack" aria-label="座次与托管控制">
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
          {#if !(showReclaim && item.isSelf)}
            <div class="seat-chip">
              <span class="seat-chip-name">{item.displayName}</span>
              {#if item.isOffline}
                <span class="tag tag-offline">离线</span>
              {/if}
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
          {/if}
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
</style>
