<script lang="ts">
  import type { HandSettlement } from "../protocol/generated/HandSettlement";
  import type { MatchMode } from "../protocol/generated/MatchMode";
  import type { Seat } from "../protocol/generated/Seat";
  import type { SeatInfo } from "../protocol/generated/SeatInfo";
  import {
    ALL_SEATS,
    seatIndex,
    seatLabel,
    seatShortWind,
  } from "../protocol/seats";
  import { kongKindLabel, winKindLabel } from "../protocol/settlements";
  import TileBadge from "./TileBadge.svelte";
  import { formatRoundTitle, getTileLabel } from "./tiles";
  import {
    buildRevealedMeldItems,
    formatScoreDelta,
    formatSeatDisplayName,
  } from "./viewModel";

  interface Props {
    settlement: HandSettlement;
    seats: [SeatInfo, SeatInfo, SeatInfo, SeatInfo];
    mode: MatchMode;
    fanCap: number;
    yourSeat: Seat | null;
    readySeats: [boolean, boolean, boolean, boolean] | null;
    canConfirmNextHand: boolean;
    disabled?: boolean;
    destructivePending?: boolean;
    onConfirmNextHand: () => void;
    onRequestEndFreeTable: () => void;
  }

  let {
    settlement,
    seats,
    mode,
    fanCap,
    yourSeat,
    readySeats,
    canConfirmNextHand,
    disabled = false,
    destructivePending = false,
    onConfirmNextHand,
    onRequestEndFreeTable,
  }: Props = $props();

  function seatName(seat: Seat): string {
    return formatSeatDisplayName(seat, seats);
  }

  const outcomeBadge = $derived.by(() => {
    if (settlement.outcome === "DrawGame") {
      return "荒庄";
    }
    const { winners } = settlement.outcome.Win;
    if (winners.length > 1) {
      return "一炮多响";
    }
    return winners[0] ? winKindLabel(winners[0].win_kind) : "胡牌";
  });

  const viewerIndex = $derived(
    yourSeat !== null ? seatIndex(yourSeat) : -1,
  );
  const viewerReady = $derived(
    viewerIndex >= 0 && readySeats !== null ? readySeats[viewerIndex] : false,
  );
</script>

<div class="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="settlement-title">
  <section class="card modal-card settlement-modal">
    <header class="modal-head">
      <div>
        <h2 id="settlement-title">
          {formatRoundTitle({
            wind_round: settlement.wind_round,
            hand_number: settlement.hand_number,
            dealer_streak: settlement.dealer_streak,
            dealer: settlement.dealer,
            active_seat: null,
          })}
        </h2>
        <p class="modal-sub">
          庄家：{seatName(settlement.dealer)}
        </p>
      </div>
      <span class="tag tag-outcome">{outcomeBadge}</span>
    </header>

    {#if settlement.outcome === "DrawGame"}
      <div class="outcome-box draw-box">
        <strong>荒庄</strong>
        <span>牌墙摸空且无胜者；本小局已成立杠分照常清算。</span>
      </div>
    {:else}
      {@const win = settlement.outcome.Win}
      <div class="outcome-box win-box">
        <div class="win-summary-row">
          <div class="win-tile-chip">
            <span>胡牌：</span>
            <TileBadge tile={win.winning_tile} size="sm" highlighted />
            <strong>{getTileLabel(win.winning_tile)}</strong>
          </div>
          {#if win.discarder !== null}
            <span class="discarder-note">
              {win.winners[0]?.win_kind === "RobKongHu"
                ? `被抢杠胡座次：${seatName(win.discarder)}`
                : `点炮者：${seatName(win.discarder)}`}
            </span>
          {/if}
        </div>

        <div class="winners-list">
          {#each win.winners as winner (winner.seat)}
            <article class="winner-card">
              <div class="winner-head">
                <strong>{seatName(winner.seat)}</strong>
                <div class="winner-badges">
                  <span class="tag tag-self">{winKindLabel(winner.win_kind)}</span>
                  <span class="tag tag-fan">
                    {winner.total_fan} 番{winner.total_fan >= fanCap ? "（封顶）" : ""} · 底分 {winner.base_points}
                  </span>
                </div>
              </div>
              <div class="fan-items">
                {#each winner.items as item (`${winner.seat}:${item.code}`)}
                  <span class="fan-chip">{item.label} +{item.fan} 番</span>
                {/each}
              </div>
            </article>
          {/each}
        </div>
      </div>
    {/if}

    {#if settlement.kong_transfers.length > 0}
      <div class="kong-box">
        <h3>杠分明细</h3>
        <ul class="kong-list">
          {#each settlement.kong_transfers as kt, idx (`${kt.actor}:${kt.tile}:${idx}`)}
            <li>
              <span class="kong-head">
                {kongKindLabel(kt.kind)} · {getTileLabel(kt.tile)}（{seatName(kt.actor)}{kt.from_seat ? ` ← ${seatLabel(kt.from_seat)}` : ""}）
              </span>
              <span class="kong-deltas">
                {#each ALL_SEATS as s, i (s)}
                  <span>{seatShortWind(s)}{formatScoreDelta(kt.score_deltas[i])}</span>
                {/each}
              </span>
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    <div class="revealed-section">
      <h3>四家摊牌与积分变动</h3>
      <div class="revealed-grid">
        {#each settlement.revealed_hands as hand, idx (hand.seat)}
          {@const delta = settlement.score_deltas[idx]}
          {@const newScore = settlement.new_scores[idx]}
          {@const isReady = readySeats ? readySeats[idx] : false}
          {@const melds = buildRevealedMeldItems(hand.melds)}
          <article class="revealed-row" class:row-self={yourSeat === hand.seat}>
            <div class="revealed-top">
              <div class="seat-title-wrap">
                <strong>{seatName(hand.seat)}</strong>
                {#if hand.seat === settlement.dealer}
                  <span class="tag">庄</span>
                {/if}
                {#if readySeats !== null}
                  <span class="tag" class:tag-online={isReady}>
                    {isReady ? "已确认" : "待确认"}
                  </span>
                {/if}
              </div>
              <div class="score-wrap">
                <span
                  class="delta-val"
                  class:pos={delta > 0}
                  class:neg={delta < 0}
                >
                  {formatScoreDelta(delta)}
                </span>
                <span class="total-val">总分 {newScore}</span>
              </div>
            </div>

            <div class="revealed-tiles">
              {#if melds.length > 0}
                <div class="meld-cluster">
                  {#each melds as meld, mIdx (`${hand.seat}:m:${mIdx}`)}
                    <div class="meld-group">
                      {#each meld.tiles as slot, tIdx (`${hand.seat}:m:${mIdx}:${tIdx}`)}
                        <TileBadge tile={slot.tile} faceUp={slot.faceUp} size="xs" />
                      {/each}
                    </div>
                  {/each}
                </div>
              {/if}
              <div class="hand-cluster">
                {#each hand.closed_tiles as tile, tIdx (`${hand.seat}:c:${tIdx}:${tile}`)}
                  <TileBadge {tile} size="xs" />
                {/each}
                {#if hand.drawn_tile !== null}
                  <span class="drawn-sep"></span>
                  <TileBadge tile={hand.drawn_tile} size="xs" highlighted />
                {/if}
              </div>
            </div>
          </article>
        {/each}
      </div>
    </div>

    <footer class="modal-actions">
      {#if mode === "FreeTable"}
        <button
          class="btn btn-subtle"
          type="button"
          disabled={disabled || destructivePending}
          onclick={onRequestEndFreeTable}
        >
          结束对局
        </button>
      {/if}
      <button
        class="btn btn-primary next-btn"
        type="button"
        disabled={disabled || viewerReady || !canConfirmNextHand}
        onclick={onConfirmNextHand}
      >
        {viewerReady ? "已确认，等待其余玩家…" : "下一小局"}
      </button>
    </footer>
  </section>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: rgba(12, 22, 26, 0.72);
    backdrop-filter: blur(3px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    overflow-y: auto;
  }

  .settlement-modal {
    width: min(760px, 100%);
    max-height: 92dvh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .modal-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 10px;
  }

  .modal-head h2,
  h3 {
    margin: 0;
  }

  h3 {
    font-size: 14px;
    color: var(--ink-muted);
    margin-bottom: 8px;
  }

  .modal-sub {
    margin: 4px 0 0;
    font-size: 13px;
    color: var(--ink-muted);
  }

  .tag-outcome {
    background: var(--accent);
    color: #fff;
    font-size: 13px;
    padding: 4px 12px;
  }

  .outcome-box {
    border-radius: 12px;
    padding: 12px;
    border: 1px solid var(--border);
  }

  .draw-box {
    background: var(--bg-muted);
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }

  .win-box {
    background: #fff6ec;
    border-color: #e4c39c;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .win-summary-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .win-tile-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .discarder-note {
    font-size: 13px;
    color: var(--ink-muted);
  }

  .winners-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .winner-card {
    background: #fff;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .winner-head {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 6px;
  }

  .winner-badges {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .tag-fan {
    background: #fde9d0;
    color: #93450d;
  }

  .fan-items {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .fan-chip {
    font-size: 12px;
    font-weight: 600;
    padding: 3px 8px;
    border-radius: 6px;
    background: var(--bg-muted);
  }

  .kong-box {
    background: #fff;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 12px;
  }

  .kong-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
  }

  .kong-list li {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 8px;
  }

  .kong-deltas {
    display: flex;
    gap: 8px;
    font-variant-numeric: tabular-nums;
    color: var(--ink-muted);
  }

  .revealed-grid {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .revealed-row {
    background: #fff;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .row-self {
    border-color: var(--accent);
    background: #fffaf6;
  }

  .revealed-top {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }

  .seat-title-wrap {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .score-wrap {
    display: flex;
    align-items: baseline;
    gap: 10px;
    font-variant-numeric: tabular-nums;
  }

  .delta-val {
    font-weight: 800;
    font-size: 15px;
  }

  .delta-val.pos {
    color: var(--jade);
  }

  .delta-val.neg {
    color: var(--danger);
  }

  .total-val {
    font-size: 13px;
    color: var(--ink-muted);
  }

  .revealed-tiles {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .meld-cluster,
  .hand-cluster {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
  }

  .meld-group {
    display: inline-flex;
    gap: 1px;
    margin-right: 6px;
    padding: 2px;
    border-radius: 6px;
    background: var(--bg-muted);
  }

  .drawn-sep {
    width: 6px;
  }

  .modal-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 10px;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }

  .next-btn {
    min-width: 150px;
  }
</style>
