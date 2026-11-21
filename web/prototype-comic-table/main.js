// PROTOTYPE Shell: ?variant= router, Floating Bottom Switcher, Sandbox Toolbar, 34-Tile Overview, Audio Tester, iPhone Preview, State Inspector & Settlement Modal.

import { tableStore } from './state.js';
import { audioRouter } from './audio.js';
import { triggerComicImpact } from './effects.js';
import { SUIT_GROUPS, createHandTile, createMeldGroup, getTileName } from './tiles.js';
import { variantA } from './variants/variantA.js';
import { variantB } from './variants/variantB.js';
import { variantC } from './variants/variantC.js';

const PROTOTYPE_MODE = true;
const VARIANTS = [variantA, variantB, variantC];
const VARIANT_MAP = Object.fromEntries(VARIANTS.map((v) => [v.id, v]));

function getInitialVariantId() {
  const params = new URLSearchParams(window.location.search);
  const v = (params.get('variant') || 'A').toUpperCase();
  return VARIANT_MAP[v] ? v : 'A';
}

function setVariantQueryParam(id) {
  const url = new URL(window.location.href);
  url.searchParams.set('variant', id);
  window.history.replaceState(null, '', url.toString());
}

const stageRoot = document.getElementById('variant-stage');
const stageViewport = document.getElementById('stage-viewport');
const switcherMount = document.getElementById('prototype-switcher');
const toolbarMount = document.getElementById('sandbox-toolbar');
const inspectorMount = document.getElementById('state-inspector');
const settlementMount = document.getElementById('settlement-modal');
const atlasModalMount = document.getElementById('tile-atlas-modal');
const audioModalMount = document.getElementById('audio-popover');

let currentVariantId = getInitialVariantId();
let activeVariant = null;
let toolbarOpen = false;
let inspectorOpen = false;
let audioPopoverOpen = false;
let atlasOpen = false;
let iphonePreview = false;

const ctx = {
  actions: {
    discardTile: (indexOrDrawn, slam) => tableStore.discardTile(indexOrDrawn, slam),
    chooseCall: (type, tile) => tableStore.chooseCall(type, tile),
  },
};

// Unlock Web Audio synchronously inside user gesture
function bindGlobalAudioUnlock() {
  const unlock = () => {
    audioRouter.unlockFromGesture();
  };
  window.addEventListener('pointerdown', unlock, { passive: true });
  window.addEventListener('keydown', unlock, { passive: true });
}

function mountVariant(id) {
  const next = VARIANT_MAP[id] || variantA;
  if (activeVariant && activeVariant.unmount) {
    activeVariant.unmount();
  }
  currentVariantId = next.id;
  setVariantQueryParam(next.id);
  activeVariant = next;
  activeVariant.mount(stageRoot, ctx);
  activeVariant.render(tableStore.getState(), ctx);
  renderSwitcher();
  if (atlasOpen) renderAtlasModal();
  if (tableStore.getState().phase === 'settlement') {
    renderSettlementModal(tableStore.getState());
  }
}

function cycleVariant(delta) {
  const idx = VARIANTS.findIndex((v) => v.id === currentVariantId);
  const nextIdx = (idx + delta + VARIANTS.length) % VARIANTS.length;
  mountVariant(VARIANTS[nextIdx].id);
}

function renderSwitcher() {
  if (!PROTOTYPE_MODE || !switcherMount) {
    if (switcherMount) switcherMount.hidden = true;
    return;
  }
  const current = VARIANT_MAP[currentVariantId];
  switcherMount.innerHTML = `
    <div class="proto-switcher-pill" role="region" aria-label="原型方案切换器">
      <span class="proto-badge">PROTOTYPE</span>
      <button type="button" class="proto-arrow-btn" data-prev aria-label="上一个风格 (←)">←</button>
      <span class="proto-current-label">${current.shortLabel}</span>
      <button type="button" class="proto-arrow-btn" data-next aria-label="下一个风格 (→)">→</button>
    </div>
  `;
  switcherMount.querySelector('[data-prev]')?.addEventListener('click', () => cycleVariant(-1));
  switcherMount.querySelector('[data-next]')?.addEventListener('click', () => cycleVariant(1));
}

function renderToolbar() {
  if (!toolbarMount) return;
  const s = tableStore.getState();
  toolbarMount.innerHTML = `
    <div class="sandbox-bar ${toolbarOpen ? 'is-open' : ''}">
      <button type="button" class="sandbox-toggle-pill" data-toggle-bar>
        <span class="sandbox-dot"></span>
        <span>原型沙盒</span>
        <span class="sandbox-caret">${toolbarOpen ? '▴' : '▾'}</span>
      </button>

      <div class="sandbox-drawer" ${toolbarOpen ? '' : 'hidden'}>
        <div class="sandbox-group">
          <span class="sandbox-group-label">场景触发</span>
          <button type="button" class="sb-btn ${s.scenarioKey === 'normal' ? 'is-on' : ''}" data-sc="normal">正常摸打</button>
          <button type="button" class="sb-btn ${s.scenarioKey === 'pon' ? 'is-on' : ''}" data-sc="pon">碰窗口</button>
          <button type="button" class="sb-btn ${s.scenarioKey === 'kong' ? 'is-on' : ''}" data-sc="kong">暗杠/补杠</button>
          <button type="button" class="sb-btn ${s.scenarioKey === 'mingkong' ? 'is-on' : ''}" data-sc="mingkong">明杠</button>
          <button type="button" class="sb-btn ${s.scenarioKey === 'robkong' ? 'is-on' : ''}" data-sc="robkong">抢杠胡</button>
          <button type="button" class="sb-btn ${s.scenarioKey === 'settlement' ? 'is-on' : ''}" data-sc="settlement">小局结算</button>
        </div>
        <div class="sandbox-group">
          <span class="sandbox-group-label">验收工具</span>
          <button type="button" class="sb-btn ${atlasOpen ? 'is-on' : ''}" data-open-atlas>34张牌面全览</button>
          <button type="button" class="sb-btn ${audioPopoverOpen ? 'is-on' : ''}" data-open-audio>音效试听</button>
          <button type="button" class="sb-btn ${iphonePreview ? 'is-on' : ''}" data-toggle-iphone>iPhone 视口 (844×390)</button>
          <button type="button" class="sb-btn ${inspectorOpen ? 'is-on' : ''}" data-toggle-inspector>状态检视</button>
        </div>
      </div>
    </div>
  `;

  toolbarMount.querySelector('[data-toggle-bar]')?.addEventListener('click', () => {
    toolbarOpen = !toolbarOpen;
    renderToolbar();
  });

  toolbarMount.querySelectorAll('[data-sc]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const sc = btn.dataset.sc;
      if (sc === 'normal') tableStore.loadNormalTurn(true);
      else if (sc === 'pon') tableStore.loadPonScenario();
      else if (sc === 'kong') tableStore.loadKongScenario('self');
      else if (sc === 'mingkong') tableStore.loadKongScenario('mingkong');
      else if (sc === 'robkong') tableStore.loadRobKongScenario();
      else if (sc === 'settlement') tableStore.loadSettlementScenario('自摸');
    });
  });

  toolbarMount.querySelector('[data-open-atlas]')?.addEventListener('click', () => {
    atlasOpen = !atlasOpen;
    renderToolbar();
    renderAtlasModal();
  });

  toolbarMount.querySelector('[data-open-audio]')?.addEventListener('click', () => {
    audioPopoverOpen = !audioPopoverOpen;
    renderToolbar();
    renderAudioPopover();
  });

  toolbarMount.querySelector('[data-toggle-iphone]')?.addEventListener('click', () => {
    iphonePreview = !iphonePreview;
    stageViewport.classList.toggle('iphone-preview', iphonePreview);
    renderToolbar();
  });

  toolbarMount.querySelector('[data-toggle-inspector]')?.addEventListener('click', () => {
    inspectorOpen = !inspectorOpen;
    renderToolbar();
    renderInspector();
  });
}

function renderAtlasModal() {
  if (!atlasModalMount) return;
  if (!atlasOpen) {
    atlasModalMount.hidden = true;
    atlasModalMount.innerHTML = '';
    return;
  }
  atlasModalMount.hidden = false;
  atlasModalMount.innerHTML = `
    <div class="proto-modal-backdrop" data-close-atlas>
      <div class="proto-modal-card proto-atlas-card" role="dialog" aria-label="34张牌面全览">
        <div class="proto-modal-head">
          <div>
            <h3>34 张标准推倒胡牌面全览（当前方案：${VARIANT_MAP[currentVariantId].name}）</h3>
            <p class="proto-modal-sub">上方为手牌清晰度，下方为牌河缩略尺寸，便于在 Mac Retina 与 iPhone 视口下直观检验辨识度</p>
          </div>
          <button type="button" class="proto-close-btn" data-close-atlas>关闭</button>
        </div>
        <div class="proto-atlas-body" data-atlas-body></div>
      </div>
    </div>
  `;

  const body = atlasModalMount.querySelector('[data-atlas-body]');
  SUIT_GROUPS.forEach((group) => {
    const sec = document.createElement('section');
    sec.className = 'atlas-group';
    sec.innerHTML = `<h4>${group.label}</h4>`;
    const grid = document.createElement('div');
    grid.className = 'atlas-grid';

    group.tiles.forEach((tileCode) => {
      const cell = document.createElement('div');
      cell.className = 'atlas-cell';
      const heroTile = createHandTile(tileCode, { variant: currentVariantId, interactive: false, size: 'hand' });
      const miniTile = createHandTile(tileCode, { variant: currentVariantId, interactive: false, size: 'river-preview' });
      const caption = document.createElement('span');
      caption.className = 'atlas-caption';
      caption.textContent = getTileName(tileCode);
      cell.append(heroTile, miniTile, caption);
      grid.appendChild(cell);
    });

    sec.appendChild(grid);
    body.appendChild(sec);
  });

  atlasModalMount.querySelectorAll('[data-close-atlas]').forEach((el) => {
    el.addEventListener('click', (e) => {
      if (e.target === el || el.tagName === 'BUTTON') {
        atlasOpen = false;
        renderToolbar();
        renderAtlasModal();
      }
    });
  });
}

function renderAudioPopover() {
  if (!audioModalMount) return;
  if (!audioPopoverOpen) {
    audioModalMount.hidden = true;
    audioModalMount.innerHTML = '';
    return;
  }
  audioModalMount.hidden = false;
  const st = audioRouter.getAudioStatus();
  const sfxItems = [
    { type: 'draw', label: '摸牌' },
    { type: 'hover', label: '悬停轻触' },
    { type: 'discard_soft', label: '轻放出牌' },
    { type: 'discard_slam', label: '重拍甩牌' },
    { type: 'meld_pon', label: '碰!' },
    { type: 'meld_kong', label: '杠!' },
    { type: 'hu', label: '胡!' },
    { type: 'pass', label: '过' },
  ];

  audioModalMount.innerHTML = `
    <div class="audio-popover-card">
      <div class="audio-pop-head">
        <strong>原生 Web Audio 音效试听</strong>
        <span class="audio-state-pill state-${st.state}">AudioContext: ${st.state}</span>
        <button type="button" class="proto-close-mini" data-close-audio>×</button>
      </div>
      <div class="audio-btn-grid">
        ${sfxItems.map((it) => `<button type="button" class="sb-btn" data-sfx="${it.type}">${it.label}</button>`).join('')}
      </div>
      <div class="audio-controls-row">
        <label>
          音量
          <input type="range" min="0" max="1" step="0.05" value="${st.volume}" data-vol-slider />
        </label>
        <div class="audio-session-wrap">
          <span>iOS AudioSession:</span>
          <button type="button" class="sb-btn ${st.sessionType === 'playback' ? 'is-on' : ''}" data-session="playback">playback (无视静音键)</button>
          <button type="button" class="sb-btn ${st.sessionType === 'ambient' ? 'is-on' : ''}" data-session="ambient">ambient (混播音乐)</button>
        </div>
      </div>
    </div>
  `;

  audioModalMount.querySelector('[data-close-audio]')?.addEventListener('click', () => {
    audioPopoverOpen = false;
    renderToolbar();
    renderAudioPopover();
  });

  audioModalMount.querySelectorAll('[data-sfx]').forEach((btn) => {
    btn.addEventListener('click', () => {
      audioRouter.unlockFromGesture();
      audioRouter.emit({ type: btn.dataset.sfx, seat: 0, tile: '5m' });
    });
  });

  audioModalMount.querySelector('[data-vol-slider]')?.addEventListener('input', (e) => {
    audioRouter.setVolume(e.target.value);
  });

  audioModalMount.querySelectorAll('[data-session]').forEach((btn) => {
    btn.addEventListener('click', () => {
      audioRouter.setAudioSessionType(btn.dataset.session);
      renderAudioPopover();
    });
  });
}

function renderInspector() {
  if (!inspectorMount) return;
  if (!inspectorOpen) {
    inspectorMount.hidden = true;
    inspectorMount.innerHTML = '';
    return;
  }
  const s = tableStore.getState();
  const aSt = audioRouter.getAudioStatus();
  const aLog = audioRouter.getAudioLog();
  inspectorMount.hidden = false;
  inspectorMount.innerHTML = `
    <div class="inspector-panel">
      <div class="inspector-head">
        <strong>状态检视（PROTOTYPE）</strong>
        <button type="button" class="proto-close-mini" data-close-inspector>×</button>
      </div>
      <dl class="inspector-kv">
        <dt>phase / seq</dt><dd>${s.phase} / #${s.seq}</dd>
        <dt>turnSeat</dt><dd>${s.players[s.turnSeat]?.wind} · ${s.players[s.turnSeat]?.name}</dd>
        <dt>牌墙剩余</dt><dd>${s.wallRemaining} 张</dd>
        <dt>过水碰锁定</dt><dd>${s.furitenFlags.过水碰.length ? s.furitenFlags.过水碰.map(getTileName).join(', ') : '无'}</dd>
        <dt>过水胡锁定</dt><dd>${s.furitenFlags.过水胡 ? '是（本巡禁止点炮/抢杠胡）' : '否'}</dd>
        <dt>最近动作</dt><dd>${s.lastAction}</dd>
        <dt>SemanticEvent</dt><dd><code>${JSON.stringify(s.lastSemanticEvent)}</code></dd>
        <dt>AudioContext</dt><dd>${aSt.state} (${aSt.sessionType})</dd>
        <dt>最近音频事件</dt><dd>${aLog.slice(0, 3).map((x) => `${x.type}@${x.ts}`).join(' | ') || '暂无'}</dd>
      </dl>
    </div>
  `;
  inspectorMount.querySelector('[data-close-inspector]')?.addEventListener('click', () => {
    inspectorOpen = false;
    renderToolbar();
    renderInspector();
  });
}

function renderSettlementModal(state) {
  if (!settlementMount) return;
  if (state.phase !== 'settlement' || !state.settlement) {
    settlementMount.hidden = true;
    settlementMount.innerHTML = '';
    return;
  }

  const st = state.settlement;
  const winner = st.winnerSeat !== null ? state.players[st.winnerSeat] : null;
  const loser = st.loserSeat !== null ? state.players[st.loserSeat] : null;

  settlementMount.hidden = false;
  settlementMount.innerHTML = `
    <div class="proto-modal-backdrop">
      <div class="proto-modal-card settlement-card theme-${currentVariantId}" role="dialog" aria-label="小局结算">
        <div class="settle-header">
          <div class="settle-badge">${st.winType}</div>
          <div class="settle-title-block">
            <h3>${winner ? `${winner.name}（${winner.wind}家）${st.winType}` : '荒庄 · 不查叫'}</h3>
            <p>${loser ? `点炮者：${loser.name}（${loser.wind}家）` : st.winType === '自摸' ? '三家平摊自摸分' : '牌墙 136 张已摸完，杠分结算仍有效'}</p>
          </div>
          ${
            winner
              ? `<div class="settle-fan-total">
                  <strong>${st.totalFan} 番</strong>
                  <span>${st.multiplier}× 倍率${st.capped ? '（4番封顶）' : ''}</span>
                </div>`
              : ''
          }
        </div>

        <div class="settle-breakdown">
          <div class="settle-items">
            <h4>番型与杠分明细</h4>
            <ul>
              ${st.items.map((it) => `<li><span>${it.label}</span><strong>${it.fan > 0 ? `+${it.fan} 番` : '0 番'}</strong></li>`).join('')}
              <li><span>本局杠分汇总</span><strong>${st.kongDeltas[0] >= 0 ? `+${st.kongDeltas[0]}` : st.kongDeltas[0]} 分</strong></li>
            </ul>
          </div>
          <div class="settle-scores">
            <h4>四家总分与本局盈亏</h4>
            <div class="settle-score-grid">
              ${state.players
                .map((p, idx) => {
                  const d = st.scoreDeltas[idx];
                  return `
                    <div class="settle-score-row ${idx === st.winnerSeat ? 'is-winner' : ''}">
                      <span class="seat-wind ${p.isDealer ? 'is-dealer' : ''}">${p.wind}${p.isDealer ? '·庄' : ''}</span>
                      <span class="s-name">${p.name}</span>
                      <span class="s-delta ${d > 0 ? 'pos' : d < 0 ? 'neg' : ''}">${d > 0 ? `+${d}` : d}</span>
                      <span class="s-total">${p.score} 分</span>
                    </div>
                  `;
                })
                .join('')}
            </div>
          </div>
        </div>

        <div class="settle-hands" data-settle-hands></div>

        <div class="settle-footer">
          <button type="button" class="settle-btn-secondary" data-try-dianpao>切换点炮结算示例</button>
          <button type="button" class="settle-btn-primary" data-next-hand>下一小局</button>
        </div>
      </div>
    </div>
  `;

  const handsContainer = settlementMount.querySelector('[data-settle-hands]');
  state.players.forEach((p, idx) => {
    const row = document.createElement('div');
    row.className = `settle-hand-row ${idx === st.winnerSeat ? 'is-winner' : ''}`;
    const label = document.createElement('div');
    label.className = 'settle-hand-label';
    label.textContent = `${p.wind} · ${p.name}`;
    const tilesWrap = document.createElement('div');
    tilesWrap.className = 'settle-hand-tiles';

    p.hand.forEach((t) => {
      tilesWrap.appendChild(createHandTile(t, { variant: currentVariantId, interactive: false, size: 'settlement' }));
    });
    if (p.drawnTile) {
      tilesWrap.appendChild(
        createHandTile(p.drawnTile, { variant: currentVariantId, isDrawn: true, interactive: false, size: 'settlement' })
      );
    }
    p.melds.forEach((m) => {
      tilesWrap.appendChild(createMeldGroup(m, { variant: currentVariantId }));
    });

    row.append(label, tilesWrap);
    handsContainer.appendChild(row);
  });

  settlementMount.querySelector('[data-next-hand]')?.addEventListener('click', () => {
    tableStore.loadNormalTurn(true);
  });
  settlementMount.querySelector('[data-try-dianpao]')?.addEventListener('click', () => {
    tableStore.loadSettlementScenario('点炮');
  });
}

// Subscribe to store & audio changes
tableStore.subscribe((state) => {
  if (activeVariant) activeVariant.render(state, ctx);
  renderToolbar();
  renderSettlementModal(state);
  if (inspectorOpen) renderInspector();
});

tableStore.onImpact((payload) => {
  triggerComicImpact(stageRoot, { variant: currentVariantId, ...payload });
});

audioRouter.subscribe(() => {
  if (audioPopoverOpen) renderAudioPopover();
  if (inspectorOpen) renderInspector();
});

// Global Left/Right arrow switcher (skipping inputs)
window.addEventListener('keydown', (e) => {
  const tag = e.target?.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || e.target?.isContentEditable) return;
  if (e.key === 'ArrowLeft') {
    e.preventDefault();
    cycleVariant(-1);
  } else if (e.key === 'ArrowRight') {
    e.preventDefault();
    cycleVariant(1);
  }
});

bindGlobalAudioUnlock();
renderToolbar();
mountVariant(currentVariantId);
