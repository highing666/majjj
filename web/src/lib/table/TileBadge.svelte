<script lang="ts">
  import type { Tile } from "../protocol/generated/Tile";
  import { getTileLabel, getTileNumeral, getTileSuit } from "./tiles";

  interface Props {
    tile: Tile | null;
    faceUp?: boolean;
    size?: "xs" | "sm" | "md";
    dimmed?: boolean;
    highlighted?: boolean;
  }

  let {
    tile,
    faceUp = true,
    size = "sm",
    dimmed = false,
    highlighted = false,
  }: Props = $props();

  const suit = $derived(tile !== null ? getTileSuit(tile) : null);
  const numeral = $derived(tile !== null ? getTileNumeral(tile) : "");
  const label = $derived(tile !== null && faceUp ? getTileLabel(tile) : "扣牌");

  const suitUnit = $derived.by(() => {
    switch (suit) {
      case "Man":
        return "万";
      case "Bamboo":
        return "条";
      case "Dot":
        return "筒";
      default:
        return "";
    }
  });

  const toneClass = $derived.by(() => {
    if (!faceUp || tile === null) {
      return "tone-back";
    }
    if (suit === "Man") {
      return "tone-man";
    }
    if (suit === "Bamboo") {
      return "tone-bamboo";
    }
    if (suit === "Dot") {
      return "tone-dot";
    }
    if (tile === 31) {
      return "tone-red";
    }
    if (tile === 32) {
      return "tone-bamboo";
    }
    if (tile === 33) {
      return "tone-dot";
    }
    return "tone-ink";
  });
</script>

<span
  class="tile-badge size-{size} {toneClass}"
  class:tile-dimmed={dimmed}
  class:tile-highlighted={highlighted}
  title={label}
  aria-label={label}
>
  {#if !faceUp || tile === null}
    <span class="tile-back-inner"></span>
  {:else if tile === 33}
    <span class="tile-white-frame"></span>
  {:else if suitUnit}
    <span class="tile-top">{numeral}</span>
    <span class="tile-bottom">{suitUnit}</span>
  {:else}
    <span class="tile-honor">{numeral}</span>
  {/if}
</span>

<style>
  .tile-badge {
    display: inline-flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    border-radius: 5px;
    border: 1.5px solid #181a20;
    background: #f5eedf;
    box-shadow: inset 0 -2px 0 #ddd0b6;
    font-weight: 800;
    line-height: 1;
    flex: 0 0 auto;
    user-select: none;
  }

  .size-xs {
    width: 22px;
    height: 30px;
    font-size: 10px;
  }

  .size-sm {
    width: 26px;
    height: 36px;
    font-size: 11px;
  }

  .size-md {
    width: 32px;
    height: 44px;
    font-size: 13px;
  }

  .tone-back {
    background: #1e5c5e;
    box-shadow: none;
    padding: 3px;
  }

  .tile-back-inner {
    width: 100%;
    height: 100%;
    border: 1px solid #2e787a;
    border-radius: 2px;
  }

  .tile-white-frame {
    width: 62%;
    height: 68%;
    border: 2px solid #1d4f8c;
    border-radius: 3px;
  }

  .tone-man .tile-top,
  .tone-red {
    color: #b12824;
  }

  .tone-man .tile-bottom {
    color: #181a20;
  }

  .tone-bamboo {
    color: #1f6e4a;
  }

  .tone-dot {
    color: #1d4f8c;
  }

  .tone-ink {
    color: #181a20;
  }

  .tile-top {
    font-size: 0.95em;
  }

  .tile-bottom {
    font-size: 0.85em;
    margin-top: 1px;
  }

  .tile-honor {
    font-size: 1.15em;
  }

  .tile-dimmed {
    opacity: 0.72;
  }

  .tile-highlighted {
    border-color: #d4891c;
    box-shadow:
      0 0 0 2px rgba(245, 190, 88, 0.45),
      inset 0 -2px 0 #ddd0b6;
  }
</style>
