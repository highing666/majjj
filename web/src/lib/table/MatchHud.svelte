<script lang="ts">
  import type { Snippet } from "svelte";
  import { matchModeLabel } from "../lobby/lobbyConfig";
  import type { ViewportDisplayState } from "../platform/viewportDisplay";
  import type { PlayerAction } from "../protocol/generated/PlayerAction";
  import type { PlayerView } from "../protocol/generated/PlayerView";
  import type { Seat } from "../protocol/generated/Seat";
  import type { TablePace } from "../protocol/generated/TablePace";
  import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
  import ActionBar from "./ActionBar.svelte";
  import type { TableLayout } from "./layout";
  import SeatControlBar from "./SeatControlBar.svelte";
  import { formatRoundTitle } from "./tiles";
  import type { TurnCueState } from "./viewModel";

  interface Props {
    seatSnapshot: TableSnapshot;
    presentedSnapshot: TableSnapshot;
    playerView: PlayerView;
    tableLayout: TableLayout | null;
    turnCue: TurnCueState;
    viewportState: ViewportDisplayState;
    currentPace: TablePace;
    actionsDisabled: boolean;
    inputLocked: boolean;
    hasModalOpen: boolean;
    canTogglePace: boolean;
    canDisband: boolean;
    shouldSuppressEscape: () => boolean;
    onToggleFullscreen: () => void;
    onTogglePace: () => void;
    onOpenSettings: () => void;
    onDisband: () => void;
    onToggleAiAssist: (seat: Seat, enabled: boolean) => void;
    onAction: (action: PlayerAction) => void;
    floatingBanners?: Snippet;
  }

  let {
    seatSnapshot,
    presentedSnapshot,
    playerView,
    tableLayout,
    turnCue,
    viewportState,
    currentPace,
    actionsDisabled,
    inputLocked,
    hasModalOpen,
    canTogglePace,
    canDisband,
    shouldSuppressEscape,
    onToggleFullscreen,
    onTogglePace,
    onOpenSettings,
    onDisband,
    onToggleAiAssist,
    onAction,
    floatingBanners,
  }: Props = $props();

  const turnBarStyle = $derived.by(() => {
    if (!tableLayout) {
      return "";
    }
    const { turnBarRect, width, height } = tableLayout;
    const centerX = turnBarRect.x + turnBarRect.width / 2;
    const bottomPx = height - (turnBarRect.y + turnBarRect.height);
    const leftPct = (centerX / width) * 100;
    const bottomPct = (bottomPx / height) * 100;
    return `left: ${leftPct.toFixed(3)}%; bottom: ${bottomPct.toFixed(3)}%; max-width: min(calc(100% - 16px), ${turnBarRect.width}px);`;
  });
</script>

<div class="glass-top-left">
  <span class="glass-pill">
    {matchModeLabel(presentedSnapshot.config.mode)}
    {#if !viewportState.isShortLandscape}
      · {formatRoundTitle(playerView.turn)} · 牌墙余 {playerView.wall_remaining} 张
    {/if}
  </span>
</div>

<div class="glass-top-right">
  <button
    class="glass-btn"
    type="button"
    aria-pressed={viewportState.isFullscreen}
    onclick={onToggleFullscreen}
  >
    {viewportState.isFullscreen ? "退出全屏" : "全屏 F"}
  </button>
  <button
    class="glass-btn"
    class:glass-btn-active={currentPace === "Fast"}
    type="button"
    aria-label={currentPace === "Fast"
      ? "牌桌节奏：快速，点击切换为标准"
      : "牌桌节奏：标准，点击切换为快速"}
    disabled={!canTogglePace}
    onclick={onTogglePace}
  >
    {currentPace === "Fast" ? "快速" : "标准"}
  </button>
  <button
    class="glass-btn"
    type="button"
    onclick={onOpenSettings}
  >
    设置
  </button>
  <button
    class="glass-btn"
    type="button"
    disabled={!canDisband}
    onclick={onDisband}
  >
    解散
  </button>
</div>

<div class="match-floating-top">
  {@render floatingBanners?.()}
  <SeatControlBar
    snapshot={seatSnapshot}
    disabled={actionsDisabled}
    floating={true}
    {onToggleAiAssist}
  />
</div>

{#if playerView.phase === "Playing"}
  {@const isShortDock = tableLayout?.isShortLandscape ?? false}
  <div
    class="action-bar-slot"
    class:turn-dock-short={isShortDock}
    style={turnBarStyle}
  >
    {#if turnCue.mode === "action_prompt"}
      <ActionBar
        {playerView}
        disabled={inputLocked}
        {hasModalOpen}
        {shouldSuppressEscape}
        {onAction}
      />
    {:else if turnCue.turnPillText !== null}
      <div
        class="turn-cue-pill"
        class:turn-cue-pill-static={!turnCue.animateBreath}
        role="status"
        aria-live="polite"
      >
        <span class="turn-cue-star" aria-hidden="true">✦</span>
        <span>{turnCue.turnPillText}</span>
      </div>
    {:else if turnCue.waitHintText !== null}
      <div class="turn-wait-hint" role="status">
        {turnCue.waitHintText}
      </div>
    {/if}
  </div>
{/if}

<style>
  .glass-top-left {
    position: absolute;
    left: max(12px, env(safe-area-inset-left));
    top: max(8px, env(safe-area-inset-top));
    z-index: 18;
    pointer-events: none;
  }

  .glass-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 700;
    background: rgba(9, 20, 24, 0.72);
    border: 1px solid rgba(212, 168, 87, 0.42);
    color: #f5be58;
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
    white-space: nowrap;
  }

  .glass-top-right {
    position: absolute;
    right: max(12px, env(safe-area-inset-right));
    top: max(8px, env(safe-area-inset-top));
    z-index: 18;
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .glass-btn {
    padding: 4px 9px;
    border-radius: 999px;
    border: 1px solid rgba(255, 255, 255, 0.22);
    background: rgba(9, 20, 24, 0.72);
    color: #f5eedf;
    font-size: 11px;
    font-weight: 600;
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
    white-space: nowrap;
  }

  .glass-btn:hover:not(:disabled) {
    border-color: #f5be58;
    color: #f5be58;
  }

  .glass-btn-active {
    background: rgba(212, 168, 87, 0.24);
    border-color: rgba(245, 190, 88, 0.78);
    color: #f5be58;
  }

  .match-floating-top {
    position: absolute;
    top: max(8px, env(safe-area-inset-top));
    left: 50%;
    transform: translateX(-50%);
    z-index: 24;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    max-width: min(520px, calc(100% - 420px));
    pointer-events: none;
  }

  .match-floating-top > :global(*) {
    pointer-events: auto;
  }

  .action-bar-slot {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    bottom: clamp(68px, 14dvh, 114px);
    z-index: 20;
    max-width: calc(100% - 16px);
    display: flex;
    justify-content: center;
    pointer-events: none;
  }

  .action-bar-slot > :global(*) {
    pointer-events: auto;
  }

  .turn-cue-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 16px;
    border-radius: 999px;
    background: linear-gradient(180deg, #ffe08a 0%, #eca535 100%);
    border: 1.5px solid #181a20;
    color: #181a20;
    font-weight: 800;
    font-size: 13px;
    line-height: 1.2;
    white-space: nowrap;
    box-shadow:
      0 2px 0 #181a20,
      0 0 16px rgba(245, 190, 88, 0.5);
    animation: turnCuePulse 1.7s ease-in-out infinite;
  }

  .turn-cue-pill-static {
    animation: none;
    box-shadow:
      0 2px 0 #181a20,
      0 0 0 1.5px #f5be58,
      0 0 14px rgba(245, 190, 88, 0.45);
  }

  .turn-cue-star {
    font-size: 12px;
    line-height: 1;
  }

  .turn-wait-hint {
    display: inline-flex;
    align-items: center;
    padding: 2px 12px;
    border-radius: 999px;
    background: rgba(9, 20, 24, 0.64);
    border: 1px solid rgba(212, 168, 87, 0.22);
    color: #94a3b8;
    font-size: 12px;
    font-weight: 600;
    line-height: 1.2;
    white-space: nowrap;
  }

  .turn-dock-short .turn-cue-pill {
    padding: 2px 12px;
    font-size: 11px;
    gap: 4px;
  }

  .turn-dock-short .turn-wait-hint {
    padding: 1px 10px;
    font-size: 10.5px;
  }

  .turn-dock-short :global(.action-bar) {
    padding: 4px 8px;
    gap: 5px;
    border-radius: 10px;
  }

  .turn-dock-short :global(.action-btn) {
    padding: 3px 8px;
    font-size: 12px;
    gap: 4px;
  }

  @keyframes turnCuePulse {
    0%,
    100% {
      box-shadow:
        0 2px 0 #181a20,
        0 0 12px rgba(245, 190, 88, 0.42);
    }
    50% {
      box-shadow:
        0 2px 0 #181a20,
        0 0 22px rgba(245, 190, 88, 0.85);
    }
  }

  @media (max-width: 720px) {
    .match-floating-top {
      top: max(36px, calc(env(safe-area-inset-top) + 30px));
      max-width: calc(100% - 24px);
    }
  }

  @media (max-height: 460px) {
    .glass-top-left,
    .glass-top-right {
      top: max(6px, env(safe-area-inset-top));
    }

    .glass-pill,
    .glass-btn {
      padding: 3px 8px;
      font-size: 11px;
    }

    .action-bar-slot {
      bottom: clamp(54px, 18dvh, 74px);
    }
  }
</style>
