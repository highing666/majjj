import { describe, expect, it, vi } from "vitest";
import type { ClearHistoryResponse } from "../protocol/generated/ClearHistoryResponse";
import type { HistoryExportResponse } from "../protocol/generated/HistoryExportResponse";
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
    },
    matches: [],
    hands: [],
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
});
