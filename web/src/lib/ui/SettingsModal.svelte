<script lang="ts">
  import type { ShutdownState } from "../lifecycle/shutdownController";
  import ShutdownControl from "../lifecycle/ShutdownControl.svelte";
  import { TABLE_PACE_OPTIONS } from "../lobby/lobbyConfig";
  import type {
    DevicePreferences,
    IosAudioSessionMode,
  } from "../preferences/devicePreferences";
  import type { TablePace } from "../protocol/generated/TablePace";

  interface Props {
    open: boolean;
    preferences: DevicePreferences;
    pace: TablePace;
    canEditPace?: boolean;
    serverVersion?: string | null;
    tableId?: string | null;
    connectionStatusText?: string | null;
    shutdownState?: ShutdownState | null;
    onRequestShutdownConfirm?: () => void;
    onCancelShutdown?: () => void;
    onConfirmShutdown?: () => void;
    onUpdatePreferences: (patch: Partial<DevicePreferences>) => void;
    onResetPreferences: () => void;
    onChangePace: (pace: TablePace) => void;
    onClose: () => void;
  }

  let {
    open,
    preferences,
    pace,
    canEditPace = true,
    serverVersion = null,
    tableId = null,
    connectionStatusText = null,
    shutdownState = null,
    onRequestShutdownConfirm,
    onCancelShutdown,
    onConfirmShutdown,
    onUpdatePreferences,
    onResetPreferences,
    onChangePace,
    onClose,
  }: Props = $props();

  function handleKeyDown(event: KeyboardEvent): void {
    if (!open) {
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }

  function handleBackdropClick(event: MouseEvent): void {
    if (event.target === event.currentTarget) {
      onClose();
    }
  }

  function formatPercent(value: number): string {
    return `${Math.round(value * 100)}%`;
  }

  function handleVolumeInput(
    field: "master_volume" | "sfx_volume",
    event: Event,
  ): void {
    const raw = Number((event.currentTarget as HTMLInputElement).value);
    if (Number.isFinite(raw)) {
      onUpdatePreferences({ [field]: raw });
    }
  }

  function setAudioSession(mode: IosAudioSessionMode): void {
    onUpdatePreferences({ ios_audio_session: mode });
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if open}
  <div
    class="settings-backdrop"
    role="presentation"
    onclick={handleBackdropClick}
  >
    <div
      class="card settings-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="settings-modal-title"
    >
      <header class="settings-header">
        <div>
          <h2 id="settings-modal-title">偏好与牌桌节奏</h2>
          <p class="settings-subtitle">
            本机操作与声音偏好独立保存于当前浏览器，不影响同桌其他设备。
          </p>
        </div>
        <button
          class="btn btn-subtle close-btn"
          type="button"
          aria-label="关闭设置"
          onclick={onClose}
        >
          关闭
        </button>
      </header>

      <div class="settings-section">
        <div class="section-label-row">
          <h3>本机设备偏好</h3>
          <span class="tag">仅存本机</span>
        </div>

        <label class="toggle-row">
          <div>
            <strong>静音全部音效</strong>
            <span class="row-hint">关闭全部牌桌音效与提示音</span>
          </div>
          <input
            type="checkbox"
            checked={preferences.muted}
            onchange={(e) =>
              onUpdatePreferences({
                muted: (e.currentTarget as HTMLInputElement).checked,
              })}
          />
        </label>

        <label class="slider-row">
          <div class="slider-head">
            <span>总音量</span>
            <span class="slider-value">
              {formatPercent(preferences.master_volume)}
            </span>
          </div>
          <input
            class="range-input"
            type="range"
            min="0"
            max="1"
            step="0.05"
            value={preferences.master_volume}
            oninput={(e) => handleVolumeInput("master_volume", e)}
          />
        </label>

        <label class="slider-row">
          <div class="slider-head">
            <span>牌桌音效音量</span>
            <span class="slider-value">
              {formatPercent(preferences.sfx_volume)}
            </span>
          </div>
          <input
            class="range-input"
            type="range"
            min="0"
            max="1"
            step="0.05"
            value={preferences.sfx_volume}
            oninput={(e) => handleVolumeInput("sfx_volume", e)}
          />
        </label>

        <div class="segmented-group">
          <div class="group-label">
            <span>iOS Safari 音频会话</span>
            <span class="row-hint">切换是否绕过 iPhone 侧边静音开关</span>
          </div>
          <div class="segmented-buttons" role="group" aria-label="iOS 音频会话">
            <button
              class="seg-btn"
              class:seg-active={preferences.ios_audio_session === "playback"}
              type="button"
              onclick={() => setAudioSession("playback")}
            >
              强制出声（Playback）
            </button>
            <button
              class="seg-btn"
              class:seg-active={preferences.ios_audio_session === "ambient"}
              type="button"
              onclick={() => setAudioSession("ambient")}
            >
              后台混播（Ambient）
            </button>
          </div>
        </div>

        <div class="segmented-group">
          <div class="group-label">
            <span>出牌手感模式</span>
            <span class="row-hint">
              Mac 默认单击直出，触屏默认二次点击或向上轻划打出
            </span>
          </div>
          <div class="segmented-buttons" role="group" aria-label="出牌手感模式">
            <button
              class="seg-btn"
              class:seg-active={!preferences.confirm_discard}
              type="button"
              onclick={() => onUpdatePreferences({ confirm_discard: false })}
            >
              单击直出
            </button>
            <button
              class="seg-btn"
              class:seg-active={preferences.confirm_discard}
              type="button"
              onclick={() => onUpdatePreferences({ confirm_discard: true })}
            >
              二次点击 / 上划打出
            </button>
          </div>
        </div>

        <label class="toggle-row">
          <div>
            <strong>精简特效 / 省电模式</strong>
            <span class="row-hint">关闭纸纹暗角滤镜、震屏与闪白，保持牌面清晰</span>
          </div>
          <input
            type="checkbox"
            checked={preferences.reduced_effects}
            onchange={(e) =>
              onUpdatePreferences({
                reduced_effects: (e.currentTarget as HTMLInputElement).checked,
              })}
          />
        </label>
      </div>

      <div class="settings-section">
        <div class="section-label-row">
          <h3>牌桌节奏</h3>
          <span class="tag tag-self">同步全桌</span>
        </div>

        <div class="segmented-buttons" role="group" aria-label="牌桌节奏">
          {#each TABLE_PACE_OPTIONS as opt (opt.value)}
            <button
              class="seg-btn"
              class:seg-active={pace === opt.value}
              type="button"
              disabled={!canEditPace}
              onclick={() => onChangePace(opt.value)}
            >
              {opt.label}
            </button>
          {/each}
        </div>
        {#if !canEditPace}
          <p class="row-hint">入座并保持连接后可切换全桌共享节奏。</p>
        {/if}
      </div>

      {#if serverVersion || tableId || connectionStatusText || shutdownState?.visible}
        <div class="settings-service-meta">
          <div class="service-meta-line">
            {#if serverVersion && tableId}
              <span>服务端版本 v{serverVersion} · 牌桌 {tableId.slice(0, 8)}</span>
            {:else}
              <span>等待服务端版本信息…</span>
            {/if}
            {#if connectionStatusText}
              <span>连接状态：{connectionStatusText}</span>
            {/if}
          </div>
          {#if shutdownState && onRequestShutdownConfirm && onCancelShutdown && onConfirmShutdown}
            <ShutdownControl
              state={shutdownState}
              showDialog={false}
              onRequestConfirm={onRequestShutdownConfirm}
              onCancel={onCancelShutdown}
              onConfirm={onConfirmShutdown}
            />
          {/if}
        </div>
      {/if}

      <footer class="settings-footer">
        <button
          class="btn btn-subtle"
          type="button"
          onclick={onResetPreferences}
        >
          恢复本机默认
        </button>
        <button class="btn btn-primary" type="button" onclick={onClose}>
          完成
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .settings-backdrop {
    position: fixed;
    inset: 0;
    z-index: 70;
    background: rgba(12, 22, 26, 0.72);
    backdrop-filter: blur(3px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
  }

  .settings-modal {
    width: min(490px, 100%);
    max-height: 90dvh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .settings-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
  }

  h2,
  h3 {
    margin: 0;
  }

  h2 {
    font-size: 19px;
  }

  h3 {
    font-size: 14px;
    color: var(--ink);
  }

  .settings-subtitle {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--ink-muted);
  }

  .close-btn {
    flex: 0 0 auto;
    padding: 6px 10px;
    font-size: 13px;
  }

  .settings-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
    border-radius: 12px;
    background: var(--bg-muted);
    border: 1px solid var(--border);
  }

  .section-label-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }

  .toggle-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    cursor: pointer;
    font-size: 14px;
  }

  .toggle-row input[type="checkbox"] {
    width: 18px;
    height: 18px;
    accent-color: var(--accent);
    flex: 0 0 auto;
  }

  .row-hint {
    display: block;
    margin: 2px 0 0;
    font-size: 12px;
    color: var(--ink-muted);
  }

  .slider-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 14px;
  }

  .slider-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
  }

  .slider-value {
    font-size: 12px;
    color: var(--ink-muted);
    font-variant-numeric: tabular-nums;
  }

  .range-input {
    width: 100%;
    accent-color: var(--accent);
  }

  .segmented-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .group-label {
    font-size: 14px;
    font-weight: 600;
  }

  .group-label .row-hint {
    font-weight: 400;
  }

  .segmented-buttons {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 6px;
  }

  .seg-btn {
    border: 1px solid var(--border);
    border-radius: 9px;
    background: #fff;
    padding: 8px 10px;
    font-size: 13px;
    font-weight: 600;
    color: var(--ink);
    transition:
      background-color 0.12s ease,
      border-color 0.12s ease,
      color 0.12s ease;
  }

  .seg-active {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .settings-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
  }

  .settings-service-meta {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-radius: 10px;
    border: 1px dashed var(--border);
    background: var(--bg-card);
    font-size: 12px;
    color: var(--ink-muted);
  }

  .service-meta-line {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 12px;
  }

  @media (max-width: 460px) {
    .segmented-buttons {
      grid-template-columns: 1fr;
    }
  }
</style>
