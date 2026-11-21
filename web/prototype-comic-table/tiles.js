// PROTOTYPE: Bespoke 34-tile vector SVG renderer and DOM tile builders for Tuidaohu.
// Zero Unicode mahjong emojis; high-contrast comic ink framing + clean ivory face for Retina legibility.

export const SUIT_GROUPS = [
  {
    key: 'm',
    label: '万子',
    tiles: ['1m', '2m', '3m', '4m', '5m', '6m', '7m', '8m', '9m'],
  },
  {
    key: 'p',
    label: '筒子',
    tiles: ['1p', '2p', '3p', '4p', '5p', '6p', '7p', '8p', '9p'],
  },
  {
    key: 's',
    label: '条子',
    tiles: ['1s', '2s', '3s', '4s', '5s', '6s', '7s', '8s', '9s'],
  },
  {
    key: 'z',
    label: '字牌',
    tiles: ['1z', '2z', '3z', '4z', '5z', '6z', '7z'],
  },
];

const CN_NUM = ['', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
const HONOR_NAMES = {
  '1z': '东风',
  '2z': '南风',
  '3z': '西风',
  '4z': '北风',
  '5z': '红中',
  '6z': '发财',
  '7z': '白板',
};
const HONOR_GLYPHS = {
  '1z': '東',
  '2z': '南',
  '3z': '西',
  '4z': '北',
  '5z': '中',
  '6z': '發',
  '7z': '白',
};

export const ALL_TILES = SUIT_GROUPS.flatMap((g) => g.tiles);

const SUIT_ORDER = { m: 0, p: 1, s: 2, z: 3 };

export function getTileName(code) {
  if (!code || code.length < 2) return '';
  if (HONOR_NAMES[code]) return HONOR_NAMES[code];
  const n = Number(code[0]);
  const suit = code[1];
  const suitName = suit === 'm' ? '万' : suit === 'p' ? '筒' : '条';
  return `${CN_NUM[n] || n}${suitName}`;
}

export function compareTiles(a, b) {
  const sa = SUIT_ORDER[a[1]] ?? 9;
  const sb = SUIT_ORDER[b[1]] ?? 9;
  if (sa !== sb) return sa - sb;
  return Number(a[0]) - Number(b[0]);
}

export function sortTiles(tiles) {
  return [...tiles].sort(compareTiles);
}

const PALETTES = {
  A: {
    ink: '#181A20',
    crimson: '#C7352B',
    cobalt: '#1C5296',
    jade: '#157F56',
    gold: '#C48A28',
    ivory: '#F5EEDF',
    ivoryShadow: '#E4D8BE',
  },
  B: {
    ink: '#0B0D12',
    crimson: '#D93B30',
    cobalt: '#1B5CAE',
    jade: '#138A63',
    gold: '#D99426',
    ivory: '#F7F1E1',
    ivoryShadow: '#E0D6BC',
  },
  C: {
    ink: '#241E1A',
    crimson: '#BA332A',
    cobalt: '#22508C',
    jade: '#18754E',
    gold: '#B88028',
    ivory: '#FBF6EB',
    ivoryShadow: '#EADEC4',
  },
};

function drawPinCircle(cx, cy, r, fill, p) {
  const innerR = r * 0.56;
  const dotR = r * 0.22;
  const ticks = [];
  for (let i = 0; i < 8; i++) {
    const rad = (i * Math.PI) / 4;
    const x1 = cx + Math.cos(rad) * (innerR + 1.2);
    const y1 = cy + Math.sin(rad) * (innerR + 1.2);
    const x2 = cx + Math.cos(rad) * (r - 2.2);
    const y2 = cy + Math.sin(rad) * (r - 2.2);
    ticks.push(`<line x1="${x1.toFixed(1)}" y1="${y1.toFixed(1)}" x2="${x2.toFixed(1)}" y2="${y2.toFixed(1)}" stroke="${p.ivory}" stroke-width="${Math.max(1.2, r * 0.12).toFixed(1)}" stroke-linecap="round"/>`);
  }
  return `
    <g>
      <circle cx="${cx}" cy="${cy}" r="${r}" fill="${fill}" stroke="${p.ink}" stroke-width="2.1"/>
      <circle cx="${cx}" cy="${cy}" r="${innerR.toFixed(1)}" fill="none" stroke="${p.ivory}" stroke-width="${Math.max(1.4, r * 0.14).toFixed(1)}"/>
      ${ticks.join('')}
      <circle cx="${cx}" cy="${cy}" r="${dotR.toFixed(1)}" fill="${p.ivory}" stroke="${p.ink}" stroke-width="1.2"/>
    </g>
  `;
}

function drawOnePinMedallion(p) {
  const petals = [];
  for (let i = 0; i < 8; i++) {
    const angle = i * 45;
    petals.push(`
      <g transform="rotate(${angle} 50 68)">
        <ellipse cx="50" cy="44" rx="6.8" ry="12" fill="${i % 2 === 0 ? p.jade : p.gold}" stroke="${p.ink}" stroke-width="1.8"/>
      </g>
    `);
  }
  return `
    <g>
      <circle cx="50" cy="68" r="35" fill="${p.cobalt}" stroke="${p.ink}" stroke-width="2.8"/>
      <circle cx="50" cy="68" r="30.5" fill="${p.ivory}" stroke="${p.ink}" stroke-width="1.6"/>
      ${petals.join('')}
      <circle cx="50" cy="68" r="14" fill="${p.crimson}" stroke="${p.ink}" stroke-width="2.2"/>
      <circle cx="50" cy="68" r="7.5" fill="${p.ivory}" stroke="${p.ink}" stroke-width="1.6"/>
      <circle cx="50" cy="68" r="3.4" fill="${p.crimson}"/>
    </g>
  `;
}

function renderPinSuit(n, p) {
  if (n === 1) return drawOnePinMedallion(p);
  const c = {
    2: [[50, 42, 16, p.jade], [50, 94, 16, p.cobalt]],
    3: [[27, 34, 13.5, p.cobalt], [50, 68, 13.5, p.crimson], [73, 102, 13.5, p.jade]],
    4: [
      [31, 42, 14, p.cobalt], [69, 42, 14, p.jade],
      [31, 94, 14, p.jade], [69, 94, 14, p.cobalt],
    ],
    5: [
      [27, 35, 12.2, p.cobalt], [73, 35, 12.2, p.jade],
      [50, 68, 12.6, p.crimson],
      [27, 101, 12.2, p.jade], [73, 101, 12.2, p.cobalt],
    ],
    6: [
      [32, 31, 12.2, p.jade], [68, 31, 12.2, p.jade],
      [32, 68, 12.2, p.crimson], [68, 68, 12.2, p.crimson],
      [32, 103, 12.2, p.crimson], [68, 103, 12.2, p.crimson],
    ],
    7: [
      [25, 25, 10.6, p.jade], [50, 36, 10.6, p.jade], [75, 47, 10.6, p.jade],
      [31, 76, 11.4, p.crimson], [69, 76, 11.4, p.crimson],
      [31, 107, 11.4, p.crimson], [69, 107, 11.4, p.crimson],
    ],
    8: [
      [31, 25, 11, p.cobalt], [69, 25, 11, p.cobalt],
      [31, 53.5, 11, p.cobalt], [69, 53.5, 11, p.cobalt],
      [31, 82.5, 11, p.cobalt], [69, 82.5, 11, p.cobalt],
      [31, 111, 11, p.cobalt], [69, 111, 11, p.cobalt],
    ],
    9: [
      [24, 30, 10.8, p.jade], [50, 30, 10.8, p.jade], [76, 30, 10.8, p.jade],
      [24, 68, 10.8, p.crimson], [50, 68, 10.8, p.crimson], [76, 68, 10.8, p.crimson],
      [24, 106, 10.8, p.cobalt], [50, 106, 10.8, p.cobalt], [76, 106, 10.8, p.cobalt],
    ],
  }[n];
  return c.map(([x, y, r, color]) => drawPinCircle(x, y, r, color, p)).join('');
}

function drawBambooStalk(cx, cy, h, color, p, angle = 0) {
  const w = 11.5;
  const top = cy - h * 0.5;
  const bot = cy + h * 0.5;
  return `
    <g transform="rotate(${angle} ${cx} ${cy})">
      <rect x="${(cx - w * 0.44).toFixed(1)}" y="${(top + 2).toFixed(1)}" width="${(w * 0.88).toFixed(1)}" height="${(h - 4).toFixed(1)}" rx="3.4" fill="${color}" stroke="${p.ink}" stroke-width="2"/>
      <rect x="${(cx - w * 0.56).toFixed(1)}" y="${(top - 1).toFixed(1)}" width="${(w * 1.12).toFixed(1)}" height="5.5" rx="2.5" fill="${color}" stroke="${p.ink}" stroke-width="1.8"/>
      <rect x="${(cx - w * 0.56).toFixed(1)}" y="${(cy - 2.6).toFixed(1)}" width="${(w * 1.12).toFixed(1)}" height="5.2" rx="2.5" fill="${color}" stroke="${p.ink}" stroke-width="1.8"/>
      <rect x="${(cx - w * 0.56).toFixed(1)}" y="${(bot - 4.5).toFixed(1)}" width="${(w * 1.12).toFixed(1)}" height="5.5" rx="2.5" fill="${color}" stroke="${p.ink}" stroke-width="1.8"/>
      <line x1="${cx}" y1="${(top + 5.5).toFixed(1)}" x2="${cx}" y2="${(bot - 5.5).toFixed(1)}" stroke="${p.ivory}" stroke-width="1.7" stroke-linecap="round" opacity="0.85"/>
    </g>
  `;
}

function drawOneBambooBird(p) {
  return `
    <g>
      <!-- Perch bamboo branch -->
      <path d="M18 106 Q50 98 82 106" fill="none" stroke="${p.jade}" stroke-width="6" stroke-linecap="round"/>
      <path d="M18 106 Q50 98 82 106" fill="none" stroke="${p.ink}" stroke-width="2" stroke-linecap="round"/>
      <!-- Tail feathers -->
      <path d="M40 84 L22 114 L34 114 L46 88 Z" fill="${p.jade}" stroke="${p.ink}" stroke-width="2" stroke-linejoin="round"/>
      <path d="M48 86 L42 118 L54 118 L55 86 Z" fill="${p.crimson}" stroke="${p.ink}" stroke-width="2" stroke-linejoin="round"/>
      <path d="M56 84 L64 114 L75 111 L61 82 Z" fill="${p.cobalt}" stroke="${p.ink}" stroke-width="2" stroke-linejoin="round"/>
      <!-- Bird body -->
      <ellipse cx="51" cy="67" rx="18" ry="22" fill="${p.jade}" stroke="${p.ink}" stroke-width="2.4"/>
      <ellipse cx="53" cy="70" rx="10.5" ry="13" fill="${p.ivory}" stroke="${p.ink}" stroke-width="1.6"/>
      <!-- Wing -->
      <path d="M35 58 C27 67 30 82 44 82 C42 71 42 64 35 58 Z" fill="${p.cobalt}" stroke="${p.ink}" stroke-width="2" stroke-linejoin="round"/>
      <path d="M67 58 C75 67 72 82 58 82 C60 71 60 64 67 58 Z" fill="${p.cobalt}" stroke="${p.ink}" stroke-width="2" stroke-linejoin="round"/>
      <!-- Head & crest -->
      <circle cx="52" cy="40" r="13" fill="${p.jade}" stroke="${p.ink}" stroke-width="2.3"/>
      <path d="M46 28 C44 17 53 16 53 27 C57 18 65 21 58 30 Z" fill="${p.crimson}" stroke="${p.ink}" stroke-width="1.9" stroke-linejoin="round"/>
      <!-- Beak & eye -->
      <path d="M62 39 L76 42 L62 46 Z" fill="${p.gold}" stroke="${p.ink}" stroke-width="1.8" stroke-linejoin="round"/>
      <circle cx="56" cy="39" r="3" fill="${p.ivory}" stroke="${p.ink}" stroke-width="1.4"/>
      <circle cx="56.8" cy="39" r="1.4" fill="${p.ink}"/>
      <!-- Feet -->
      <line x1="45" y1="88" x2="43" y2="102" stroke="${p.ink}" stroke-width="2.4" stroke-linecap="round"/>
      <line x1="57" y1="88" x2="59" y2="102" stroke="${p.ink}" stroke-width="2.4" stroke-linecap="round"/>
    </g>
  `;
}

function renderSouSuit(n, p) {
  if (n === 1) return drawOneBambooBird(p);
  if (n === 2) {
    return [
      drawBambooStalk(50, 42, 38, p.jade, p),
      drawBambooStalk(50, 94, 38, p.cobalt, p),
    ].join('');
  }
  if (n === 3) {
    return [
      drawBambooStalk(50, 42, 36, p.jade, p),
      drawBambooStalk(32, 94, 36, p.cobalt, p),
      drawBambooStalk(68, 94, 36, p.jade, p),
    ].join('');
  }
  if (n === 4) {
    return [
      drawBambooStalk(32, 42, 36, p.cobalt, p),
      drawBambooStalk(68, 42, 36, p.jade, p),
      drawBambooStalk(32, 94, 36, p.jade, p),
      drawBambooStalk(68, 94, 36, p.cobalt, p),
    ].join('');
  }
  if (n === 5) {
    return [
      drawBambooStalk(27, 38, 34, p.jade, p),
      drawBambooStalk(73, 38, 34, p.cobalt, p),
      drawBambooStalk(50, 68, 34, p.crimson, p),
      drawBambooStalk(27, 98, 34, p.cobalt, p),
      drawBambooStalk(73, 98, 34, p.jade, p),
    ].join('');
  }
  if (n === 6) {
    return [
      drawBambooStalk(26, 42, 36, p.jade, p),
      drawBambooStalk(50, 42, 36, p.jade, p),
      drawBambooStalk(74, 42, 36, p.jade, p),
      drawBambooStalk(26, 94, 36, p.jade, p),
      drawBambooStalk(50, 94, 36, p.jade, p),
      drawBambooStalk(74, 94, 36, p.jade, p),
    ].join('');
  }
  if (n === 7) {
    return [
      drawBambooStalk(50, 30, 29, p.crimson, p),
      drawBambooStalk(26, 68, 29, p.jade, p),
      drawBambooStalk(50, 68, 29, p.cobalt, p),
      drawBambooStalk(74, 68, 29, p.jade, p),
      drawBambooStalk(26, 106, 29, p.jade, p),
      drawBambooStalk(50, 106, 29, p.cobalt, p),
      drawBambooStalk(74, 106, 29, p.jade, p),
    ].join('');
  }
  if (n === 8) {
    return [
      drawBambooStalk(22, 42, 34, p.jade, p),
      drawBambooStalk(41, 44, 31, p.cobalt, p, 26),
      drawBambooStalk(59, 44, 31, p.cobalt, p, -26),
      drawBambooStalk(78, 42, 34, p.jade, p),
      drawBambooStalk(22, 95, 34, p.jade, p),
      drawBambooStalk(41, 93, 31, p.cobalt, p, -26),
      drawBambooStalk(59, 93, 31, p.cobalt, p, 26),
      drawBambooStalk(78, 95, 34, p.jade, p),
    ].join('');
  }
  // 9s: 3 rows x 3 columns (jade / crimson / cobalt)
  return [30, 68, 106]
    .flatMap((y) => [
      drawBambooStalk(25, y, 28, p.jade, p),
      drawBambooStalk(50, y, 28, p.crimson, p),
      drawBambooStalk(75, y, 28, p.cobalt, p),
    ])
    .join('');
}

function renderManSuit(n, p) {
  const numChar = CN_NUM[n];
  return `
    <g>
      <text x="50" y="54" text-anchor="middle" font-family="'Songti SC', 'STSong', 'Noto Serif CJK SC', 'PingFang SC', serif" font-weight="900" font-size="40" fill="${p.ink}" stroke="${p.ink}" stroke-width="0.6">${numChar}</text>
      <text x="50" y="109" text-anchor="middle" font-family="'Songti SC', 'STSong', 'Noto Serif CJK SC', 'PingFang SC', serif" font-weight="900" font-size="45" fill="${p.crimson}" stroke="${p.ink}" stroke-width="1.2">萬</text>
    </g>
  `;
}

function renderHonorSuit(code, p) {
  if (code === '7z') {
    // Classic double-bordered cobalt frame for 白板
    return `
      <g>
        <rect x="20" y="24" width="60" height="88" rx="6" fill="none" stroke="${p.cobalt}" stroke-width="5"/>
        <rect x="27" y="31" width="46" height="74" rx="3" fill="none" stroke="${p.ink}" stroke-width="1.8" stroke-dasharray="6 4"/>
        <path d="M20 38 L29 24 M71 24 L80 38 M20 98 L29 112 M71 112 L80 98" stroke="${p.cobalt}" stroke-width="3" stroke-linecap="round"/>
      </g>
    `;
  }
  const glyph = HONOR_GLYPHS[code];
  const color = code === '5z' ? p.crimson : code === '6z' ? p.jade : p.ink;
  const stroke = code === '5z' || code === '6z' ? p.ink : 'none';
  return `
    <g>
      <text x="50" y="89" text-anchor="middle" font-family="'Songti SC', 'STSong', 'Noto Serif CJK SC', 'PingFang SC', serif" font-weight="900" font-size="60" fill="${color}" stroke="${stroke}" stroke-width="1.2">${glyph}</text>
    </g>
  `;
}

export function renderTileFaceSvg(tileCode, variant = 'A') {
  const p = PALETTES[variant] || PALETTES.A;
  const suit = tileCode[1];
  const n = Number(tileCode[0]);
  let inner = '';
  if (suit === 'm') inner = renderManSuit(n, p);
  else if (suit === 'p') inner = renderPinSuit(n, p);
  else if (suit === 's') inner = renderSouSuit(n, p);
  else inner = renderHonorSuit(tileCode, p);

  return `<svg class="tile-svg" viewBox="0 0 100 136" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
    <rect x="3" y="3" width="94" height="130" rx="10" fill="${p.ivory}" />
    <path d="M8 15 Q11 8 19 8 L82 8" fill="none" stroke="#FFFFFF" stroke-width="2.5" opacity="0.7" stroke-linecap="round"/>
    <path d="M12 126 L88 126" fill="none" stroke="${p.ivoryShadow}" stroke-width="3" stroke-linecap="round"/>
    ${inner}
  </svg>`;
}

export function createHandTile(tileCode, opts = {}) {
  const {
    variant = 'A',
    isDrawn = false,
    selected = false,
    focused = false,
    interactive = true,
    size = 'hand', // 'hand' | 'mini' | 'settlement'
  } = opts;

  const el = document.createElement(interactive ? 'button' : 'div');
  if (interactive) el.type = 'button';
  el.className = `mj-tile mj-tile--${size} variant-${variant}`;
  if (isDrawn) el.classList.add('is-drawn');
  if (selected) el.classList.add('is-selected');
  if (focused) el.classList.add('is-focused');
  el.dataset.tile = tileCode;
  el.setAttribute('aria-label', getTileName(tileCode));
  el.innerHTML = `<span class="mj-tile-body">${renderTileFaceSvg(tileCode, variant)}</span>`;
  return el;
}

export function createRiverTile(discardObj, opts = {}) {
  const {
    variant = 'A',
    isLatest = false,
    orientation = 'bottom', // 'bottom' | 'right' | 'top' | 'left'
    isGhost = false,
  } = opts;

  const tileCode = typeof discardObj === 'string' ? discardObj : discardObj.tile;
  const isTsumogiri = typeof discardObj === 'object' ? Boolean(discardObj.isTsumogiri) : false;

  const el = document.createElement('div');
  el.className = `mj-river-tile orient-${orientation} variant-${variant}`;
  if (isTsumogiri) el.classList.add('is-tsumogiri');
  if (isLatest) el.classList.add('is-latest');
  if (isGhost) el.classList.add('is-ghost');
  el.dataset.tile = tileCode;
  el.title = `${getTileName(tileCode)}${isTsumogiri ? '（摸切）' : '（手切）'}`;
  el.innerHTML = `<span class="mj-tile-body">${renderTileFaceSvg(tileCode, variant)}</span>`;
  return el;
}

export function createTileBack(opts = {}) {
  const { variant = 'A', orientation = 'top' } = opts;
  const el = document.createElement('div');
  el.className = `mj-back mj-back--${orientation} variant-${variant}`;
  return el;
}

export function createMeldGroup(meld, opts = {}) {
  const { variant = 'A' } = opts;
  const wrap = document.createElement('div');
  wrap.className = `mj-meld mj-meld--${meld.kind} variant-${variant}`;

  if (meld.kind === 'ankong') {
    meld.tiles.forEach((t, idx) => {
      if (idx === 0 || idx === 3) {
        const back = createTileBack({ variant, orientation: 'flat' });
        back.classList.add('mj-meld-tile');
        wrap.appendChild(back);
      } else {
        const face = createHandTile(t, { variant, interactive: false, size: 'meld' });
        face.classList.add('mj-meld-tile');
        wrap.appendChild(face);
      }
    });
    return wrap;
  }

  if (meld.kind === 'bukong') {
    meld.tiles.slice(0, 3).forEach((t, idx) => {
      if (idx === 1) {
        const stack = document.createElement('div');
        stack.className = 'mj-meld-stack';
        const base = createHandTile(t, { variant, interactive: false, size: 'meld' });
        base.classList.add('mj-meld-tile', 'is-sideways');
        const cap = createHandTile(meld.tiles[3] || t, { variant, interactive: false, size: 'meld' });
        cap.classList.add('mj-meld-tile', 'is-sideways', 'is-stacked');
        stack.append(base, cap);
        wrap.appendChild(stack);
      } else {
        const face = createHandTile(t, { variant, interactive: false, size: 'meld' });
        face.classList.add('mj-meld-tile');
        wrap.appendChild(face);
      }
    });
    return wrap;
  }

  // pon or mingkong
  const sidewaysIndex = meld.kind === 'mingkong' ? 1 : 1;
  meld.tiles.forEach((t, idx) => {
    const face = createHandTile(t, { variant, interactive: false, size: 'meld' });
    face.classList.add('mj-meld-tile');
    if (idx === sidewaysIndex) face.classList.add('is-sideways');
    wrap.appendChild(face);
  });

  return wrap;
}
