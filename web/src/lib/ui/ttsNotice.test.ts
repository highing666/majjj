import { describe, expect, it, vi } from "vitest";
import { createTtsNoticeController, type TtsNoticeViewState } from "./ttsNotice";

describe("TtsNoticeController", () => {
  it("stays hidden initially and ignores initial available=true without prior outage", () => {
    vi.useFakeTimers();
    try {
      const controller = createTtsNoticeController({ recoveryDurationMs: 3000 });
      expect(controller.getState()).toEqual({
        visible: false,
        kind: null,
        message: "",
      });

      controller.applyNotice({
        available: true,
        message: "局域网台词语音服务已连接",
      });
      expect(controller.getState().visible).toBe(false);
      controller.dispose();
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows degraded notice persistently, flashes recovery on available=true, and auto-dismisses", () => {
    vi.useFakeTimers();
    try {
      const snapshots: TtsNoticeViewState[] = [];
      const controller = createTtsNoticeController({ recoveryDurationMs: 3000 });
      controller.subscribe((next) => snapshots.push(next));

      controller.applyNotice({
        available: false,
        message: "局域网台词语音暂不可用，继续显示气泡与离线语音包",
      });
      expect(controller.getState()).toEqual({
        visible: true,
        kind: "degraded",
        message: "局域网台词语音暂不可用，继续显示气泡与离线语音包",
      });

      // Stays visible across arbitrary elapsed time while degraded
      vi.advanceTimersByTime(60_000);
      expect(controller.getState().visible).toBe(true);
      expect(controller.getState().kind).toBe("degraded");

      // Recovery notice displays briefly then auto-dismisses
      controller.applyNotice({
        available: true,
        message: "局域网台词语音已恢复",
      });
      expect(controller.getState()).toEqual({
        visible: true,
        kind: "recovered",
        message: "局域网台词语音已恢复",
      });

      vi.advanceTimersByTime(2999);
      expect(controller.getState().visible).toBe(true);

      vi.advanceTimersByTime(1);
      expect(controller.getState()).toEqual({
        visible: false,
        kind: null,
        message: "",
      });

      // Duplicate available=true after recovery dismiss stays hidden
      controller.applyNotice({
        available: true,
        message: "局域网台词语音已恢复",
      });
      expect(controller.getState().visible).toBe(false);
      expect(snapshots.length).toBeGreaterThanOrEqual(4);

      controller.dispose();
    } finally {
      vi.useRealTimers();
    }
  });

  it("cancels pending recovery auto-dismiss if TTS degrades again before timer fires", () => {
    vi.useFakeTimers();
    try {
      const controller = createTtsNoticeController({ recoveryDurationMs: 3000 });

      controller.applyNotice({ available: false, message: "语音服务断开" });
      controller.applyNotice({ available: true, message: "语音服务恢复" });
      vi.advanceTimersByTime(1500);

      controller.applyNotice({ available: false, message: "语音服务再次超时" });
      expect(controller.getState()).toEqual({
        visible: true,
        kind: "degraded",
        message: "语音服务再次超时",
      });

      // Advancing past original recovery deadline must NOT hide the new degraded notice
      vi.advanceTimersByTime(5000);
      expect(controller.getState()).toEqual({
        visible: true,
        kind: "degraded",
        message: "语音服务再次超时",
      });

      controller.dispose();
    } finally {
      vi.useRealTimers();
    }
  });
});
