import type { Seat } from "../protocol/generated/Seat";
import type { Suit } from "../protocol/generated/Suit";
import type { Tile } from "../protocol/generated/Tile";
import type { TurnView } from "../protocol/generated/TurnView";
import type { Wind } from "../protocol/generated/Wind";

const CHINESE_NUMERALS = ["一", "二", "三", "四", "五", "六", "七", "八", "九"];
const WIND_TILES = ["东", "南", "西", "北"];
const DRAGON_TILES = ["中", "发", "白"];

/**
 * Returns the canonical suit category for a tile index in `0..34`.
 */
export function getTileSuit(tile: Tile): Suit {
  if (tile >= 0 && tile <= 8) {
    return "Man";
  }
  if (tile >= 9 && tile <= 17) {
    return "Bamboo";
  }
  if (tile >= 18 && tile <= 26) {
    return "Dot";
  }
  if (tile >= 27 && tile <= 30) {
    return "Wind";
  }
  return "Dragon";
}

/**
 * Returns 1-based rank `1..9` for number tiles, or `null` for honor tiles.
 */
export function getTileRank(tile: Tile): number | null {
  if (tile >= 0 && tile <= 26) {
    return (tile % 9) + 1;
  }
  return null;
}

/**
 * Returns the Chinese tile name (e.g. `一万`, `五条`, `九筒`, `东`, `中`).
 */
export function getTileLabel(tile: Tile): string {
  const suit = getTileSuit(tile);
  const rank = getTileRank(tile);
  if (rank !== null) {
    const numeral = CHINESE_NUMERALS[rank - 1];
    switch (suit) {
      case "Man":
        return `${numeral}万`;
      case "Bamboo":
        return `${numeral}条`;
      case "Dot":
        return `${numeral}筒`;
    }
  }
  if (tile >= 27 && tile <= 30) {
    return WIND_TILES[tile - 27];
  }
  if (tile >= 31 && tile <= 33) {
    return DRAGON_TILES[tile - 31];
  }
  return "?";
}

/**
 * Returns the compact display numeral or character for a tile.
 */
export function getTileNumeral(tile: Tile): string {
  const rank = getTileRank(tile);
  if (rank !== null) {
    return CHINESE_NUMERALS[rank - 1];
  }
  return getTileLabel(tile);
}

/**
 * Returns Chinese seat label (`东座`/`南座`/`西座`/`北座`).
 */
export function seatLabel(seat: Seat): string {
  switch (seat) {
    case "East":
      return "东座";
    case "South":
      return "南座";
    case "West":
      return "西座";
    case "North":
      return "北座";
  }
}

/**
 * Returns single-character wind label for a seat (`东`/`南`/`西`/`北`).
 */
export function seatShortWind(seat: Seat): string {
  switch (seat) {
    case "East":
      return "东";
    case "South":
      return "南";
    case "West":
      return "西";
    case "North":
      return "北";
  }
}

/**
 * Returns Chinese wind round label (`东风圈`..`北风圈`).
 */
export function windRoundLabel(wind: Wind): string {
  switch (wind) {
    case "East":
      return "东风圈";
    case "South":
      return "南风圈";
    case "West":
      return "西风圈";
    case "North":
      return "北风圈";
  }
}

/**
 * Formats the round/hand header including optional dealer streak (`连庄`).
 */
export function formatRoundTitle(turn: TurnView): string {
  const base = `${windRoundLabel(turn.wind_round)} 第 ${turn.hand_number} 小局`;
  if (turn.dealer_streak > 0) {
    return `${base} · 连庄 ${turn.dealer_streak}`;
  }
  return base;
}
