<script lang="ts">
  import { createAudioEventRouter } from "./lib/audio/audioRouter";
  import type { AudioStatus } from "./lib/audio/audioTypes";
  import {
    createHistoryController,
    type HistoryState,
  } from "./lib/history/historyClient";
  import HistoryModal from "./lib/history/HistoryModal.svelte";
  import {
    createShutdownController,
    type ShutdownState,
  } from "./lib/lifecycle/shutdownController";
  import ShutdownControl from "./lib/lifecycle/ShutdownControl.svelte";
  import { patchLobbyConfig } from "./lib/lobby/lobbyConfig";
  import LobbyView from "./lib/lobby/LobbyView.svelte";
  import {
    getBrowserDevicePreferencesStore,
    type DevicePreferences,
  } from "./lib/preferences/devicePreferences";
  import {
    createViewportDisplayController,
    type ViewportDisplayState,
  } from "./lib/platform/viewportDisplay";
  import { createWakeLockController } from "./lib/platform/wakeLock";
  import ComicImpactOverlay from "./lib/presentation/ComicImpactOverlay.svelte";
  import { createPresentationEffectsBus } from "./lib/presentation/effectsBus";
  import {
    createPresentationQueue,
    type PresentationState,
  } from "./lib/presentation/presentationQueue";
  import type { PlayerAction } from "./lib/protocol/generated/PlayerAction";
  import type { Seat } from "./lib/protocol/generated/Seat";
  import type { SeatInfo } from "./lib/protocol/generated/SeatInfo";
  import type { TablePace } from "./lib/protocol/generated/TablePace";
  import type { Tile } from "./lib/protocol/generated/Tile";
  import { seatIndex } from "./lib/protocol/seats";
  import { createTableClient } from "./lib/session/client";
  import type { SessionState } from "./lib/session/reducer";
  import DestructiveOpModal from "./lib/table/DestructiveOpModal.svelte";
  import MatchHud from "./lib/table/MatchHud.svelte";
  import MatchSummaryView from "./lib/table/MatchSummaryView.svelte";
  import {
    createRenderQualityController,
    type RenderQualityState,
  } from "./lib/table/renderQuality";
  import RotateOverlay from "./lib/table/RotateOverlay.svelte";
  import {
    buildOfflinePauseBanner,
    buildTakeOverSeatItems,
    getOfflineHumanTakeOverSeats,
    type TakeOverKind,
  } from "./lib/table/seatControl";
  import SettlementModal from "./lib/table/SettlementModal.svelte";
  import TableCanvas from "./lib/table/TableCanvas.svelte";
  import TakeOverModal from "./lib/table/TakeOverModal.svelte";
  import type { TableLayout } from "./lib/table/layout";
  import {
    deriveTurnCueState,
    findLegalDiscardAction,
  } from "./lib/table/viewModel";
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
  const viewportDisplay = createViewportDisplayController();
  const presentationQueue = createPresentationQueue({ effectsBus });
  const audioRouter = createAudioEventRouter({
    initialPreferences: initialDevicePrefs,
  });
  const shutdownController = createShutdownController({
    onShutdownSuccess: () => client.dispose(),
  });
  const historyController = createHistoryController();
  let sessionState = $state<SessionState>(client.getState());
  let shutdownState = $state<ShutdownState>(shutdownController.getState());
  let historyState = $state<HistoryState>(historyController.getState());
  let presentationState = $state<PresentationState>(
    presentationQueue.getState(),
  );
  let devicePrefs = $state<DevicePreferences>(initialDevicePrefs);
  let renderQualityState = $state<RenderQualityState>(renderQuality.getState());
  let viewportState = $state<ViewportDisplayState>(viewportDisplay.getState());
  let webglRecoveryState = $state<WebGLRecoveryState | null>(null);
  let tableLayout = $state<TableLayout | null>(null);
  let audioStatus = $state<AudioStatus>(audioRouter.getStatus());
  let settingsOpen = $state(false);
  let takeOverTarget = $state<{
    seatInfo: SeatInfo;
    kind: TakeOverKind;
  } | null>(null);
  const initialNickname = client.getIdentity().nickname;

  $effect(() => {
    const unsubscribeClient = client.subscribe((next) => {
      sessionState = next;
    });
    const unsubscribeShutdown = shutdownController.subscribe((next) => {
      shutdownState = next;
    });
    const unsubscribeHistory = historyController.subscribe((next) => {
      historyState = next;
    });
    const unsubscribePrefs = preferencesStore.subscribe((next) => {
      devicePrefs = next;
    });
    const unsubscribeRenderQuality = renderQuality.subscribe((next) => {
      renderQualityState = next;
    });
    const unsubscribeViewportDisplay = viewportDisplay.subscribe((next) => {
      viewportState = next;
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
      unsubscribeViewportDisplay();
      unsubscribeRenderQuality();
      unsubscribePrefs();
      unsubscribeHistory();
      unsubscribeShutdown();
      unsubscribeClient();
      viewportDisplay.dispose();
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
      tableLayout = null;
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
  const yourSeatAiAssisted = $derived(
    snapshot !== null &&
      snapshot.your_seat !== null &&
      snapshot.seats[seatIndex(snapshot.your_seat)].ai_assisted,
  );
  const inputLocked = $derived(
    actionsDisabled ||
      sessionState.pendingActionId !== null ||
      yourSeatAiAssisted,
  );
  const turnCue = $derived(
    deriveTurnCueState({
      playerView: presentedPlayerView,
      pendingAddedKong: presentationState.pendingAddedKong,
      inputLocked,
      reducedEffects: renderQualityState.reducedEffects,
    }),
  );
  const settlementVisible = $derived(
    presentedSnapshot !== null &&
      presentedSnapshot.phase === "InMatch" &&
      presentationState.settlementVisible &&
      presentedPlayerView?.phase === "BetweenHands" &&
      presentedPlayerView.last_settlement !== null,
  );
  const isSeatedMatch = $derived(
    presentedSnapshot?.phase === "InMatch" && presentedPlayerView !== null,
  );
  const currentPace = $derived<TablePace>(
    snapshot?.pace ?? presentedSnapshot?.pace ?? "Standard",
  );
  const connectionStatusText = $derived(
    shutdownState.phase === "stopped"
      ? "本机服务已关闭"
      : sessionState.replacedByNewTab
        ? "已被新标签页接管"
        : sessionState.connectionStatus === "connected"
          ? "已连接"
          : sessionState.connectionStatus === "connecting"
            ? "连接中"
            : "已断开",
  );
  const hasModalOpen = $derived(
    settingsOpen ||
      historyState.open ||
      takeOverTarget !== null ||
      viewportState.showRotateOverlay ||
      shutdownState.phase === "confirming" ||
      shutdownState.phase === "submitting" ||
      shutdownState.phase === "error" ||
      (snapshot !== null &&
        (snapshot.pending_destructive_op !== null || settlementVisible)),
  );

  $effect(() => {
    viewportDisplay.syncPhase(
      presentedSnapshot?.phase ?? null,
      presentedPlayerView !== null,
    );
  });

  $effect(() => {
    viewportDisplay.setModalOpen(hasModalOpen);
  });

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

  function openTakeOverModal(seatInfo: SeatInfo, kind: TakeOverKind): void {
    takeOverTarget = { seatInfo, kind };
  }

  function handleConfirmTakeOver(seat: Seat): void {
    client.takeOverSeat(seat, true);
    takeOverTarget = null;
  }
</script>

{#snippet statusBanners(compact = false)}
  {#if shutdownState.phase === "stopped"}
    <div class="banner banner-warn" class:banner-floating={compact} role="status">
      <div>
        <strong>本机服务已关闭</strong>
        {#if !compact}
          <div>
            本机 majjj 服务已安全停止。如需继续游玩，请重新启动应用或服务后刷新页面。
          </div>
        {/if}
      </div>
      <button class="btn" type="button" onclick={reloadPage}>刷新页面</button>
    </div>
  {:else if sessionState.replacedByNewTab}
    <div class="banner banner-danger" class:banner-floating={compact} role="alert">
      <div>
        <strong>会话已被新标签页接管</strong>
        {#if !compact}
          <div>
            同浏览器的另一标签页已接管当前牌桌连接，本页已停止自动重连，避免双标签页互踢。
          </div>
        {/if}
      </div>
      <button class="btn" type="button" onclick={reloadPage}>刷新接回</button>
    </div>
  {:else if sessionState.connectionStatus !== "connected"}
    <div class="banner banner-warn" class:banner-floating={compact} role="status">
      <span>
        {sessionState.connectionStatus === "connecting"
          ? "正在连接牌桌服务…"
          : "与牌桌服务的连接已断开，正在自动重连…"}
      </span>
    </div>
  {/if}

  {#if sessionState.lastErrorNotice}
    <div class="banner banner-warn" class:banner-floating={compact} role="status">
      <span>提示：{sessionState.lastErrorNotice}</span>
    </div>
  {/if}

  {#if sessionState.lastRejectedReason}
    <div class="banner banner-warn" class:banner-floating={compact} role="status">
      <span>操作未生效：{sessionState.lastRejectedReason}</span>
    </div>
  {/if}

  {#if webglRecoveryState?.needsRefresh}
    <div class="banner banner-danger" class:banner-floating={compact} role="alert">
      <div>
        <strong>2D 牌桌渲染异常</strong>
        {#if !compact}
          <div>{webglRecoveryState.errorMessage}</div>
        {/if}
      </div>
      <button class="btn" type="button" onclick={reloadPage}>刷新页面</button>
    </div>
  {/if}
{/snippet}

<main class="app-shell" class:app-shell-match={isSeatedMatch}>
  {#if !isSeatedMatch}
    {@render statusBanners(false)}
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
      onOpenHistory={() => void historyController.open()}
      onTakeOverSeat={(seatInfo) => openTakeOverModal(seatInfo, "offline_human")}
    />
  {:else if presentedSnapshot.phase === "InMatch"}
    <section class="match-shell" class:match-shell-immersive={isSeatedMatch}>
      {#if presentedSnapshot && presentedPlayerView}
        <div
          class="table-stage"
          class:table-stage-immersive={isSeatedMatch}
          class:stage-shaking={presentationState.activeImpact?.dampedShake ??
            false}
        >
          <TableCanvas
            snapshot={presentedSnapshot}
            pendingAddedKong={presentationState.pendingAddedKong}
            {inputLocked}
            confirmDiscard={devicePrefs.confirm_discard}
            paperGrainEnabled={renderQualityState.paperGrainEnabled}
            reducedEffects={renderQualityState.reducedEffects}
            onDiscard={handleCanvasDiscard}
            onLayoutChange={(layout) => (tableLayout = layout)}
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

          <MatchHud
            seatSnapshot={snapshot ?? presentedSnapshot}
            {presentedSnapshot}
            playerView={presentedPlayerView}
            {tableLayout}
            {turnCue}
            {viewportState}
            {currentPace}
            {actionsDisabled}
            {inputLocked}
            {hasModalOpen}
            canTogglePace={!actionsDisabled &&
              (snapshot?.your_seat ?? presentedSnapshot.your_seat) !== null}
            canDisband={!actionsDisabled &&
              snapshot !== null &&
              snapshot.your_seat !== null &&
              snapshot.pending_destructive_op === null}
            shouldSuppressEscape={() =>
              viewportDisplay.shouldSuppressActionBarEscape()}
            onToggleFullscreen={() => void viewportDisplay.toggleFullscreen()}
            onTogglePace={() =>
              handleSetPace(currentPace === "Fast" ? "Standard" : "Fast")}
            onOpenSettings={() => (settingsOpen = true)}
            onDisband={() => client.requestDisbandMatch()}
            onToggleAiAssist={(seat, enabled) => client.toggleAiAssist(seat, enabled)}
            onAction={handleActionBar}
          >
            {#snippet floatingBanners()}
              {@render statusBanners(true)}
            {/snippet}
          </MatchHud>

          <ComicImpactOverlay
            impact={presentationState.activeImpact}
            viewerSeat={presentedSnapshot.your_seat}
            seats={presentedSnapshot.seats}
            onSkip={() => presentationQueue.skipActiveImpact()}
          />

          <RotateOverlay
            visible={viewportState.showRotateOverlay}
            reducedMotion={renderQualityState.reducedEffects}
            onBypass={() => viewportDisplay.bypassPortrait()}
          />
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
        {@const activeSnap = snapshot ?? presentedSnapshot}
        {@const offlineNotice = buildOfflinePauseBanner(activeSnap)}
        {@const takeOverItems = buildTakeOverSeatItems(activeSnap)}
        {#if offlineNotice !== null}
          <div class="banner banner-warn" role="status">
            <span>{offlineNotice}</span>
          </div>
        {/if}
        <section class="card placeholder-card">
          <h1>对局进行中</h1>
          <p class="placeholder-note">
            当前本机未入座。你可以直接接管 AI 座位立即加入本小局，或经确认后接管已离线人类座位继续对局。
          </p>
          <div class="observer-seats-grid">
            {#each takeOverItems as item (item.seatInfo.seat)}
              <article class="observer-seat-card">
                <div class="observer-seat-head">
                  <strong>{item.displayName}</strong>
                  <span
                    class="tag"
                    class:tag-offline={item.takeOverKind === "offline_human"}
                    class:tag-online={item.takeOverKind === null && item.seatInfo.kind === "Human"}
                  >
                    {item.statusLabel}
                  </span>
                </div>
                {#if item.takeOverKind !== null && item.actionLabel !== null}
                  {@const kind = item.takeOverKind}
                  <button
                    class="btn"
                    class:btn-jade={kind === "ai"}
                    class:btn-primary={kind === "offline_human"}
                    type="button"
                    disabled={actionsDisabled}
                    onclick={() => openTakeOverModal(item.seatInfo, kind)}
                  >
                    {item.actionLabel}
                  </button>
                {:else}
                  <button class="btn" type="button" disabled>
                    不可接管
                  </button>
                {/if}
              </article>
            {/each}
          </div>
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
    {@const activeSnap = snapshot ?? presentedSnapshot}
    <MatchSummaryView
      summary={presentedPlayerView.match_summary}
      config={activeSnap.config}
      seats={activeSnap.seats}
      yourSeat={activeSnap.your_seat}
      lastSettlement={presentedPlayerView.last_settlement}
      disabled={actionsDisabled}
      onUpdateConfig={(config) => client.updateLobbyConfig(config)}
      onStartMatch={() => client.startMatch()}
      onReturnToLobby={() => client.returnToLobby()}
      onOpenHistory={() => void historyController.open()}
      onTakeOverSeat={(seatInfo) => openTakeOverModal(seatInfo, "offline_human")}
    />
  {:else}
    {@const activeSnap = snapshot ?? presentedSnapshot}
    {@const offlineTakeOverSeats = getOfflineHumanTakeOverSeats(
      activeSnap.seats,
      activeSnap.your_seat,
    )}
    <section class="card placeholder-card">
      <h1>对局已结束</h1>
      <p class="placeholder-note">
        当前牌桌已进入总结算阶段。
      </p>
      {#if activeSnap.your_seat !== null}
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
      {:else if offlineTakeOverSeats.length > 0}
        <div class="fallback-actions">
          {#each offlineTakeOverSeats as item (item.seatInfo.seat)}
            <button
              class="btn btn-primary"
              type="button"
              disabled={actionsDisabled}
              onclick={() => openTakeOverModal(item.seatInfo, "offline_human")}
            >
              {item.actionLabel ?? "接管离线座位"}（{item.displayName}）
            </button>
          {/each}
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
    serverVersion={snapshot?.server_version ?? null}
    tableId={snapshot?.table_id ?? null}
    {connectionStatusText}
    {shutdownState}
    onRequestShutdownConfirm={() => shutdownController.requestConfirm()}
    onCancelShutdown={() => shutdownController.cancelConfirm()}
    onConfirmShutdown={() => shutdownController.confirmShutdown()}
    onUpdatePreferences={(patch) => preferencesStore.update(patch)}
    onResetPreferences={() => preferencesStore.reset()}
    onChangePace={handleSettingsPaceChange}
    onClose={() => (settingsOpen = false)}
  />

  <HistoryModal
    state={historyState}
    onClose={() => historyController.close()}
    onRefresh={() => void historyController.refresh()}
    onExport={() => void historyController.requestExport()}
    onRequestClear={() => historyController.requestClear()}
    onCancelClear={() => historyController.cancelClear()}
    onConfirmClear={() => void historyController.confirmClear()}
  />

  <AudioResumeCapsule
    visible={audioStatus.needsResumeCapsule}
    onResume={() => audioRouter.unlockFromGesture()}
  />

  {#if takeOverTarget !== null}
    <TakeOverModal
      seatInfo={takeOverTarget.seatInfo}
      kind={takeOverTarget.kind}
      disabled={actionsDisabled}
      onConfirm={handleConfirmTakeOver}
      onCancel={() => (takeOverTarget = null)}
    />
  {/if}

  <ShutdownControl
    state={shutdownState}
    showTrigger={false}
    onRequestConfirm={() => shutdownController.requestConfirm()}
    onCancel={() => shutdownController.cancelConfirm()}
    onConfirm={() => shutdownController.confirmShutdown()}
  />

  {#if !isSeatedMatch}
    <footer class="footer-bar">
      <span>
        {#if snapshot}
          服务端版本 v{snapshot.server_version} · 牌桌 {snapshot.table_id.slice(0, 8)}
        {:else}
          等待服务端版本信息…
        {/if}
      </span>
      <span class="footer-status">
        连接状态：{connectionStatusText}
        <ShutdownControl
          state={shutdownState}
          showDialog={false}
          onRequestConfirm={() => shutdownController.requestConfirm()}
          onCancel={() => shutdownController.cancelConfirm()}
          onConfirm={() => shutdownController.confirmShutdown()}
        />
      </span>
    </footer>
  {/if}
</main>

<style>
  .footer-status {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
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

  .observer-seats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
    gap: 12px;
  }

  .observer-seat-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border-radius: 12px;
    border: 1px solid var(--border);
    background: var(--bg-muted);
  }

  .observer-seat-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .match-shell {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .match-shell-immersive {
    position: relative;
    width: 100%;
    height: 100%;
    min-height: 0;
    gap: 0;
    overflow: hidden;
  }

  .table-stage {
    position: relative;
    width: 100%;
    height: 100%;
    min-height: 0;
  }

  .table-stage-immersive {
    flex: 1 1 auto;
    overflow: hidden;
  }

  .banner-floating {
    padding: 5px 10px;
    border-radius: 10px;
    font-size: 12px;
    background: rgba(16, 32, 38, 0.92);
    border-color: rgba(245, 190, 88, 0.65);
    color: #fef3c7;
    box-shadow: 0 6px 16px rgba(0, 0, 0, 0.35);
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
  }

  .banner-floating.banner-danger {
    background: rgba(88, 20, 20, 0.92);
    border-color: #f87171;
    color: #fee2e2;
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
</style>
