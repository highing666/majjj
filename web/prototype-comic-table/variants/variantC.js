// PROTOTYPE Variant C: Warm Parlor Table — full-bleed edge-to-edge mahogany border, oversized bone-ivory tiles, drag-to-throw arc + A/D+W keyboard, lacquer call stamps with target mini tile.

import { getImpastoTextureUrl } from '../texture.js';
import {
  createHandTile,
  createMeldGroup,
  createRiverTile,
  createTileBack,
  getTileName,
  renderTileFaceSvg,
} from '../tiles.js';
import { audioRouter } from '../audio.js';

export const variantC = {
  id: 'C',
  name: '雀馆暖桌（拖拽甩牌 + 宽屏大牌）',
  shortLabel: 'C (雀馆暖桌 · 拖拽甩牌+大牌)',

  mount(rootEl, ctx) {
    this.root = rootEl;
    this.ctx = ctx;
    this.focusedIndex = -1; // keyboard focus index across hand + drawn
    this.lastClickKey = null;
    this.lastClickTs = 0;
    this.dragState = null;
    this.lastState = null;

    rootEl.className = 'table-shell theme-C';
    rootEl.style.setProperty('--impasto-url', `url("${getImpastoTextureUrl('parlor')}")`);

    rootEl.innerHTML = `
      <div class="felt-backdrop felt-backdrop--parlor"></div>
      <div class="impasto-overlay"></div>
      <div class="spotlight-vignette spotlight-vignette--parlor"></div>
      <div class="vc-mahogany-rail" aria-hidden="true"></div>

      <div class="vc-top-ribbon" data-round-ribbon>东一局 · 东风战 · 牌墙余 58 张</div>

      <div class="vc-corner-chip pos-tl" data-corner="2"></div>
      <div class="vc-corner-chip pos-tr" data-corner="1"></div>
      <div class="vc-corner-chip pos-bl" data-corner="3"></div>
      <div class="vc-corner-chip pos-br" data-corner="0"></div>

      <div class="vc-throw-arc-overlay" data-throw-arc aria-hidden="true">
        <svg viewBox="0 0 800 180" preserveAspectRatio="none">
          <path d="M 80 155 Q 400 22 720 155" fill="none" stroke="rgba(247, 224, 166, 0.55)" stroke-width="4" stroke-dasharray="12 10" stroke-linecap="round"/>
        </svg>
        <span class="vc-throw-hint" data-throw-hint>向上拖过虚线甩出舍牌</span>
      </div>

      <div class="vc-arena">
        <div class="vc-opp-top">
          <div class="va-opp-hand va-opp-hand--top" data-hand="2"></div>
          <div class="va-opp-melds va-opp-melds--top" data-melds="2"></div>
        </div>

        <div class="vc-opp-left">
          <div class="va-opp-hand va-opp-hand--left" data-hand="3"></div>
          <div class="va-opp-melds va-opp-melds--side" data-melds="3"></div>
        </div>

        <div class="vc-center-rivers">
          <div class="va-river va-river--top" data-river="2"></div>
          <div class="va-river va-river--left" data-river="3"></div>
          <div class="vc-center-seal" data-center-seal>
            <span class="vc-seal-wind" data-turn-wind>东</span>
            <span class="vc-seal-wall">余 <strong data-wall-count>58</strong></span>
          </div>
          <div class="va-river va-river--right" data-river="1"></div>
          <div class="va-river va-river--bottom" data-river="0"></div>
        </div>

        <div class="vc-opp-right">
          <div class="va-opp-hand va-opp-hand--right" data-hand="1"></div>
          <div class="va-opp-melds va-opp-melds--side" data-melds="1"></div>
        </div>

        <div class="vc-hero-ledge-wrap">
          <div class="vc-lacquer-calls" data-calls></div>
          <div class="vc-wooden-ledge">
            <div class="va-hero-rack" data-hero-hand></div>
            <div class="va-hero-melds" data-melds="0"></div>
          </div>
          <div class="vc-controls-caption">操作：按住向上拖拽甩牌 · 双击出牌 · A/D 选牌 W/Enter 打出</div>
        </div>
      </div>
    `;

    // Strictly use A/D + W/Enter so global Left/Right arrows remain dedicated to prototype variant switcher
    this.onKeyDown = (e) => {
      if (!this.lastState) return;
      const tag = e.target?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || e.target?.isContentEditable) return;
      if (this.lastState.phase !== 'player_turn' || this.lastState.turnSeat !== 0) return;

      const hero = this.lastState.players[0];
      const totalCount = hero.hand.length + (hero.drawnTile ? 1 : 0);
      if (totalCount === 0) return;

      const key = e.key.toLowerCase();
      if (key === 'a') {
        e.preventDefault();
        this.focusedIndex = this.focusedIndex <= 0 ? totalCount - 1 : this.focusedIndex - 1;
        audioRouter.emit({ type: 'hover', seat: 0 });
        this.render(this.lastState, this.ctx);
      } else if (key === 'd') {
        e.preventDefault();
        this.focusedIndex = (this.focusedIndex + 1) % totalCount;
        audioRouter.emit({ type: 'hover', seat: 0 });
        this.render(this.lastState, this.ctx);
      } else if ((key === 'w' || e.key === 'Enter') && this.focusedIndex >= 0 && this.focusedIndex < totalCount) {
        e.preventDefault();
        const isDrawn = hero.drawnTile && this.focusedIndex === totalCount - 1;
        const target = isDrawn ? 'drawn' : this.focusedIndex;
        this.focusedIndex = -1;
        this.ctx.actions.discardTile(target, true);
      }
    };

    window.addEventListener('keydown', this.onKeyDown);
  },

  render(state, ctx) {
    this.lastState = state;
    this.ctx = ctx;

    if (state.phase !== 'player_turn' || state.turnSeat !== 0) {
      this.focusedIndex = -1;
    }

    const ribbon = this.root.querySelector('[data-round-ribbon]');
    if (ribbon) {
      const ren = state.renchan > 0 ? ` · 连庄 ${state.renchan}` : '';
      ribbon.textContent = `${state.roundLabel} · ${state.mode}${ren} · 牌墙余 ${state.wallRemaining} 张`;
    }

    const wallEl = this.root.querySelector('[data-wall-count]');
    if (wallEl) wallEl.textContent = String(state.wallRemaining);

    const turnWindEl = this.root.querySelector('[data-turn-wind]');
    if (turnWindEl) {
      turnWindEl.textContent = state.players[state.turnSeat]?.wind || '东';
    }

    // 4 outer corner chips
    state.players.forEach((p, seat) => {
      const chip = this.root.querySelector(`[data-corner="${seat}"]`);
      if (!chip) return;
      const isActive = state.turnSeat === seat && state.phase !== 'settlement';
      chip.classList.toggle('is-active', isActive);
      chip.innerHTML = `
        <span class="seat-wind ${p.isDealer ? 'is-dealer' : ''}">${p.wind}${p.isDealer ? '·庄' : ''}</span>
        <span class="seat-name">${p.name}</span>
        <span class="seat-score">${p.score}</span>
      `;
    });

    // Opponents concealed hands & melds
    [1, 2, 3].forEach((seat) => {
      const p = state.players[seat];
      const handEl = this.root.querySelector(`[data-hand="${seat}"]`);
      if (handEl) {
        handEl.innerHTML = '';
        const ori = seat === 2 ? 'top' : seat === 1 ? 'right' : 'left';
        for (let i = 0; i < p.hand.length; i++) {
          handEl.appendChild(createTileBack({ variant: 'C', orientation: ori }));
        }
      }
      const meldsEl = this.root.querySelector(`[data-melds="${seat}"]`);
      if (meldsEl) {
        meldsEl.innerHTML = '';
        p.melds.forEach((m) => meldsEl.appendChild(createMeldGroup(m, { variant: 'C' })));
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
        riverEl.appendChild(createRiverTile(disc, { variant: 'C', orientation: ori, isLatest }));
      });
    });

    // Hero melds
    const heroMeldsEl = this.root.querySelector('[data-melds="0"]');
    if (heroMeldsEl) {
      heroMeldsEl.innerHTML = '';
      state.players[0].melds.forEach((m) => heroMeldsEl.appendChild(createMeldGroup(m, { variant: 'C' })));
    }

    // Hero hand with drag-to-throw + double-click + keyboard focus
    const hero = state.players[0];
    const heroRack = this.root.querySelector('[data-hero-hand]');
    const arcOverlay = this.root.querySelector('[data-throw-arc]');
    const arcHint = this.root.querySelector('[data-throw-hint]');
    const canDiscard = state.phase === 'player_turn' && state.turnSeat === 0;

    if (heroRack) {
      heroRack.innerHTML = '';
      const attachInteractions = (tileEl, indexInRack, targetKey, tileCode) => {
        tileEl.classList.add('is-interactive');
        if (!canDiscard) tileEl.disabled = true;

        tileEl.addEventListener('pointerdown', (e) => {
          if (!canDiscard || e.button !== 0) return;
          e.preventDefault();
          tileEl.setPointerCapture(e.pointerId);
          const rect = tileEl.getBoundingClientRect();
          const rootRect = this.root.getBoundingClientRect();
          const startX = e.clientX;
          const startY = e.clientY;
          let dragging = false;

          const onMove = (mv) => {
            const dx = mv.clientX - startX;
            const dy = mv.clientY - startY;
            if (!dragging && Math.hypot(dx, dy) > 8) {
              dragging = true;
              tileEl.classList.add('is-dragging');
              if (arcOverlay) arcOverlay.classList.add('is-visible');
            }
            if (!dragging) return;

            tileEl.style.transform = `translate(${dx}px, ${dy}px) scale(1.06) rotate(${Math.max(-8, Math.min(8, dx * 0.04))}deg)`;
            const throwThreshold = rootRect.top + rootRect.height * 0.64;
            const readyToThrow = rect.top + dy < throwThreshold;
            if (arcOverlay) arcOverlay.classList.toggle('is-ready', readyToThrow);
            if (arcHint) arcHint.textContent = readyToThrow ? '松手甩出牌河！' : '继续向上拖过虚线甩牌';
          };

          const cleanup = () => {
            tileEl.removeEventListener('pointermove', onMove);
            tileEl.removeEventListener('pointerup', onUp);
            tileEl.removeEventListener('pointercancel', onCancel);
            tileEl.classList.remove('is-dragging');
            tileEl.style.transform = '';
            if (arcOverlay) {
              arcOverlay.classList.remove('is-visible', 'is-ready');
            }
          };

          const onUp = (up) => {
            const dy = up.clientY - startY;
            const throwThreshold = rootRect.top + rootRect.height * 0.64;
            const didDrag = dragging;
            cleanup();

            if (didDrag) {
              if (rect.top + dy < throwThreshold) {
                this.focusedIndex = -1;
                ctx.actions.discardTile(targetKey, true);
              }
              return;
            }

            // Treat as tap/click (support double-click discard + focus selection)
            const now = performance.now();
            if (this.lastClickKey === targetKey && now - this.lastClickTs < 380) {
              this.lastClickKey = null;
              this.focusedIndex = -1;
              ctx.actions.discardTile(targetKey, true);
            } else {
              this.lastClickKey = targetKey;
              this.lastClickTs = now;
              this.focusedIndex = indexInRack;
              audioRouter.emit({ type: 'hover', seat: 0, tile: tileCode });
              this.render(state, ctx);
            }
          };

          const onCancel = () => cleanup();

          tileEl.addEventListener('pointermove', onMove);
          tileEl.addEventListener('pointerup', onUp);
          tileEl.addEventListener('pointercancel', onCancel);
        });
      };

      hero.hand.forEach((tileCode, idx) => {
        const tileEl = createHandTile(tileCode, {
          variant: 'C',
          focused: this.focusedIndex === idx,
          interactive: true,
        });
        attachInteractions(tileEl, idx, idx, tileCode);
        heroRack.appendChild(tileEl);
      });

      if (hero.drawnTile) {
        const drawnIdx = hero.hand.length;
        const tileEl = createHandTile(hero.drawnTile, {
          variant: 'C',
          isDrawn: true,
          focused: this.focusedIndex === drawnIdx,
          interactive: true,
        });
        attachInteractions(tileEl, drawnIdx, 'drawn', hero.drawnTile);
        heroRack.appendChild(tileEl);
      }
    }

    // Centered tactile lacquer stamp dock with target mini tile SVG
    const callsEl = this.root.querySelector('[data-calls]');
    if (callsEl) {
      callsEl.innerHTML = '';
      if (state.pendingCalls?.length > 0 && state.phase !== 'settlement') {
        state.pendingCalls.forEach((call) => {
          const btn = document.createElement('button');
          btn.type = 'button';
          btn.className = `vc-stamp-btn call-${call.type}`;
          const miniTileHtml =
            call.tile && call.type !== 'pass'
              ? `<span class="vc-mini-tile" title="${getTileName(call.tile)}">${renderTileFaceSvg(call.tile, 'C')}</span>`
              : '';
          btn.innerHTML = `
            ${miniTileHtml}
            <span class="vc-stamp-text">${call.label}</span>
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
    window.removeEventListener('keydown', this.onKeyDown);
    this.root = null;
    this.lastState = null;
  },
};
