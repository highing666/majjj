<script lang="ts">
  import LobbyView from "./lib/lobby/LobbyView.svelte";
  import { createTableClient } from "./lib/session/client";
  import type { SessionState } from "./lib/session/reducer";

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
  const actionsDisabled = $derived(
    sessionState.replacedByNewTab ||
      sessionState.connectionStatus !== "connected",
  );

  function reloadPage(): void {
    window.location.reload();
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
  {:else}
    <section class="card placeholder-card">
      <h1>
        {snapshot.phase === "Finished" ? "对局已结束" : "对局进行中"}
      </h1>
      <p class="placeholder-meta">
        当前座次：{snapshot.your_seat ?? "未入座"} · 同步序号：{snapshot.seq} ·
        引擎阶段：{snapshot.player_view?.phase ?? "等待视图"}
      </p>
      <p class="placeholder-note">
        2D 牌桌画布与结算界面将在下一步接入此视图。
      </p>
      {#if snapshot.phase === "Finished"}
        <div>
          <button
            class="btn btn-primary"
            type="button"
            onclick={() => client.returnToLobby()}
            disabled={actionsDisabled}
          >
            返回开桌大厅
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

  .placeholder-meta,
  .placeholder-note {
    margin: 0;
    color: var(--ink-muted);
    font-size: 14px;
  }
</style>
