<script lang="ts">
  import type { ActiveBanterVoiceState } from "../audio/audioTypes";
  import PersonaAvatar from "../personas/PersonaAvatar.svelte";
  import type { PlayerView } from "../protocol/generated/PlayerView";
  import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
  import { rectToPercentStyle } from "./banterLayout";
  import { isSeatActivelySpeaking } from "./banterVisibility";
  import type { Rect, TableLayout } from "./layout";
  import {
    buildTablePersonaPocketModels,
    type TablePersonaPocketModel,
  } from "./viewModel";

  interface Props {
    snapshot: TableSnapshot;
    playerView: PlayerView;
    tableLayout: TableLayout | null;
    activeBanterVoice?: ActiveBanterVoiceState | null;
    reducedEffects?: boolean;
  }

  let {
    snapshot,
    playerView,
    tableLayout,
    activeBanterVoice = null,
    reducedEffects = false,
  }: Props = $props();

  const pocketModels = $derived(
    buildTablePersonaPocketModels(snapshot, playerView),
  );

  function rectStyle(layout: TableLayout, rect: Rect): string {
    return rectToPercentStyle(layout, rect, true);
  }

  function avatarSizeFor(rect: Rect): number {
    return Math.max(16, Math.min(34, Math.floor(rect.height - 4)));
  }

  function pocketItems(
    layout: TableLayout,
  ): Array<{ model: TablePersonaPocketModel; rect: Rect }> {
    const out: Array<{ model: TablePersonaPocketModel; rect: Rect }> = [];
    for (const slot of ["top", "left", "right"] as const) {
      const model = pocketModels[slot];
      if (model !== null) {
        out.push({ model, rect: layout.personaPockets[slot] });
      }
    }
    return out;
  }
</script>

{#if tableLayout}
  <div class="persona-pockets-layer" aria-label="牌友头像区">
    {#each pocketItems(tableLayout) as item (item.model.slot)}
      {@const avatarPx = avatarSizeFor(item.rect)}
      {@const showMoodChip = item.rect.width >= 66}
      {@const showMetaLine = item.rect.height >= 30 && item.rect.width >= 112}
      {@const isSpeaking = isSeatActivelySpeaking(
        item.model.seat,
        activeBanterVoice,
      )}
      <div
        class="persona-pocket persona-pocket-{item.model.slot}"
        class:persona-pocket-active={item.model.isActive}
        class:persona-pocket-speaking={isSpeaking}
        class:persona-pocket-speaking-static={isSpeaking && reducedEffects}
        style="{rectStyle(tableLayout, item.rect)} --persona-accent: {item.model.accentHex};"
        data-seat={item.model.seat}
        data-persona={item.model.personaId}
        data-mood={item.model.mood}
        data-speaking={isSpeaking ? "true" : undefined}
      >
        <div class="persona-avatar-wrap">
          <PersonaAvatar
            personaId={item.model.personaId}
            mood={item.model.mood}
            size={avatarPx}
          />
          {#if isSpeaking}
            <span
              class="speaking-pulse-dot"
              class:speaking-pulse-dot-static={reducedEffects}
              aria-hidden="true"
            ></span>
          {/if}
        </div>
        {#if showMoodChip}
          <div class="persona-pocket-info">
            <span
              class="persona-mood-pill"
              style="background: {item.model.badgeBg}; color: {item.model.badgeFg};"
            >
              {item.model.moodShortLabel}
            </span>
            {#if showMetaLine}
              <span class="persona-style-tag">{item.model.styleLabel}</span>
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .persona-pockets-layer {
    position: absolute;
    inset: 0;
    z-index: 12;
    pointer-events: none;
    overflow: hidden;
  }

  .persona-pocket {
    position: absolute;
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 1px 4px;
    border-radius: 9px;
    background: rgba(11, 24, 29, 0.78);
    border: 1px solid rgba(212, 168, 87, 0.34);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
    overflow: hidden;
    box-sizing: border-box;
  }

  .persona-pocket-active {
    border-color: var(--persona-accent, #f5be58);
    box-shadow: 0 0 10px rgba(245, 190, 88, 0.38);
  }

  .persona-pocket-speaking {
    border-color: var(--persona-accent, #f5be58);
    box-shadow:
      0 0 0 1.5px var(--persona-accent, #f5be58),
      0 0 14px rgba(245, 190, 88, 0.56);
    animation: pocketVoicePulse 1.15s ease-in-out infinite;
  }

  .persona-pocket-speaking-static {
    animation: none;
    box-shadow:
      0 0 0 2px var(--persona-accent, #f5be58),
      0 0 10px rgba(245, 190, 88, 0.42);
  }

  .persona-avatar-wrap {
    position: relative;
    display: inline-flex;
    align-items: center;
    flex: 0 0 auto;
  }

  .speaking-pulse-dot {
    position: absolute;
    right: -2px;
    bottom: -2px;
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--persona-accent, #f5be58);
    border: 1.5px solid #0b181d;
    box-shadow: 0 0 6px var(--persona-accent, #f5be58);
    animation: voiceDotPulse 0.9s ease-in-out infinite;
  }

  .speaking-pulse-dot-static {
    animation: none;
  }

  @keyframes pocketVoicePulse {
    0%,
    100% {
      box-shadow:
        0 0 0 1.5px var(--persona-accent, #f5be58),
        0 0 9px rgba(245, 190, 88, 0.38);
    }
    50% {
      box-shadow:
        0 0 0 2.5px var(--persona-accent, #f5be58),
        0 0 18px rgba(245, 190, 88, 0.72);
    }
  }

  @keyframes voiceDotPulse {
    0%,
    100% {
      transform: scale(0.92);
      opacity: 0.85;
    }
    50% {
      transform: scale(1.22);
      opacity: 1;
    }
  }

  .persona-pocket-right {
    flex-direction: row-reverse;
  }

  .persona-pocket-info {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    overflow: hidden;
  }

  .persona-mood-pill {
    display: inline-flex;
    align-items: center;
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 800;
    line-height: 1.25;
    white-space: nowrap;
  }

  .persona-style-tag {
    font-size: 10px;
    font-weight: 600;
    color: #d6dfd8;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
