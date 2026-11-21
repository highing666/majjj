// PROTOTYPE: Pure in-memory Tuidaohu table state + interactive mock driver.
// Strict GLOSSARY.md vocabulary, NO '吃', NO in-game beginner hints (听牌/荐牌).

import { ALL_TILES, getTileName, sortTiles } from './tiles.js';
import { audioRouter } from './audio.js';

function makeBasePlayers() {
  return [
    {
      seat: 0,
      name: '我',
      wind: '东',
      isHuman: true,
      isDealer: true,
      score: 100,
      lastDelta: 0,
      hand: [],
      drawnTile: null,
      melds: [],
      river: [],
    },
    {
      seat: 1,
      name: '阿杰',
      wind: '南',
      isHuman: false,
      isDealer: false,
      score: 100,
      lastDelta: 0,
      hand: [],
      drawnTile: null,
      melds: [],
      river: [],
    },
    {
      seat: 2,
      name: '蔚',
      wind: '西',
      isHuman: false,
      isDealer: false,
      score: 100,
      lastDelta: 0,
      hand: [],
      drawnTile: null,
      melds: [],
      river: [],
    },
    {
      seat: 3,
      name: '金克丝',
      wind: '北',
      isHuman: false,
      isDealer: false,
      score: 100,
      lastDelta: 0,
      hand: [],
      drawnTile: null,
      melds: [],
      river: [],
    },
  ];
}

function createWallBag(excludeList) {
  const counts = new Map();
  for (const t of ALL_TILES) counts.set(t, 4);
  for (const t of excludeList) {
    counts.set(t, Math.max(0, (counts.get(t) || 0) - 1));
  }
  const bag = [];
  for (const [t, c] of counts.entries()) {
    for (let i = 0; i < c; i++) bag.push(t);
  }
  for (let i = bag.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [bag[i], bag[j]] = [bag[j], bag[i]];
  }
  return bag;
}

function makeDummyOpponentHand(count) {
  return Array.from({ length: count }, () => '1m');
}

export class PrototypeTableStore {
  constructor() {
    this.listeners = new Set();
    this.impactListeners = new Set();
    this.aiTimer = null;
    this.state = null;
    this.loadNormalTurn(false);
  }

  subscribe(fn) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  onImpact(fn) {
    this.impactListeners.add(fn);
    return () => this.impactListeners.delete(fn);
  }

  emitImpact(payload) {
    this.impactListeners.forEach((fn) => fn(payload));
  }

  notify() {
    const snap = this.getState();
    this.listeners.forEach((fn) => fn(snap));
  }

  getState() {
    return this.state;
  }

  clearAiTimer() {
    if (this.aiTimer) {
      clearTimeout(this.aiTimer);
      this.aiTimer = null;
    }
  }

  pushLog(text, semanticEvent = null) {
    this.state.seq += 1;
    this.state.lastAction = text;
    if (semanticEvent) {
      this.state.lastSemanticEvent = { ...semanticEvent, seq: this.state.seq };
    }
    this.state.eventLog = [
      { seq: this.state.seq, text },
      ...this.state.eventLog,
    ].slice(0, 14);
  }

  buildPlayerTurnSelfCalls(hero) {
    const calls = [];
    // Check 暗杠 (4 identical in standing + drawn)
    const allHand = hero.drawnTile ? [...hero.hand, hero.drawnTile] : [...hero.hand];
    const counts = new Map();
    for (const t of allHand) counts.set(t, (counts.get(t) || 0) + 1);
    for (const [tile, count] of counts.entries()) {
      if (count === 4) {
        calls.push({ type: 'ankong', label: '暗杠', tile });
      }
    }
    // Check 补杠 (matching existing pon meld)
    for (const meld of hero.melds) {
      if (meld.kind === 'pon' && allHand.includes(meld.tiles[0])) {
        calls.push({ type: 'bukong', label: '补杠', tile: meld.tiles[0] });
      }
    }
    // Check scripted/sample 自摸 if in winning hand
    if (this.isSimpleWinningHand(allHand)) {
      calls.push({
        type: 'hu',
        label: '自摸',
        tile: hero.drawnTile || hero.hand[hero.hand.length - 1],
        winType: '自摸',
      });
    }
    return calls;
  }

  isSimpleWinningHand(tiles) {
    if (tiles.length % 3 !== 2) return false;
    // Fast clean check for standard 4 melds + 1 pair so natural free-play hu works if lucky
    const counts = new Map();
    for (const t of tiles) counts.set(t, (counts.get(t) || 0) + 1);
    for (const [pairTile, c] of counts.entries()) {
      if (c >= 2) {
        const rem = new Map(counts);
        rem.set(pairTile, c - 2);
        if (this.canFormMelds(rem)) return true;
      }
    }
    return false;
  }

  canFormMelds(counts) {
    let first = null;
    for (const t of ALL_TILES) {
      if ((counts.get(t) || 0) > 0) {
        first = t;
        break;
      }
    }
    if (!first) return true;
    const c = counts.get(first);
    if (c >= 3) {
      counts.set(first, c - 3);
      if (this.canFormMelds(counts)) return true;
      counts.set(first, c);
    }
    const suit = first[1];
    const n = Number(first[0]);
    if (suit !== 'z' && n <= 7) {
      const t2 = `${n + 1}${suit}`;
      const t3 = `${n + 2}${suit}`;
      if ((counts.get(t2) || 0) > 0 && (counts.get(t3) || 0) > 0) {
        counts.set(first, c - 1);
        counts.set(t2, counts.get(t2) - 1);
        counts.set(t3, counts.get(t3) - 1);
        if (this.canFormMelds(counts)) return true;
        counts.set(first, c);
        counts.set(t2, counts.get(t2) + 1);
        counts.set(t3, counts.get(t3) + 1);
      }
    }
    return false;
  }

  // Scenario 1: Normal mid-hand hero turn
  loadNormalTurn(emitDrawAudio = true) {
    this.clearAiTimer();
    const players = makeBasePlayers();
    players[0].hand = sortTiles(['2m', '3m', '4m', '5m', '5m', '3p', '4p', '5p', '6s', '7s', '8s', '1z', '1z']);
    players[0].drawnTile = '6m';
    players[0].river = [
      { tile: '4z', isTsumogiri: false },
      { tile: '9p', isTsumogiri: true },
      { tile: '1s', isTsumogiri: false },
      { tile: '7z', isTsumogiri: true },
      { tile: '9m', isTsumogiri: false },
    ];

    players[1].hand = makeDummyOpponentHand(10);
    players[1].melds = [{ kind: 'pon', tiles: ['9p', '9p', '9p'], fromSeat: 2 }];
    players[1].river = [
      { tile: '2z', isTsumogiri: false },
      { tile: '8m', isTsumogiri: true },
      { tile: '1p', isTsumogiri: false },
      { tile: '6z', isTsumogiri: true },
      { tile: '4s', isTsumogiri: false },
    ];

    players[2].hand = makeDummyOpponentHand(13);
    players[2].river = [
      { tile: '3z', isTsumogiri: true },
      { tile: '1m', isTsumogiri: false },
      { tile: '9s', isTsumogiri: true },
      { tile: '2p', isTsumogiri: false },
      { tile: '7p', isTsumogiri: true },
    ];

    players[3].hand = makeDummyOpponentHand(13);
    players[3].river = [
      { tile: '1m', isTsumogiri: false },
      { tile: '9s', isTsumogiri: false },
      { tile: '3z', isTsumogiri: true },
      { tile: '8p', isTsumogiri: false },
      { tile: '2s', isTsumogiri: true },
    ];

    const used = [
      ...players[0].hand,
      players[0].drawnTile,
      ...players[0].river.map((r) => r.tile),
      ...players[1].melds.flatMap((m) => m.tiles),
      ...players[1].river.map((r) => r.tile),
      ...players[2].river.map((r) => r.tile),
      ...players[3].river.map((r) => r.tile),
    ];
    const wallBag = createWallBag(used);

    this.state = {
      scenarioKey: 'normal',
      mode: '东风战',
      roundLabel: '东一局',
      renchan: 0,
      wallRemaining: 58,
      wallBag,
      phase: 'player_turn',
      turnSeat: 0,
      seq: 12,
      players,
      pendingCalls: [],
      furitenFlags: { 过水碰: [], 过水胡: false },
      latestDiscardSeat: 3,
      lastAction: '轮到我摸进 六万，等待舍牌',
      lastSemanticEvent: { type: 'TileDrawn', seat: 0, tile: '6m', seq: 12 },
      eventLog: [
        { seq: 12, text: '我 从牌墙摸入 六万' },
        { seq: 11, text: '金克丝 摸切 二条' },
        { seq: 10, text: '蔚 摸切 七筒' },
      ],
      settlement: null,
    };
    this.state.pendingCalls = this.buildPlayerTurnSelfCalls(players[0]);
    if (emitDrawAudio) {
      audioRouter.emit({ type: 'draw', seat: 0, tile: '6m' });
    }
    this.notify();
  }

  // Scenario 2: Pon window (下家 just discarded 五万)
  loadPonScenario() {
    this.clearAiTimer();
    const players = makeBasePlayers();
    players[0].hand = sortTiles(['2m', '3m', '4m', '5m', '5m', '4p', '5p', '6p', '7s', '8s', '9s', '5z', '5z']);
    players[0].drawnTile = null;
    players[0].river = [
      { tile: '4z', isTsumogiri: false },
      { tile: '1p', isTsumogiri: true },
      { tile: '9s', isTsumogiri: false },
      { tile: '7z', isTsumogiri: true },
    ];

    players[1].hand = makeDummyOpponentHand(13);
    players[1].river = [
      { tile: '2z', isTsumogiri: true },
      { tile: '9m', isTsumogiri: false },
      { tile: '3s', isTsumogiri: true },
      { tile: '5m', isTsumogiri: false },
    ];

    players[2].hand = makeDummyOpponentHand(13);
    players[2].river = [
      { tile: '1m', isTsumogiri: false },
      { tile: '8p', isTsumogiri: true },
      { tile: '6z', isTsumogiri: false },
    ];

    players[3].hand = makeDummyOpponentHand(13);
    players[3].river = [
      { tile: '3z', isTsumogiri: true },
      { tile: '2p', isTsumogiri: false },
      { tile: '4s', isTsumogiri: true },
    ];

    this.state = {
      scenarioKey: 'pon',
      mode: '东风战',
      roundLabel: '东一局',
      renchan: 0,
      wallRemaining: 54,
      wallBag: createWallBag(players[0].hand),
      phase: 'call_window',
      turnSeat: 1,
      seq: 18,
      players,
      pendingCalls: [
        { type: 'pon', label: '碰', tile: '5m', fromSeat: 1 },
        { type: 'pass', label: '过', tile: '5m', fromSeat: 1 },
      ],
      furitenFlags: { 过水碰: [], 过水胡: false },
      latestDiscardSeat: 1,
      lastAction: '阿杰 手切 五万，我可选择 碰 / 过',
      lastSemanticEvent: { type: 'CallWindowOpened', seat: 0, tile: '5m', options: ['pon', 'pass'], seq: 18 },
      eventLog: [
        { seq: 18, text: '阿杰 手切 五万（触发鸣牌窗口）' },
        { seq: 17, text: '我 手切 九条' },
      ],
      settlement: null,
    };
    audioRouter.emit({ type: 'discard_soft', seat: 1, tile: '5m' });
    this.notify();
  }

  // Scenario 3: Ankong (暗杠) & Bukong (补杠), or Mingkong (明杠)
  loadKongScenario(subKind = 'self') {
    this.clearAiTimer();
    const players = makeBasePlayers();

    if (subKind === 'mingkong') {
      players[0].hand = sortTiles(['2m', '3m', '4m', '6p', '7p', '8p', '9s', '9s', '9s', '1z', '1z', '4s', '5s']);
      players[0].drawnTile = null;
      players[0].river = [
        { tile: '4z', isTsumogiri: false },
        { tile: '1p', isTsumogiri: true },
        { tile: '2z', isTsumogiri: false },
      ];
      players[1].hand = makeDummyOpponentHand(13);
      players[1].river = [{ tile: '8m', isTsumogiri: false }, { tile: '3p', isTsumogiri: true }];
      players[2].hand = makeDummyOpponentHand(13);
      players[2].river = [{ tile: '7z', isTsumogiri: true }, { tile: '1m', isTsumogiri: false }];
      players[3].hand = makeDummyOpponentHand(13);
      players[3].river = [{ tile: '6z', isTsumogiri: false }, { tile: '9s', isTsumogiri: false }];

      this.state = {
        scenarioKey: 'mingkong',
        mode: '东风战',
        roundLabel: '东二局',
        renchan: 0,
        wallRemaining: 49,
        wallBag: createWallBag(players[0].hand),
        phase: 'call_window',
        turnSeat: 3,
        seq: 23,
        players,
        pendingCalls: [
          { type: 'mingkong', label: '明杠', tile: '9s', fromSeat: 3 },
          { type: 'pon', label: '碰', tile: '9s', fromSeat: 3 },
          { type: 'pass', label: '过', tile: '9s', fromSeat: 3 },
        ],
        furitenFlags: { 过水碰: [], 过水胡: false },
        latestDiscardSeat: 3,
        lastAction: '金克丝 手切 九条，我可选择 明杠 / 碰 / 过',
        lastSemanticEvent: { type: 'CallWindowOpened', seat: 0, tile: '9s', options: ['mingkong', 'pon', 'pass'], seq: 23 },
        eventLog: [
          { seq: 23, text: '金克丝 手切 九条（触发明杠窗口）' },
        ],
        settlement: null,
      };
      audioRouter.emit({ type: 'discard_soft', seat: 3, tile: '9s' });
      this.notify();
      return;
    }

    // Default: hero turn with 暗杠 (8p x4) and 补杠 (3m pon + drawn 3m)
    players[0].melds = [{ kind: 'pon', tiles: ['3m', '3m', '3m'], fromSeat: 1 }];
    players[0].hand = sortTiles(['5m', '6m', '7m', '8p', '8p', '8p', '8p', '2s', '3s', '4s']);
    players[0].drawnTile = '3m';
    players[0].river = [
      { tile: '4z', isTsumogiri: false },
      { tile: '1s', isTsumogiri: true },
      { tile: '9m', isTsumogiri: false },
      { tile: '7z', isTsumogiri: true },
    ];

    players[1].hand = makeDummyOpponentHand(13);
    players[1].river = [{ tile: '2z', isTsumogiri: false }, { tile: '8m', isTsumogiri: true }, { tile: '1p', isTsumogiri: false }];
    players[2].hand = makeDummyOpponentHand(13);
    players[2].river = [{ tile: '9s', isTsumogiri: true }, { tile: '3z', isTsumogiri: false }, { tile: '6z', isTsumogiri: true }];
    players[3].hand = makeDummyOpponentHand(13);
    players[3].river = [{ tile: '1m', isTsumogiri: false }, { tile: '5p', isTsumogiri: true }, { tile: '9p', isTsumogiri: false }];

    this.state = {
      scenarioKey: 'kong',
      mode: '东风战',
      roundLabel: '东二局',
      renchan: 0,
      wallRemaining: 46,
      wallBag: createWallBag([...players[0].hand, '3m', '3m', '3m', '3m']),
      phase: 'player_turn',
      turnSeat: 0,
      seq: 21,
      players,
      pendingCalls: [],
      furitenFlags: { 过水碰: [], 过水胡: false },
      latestDiscardSeat: 3,
      lastAction: '摸进 三万：可 暗杠(八筒) 或 补杠(三万)',
      lastSemanticEvent: { type: 'TileDrawn', seat: 0, tile: '3m', seq: 21 },
      eventLog: [
        { seq: 21, text: '我 从牌墙摸入 三万（手牌同时满足暗杠与补杠）' },
        { seq: 20, text: '金克丝 手切 九筒' },
      ],
      settlement: null,
    };
    this.state.pendingCalls = this.buildPlayerTurnSelfCalls(players[0]);
    audioRouter.emit({ type: 'draw', seat: 0, tile: '3m' });
    this.notify();
  }

  // Scenario 4: Robbing a kong (抢杠胡)
  loadRobKongScenario() {
    this.clearAiTimer();
    const players = makeBasePlayers();
    players[0].hand = sortTiles(['1m', '2m', '3m', '4p', '5p', '6p', '7s', '8s', '9s', '2p', '2p', '2p', '5z']);
    players[0].drawnTile = null;
    players[0].river = [
      { tile: '4z', isTsumogiri: false },
      { tile: '1s', isTsumogiri: true },
      { tile: '9m', isTsumogiri: false },
      { tile: '7z', isTsumogiri: true },
      { tile: '3z', isTsumogiri: false },
    ];

    players[1].hand = makeDummyOpponentHand(13);
    players[1].river = [{ tile: '2z', isTsumogiri: false }, { tile: '8m', isTsumogiri: true }, { tile: '1p', isTsumogiri: false }, { tile: '4s', isTsumogiri: true }];

    players[2].hand = makeDummyOpponentHand(10);
    players[2].melds = [{ kind: 'pon', tiles: ['5z', '5z', '5z'], fromSeat: 1 }];
    players[2].river = [{ tile: '9s', isTsumogiri: true }, { tile: '3m', isTsumogiri: false }, { tile: '6z', isTsumogiri: true }, { tile: '8p', isTsumogiri: false }];

    players[3].hand = makeDummyOpponentHand(13);
    players[3].river = [{ tile: '1p', isTsumogiri: false }, { tile: '5m', isTsumogiri: true }, { tile: '2s', isTsumogiri: false }, { tile: '9p', isTsumogiri: true }];

    this.state = {
      scenarioKey: 'robkong',
      mode: '东风战',
      roundLabel: '东三局',
      renchan: 1,
      wallRemaining: 32,
      wallBag: createWallBag(players[0].hand),
      phase: 'rob_kong_window',
      turnSeat: 2,
      seq: 34,
      players,
      pendingCalls: [
        { type: 'hu', label: '抢杠胡', tile: '5z', fromSeat: 2, winType: '抢杠胡' },
        { type: 'pass', label: '过', tile: '5z', fromSeat: 2 },
      ],
      furitenFlags: { 过水碰: [], 过水胡: false },
      latestDiscardSeat: 1,
      lastAction: '蔚 宣布补杠 红中，我可 抢杠胡 / 过',
      lastSemanticEvent: { type: 'RobKongWindowOpened', seat: 0, actorSeat: 2, tile: '5z', seq: 34 },
      eventLog: [
        { seq: 34, text: '蔚 试图补杠 红中（等待抢杠胡裁决）' },
        { seq: 33, text: '蔚 摸入 红中' },
      ],
      settlement: null,
    };
    audioRouter.emit({ type: 'meld_pon', seat: 2, tile: '5z' });
    this.notify();
  }

  // Scenario 5: Settlement modal (小局结算)
  loadSettlementScenario(winType = '自摸') {
    this.clearAiTimer();
    const players = makeBasePlayers();

    players[0].melds = [{ kind: 'ankong', tiles: ['4m', '4m', '4m', '4m'], fromSeat: 0 }];
    // Clean 10 standing + 1 ankong + 1 winning tile = complete 14-tile hand
    players[0].hand = sortTiles(['2m', '3m', '4m', '5m', '6m', '7m', '8m', '8m', '1m', '1m']);
    players[0].drawnTile = '1m';
    players[0].river = [
      { tile: '4z', isTsumogiri: false },
      { tile: '1s', isTsumogiri: true },
      { tile: '9p', isTsumogiri: false },
      { tile: '7z', isTsumogiri: true },
    ];

    players[1].hand = sortTiles(['3p', '4p', '5p', '6s', '7s', '8s', '2z', '2z', '2z', '5s', '6s', '1p', '9p']);
    players[1].river = [{ tile: '2z', isTsumogiri: false }, { tile: '8p', isTsumogiri: true }];

    players[2].hand = sortTiles(['1s', '2s', '3s', '4s', '4s', '7p', '8p', '9p', '5z', '5z', '3z', '3z', '6p']);
    players[2].river = [{ tile: '9s', isTsumogiri: true }, { tile: '6z', isTsumogiri: false }];

    players[3].hand = sortTiles(['2p', '3p', '4p', '5p', '5p', '6s', '7s', '8s', '1z', '1z', '4z', '4z', '8p']);
    players[3].river = [{ tile: '3z', isTsumogiri: false }, { tile: '9p', isTsumogiri: true }];

    const items =
      winType === '抢杠胡'
        ? [
            { label: '抢杠胡', fan: 1 },
            { label: '门前清', fan: 1 },
          ]
        : [
            { label: '清一色', fan: 2 },
            { label: '门前清', fan: 1 },
            { label: '自摸', fan: 1 },
          ];
    const rawFan = items.reduce((sum, it) => sum + it.fan, 0);
    const totalFan = Math.min(4, rawFan);
    const multiplier = Math.pow(2, totalFan); // 4番 16× 封顶
    const baseScore = 1 * multiplier;
    const scoreDeltas =
      winType === '自摸'
        ? [baseScore * 3 + 6, -baseScore - 2, -baseScore - 2, -baseScore - 2]
        : [baseScore + 6, -2, -baseScore - 2, -2];

    players.forEach((p, idx) => {
      p.lastDelta = scoreDeltas[idx];
      p.score += scoreDeltas[idx];
    });

    this.state = {
      scenarioKey: 'settlement',
      mode: '东风战',
      roundLabel: '东一局',
      renchan: 0,
      wallRemaining: 28,
      wallBag: [],
      phase: 'settlement',
      turnSeat: 0,
      seq: 42,
      players,
      pendingCalls: [],
      furitenFlags: { 过水碰: [], 过水胡: false },
      latestDiscardSeat: 0,
      lastAction: `我 ${winType} 胡牌！总 ${totalFan} 番（${multiplier}×）`,
      lastSemanticEvent: { type: 'HandSettled', winnerSeat: 0, winType, totalFan, seq: 42 },
      eventLog: [
        { seq: 42, text: `小局结算：我 ${winType}（${totalFan}番 ${multiplier}×）` },
        { seq: 41, text: '我 暗杠 四万（杠分 +2/家）' },
      ],
      settlement: {
        winType,
        winnerSeat: 0,
        loserSeat: winType === '自摸' ? null : 2,
        winTile: '1m',
        items,
        rawFan,
        totalFan,
        capped: rawFan >= 4,
        multiplier,
        kongDeltas: [6, -2, -2, -2],
        scoreDeltas,
      },
    };

    audioRouter.emit({ type: 'hu', seat: 0, tile: '1m' });
    this.emitImpact({
      kind: 'hu',
      label: winType === '自摸' ? '自摸!' : winType === '抢杠胡' ? '抢杠胡!' : '胡!',
      sublabel: `${totalFan}番 · ${multiplier}×`,
      seat: 0,
    });
    this.notify();
  }

  discardTile(indexOrDrawn, slam = false) {
    if (!this.state || this.state.phase !== 'player_turn' || this.state.turnSeat !== 0) return false;
    const hero = this.state.players[0];
    let discarded = null;
    let isTsumogiri = false;

    if (indexOrDrawn === 'drawn') {
      if (!hero.drawnTile) return false;
      discarded = hero.drawnTile;
      isTsumogiri = true;
      hero.drawnTile = null;
    } else {
      const idx = Number(indexOrDrawn);
      if (Number.isNaN(idx) || idx < 0 || idx >= hero.hand.length) return false;
      discarded = hero.hand.splice(idx, 1)[0];
      if (hero.drawnTile) {
        hero.hand.push(hero.drawnTile);
        hero.drawnTile = null;
      }
      hero.hand = sortTiles(hero.hand);
      isTsumogiri = false;
    }

    hero.river.push({ tile: discarded, isTsumogiri });
    this.state.latestDiscardSeat = 0;
    this.state.pendingCalls = [];
    this.state.phase = 'ai_thinking';
    this.state.turnSeat = 1;
    this.pushLog(
      `我 ${isTsumogiri ? '摸切' : '手切'} ${getTileName(discarded)}`,
      { type: 'TileDiscarded', seat: 0, tile: discarded, isTsumogiri }
    );

    audioRouter.emit({
      type: slam ? 'discard_slam' : 'discard_soft',
      seat: 0,
      tile: discarded,
    });

    this.notify();
    this.scheduleNextAiTurn();
    return true;
  }

  scheduleNextAiTurn() {
    this.clearAiTimer();
    this.aiTimer = setTimeout(() => {
      if (!this.state || this.state.phase !== 'ai_thinking') return;
      const seat = this.state.turnSeat;
      if (seat === 0) {
        this.heroDrawFromWall();
        return;
      }

      if (this.state.wallRemaining <= 0) {
        this.triggerDrawExhausted();
        return;
      }

      const ai = this.state.players[seat];
      const drawn = this.state.wallBag.pop() || '5z';
      this.state.wallRemaining = Math.max(0, this.state.wallRemaining - 1);

      // Occasionally let AI keep drawn tile and hand-discard a tile that matches hero's pair for lively demo
      const heroCounts = new Map();
      for (const t of this.state.players[0].hand) {
        heroCounts.set(t, (heroCounts.get(t) || 0) + 1);
      }

      let discard = drawn;
      let isTsumogiri = true;
      const candidatePool = ALL_TILES.filter(
        (t) => (heroCounts.get(t) || 0) >= 2 && !this.state.furitenFlags.过水碰.includes(t)
      );

      if (candidatePool.length > 0 && Math.random() < 0.28) {
        discard = candidatePool[0];
        isTsumogiri = false;
      } else if (Math.random() < 0.45) {
        isTsumogiri = false;
      }

      ai.river.push({ tile: discard, isTsumogiri });
      this.state.latestDiscardSeat = seat;
      this.pushLog(
        `${ai.name} ${isTsumogiri ? '摸切' : '手切'} ${getTileName(discard)}`,
        { type: 'TileDiscarded', seat, tile: discard, isTsumogiri }
      );
      audioRouter.emit({ type: 'discard_soft', seat, tile: discard });

      // Check if hero can 碰 or 明杠 (unless blocked by 过水碰)
      const heroMatch = heroCounts.get(discard) || 0;
      if (heroMatch >= 2 && !this.state.furitenFlags.过水碰.includes(discard)) {
        const calls = [];
        if (heroMatch >= 3) {
          calls.push({ type: 'mingkong', label: '明杠', tile: discard, fromSeat: seat });
        }
        calls.push({ type: 'pon', label: '碰', tile: discard, fromSeat: seat });
        calls.push({ type: 'pass', label: '过', tile: discard, fromSeat: seat });
        this.state.phase = 'call_window';
        this.state.pendingCalls = calls;
        this.pushLog(`可响应 ${ai.name} 打出的 ${getTileName(discard)}`, {
          type: 'CallWindowOpened',
          seat: 0,
          tile: discard,
        });
        this.notify();
        return;
      }

      this.state.turnSeat = (seat + 1) % 4;
      this.notify();
      this.scheduleNextAiTurn();
    }, 460);
  }

  heroDrawFromWall() {
    if (this.state.wallRemaining <= 0) {
      this.triggerDrawExhausted();
      return;
    }
    const hero = this.state.players[0];
    const drawn = this.state.wallBag.pop() || '6m';
    this.state.wallRemaining = Math.max(0, this.state.wallRemaining - 1);
    hero.drawnTile = drawn;
    this.state.phase = 'player_turn';
    this.state.turnSeat = 0;
    // Per Tuidaohu rule: hero's own draw clears same-turn 过水碰 / 过水胡
    this.state.furitenFlags.过水碰 = [];
    this.state.furitenFlags.过水胡 = false;
    this.state.pendingCalls = this.buildPlayerTurnSelfCalls(hero);
    this.pushLog(`我 从牌墙摸入 ${getTileName(drawn)}`, {
      type: 'TileDrawn',
      seat: 0,
      tile: drawn,
    });
    audioRouter.emit({ type: 'draw', seat: 0, tile: drawn });
    this.notify();
  }

  triggerDrawExhausted() {
    this.clearAiTimer();
    this.state.phase = 'settlement';
    this.state.pendingCalls = [];
    this.state.settlement = {
      winType: '荒庄',
      winnerSeat: null,
      loserSeat: null,
      winTile: null,
      items: [{ label: '牌墙摸完 · 荒庄不查叫（杠分仍有效）', fan: 0 }],
      rawFan: 0,
      totalFan: 0,
      capped: false,
      multiplier: 0,
      kongDeltas: [0, 0, 0, 0],
      scoreDeltas: [0, 0, 0, 0],
    };
    this.pushLog('牌墙已空，本小局荒庄（不查叫）', { type: 'RoundExhausted' });
    this.notify();
  }

  chooseCall(actionType, tileOverride = null) {
    if (!this.state) return;
    const call =
      this.state.pendingCalls.find(
        (c) => c.type === actionType && (!tileOverride || c.tile === tileOverride)
      ) || this.state.pendingCalls.find((c) => c.type === actionType);
    if (!call) return;

    const hero = this.state.players[0];

    if (actionType === 'pass') {
      audioRouter.emit({ type: 'pass', seat: 0, tile: call.tile });
      const wasRobKong = this.state.phase === 'rob_kong_window';
      const hadHu = this.state.pendingCalls.some((c) => c.type === 'hu');
      if (hadHu) {
        this.state.furitenFlags.过水胡 = true;
      }
      if (this.state.pendingCalls.some((c) => c.type === 'pon') && call.tile) {
        if (!this.state.furitenFlags.过水碰.includes(call.tile)) {
          this.state.furitenFlags.过水碰.push(call.tile);
        }
      }

      if (wasRobKong) {
        // Opponent completes 补杠
        const actor = this.state.players[call.fromSeat];
        const ponMeld = actor.melds.find((m) => m.kind === 'pon' && m.tiles[0] === call.tile);
        if (ponMeld) {
          ponMeld.kind = 'bukong';
          ponMeld.tiles.push(call.tile);
        }
        this.state.phase = 'ai_thinking';
        this.state.turnSeat = (call.fromSeat + 1) % 4;
        this.state.pendingCalls = [];
        this.pushLog(`我 选择 过（触发过水胡），${actor.name} 完成补杠`, {
          type: 'ActionPassed',
          seat: 0,
        });
        this.notify();
        this.scheduleNextAiTurn();
        return;
      }

      this.state.pendingCalls = [];
      this.state.phase = 'ai_thinking';
      this.state.turnSeat = (this.state.turnSeat + 1) % 4;
      this.pushLog(`我 选择 过（锁定同巡过水${hadHu ? '胡' : '碰'}: ${getTileName(call.tile)}）`, {
        type: 'ActionPassed',
        seat: 0,
        tile: call.tile,
      });
      this.notify();
      this.scheduleNextAiTurn();
      return;
    }

    if (actionType === 'hu') {
      const winType = call.winType || (this.state.phase === 'rob_kong_window' ? '抢杠胡' : '自摸');
      this.loadSettlementScenario(winType);
      return;
    }

    if (actionType === 'pon') {
      let removed = 0;
      hero.hand = hero.hand.filter((t) => {
        if (t === call.tile && removed < 2) {
          removed += 1;
          return false;
        }
        return true;
      });
      const fromPlayer = this.state.players[call.fromSeat];
      if (fromPlayer && fromPlayer.river.length > 0 && fromPlayer.river[fromPlayer.river.length - 1].tile === call.tile) {
        fromPlayer.river.pop();
      }
      hero.melds.push({
        kind: 'pon',
        tiles: [call.tile, call.tile, call.tile],
        fromSeat: call.fromSeat,
      });
      hero.drawnTile = null;
      this.state.furitenFlags.过水碰 = [];
      this.state.furitenFlags.过水胡 = false;
      this.state.phase = 'player_turn';
      this.state.turnSeat = 0;
      this.state.pendingCalls = [];
      this.pushLog(`我 碰 ${getTileName(call.tile)}！请打出一张舍牌`, {
        type: 'MeldDeclared',
        kind: 'pon',
        seat: 0,
        tile: call.tile,
      });
      audioRouter.emit({ type: 'meld_pon', seat: 0, tile: call.tile });
      this.emitImpact({ kind: 'pon', label: '碰!', sublabel: getTileName(call.tile), seat: 0 });
      this.notify();
      return;
    }

    if (actionType === 'mingkong') {
      let removed = 0;
      hero.hand = hero.hand.filter((t) => {
        if (t === call.tile && removed < 3) {
          removed += 1;
          return false;
        }
        return true;
      });
      const fromPlayer = this.state.players[call.fromSeat];
      if (fromPlayer && fromPlayer.river.length > 0 && fromPlayer.river[fromPlayer.river.length - 1].tile === call.tile) {
        fromPlayer.river.pop();
      }
      hero.melds.push({
        kind: 'mingkong',
        tiles: [call.tile, call.tile, call.tile, call.tile],
        fromSeat: call.fromSeat,
      });
      // 明杠杠分: +2 from each / +6 total (明2)
      this.applyKongScore(0, 2);
      const tailDraw = this.state.wallBag.pop() || '4s';
      this.state.wallRemaining = Math.max(0, this.state.wallRemaining - 1);
      hero.drawnTile = tailDraw;
      this.state.phase = 'player_turn';
      this.state.turnSeat = 0;
      this.state.pendingCalls = this.buildPlayerTurnSelfCalls(hero);
      this.pushLog(`我 明杠 ${getTileName(call.tile)}（杠分+6），牌墙尾补牌 ${getTileName(tailDraw)}`, {
        type: 'MeldDeclared',
        kind: 'mingkong',
        seat: 0,
        tile: call.tile,
      });
      audioRouter.emit({ type: 'meld_kong', seat: 0, tile: call.tile });
      this.emitImpact({ kind: 'kong', label: '明杠!', sublabel: `${getTileName(call.tile)} · +6分`, seat: 0 });
      this.notify();
      return;
    }

    if (actionType === 'ankong') {
      let removed = 0;
      if (hero.drawnTile === call.tile) {
        hero.drawnTile = null;
        removed = 1;
      }
      hero.hand = hero.hand.filter((t) => {
        if (t === call.tile && removed < 4) {
          removed += 1;
          return false;
        }
        return true;
      });
      if (hero.drawnTile) {
        hero.hand = sortTiles([...hero.hand, hero.drawnTile]);
        hero.drawnTile = null;
      }
      hero.melds.push({
        kind: 'ankong',
        tiles: [call.tile, call.tile, call.tile, call.tile],
        fromSeat: 0,
      });
      this.applyKongScore(0, 2);
      const tailDraw = this.state.wallBag.pop() || '4s';
      this.state.wallRemaining = Math.max(0, this.state.wallRemaining - 1);
      hero.drawnTile = tailDraw;
      this.state.pendingCalls = this.buildPlayerTurnSelfCalls(hero);
      this.pushLog(`我 暗杠 ${getTileName(call.tile)}（杠分+6），牌墙尾补牌 ${getTileName(tailDraw)}`, {
        type: 'MeldDeclared',
        kind: 'ankong',
        seat: 0,
        tile: call.tile,
      });
      audioRouter.emit({ type: 'meld_kong', seat: 0, tile: call.tile });
      this.emitImpact({ kind: 'kong', label: '暗杠!', sublabel: `${getTileName(call.tile)} · +6分`, seat: 0 });
      this.notify();
      return;
    }

    if (actionType === 'bukong') {
      if (hero.drawnTile === call.tile) {
        hero.drawnTile = null;
      } else {
        const idx = hero.hand.indexOf(call.tile);
        if (idx !== -1) hero.hand.splice(idx, 1);
        if (hero.drawnTile) {
          hero.hand = sortTiles([...hero.hand, hero.drawnTile]);
          hero.drawnTile = null;
        }
      }
      const ponMeld = hero.melds.find((m) => m.kind === 'pon' && m.tiles[0] === call.tile);
      if (ponMeld) {
        ponMeld.kind = 'bukong';
        ponMeld.tiles.push(call.tile);
      }
      this.applyKongScore(0, 1);
      const tailDraw = this.state.wallBag.pop() || '7s';
      this.state.wallRemaining = Math.max(0, this.state.wallRemaining - 1);
      hero.drawnTile = tailDraw;
      this.state.pendingCalls = this.buildPlayerTurnSelfCalls(hero);
      this.pushLog(`我 补杠 ${getTileName(call.tile)}（杠分+3），牌墙尾补牌 ${getTileName(tailDraw)}`, {
        type: 'MeldDeclared',
        kind: 'bukong',
        seat: 0,
        tile: call.tile,
      });
      audioRouter.emit({ type: 'meld_kong', seat: 0, tile: call.tile });
      this.emitImpact({ kind: 'kong', label: '补杠!', sublabel: `${getTileName(call.tile)} · +3分`, seat: 0 });
      this.notify();
    }
  }

  applyKongScore(winnerSeat, perPlayer) {
    this.state.players.forEach((p, idx) => {
      if (idx === winnerSeat) {
        p.score += perPlayer * 3;
        p.lastDelta = perPlayer * 3;
      } else {
        p.score -= perPlayer;
        p.lastDelta = -perPlayer;
      }
    });
  }
}

export const tableStore = new PrototypeTableStore();
