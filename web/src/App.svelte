<script lang="ts">
  import LobbyView from "./lib/lobby/LobbyView.svelte";
  import type { PlayerAction } from "./lib/protocol/generated/PlayerAction";
  import type { TablePace } from "./lib/protocol/generated/TablePace";
  import type { Tile } from "./lib/protocol/generated/Tile";
  import { createTableClient } from "./lib/session/client";
  import type { SessionState } from "./lib/session/reducer";
  import ActionBar from "./lib/table/ActionBar.svelte";
  import DestructiveOpModal from "./lib/table/DestructiveOpModal.svelte";
  import MatchSummaryView from "./lib/table/MatchSummaryView.svelte";
  import SettlementModal from "./lib/table/SettlementModal.svelte";
  import TableCanvas from "./lib/table/TableCanvas.svelte";
  import { formatRoundTitle, seatLabel } from "./lib/table/tiles";
  import { findLegalDiscardAction } from "./lib/table/viewModel";

  const client = createTableClient();
  let sessionState = $state<SessionState>(client.getState());
  const initialNickname = client.getIdentity().nickname;

  $effect(() => {
    const unsubscribe = client.subscribe((next) => {
      sessionState = next;
    });
    client.connect();
    return () => {
      unsubscribe();
      client.dispose();
    };
  });

  const snapshot = $derived(sessionState.snapshot);
  const playerView = $derived(snapshot?.player_view ?? null);
  const actionsDisabled = $derived(
    sessionState.replacedByNewTab ||
      sessionState.connectionStatus !== "connected",
  );
  const inputLocked = $derived(
    actionsDisabled || sessionState.pendingActionId !== null,
  );
  const hasModalOpen = $derived(
    snapshot !== null &&
      (snapshot.pending_destructive_op !== null ||
        (snapshot.phase === "InMatch" &&
          playerView?.phase === "BetweenHands" &&
          playerView.last_settlement !== null)),
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
    client.submitAction(action);
  }

  function handleSetPace(pace: TablePace): void {
    if (actionsDisabled || snapshot?.your_seat === null) {
      return;
    }
    if (snapshot?.pace !== pace) {
      client.setPace(pace);
    }
  }
</script>

<main class="app-shell">
  {#if sessionState.replacedByNewTab}
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

  {#if snapshot === null}
    <section class="card placeholder-card">
      <h1>麻将牌局 · Majjj</h1>
      <p>正在获取当前牌桌状态，请稍候…</p>
    </section>
  {:else if snapshot.phase === "Lobby"}
    <LobbyView
      {snapshot}
      {initialNickname}
      disabled={actionsDisabled}
      onUpdateNickname={(name) => client.updateNickname(name)}
      onJoinSeat={(seat) => client.joinSeat(seat)}
      onLeaveSeat={() => client.leaveSeat()}
      onUpdateConfig={(config) => client.updateLobbyConfig(config)}
      onStartMatch={() => client.startMatch()}
    />
  {:else if snapshot.phase === "InMatch"}
    <section class="match-shell">
      <header class="match-topbar">
        <div class="topbar-info">
          <span class="tag tag-mode">
            {snapshot.config.mode === "EastOnly" ? "东风战" : "自由桌"}
          </span>
          {#if playerView}
            <span class="topbar-round">
              {formatRoundTitle(playerView.turn)} · 牌墙余 {playerView.wall_remaining} 张
            </span>
            {#if sessionState.pendingAddedKong !== null}
              <span class="tag tag-pending">补杠待确认…</span>
            {:else if playerView.phase === "Playing" && playerView.turn.active_seat === snapshot.your_seat}
              <span class="tag tag-turn">轮到你行动</span>
            {:else if playerView.phase === "Playing" && playerView.turn.active_seat !== null}
              <span class="tag">等待{seatLabel(playerView.turn.active_seat)}</span>
            {/if}
          {/if}
        </div>

        <div class="topbar-controls">
          <div class="pace-switch" role="group" aria-label="牌桌节奏">
            <button
              class="pace-btn"
              class:pace-active={snapshot.pace === "Standard"}
              type="button"
              disabled={actionsDisabled || snapshot.your_seat === null}
              onclick={() => handleSetPace("Standard")}
            >
              标准节奏
            </button>
            <button
              class="pace-btn"
              class:pace-active={snapshot.pace === "Fast"}
              type="button"
              disabled={actionsDisabled || snapshot.your_seat === null}
              onclick={() => handleSetPace("Fast")}
            >
              快速节奏
            </button>
          </div>

          <button
            class="btn btn-subtle disband-btn"
            type="button"
            disabled={actionsDisabled ||
              snapshot.your_seat === null ||
              snapshot.pending_destructive_op !== null}
            onclick={() => client.requestDisbandMatch()}
          >
            解散对局
          </button>
        </div>
      </header>

      {#if playerView}
        <div class="table-stage">
          <TableCanvas
            {snapshot}
            pendingAddedKong={sessionState.pendingAddedKong}
            {inputLocked}
            onDiscard={handleCanvasDiscard}
          />

          {#if playerView.phase === "Playing"}
            <div class="action-bar-slot">
              <ActionBar
                {playerView}
                disabled={inputLocked}
                {hasModalOpen}
                onAction={handleActionBar}
              />
            </div>
          {/if}
        </div>

        {#if playerView.phase === "BetweenHands" && playerView.last_settlement !== null}
          <SettlementModal
            settlement={playerView.last_settlement}
            seats={snapshot.seats}
            mode={playerView.mode}
            fanCap={playerView.rule_profile.fan_cap}
            yourSeat={snapshot.your_seat}
            readySeats={playerView.ready_seats}
            canConfirmNextHand={playerView.legal_actions.includes(
              "ConfirmNextHand",
            )}
            disabled={inputLocked}
            destructivePending={snapshot.pending_destructive_op !== null}
            onConfirmNextHand={() => client.submitAction("ConfirmNextHand")}
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

      {#if snapshot.pending_destructive_op !== null}
        <DestructiveOpModal
          op={snapshot.pending_destructive_op}
          seats={snapshot.seats}
          yourSeat={snapshot.your_seat}
          disabled={actionsDisabled}
          onConfirm={(opId, accept) => client.confirmDestructiveOp(opId, accept)}
        />
      {/if}
    </section>
  {:else if playerView?.match_summary}
    <MatchSummaryView
      summary={playerView.match_summary}
      seats={snapshot.seats}
      yourSeat={snapshot.your_seat}
      lastSettlement={playerView.last_settlement}
      disabled={actionsDisabled}
      onStartMatch={() => client.startMatch()}
      onReturnToLobby={() => client.returnToLobby()}
    />
  {:else}
    <section class="card placeholder-card">
      <h1>对局已结束</h1>
      <p class="placeholder-note">
        当前牌桌已进入总结算阶段。
      </p>
      {#if snapshot.your_seat !== null}
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

  <footer class="footer-bar">
    <span>
      {#if snapshot}
        服务端版本 v{snapshot.server_version} · 牌桌 {snapshot.table_id.slice(0, 8)}
      {:else}
        等待服务端版本信息…
      {/if}
    </span>
    <span>
      连接状态：{sessionState.replacedByNewTab
        ? "已被新标签页接管"
        : sessionState.connectionStatus === "connected"
          ? "已连接"
          : sessionState.connectionStatus === "connecting"
            ? "连接中"
            : "已断开"}
    </span>
  </footer>
</main>

<style>
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
    gap: 10px;
  }

  .match-topbar {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    border-radius: var(--radius);
    background: var(--bg-card);
    border: 1px solid var(--border);
    box-shadow: var(--shadow);
  }

  .topbar-info {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .topbar-round {
    font-size: 14px;
    font-weight: 700;
  }

  .tag-mode {
    background: #14282e;
    color: #f5eedf;
  }

  .tag-turn {
    background: #fce6c2;
    color: #93450d;
  }

  .tag-pending {
    background: #fdf0df;
    color: #a65414;
  }

  .topbar-controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .pace-switch {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
    background: var(--bg-muted);
  }

  .pace-btn {
    border: none;
    background: transparent;
    padding: 6px 10px;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-muted);
  }

  .pace-active {
    background: #14282e;
    color: #f5eedf;
  }

  .disband-btn {
    padding: 6px 12px;
    font-size: 13px;
  }

  .table-stage {
    position: relative;
  }

  .action-bar-slot {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    bottom: clamp(62px, 16%, 104px);
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
