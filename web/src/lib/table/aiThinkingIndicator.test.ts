import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  AI_LONG_THINK_INDICATOR_DELAY_MS,
  createAiThinkingIndicatorController,
} from "./aiThinkingIndicator";

describe("aiThinkingIndicator", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("illuminates the active AI seat only after 2500ms in Playing phase and clears on turn/phase/seat transitions", () => {
    expect(AI_LONG_THINK_INDICATOR_DELAY_MS).toBe(2500);

    const observed: Array<string | null> = [];
    const controller = createAiThinkingIndicatorController();
    const unsub = controller.subscribe((seat) => observed.push(seat));

    expect(controller.getThinkingSeat()).toBeNull();

    // Active AI seat starts thinking at seq=10 -> still hidden at 2499ms, illuminates at 2500ms.
    controller.observe({
      phase: "InMatch",
      publicPhase: "Playing",
      activeSeat: "South",
      activeSeatIsAi: true,
      turnSeq: 10,
    });
    vi.advanceTimersByTime(2499);
    expect(controller.getThinkingSeat()).toBeNull();

    vi.advanceTimersByTime(1);
    expect(controller.getThinkingSeat()).toBe("South");

    // Re-observing the exact same turn (e.g., unrelated re-render) keeps indicator steady without resetting.
    controller.observe({
      phase: "InMatch",
      publicPhase: "Playing",
      activeSeat: "South",
      activeSeatIsAi: true,
      turnSeq: 10,
    });
    expect(controller.getThinkingSeat()).toBe("South");

    // Same seat advances to a new turnSeq (e.g., meld follow-up discard) -> clears immediately and re-arms 2500ms!
    controller.observe({
      phase: "InMatch",
      publicPhase: "Playing",
      activeSeat: "South",
      activeSeatIsAi: true,
      turnSeq: 11,
    });
    expect(controller.getThinkingSeat()).toBeNull();
    vi.advanceTimersByTime(2500);
    expect(controller.getThinkingSeat()).toBe("South");

    // Switching to another AI seat clears immediately and starts a fresh 2500ms timer for West.
    controller.observe({
      phase: "InMatch",
      publicPhase: "Playing",
      activeSeat: "West",
      activeSeatIsAi: true,
      turnSeq: 12,
    });
    expect(controller.getThinkingSeat()).toBeNull();
    vi.advanceTimersByTime(2500);
    expect(controller.getThinkingSeat()).toBe("West");

    // Switching to a human seat (or seat takeover turning West into Human) clears immediately and never re-arms.
    controller.observe({
      phase: "InMatch",
      publicPhase: "Playing",
      activeSeat: "West",
      activeSeatIsAi: false,
      turnSeq: 12,
    });
    expect(controller.getThinkingSeat()).toBeNull();
    vi.advanceTimersByTime(5000);
    expect(controller.getThinkingSeat()).toBeNull();

    // Non-Playing phase (BetweenHands / Lobby) never illuminates.
    controller.observe({
      phase: "InMatch",
      publicPhase: "BetweenHands",
      activeSeat: "North",
      activeSeatIsAi: true,
      turnSeq: 13,
    });
    vi.advanceTimersByTime(5000);
    expect(controller.getThinkingSeat()).toBeNull();

    expect(observed).toEqual([null, "South", null, "South", null, "West", null]);

    unsub();
    controller.dispose();
  });

  it("cancels pending timer cleanly on dispose", () => {
    const controller = createAiThinkingIndicatorController();
    controller.observe({
      phase: "InMatch",
      publicPhase: "Playing",
      activeSeat: "North",
      activeSeatIsAi: true,
      turnSeq: 20,
    });
    vi.advanceTimersByTime(1000);
    controller.dispose();
    vi.advanceTimersByTime(5000);
    expect(controller.getThinkingSeat()).toBeNull();
  });
});
