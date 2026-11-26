import type { AiDifficulty } from "../protocol/generated/AiDifficulty";
import type { LobbyConfig } from "../protocol/generated/LobbyConfig";
import type { MatchMode } from "../protocol/generated/MatchMode";
import type { SeatingRule } from "../protocol/generated/SeatingRule";
import type { TablePace } from "../protocol/generated/TablePace";

export interface ConfigOption<T extends string> {
  value: T;
  label: string;
}

export const MATCH_MODE_OPTIONS: ReadonlyArray<ConfigOption<MatchMode>> = [
  { value: "EastOnly", label: "东风战（东一至东四含连庄）" },
  { value: "FreeTable", label: "自由桌（不限小局数）" },
];

export const SEATING_RULE_OPTIONS: ReadonlyArray<ConfigOption<SeatingRule>> = [
  { value: "KeepLobbySeats", label: "按大厅所选座次（默认）" },
  { value: "Random", label: "随机定座" },
  { value: "FixedOpposite", label: "双人固定对门" },
  { value: "FixedAdjacent", label: "双人固定上下家" },
];

export const AI_DIFFICULTY_OPTIONS: ReadonlyArray<ConfigOption<AiDifficulty>> = [
  { value: "Strong", label: "强力" },
  { value: "Intermediate", label: "进阶" },
];

export const TABLE_PACE_OPTIONS: ReadonlyArray<ConfigOption<TablePace>> = [
  { value: "Standard", label: "标准节奏" },
  { value: "Fast", label: "快速节奏" },
];

/**
 * Returns the concise Chinese label for a `MatchMode`.
 */
export function matchModeLabel(mode: MatchMode): string {
  return mode === "EastOnly" ? "东风战" : "自由桌";
}

/**
 * Returns a new `LobbyConfig` with `partial` applied without mutating the source object.
 */
export function patchLobbyConfig(
  config: LobbyConfig,
  partial: Partial<LobbyConfig>,
): LobbyConfig {
  return { ...config, ...partial };
}
