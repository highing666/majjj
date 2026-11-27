import { type FederatedPointerEvent, Texture } from "pixi.js";
import type { LobbyConfig } from "../protocol/generated/LobbyConfig";
import type { PlayerView } from "../protocol/generated/PlayerView";
import type { PublicSeatView } from "../protocol/generated/PublicSeatView";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import type { Tile } from "../protocol/generated/Tile";
import { ATLAS_GRID } from "./atlasConstants";
import {
  MELD_GROUP_GAP,
  MELD_TILE_GAP,
  type Rect,
  type TableLayout,
  type TableLayoutOptions,
} from "./layout";
import type { TileAtlas, TileBodyKind } from "./tileAtlas";

export function makeLobbyConfig(
  overrides: Partial<LobbyConfig> = {},
): LobbyConfig {
  return {
    mode: "EastOnly",
    seating: "KeepLobbySeats",
    ai_difficulty: "Intermediate",
    pace: "Standard",
    ...overrides,
  };
}

export function makeSeatInfo(
  seat: Seat,
  nickname: string,
  overrides: Partial<SeatInfo> = {},
): SeatInfo {
  return {
    seat,
    kind: seat === "East" ? "Human" : "Ai",
    nickname,
    connected: true,
    ai_assisted: false,
    ai_difficulty: null,
    ai_style: null,
    persona_id: null,
    mood: null,
    voice_pack_id: null,
    ...overrides,
  };
}

export function makePublicSeatView(
  seat: Seat,
  overrides: Partial<PublicSeatView> = {},
): PublicSeatView {
  return {
    seat,
    score: 0,
    hand_count: 13,
    river: [],
    melds: [],
    last_draw_or_meld_seq: 0,
    ...overrides,
  };
}

export function makePlayerView(
  overrides: Partial<PlayerView> = {},
): PlayerView {
  return {
    viewer: "East",
    mode: "EastOnly",
    rule_profile: {
      name: "推倒胡",
      allow_chi: false,
      allow_four_of_kind_seven_pairs: true,
      allow_multi_ron: true,
      fan_cap: 8,
      wall_size: 136,
      dead_wall_size: 14,
      starting_scores_east_only: [0, 0, 0, 0],
      starting_scores_free_table: [0, 0, 0, 0],
    },
    phase: "Playing",
    hand: {
      closed_tiles: [0, 1, 2, 9, 10, 11, 18, 19, 20, 27, 27, 31, 31],
      drawn_tile: 5,
    },
    seats: [
      makePublicSeatView("East", { hand_count: 14 }),
      makePublicSeatView("South"),
      makePublicSeatView("West"),
      makePublicSeatView("North"),
    ],
    wall_remaining: 69,
    turn: {
      wind_round: "East",
      hand_number: 1,
      dealer_streak: 0,
      dealer: "East",
      active_seat: "East",
    },
    legal_actions: [],
    last_settlement: null,
    ready_seats: null,
    match_summary: null,
    seq: 2,
    ...overrides,
  };
}

export function makeTableSnapshot(
  overrides: Partial<TableSnapshot> = {},
): TableSnapshot {
  return {
    table_id: "tbl-test",
    phase: "InMatch",
    config: makeLobbyConfig(),
    pace: "Standard",
    slow_think_nudge_enabled: true,
    seats: [
      makeSeatInfo("East", "阿东"),
      makeSeatInfo("South", "小南"),
      makeSeatInfo("West", "阿西"),
      makeSeatInfo("North", "北北"),
    ],
    your_seat: "East",
    seat_token: "tok-east",
    pending_destructive_op: null,
    seq: 2,
    server_version: "0.1.0",
    lan_urls: {
      preferred_ipv4_url: null,
      ipv4_urls: [],
      local_hostname_url: null,
    },
    host_is_loopback: true,
    client_is_remote: false,
    player_view: makePlayerView(),
    ...overrides,
  };
}

export function makePointerEvent(x = 0, y = 0): FederatedPointerEvent {
  return {
    global: { x, y },
    stopPropagation: () => {},
  } as unknown as FederatedPointerEvent;
}

export function installCanvasDocumentStub(): () => void {
  const prevDocument = globalThis.document;
  const prevCtx2D = (globalThis as { CanvasRenderingContext2D?: unknown })
    .CanvasRenderingContext2D;
  const noop = () => {};
  const fakeCtx = {
    save: noop,
    restore: noop,
    scale: noop,
    translate: noop,
    beginPath: noop,
    closePath: noop,
    rect: noop,
    fill: noop,
    stroke: noop,
    clearRect: noop,
    fillRect: noop,
    fillText: noop,
    strokeText: noop,
    setTransform: noop,
    getImageData: () => ({ data: new Uint8ClampedArray(4) }),
    measureText: (text: string) => ({
      width: Math.max(8, text.length * 8),
      actualBoundingBoxAscent: 8,
      actualBoundingBoxDescent: 2,
      fontBoundingBoxAscent: 9,
      fontBoundingBoxDescent: 3,
      actualBoundingBoxLeft: 0,
      actualBoundingBoxRight: Math.max(8, text.length * 8),
    }),
    fillStyle: "",
    strokeStyle: "",
    lineWidth: 1,
    font: "",
    textAlign: "left",
    textBaseline: "alphabetic",
  };
  (globalThis as { CanvasRenderingContext2D?: unknown }).CanvasRenderingContext2D =
    function CanvasRenderingContext2D() {};
  (globalThis as unknown as { document: unknown }).document = {
    createElement: () => ({
      width: 256,
      height: 256,
      style: {},
      getContext: () => fakeCtx,
    }),
  };
  return () => {
    if (prevDocument === undefined) {
      delete (globalThis as { document?: Document }).document;
    } else {
      globalThis.document = prevDocument;
    }
    if (prevCtx2D === undefined) {
      delete (globalThis as { CanvasRenderingContext2D?: unknown })
        .CanvasRenderingContext2D;
    } else {
      (globalThis as { CanvasRenderingContext2D?: unknown }).CanvasRenderingContext2D =
        prevCtx2D;
    }
  };
}

export function createSpyTileAtlas() {
  const standingTextures = new Map<Tile, Texture>();
  const laidTextures = new Map<Tile, Texture>();
  const backTexture = new Texture();
  const laidBackTexture = new Texture();
  const shadowTexture = new Texture();
  const feltImpastoTexture = new Texture();
  const feltBrushTexture = new Texture();
  const calls: Array<{ tile: Tile; body: TileBodyKind | undefined }> = [];

  for (let t = 0; t < 34; t += 1) {
    standingTextures.set(t, new Texture());
    laidTextures.set(t, new Texture());
  }

  const atlas: TileAtlas = {
    width: ATLAS_GRID.cols * ATLAS_GRID.cellW,
    height: ATLAS_GRID.rows * ATLAS_GRID.cellH,
    backTexture,
    laidBackTexture,
    shadowTexture,
    feltImpastoTexture,
    feltBrushTexture,
    getTileTexture(tile: Tile, body?: TileBodyKind): Texture {
      calls.push({ tile, body });
      if (body === "laid") {
        return laidTextures.get(tile) ?? laidBackTexture;
      }
      return standingTextures.get(tile) ?? backTexture;
    },
    destroy() {
      for (const tex of standingTextures.values()) tex.destroy(false);
      for (const tex of laidTextures.values()) tex.destroy(false);
      backTexture.destroy(false);
      laidBackTexture.destroy(false);
      shadowTexture.destroy(false);
      feltImpastoTexture.destroy(false);
      feltBrushTexture.destroy(false);
    },
  };

  return {
    atlas,
    calls,
    standingTextures,
    laidTextures,
    backTexture,
    laidBackTexture,
  };
}

export interface Adr0013ViewportSpec {
  readonly name: string;
  readonly width: number;
  readonly height: number;
  readonly wide: boolean;
  readonly minHandW: number;
  readonly minRiverW: number;
  readonly minMeldW: number;
}

export const ADR_0013_VIEWPORT_MATRIX: readonly Adr0013ViewportSpec[] = [
  // 4 core target devices from ADR-0013 / Spec #96 (full & browser/Safari-bar states)
  { name: '14" MBP full 1512x982', width: 1512, height: 982, wide: true, minHandW: 74, minRiverW: 46, minMeldW: 38 },
  { name: '14" MBP browser 1512x860', width: 1512, height: 860, wide: true, minHandW: 74, minRiverW: 46, minMeldW: 38 },
  { name: '13" MBA M2 full 1470x956', width: 1470, height: 956, wide: true, minHandW: 71, minRiverW: 46, minMeldW: 37 },
  { name: '13" MBA M2 browser 1470x835', width: 1470, height: 835, wide: true, minHandW: 71, minRiverW: 46, minMeldW: 37 },
  { name: "iPhone 13 Pro full 844x390", width: 844, height: 390, wide: true, minHandW: 41, minRiverW: 28, minMeldW: 20 },
  { name: "iPhone 13 Pro Safari 844x325", width: 844, height: 325, wide: true, minHandW: 41, minRiverW: 18, minMeldW: 20 },
  { name: "iPhone 17 full 874x402", width: 874, height: 402, wide: true, minHandW: 43, minRiverW: 28, minMeldW: 21 },
  { name: "iPhone 17 Safari 874x335", width: 874, height: 335, wide: true, minHandW: 43, minRiverW: 19, minMeldW: 21 },
  // Standard / ultra-wide / tablet / compact landscape + narrow portrait fallbacks
  { name: "desktop FHD 1920x1080", width: 1920, height: 1080, wide: true, minHandW: 88, minRiverW: 46, minMeldW: 38 },
  { name: "Mac wide 1440x900", width: 1440, height: 900, wide: true, minHandW: 69, minRiverW: 46, minMeldW: 36 },
  { name: "Mac laptop 1280x720", width: 1280, height: 720, wide: true, minHandW: 60, minRiverW: 46, minMeldW: 31 },
  { name: "iPad landscape 1024x768", width: 1024, height: 768, wide: true, minHandW: 43, minRiverW: 40, minMeldW: 22 },
  { name: "desktop 1024x720", width: 1024, height: 720, wide: true, minHandW: 43, minRiverW: 40, minMeldW: 22 },
  { name: "compact landscape 760x340", width: 760, height: 340, wide: true, minHandW: 36, minRiverW: 20, minMeldW: 18 },
  { name: "SE landscape 568x320", width: 568, height: 320, wide: true, minHandW: 22, minRiverW: 18, minMeldW: 12 },
  { name: "iPhone portrait fallback 390x844", width: 390, height: 844, wide: false, minHandW: 23, minRiverW: 15, minMeldW: 14 },
  { name: "SE portrait fallback 320x568", width: 320, height: 568, wide: false, minHandW: 19, minRiverW: 14, minMeldW: 12 },
];

export interface ViewerMeldScenario {
  readonly label: string;
  readonly options: Required<TableLayoutOptions>;
}

export const VIEWER_MELD_SCENARIOS: readonly ViewerMeldScenario[] = [
  { label: "0 melds (13+1)", options: { bottomMeldTileCount: 0, bottomMeldGroupCount: 0, viewerClosedCount: 13, viewerHasDrawn: true } },
  { label: "1 pon (10+1)", options: { bottomMeldTileCount: 3, bottomMeldGroupCount: 1, viewerClosedCount: 10, viewerHasDrawn: true } },
  { label: "1 pon post-pon (10+0)", options: { bottomMeldTileCount: 3, bottomMeldGroupCount: 1, viewerClosedCount: 10, viewerHasDrawn: false } },
  { label: "1 kong (10+1)", options: { bottomMeldTileCount: 4, bottomMeldGroupCount: 1, viewerClosedCount: 10, viewerHasDrawn: true } },
  { label: "2 pons (7+1)", options: { bottomMeldTileCount: 6, bottomMeldGroupCount: 2, viewerClosedCount: 7, viewerHasDrawn: true } },
  { label: "2 kongs (7+1)", options: { bottomMeldTileCount: 8, bottomMeldGroupCount: 2, viewerClosedCount: 7, viewerHasDrawn: true } },
  { label: "3 pons (4+1)", options: { bottomMeldTileCount: 9, bottomMeldGroupCount: 3, viewerClosedCount: 4, viewerHasDrawn: true } },
  { label: "3 kongs (4+1)", options: { bottomMeldTileCount: 12, bottomMeldGroupCount: 3, viewerClosedCount: 4, viewerHasDrawn: true } },
  { label: "4 pons (1+1)", options: { bottomMeldTileCount: 12, bottomMeldGroupCount: 4, viewerClosedCount: 1, viewerHasDrawn: true } },
  { label: "4 kongs (1+1)", options: { bottomMeldTileCount: 16, bottomMeldGroupCount: 4, viewerClosedCount: 1, viewerHasDrawn: true } },
];

export function rectsOverlap(a: Rect, b: Rect): boolean {
  return (
    a.x < b.x + b.width &&
    a.x + a.width > b.x &&
    a.y < b.y + b.height &&
    a.y + a.height > b.y
  );
}

export function computeTestMeldSpan(
  tileCount: number,
  groupCount: number,
  tileW: number,
): number {
  if (tileCount <= 0) {
    return 0;
  }
  const groups = Math.max(1, groupCount);
  return (
    tileCount * tileW +
    Math.max(0, tileCount - groups) * MELD_TILE_GAP +
    Math.max(0, groups - 1) * MELD_GROUP_GAP
  );
}

export function computeBottomMeldRect(
  layout: TableLayout,
  tileCount: number,
  groupCount: number,
): Rect | null {
  if (tileCount <= 0) {
    return null;
  }
  const width = computeTestMeldSpan(tileCount, groupCount, layout.meldTileW);
  return {
    x: layout.melds.bottom.x - width,
    y: layout.melds.bottom.y,
    width,
    height: layout.meldTileH,
  };
}
