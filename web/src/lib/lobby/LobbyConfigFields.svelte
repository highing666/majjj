<script lang="ts">
  import type { AiDifficulty } from "../protocol/generated/AiDifficulty";
  import type { LobbyConfig } from "../protocol/generated/LobbyConfig";
  import type { MatchMode } from "../protocol/generated/MatchMode";
  import type { SeatingRule } from "../protocol/generated/SeatingRule";
  import type { TablePace } from "../protocol/generated/TablePace";
  import {
    AI_DIFFICULTY_OPTIONS,
    MATCH_MODE_OPTIONS,
    SEATING_RULE_OPTIONS,
    TABLE_PACE_OPTIONS,
    patchLobbyConfig,
  } from "./lobbyConfig";

  interface Props {
    config: LobbyConfig;
    disabled?: boolean;
    compact?: boolean;
    onUpdateConfig: (config: LobbyConfig) => void;
  }

  let {
    config,
    disabled = false,
    compact = false,
    onUpdateConfig,
  }: Props = $props();

  function updateField<K extends keyof LobbyConfig>(
    key: K,
    value: LobbyConfig[K],
  ): void {
    if (disabled) {
      return;
    }
    onUpdateConfig(patchLobbyConfig(config, { [key]: value }));
  }
</script>

<div class="config-grid" class:config-compact={compact}>
  <label class="config-field">
    <span class="field-label">对局模式</span>
    <select
      class="select-input"
      value={config.mode}
      onchange={(e) =>
        updateField(
          "mode",
          (e.currentTarget as HTMLSelectElement).value as MatchMode,
        )}
      {disabled}
    >
      {#each MATCH_MODE_OPTIONS as opt (opt.value)}
        <option value={opt.value}>{opt.label}</option>
      {/each}
    </select>
  </label>

  <label class="config-field">
    <span class="field-label">定座规则</span>
    <select
      class="select-input"
      value={config.seating}
      onchange={(e) =>
        updateField(
          "seating",
          (e.currentTarget as HTMLSelectElement).value as SeatingRule,
        )}
      {disabled}
    >
      {#each SEATING_RULE_OPTIONS as opt (opt.value)}
        <option value={opt.value}>{opt.label}</option>
      {/each}
    </select>
  </label>

  <label class="config-field">
    <span class="field-label">AI 难度档位</span>
    <select
      class="select-input"
      value={config.ai_difficulty}
      onchange={(e) =>
        updateField(
          "ai_difficulty",
          (e.currentTarget as HTMLSelectElement).value as AiDifficulty,
        )}
      {disabled}
    >
      {#each AI_DIFFICULTY_OPTIONS as opt (opt.value)}
        <option value={opt.value}>{opt.label}</option>
      {/each}
    </select>
  </label>

  <label class="config-field">
    <span class="field-label">牌桌节奏</span>
    <select
      class="select-input"
      value={config.pace}
      onchange={(e) =>
        updateField(
          "pace",
          (e.currentTarget as HTMLSelectElement).value as TablePace,
        )}
      {disabled}
    >
      {#each TABLE_PACE_OPTIONS as opt (opt.value)}
        <option value={opt.value}>{opt.label}</option>
      {/each}
    </select>
  </label>
</div>

<style>
  .config-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .config-compact {
    gap: 8px;
  }

  .config-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .config-compact .config-field {
    gap: 4px;
  }

  .field-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-muted);
  }

  .select-input {
    width: 100%;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 9px 12px;
    background: #fff;
  }

  .config-compact .select-input {
    padding: 7px 10px;
    font-size: 13px;
  }

  @media (max-width: 540px) {
    .config-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
