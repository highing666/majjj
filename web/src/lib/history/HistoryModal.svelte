<script lang="ts">
  import type { HistoryState } from "./historyClient";
  import {
    buildHandHistoryRows,
    buildMatchHistoryRows,
    buildPlayerSummaryCards,
  } from "./historyViewModel";

  type HistoryTab = "players" | "matches" | "hands";

  interface Props {
    state: HistoryState;
    onClose: () => void;
    onRefresh: () => void;
    onExport: () => void;
    onRequestClear: () => void;
    onCancelClear: () => void;
    onConfirmClear: () => void;
  }

  let {
    state: historyState,
    onClose,
    onRefresh,
    onExport,
    onRequestClear,
    onCancelClear,
    onConfirmClear,
  }: Props = $props();

  let activeTab = $state<HistoryTab>("players");

  const playerCards = $derived(
    historyState.stats
      ? buildPlayerSummaryCards(historyState.stats.players)
      : [],
  );
  const matchRows = $derived(
    historyState.stats
      ? buildMatchHistoryRows(historyState.stats.recent_matches)
      : [],
  );
  const handRows = $derived(
    historyState.stats
      ? buildHandHistoryRows(historyState.stats.recent_hands)
      : [],
  );

  const clearDialogOpen = $derived(
    historyState.clearPhase === "confirming" ||
      historyState.clearPhase === "submitting" ||
      historyState.clearPhase === "error",
  );
  const isSubmittingClear = $derived(historyState.clearPhase === "submitting");
  const isExporting = $derived(historyState.exportPhase === "exporting");

  function handleKeyDown(event: KeyboardEvent): void {
    if (!historyState.open) {
      return;
    }
    if (event.key === "Escape" && !isSubmittingClear) {
      event.preventDefault();
      if (clearDialogOpen) {
        onCancelClear();
      } else {
        onClose();
      }
    }
  }

  function handleBackdropClick(event: MouseEvent): void {
    if (event.target === event.currentTarget && !isSubmittingClear) {
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if historyState.open}
  <div
    class="history-backdrop"
    role="presentation"
    onclick={handleBackdropClick}
  >
    <div
      class="card history-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="history-modal-title"
    >
      <header class="history-header">
        <div>
          <h2 id="history-modal-title">战绩总览</h2>
          <p class="history-subtitle">
            按规范化昵称合并统计成员长期战绩，支持导出 JSON 备份与二次确认清空。
          </p>
        </div>
        <div class="history-header-actions">
          <button
            class="btn btn-subtle compact-btn"
            type="button"
            disabled={historyState.loading || isSubmittingClear}
            onclick={onRefresh}
          >
            {historyState.loading ? "刷新中…" : "刷新"}
          </button>
          <button
            class="btn btn-subtle compact-btn"
            type="button"
            aria-label="关闭战绩"
            disabled={isSubmittingClear}
            onclick={onClose}
          >
            关闭
          </button>
        </div>
      </header>

      <div class="history-toolbar">
        <div class="toolbar-meta">
          {#if historyState.stats}
            <span class="tag">完赛对局 {historyState.stats.total_matches} 场</span>
            <span class="tag">已结算小局 {historyState.stats.total_hands} 小局</span>
            <span class="tag">成员 {playerCards.length} 人</span>
          {/if}
        </div>
        <div class="toolbar-actions">
          <button
            class="btn btn-jade compact-btn"
            type="button"
            disabled={isExporting || isSubmittingClear}
            onclick={onExport}
          >
            {isExporting ? "正在导出…" : "导出 JSON 备份"}
          </button>
          <button
            class="btn btn-subtle danger-btn compact-btn"
            type="button"
            disabled={historyState.loading || isSubmittingClear}
            onclick={onRequestClear}
          >
            清空战绩
          </button>
        </div>
      </div>

      {#if historyState.exportNotice}
        <div class="banner banner-ok" role="status">
          <span>{historyState.exportNotice}</span>
        </div>
      {/if}

      {#if historyState.exportErrorMessage}
        <div class="banner banner-warn" role="alert">
          <span>{historyState.exportErrorMessage}</span>
        </div>
      {/if}

      {#if historyState.clearNotice}
        <div class="banner banner-ok" role="status">
          <span>{historyState.clearNotice}</span>
        </div>
      {/if}

      {#if clearDialogOpen}
        <div class="clear-confirm-box" role="alertdialog" aria-labelledby="clear-confirm-title">
          <strong id="clear-confirm-title">确认清空全部历史战绩？</strong>
          <p class="clear-confirm-desc">
            此操作将永久清空已记录的完赛对局与小局明细，无法撤销；您的默认玩法配置、牌桌节奏与常用昵称会完整保留。
          </p>
          {#if historyState.clearErrorMessage}
            <div class="banner banner-warn" role="alert">
              <span>{historyState.clearErrorMessage}</span>
            </div>
          {/if}
          <div class="clear-confirm-actions">
            <button
              class="btn btn-subtle compact-btn"
              type="button"
              disabled={isSubmittingClear}
              onclick={onCancelClear}
            >
              取消
            </button>
            <button
              class="btn btn-danger compact-btn"
              type="button"
              disabled={isSubmittingClear}
              onclick={onConfirmClear}
            >
              {isSubmittingClear ? "正在清空…" : "确认清空战绩"}
            </button>
          </div>
        </div>
      {/if}

      {#if historyState.errorMessage}
        <div class="banner banner-warn" role="alert">
          <span>{historyState.errorMessage}</span>
          <button
            class="btn btn-subtle compact-btn"
            type="button"
            onclick={onRefresh}
          >
            重试
          </button>
        </div>
      {/if}

      {#if historyState.loading && historyState.stats === null}
        <div class="empty-box">正在加载战绩统计…</div>
      {:else if historyState.stats !== null}
        <div class="history-tabs" role="tablist" aria-label="战绩分类">
          <button
            class="tab-btn"
            class:tab-active={activeTab === "players"}
            type="button"
            role="tab"
            aria-selected={activeTab === "players"}
            onclick={() => (activeTab = "players")}
          >
            成员总览（{playerCards.length}）
          </button>
          <button
            class="tab-btn"
            class:tab-active={activeTab === "matches"}
            type="button"
            role="tab"
            aria-selected={activeTab === "matches"}
            onclick={() => (activeTab = "matches")}
          >
            近期对局（{matchRows.length}）
          </button>
          <button
            class="tab-btn"
            class:tab-active={activeTab === "hands"}
            type="button"
            role="tab"
            aria-selected={activeTab === "hands"}
            onclick={() => (activeTab = "hands")}
          >
            近期小局（{handRows.length}）
          </button>
        </div>

        <div class="history-content">
          {#if activeTab === "players"}
            {#if playerCards.length === 0}
              <div class="empty-box">
                暂无成员战绩记录，完成一小局后即可在此查看统计。
              </div>
            {:else}
              <div class="players-grid">
                {#each playerCards as card (card.nickname)}
                  <article class="record-card player-card">
                    <div class="record-head">
                      <strong class="player-name">{card.nickname}</strong>
                      <span
                        class="score-badge"
                        class:pos={card.cumulativeNetTone === "pos"}
                        class:neg={card.cumulativeNetTone === "neg"}
                      >
                        累计 {card.cumulativeNetScoreText}
                      </span>
                    </div>
                    <div class="player-metrics">
                      <div class="metric-cell">
                        <span class="metric-label">完赛对局</span>
                        <strong>{card.completedMatches} 场</strong>
                      </div>
                      <div class="metric-cell">
                        <span class="metric-label">参与小局</span>
                        <strong>
                          {card.settledHands}
                          {#if card.abortedHands > 0}
                            <span class="metric-sub">（含解散 {card.abortedHands}）</span>
                          {/if}
                        </strong>
                      </div>
                      <div class="metric-cell">
                        <span class="metric-label">胡牌 / 自摸</span>
                        <strong>{card.wins} / {card.tsumoWins}</strong>
                      </div>
                      <div class="metric-cell">
                        <span class="metric-label">点炮次数</span>
                        <strong>{card.dealIns}</strong>
                      </div>
                      <div class="metric-cell">
                        <span class="metric-label">AI 托管小局</span>
                        <strong>{card.aiAssistedHands}</strong>
                      </div>
                      <div class="metric-cell">
                        <span class="metric-label">对局均分 / 小局均分</span>
                        <strong>{card.averageNetScoreText} / {card.averageHandNetScoreText}</strong>
                      </div>
                    </div>
                  </article>
                {/each}
              </div>
            {/if}
          {:else if activeTab === "matches"}
            {#if matchRows.length === 0}
              <div class="empty-box">
                暂无已完赛对局记录（中途解散的东风战与 0 小局自由桌不计入完赛对局）。
              </div>
            {:else}
              <div class="records-list">
                {#each matchRows as row (row.matchId)}
                  <article class="record-card">
                    <div class="record-head">
                      <div class="record-tags">
                        <span class="tag tag-online">{row.modeLabel}</span>
                        <span class="tag">共 {row.handsPlayed} 小局</span>
                        {#if row.assistedByAi}
                          <span class="tag tag-offline">AI 托管</span>
                        {/if}
                      </div>
                      <span class="record-time">{row.timeText}</span>
                    </div>
                    <div class="seat-deltas-grid">
                      {#each row.seats as seat (seat.seat)}
                        <div class="seat-delta-item">
                          <div class="seat-delta-name">
                            <span>{seat.nickname}</span>
                            {#if seat.isAi}
                              <span class="tag mini-tag">AI</span>
                            {/if}
                            {#if seat.assistedByAi}
                              <span class="tag tag-offline mini-tag">AI 托管</span>
                            {/if}
                          </div>
                          <div class="seat-delta-score">
                            <span class="final-score">{seat.finalScore} 分</span>
                            <strong
                              class:pos={seat.netTone === "pos"}
                              class:neg={seat.netTone === "neg"}
                            >
                              {seat.netDeltaText}
                            </strong>
                          </div>
                        </div>
                      {/each}
                    </div>
                  </article>
                {/each}
              </div>
            {/if}
          {:else}
            {#if handRows.length === 0}
              <div class="empty-box">暂无已结算小局记录。</div>
            {:else}
              <div class="records-list">
                {#each handRows as row (row.key)}
                  <article class="record-card">
                    <div class="record-head">
                      <div class="record-tags">
                        <strong>{row.roundTitle}</strong>
                        <span class="tag">{row.modeLabel}</span>
                        <span class="tag tag-online">
                          {row.outcomeLabel}{row.winningTileLabel ? ` · ${row.winningTileLabel}` : ""}
                        </span>
                        {#if row.assistedByAi}
                          <span class="tag tag-offline">AI 托管</span>
                        {/if}
                        {#if row.aborted}
                          <span class="tag tag-danger">中途解散</span>
                        {/if}
                      </div>
                      <span class="record-time">{row.timeText}</span>
                    </div>

                    {#if row.winners.length > 0}
                      <div class="hand-detail-block">
                        {#each row.winners as winner (winner.seat)}
                          <div class="winner-line">
                            <span>
                              <strong>{winner.displayName}</strong>
                              <span>{winner.winKindLabel}（{winner.totalFan} 番）</span>
                            </span>
                            <span class="fan-pills">
                              {#each winner.fanItems as item, idx (`${winner.seat}:${item.code}:${idx}`)}
                                <span class="fan-pill">{item.label} {item.fan} 番</span>
                              {/each}
                            </span>
                          </div>
                        {/each}
                        {#if row.discarderLabel}
                          <div class="discarder-note">点炮者：{row.discarderLabel}</div>
                        {/if}
                      </div>
                    {/if}

                    {#if row.kongTransfers.length > 0}
                      <div class="hand-detail-block kong-block">
                        <span class="detail-label">杠分：</span>
                        <div class="kong-pills">
                          {#each row.kongTransfers as kt, idx (`${row.key}:k:${idx}`)}
                            <span class="tag">
                              {kt.actorLabel} {kt.kindLabel} {kt.tileLabel}（{kt.actorDeltaText}）{kt.fromLabel ? ` ← ${kt.fromLabel}` : ""}
                            </span>
                          {/each}
                        </div>
                      </div>
                    {/if}

                    <div class="seat-deltas-grid">
                      {#each row.seatDeltas as sd (sd.seat)}
                        <div class="seat-delta-item">
                          <div class="seat-delta-name">
                            <span>{sd.nickname}</span>
                            {#if sd.isAi}
                              <span class="tag mini-tag">AI</span>
                            {/if}
                            {#if sd.assistedByAi}
                              <span class="tag tag-offline mini-tag">AI 托管</span>
                            {/if}
                            {#if sd.tookOverAi}
                              <span class="tag tag-self mini-tag">热接管</span>
                            {/if}
                          </div>
                          <strong
                            class="num-delta"
                            class:pos={sd.tone === "pos"}
                            class:neg={sd.tone === "neg"}
                          >
                            {sd.deltaText}
                          </strong>
                        </div>
                      {/each}
                    </div>
                  </article>
                {/each}
              </div>
            {/if}
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .history-backdrop {
    position: fixed;
    inset: 0;
    z-index: 72;
    background: rgba(12, 22, 26, 0.74);
    backdrop-filter: blur(3px);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
  }

  .history-modal {
    width: min(820px, 100%);
    max-height: 90dvh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .history-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 12px;
  }

  .history-header-actions {
    display: flex;
    gap: 8px;
    flex: 0 0 auto;
  }

  h2 {
    margin: 0;
    font-size: 20px;
  }

  .history-subtitle {
    margin: 4px 0 0;
    font-size: 13px;
    color: var(--ink-muted);
  }

  .compact-btn {
    padding: 6px 11px;
    font-size: 13px;
  }

  .danger-btn {
    color: var(--danger);
    border-color: #e6b8b8;
  }

  .btn-danger {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }

  .history-toolbar {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 12px;
    background: var(--bg-muted);
    border: 1px solid var(--border);
  }

  .toolbar-meta,
  .toolbar-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .banner-ok {
    background: var(--jade-soft);
    border-color: #b7dfd0;
    color: var(--jade);
  }

  .clear-confirm-box {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px;
    border-radius: 12px;
    background: var(--danger-soft);
    border: 1px solid #eab5b5;
  }

  .clear-confirm-desc {
    margin: 0;
    font-size: 13px;
    color: var(--ink);
    line-height: 1.5;
  }

  .clear-confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .history-tabs {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
  }

  .tab-btn {
    border: 1px solid var(--border);
    border-radius: 10px;
    background: #fff;
    padding: 8px 10px;
    font-size: 13px;
    font-weight: 600;
    color: var(--ink-muted);
  }

  .tab-active {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .history-content {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .empty-box {
    padding: 24px 16px;
    text-align: center;
    border-radius: 12px;
    background: var(--bg-muted);
    color: var(--ink-muted);
    font-size: 14px;
  }

  .players-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 10px;
  }

  .records-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .record-card {
    border: 1px solid var(--border);
    border-radius: 12px;
    background: #fff;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .record-head {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }

  .record-tags {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .record-time {
    font-size: 12px;
    color: var(--ink-muted);
    font-variant-numeric: tabular-nums;
  }

  .player-name {
    font-size: 16px;
  }

  .score-badge {
    font-weight: 800;
    font-size: 14px;
    padding: 3px 9px;
    border-radius: 999px;
    background: var(--bg-muted);
    font-variant-numeric: tabular-nums;
  }

  .player-metrics {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  .metric-cell {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px 8px;
    border-radius: 8px;
    background: var(--bg-muted);
    font-size: 13px;
  }

  .metric-label {
    font-size: 11px;
    color: var(--ink-muted);
  }

  .metric-sub {
    font-size: 11px;
    font-weight: 500;
    color: var(--ink-muted);
  }

  .seat-deltas-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  .seat-delta-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--bg-muted);
    font-size: 13px;
  }

  .seat-delta-name {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }

  .seat-delta-score {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 0 0 auto;
    font-variant-numeric: tabular-nums;
  }

  .final-score {
    font-size: 12px;
    color: var(--ink-muted);
  }

  .num-delta {
    font-variant-numeric: tabular-nums;
    flex: 0 0 auto;
  }

  .pos {
    color: var(--jade);
  }

  .neg {
    color: var(--danger);
  }

  .tag-danger {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .mini-tag {
    font-size: 11px;
    padding: 1px 6px;
  }

  .hand-detail-block {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border-radius: 8px;
    background: #faf5eb;
    border: 1px dashed var(--border);
    font-size: 13px;
  }

  .winner-line {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 6px;
  }

  .fan-pills,
  .kong-pills {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .fan-pill {
    font-size: 11px;
    font-weight: 600;
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
  }

  .discarder-note,
  .detail-label {
    font-size: 12px;
    color: var(--ink-muted);
  }

  .kong-block {
    flex-direction: row;
    flex-wrap: wrap;
    align-items: center;
  }

  @media (max-width: 560px) {
    .history-tabs {
      grid-template-columns: 1fr;
    }

    .seat-deltas-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
