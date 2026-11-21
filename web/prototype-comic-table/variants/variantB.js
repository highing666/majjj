// PROTOTYPE Variant B: Zaun Comic Panels — 78% table + 22% right HUD rail, instant single-click discard with ghost river slot, speech-bubble calls.

import { getImpastoTextureUrl } from '../texture.js';
import {
  createHandTile,
  createMeldGroup,
  createRiverTile,
  createTileBack,
  getTileName,
} from '../tiles.js';
import { audioRouter } from '../audio.js';

export const variantB = {
  id: 'B',
  name: '祖安漫画分镜（单击直出 + 右侧情报栏）',
  shortLabel: 'B (祖安分镜 · 单击直出+侧栏)',

  mount(rootEl, ctx) {
    this.root = rootEl;
    this.ctx = ctx;
    this.instantMode = true; // true = 单击直出, false = 双击防误触
    this.hoverTile = null;
    this.selectedKey = null;
    this.lastState = null;

    rootEl.className = 'table-shell theme-B';
    rootEl.style.setProperty('--impasto-url', `url("${getImpastoTextureUrl('zaun')}")`);

    rootEl.innerHTML = `
      <div class="vb-split-layout">
        <div class="vb-stage">
          <div class="felt-backdrop felt-backdrop--zaun"></div>
          <div class="impasto-overlay"></div>
          <div class="spotlight-vignette spotlight-vignette--zaun"></div>
          <div class="vb-halftone-corners" aria-hidden="true"></div>

          <div class="vb-table-arena">
            <div class="vb-top-seat">
              <div class="va-opp-hand va-opp-hand--top" data-hand="2"></div>
              <div class="va-opp-melds va-opp-melds--top" data-melds="2"></div>
            </div>

            <div class="vb-left-seat">
              <div class="va-opp-hand va-opp-hand--left" data-hand="3"></div>
              <div class="va-opp-melds va-opp-melds--side" data-melds="3"></div>
            </div>

            <div class="vb-center-arena">
              <div class="va-river va-river--top" data-river="2"></div>
              <div class="va-river va-river--left" data-river="3"></div>

              <div class="vb-center-beacon" data-beacon>
                <div class="vb-beacon-wall">余 <strong data-wall-count>58</strong></div>
                <div class="vb-beacon-turn" data-turn-label>东 · 我出牌</div>
              </div>

              <div class="va-river va-river--right" data-river="1"></div>
              <div class="va-river va-river--bottom" data-river="0"></div>
            </div>

            <div class="vb-right-seat">
              <div class="va-opp-hand va-opp-hand--right" data-hand="1"></div>
              <div class="va-opp-melds va-opp-melds--side" data-melds="1"></div>
            </div>

            <div class="vb-call-bubbles" data-calls></div>

            <div class="vb-hero-dock">
              <div class="va-hero-rack" data-hero-hand></div>
              <div class="va-hero-melds" data-melds="0"></div>
            </div>
          </div>
        </div>

        <aside class="vb-hud-rail">
          <div class="vb-rail-header">
            <div class="vb-rail-round" data-round-info>东一局 · 东风战</div>
            <div class="vb-rail-mode-toggle">
              <span class="vb-toggle-label">出牌手感</span>
              <button type="button" class="vb-mode-btn" data-toggle-discard-mode>
                单击直出
              </button>
            </div>
          </div>

          <div class="vb-player-cards" data-player-cards></div>

          <div class="vb-feed-panel">
            <div class="vb-feed-title">牌局动态</div>
            <ul class="vb-feed-list" data-feed-list></ul>
          </div>
        </aside>
      </div>
    `;

    const modeBtn = rootEl.querySelector('[data-toggle-discard-mode]');
    modeBtn?.addEventListener('click', () => {
      this.instantMode = !this.instantMode;
      this.selectedKey = null;
      this.hoverTile = null;
      if (this.lastState) this.render(this.lastState, this.ctx);
    });
  },

  updateGhostSlot(riverEl) {
    if (!riverEl) return;
    const prevGhost = riverEl.querySelector('.mj-river-tile.is-ghost');
    if (prevGhost) prevGhost.remove();
    if (this.hoverTile && this.lastState?.phase === 'player_turn' && this.lastState?.turnSeat === 0) {
      const ghost = createRiverTile(
        { tile: this.hoverTile, isTsumogiri: false },
        { variant: 'B', orientation: 'bottom', isGhost: true }
      );
      riverEl.appendChild(ghost);
    }
  },

  render(state, ctx) {
    this.lastState = state;
    this.ctx = ctx;

    if (state.phase !== 'player_turn' || state.turnSeat !== 0) {
      this.hoverTile = null;
      this.selectedKey = null;
    }

    const roundEl = this.root.querySelector('[data-round-info]');
    if (roundEl) {
      const ren = state.renchan > 0 ? ` · 连庄 ${state.renchan}` : '';
      roundEl.textContent = `${state.roundLabel} · ${state.mode}${ren}`;
    }

    const modeBtn = this.root.querySelector('[data-toggle-discard-mode]');
    if (modeBtn) {
      modeBtn.textContent = this.instantMode ? '单击直出 (快)' : '双击防误触';
      modeBtn.classList.toggle('is-instant', this.instantMode);
    }

    const wallEl = this.root.querySelector('[data-wall-count]');
    if (wallEl) wallEl.textContent = String(state.wallRemaining);

    const turnEl = this.root.querySelector('[data-turn-label]');
    if (turnEl) {
      const activeP = state.players[state.turnSeat];
      turnEl.textContent =
        state.phase === 'settlement'
          ? '本小局结算'
          : state.phase === 'call_window' || state.phase === 'rob_kong_window'
          ? '鸣牌裁决'
          : `${activeP.wind} · ${activeP.name}`;
      turnEl.classList.toggle('is-hero-turn', state.phase === 'player_turn' && state.turnSeat === 0);
    }

    // Right comic rail player cards
    const cardsEl = this.root.querySelector('[data-player-cards]');
    if (cardsEl) {
      cardsEl.innerHTML = '';
      state.players.forEach((p, seat) => {
        const isActive = state.turnSeat === seat && state.phase !== 'settlement';
        const card = document.createElement('div');
        card.className = `vb-player-card ${isActive ? 'is-active' : ''} ${p.isHuman ? 'is-human' : ''}`;
        const deltaStr = p.lastDelta > 0 ? `+${p.lastDelta}` : p.lastDelta < 0 ? `${p.lastDelta}` : '';
        card.innerHTML = `
          <div class="vb-card-top">
            <span class="seat-wind ${p.isDealer ? 'is-dealer' : ''}">${p.wind}${p.isDealer ? '·庄' : ''}</span>
            <span class="vb-card-name">${p.name}</span>
            <span class="vb-card-score">${p.score}</span>
          </div>
          <div class="vb-card-sub">
            <span>鸣牌 ${p.melds.length} 组</span>
            ${deltaStr ? `<span class="vb-delta ${p.lastDelta > 0 ? 'pos' : 'neg'}">${deltaStr}</span>` : '<span></span>'}
          </div>
        `;
        cardsEl.appendChild(card);
      });
    }

    // Feed
    const feedEl = this.root.querySelector('[data-feed-list]');
    if (feedEl) {
      feedEl.innerHTML = state.eventLog
        .slice(0, 7)
        .map((item) => `<li><span class="feed-seq">#${item.seq}</span> ${item.text}</li>`)
        .join('');
    }

    // Opponent hands & melds
    [1, 2, 3].forEach((seat) => {
      const p = state.players[seat];
      const handEl = this.root.querySelector(`[data-hand="${seat}"]`);
      if (handEl) {
        handEl.innerHTML = '';
        const ori = seat === 2 ? 'top' : seat === 1 ? 'right' : 'left';
        for (let i = 0; i < p.hand.length; i++) {
          handEl.appendChild(createTileBack({ variant: 'B', orientation: ori }));
        }
      }
      const meldsEl = this.root.querySelector(`[data-melds="${seat}"]`);
      if (meldsEl) {
        meldsEl.innerHTML = '';
        p.melds.forEach((m) => meldsEl.appendChild(createMeldGroup(m, { variant: 'B' })));
      }
    });

    // Rivers
    state.players.forEach((p, seat) => {
      const riverEl = this.root.querySelector(`[data-river="${seat}"]`);
      if (!riverEl) return;
      riverEl.innerHTML = '';
      const ori = seat === 0 ? 'bottom' : seat === 1 ? 'right' : seat === 2 ? 'top' : 'left';
      p.river.forEach((disc, idx) => {
        const isLatest = seat === state.latestDiscardSeat && idx === p.river.length - 1;
        riverEl.appendChild(createRiverTile(disc, { variant: 'B', orientation: ori, isLatest }));
      });
    });

    const heroRiverEl = this.root.querySelector('[data-river="0"]');
    this.updateGhostSlot(heroRiverEl);

    // Hero melds
    const heroMeldsEl = this.root.querySelector('[data-melds="0"]');
    if (heroMeldsEl) {
      heroMeldsEl.innerHTML = '';
      state.players[0].melds.forEach((m) => heroMeldsEl.appendChild(createMeldGroup(m, { variant: 'B' })));
    }

    // Hero hand
    const hero = state.players[0];
    const heroRack = this.root.querySelector('[data-hero-hand]');
    const canDiscard = state.phase === 'player_turn' && state.turnSeat === 0;
    if (heroRack) {
      heroRack.innerHTML = '';
      const bindTile = (tileEl, key, tileCode) => {
        tileEl.classList.add('is-interactive');
        if (!canDiscard) tileEl.disabled = true;

        tileEl.addEventListener('pointerenter', () => {
          if (!canDiscard) return;
          this.hoverTile = tileCode;
          this.updateGhostSlot(heroRiverEl);
          audioRouter.emit({ type: 'hover', seat: 0, tile: tileCode });
        });
        tileEl.addEventListener('pointerleave', () => {
          if (this.hoverTile === tileCode) {
            this.hoverTile = null;
            this.updateGhostSlot(heroRiverEl);
          }
        });
        tileEl.addEventListener('click', () => {
          if (!canDiscard) return;
          if (this.instantMode) {
            this.hoverTile = null;
            this.selectedKey = null;
            ctx.actions.discardTile(key === 'drawn' ? 'drawn' : Number(key.replace('hand-', '')), true);
          } else {
            if (this.selectedKey === key) {
              this.hoverTile = null;
              this.selectedKey = null;
              ctx.actions.discardTile(key === 'drawn' ? 'drawn' : Number(key.replace('hand-', '')), true);
            } else {
              this.selectedKey = key;
              this.render(state, ctx);
            }
          }
        });
      };

      hero.hand.forEach((tileCode, idx) => {
        const key = `hand-${idx}`;
        const tileEl = createHandTile(tileCode, {
          variant: 'B',
          selected: !this.instantMode && this.selectedKey === key,
          interactive: true,
        });
        bindTile(tileEl, key, tileCode);
        heroRack.appendChild(tileEl);
      });

      if (hero.drawnTile) {
        const tileEl = createHandTile(hero.drawnTile, {
          variant: 'B',
          isDrawn: true,
          selected: !this.instantMode && this.selectedKey === 'drawn',
          interactive: true,
        });
        bindTile(tileEl, 'drawn', hero.drawnTile);
        heroRack.appendChild(tileEl);
      }
    }

    // Speech-bubble call cluster near lower-center
    const callsEl = this.root.querySelector('[data-calls]');
    if (callsEl) {
      callsEl.innerHTML = '';
      if (state.pendingCalls?.length > 0 && state.phase !== 'settlement') {
        state.pendingCalls.forEach((call) => {
          const btn = document.createElement('button');
          btn.type = 'button';
          btn.className = `vb-bubble-btn call-${call.type}`;
          btn.innerHTML = `
            <span class="vb-bubble-main">${call.label}</span>
            ${call.tile && call.type !== 'pass' ? `<span class="vb-bubble-tile">${getTileName(call.tile)}</span>` : ''}
          `;
          btn.addEventListener('click', () => {
            ctx.actions.chooseCall(call.type, call.tile);
          });
          callsEl.appendChild(btn);
        });
      }
    }
  },

  unmount() {
    this.root = null;
    this.lastState = null;
    this.hoverTile = null;
    this.selectedKey = null;
  },
};
