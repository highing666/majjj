<script lang="ts">
  import atlasPngUrl from "../../assets/atlas/tiles@2x.png";
  import type { Tile } from "../protocol/generated/Tile";
  import {
    projectTileBadgeAtlas,
    type TileBadgeSize,
  } from "./tileBadgeAtlas";

  interface Props {
    tile: Tile | null;
    faceUp?: boolean;
    size?: TileBadgeSize;
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

  const projection = $derived(projectTileBadgeAtlas(tile, faceUp, size));
</script>

<span
  class="tile-badge size-{size}"
  class:tile-back={!projection.faceUp}
  class:tile-dimmed={dimmed}
  class:tile-highlighted={highlighted}
  role="img"
  style:width={`${projection.width}px`}
  style:height={`${projection.height}px`}
  style:background-image={`url("${atlasPngUrl}")`}
  style:background-size={projection.backgroundSize}
  style:background-position={projection.backgroundPosition}
  title={projection.ariaLabel}
  aria-label={projection.ariaLabel}
></span>

<style>
  .tile-badge {
    display: inline-block;
    flex: 0 0 auto;
    vertical-align: middle;
    background-repeat: no-repeat;
    background-color: transparent;
    border: 0;
    user-select: none;
    transition:
      filter 0.12s ease,
      box-shadow 0.12s ease,
      opacity 0.12s ease;
  }

  .size-xs {
    border-radius: 3px;
  }

  .size-sm {
    border-radius: 3.5px;
  }

  .size-md {
    border-radius: 4px;
  }

  .tile-dimmed {
    opacity: 0.72;
    filter: saturate(0.75);
  }

  .tile-highlighted {
    box-shadow:
      0 0 0 1.5px #f5be58,
      0 0 10px rgba(245, 190, 88, 0.72);
  }
</style>
