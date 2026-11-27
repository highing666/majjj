import { describe, expect, it, vi } from "vitest";
import {
  canShowShutdownControl,
  createShutdownController,
  type ShutdownFetch,
} from "./shutdownController";

describe("shutdownController", () => {
  it("exposes loopback visibility strictly when host_is_loopback is true", () => {
    expect(canShowShutdownControl(null)).toBe(false);
    expect(canShowShutdownControl({ host_is_loopback: false })).toBe(false);
    expect(canShowShutdownControl({ host_is_loopback: true })).toBe(true);
  });

  it("never triggers POST /api/shutdown when host_is_loopback is false", async () => {
    const fetchFn = vi.fn<ShutdownFetch>();
    const onShutdownSuccess = vi.fn();
    const controller = createShutdownController({ fetchFn, onShutdownSuccess });

    controller.syncHostIsLoopback(false);
    expect(controller.getState().visible).toBe(false);

    controller.requestConfirm();
    expect(controller.getState().phase).toBe("idle");

    const ok = await controller.confirmShutdown();
    expect(ok).toBe(false);
    expect(fetchFn).not.toHaveBeenCalled();
    expect(onShutdownSuccess).not.toHaveBeenCalled();
  });

  it("requires confirmation before calling POST /api/shutdown and handles cancel", () => {
    const fetchFn = vi.fn<ShutdownFetch>();
    const controller = createShutdownController({ fetchFn });
    controller.syncHostIsLoopback(true);

    expect(controller.getState()).toEqual({
      visible: true,
      phase: "idle",
      errorMessage: null,
    });

    controller.requestConfirm();
    expect(controller.getState().phase).toBe("confirming");
    expect(fetchFn).not.toHaveBeenCalled();

    controller.cancelConfirm();
    expect(controller.getState()).toEqual({
      visible: true,
      phase: "idle",
      errorMessage: null,
    });
    expect(fetchFn).not.toHaveBeenCalled();
  });

  it("transitions to stopped and invokes onShutdownSuccess on 200 OK", async () => {
    const fetchFn = vi.fn<ShutdownFetch>().mockResolvedValue({
      ok: true,
      status: 200,
    });
    const onShutdownSuccess = vi.fn();
    const controller = createShutdownController({ fetchFn, onShutdownSuccess });
    controller.syncHostIsLoopback(true);

    controller.requestConfirm();
    const result = await controller.confirmShutdown();

    expect(result).toBe(true);
    expect(fetchFn).toHaveBeenCalledTimes(1);
    expect(fetchFn).toHaveBeenCalledWith("/api/shutdown", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
    });
    expect(onShutdownSuccess).toHaveBeenCalledTimes(1);
    expect(controller.getState()).toEqual({
      visible: false,
      phase: "stopped",
      errorMessage: null,
    });

    // Stopped state remains sticky even if snapshot still references loopback.
    controller.syncHostIsLoopback(true);
    expect(controller.getState().phase).toBe("stopped");
  });

  it("surfaces 403 and network errors without calling onShutdownSuccess", async () => {
    const fetchFn = vi
      .fn<ShutdownFetch>()
      .mockResolvedValueOnce({ ok: false, status: 403 })
      .mockRejectedValueOnce(new Error("connection reset"));
    const onShutdownSuccess = vi.fn();
    const controller = createShutdownController({ fetchFn, onShutdownSuccess });
    controller.syncHostIsLoopback(true);

    controller.requestConfirm();
    const first = await controller.confirmShutdown();
    expect(first).toBe(false);
    expect(onShutdownSuccess).not.toHaveBeenCalled();
    expect(controller.getState().phase).toBe("error");
    expect(controller.getState().errorMessage).toContain("403");

    const second = await controller.confirmShutdown();
    expect(second).toBe(false);
    expect(onShutdownSuccess).not.toHaveBeenCalled();
    expect(controller.getState().phase).toBe("error");
    expect(controller.getState().errorMessage).toBeTruthy();

    controller.cancelConfirm();
    expect(controller.getState()).toEqual({
      visible: true,
      phase: "idle",
      errorMessage: null,
    });
  });
});
