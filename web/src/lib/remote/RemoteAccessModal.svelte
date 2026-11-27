<script lang="ts">
  import RemoteAccessPanel from "./RemoteAccessPanel.svelte";
  import type { RemoteAccessState } from "./remoteAccessController";

  interface Props {
    open: boolean;
    state: RemoteAccessState;
    onClose: () => void;
    onRefresh: () => void;
    onEnable: () => void;
    onDisable: () => void;
    onResetCode: () => void;
    onSavePublicUrl: (publicUrl: string | null) => Promise<boolean>;
    onSelectBaseUrl: (baseUrl: string | null) => void;
  }

  let {
    open,
    state,
    onClose,
    onRefresh,
    onEnable,
    onDisable,
    onResetCode,
    onSavePublicUrl,
    onSelectBaseUrl,
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
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if open && state.visible}
  <div
    class="remote-backdrop"
    role="presentation"
    onclick={handleBackdropClick}
  >
    <div
      class="card remote-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="remote-access-modal-title"
    >
      <header class="remote-modal-header">
        <div>
          <h2 id="remote-access-modal-title">远程联机管理</h2>
          <p class="remote-modal-subtitle">
            对局进行中也可随时开启或关闭远程联机，远程朋友凭口令入桌后可接管空余 AI 座次。
          </p>
        </div>
        <button
          class="btn btn-subtle close-btn"
          type="button"
          aria-label="关闭远程联机管理"
          onclick={onClose}
        >
          关闭
        </button>
      </header>

      <RemoteAccessPanel
        {state}
        embedded={true}
        {onRefresh}
        {onEnable}
        {onDisable}
        {onResetCode}
        {onSavePublicUrl}
        {onSelectBaseUrl}
      />
    </div>
  </div>
{/if}

<style>
  .remote-backdrop {
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

  .remote-modal {
    width: min(460px, 100%);
    max-height: 90dvh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .remote-modal-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
  }

  h2 {
    margin: 0;
    font-size: 19px;
  }

  .remote-modal-subtitle {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--ink-muted);
    line-height: 1.45;
  }

  .close-btn {
    flex: 0 0 auto;
    padding: 6px 10px;
    font-size: 13px;
  }
</style>
