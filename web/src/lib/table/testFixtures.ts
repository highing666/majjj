import { type FederatedPointerEvent, Texture } from "pixi.js";
import type { LobbyConfig } from "../protocol/generated/LobbyConfig";
import type { PlayerView } from "../protocol/generated/PlayerView";
import type { PublicSeatView } from "../protocol/generated/PublicSeatView";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import type { Tile } from "../protocol/generated/Tile";
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
    width: 1440,
    height: 1536,
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
