import { describe, expect, it } from "vitest";
import type { QuickEmote } from "../protocol/generated/QuickEmote";
import {
  QUICK_EMOTE_COOLDOWN_MS,
  QUICK_EMOTE_LIST,
  getQuickEmoteMeta,
} from "../protocol/personas";
import {
  createQuickEmoteCooldown,
  formatCooldownSeconds,
} from "./quickEmoteCooldown";

describe("quickEmoteCooldown & catalog", () => {
  it("exposes all 6 canonical Spec #63 quick emotes in display order", () => {
    expect(QUICK_EMOTE_LIST).toHaveLength(6);
    expect(QUICK_EMOTE_LIST.map((item) => item.label)).toEqual([
      "就这？",
      "搞快点",
      "别点我 / 求放过",
      "666 / 漂亮",
      "发牌员演我",
      "这把我要胡大的",
    ]);
    expect(getQuickEmoteMeta("GoingBig").label).toBe("这把我要胡大的");
  });

  it("enforces 4-second client-side cooldown with countdown ticks and blocks rapid duplicate sends", () => {
    let nowMs = 1000;
    const sent: QuickEmote[] = [];
    const scheduled = new Map<number, { cb: () => void; delayMs: number }>();
    let nextTimerId = 1;

    const controller = createQuickEmoteCooldown({
      cooldownMs: QUICK_EMOTE_COOLDOWN_MS,
      tickMs: 1000,
      now: () => nowMs,
      scheduler: {
        setTimeout(cb, delayMs) {
          const id = nextTimerId++;
          scheduled.set(id, { cb, delayMs });
          return id;
        },
        clearTimeout(handle) {
          scheduled.delete(handle as number);
        },
      },
      onSend: (emote) => {
        sent.push(emote);
        return true;
      },
    });

    expect(controller.getState()).toEqual({
      onCooldown: false,
      remainingMs: 0,
      lastEmote: null,
    });

    // First send succeeds and enters 4000ms cooldown.
    expect(controller.trigger("IsThatIt")).toBe(true);
    expect(sent).toEqual(["IsThatIt"]);
    expect(controller.getState()).toEqual({
      onCooldown: true,
      remainingMs: 4000,
      lastEmote: "IsThatIt",
    });
    expect(formatCooldownSeconds(controller.getState().remainingMs)).toBe("4s");

    // Rapid second send during cooldown is blocked client-side.
    nowMs += 500;
    expect(controller.trigger("HurryUp")).toBe(false);
    expect(sent).toEqual(["IsThatIt"]);

    // Advance tick to 2.5s remaining -> still cooling.
    nowMs = 2500;
    const [tick1] = [...scheduled.keys()];
    const entry1 = scheduled.get(tick1!)!;
    scheduled.delete(tick1!);
    entry1.cb();
    expect(controller.getState().onCooldown).toBe(true);
    expect(formatCooldownSeconds(controller.getState().remainingMs)).toBe("3s");

    // Advance past 4000ms expiry -> cooldown clears and next emote succeeds.
    nowMs = 5000;
    const [tick2] = [...scheduled.keys()];
    const entry2 = scheduled.get(tick2!)!;
    scheduled.delete(tick2!);
    entry2.cb();
    expect(controller.getState().onCooldown).toBe(false);
    expect(controller.getState().remainingMs).toBe(0);
    expect(formatCooldownSeconds(0)).toBe("");

    expect(controller.trigger("DealerRigged")).toBe(true);
    expect(sent).toEqual(["IsThatIt", "DealerRigged"]);
    expect(controller.getState().lastEmote).toBe("DealerRigged");

    controller.dispose();
    expect(scheduled.size).toBe(0);
  });

  it("does not enter cooldown when onSend returns false (disconnected/offline)", () => {
    const controller = createQuickEmoteCooldown({
      onSend: () => false,
    });
    expect(controller.trigger("NicePlay")).toBe(false);
    expect(controller.getState().onCooldown).toBe(false);
    expect(controller.getState().lastEmote).toBeNull();
    controller.dispose();
  });
});
