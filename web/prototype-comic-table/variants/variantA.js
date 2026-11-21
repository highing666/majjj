// PROTOTYPE Variant A: Piltover Stage — symmetric compass hub + 2-step click-to-discard + angled comic call banners.

import { getImpastoTextureUrl } from '../texture.js';
import {
  createHandTile,
  createMeldGroup,
  createRiverTile,
  createTileBack,
  getTileName,
} from '../tiles.js';
import { audioRouter } from '../audio.js';

export const variantA = {
  id: 'A',
  name: '皮城舞台（二次确认）',
  shortLabel: 'A (皮城舞台 · 二次确认)',

  mount(rootEl, ctx) {
    this.root = rootEl;
    this.ctx = ctx;
    this.selectedKey = null; // e.g. 'hand-3' | 'drawn'
    this.lastState = null;

    rootEl.className = 'table-shell theme-A';
    rootEl.style.setProperty('--impasto-url', `url("${getImpastoTextureUrl('piltover')}")`);

    rootEl.innerHTML = `
      <div class="felt-backdrop felt-backdrop--piltover"></div>
      <div class="impasto-overlay"></div>
      <div class="spotlight-vignette spotlight-vignette--piltover"></div>
      <div class="artdeco-frame" aria-hidden="true">
        <span class="deco-corner tl"></span>
        <span class="deco-corner tr"></span>
        <span class="deco-corner bl"></span>
        <span class="deco-corner br"></span>
      </div>

      <div class="va-arena">
        <div class="va-seat-top">
          <div class="va-opp-meta" data-seat-meta="2"></div>
          <div class="va-opp-rack-row">
            <div class="va-opp-hand va-opp-hand--top" data-hand="2"></div>
            <div class="va-opp-melds va-opp-melds--top" data-melds="2"></div>
          </div>
        </div>

        <div class="va-seat-left">
          <div class="va-opp-meta va-opp-meta--side" data-seat-meta="3"></div>
          <div class="va-opp-rack-col">
            <div class="va-opp-hand va-opp-hand--left" data-hand="3"></div>
            <div class="va-opp-melds va-opp-melds--side" data-melds="3"></div>
          </div>
        </div>

        <div class="va-center-grid">
          <div class="va-river va-river--top" data-river="2"></div>
          <div class="va-river va-river--left" data-river="3"></div>

          <div class="va-compass-hub" data-hub>
            <div class="va-hub-ring">
              <div class="va-hub-round" data-round-info>东一局 · 东风战</div>
              <div class="va-hub-wall">余 <strong data-wall-count>58</strong> 张</div>
              <div class="va-hub-winds">
                <div class="va-hub-seat pos-top" data-hub-seat="2"></div>
                <div class="va-hub-seat pos-left" data-hub-seat="3"></div>
                <div class="va-hub-seat pos-right" data-hub-seat="1"></div>
                <div class="va-hub-seat pos-bottom" data-hub-seat="0"></div>
              </div>
            </div>
          </div>

          <div class="va-river va-river--right" data-river="1"></div>
          <div class="va-river va-river--bottom" data-river="0"></div>
        </div>

        <div class="va-seat-right">
          <div class="va-opp-rack-col">
            <div class="va-opp-hand va-opp-hand--right" data-hand="1"></div>
            <div class="va-opp-melds va-opp-melds--side" data-melds="1"></div>
          </div>
          <div class="va-opp-meta va-opp-meta--side" data-seat-meta="1"></div>
        </div>

        <div class="va-hero-zone">
          <div class="va-call-banners" data-calls></div>
          <div class="va-hero-bar">
            <div class="va-hero-badge" data-seat-meta="0"></div>
            <div class="va-hero-rack" data-hero-hand></div>
            <div class="va-hero-melds" data-melds="0"></div>
          </div>
        </div>
      </div>
    `;

    this.onRootClick = (e) => {
      const target = e.target;
      if (target.closest('.mj-tile.is-interactive') || target.closest('.va-call-btn')) return;
      if (this.selectedKey !== null) {
        this.selectedKey = null;
        if (this.lastState) this.render(this.lastState, this.ctx);
      }
    };

    this.onContextMenu = (e) => {
      if (this.selectedKey !== null) {
        e.preventDefault();
        this.selectedKey = null;
        if (this.lastState) this.render(this.lastState, this.ctx);
      }
    };

    this.onKeyDown = (e) => {
      if (!this.lastState) return;
      const tag = e.target?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || e.target?.isContentEditable) return;

      const state = this.lastState;
      if (state.pendingCalls?.length > 0 && state.phase !== 'settlement') {
        const key = e.key.toLowerCase();
        if (key === 'h') {
          const c = state.pendingCalls.find((x) => x.type === 'hu');
          if (c) {
            e.preventDefault();
            this.ctx.actions.chooseCall('hu', c.tile);
            return;
          }
        }
        if (key === 'g') {
          const c = state.pendingCalls.find((x) =>
            ['ankong', 'bukong', 'mingkong'].includes(x.type)
          );
          if (c) {
            e.preventDefault();
            this.ctx.actions.chooseCall(c.type, c.tile);
            return;
          }
        }
        if (key === 'p') {
          const c = state.pendingCalls.find((x) => x.type === 'pon');
          if (c) {
            e.preventDefault();
            this.ctx.actions.chooseCall('pon', c.tile);
            return;
          }
        }
        if (e.key === 'Escape') {
          const c = state.pendingCalls.find((x) => x.type === 'pass');
          if (c) {
            e.preventDefault();
            this.ctx.actions.chooseCall('pass', c.tile);
            return;
          }
        }
      }

      if ((e.key === 'Enter' || e.key === ' ') && this.selectedKey !== null) {
        if (state.phase === 'player_turn' && state.turnSeat === 0) {
          e.preventDefault();
          const key = this.selectedKey;
          this.selectedKey = null;
          this.ctx.actions.discardTile(key === 'drawn' ? 'drawn' : Number(key.replace('hand-', '')), false);
        }
      } else if (e.key === 'Escape' && this.selectedKey !== null) {
        this.selectedKey = null;
        this.render(state, this.ctx);
      }
    };

    rootEl.addEventListener('click', this.onRootClick);
    rootEl.addEventListener('contextmenu', this.onContextMenu);
    window.addEventListener('keydown', this.onKeyDown);
  },

  render(state, ctx) {
    this.lastState = state;
    this.ctx = ctx;

    if (state.phase !== 'player_turn' || state.turnSeat !== 0) {
      this.selectedKey = null;
    }

    const roundEl = this.root.querySelector('[data-round-info]');
    const wallEl = this.root.querySelector('[data-wall-count]');
    if (roundEl) {
      const ren = state.renchan > 0 ? ` · 连庄 ${state.renchan}` : '';
      roundEl.textContent = `${state.roundLabel} · ${state.mode}${ren}`;
    }
    if (wallEl) wallEl.textContent = String(state.wallRemaining);

    // Hub compass seat indicators + outer badges
    state.players.forEach((p, seat) => {
      const isActive = state.turnSeat === seat && state.phase !== 'settlement';
      const hubChip = this.root.querySelector(`[data-hub-seat="${seat}"]`);
      if (hubChip) {
        hubChip.classList.toggle('is-active', isActive);
        hubChip.innerHTML = `
          <span class="va-wind-badge ${p.isDealer ? 'is-dealer' : ''}">${p.wind}</span>
          <span class="va-hub-score">${p.score}</span>
        `;
      }

      const metaEl = this.root.querySelector(`[data-seat-meta="${seat}"]`);
      if (metaEl) {
        metaEl.classList.toggle('is-active', isActive);
        metaEl.innerHTML = `
          <span class="seat-wind ${p.isDealer ? 'is-dealer' : ''}">${p.wind}${p.isDealer ? '·庄' : ''}</span>
          <span class="seat-name">${p.name}</span>
          <span class="seat-score">${p.score}</span>
        `;
      }
    });

    // Opponents concealed hands + melds
    [1, 2, 3].forEach((seat) => {
      const p = state.players[seat];
      const handEl = this.root.querySelector(`[data-hand="${seat}"]`);
      if (handEl) {
        handEl.innerHTML = '';
        const ori = seat === 2 ? 'top' : seat === 1 ? 'right' : 'left';
        const count = p.hand.length;
        for (let i = 0; i < count; i++) {
          handEl.appendChild(createTileBack({ variant: 'A', orientation: ori }));
        }
      }
      const meldsEl = this.root.querySelector(`[data-melds="${seat}"]`);
      if (meldsEl) {
        meldsEl.innerHTML = '';
        p.melds.forEach((m) => meldsEl.appendChild(createMeldGroup(m, { variant: 'A' })));
      }
    });

    // 4 rivers (6 per row)
    state.players.forEach((p, seat) => {
      const riverEl = this.root.querySelector(`[data-river="${seat}"]`);
      if (!riverEl) return;
      riverEl.innerHTML = '';
      const ori = seat === 0 ? 'bottom' : seat === 1 ? 'right' : seat === 2 ? 'top' : 'left';
      p.river.forEach((disc, idx) => {
        const isLatest = seat === state.latestDiscardSeat && idx === p.river.length - 1;
        riverEl.appendChild(createRiverTile(disc, { variant: 'A', orientation: ori, isLatest }));
      });
    });

    // Hero melds
    const heroMeldsEl = this.root.querySelector('[data-melds="0"]');
    if (heroMeldsEl) {
      heroMeldsEl.innerHTML = '';
      state.players[0].melds.forEach((m) => heroMeldsEl.appendChild(createMeldGroup(m, { variant: 'A' })));
    }

    // Hero standing hand + drawn tile
    const hero = state.players[0];
    const heroRack = this.root.querySelector('[data-hero-hand]');
    const canDiscard = state.phase === 'player_turn' && state.turnSeat === 0;
    if (heroRack) {
      heroRack.innerHTML = '';
      const bindInteractiveTile = (tileEl, key, tileCode) => {
        tileEl.classList.add('is-interactive');
        if (!canDiscard) tileEl.disabled = true;
        if (this.selectedKey === key) {
          const hint = document.createElement('span');
          hint.className = 'va-confirm-pill';
          hint.textContent = '再点一次 / Enter 打出';
          tileEl.appendChild(hint);
        }
        tileEl.addEventListener('pointerenter', () => {
          if (canDiscard) audioRouter.emit({ type: 'hover', seat: 0, tile: tileCode });
        });
        tileEl.addEventListener('click', (e) => {
          e.stopPropagation();
          if (!canDiscard) return;
          if (this.selectedKey === key) {
            this.selectedKey = null;
            ctx.actions.discardTile(key === 'drawn' ? 'drawn' : Number(key.replace('hand-', '')), false);
          } else {
            this.selectedKey = key;
            audioRouter.emit({ type: 'hover', seat: 0, tile: tileCode });
            this.render(state, ctx);
          }
        });
      };

      hero.hand.forEach((tileCode, idx) => {
        const key = `hand-${idx}`;
        const tileEl = createHandTile(tileCode, {
          variant: 'A',
          selected: this.selectedKey === key,
          interactive: true,
        });
        bindInteractiveTile(tileEl, key, tileCode);
        heroRack.appendChild(tileEl);
      });

      if (hero.drawnTile) {
        const tileEl = createHandTile(hero.drawnTile, {
          variant: 'A',
          isDrawn: true,
          selected: this.selectedKey === 'drawn',
          interactive: true,
        });
        bindInteractiveTile(tileEl, 'drawn', hero.drawnTile);
        heroRack.appendChild(tileEl);
      }
    }

    // Call banners above right side of hero hand
    const callsEl = this.root.querySelector('[data-calls]');
    if (callsEl) {
      callsEl.innerHTML = '';
      if (state.pendingCalls?.length > 0 && state.phase !== 'settlement') {
        state.pendingCalls.forEach((call) => {
          const btn = document.createElement('button');
          btn.type = 'button';
          btn.className = `va-call-btn call-${call.type}`;
          const hotkey =
            call.type === 'hu'
              ? 'H'
              : ['ankong', 'bukong', 'mingkong'].includes(call.type)
              ? 'G'
              : call.type === 'pon'
              ? 'P'
              : 'Esc';
          btn.innerHTML = `
            <span class="call-label">${call.label}</span>
            ${call.tile && call.type !== 'pass' ? `<span class="call-tile-name">${getTileName(call.tile)}</span>` : ''}
            <kbd class="call-kbd">${hotkey}</kbd>
          `;
          btn.addEventListener('click', (e) => {
            e.stopPropagation();
            ctx.actions.chooseCall(call.type, call.tile);
          });
          callsEl.appendChild(btn);
        });
      }
    }
  },

  unmount() {
    if (this.root) {
      this.root.removeEventListener('click', this.onRootClick);
      this.root.removeEventListener('contextmenu', this.onContextMenu);
    }
    window.removeEventListener('keydown', this.onKeyDown);
    this.root = null;
    this.lastState = null;
  },
};
