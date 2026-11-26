import { describe, expect, it, vi } from "vitest";
import type { ClearHistoryResponse } from "../protocol/generated/ClearHistoryResponse";
import type { ClearPersonaMemoryResponse } from "../protocol/generated/ClearPersonaMemoryResponse";
import type { HistoryExportResponse } from "../protocol/generated/HistoryExportResponse";
import type { PersonaMemoryResponse } from "../protocol/generated/PersonaMemoryResponse";
import type { PreferencesData } from "../protocol/generated/PreferencesData";
import type { StatsResponse } from "../protocol/generated/StatsResponse";
import {
  buildHistoryExportDownload,
  createHistoryClient,
  createHistoryController,
  HistoryClientError,
  type HistoryFetch,
} from "./historyClient";

function jsonResponse(
  status: number,
  body: unknown,
  ok = status >= 200 && status < 300,
) {
  return {
    ok,
    status,
    json: async () => body,
    text: async () => JSON.stringify(body),
  };
}

function sampleStats(): StatsResponse {
  return {
    schema_version: 1,
    total_matches: 1,
    total_hands: 2,
    players: [
      {
        nickname: "阿东",
        completed_matches: 1,
        settled_hands: 2,
        aborted_hands: 0,
        wins: 1,
        tsumo_wins: 1,
        deal_ins: 0,
        ai_assisted_hands: 0,
        cumulative_net_score: 18,
        average_net_score: 18,
        average_hand_net_score: 9,
      },
    ],
    recent_matches: [],
    recent_hands: [],
  };
}

function sampleExport(): HistoryExportResponse {
  return {
    schema_version: 1,
    exported_at_ms: Date.UTC(2026, 10, 26, 4, 10, 44),
    preferences: {
      schema_version: 1,
      recent_nicknames: ["阿东"],
      default_mode: "EastOnly",
      default_seating: "Random",
      default_ai_difficulty: "Strong",
      default_pace: "Standard",
      llm_base_url: null,
      tts_base_url: null,
      slow_think_nudge_enabled: true,
    },
    matches: [],
    hands: [],
  };
}

function samplePersonaMemory(): PersonaMemoryResponse {
  return {
    schema_version: 1,
    total_pairs: 1,
    total_notes: 2,
    personas: [
      {
        persona_id: "TeaSarcasm",
        players: [
          {
            player_nickname: "小王",
            ledger: {
              hands_played: 4,
              persona_wins_against_player: 1,
              player_wins_against_persona: 2,
              persona_dealt_into_player: 1,
              player_dealt_into_persona: 1,
              highest_fan_seen: 4,
              net_score_delta: -12,
              player_pon_count: 3,
              player_kong_count: 1,
            },
            grudge_notes: [
              {
                summary: "小王抢杠胡气死我了",
                created_at_ms: Date.UTC(2026, 10, 26, 4, 0, 0),
                trigger: "RobKongHu",
              },
              {
                summary: "这笔债下次开桌慢慢算",
                created_at_ms: Date.UTC(2026, 10, 26, 4, 15, 0),
                trigger: "MatchFinished",
              },
            ],
            updated_at_ms: Date.UTC(2026, 10, 26, 4, 15, 0),
          },
        ],
      },
    ],
  };
}

describe("historyClient", () => {
  it("fetches stats from GET /api/stats with no-store cache policy", async () => {
    const expected = sampleStats();
    const fetchFn = vi
      .fn<HistoryFetch>()
      .mockResolvedValue(jsonResponse(200, expected));
    const client = createHistoryClient(fetchFn);

    const stats = await client.fetchStats();

    expect(stats).toEqual(expected);
    expect(fetchFn).toHaveBeenCalledTimes(1);
    expect(fetchFn).toHaveBeenCalledWith("/api/stats", {
      method: "GET",
      cache: "no-store",
    });
  });

  it("fetches preferences from GET /api/preferences with no-store cache policy", async () => {
    const expected: PreferencesData = {
      schema_version: 1,
      recent_nicknames: ["阿东", "阿南"],
      default_mode: "EastOnly",
      default_seating: "Random",
      default_ai_difficulty: "Strong",
      default_pace: "Standard",
      llm_base_url: null,
      tts_base_url: null,
      slow_think_nudge_enabled: true,
    };
    const fetchFn = vi
      .fn<HistoryFetch>()
      .mockResolvedValue(jsonResponse(200, expected));
    const client = createHistoryClient(fetchFn);

    const prefs = await client.fetchPreferences();

    expect(prefs).toEqual(expected);
    expect(fetchFn).toHaveBeenCalledTimes(1);
    expect(fetchFn).toHaveBeenCalledWith("/api/preferences", {
      method: "GET",
      cache: "no-store",
    });
  });

  it("exports history from GET /api/history/export and builds deterministic JSON download payload", async () => {
    const exported = sampleExport();
    const fetchFn = vi
      .fn<HistoryFetch>()
      .mockResolvedValue(jsonResponse(200, exported));
    const client = createHistoryClient(fetchFn);

    const result = await client.exportHistory();
    expect(result).toEqual(exported);
    expect(fetchFn).toHaveBeenCalledWith("/api/history/export", {
      method: "GET",
      cache: "no-store",
    });

    const download = buildHistoryExportDownload(result);
    expect(download.filename).toBe("majjj-history-20261126-041044.json");
    expect(download.mimeType).toBe("application/json;charset=utf-8");
    expect(JSON.parse(download.jsonText)).toEqual(exported);
  });

  it("sends explicit { confirm: true } payload to POST /api/history/clear and blocks unconfirmed calls", async () => {
    const clearResp: ClearHistoryResponse = {
      cleared: true,
      cleared_matches: 3,
      cleared_hands: 12,
    };
    const fetchFn = vi
      .fn<HistoryFetch>()
      .mockResolvedValue(jsonResponse(200, clearResp));
    const client = createHistoryClient(fetchFn);

    await expect(client.clearHistory(false)).rejects.toBeInstanceOf(
      HistoryClientError,
    );
    expect(fetchFn).not.toHaveBeenCalled();

    const resp = await client.clearHistory(true);
    expect(resp).toEqual(clearResp);
    expect(fetchFn).toHaveBeenCalledTimes(1);
    expect(fetchFn).toHaveBeenCalledWith("/api/history/clear", {
      method: "POST",
      cache: "no-store",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ confirm: true }),
    });
  });

  it("surfaces structured error messages for server errors and network failures", async () => {
    const fetchFn = vi
      .fn<HistoryFetch>()
      .mockResolvedValueOnce(
        jsonResponse(400, { error: "confirm must be true" }),
      )
      .mockRejectedValueOnce(new Error("network offline"));
    const client = createHistoryClient(fetchFn);

    try {
      await client.clearHistory(true);
      expect.unreachable("expected clearHistory to reject on 400");
    } catch (err) {
      expect(err).toBeInstanceOf(HistoryClientError);
      const clientErr = err as HistoryClientError;
      expect(clientErr.status).toBe(400);
      expect(clientErr.message).toContain("confirm must be true");
    }

    try {
      await client.fetchStats();
      expect.unreachable("expected fetchStats to reject on network error");
    } catch (err) {
      expect(err).toBeInstanceOf(HistoryClientError);
      const clientErr = err as HistoryClientError;
      expect(clientErr.status).toBeNull();
      expect(clientErr.message).toContain("无法连接");
    }
  });

  it("fetches persona memory and sends explicit { confirm: true } to POST /api/persona-memory/clear", async () => {
    const memory = samplePersonaMemory();
    const clearResp: ClearPersonaMemoryResponse = {
      cleared: true,
      cleared_pairs: 1,
      cleared_notes: 2,
    };
    const fetchFn = vi
      .fn<HistoryFetch>()
      .mockResolvedValueOnce(jsonResponse(200, memory))
      .mockResolvedValueOnce(jsonResponse(200, clearResp));
    const client = createHistoryClient(fetchFn);

    const fetched = await client.fetchPersonaMemory();
    expect(fetched).toEqual(memory);
    expect(fetchFn).toHaveBeenNthCalledWith(1, "/api/persona-memory", {
      method: "GET",
      cache: "no-store",
    });

    await expect(client.clearPersonaMemory(false)).rejects.toBeInstanceOf(
      HistoryClientError,
    );
    expect(fetchFn).toHaveBeenCalledTimes(1);

    const cleared = await client.clearPersonaMemory(true);
    expect(cleared).toEqual(clearResp);
    expect(fetchFn).toHaveBeenNthCalledWith(2, "/api/persona-memory/clear", {
      method: "POST",
      cache: "no-store",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ confirm: true }),
    });
  });
});

describe("historyController", () => {
  it("loads stats on open, handles failure and retry, and runs export download", async () => {
    const stats = sampleStats();
    const exported = sampleExport();
    const fetchFn = vi
      .fn<HistoryFetch>()
      .mockRejectedValueOnce(new Error("timeout"))
      .mockResolvedValueOnce(jsonResponse(200, stats))
      .mockResolvedValueOnce(jsonResponse(200, exported));
    const triggerDownload = vi.fn();

    const controller = createHistoryController({
      fetchFn,
      onTriggerDownload: triggerDownload,
    });

    await controller.open();
    expect(controller.getState().open).toBe(true);
    expect(controller.getState().loading).toBe(false);
    expect(controller.getState().stats).toBeNull();
    expect(controller.getState().errorMessage).toBeTruthy();

    await controller.refresh();
    expect(controller.getState().errorMessage).toBeNull();
    expect(controller.getState().stats).toEqual(stats);

    const ok = await controller.requestExport();
    expect(ok).toBe(true);
    expect(triggerDownload).toHaveBeenCalledTimes(1);
    expect(triggerDownload.mock.calls[0][0].filename).toBe(
      "majjj-history-20261126-041044.json",
    );
    expect(controller.getState().exportPhase).toBe("success");
    expect(controller.getState().exportNotice).toContain(
      "majjj-history-20261126-041044.json",
    );
  });

  it("enforces two-step clear confirmation and immediately re-fetches stats on success", async () => {
    const initialStats = sampleStats();
    const clearedStats: StatsResponse = {
      schema_version: 1,
      total_matches: 0,
      total_hands: 0,
      players: [],
      recent_matches: [],
      recent_hands: [],
    };
    const fetchFn = vi
      .fn<HistoryFetch>()
      .mockResolvedValueOnce(jsonResponse(200, initialStats))
      .mockResolvedValueOnce(
        jsonResponse(200, {
          cleared: true,
          cleared_matches: 1,
          cleared_hands: 2,
        } satisfies ClearHistoryResponse),
      )
      .mockResolvedValueOnce(jsonResponse(200, clearedStats));

    const controller = createHistoryController({ fetchFn });
    await controller.open();
    expect(controller.getState().stats?.total_hands).toBe(2);

    // First click only enters confirming state; never calls POST /api/history/clear.
    controller.requestClear();
    expect(controller.getState().clearPhase).toBe("confirming");
    expect(fetchFn).toHaveBeenCalledTimes(1);

    // Cancel returns to idle without clearing.
    controller.cancelClear();
    expect(controller.getState().clearPhase).toBe("idle");
    expect(fetchFn).toHaveBeenCalledTimes(1);

    // Confirmed clear calls POST /api/history/clear then immediately re-fetches GET /api/stats.
    controller.requestClear();
    const cleared = await controller.confirmClear();
    expect(cleared).toBe(true);
    expect(fetchFn).toHaveBeenCalledTimes(3);
    expect(fetchFn.mock.calls[1][0]).toBe("/api/history/clear");
    expect(fetchFn.mock.calls[2][0]).toBe("/api/stats");
    expect(controller.getState().clearPhase).toBe("idle");
    expect(controller.getState().stats).toEqual(clearedStats);
    expect(controller.getState().clearNotice).toContain("1 场对局");
    expect(controller.getState().clearNotice).toContain("2 小局");
  });

  it("handles export/clear errors cleanly without wiping loaded stats", async () => {
    const stats = sampleStats();
    const fetchFn = vi
      .fn<HistoryFetch>()
      .mockResolvedValueOnce(jsonResponse(200, stats))
      .mockResolvedValueOnce(jsonResponse(500, { error: "disk busy" }))
      .mockResolvedValueOnce(jsonResponse(500, { error: "clear failed" }));

    const controller = createHistoryController({ fetchFn });
    await controller.open();

    const exportOk = await controller.requestExport();
    expect(exportOk).toBe(false);
    expect(controller.getState().exportPhase).toBe("error");
    expect(controller.getState().exportErrorMessage).toContain("disk busy");
    expect(controller.getState().stats).toEqual(stats);

    controller.requestClear();
    const clearOk = await controller.confirmClear();
    expect(clearOk).toBe(false);
    expect(controller.getState().clearPhase).toBe("error");
    expect(controller.getState().clearErrorMessage).toContain("clear failed");
    expect(controller.getState().stats).toEqual(stats);

    controller.close();
    expect(controller.getState().open).toBe(false);
    expect(controller.getState().clearPhase).toBe("idle");
  });

  it("loads persona memory and enforces two-step clear confirmation with automatic re-fetch", async () => {
    const initialMemory = samplePersonaMemory();
    const emptyMemory: PersonaMemoryResponse = {
      schema_version: 1,
      total_pairs: 0,
      total_notes: 0,
      personas: [{ persona_id: "TeaSarcasm", players: [] }],
    };
    const fetchFn = vi
      .fn<HistoryFetch>()
      .mockResolvedValueOnce(jsonResponse(200, initialMemory))
      .mockResolvedValueOnce(
        jsonResponse(200, {
          cleared: true,
          cleared_pairs: 1,
          cleared_notes: 2,
        } satisfies ClearPersonaMemoryResponse),
      )
      .mockResolvedValueOnce(jsonResponse(200, emptyMemory));

    const controller = createHistoryController({ fetchFn });
    await controller.refreshPersonaMemory();
    expect(controller.getState().personaMemory).toEqual(initialMemory);
    expect(controller.getState().personaMemoryErrorMessage).toBeNull();

    // First click enters confirming state without posting.
    controller.requestClearMemory();
    expect(controller.getState().clearMemoryPhase).toBe("confirming");
    expect(fetchFn).toHaveBeenCalledTimes(1);

    // Cancel returns to idle.
    controller.cancelClearMemory();
    expect(controller.getState().clearMemoryPhase).toBe("idle");
    expect(fetchFn).toHaveBeenCalledTimes(1);

    // Confirm posts clear and immediately re-fetches /api/persona-memory.
    controller.requestClearMemory();
    const ok = await controller.confirmClearMemory();
    expect(ok).toBe(true);
    expect(fetchFn).toHaveBeenCalledTimes(3);
    expect(fetchFn.mock.calls[1][0]).toBe("/api/persona-memory/clear");
    expect(fetchFn.mock.calls[2][0]).toBe("/api/persona-memory");
    expect(controller.getState().clearMemoryPhase).toBe("idle");
    expect(controller.getState().personaMemory).toEqual(emptyMemory);
    expect(controller.getState().clearMemoryNotice).toContain("1 组牌友记忆");
    expect(controller.getState().clearMemoryNotice).toContain("2 条恩怨小本本");
  });
});
