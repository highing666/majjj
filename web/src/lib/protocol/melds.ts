import type { MeldView } from "./generated/MeldView";
import type { RevealedMeld } from "./generated/RevealedMeld";
import type { Seat } from "./generated/Seat";
import type { Tile } from "./generated/Tile";

export type MeldKind =
  | "Pon"
  | "ExposedKong"
  | "AddedKong"
  | "ConcealedKong"
  | "Chi";

export type ParsedMeld =
  | {
      kind: "Pon";
      tile: Tile;
      fromSeat: Seat;
      seq: number;
    }
  | {
      kind: "ExposedKong";
      tile: Tile;
      fromSeat: Seat;
      seq: number;
    }
  | {
      kind: "AddedKong";
      tile: Tile;
      fromSeat: Seat;
      seq: number;
    }
  | {
      kind: "ConcealedKong";
      tile: Tile | null;
      seq: number;
    }
  | {
      kind: "Chi";
      tiles: [Tile, Tile, Tile];
      calledTile: Tile;
      fromSeat: Seat;
      seq: number;
    };

/**
 * Normalizes a `MeldView` or `RevealedMeld` tagged union into a discriminated `{ kind, ... }` shape.
 */
export function parseMeld(meld: MeldView | RevealedMeld): ParsedMeld {
  if ("Pon" in meld) {
    return {
      kind: "Pon",
      tile: meld.Pon.tile,
      fromSeat: meld.Pon.from_seat,
      seq: meld.Pon.seq,
    };
  }
  if ("ExposedKong" in meld) {
    return {
      kind: "ExposedKong",
      tile: meld.ExposedKong.tile,
      fromSeat: meld.ExposedKong.from_seat,
      seq: meld.ExposedKong.seq,
    };
  }
  if ("AddedKong" in meld) {
    return {
      kind: "AddedKong",
      tile: meld.AddedKong.tile,
      fromSeat: meld.AddedKong.from_seat,
      seq: meld.AddedKong.seq,
    };
  }
  if ("ConcealedKong" in meld) {
    return {
      kind: "ConcealedKong",
      tile: meld.ConcealedKong.tile,
      seq: meld.ConcealedKong.seq,
    };
  }
  return {
    kind: "Chi",
    tiles: meld.Chi.tiles,
    calledTile: meld.Chi.called_tile,
    fromSeat: meld.Chi.from_seat,
    seq: meld.Chi.seq,
  };
}
