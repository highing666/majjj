import type { Suit } from "../protocol/generated/Suit";
import type { Tile } from "../protocol/generated/Tile";
import type { TurnView } from "../protocol/generated/TurnView";
import type { Wind } from "../protocol/generated/Wind";

export type DragonKind = "Red" | "Green" | "White";
export type TileTone = "man" | "bamboo" | "dot" | "red" | "ink";

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
 * Returns the Chinese suit suffix (`万`/`条`/`筒`) for a suit, or `""` for honors.
 */
export function getSuitUnit(suit: Suit): "万" | "条" | "筒" | "" {
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
}

/**
 * Returns `"Red"` (`中`), `"Green"` (`发`), `"White"` (`白`) for dragon tiles, or `null`.
 */
export function getDragonKind(tile: Tile): DragonKind | null {
  switch (tile) {
    case 31:
      return "Red";
    case 32:
      return "Green";
    case 33:
      return "White";
    default:
      return null;
  }
}

/**
 * Returns the semantic color tone for rendering a face-up tile.
 */
export function getTileTone(tile: Tile): TileTone {
  const suit = getTileSuit(tile);
  switch (suit) {
    case "Man":
      return "man";
    case "Bamboo":
      return "bamboo";
    case "Dot":
      return "dot";
    case "Wind":
      return "ink";
    case "Dragon": {
      const dragon = getDragonKind(tile);
      if (dragon === "Red") {
        return "red";
      }
      if (dragon === "Green") {
        return "bamboo";
      }
      return "dot";
    }
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

const SHORT_WIND_LABEL: Record<Wind, string> = {
  East: "东",
  South: "南",
  West: "西",
  North: "北",
};

/**
 * Formats a compact round/hand badge for the center compass hub (e.g. `东1`, `东2·连1`).
 */
export function formatCompactRoundTitle(turn: TurnView): string {
  const base = `${SHORT_WIND_LABEL[turn.wind_round]}${turn.hand_number}`;
  if (turn.dealer_streak > 0) {
    return `${base}·连${turn.dealer_streak}`;
  }
  return base;
}
