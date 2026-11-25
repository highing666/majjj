<script lang="ts">
  import type { Seat } from "../protocol/generated/Seat";
  import type { SeatInfo } from "../protocol/generated/SeatInfo";
  import {
    formatSeatDisplayName,
    seatRelativeSlot,
    type RelativeSlot,
  } from "../table/viewModel";
  import type { PiltoverImpactFrame } from "./effectsBus";

  interface Props {
    impact: PiltoverImpactFrame | null;
    viewerSeat?: Seat | null;
    seats?: readonly [SeatInfo, SeatInfo, SeatInfo, SeatInfo] | null;
    onSkip?: () => void;
  }

  let {
    impact,
    viewerSeat = null,
    seats = null,
    onSkip,
  }: Props = $props();

  interface SpeedlineRay {
    x2: string;
    y2: string;
    width: number;
  }

  const SPEEDLINE_RAYS: SpeedlineRay[] = Array.from({ length: 28 }, (_, i) => {
    const angleRad = ((i * 360) / 28) * (Math.PI / 180);
    const len = i % 2 === 0 ? 44 : 32;
    const radius = 120 + len;
    return {
      x2: (200 + Math.cos(angleRad) * radius).toFixed(1),
      y2: (200 + Math.sin(angleRad) * radius).toFixed(1),
      width: i % 3 === 0 ? 4.2 : 2.1,
    };
  });

  interface SeatHighlightItem {
    seat: Seat;
    slot: RelativeSlot;
    title: string;
  }

  const winnerHighlights = $derived.by<SeatHighlightItem[]>(() => {
    if (!impact || impact.seats.length === 0) {
      return [];
    }
    const baseSeat = viewerSeat ?? "East";
    return impact.seats.map((seat) => ({
      seat,
      slot: seatRelativeSlot(baseSeat, seat),
      title: formatSeatDisplayName(seat, seats),
    }));
  });

  function handleKeyDown(event: KeyboardEvent): void {
    if (!impact?.skippable) {
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onSkip?.();
    }
  }
</script>

{#snippet impactContent(active: PiltoverImpactFrame)}
  {#if active.warmFlash}
    <div class="impact-flash-warm" aria-hidden="true"></div>
  {/if}

  {#each winnerHighlights as item (item.seat)}
    <div class="seat-highlight-badge slot-{item.slot}">
      <span class="seat-highlight-dot" aria-hidden="true"></span>
      <span>{item.title}</span>
    </div>
  {/each}

  <div class="impact-burst-center">
    {#if active.speedlines}
      <svg
        class="impact-speedlines"
        viewBox="0 0 400 400"
        xmlns="http://www.w3.org/2000/svg"
        aria-hidden="true"
      >
        {#each SPEEDLINE_RAYS as ray, idx (idx)}
          <line
            x1="200"
            y1="200"
            x2={ray.x2}
            y2={ray.y2}
            stroke={active.accentColor}
            stroke-width={ray.width}
            stroke-dasharray="48 160"
            stroke-dashoffset="-48"
            stroke-linecap="round"
          />
        {/each}
      </svg>
    {/if}

    <div class="impact-stamp-piltover">
      <span class="impact-stamp-main">{active.label}</span>
      {#if active.sublabel}
        <span class="impact-stamp-sub">{active.sublabel}</span>
      {/if}
    </div>
  </div>
{/snippet}

{#if impact}
  {#key impact.id}
    {#if impact.skippable}
      <div
        class="comic-impact-layer is-skippable impact-kind-{impact.kind}"
        style:--impact-accent={impact.accentColor}
        style:--impact-skew={`${impact.skewDeg}deg`}
        role="button"
        tabindex="0"
        aria-label="点按跳过过场查看结算"
        onclick={() => onSkip?.()}
        onkeydown={handleKeyDown}
      >
        {@render impactContent(impact)}
        <div class="skip-hint">点按画面立即查看结算</div>
      </div>
    {:else}
      <div
        class="comic-impact-layer impact-kind-{impact.kind}"
        style:--impact-accent={impact.accentColor}
        style:--impact-skew={`${impact.skewDeg}deg`}
        aria-live="polite"
      >
        {@render impactContent(impact)}
      </div>
    {/if}
  {/key}
{/if}

<style>
  .comic-impact-layer {
    position: absolute;
    inset: 0;
    z-index: 30;
    pointer-events: none;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    border-radius: 16px;
  }

  .comic-impact-layer.is-skippable {
    pointer-events: auto;
    cursor: pointer;
  }

  .impact-flash-warm {
    position: absolute;
    inset: 0;
    background: rgba(255, 226, 158, 0.28);
    animation: piltoverWarmFlash 140ms steps(2, end) forwards;
    pointer-events: none;
  }

  .impact-burst-center {
    position: relative;
    width: min(340px, 78vw);
    height: min(340px, 78vw);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .impact-speedlines {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    animation: piltoverSpeedSpin 700ms steps(8, end) forwards;
  }

  .impact-stamp-piltover {
    position: relative;
    padding: 14px 30px;
    background: #141920;
    border: 4px solid var(--impact-accent, #f3a93b);
    box-shadow: 6px 7px 0 #090c10;
    transform: rotate(var(--impact-skew, -7deg));
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    animation: piltoverComicPop 220ms steps(4, end) forwards;
  }

  .impact-stamp-main {
    font-size: clamp(32px, 5vw, 48px);
    font-weight: 900;
    color: #ffebc2;
    letter-spacing: 0.04em;
    line-height: 1.08;
    text-shadow: 3px 3px 0 #090c10;
  }

  .impact-stamp-sub {
    font-size: 14px;
    font-weight: 800;
    color: var(--impact-accent, #f3a93b);
  }

  .seat-highlight-badge {
    position: absolute;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border-radius: 999px;
    background: rgba(20, 25, 32, 0.94);
    border: 2px solid var(--impact-accent, #f3a93b);
    box-shadow:
      0 0 18px rgba(243, 169, 59, 0.55),
      0 3px 0 #090c10;
    color: #ffebc2;
    font-size: 13px;
    font-weight: 800;
    animation: piltoverHighlightPulse 360ms ease-out forwards;
  }

  .seat-highlight-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--impact-accent, #f3a93b);
  }

  .slot-top {
    top: 14px;
    left: 16px;
  }

  .slot-right {
    top: 14px;
    right: 16px;
  }

  .slot-left {
    bottom: 20%;
    left: 16px;
  }

  .slot-bottom {
    bottom: 20%;
    right: 16px;
  }

  .skip-hint {
    position: absolute;
    bottom: 14px;
    left: 50%;
    transform: translateX(-50%);
    padding: 4px 12px;
    border-radius: 999px;
    background: rgba(16, 32, 38, 0.88);
    border: 1px solid rgba(243, 169, 59, 0.55);
    color: #f5eedf;
    font-size: 12px;
    font-weight: 600;
  }

  @keyframes piltoverWarmFlash {
    0% {
      opacity: 1;
    }
    100% {
      opacity: 0;
    }
  }

  @keyframes piltoverSpeedSpin {
    0% {
      transform: scale(0.55) rotate(0deg);
      opacity: 0.95;
    }
    100% {
      transform: scale(1.22) rotate(18deg);
      opacity: 0.2;
    }
  }

  @keyframes piltoverComicPop {
    0% {
      transform: scale(1.68) rotate(-10deg);
      opacity: 0;
    }
    100% {
      transform: scale(1) rotate(var(--impact-skew, -7deg));
      opacity: 1;
    }
  }

  @keyframes piltoverHighlightPulse {
    0% {
      transform: scale(0.86);
      opacity: 0;
    }
    100% {
      transform: scale(1);
      opacity: 1;
    }
  }
</style>
