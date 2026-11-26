<script lang="ts">
  import { createAudioEventRouter } from "./lib/audio/audioRouter";
  import type { AudioStatus } from "./lib/audio/audioTypes";
  import {
    createShutdownController,
    type ShutdownState,
  } from "./lib/lifecycle/shutdownController";
  import ShutdownControl from "./lib/lifecycle/ShutdownControl.svelte";
  import { matchModeLabel, patchLobbyConfig } from "./lib/lobby/lobbyConfig";
  import LobbyView from "./lib/lobby/LobbyView.svelte";
  import {
    getBrowserDevicePreferencesStore,
    type DevicePreferences,
  } from "./lib/preferences/devicePreferences";
  import { createWakeLockController } from "./lib/platform/wakeLock";
  import ComicImpactOverlay from "./lib/presentation/ComicImpactOverlay.svelte";
  import { createPresentationEffectsBus } from "./lib/presentation/effectsBus";
  import {
    createPresentationQueue,
    type PresentationState,
  } from "./lib/presentation/presentationQueue";
  import type { PlayerAction } from "./lib/protocol/generated/PlayerAction";
  import type { TablePace } from "./lib/protocol/generated/TablePace";
  import type { Tile } from "./lib/protocol/generated/Tile";
  import { createTableClient } from "./lib/session/client";
  import type { SessionState } from "./lib/session/reducer";
  import ActionBar from "./lib/table/ActionBar.svelte";
  import DestructiveOpModal from "./lib/table/DestructiveOpModal.svelte";
  import MatchSummaryView from "./lib/table/MatchSummaryView.svelte";
  import {
    createRenderQualityController,
    type RenderQualityState,
  } from "./lib/table/renderQuality";
  import SettlementModal from "./lib/table/SettlementModal.svelte";
  import TableCanvas from "./lib/table/TableCanvas.svelte";
  import { formatRoundTitle, seatLabel } from "./lib/table/tiles";
  import { findLegalDiscardAction } from "./lib/table/viewModel";
  import type { WebGLRecoveryState } from "./lib/table/webglRecovery";
  import AudioResumeCapsule from "./lib/ui/AudioResumeCapsule.svelte";
  import SettingsModal from "./lib/ui/SettingsModal.svelte";

  const client = createTableClient();
  const preferencesStore = getBrowserDevicePreferencesStore();
  const initialDevicePrefs = preferencesStore.load();
  const effectsBus = createPresentationEffectsBus(
    initialDevicePrefs.reduced_effects,
  );
  const renderQuality = createRenderQualityController({
    effectsBus,
    initialManualReducedEffects: initialDevicePrefs.reduced_effects,
  });
  const wakeLock = createWakeLockController();
  const presentationQueue = createPresentationQueue({ effectsBus });
  const audioRouter = createAudioEventRouter({
    initialPreferences: initialDevicePrefs,
  });
  const shutdownController = createShutdownController({
    onShutdownSuccess: () => client.dispose(),
  });
  let sessionState = $state<SessionState>(client.getState());
  let shutdownState = $state<ShutdownState>(shutdownController.getState());
  let presentationState = $state<PresentationState>(
    presentationQueue.getState(),
  );
  let devicePrefs = $state<DevicePreferences>(initialDevicePrefs);
  let renderQualityState = $state<RenderQualityState>(renderQuality.getState());
  let webglRecoveryState = $state<WebGLRecoveryState | null>(null);
  let audioStatus = $state<AudioStatus>(audioRouter.getStatus());
  let settingsOpen = $state(false);
  const initialNickname = client.getIdentity().nickname;

  $effect(() => {
    const unsubscribeClient = client.subscribe((next) => {
      sessionState = next;
    });
    const unsubscribeShutdown = shutdownController.subscribe((next) => {
      shutdownState = next;
    });
    const unsubscribePrefs = preferencesStore.subscribe((next) => {
      devicePrefs = next;
    });
    const unsubscribeRenderQuality = renderQuality.subscribe((next) => {
      renderQualityState = next;
    });
    const unsubscribePresentation = presentationQueue.subscribe((next) => {
      presentationState = next;
    });
    const unsubscribeAudioStatus = audioRouter.subscribe((next) => {
      audioStatus = next;
    });
    const unsubscribeSemantic = presentationQueue.onSemanticEvent((event) => {
      audioRouter.handleSemanticEvent(event);
    });
    const unbindAudioGestures = audioRouter.bindGestureUnlock(window);
    const unsubscribeMessages = client.onServerMessage((message) => {
      presentationQueue.enqueueServerMessage(message);
    });
    client.connect();
    return () => {
      unsubscribeMessages();
      unbindAudioGestures();
      unsubscribeSemantic();
      unsubscribeAudioStatus();
      unsubscribePresentation();
      unsubscribeRenderQuality();
      unsubscribePrefs();
      unsubscribeShutdown();
      unsubscribeClient();
      wakeLock.dispose();
      renderQuality.dispose();
      audioRouter.dispose();
      presentationQueue.dispose();
      effectsBus.dispose();
      client.dispose();
    };
  });

  $effect(() => {
    renderQuality.setManualReducedEffects(devicePrefs.reduced_effects);
    audioRouter.applyPreferences(devicePrefs);
  });

  const snapshot = $derived(sessionState.snapshot);

  $effect(() => {
    shutdownController.syncHostIsLoopback(snapshot?.host_is_loopback === true);
  });

  $effect(() => {
    const phase = presentedSnapshot?.phase ?? null;
    wakeLock.syncPhase(phase);
    if (phase !== "InMatch") {
      webglRecoveryState = null;
      renderQuality.resetSamplingWindow();
    }
  });
  const playerView = $derived(snapshot?.player_view ?? null);
  const presentedSnapshot = $derived(
    presentationState.snapshot ?? sessionState.snapshot,
  );
  const presentedPlayerView = $derived(
    presentedSnapshot?.player_view ?? null,
  );
  const actionsDisabled = $derived(
    sessionState.replacedByNewTab ||
      sessionState.connectionStatus !== "connected",
  );
  const inputLocked = $derived(
    actionsDisabled || sessionState.pendingActionId !== null,
  );
  const settlementVisible = $derived(
    presentedSnapshot !== null &&
      presentedSnapshot.phase === "InMatch" &&
      presentationState.settlementVisible &&
      presentedPlayerView?.phase === "BetweenHands" &&
      presentedPlayerView.last_settlement !== null,
  );
  const hasModalOpen = $derived(
    settingsOpen ||
      shutdownState.phase === "confirming" ||
      shutdownState.phase === "submitting" ||
      shutdownState.phase === "error" ||
      (snapshot !== null &&
        (snapshot.pending_destructive_op !== null || settlementVisible)),
  );

  function reloadPage(): void {
    window.location.reload();
  }

  function handleCanvasDiscard(tile: Tile, fromDrawn: boolean): void {
    if (!playerView || inputLocked) {
      return;
    }
    const action = findLegalDiscardAction(
      playerView.legal_actions,
      tile,
      fromDrawn,
    );
    if (action) {
      client.submitAction(action);
    }
  }

  function handleActionBar(action: PlayerAction): void {
    if (inputLocked) {
      return;
    }
    if (action === "Pass") {
      presentationQueue.emitLocalUiAction({ type: "UiPassClicked" });
    }
    client.submitAction(action);
  }

  function handleConfirmNextHand(): void {
    if (inputLocked) {
      return;
    }
    presentationQueue.emitLocalUiAction({ type: "UiConfirmNextHand" });
    client.submitAction("ConfirmNextHand");
  }

  function handleSetPace(pace: TablePace): void {
    if (actionsDisabled || snapshot?.your_seat === null) {
      return;
    }
    if (snapshot?.pace !== pace) {
      client.setPace(pace);
    }
  }

  function handleSettingsPaceChange(pace: TablePace): void {
    if (!snapshot || actionsDisabled || snapshot.your_seat === null) {
      return;
    }
    if (snapshot.phase === "InMatch") {
      handleSetPace(pace);
    } else if (snapshot.config.pace !== pace) {
      client.updateLobbyConfig(patchLobbyConfig(snapshot.config, { pace }));
    }
  }
</script>

<main class="app-shell" class:app-shell-match={presentedSnapshot?.phase === "InMatch"}>
  {#if shutdownState.phase === "stopped"}
    <div class="banner banner-warn" role="status">
      <div>
        <strong>本机服务已关闭</strong>
        <div>
          本机 majjj 服务已安全停止。如需继续游玩，请重新启动应用或服务后刷新页面。
        </div>
      </div>
      <button class="btn" type="button" onclick={reloadPage}>刷新页面</button>
    </div>
  {:else if sessionState.replacedByNewTab}
    <div class="banner banner-danger" role="alert">
      <div>
        <strong>会话已被新标签页接管</strong>
        <div>
          同浏览器的另一标签页已接管当前牌桌连接，本页已停止自动重连，避免双标签页互踢。
        </div>
      </div>
      <button class="btn" type="button" onclick={reloadPage}>刷新接回</button>
    </div>
  {:else if sessionState.connectionStatus !== "connected"}
    <div class="banner banner-warn" role="status">
      <span>
        {sessionState.connectionStatus === "connecting"
          ? "正在连接牌桌服务…"
          : "与牌桌服务的连接已断开，正在自动重连…"}
      </span>
    </div>
  {/if}

  {#if sessionState.lastErrorNotice}
    <div class="banner banner-warn" role="status">
      <span>提示：{sessionState.lastErrorNotice}</span>
    </div>
  {/if}

  {#if sessionState.lastRejectedReason}
    <div class="banner banner-warn" role="status">
      <span>操作未生效：{sessionState.lastRejectedReason}</span>
    </div>
  {/if}

  {#if webglRecoveryState?.needsRefresh}
    <div class="banner banner-danger" role="alert">
      <div>
        <strong>2D 牌桌渲染异常</strong>
        <div>{webglRecoveryState.errorMessage}</div>
      </div>
      <button class="btn" type="button" onclick={reloadPage}>刷新页面</button>
    </div>
  {/if}

  {#if presentedSnapshot === null}
    <section class="card placeholder-card">
      <h1>麻将牌局 · Majjj</h1>
      <p>正在获取当前牌桌状态，请稍候…</p>
    </section>
  {:else if presentedSnapshot.phase === "Lobby"}
    <LobbyView
      snapshot={snapshot ?? presentedSnapshot}
      {initialNickname}
      disabled={actionsDisabled}
      onUpdateNickname={(name) => client.updateNickname(name)}
      onJoinSeat={(seat) => client.joinSeat(seat)}
      onLeaveSeat={() => client.leaveSeat()}
      onUpdateConfig={(config) => client.updateLobbyConfig(config)}
      onStartMatch={() => client.startMatch()}
      onOpenSettings={() => (settingsOpen = true)}
    />
  {:else if presentedSnapshot.phase === "InMatch"}
    <section class="match-shell">
      <header class="match-topbar">
        <div class="topbar-info">
          <span class="tag tag-mode">
            {matchModeLabel(presentedSnapshot.config.mode)}
          </span>
          {#if presentedPlayerView}
            <span class="topbar-round">
              {formatRoundTitle(presentedPlayerView.turn)} · 牌墙余 {presentedPlayerView.wall_remaining} 张
            </span>
            {#if presentationState.pendingAddedKong !== null}
              <span class="tag tag-pending">补杠待确认…</span>
            {:else if presentedPlayerView.phase === "Playing" && presentedPlayerView.turn.active_seat === presentedSnapshot.your_seat}
              <span class="tag tag-turn">轮到你行动</span>
            {:else if presentedPlayerView.phase === "Playing" && presentedPlayerView.turn.active_seat !== null}
              <span class="tag">等待{seatLabel(presentedPlayerView.turn.active_seat)}</span>
            {/if}
          {/if}
        </div>

        <div class="topbar-controls">
          <div class="pace-switch" role="group" aria-label="牌桌节奏">
            <button
              class="pace-btn"
              class:pace-active={(snapshot?.pace ?? presentedSnapshot.pace) === "Standard"}
              type="button"
              disabled={actionsDisabled || (snapshot?.your_seat ?? presentedSnapshot.your_seat) === null}
              onclick={() => handleSetPace("Standard")}
            >
              标准节奏
            </button>
            <button
              class="pace-btn"
              class:pace-active={(snapshot?.pace ?? presentedSnapshot.pace) === "Fast"}
              type="button"
              disabled={actionsDisabled || (snapshot?.your_seat ?? presentedSnapshot.your_seat) === null}
              onclick={() => handleSetPace("Fast")}
            >
              快速节奏
            </button>
          </div>

          <button
            class="btn btn-subtle topbar-btn"
            type="button"
            onclick={() => (settingsOpen = true)}
          >
            设置
          </button>

          <button
            class="btn btn-subtle topbar-btn"
            type="button"
            disabled={actionsDisabled ||
              snapshot === null ||
              snapshot.your_seat === null ||
              snapshot.pending_destructive_op !== null}
            onclick={() => client.requestDisbandMatch()}
          >
            解散对局
          </button>
        </div>
      </header>

      {#if presentedSnapshot && presentedPlayerView}
        <div
          class="table-stage"
          class:stage-shaking={presentationState.activeImpact?.dampedShake ??
            false}
        >
          <TableCanvas
            snapshot={presentedSnapshot}
            pendingAddedKong={presentationState.pendingAddedKong}
            {inputLocked}
            confirmDiscard={devicePrefs.confirm_discard}
            paperGrainEnabled={renderQualityState.paperGrainEnabled}
            onDiscard={handleCanvasDiscard}
            onFrameTick={() => renderQuality.recordFrame()}
            onWebGLRecoveryChange={(state) => (webglRecoveryState = state)}
            onRefresh={reloadPage}
            onTileHover={(tile, fromDrawn) =>
              presentationQueue.emitLocalUiAction({
                type: "UiTileHover",
                tile,
                fromDrawn,
              })}
            onTileSelect={(tile, fromDrawn) =>
              presentationQueue.emitLocalUiAction({
                type: "UiTileSelect",
                tile,
                fromDrawn,
              })}
          />

          <ComicImpactOverlay
            impact={presentationState.activeImpact}
            viewerSeat={presentedSnapshot.your_seat}
            seats={presentedSnapshot.seats}
            onSkip={() => presentationQueue.skipActiveImpact()}
          />

          {#if presentedPlayerView.phase === "Playing"}
            <div class="action-bar-slot">
              <ActionBar
                playerView={presentedPlayerView}
                disabled={inputLocked}
                {hasModalOpen}
                onAction={handleActionBar}
              />
            </div>
          {/if}
        </div>

        {#if settlementVisible && presentedPlayerView.last_settlement !== null}
          <SettlementModal
            settlement={presentedPlayerView.last_settlement}
            seats={presentedSnapshot.seats}
            mode={presentedPlayerView.mode}
            fanCap={presentedPlayerView.rule_profile.fan_cap}
            yourSeat={presentedSnapshot.your_seat}
            readySeats={presentedPlayerView.ready_seats}
            canConfirmNextHand={(playerView?.legal_actions ?? []).includes(
              "ConfirmNextHand",
            )}
            disabled={inputLocked}
            destructivePending={snapshot !== null &&
              snapshot.pending_destructive_op !== null}
            onConfirmNextHand={handleConfirmNextHand}
            onRequestEndFreeTable={() => client.requestEndFreeTable()}
          />
        {/if}
      {:else}
        <section class="card placeholder-card">
          <h1>对局进行中</h1>
          <p class="placeholder-note">
            当前本机未入座，正在观摩实时牌桌。待本场对局结束后可返回开桌大厅入座。
          </p>
        </section>
      {/if}

      {#if snapshot !== null && snapshot.pending_destructive_op !== null}
        <DestructiveOpModal
          op={snapshot.pending_destructive_op}
          seats={snapshot.seats}
          yourSeat={snapshot.your_seat}
          disabled={actionsDisabled}
          onConfirm={(opId, accept) => client.confirmDestructiveOp(opId, accept)}
        />
      {/if}
    </section>
  {:else if presentedPlayerView?.match_summary}
    <MatchSummaryView
      summary={presentedPlayerView.match_summary}
      config={presentedSnapshot.config}
      seats={presentedSnapshot.seats}
      yourSeat={presentedSnapshot.your_seat}
      lastSettlement={presentedPlayerView.last_settlement}
      disabled={actionsDisabled}
      onUpdateConfig={(config) => client.updateLobbyConfig(config)}
      onStartMatch={() => client.startMatch()}
      onReturnToLobby={() => client.returnToLobby()}
    />
  {:else}
    <section class="card placeholder-card">
      <h1>对局已结束</h1>
      <p class="placeholder-note">
        当前牌桌已进入总结算阶段。
      </p>
      {#if presentedSnapshot.your_seat !== null}
        <div class="fallback-actions">
          <button
            class="btn btn-subtle"
            type="button"
            onclick={() => client.returnToLobby()}
            disabled={actionsDisabled}
          >
            返回开桌大厅
          </button>
          <button
            class="btn btn-primary"
            type="button"
            onclick={() => client.startMatch()}
            disabled={actionsDisabled}
          >
            原班人马再开一桌
          </button>
        </div>
      {/if}
    </section>
  {/if}

  <SettingsModal
    open={settingsOpen}
    preferences={devicePrefs}
    pace={snapshot?.phase === "InMatch"
      ? snapshot.pace
      : (snapshot?.config.pace ?? "Standard")}
    canEditPace={snapshot !== null &&
      !actionsDisabled &&
      snapshot.your_seat !== null}
    onUpdatePreferences={(patch) => preferencesStore.update(patch)}
    onResetPreferences={() => preferencesStore.reset()}
    onChangePace={handleSettingsPaceChange}
    onClose={() => (settingsOpen = false)}
  />

  <AudioResumeCapsule
    visible={audioStatus.needsResumeCapsule}
    onResume={() => audioRouter.unlockFromGesture()}
  />

  <footer class="footer-bar">
    <span>
      {#if snapshot}
        服务端版本 v{snapshot.server_version} · 牌桌 {snapshot.table_id.slice(0, 8)}
      {:else}
        等待服务端版本信息…
      {/if}
    </span>
    <span class="footer-status">
      连接状态：{shutdownState.phase === "stopped"
        ? "本机服务已关闭"
        : sessionState.replacedByNewTab
          ? "已被新标签页接管"
          : sessionState.connectionStatus === "connected"
            ? "已连接"
            : sessionState.connectionStatus === "connecting"
              ? "连接中"
              : "已断开"}
      <ShutdownControl
        state={shutdownState}
        onRequestConfirm={() => shutdownController.requestConfirm()}
        onCancel={() => shutdownController.cancelConfirm()}
        onConfirm={() => shutdownController.confirmShutdown()}
      />
    </span>
  </footer>
</main>

<style>
  .footer-status {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .app-shell-match {
    max-width: 1380px;
    padding-top: max(10px, env(safe-area-inset-top));
    padding-bottom: max(12px, env(safe-area-inset-bottom));
    gap: 10px;
  }

  .placeholder-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .placeholder-card h1 {
    margin: 0;
    font-size: 22px;
  }

  .placeholder-note {
    margin: 0;
    color: var(--ink-muted);
    font-size: 14px;
  }

  .fallback-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .match-shell {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .match-topbar {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border-radius: 12px;
    background: #102026;
    color: #f5eedf;
    border: 1px solid rgba(212, 168, 87, 0.45);
    box-shadow: 0 6px 18px rgba(10, 18, 24, 0.2);
  }

  .topbar-info {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .topbar-round {
    font-size: 13px;
    font-weight: 700;
    color: #f5eedf;
  }

  .tag-mode {
    background: rgba(212, 168, 87, 0.2);
    border: 1px solid rgba(212, 168, 87, 0.55);
    color: #f5be58;
  }

  .tag-turn {
    background: #f5be58;
    color: #181a20;
  }

  .tag-pending {
    background: #f09d38;
    color: #181a20;
  }

  .topbar-controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .pace-switch {
    display: inline-flex;
    border: 1px solid rgba(212, 168, 87, 0.4);
    border-radius: 9px;
    overflow: hidden;
    background: #0b161b;
  }

  .pace-btn {
    border: none;
    background: transparent;
    padding: 4px 10px;
    font-size: 12px;
    font-weight: 600;
    color: #a9c0c4;
  }

  .pace-active {
    background: #d4a857;
    color: #181a20;
  }

  .topbar-btn {
    padding: 4px 10px;
    font-size: 12px;
    color: #f5eedf;
    border-color: rgba(245, 238, 223, 0.28);
  }

  .topbar-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.1);
  }

  .table-stage {
    position: relative;
  }

  .table-stage.stage-shaking {
    animation: stagePiltoverShake 220ms steps(2, end);
  }

  @keyframes stagePiltoverShake {
    0% {
      transform: translate(0, 0);
    }
    25% {
      transform: translate(-5px, 3px);
    }
    50% {
      transform: translate(5px, -4px);
    }
    75% {
      transform: translate(-3px, -2px);
    }
    100% {
      transform: translate(0, 0);
    }
  }

  .action-bar-slot {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    bottom: clamp(68px, 15%, 124px);
    z-index: 20;
    max-width: calc(100% - 16px);
    pointer-events: none;
  }

  .action-bar-slot > :global(*) {
    pointer-events: auto;
  }

  @media (max-height: 460px) {
    .action-bar-slot {
      position: static;
      transform: none;
      max-width: 100%;
      margin-top: 8px;
      pointer-events: auto;
    }
  }
</style>
