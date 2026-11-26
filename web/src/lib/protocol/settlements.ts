import type { KongKind } from "./generated/KongKind";
import type { WinKind } from "./generated/WinKind";

/**
 * Maps a `WinKind` variant to its canonical `GLOSSARY.md` label.
 */
export function winKindLabel(kind: WinKind): string {
  switch (kind) {
    case "Tsumo":
      return "自摸";
    case "Ron":
      return "点炮";
    case "RobKongHu":
      return "抢杠胡";
  }
}

/**
 * Maps a `KongKind` variant to its canonical `GLOSSARY.md` label.
 */
export function kongKindLabel(kind: KongKind): string {
  switch (kind) {
    case "ExposedKong":
      return "明杠";
    case "AddedKong":
      return "补杠";
    case "ConcealedKong":
      return "暗杠";
  }
}
