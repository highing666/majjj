<script lang="ts">
  import type { Tile } from "../../protocol/generated/Tile";
  import type { PrototypeVariantKey } from "./mockState";
  import artA1Pin from "./generated/art-a-1pin.png";
  import artA1Sou from "./generated/art-a-1sou.png";
  import artB1Pin from "./generated/art-b-1pin.png";
  import artB1Sou from "./generated/art-b-1sou.png";
  import artC1Pin from "./generated/art-c-1pin.png";
  import artC1Sou from "./generated/art-c-1sou.png";

  interface Props {
    tile: Tile | null;
    variant?: PrototypeVariantKey;
    body?: "standing" | "laid" | "back" | "badge";
    width?: number;
    height?: number;
    selected?: boolean;
    highlighted?: boolean;
    dimmed?: boolean;
    isTsumogiri?: boolean;
    rotation?: number;
    interactive?: boolean;
    onclick?: () => void;
  }

  let {
    tile,
    variant = "A",
    body = "standing",
    width = 48,
    height = 66,
    selected = false,
    highlighted = false,
    dimmed = false,
    isTsumogiri = false,
    rotation = 0,
    interactive = false,
    onclick,
  }: Props = $props();

  const ARTWORK: Record<
    PrototypeVariantKey,
    { sou1: string; pin1: string }
  > = {
    A: { sou1: artA1Sou, pin1: artA1Pin },
    B: { sou1: artB1Sou, pin1: artB1Pin },
    C: { sou1: artC1Sou, pin1: artC1Pin },
  };

  const STYLE_TOKENS: Record<
    PrototypeVariantKey,
    {
      ivory: string;
      ivoryBottom: string;
      jadeBack: string;
      jadeDark: string;
      ink: string;
      crimson: string;
      jade: string;
      cobalt: string;
      manFont: string;
      honorFont: string;
    }
  > = {
    A: {
      ivory: "#f6efe0",
      ivoryBottom: "#e2d4b7",
      jadeBack: "#186258",
      jadeDark: "#0e433c",
      ink: "#1c1917",
      crimson: "#b92822",
      jade: "#137449",
      cobalt: "#1b4e8d",
      manFont: '"Kaiti SC", "STKaiti", "BiauKai", "Songti SC", serif',
      honorFont: '"Kaiti SC", "STKaiti", "BiauKai", "Songti SC", serif',
    },
    B: {
      ivory: "#f7efdd",
      ivoryBottom: "#dfcda8",
      jadeBack: "#176d5d",
      jadeDark: "#0c493e",
      ink: "#141418",
      crimson: "#c92b24",
      jade: "#127e4d",
      cobalt: "#184f96",
      manFont: '"Kaiti SC", "STKaiti", "Hiragino Mincho ProN", "Songti SC", serif',
      honorFont: '"Kaiti SC", "STKaiti", "Hiragino Mincho ProN", "Songti SC", serif',
    },
    C: {
      ivory: "#f9f3e6",
      ivoryBottom: "#e5d9bf",
      jadeBack: "#15574f",
      jadeDark: "#0c3a34",
      ink: "#111317",
      crimson: "#c2241f",
      jade: "#0f7145",
      cobalt: "#16488a",
      manFont: '"Songti SC", "STSong", "Kaiti SC", serif',
      honorFont: '"Songti SC", "STSong", "Kaiti SC", serif',
    },
  };

  const CN_NUM = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
  const HONOR_GLYPHS = ["東", "南", "西", "北", "中", "發", "白"];

  const tokens = $derived(STYLE_TOKENS[variant]);
  const art = $derived(ARTWORK[variant]);
  const isBack = $derived(body === "back" || tile === null);
  const isStanding = $derived(body === "standing");
  const isOneBamboo = $derived(!isBack && tile === 9);
  const isOneDot = $derived(!isBack && tile === 18);
  const isWhiteDragon = $derived(!isBack && tile === 33);

  interface BambooPiece {
    x: number;
    y: number;
    h: number;
    color: string;
    angle?: number;
  }

  interface PlumPinPiece {
    x: number;
    y: number;
    r: number;
    color: string;
  }

  function getBambooPieces(rank: number, t: typeof tokens): BambooPiece[] {
    const g = t.jade;
    const r = t.crimson;
    switch (rank) {
      case 2:
        return [
          { x: 50, y: 38, h: 42, color: g },
          { x: 50, y: 92, h: 42, color: g },
        ];
      case 3:
        return [
          { x: 50, y: 36, h: 40, color: g },
          { x: 30, y: 92, h: 40, color: g },
          { x: 70, y: 92, h: 40, color: g },
        ];
      case 4:
        return [
          { x: 31, y: 38, h: 40, color: g },
          { x: 69, y: 38, h: 40, color: g },
          { x: 31, y: 92, h: 40, color: g },
          { x: 69, y: 92, h: 40, color: g },
        ];
      case 5:
        return [
          { x: 25, y: 35, h: 36, color: g },
          { x: 75, y: 35, h: 36, color: g },
          { x: 50, y: 65, h: 36, color: r },
          { x: 25, y: 95, h: 36, color: g },
          { x: 75, y: 95, h: 36, color: g },
        ];
      case 6:
        return [
          { x: 25, y: 38, h: 40, color: g },
          { x: 50, y: 38, h: 40, color: g },
          { x: 75, y: 38, h: 40, color: g },
          { x: 25, y: 92, h: 40, color: g },
          { x: 50, y: 92, h: 40, color: g },
          { x: 75, y: 92, h: 40, color: g },
        ];
      case 7:
        return [
          { x: 50, y: 26, h: 30, color: r },
          { x: 25, y: 64, h: 30, color: g },
          { x: 50, y: 64, h: 30, color: g },
          { x: 75, y: 64, h: 30, color: g },
          { x: 25, y: 102, h: 30, color: g },
          { x: 50, y: 102, h: 30, color: g },
          { x: 75, y: 102, h: 30, color: g },
        ];
      case 8:
        // Classic symmetric green M/W bamboo knot
        return [
          { x: 20, y: 38, h: 36, color: g },
          { x: 39, y: 40, h: 34, color: g, angle: 28 },
          { x: 61, y: 40, h: 34, color: g, angle: -28 },
          { x: 80, y: 38, h: 36, color: g },
          { x: 20, y: 92, h: 36, color: g },
          { x: 39, y: 90, h: 34, color: g, angle: -28 },
          { x: 61, y: 90, h: 34, color: g, angle: 28 },
          { x: 80, y: 92, h: 36, color: g },
        ];
      case 9:
        return [
          { x: 24, y: 27, h: 29, color: g },
          { x: 50, y: 27, h: 29, color: r },
          { x: 76, y: 27, h: 29, color: g },
          { x: 24, y: 65, h: 29, color: g },
          { x: 50, y: 65, h: 29, color: r },
          { x: 76, y: 65, h: 29, color: g },
          { x: 24, y: 103, h: 29, color: g },
          { x: 50, y: 103, h: 29, color: r },
          { x: 76, y: 103, h: 29, color: g },
        ];
      default:
        return [];
    }
  }

  function getPlumPins(rank: number, t: typeof tokens): PlumPinPiece[] {
    const g = t.jade;
    const r = t.crimson;
    const b = t.cobalt;
    switch (rank) {
      case 2:
        return [
          { x: 50, y: 38, r: 17, color: g },
          { x: 50, y: 92, r: 17, color: b },
        ];
      case 3:
        return [
          { x: 26, y: 31, r: 14, color: b },
          { x: 50, y: 65, r: 14, color: r },
          { x: 74, y: 99, r: 14, color: g },
        ];
      case 4:
        return [
          { x: 30, y: 39, r: 15, color: b },
          { x: 70, y: 39, r: 15, color: g },
          { x: 30, y: 91, r: 15, color: g },
          { x: 70, y: 91, r: 15, color: b },
        ];
      case 5:
        return [
          { x: 26, y: 33, r: 13, color: b },
          { x: 74, y: 33, r: 13, color: g },
          { x: 50, y: 65, r: 13.5, color: r },
          { x: 26, y: 97, r: 13, color: g },
          { x: 74, y: 97, r: 13, color: b },
        ];
      case 6:
        return [
          { x: 31, y: 29, r: 13, color: g },
          { x: 69, y: 29, r: 13, color: g },
          { x: 31, y: 65, r: 13, color: r },
          { x: 69, y: 65, r: 13, color: r },
          { x: 31, y: 101, r: 13, color: r },
          { x: 69, y: 101, r: 13, color: r },
        ];
      case 7:
        return [
          { x: 24, y: 23, r: 11.2, color: g },
          { x: 50, y: 34, r: 11.2, color: g },
          { x: 76, y: 45, r: 11.2, color: g },
          { x: 30, y: 74, r: 12.2, color: r },
          { x: 70, y: 74, r: 12.2, color: r },
          { x: 30, y: 103, r: 12.2, color: r },
          { x: 70, y: 103, r: 12.2, color: r },
        ];
      case 8:
        return [23, 51, 79, 107].flatMap((y) => [
          { x: 30, y, r: 11.6, color: b },
          { x: 70, y, r: 11.6, color: b },
        ]);
      case 9:
        return [
          { y: 27, c: g },
          { y: 65, c: r },
          { y: 103, c: b },
        ].flatMap(({ y, c }) => [
          { x: 23, y, r: 11.4, color: c },
          { x: 50, y, r: 11.4, color: c },
          { x: 77, y, r: 11.4, color: c },
        ]);
      default:
        return [];
    }
  }

  function petalAngles(): number[] {
    return [0, 72, 144, 216, 288];
  }

  const labelText = $derived.by(() => {
    if (isBack || tile === null) return "扣牌";
    if (tile < 9) return `${CN_NUM[tile + 1]}万`;
    if (tile < 18) return `${CN_NUM[tile - 8]}条`;
    if (tile < 27) return `${CN_NUM[tile - 17]}筒`;
    return HONOR_GLYPHS[tile - 27] ?? "?";
  });
</script>

{#snippet tileFace()}
  <div
    class="tile-shell body-{body} style-{variant}"
    class:is-selected={selected}
    class:is-highlighted={highlighted}
    class:is-dimmed={dimmed}
    class:is-tsumogiri={isTsumogiri}
    style:--tile-w="{width}px"
    style:--tile-h="{height}px"
    style:--tile-rot="{rotation}deg"
    style:--ivory={tokens.ivory}
    style:--ivory-bottom={tokens.ivoryBottom}
    style:--jade-back={tokens.jadeBack}
    style:--jade-dark={tokens.jadeDark}
    style:--tile-ink={tokens.ink}
  >
    {#if isStanding}
      <span class="standing-jade-lip" aria-hidden="true"></span>
    {/if}

    <div class="tile-face-card" class:tile-back-card={isBack}>
      {#if isBack}
        <svg viewBox="0 0 100 130" class="tile-svg" aria-hidden="true">
          <rect
            x="10"
            y="10"
            width="80"
            height="110"
            rx="7"
            fill="none"
            stroke="rgba(167, 243, 208, 0.36)"
            stroke-width="2.4"
          />
          <path
            d="M50 38 L66 65 L50 92 L34 65 Z"
            fill="none"
            stroke="rgba(167, 243, 208, 0.45)"
            stroke-width="2.2"
          />
          <circle cx="50" cy="65" r="5" fill="rgba(212, 168, 87, 0.55)" />
        </svg>
      {:else if isOneBamboo}
        <img
          src={art.sou1}
          alt="一条"
          class="tile-art-img art-1sou"
          draggable="false"
        />
      {:else if isOneDot}
        <img
          src={art.pin1}
          alt="一筒"
          class="tile-art-img art-1pin"
          draggable="false"
        />
      {:else if isWhiteDragon}
        <!-- Mainland traditional pure blank white dragon with faint inner recessed bevel -->
        <svg viewBox="0 0 100 130" class="tile-svg" aria-hidden="true">
          <rect
            x="13"
            y="14"
            width="74"
            height="102"
            rx="7"
            fill="none"
            stroke="rgba(128, 114, 88, 0.22)"
            stroke-width="1.6"
          />
        </svg>
      {:else if tile !== null && tile < 9}
        {@const rank = tile + 1}
        <svg viewBox="0 0 100 130" class="tile-svg" aria-hidden="true">
          <text
            x="50"
            y="54"
            text-anchor="middle"
            font-family={tokens.manFont}
            font-weight="900"
            font-size={variant === "C" ? "44" : "42"}
            fill={tokens.ink}
          >
            {CN_NUM[rank]}
          </text>
          <text
            x="50"
            y="108"
            text-anchor="middle"
            font-family={tokens.manFont}
            font-weight="900"
            font-size={variant === "C" ? "47" : "45"}
            fill={tokens.crimson}
          >
            萬
          </text>
        </svg>
      {:else if tile !== null && tile >= 9 && tile < 18}
        {@const rank = tile - 8}
        {@const pieces = getBambooPieces(rank, tokens)}
        <svg viewBox="0 0 100 130" class="tile-svg" aria-hidden="true">
          {#each pieces as piece, idx (idx)}
            {@const bw = rank >= 7 ? 10.2 : 12.2}
            {@const top = piece.y - piece.h / 2}
            {@const bot = piece.y + piece.h / 2}
            <g transform="rotate({piece.angle ?? 0} {piece.x} {piece.y})">
              <rect
                x={piece.x - bw * 0.42}
                y={top + 2}
                width={bw * 0.84}
                height={piece.h - 4}
                rx="3.2"
                fill={piece.color}
                stroke={variant === "C" ? tokens.ink : "rgba(18, 22, 24, 0.72)"}
                stroke-width={variant === "C" ? "1.6" : "1.1"}
              />
              <!-- Three classic bamboo nodes -->
              <rect
                x={piece.x - bw * 0.58}
                y={top - 0.5}
                width={bw * 1.16}
                height="4.6"
                rx="2.3"
                fill={piece.color}
                stroke={variant === "C" ? tokens.ink : "rgba(18, 22, 24, 0.72)"}
                stroke-width="1.1"
              />
              <rect
                x={piece.x - bw * 0.58}
                y={piece.y - 2.3}
                width={bw * 1.16}
                height="4.6"
                rx="2.3"
                fill={piece.color}
                stroke={variant === "C" ? tokens.ink : "rgba(18, 22, 24, 0.72)"}
                stroke-width="1.1"
              />
              <rect
                x={piece.x - bw * 0.58}
                y={bot - 4.1}
                width={bw * 1.16}
                height="4.6"
                rx="2.3"
                fill={piece.color}
                stroke={variant === "C" ? tokens.ink : "rgba(18, 22, 24, 0.72)"}
                stroke-width="1.1"
              />
              <line
                x1={piece.x - bw * 0.1}
                y1={top + 5.5}
                x2={piece.x - bw * 0.1}
                y2={bot - 5.5}
                stroke="rgba(255, 252, 240, 0.7)"
                stroke-width="1.4"
                stroke-linecap="round"
              />
            </g>
          {/each}
        </svg>
      {:else if tile !== null && tile >= 18 && tile < 27}
        {@const rank = tile - 17}
        {@const pins = getPlumPins(rank, tokens)}
        <svg viewBox="0 0 100 130" class="tile-svg" aria-hidden="true">
          {#each pins as pin, idx (idx)}
            <g>
              <circle
                cx={pin.x}
                cy={pin.y}
                r={pin.r}
                fill={pin.color}
                stroke={tokens.ink}
                stroke-width={variant === "C" ? "1.9" : "1.4"}
              />
              <circle
                cx={pin.x}
                cy={pin.y}
                r={pin.r * 0.74}
                fill="none"
                stroke={tokens.ivory}
                stroke-width={Math.max(1.1, pin.r * 0.1)}
                opacity="0.92"
              />
              <!-- Traditional 5-petal plum blossom inside disc -->
              {#each petalAngles() as deg (deg)}
                <g transform="rotate({deg} {pin.x} {pin.y})">
                  <ellipse
                    cx={pin.x}
                    cy={pin.y - pin.r * 0.37}
                    rx={pin.r * 0.17}
                    ry={pin.r * 0.25}
                    fill={tokens.ivory}
                  />
                </g>
              {/each}
              <circle cx={pin.x} cy={pin.y} r={pin.r * 0.19} fill={pin.color} />
              <circle cx={pin.x} cy={pin.y} r={pin.r * 0.09} fill={tokens.ivory} />
            </g>
          {/each}
        </svg>
      {:else if tile !== null && tile >= 27}
        {@const glyph = HONOR_GLYPHS[tile - 27]}
        {@const color =
          tile === 31
            ? tokens.crimson
            : tile === 32
              ? tokens.jade
              : tokens.ink}
        <svg viewBox="0 0 100 130" class="tile-svg" aria-hidden="true">
          <text
            x="50"
            y="89"
            text-anchor="middle"
            font-family={tokens.honorFont}
            font-weight="900"
            font-size={variant === "C" ? "66" : "63"}
            fill={color}
          >
            {glyph}
          </text>
        </svg>
      {/if}

      {#if isTsumogiri}
        <span class="tsumogiri-dot" title="摸切" aria-hidden="true"></span>
      {/if}
    </div>
  </div>
{/snippet}

{#if interactive}
  <button
    class="tile-btn-wrap"
    type="button"
    aria-label={labelText}
    title={labelText}
    {onclick}
  >
    {@render tileFace()}
  </button>
{:else}
  <span class="tile-static-wrap" aria-label={labelText} title={labelText}>
    {@render tileFace()}
  </span>
{/if}

<style>
  .tile-btn-wrap,
  .tile-static-wrap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    background: transparent;
    flex: 0 0 auto;
    user-select: none;
    -webkit-user-select: none;
  }

  .tile-btn-wrap {
    cursor: pointer;
    transition: transform 0.12s ease;
  }

  .tile-btn-wrap:hover .tile-shell {
    transform: translateY(-5px);
    box-shadow: 0 7px 14px rgba(5, 10, 14, 0.5);
  }

  .tile-shell {
    position: relative;
    width: var(--tile-w);
    height: var(--tile-h);
    transform: rotate(var(--tile-rot));
    transition:
      transform 0.14s ease,
      filter 0.2s ease,
      box-shadow 0.2s ease;
  }

  .standing-jade-lip {
    position: absolute;
    inset: -3px 0 0 0;
    border-radius: calc(var(--tile-w) * 0.15);
    background: linear-gradient(180deg, var(--jade-back) 0%, var(--jade-dark) 100%);
    border: 1.5px solid #091d1b;
    box-shadow: 0 4px 8px rgba(4, 10, 14, 0.45);
    z-index: 0;
  }

  .tile-face-card {
    position: relative;
    z-index: 1;
    width: 100%;
    height: 100%;
    border-radius: calc(var(--tile-w) * 0.14);
    background: linear-gradient(180deg, #fffdf8 0%, var(--ivory) 82%, var(--ivory-bottom) 100%);
    border: 1.5px solid var(--tile-ink);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.9),
      inset 0 -2px 0 rgba(164, 144, 106, 0.35);
    overflow: hidden;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .body-standing .tile-face-card {
    transform: translateY(2px);
    height: calc(100% - 2px);
  }

  .body-laid .tile-face-card {
    box-shadow:
      0 2px 4px rgba(6, 12, 16, 0.42),
      inset 0 1px 0 rgba(255, 255, 255, 0.85),
      inset 0 -2px 0 rgba(160, 140, 104, 0.38);
  }

  .body-badge .tile-face-card {
    border-width: 1.25px;
    box-shadow:
      0 1px 2px rgba(6, 12, 16, 0.32),
      inset 0 -1.5px 0 rgba(160, 140, 104, 0.38);
  }

  .tile-back-card {
    background: linear-gradient(155deg, #21786b 0%, var(--jade-back) 58%, var(--jade-dark) 100%);
    border-color: #0b2220;
    box-shadow:
      0 2px 4px rgba(6, 12, 16, 0.42),
      inset 0 1px 0 rgba(167, 243, 208, 0.25);
  }

  .tile-svg {
    width: 100%;
    height: 100%;
    display: block;
  }

  .tile-art-img {
    width: 92%;
    height: 92%;
    object-fit: contain;
    mix-blend-mode: multiply;
    pointer-events: none;
  }

  .art-1pin {
    width: 94%;
    height: 94%;
  }

  .is-highlighted .tile-face-card {
    border-color: #f5be58;
    box-shadow:
      0 0 0 2px #f5be58,
      0 0 16px rgba(245, 190, 88, 0.72),
      inset 0 1px 0 rgba(255, 255, 255, 0.9);
  }

  .is-selected {
    transform: translateY(-7px) rotate(var(--tile-rot));
  }

  .is-selected .tile-face-card {
    border-color: #f59e0b;
    box-shadow:
      0 0 0 2px rgba(245, 190, 88, 0.9),
      0 8px 18px rgba(245, 190, 88, 0.45);
  }

  .is-dimmed .tile-face-card {
    filter: brightness(0.76) saturate(0.72);
  }

  .is-tsumogiri .tile-face-card {
    filter: brightness(0.86) saturate(0.82);
  }

  .tsumogiri-dot {
    position: absolute;
    right: 3px;
    top: 3px;
    width: max(3.5px, calc(var(--tile-w) * 0.14));
    height: max(3.5px, calc(var(--tile-w) * 0.14));
    border-radius: 999px;
    background: rgba(24, 26, 32, 0.48);
  }
</style>
