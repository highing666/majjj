<script lang="ts">
  import type { ActiveBanterVoiceState } from "../audio/audioTypes";
  import PersonaAvatar from "../personas/PersonaAvatar.svelte";
  import type {
    ActiveBanterBubble,
    ActiveBanterMap,
  } from "../presentation/presentationQueue";
  import type { Seat } from "../protocol/generated/Seat";
  import type { SeatInfo } from "../protocol/generated/SeatInfo";
  import {
    getPersonaMeta,
    getPersonaMoodMeta,
    getQuickEmoteMeta,
  } from "../protocol/personas";
  import {
    ALL_SEATS,
    formatRelativeSeatDisplay,
    seatIndex,
  } from "../protocol/seats";
  import {
    isBubbleActivelySpeaking,
    shouldShowBubbleText,
  } from "./banterVisibility";

  interface Props {
    seats: [SeatInfo, SeatInfo, SeatInfo, SeatInfo];
    yourSeat: Seat | null;
    activeBubbles: ActiveBanterMap;
    activeBanterVoice?: ActiveBanterVoiceState | null;
    hideAiBanterText?: boolean;
    reducedEffects?: boolean;
  }

  let {
    seats,
    yourSeat,
    activeBubbles,
    activeBanterVoice = null,
    hideAiBanterText = false,
    reducedEffects = false,
  }: Props = $props();

  interface StripItem {
    bubble: ActiveBanterBubble;
    speakerName: string;
    replyTargetName: string | null;
    accentHex: string;
    moodShortLabel: string;
    moodBadgeBg: string;
    moodBadgeFg: string;
    speaking: boolean;
    hideText: boolean;
  }

  const items = $derived.by<StripItem[]>(() => {
    const active: ActiveBanterBubble[] = [];
    for (const seat of ALL_SEATS) {
      const bubble = activeBubbles[seat];
      if (bubble !== null) {
        active.push(bubble);
      }
    }
    active.sort((a, b) => a.bubbleId - b.bubbleId);

    return active.map((bubble) => {
      const seatInfo = seats[seatIndex(bubble.seat)];
      const replyTargetName =
        bubble.replyToSeat !== null
          ? formatRelativeSeatDisplay(bubble.replyToSeat, yourSeat, seats)
          : null;
      const speaking = isBubbleActivelySpeaking(bubble, activeBanterVoice);
      const hideText = !shouldShowBubbleText(bubble, hideAiBanterText);
      if (
        bubble.kind === "banter" &&
        bubble.personaId !== null &&
        bubble.mood !== null
      ) {
        const personaMeta = getPersonaMeta(bubble.personaId);
        const moodMeta = getPersonaMoodMeta(bubble.mood);
        return {
          bubble,
          speakerName: seatInfo.nickname.trim() || personaMeta.name,
          replyTargetName,
          accentHex: personaMeta.accentHex,
          moodShortLabel: moodMeta.shortLabel,
          moodBadgeBg: moodMeta.badgeBg,
          moodBadgeFg: moodMeta.badgeFg,
          speaking,
          hideText,
        };
      }
      const emoteMeta = bubble.emote ? getQuickEmoteMeta(bubble.emote) : null;
      return {
        bubble,
        speakerName: seatInfo.nickname.trim() || "玩家",
        replyTargetName,
        accentHex: emoteMeta?.accentHex ?? "#F5BE58",
        moodShortLabel: "快捷互动",
        moodBadgeBg: emoteMeta?.accentHex ?? "#F5BE58",
        moodBadgeFg: "#181A20",
        speaking: false,
        hideText: false,
      };
    });
  });
</script>

{#if items.length > 0}
  <section
    class="post-hand-banter-strip"
    aria-label="复盘牌桌台词"
    aria-live="polite"
  >
    {#each items as item (item.bubble.bubbleId)}
      <article
        class="banter-card"
        class:banter-card-static={reducedEffects}
        class:banter-card-compact={item.hideText}
        class:banter-card-speaking={item.speaking}
        class:banter-card-speaking-static={item.speaking && reducedEffects}
        style="--banter-accent: {item.accentHex};"
        data-seat={item.bubble.seat}
        data-mood={item.bubble.mood ?? "QuickEmote"}
        data-speaking={item.speaking ? "true" : undefined}
        role="status"
      >
        <div class="banter-avatar-wrap">
          {#if item.bubble.personaId !== null}
            <PersonaAvatar
              personaId={item.bubble.personaId}
              mood={item.bubble.mood}
              size={36}
            />
          {:else}
            <span class="human-avatar-dot" aria-hidden="true">言</span>
          {/if}
          {#if item.speaking}
            <span
              class="banter-speaking-dot"
              class:banter-speaking-dot-static={reducedEffects}
              aria-label="正在说话"
            ></span>
          {/if}
        </div>
        <div class="banter-card-body">
          <div class="banter-card-head">
            <strong class="banter-speaker">{item.speakerName}</strong>
            <span
              class="banter-mood-pill"
              style="background: {item.moodBadgeBg}; color: {item.moodBadgeFg};"
            >
              {item.moodShortLabel}
            </span>
            {#if item.replyTargetName !== null}
              <span class="banter-reply-tag">↩ {item.replyTargetName}</span>
            {/if}
          </div>
          {#if !item.hideText}
            <p class="banter-line">{item.bubble.text}</p>
          {/if}
        </div>
      </article>
    {/each}
  </section>
{/if}

<style>
  .post-hand-banter-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    pointer-events: none;
  }

  .banter-card {
    flex: 1 1 210px;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: 12px;
    background: #fff8ea;
    border: 2px solid #181a20;
    border-left: 5px solid var(--banter-accent, #e85d3f);
    color: #181a20;
    box-shadow: 2px 3px 0 #181a20;
    animation: stripPop 180ms cubic-bezier(0.22, 1.28, 0.36, 1);
  }

  .banter-card-static {
    animation: none;
  }

  .banter-card-compact {
    flex: 0 1 auto;
    padding: 5px 10px;
  }

  .banter-card-speaking {
    box-shadow:
      2px 3px 0 #181a20,
      0 0 0 2px var(--banter-accent, #e85d3f),
      0 4px 14px rgba(245, 190, 88, 0.36);
    animation:
      stripPop 180ms cubic-bezier(0.22, 1.28, 0.36, 1),
      stripSpeakingPulse 1.15s ease-in-out 180ms infinite;
  }

  .banter-card-speaking-static {
    animation: none;
    box-shadow:
      2px 3px 0 #181a20,
      0 0 0 2px var(--banter-accent, #e85d3f);
  }

  .banter-avatar-wrap {
    position: relative;
    display: inline-flex;
    flex: 0 0 auto;
  }

  .banter-speaking-dot {
    position: absolute;
    right: -3px;
    bottom: -3px;
    width: 9px;
    height: 9px;
    border-radius: 999px;
    background: var(--banter-accent, #e85d3f);
    border: 1.5px solid #fff8ea;
    animation: stripDotBeat 0.85s ease-in-out infinite;
  }

  .banter-speaking-dot-static {
    animation: none;
  }

  .human-avatar-dot {
    width: 36px;
    height: 36px;
    flex: 0 0 auto;
    border-radius: 9px;
    border: 2px solid #181a20;
    background: var(--banter-accent, #f5be58);
    color: #181a20;
    font-size: 14px;
    font-weight: 900;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .banter-card-body {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .banter-card-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
    line-height: 1.15;
  }

  .banter-speaker {
    font-size: 12px;
    font-weight: 800;
    color: #3b2f23;
  }

  .banter-mood-pill {
    display: inline-flex;
    align-items: center;
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 800;
    line-height: 1.3;
    white-space: nowrap;
  }

  .banter-reply-tag {
    font-size: 10.5px;
    font-weight: 700;
    color: #6b5643;
    white-space: nowrap;
  }

  .banter-line {
    margin: 0;
    font-size: 13px;
    font-weight: 800;
    line-height: 1.32;
    word-break: break-word;
  }

  @keyframes stripPop {
    0% {
      opacity: 0;
      transform: translateY(-4px) scale(0.95);
    }
    100% {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  @keyframes stripSpeakingPulse {
    0%,
    100% {
      box-shadow:
        2px 3px 0 #181a20,
        0 0 0 1.5px var(--banter-accent, #e85d3f);
    }
    50% {
      box-shadow:
        2px 3px 0 #181a20,
        0 0 0 2.5px var(--banter-accent, #e85d3f),
        0 4px 16px rgba(245, 190, 88, 0.48);
    }
  }

  @keyframes stripDotBeat {
    0%,
    100% {
      transform: scale(0.88);
      opacity: 0.78;
    }
    50% {
      transform: scale(1.22);
      opacity: 1;
    }
  }
</style>
