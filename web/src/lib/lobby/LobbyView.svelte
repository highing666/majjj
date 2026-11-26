<script lang="ts">
  import QRCode from "qrcode";
  import { createHistoryClient } from "../history/historyClient";
  import type { LobbyConfig } from "../protocol/generated/LobbyConfig";
  import type { Seat } from "../protocol/generated/Seat";
  import type { SeatInfo } from "../protocol/generated/SeatInfo";
  import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
  import LobbyConfigFields from "./LobbyConfigFields.svelte";
  import { buildLobbyTableViewModel, type LobbyTableSlot } from "./lobbyTable";

  interface Props {
    snapshot: TableSnapshot;
    initialNickname: string;
    disabled?: boolean;
    onUpdateNickname: (nickname: string) => void;
    onJoinSeat: (seat: Seat | null) => void;
    onLeaveSeat: () => void;
    onUpdateConfig: (config: LobbyConfig) => void;
    onStartMatch: () => void;
    onOpenSettings?: () => void;
    onOpenHistory?: () => void;
    onTakeOverSeat?: (seatInfo: SeatInfo) => void;
  }

  let {
    snapshot,
    initialNickname,
    disabled = false,
    onUpdateNickname,
    onJoinSeat,
    onLeaveSeat,
    onUpdateConfig,
    onStartMatch,
    onOpenSettings,
    onOpenHistory,
    onTakeOverSeat,
  }: Props = $props();

  let lastAppliedNickname = $state("");
  let nicknameDraft = $state("");
  let nicknameDirty = $state(false);
  let nicknameSavedNotice = $state(false);
  let selectedIpv4Url = $state<string | null>(null);
  let qrDataUrl = $state<string | null>(null);
  let qrError = $state<string | null>(null);
  let copiedLabel = $state<string | null>(null);
  let recentNicknames = $state<string[]>([]);

  $effect(() => {
    let cancelled = false;
    createHistoryClient()
      .fetchPreferences()
      .then((prefs) => {
        if (!cancelled) {
          recentNicknames = prefs.recent_nicknames.filter(
            (n) => n.trim().length > 0,
          );
        }
      })
      .catch(() => {
        // Non-blocking: ignore unavailable preferences endpoint in offline/mocked contexts.
      });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    if (!nicknameDirty) {
      nicknameDraft = initialNickname;
      lastAppliedNickname = initialNickname;
    }
  });

  const ipv4Urls = $derived(snapshot.lan_urls.ipv4_urls);
  const preferredIpv4Url = $derived(snapshot.lan_urls.preferred_ipv4_url);
  const localHostnameUrl = $derived(snapshot.lan_urls.local_hostname_url);

  const activeIpv4Url = $derived.by(() => {
    if (
      selectedIpv4Url &&
      (ipv4Urls.includes(selectedIpv4Url) ||
        selectedIpv4Url === preferredIpv4Url)
    ) {
      return selectedIpv4Url;
    }
    return preferredIpv4Url ?? ipv4Urls[0] ?? null;
  });

  $effect(() => {
    const targetUrl = activeIpv4Url;
    if (!targetUrl) {
      qrDataUrl = null;
      qrError = null;
      return;
    }
    let cancelled = false;
    QRCode.toDataURL(targetUrl, {
      width: 220,
      margin: 2,
      color: {
        dark: "#241b14",
        light: "#fffaf2",
      },
    })
      .then((dataUrl) => {
        if (!cancelled) {
          qrDataUrl = dataUrl;
          qrError = null;
        }
      })
      .catch(() => {
        if (!cancelled) {
          qrDataUrl = null;
          qrError = "二维码生成失败，请直接复制下方地址访问。";
        }
      });

    return () => {
      cancelled = true;
    };
  });

  const tableModel = $derived(buildLobbyTableViewModel(snapshot));
  const isSeated = $derived(tableModel.isSeated);
  const humanCount = $derived(
    snapshot.seats.filter((seatInfo) => seatInfo.kind === "Human").length,
  );
  const aiFillCount = $derived(
    snapshot.seats.filter((seatInfo) => seatInfo.kind === "Empty").length,
  );
  const canConfigure = $derived(isSeated && !disabled);
  const canStartMatch = $derived(isSeated && humanCount >= 1 && !disabled);

  function handleNicknameSubmit(event: SubmitEvent): void {
    event.preventDefault();
    applyNickname();
  }

  function applyNickname(showNotice = true): void {
    const trimmed = nicknameDraft.trim();
    nicknameDraft = trimmed;
    nicknameDirty = false;
    if (trimmed !== lastAppliedNickname) {
      lastAppliedNickname = trimmed;
      onUpdateNickname(trimmed);
    }
    if (trimmed.length > 0) {
      recentNicknames = [
        trimmed,
        ...recentNicknames.filter((item) => item !== trimmed),
      ].slice(0, 12);
    }
    if (showNotice) {
      nicknameSavedNotice = true;
      setTimeout(() => {
        nicknameSavedNotice = false;
      }, 1800);
    }
  }

  function selectRecentNickname(name: string): void {
    if (disabled) {
      return;
    }
    nicknameDraft = name;
    applyNickname(true);
  }

  function handleNicknameBlur(): void {
    if (!disabled && nicknameDirty) {
      applyNickname(false);
    }
  }

  function handleIpv4Switch(event: Event): void {
    selectedIpv4Url = (event.currentTarget as HTMLSelectElement).value || null;
  }

  async function copyUrl(url: string, label: string): Promise<void> {
    try {
      if (typeof navigator !== "undefined" && navigator.clipboard) {
        await navigator.clipboard.writeText(url);
        copiedLabel = label;
        setTimeout(() => {
          if (copiedLabel === label) {
            copiedLabel = null;
          }
        }, 1800);
      }
    } catch {
      // Clipboard may be unavailable on non-HTTPS Safari without gesture.
    }
  }

  function describeSeatStatus(seatInfo: SeatInfo): string {
    if (seatInfo.kind === "Human") {
      return seatInfo.nickname || "未命名玩家";
    }
    if (seatInfo.kind === "Ai") {
      return "AI 玩家";
    }
    return "空位 · 开局补 AI";
  }
</script>

{#snippet seatCard(slot: LobbyTableSlot)}
  {@const seatInfo = slot.seatInfo}
  <article
    class="seat-card ring-slot-{slot.slot}"
    class:seat-mine={slot.isMine}
  >
    <div class="seat-top">
      <strong class="seat-name">{slot.label}</strong>
      <div class="seat-tags">
        {#if slot.isMine}
          <span class="tag tag-self">本机</span>
        {/if}
        {#if seatInfo.kind === "Human"}
          <span
            class="tag"
            class:tag-online={seatInfo.connected}
            class:tag-offline={!seatInfo.connected}
          >
            {seatInfo.connected ? "在线" : "离线"}
          </span>
        {:else if seatInfo.kind === "Empty"}
          <span class="tag">AI 补位</span>
        {:else}
          <span class="tag">AI</span>
        {/if}
      </div>
    </div>

    <p class="seat-occupant">{describeSeatStatus(seatInfo)}</p>

    <div class="seat-actions">
      {#if slot.isMine}
        <button
          class="btn btn-subtle"
          type="button"
          onclick={onLeaveSeat}
          {disabled}
        >
          离座
        </button>
      {:else if seatInfo.kind === "Empty"}
        <button
          class="btn btn-jade"
          type="button"
          onclick={() => onJoinSeat(slot.seat)}
          {disabled}
        >
          {isSeated ? "换到此座" : "入座"}
        </button>
      {:else if !isSeated && seatInfo.kind === "Human" && !seatInfo.connected && onTakeOverSeat}
        <button
          class="btn btn-primary"
          type="button"
          onclick={() => onTakeOverSeat(seatInfo)}
          {disabled}
        >
          接管离线座位
        </button>
      {:else}
        <button class="btn" type="button" disabled>已占用</button>
      {/if}
    </div>
  </article>
{/snippet}

<section class="lobby-grid">
  <div class="lobby-main">
    <div class="card">
      <div class="section-head">
        <div>
          <h1>麻将牌局 · 开桌大厅</h1>
          <p class="subtitle">
            家庭局域网直连牌桌，任一已入座玩家均可对等调整规则并开局。
          </p>
        </div>
        <div class="lobby-head-actions">
          <span class="tag">
            已入座 {humanCount} 人 · 空位将补 {aiFillCount} 名 AI
          </span>
          {#if onOpenHistory}
            <button
              class="btn btn-subtle settings-trigger"
              type="button"
              onclick={onOpenHistory}
            >
              战绩
            </button>
          {/if}
          {#if onOpenSettings}
            <button
              class="btn btn-subtle settings-trigger"
              type="button"
              onclick={onOpenSettings}
            >
              偏好设置
            </button>
          {/if}
        </div>
      </div>

      <form class="nickname-form" onsubmit={handleNicknameSubmit}>
        <label class="field-label" for="player-nickname">我的昵称</label>
        <div class="nickname-row">
          <input
            id="player-nickname"
            class="text-input"
            type="text"
            maxlength="24"
            list="recent-nicknames-list"
            placeholder="输入昵称（留空则按座次自动命名）"
            value={nicknameDraft}
            oninput={(e) => {
              nicknameDraft = (e.currentTarget as HTMLInputElement).value;
              nicknameDirty = true;
            }}
            onblur={handleNicknameBlur}
            {disabled}
          />
          <button class="btn" type="submit" {disabled}>保存昵称</button>
        </div>
        {#if recentNicknames.length > 0}
          <datalist id="recent-nicknames-list">
            {#each recentNicknames as name (name)}
              <option value={name}></option>
            {/each}
          </datalist>
          <div class="recent-nicknames-row" aria-label="常用昵称">
            <span class="recent-label">常用昵称：</span>
            {#each recentNicknames as name (name)}
              <button
                class="nickname-chip"
                class:nickname-chip-active={nicknameDraft.trim() === name}
                type="button"
                {disabled}
                onclick={() => selectRecentNickname(name)}
              >
                {name}
              </button>
            {/each}
          </div>
        {/if}
        {#if nicknameSavedNotice}
          <p class="hint-ok">昵称已保存至本机浏览器</p>
        {/if}
      </form>

      <div class="ring-table" aria-label="四边环桌选座">
        <div class="ring-row-single">
          {@render seatCard(tableModel.slots.top)}
        </div>
        <div class="ring-row-middle">
          {@render seatCard(tableModel.slots.left)}
          <div class="ring-center-badge" aria-label="逆时针出牌顺序">
            <span class="ring-arrow" aria-hidden="true">↺</span>
            <span class="ring-direction">逆时针</span>
          </div>
          {@render seatCard(tableModel.slots.right)}
        </div>
        <div class="ring-row-single">
          {@render seatCard(tableModel.slots.bottom)}
        </div>
      </div>

      {#if tableModel.shuffleNotice}
        <p class="shuffle-notice" role="status">{tableModel.shuffleNotice}</p>
      {/if}
    </div>

    <div class="card">
      <div class="section-head">
        <h2>牌桌规则配置</h2>
        {#if !isSeated}
          <span class="tag tag-offline">请先入座后再修改规则</span>
        {/if}
      </div>

      <LobbyConfigFields
        config={snapshot.config}
        disabled={!canConfigure}
        {onUpdateConfig}
      />

      <div class="start-bar">
        <p class="start-hint">
          {#if isSeated}
            点击开始后将按定座规则就位，剩余 {aiFillCount} 个空位由 AI 自动补齐并进入首小局。
          {:else}
            请先在上方任选一个空座入座，随后即可发起对局。
          {/if}
        </p>
        <button
          class="btn btn-primary start-btn"
          type="button"
          onclick={onStartMatch}
          disabled={!canStartMatch}
        >
          开始对局
        </button>
      </div>
    </div>
  </div>

  <aside class="card lan-panel">
    <h2>手机扫码入桌</h2>
    <p class="subtitle">
      同一家庭 Wi-Fi 下的 iPhone 可直接用系统相机扫码，在 Safari 零安装打开。
    </p>

    {#if activeIpv4Url}
      <div class="qr-frame">
        {#if qrDataUrl}
          <img
            class="qr-image"
            src={qrDataUrl}
            alt="局域网扫码入桌二维码"
            width="220"
            height="220"
          />
        {:else if qrError}
          <p class="qr-fallback">{qrError}</p>
        {:else}
          <p class="qr-fallback">正在生成二维码…</p>
        {/if}
      </div>

      {#if ipv4Urls.length > 1}
        <label class="config-field">
          <span class="field-label">备选局域网网卡 IP</span>
          <select
            class="select-input"
            value={activeIpv4Url}
            onchange={handleIpv4Switch}
          >
            {#each ipv4Urls as url (url)}
              <option value={url}>{url}</option>
            {/each}
          </select>
        </label>
      {/if}

      <div class="url-box">
        <span class="field-label">当前局域网 IPv4 地址</span>
        <div class="url-row">
          <code class="url-code">{activeIpv4Url}</code>
          <button
            class="btn btn-subtle copy-btn"
            type="button"
            onclick={() => copyUrl(activeIpv4Url, "ipv4")}
          >
            {copiedLabel === "ipv4" ? "已复制" : "复制"}
          </button>
        </div>
      </div>
    {:else}
      <div class="qr-empty">
        暂未检测到可用的私有局域网 IPv4 地址，请确认宿主机已连接家庭 Wi-Fi 或有线网。
      </div>
    {/if}

    {#if localHostnameUrl}
      <div class="url-box">
        <span class="field-label">宿主机 .local 名称地址</span>
        <div class="url-row">
          <code class="url-code">{localHostnameUrl}</code>
          <button
            class="btn btn-subtle copy-btn"
            type="button"
            onclick={() => copyUrl(localHostnameUrl, "local")}
          >
            {copiedLabel === "local" ? "已复制" : "复制"}
          </button>
        </div>
      </div>
    {/if}
  </aside>
</section>

<style>
  h1,
  h2 {
    margin: 0;
    line-height: 1.25;
  }

  h1 {
    font-size: 22px;
  }

  h2 {
    font-size: 17px;
  }

  .subtitle {
    margin: 4px 0 0;
    font-size: 13px;
    color: var(--ink-muted);
  }

  .lobby-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.55fr) minmax(270px, 1fr);
    gap: 16px;
    align-items: start;
  }

  .lobby-main {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .section-head {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: flex-start;
    gap: 10px;
    margin-bottom: 14px;
  }

  .lobby-head-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .settings-trigger {
    padding: 5px 10px;
    font-size: 12px;
  }

  .nickname-form {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 16px;
  }

  .field-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-muted);
  }

  .nickname-row {
    display: flex;
    gap: 8px;
  }

  .text-input,
  .select-input {
    width: 100%;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 9px 12px;
    background: #fff;
  }

  .hint-ok {
    margin: 0;
    font-size: 12px;
    color: var(--jade);
  }

  .recent-nicknames-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .recent-label {
    font-size: 12px;
    color: var(--ink-muted);
  }

  .nickname-chip {
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 3px 10px;
    font-size: 12px;
    background: var(--bg-muted);
    color: var(--ink);
    cursor: pointer;
  }

  .nickname-chip-active {
    border-color: var(--accent);
    background: #fff3e8;
    color: var(--accent);
    font-weight: 600;
  }

  .ring-table {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border-radius: 14px;
    border: 1px solid var(--border);
    background: #f6efe4;
  }

  .ring-row-single {
    display: flex;
    justify-content: center;
  }

  .ring-row-single .seat-card {
    width: min(62%, 230px);
  }

  .ring-row-middle {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: center;
    gap: 10px;
  }

  .ring-center-badge {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    width: 60px;
    height: 60px;
    border-radius: 999px;
    border: 1px dashed var(--border);
    background: #fffaf2;
    color: var(--ink-muted);
    user-select: none;
  }

  .ring-arrow {
    font-size: 18px;
    line-height: 1;
    color: var(--accent);
  }

  .ring-direction {
    font-size: 11px;
    font-weight: 600;
    line-height: 1;
  }

  .shuffle-notice {
    margin: 10px 0 0;
    padding: 8px 12px;
    border-radius: 10px;
    border: 1px solid #ecc69e;
    background: #fff4e5;
    color: #8a4b08;
    font-size: 13px;
    font-weight: 500;
  }

  .seat-card {
    border: 1px solid var(--border);
    border-radius: 12px;
    background: #fff;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }

  .seat-mine {
    border-color: var(--accent);
    background: #fff8f4;
  }

  .seat-top {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 4px 8px;
  }

  .seat-name {
    font-size: 16px;
  }

  .seat-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .seat-occupant {
    margin: 0;
    font-size: 14px;
    color: var(--ink);
    min-height: 20px;
  }

  .seat-actions {
    display: flex;
  }

  .seat-actions .btn {
    width: 100%;
  }

  .config-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .start-bar {
    margin-top: 16px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }

  .start-hint {
    margin: 0;
    font-size: 13px;
    color: var(--ink-muted);
    flex: 1 1 240px;
  }

  .start-btn {
    min-width: 144px;
    padding: 11px 22px;
    font-size: 16px;
  }

  .lan-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .qr-frame {
    display: flex;
    justify-content: center;
    align-items: center;
    padding: 12px;
    border-radius: 12px;
    border: 1px dashed var(--border);
    background: #fffaf2;
  }

  .qr-image {
    display: block;
    max-width: 100%;
    height: auto;
    border-radius: 8px;
  }

  .qr-fallback,
  .qr-empty {
    margin: 0;
    padding: 16px 10px;
    text-align: center;
    font-size: 13px;
    color: var(--ink-muted);
    border-radius: 10px;
    background: var(--bg-muted);
  }

  .url-box {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .url-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .url-code {
    flex: 1 1 auto;
    min-width: 0;
    overflow-wrap: anywhere;
    font-size: 12px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--bg-muted);
    border: 1px solid var(--border);
  }

  .copy-btn {
    flex: 0 0 auto;
    padding: 7px 10px;
    font-size: 12px;
  }

  @media (max-width: 820px) {
    .lobby-grid {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 540px) {
    .ring-table {
      padding: 8px;
      gap: 8px;
    }

    .ring-row-single .seat-card {
      width: min(64%, 200px);
    }

    .ring-row-middle {
      grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
      gap: 6px;
    }

    .ring-center-badge {
      width: 46px;
      height: 46px;
    }

    .ring-arrow {
      font-size: 15px;
    }

    .ring-direction {
      font-size: 10px;
    }

    .seat-card {
      padding: 8px;
      gap: 6px;
    }

    .seat-top {
      gap: 4px 6px;
    }

    .seat-name {
      font-size: 14px;
    }

    .seat-occupant {
      font-size: 13px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .seat-actions .btn {
      padding: 6px 8px;
      font-size: 12px;
    }

    .nickname-row {
      flex-direction: column;
    }
  }
</style>
