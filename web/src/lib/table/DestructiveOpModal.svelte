<script lang="ts">
  import type { PendingDestructiveOp } from "../protocol/generated/PendingDestructiveOp";
  import type { Seat } from "../protocol/generated/Seat";
  import type { SeatInfo } from "../protocol/generated/SeatInfo";
  import { formatRelativeSeatParen } from "../protocol/seats";

  interface Props {
    op: PendingDestructiveOp;
    seats: [SeatInfo, SeatInfo, SeatInfo, SeatInfo];
    yourSeat: Seat | null;
    disabled?: boolean;
    onConfirm: (opId: string, accept: boolean) => void;
  }

  let { op, seats, yourSeat, disabled = false, onConfirm }: Props = $props();

  function seatDisplayName(seat: Seat): string {
    return formatRelativeSeatParen(seat, yourSeat, seats);
  }

  const titleText = $derived(
    op.kind === "EndFreeTable" ? "确认结束自由桌？" : "确认解散当前对局？",
  );

  const descriptionText = $derived(
    op.kind === "EndFreeTable"
      ? `${seatDisplayName(op.requested_by)} 请求结束本场自由桌并进入总结算页面。`
      : `${seatDisplayName(op.requested_by)} 请求中途解散当前未完对局并返回开桌大厅。`,
  );

  const isRequired = $derived(
    yourSeat !== null && op.required_seats.includes(yourSeat),
  );
  const alreadyConfirmed = $derived(
    yourSeat !== null && op.confirmed_seats.includes(yourSeat),
  );
</script>

<div class="confirm-backdrop" role="dialog" aria-modal="true" aria-labelledby="destructive-op-title">
  <section class="card confirm-modal">
    <h2 id="destructive-op-title">{titleText}</h2>
    <p class="confirm-desc">{descriptionText}</p>

    <div class="confirm-seats">
      {#each op.required_seats as reqSeat (reqSeat)}
        {@const done = op.confirmed_seats.includes(reqSeat)}
        <div class="confirm-seat-row">
          <span>{seatDisplayName(reqSeat)}</span>
          <span class="tag" class:tag-online={done}>
            {done ? "已同意" : "待确认"}
          </span>
        </div>
      {/each}
    </div>

    <div class="confirm-actions">
      {#if isRequired && !alreadyConfirmed}
        <button
          class="btn btn-subtle"
          type="button"
          {disabled}
          onclick={() => onConfirm(op.op_id, false)}
        >
          拒绝
        </button>
        <button
          class="btn btn-primary"
          type="button"
          {disabled}
          onclick={() => onConfirm(op.op_id, true)}
        >
          {op.kind === "EndFreeTable" ? "同意结束" : "同意解散"}
        </button>
      {:else}
        <span class="waiting-hint">等待其他在线玩家确认…</span>
        {#if isRequired}
          <button
            class="btn btn-subtle"
            type="button"
            {disabled}
            onclick={() => onConfirm(op.op_id, false)}
          >
            取消请求
          </button>
        {/if}
      {/if}
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

  .confirm-seats {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--bg-muted);
  }

  .confirm-seat-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 14px;
  }

  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .waiting-hint {
    margin-right: auto;
    font-size: 13px;
    color: var(--ink-muted);
  }
</style>
