import { describe, expect, it } from "vitest";
import {
  aiStyleLabel,
  getPersonaMeta,
  getPersonaMoodMeta,
  PERSONA_IDS,
  PERSONA_LIST,
  PERSONA_MOODS,
  personaFrameKey,
} from "./personas";

describe("protocol/personas metadata single source of truth", () => {
  it("defines all 5 built-in personas with spec-exact names and default AiStyles", () => {
    expect(PERSONA_IDS).toHaveLength(5);
    expect(PERSONA_LIST).toHaveLength(5);

    expect(getPersonaMeta("QuickHothead")).toMatchObject({
      name: "急眼快攻哥",
      defaultStyle: "AggressiveFast",
      styleLabel: "快攻",
    });
    expect(getPersonaMeta("TeaSarcasm")).toMatchObject({
      name: "阴阳茶艺师",
      defaultStyle: "DefensiveBigHand",
      styleLabel: "防守做大牌",
    });
    expect(getPersonaMeta("StubbornBlamer")).toMatchObject({
      name: "嘴硬死磕王",
      defaultStyle: "AggressiveFast",
      styleLabel: "快攻",
    });
    expect(getPersonaMeta("ChaosInstigator")).toMatchObject({
      name: "拱火乐子人",
      defaultStyle: "Balanced",
      styleLabel: "均衡",
    });
    expect(getPersonaMeta("TiltedMaster")).toMatchObject({
      name: "红温老雀神",
      defaultStyle: "DefensiveBigHand",
      styleLabel: "防守做大牌",
    });
    expect(aiStyleLabel("Balanced")).toBe("均衡");
  });

  it("defines all 6 canonical PersonaMood labels and stable frame keys", () => {
    expect(PERSONA_MOODS).toEqual([
      "Smug",
      "Mocking",
      "Tilted",
      "Nervous",
      "Plotting",
      "Deadpan",
    ]);

    for (const mood of PERSONA_MOODS) {
      const meta = getPersonaMoodMeta(mood);
      expect(meta.mood).toBe(mood);
      expect(meta.label.length).toBeGreaterThanOrEqual(4);
      expect(meta.shortLabel.length).toBeGreaterThanOrEqual(2);
    }

    expect(personaFrameKey("TeaSarcasm", "Mocking")).toBe(
      "persona_TeaSarcasm_Mocking",
    );
  });
});
