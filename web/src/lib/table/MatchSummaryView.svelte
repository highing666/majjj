<script lang="ts">
  import type { ActiveBanterVoiceState } from "../audio/audioTypes";
  import LobbyConfigFields from "../lobby/LobbyConfigFields.svelte";
  import { matchModeLabel } from "../lobby/lobbyConfig";
  import type { ActiveBanterMap } from "../presentation/presentationQueue";
  import type { HandSettlement } from "../protocol/generated/HandSettlement";
  import type { LobbyConfig } from "../protocol/generated/LobbyConfig";
  import type { MatchSummary } from "../protocol/generated/MatchSummary";
  import type { Seat } from "../protocol/generated/Seat";
  import type { SeatInfo } from "../protocol/generated/SeatInfo";
  import { formatRelativeSeatDisplay } from "../protocol/seats";
  import PostHandBanterStrip from "./PostHandBanterStrip.svelte";
  import { getOfflineHumanTakeOverSeats } from "./seatControl";
  import TileBadge from "./TileBadge.svelte";
  import { formatRoundTitle, getTileLabel } from "./tiles";
  import {
    buildMatchStandings,
    buildRevealedMeldItems,
    formatScoreDelta,
  } from "./viewModel";

  interface Props {
    summary: MatchSummary;
    config: LobbyConfig;
    seats: [SeatInfo, SeatInfo, SeatInfo, SeatInfo];
    yourSeat: Seat | null;
    lastSettlement: HandSettlement | null;
    activeBubbles?: ActiveBanterMap | null;
    activeBanterVoice?: ActiveBanterVoiceState | null;
    hideAiBanterText?: boolean;
    reducedEffects?: boolean;
    disabled?: boolean;
    clientIsRemote?: boolean;
    onUpdateConfig: (config: LobbyConfig) => void;
    onStartMatch: () => void;
    onReturnToLobby: () => void;
    onOpenHistory?: () => void;
    onTakeOverSeat?: (seatInfo: SeatInfo) => void;
  }

  let {
    summary,
    config,
    seats,
    yourSeat,
    lastSettlement,
    activeBubbles = null,
    activeBanterVoice = null,
    hideAiBanterText = false,
    reducedEffects = false,
    disabled = false,
    clientIsRemote = false,
    onUpdateConfig,
    onStartMatch,
    onReturnToLobby,
    onOpenHistory,
    onTakeOverSeat,
  }: Props = $props();

  const standings = $derived(buildMatchStandings(summary, seats, yourSeat));
  const modeLabel = $derived(matchModeLabel(summary.mode));
  const canOperate = $derived(yourSeat !== null && !disabled);
  const offlineTakeOverSeats = $derived(
    getOfflineHumanTakeOverSeats(seats, yourSeat, clientIsRemote),
  );
</script>

<section class="card summary-card">
  <header class="summary-head">
    <div>
      <h1>对局总结算</h1>
      <p class="summary-sub">
        {modeLabel} · 共完成 {summary.hands_played} 小局
      </p>
    </div>
    <div class="summary-head-actions">
      <span class="tag tag-online">已完赛</span>
      {#if onOpenHistory}
        <button
          class="btn btn-subtle history-trigger"
          type="button"
          onclick={onOpenHistory}
        >
          战绩
        </button>
      {/if}
    </div>
  </header>

  {#if activeBubbles}
    <PostHandBanterStrip
      {seats}
      {yourSeat}
      {activeBubbles}
      {activeBanterVoice}
      {hideAiBanterText}
      {reducedEffects}
    />
  {/if}

  {#if offlineTakeOverSeats.length > 0 && onTakeOverSeat}
    <div class="offline-takeover-box" role="status">
      <div class="offline-takeover-copy">
        <strong>发现已离线的人类座位</strong>
        <span>当前本机未入座，可经确认后跨设备接管离线座位参与下一桌或返回大厅。</span>
      </div>
      <div class="offline-takeover-list">
        {#each offlineTakeOverSeats as item (item.seatInfo.seat)}
          <div class="offline-takeover-item">
            <span>{item.displayName}</span>
            <button
              class="btn btn-primary offline-takeover-btn"
              type="button"
              {disabled}
              onclick={() => onTakeOverSeat(item.seatInfo)}
            >
              {item.actionLabel ?? "接管离线座位"}
            </button>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <div class="standings-table-wrap">
    <table class="standings-table">
      <thead>
        <tr>
          <th>名次</th>
          <th>座次与玩家</th>
          <th>起始分</th>
          <th>最终分</th>
          <th>净盈亏</th>
        </tr>
      </thead>
      <tbody>
        {#each standings as row (row.seat)}
          <tr class:self-row={yourSeat === row.seat}>
            <td class="rank-cell">#{row.rank}</td>
            <td>
              <div class="player-cell">
                {#if row.seatName}
                  <strong>{row.seatName}</strong>
                {/if}
                {#if row.nickname}
                  <span>{row.nickname}</span>
                {/if}
                {#if yourSeat === row.seat}
                  <span class="tag tag-self">本机</span>
                {:else if row.isAi}
                  <span class="tag">AI</span>
                {/if}
              </div>
            </td>
            <td class="num-cell">{row.initialScore}</td>
            <td class="num-cell"><strong>{row.finalScore}</strong></td>
            <td
              class="num-cell delta-cell"
              class:pos={row.netDelta > 0}
              class:neg={row.netDelta < 0}
            >
              {formatScoreDelta(row.netDelta)}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>

  {#if lastSettlement}
    <details class="last-hand-details">
      <summary>
        最后一小局详情（{formatRoundTitle({
          wind_round: lastSettlement.wind_round,
          hand_number: lastSettlement.hand_number,
          dealer_streak: lastSettlement.dealer_streak,
          dealer: lastSettlement.dealer,
          active_seat: null,
        })} · {lastSettlement.outcome === "DrawGame"
          ? "荒庄"
          : `胡牌 ${getTileLabel(lastSettlement.outcome.Win.winning_tile)}`}）
      </summary>
      <div class="last-hand-body">
        {#each lastSettlement.revealed_hands as hand, idx (hand.seat)}
          {@const melds = buildRevealedMeldItems(hand.melds)}
          <div class="last-hand-row">
            <div class="last-hand-seat">
              <strong>{formatRelativeSeatDisplay(hand.seat, yourSeat, seats)}</strong>
              <span>{formatScoreDelta(lastSettlement.score_deltas[idx])}</span>
            </div>
            <div class="last-hand-tiles">
              {#each melds as meld, mIdx (`${hand.seat}:fm:${mIdx}`)}
                <span class="meld-pill">
                  {#each meld.tiles as slot, tIdx (`${hand.seat}:fm:${mIdx}:${tIdx}`)}
                    <TileBadge tile={slot.tile} faceUp={slot.faceUp} size="xs" />
                  {/each}
                </span>
              {/each}
              {#each hand.closed_tiles as tile, tIdx (`${hand.seat}:fc:${tIdx}:${tile}`)}
                <TileBadge {tile} size="xs" />
              {/each}
              {#if hand.drawn_tile !== null}
                <TileBadge tile={hand.drawn_tile} size="xs" highlighted />
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </details>
  {/if}

  <section class="rematch-config">
    <div class="rematch-config-head">
      <h2>再开一桌规则设置</h2>
      {#if yourSeat === null}
        <span class="tag tag-offline">当前未入座，仅入座玩家可调整</span>
      {:else}
        <span class="rematch-config-note">修改后即时同步全桌</span>
      {/if}
    </div>
    <LobbyConfigFields
      {config}
      disabled={!canOperate}
      compact
      {onUpdateConfig}
    />
  </section>

  <footer class="summary-actions">
    <button
      class="btn btn-subtle"
      type="button"
      disabled={!canOperate}
      onclick={onReturnToLobby}
    >
      返回开桌大厅
    </button>
    <button
      class="btn btn-primary"
      type="button"
      disabled={!canOperate}
      onclick={onStartMatch}
    >
      原班人马再开一桌
    </button>
  </footer>
</section>

<style>
  .summary-card {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .summary-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 10px;
  }

  .summary-head-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .history-trigger {
    padding: 5px 10px;
    font-size: 12px;
  }

  h1 {
    margin: 0;
    font-size: 22px;
  }

  .summary-sub {
    margin: 4px 0 0;
    font-size: 14px;
    color: var(--ink-muted);
  }

  .standings-table-wrap {
    overflow-x: auto;
  }

  .standings-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 14px;
  }

  .standings-table th,
  .standings-table td {
    padding: 10px 8px;
    border-bottom: 1px solid var(--border);
    text-align: left;
  }

  .standings-table th {
    font-size: 12px;
    color: var(--ink-muted);
    font-weight: 600;
  }

  .self-row {
    background: #fff7f1;
  }

  .rank-cell {
    font-weight: 800;
    width: 56px;
  }

  .player-cell {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .num-cell {
    font-variant-numeric: tabular-nums;
  }

  .delta-cell {
    font-weight: 800;
  }

  .delta-cell.pos {
    color: var(--jade);
  }

  .delta-cell.neg {
    color: var(--danger);
  }

  .last-hand-details {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 12px;
    background: #fff;
  }

  .last-hand-details summary {
    cursor: pointer;
    font-size: 13px;
    font-weight: 600;
    color: var(--ink-muted);
  }

  .last-hand-body {
    margin-top: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .last-hand-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding-top: 6px;
    border-top: 1px dashed var(--border);
  }

  .last-hand-seat {
    min-width: 84px;
    display: flex;
    gap: 6px;
    font-size: 13px;
  }

  .last-hand-tiles {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
  }

  .meld-pill {
    display: inline-flex;
    gap: 1px;
    margin-right: 6px;
    padding: 2px;
    border-radius: 4px;
    background: var(--bg-muted);
  }

  .rematch-config {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--bg-muted);
  }

  .rematch-config-head {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }

  .rematch-config-head h2 {
    margin: 0;
    font-size: 15px;
  }

  .rematch-config-note {
    font-size: 12px;
    color: var(--ink-muted);
  }

  .summary-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 10px;
    padding-top: 6px;
  }

  .offline-takeover-box {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border-radius: 10px;
    border: 1px solid #e2b887;
    background: #fff7ea;
  }

  .offline-takeover-copy {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 13px;
  }

  .offline-takeover-list {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .offline-takeover-item {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 8px;
    background: #fff;
    border: 1px solid var(--border);
    font-size: 13px;
  }

  .offline-takeover-btn {
    padding: 5px 10px;
    font-size: 12px;
  }
</style>
