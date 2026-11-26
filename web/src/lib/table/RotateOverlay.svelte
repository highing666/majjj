<script lang="ts">
  interface Props {
    visible: boolean;
    reducedMotion?: boolean;
    onBypass: () => void;
  }

  let { visible, reducedMotion = false, onBypass }: Props = $props();
</script>

{#if visible}
  <div class="rotate-overlay" role="dialog" aria-modal="true" aria-label="横屏引导">
    <div class="rotate-card">
      <div
        class="phone-icon-wrap"
        class:animate={!reducedMotion}
        aria-hidden="true"
      >
        <svg viewBox="0 0 64 64" width="60" height="60">
          <rect
            x="19"
            y="8"
            width="26"
            height="46"
            rx="5"
            fill="#10252b"
            stroke="#f5be58"
            stroke-width="2.5"
          />
          <rect x="22" y="14" width="20" height="33" rx="2" fill="#1b3e47" />
          <circle cx="32" cy="50" r="1.8" fill="#f5be58" />
          <path
            d="M52 24 C56 30, 54 38, 48 42"
            fill="none"
            stroke="#f5be58"
            stroke-width="2.2"
            stroke-linecap="round"
          />
          <polygon points="46,44 48,38 52,42" fill="#f5be58" />
        </svg>
      </div>

      <h2>请横屏旋转手机打牌</h2>
      <p>横屏下 14 张手牌与四家牌河更舒展易点按，告别误触与遮挡。</p>
    </div>

    <button class="bypass-btn" type="button" onclick={onBypass}>
      临时竖屏查看
    </button>
  </div>
{/if}

<style>
  .rotate-overlay {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 24px;
    background: rgba(9, 20, 24, 0.94);
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    color: #f5eedf;
    text-align: center;
  }

  .rotate-card {
    max-width: 280px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  .phone-icon-wrap {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 78px;
    height: 78px;
    border-radius: 20px;
    background: rgba(245, 190, 88, 0.12);
    border: 1.5px solid rgba(245, 190, 88, 0.45);
  }

  .phone-icon-wrap.animate svg {
    animation: tilt-landscape 2.4s ease-in-out infinite;
  }

  @media (prefers-reduced-motion: reduce) {
    .phone-icon-wrap.animate svg {
      animation: none;
    }
  }

  @keyframes tilt-landscape {
    0%,
    20% {
      transform: rotate(0deg);
    }
    55%,
    80% {
      transform: rotate(-90deg);
    }
    100% {
      transform: rotate(0deg);
    }
  }

  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 800;
    color: #f5be58;
  }

  p {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: #cbd5e1;
  }

  .bypass-btn {
    position: absolute;
    right: max(16px, env(safe-area-inset-right));
    bottom: max(28px, env(safe-area-inset-bottom));
    padding: 7px 14px;
    border-radius: 999px;
    border: 1px solid rgba(212, 168, 87, 0.45);
    background: rgba(16, 36, 43, 0.88);
    color: #e2e8f0;
    font-size: 12px;
    font-weight: 600;
  }

  .bypass-btn:hover {
    background: rgba(30, 58, 68, 0.95);
    color: #f5be58;
  }
</style>
