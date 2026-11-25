<script lang="ts">
  interface Props {
    visible: boolean;
    onResume: () => void;
  }

  let { visible, onResume }: Props = $props();
</script>

{#if visible}
  <button
    class="audio-resume-capsule"
    type="button"
    aria-label="点按恢复音效"
    onclick={onResume}
  >
    🔇 点按恢复音效
  </button>
{/if}

<style>
  .audio-resume-capsule {
    position: fixed;
    left: max(12px, env(safe-area-inset-left));
    bottom: max(12px, env(safe-area-inset-bottom));
    z-index: 45;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 999px;
    border: 1px solid rgba(212, 168, 87, 0.75);
    background: rgba(16, 32, 38, 0.92);
    color: #f5eedf;
    font-size: 12px;
    font-weight: 600;
    line-height: 1.2;
    box-shadow: 0 6px 16px rgba(10, 18, 24, 0.28);
    backdrop-filter: blur(4px);
    transition:
      transform 0.1s ease,
      border-color 0.15s ease,
      background-color 0.15s ease;
  }

  .audio-resume-capsule:hover {
    border-color: #f5be58;
    background: rgba(20, 40, 46, 0.96);
  }

  .audio-resume-capsule:active {
    transform: translateY(1px);
  }
</style>
