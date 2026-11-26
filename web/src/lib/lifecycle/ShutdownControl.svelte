<script lang="ts">
  import type { ShutdownState } from "./shutdownController";

  interface Props {
    state: ShutdownState;
    showTrigger?: boolean;
    showDialog?: boolean;
    onRequestConfirm: () => void;
    onCancel: () => void;
    onConfirm: () => void;
  }

  let {
    state,
    showTrigger = true,
    showDialog = true,
    onRequestConfirm,
    onCancel,
    onConfirm,
  }: Props = $props();

  const dialogOpen = $derived(
    state.phase === "confirming" ||
      state.phase === "submitting" ||
      state.phase === "error",
  );
  const submitting = $derived(state.phase === "submitting");
</script>

{#if showTrigger && state.visible}
  <button
    class="btn btn-subtle shutdown-trigger"
    type="button"
    onclick={onRequestConfirm}
    disabled={submitting}
  >
    关闭本机服务
  </button>
{/if}

{#if showDialog && dialogOpen}
  <div
    class="confirm-backdrop"
    role="dialog"
    aria-modal="true"
    aria-labelledby="shutdown-dialog-title"
  >
    <section class="card confirm-modal">
      <h2 id="shutdown-dialog-title">确认关闭本机服务？</h2>
      <p class="confirm-desc">
        关闭后本机 majjj 服务将立即优雅退出，当前牌桌上的所有局域网设备都会断开连接。
      </p>

      {#if state.errorMessage}
        <div class="banner banner-warn" role="alert">
          <span>{state.errorMessage}</span>
        </div>
      {/if}

      <div class="confirm-actions">
        <button
          class="btn btn-subtle"
          type="button"
          onclick={onCancel}
          disabled={submitting}
        >
          取消
        </button>
        <button
          class="btn btn-primary"
          type="button"
          onclick={onConfirm}
          disabled={submitting}
        >
          {submitting ? "正在关闭…" : "确认关闭"}
        </button>
      </div>
    </section>
  </div>
{/if}

<style>
  .shutdown-trigger {
    padding: 3px 10px;
    font-size: 12px;
  }

  .confirm-backdrop {
    position: fixed;
    inset: 0;
    z-index: 75;
    background: rgba(12, 22, 26, 0.78);
    backdrop-filter: blur(3px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
  }

  .confirm-modal {
    width: min(420px, 100%);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  h2 {
    margin: 0;
    font-size: 19px;
  }

  .confirm-desc {
    margin: 0;
    font-size: 14px;
    color: var(--ink-muted);
    line-height: 1.5;
  }

  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 10px;
  }
</style>
