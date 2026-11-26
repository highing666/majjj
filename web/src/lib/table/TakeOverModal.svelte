<script lang="ts">
  import type { Seat } from "../protocol/generated/Seat";
  import type { SeatInfo } from "../protocol/generated/SeatInfo";
  import { getTakeOverModalCopy, type TakeOverKind } from "./seatControl";

  interface Props {
    seatInfo: SeatInfo;
    kind: TakeOverKind;
    disabled?: boolean;
    onConfirm: (seat: Seat) => void;
    onCancel: () => void;
  }

  let {
    seatInfo,
    kind,
    disabled = false,
    onConfirm,
    onCancel,
  }: Props = $props();

  const copy = $derived(getTakeOverModalCopy(seatInfo, kind));
</script>

<div
  class="confirm-backdrop"
  role="dialog"
  aria-modal="true"
  aria-labelledby="takeover-modal-title"
>
  <section class="card confirm-modal">
    <h2 id="takeover-modal-title">{copy.title}</h2>
    <p class="confirm-desc">{copy.description}</p>

    <div class="confirm-actions">
      <button
        class="btn btn-subtle"
        type="button"
        {disabled}
        onclick={onCancel}
      >
        取消
      </button>
      <button
        class="btn btn-primary"
        type="button"
        {disabled}
        onclick={() => onConfirm(seatInfo.seat)}
      >
        {copy.confirmLabel}
      </button>
    </div>
  </section>
</div>

<style>
  .confirm-backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: rgba(12, 22, 26, 0.78);
    backdrop-filter: blur(3px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
  }

  .confirm-modal {
    width: min(440px, 100%);
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
    flex-wrap: wrap;
  }
</style>
