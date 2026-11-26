<script lang="ts">
  import type {
    ActiveBanterBubble,
    ActiveBanterMap,
  } from "../presentation/presentationQueue";
  import type { PlayerView } from "../protocol/generated/PlayerView";
  import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
  import { getPersonaMeta, getPersonaMoodMeta } from "../protocol/personas";
  import { ALL_SEATS, seatIndex, seatRelativeSlot } from "../protocol/seats";
  import {
    computeBanterBubbleAnchor,
    type BanterBubbleAnchor,
  } from "./banterLayout";
  import type { TableLayout } from "./layout";

  interface Props {
    snapshot: TableSnapshot;
    playerView: PlayerView;
    tableLayout: TableLayout | null;
    activeBubbles: ActiveBanterMap;
    reducedEffects?: boolean;
  }

  let {
    snapshot,
    playerView,
    tableLayout,
    activeBubbles,
    reducedEffects = false,
  }: Props = $props();

  interface RenderedBubble {
    bubble: ActiveBanterBubble;
    anchor: BanterBubbleAnchor;
    speakerName: string;
    accentHex: string;
    moodShortLabel: string;
    moodBadgeBg: string;
    moodBadgeFg: string;
  }

  const renderedBubbles = $derived.by<RenderedBubble[]>(() => {
    if (!tableLayout) {
      return [];
    }
    const items: RenderedBubble[] = [];
    for (const seat of ALL_SEATS) {
      const bubble = activeBubbles[seat];
      if (!bubble) {
        continue;
      }
      const seatInfo = snapshot.seats[seatIndex(seat)];
      const personaMeta = getPersonaMeta(bubble.personaId);
      const moodMeta = getPersonaMoodMeta(bubble.mood);
      const slot = seatRelativeSlot(playerView.viewer, seat);
      const anchor = computeBanterBubbleAnchor(tableLayout, slot);
      items.push({
        bubble,
        anchor,
        speakerName: seatInfo.nickname.trim() || personaMeta.name,
        accentHex: personaMeta.accentHex,
        moodShortLabel: moodMeta.shortLabel,
        moodBadgeBg: moodMeta.badgeBg,
        moodBadgeFg: moodMeta.badgeFg,
      });
    }
    return items;
  });

  function anchorStyle(layout: TableLayout, anchor: BanterBubbleAnchor): string {
    const leftPct = (anchor.leftPx / layout.width) * 100;
    const topPct = (anchor.topPx / layout.height) * 100;
    const widthPct = (anchor.maxWidthPx / layout.width) * 100;
    return `left: ${leftPct.toFixed(3)}%; top: ${topPct.toFixed(3)}%; width: ${widthPct.toFixed(3)}%; max-height: ${anchor.maxHeightPx}px;`;
  }
</script>

{#if tableLayout && renderedBubbles.length > 0}
  <div class="banter-bubbles-layer" aria-label="牌桌台词气泡" aria-live="polite">
    {#each renderedBubbles as item (`${item.bubble.seat}-${item.bubble.eventSeq}-${item.bubble.text}`)}
      <div
        class="banter-anchor"
        class:banter-anchor-right={item.anchor.alignRight}
        class:banter-anchor-bottom={item.anchor.alignBottom}
        style="{anchorStyle(tableLayout, item.anchor)} --banter-accent: {item.accentHex};"
        data-seat={item.bubble.seat}
        data-mood={item.bubble.mood}
      >
        <div
          class="comic-bubble tail-{item.anchor.tail}"
          class:comic-bubble-static={reducedEffects}
          role="status"
        >
          <div class="comic-bubble-head">
            <span class="comic-speaker">{item.speakerName}</span>
            <span
              class="comic-mood"
              style="background: {item.moodBadgeBg}; color: {item.moodBadgeFg};"
            >
              {item.moodShortLabel}
            </span>
          </div>
          <p class="comic-text">{item.bubble.text}</p>
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .banter-bubbles-layer {
    position: absolute;
    inset: 0;
    z-index: 16;
    pointer-events: none;
    overflow: hidden;
  }

  .banter-anchor {
    position: absolute;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    pointer-events: none;
  }

  .banter-anchor-right {
    align-items: flex-end;
  }

  .banter-anchor-bottom {
    justify-content: flex-end;
  }

  .comic-bubble {
    position: relative;
    max-width: 100%;
    padding: 5px 8px 6px;
    border-radius: 10px;
    background: #fff8ea;
    border: 2px solid #181a20;
    border-left: 4px solid var(--banter-accent, #e85d3f);
    color: #181a20;
    box-shadow:
      2px 3px 0 #181a20,
      0 6px 14px rgba(0, 0, 0, 0.36);
    animation: banterPop 180ms cubic-bezier(0.22, 1.28, 0.36, 1);
  }

  .comic-bubble-static {
    animation: none;
  }

  .comic-bubble::before,
  .comic-bubble::after {
    content: "";
    position: absolute;
    width: 0;
    height: 0;
    border-style: solid;
  }

  .tail-top-left::before {
    top: -9px;
    left: 12px;
    border-width: 0 7px 9px 7px;
    border-color: transparent transparent #181a20 transparent;
  }

  .tail-top-left::after {
    top: -6px;
    left: 14px;
    border-width: 0 5px 7px 5px;
    border-color: transparent transparent #fff8ea transparent;
  }

  .tail-top-right::before {
    top: -9px;
    right: 12px;
    border-width: 0 7px 9px 7px;
    border-color: transparent transparent #181a20 transparent;
  }

  .tail-top-right::after {
    top: -6px;
    right: 14px;
    border-width: 0 5px 7px 5px;
    border-color: transparent transparent #fff8ea transparent;
  }

  .tail-bottom-left::before {
    bottom: -9px;
    left: 12px;
    border-width: 9px 7px 0 7px;
    border-color: #181a20 transparent transparent transparent;
  }

  .tail-bottom-left::after {
    bottom: -6px;
    left: 14px;
    border-width: 7px 5px 0 5px;
    border-color: #fff8ea transparent transparent transparent;
  }

  .comic-bubble-head {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-bottom: 2px;
    line-height: 1.1;
  }

  .comic-speaker {
    font-size: 10px;
    font-weight: 800;
    color: #3b2f23;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .comic-mood {
    display: inline-flex;
    align-items: center;
    padding: 0 5px;
    border-radius: 999px;
    font-size: 9.5px;
    font-weight: 800;
    line-height: 1.35;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .comic-text {
    margin: 0;
    font-size: 12px;
    font-weight: 800;
    line-height: 1.28;
    letter-spacing: 0.01em;
    word-break: break-word;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  @keyframes banterPop {
    0% {
      opacity: 0;
      transform: translateY(-4px) scale(0.92);
    }
    100% {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  @media (max-height: 460px) {
    .comic-bubble {
      padding: 3px 6px 4px;
      border-radius: 8px;
    }

    .comic-text {
      font-size: 11px;
      line-height: 1.22;
    }
  }
</style>
