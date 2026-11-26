import { describe, expect, it } from "vitest";
import {
  ALL_SEATS,
  formatRelativeSeatDisplay,
  formatRelativeSeatParen,
  formatSeatEntrySeatless,
  formatSeatlessDisplay,
  getRelativeSeats,
  resolveSeatEntryNickname,
  resolveSeatNickname,
  seatIndex,
  seatNumberLabel,
  seatRelativeRole,
  seatRelativeShortRole,
  seatRelativeSlot,
  type SeatDisplayEntry,
} from "./seats";

const SAMPLE_SEATS: readonly [
  SeatDisplayEntry,
  SeatDisplayEntry,
  SeatDisplayEntry,
  SeatDisplayEntry,
] = [
  { kind: "Human", nickname: "  小红  " },
  { kind: "Ai", nickname: "" },
  { seat_kind: "Human", nickname: "   " },
  { seat_kind: "Ai", nickname: "AI北" },
];

describe("protocol/seats", () => {
  it("maps counter-clockwise relative slots and roles for all four viewers", () => {
    expect(getRelativeSeats("East")).toEqual({
      bottom: "East",
      right: "South",
      top: "West",
      left: "North",
    });
    expect(getRelativeSeats("South")).toEqual({
      bottom: "South",
      right: "West",
      top: "North",
      left: "East",
    });
    expect(getRelativeSeats("West")).toEqual({
      bottom: "West",
      right: "North",
      top: "East",
      left: "South",
    });
    expect(getRelativeSeats("North")).toEqual({
      bottom: "North",
      right: "East",
      top: "South",
      left: "West",
    });

    expect(seatRelativeSlot("South", "South")).toBe("bottom");
    expect(seatRelativeSlot("South", "West")).toBe("right");
    expect(seatRelativeSlot("South", "North")).toBe("top");
    expect(seatRelativeSlot("South", "East")).toBe("left");

    expect(seatRelativeRole("South", "South")).toBe("自家");
    expect(seatRelativeRole("South", "West")).toBe("下家");
    expect(seatRelativeRole("South", "North")).toBe("对门");
    expect(seatRelativeRole("South", "East")).toBe("上家");

    expect(seatRelativeShortRole("South", "South")).toBe("自");
    expect(seatRelativeShortRole("South", "West")).toBe("下");
    expect(seatRelativeShortRole("South", "North")).toBe("对");
    expect(seatRelativeShortRole("South", "East")).toBe("上");
  });

  it("formats number labels, nicknames, and seatless/relative display strings", () => {
    expect(ALL_SEATS.map(seatIndex)).toEqual([0, 1, 2, 3]);
    expect(ALL_SEATS.map(seatNumberLabel)).toEqual([
      "1号位",
      "2号位",
      "3号位",
      "4号位",
    ]);

    expect(resolveSeatNickname("East", SAMPLE_SEATS)).toBe("小红");
    expect(resolveSeatNickname("South", SAMPLE_SEATS)).toBe("AI 2");
    expect(resolveSeatNickname("West", SAMPLE_SEATS)).toBe("");
    expect(resolveSeatNickname("North", SAMPLE_SEATS)).toBe("AI北");
    expect(resolveSeatNickname("East", null)).toBe("");

    expect(formatSeatlessDisplay("East", SAMPLE_SEATS)).toBe("小红");
    expect(formatSeatlessDisplay("South", SAMPLE_SEATS)).toBe("AI 2");
    expect(formatSeatlessDisplay("West", SAMPLE_SEATS)).toBe("3号位");
    expect(formatSeatlessDisplay("North", SAMPLE_SEATS)).toBe("AI北");
    expect(formatSeatlessDisplay("East", null)).toBe("1号位");
    expect(formatSeatlessDisplay("North", null)).toBe("4号位");

    expect(resolveSeatEntryNickname("West", SAMPLE_SEATS[2])).toBe("");
    expect(formatSeatEntrySeatless("East", SAMPLE_SEATS[0])).toBe("小红");
    expect(formatSeatEntrySeatless("South", SAMPLE_SEATS[1])).toBe("AI 2");
    expect(formatSeatEntrySeatless("West", SAMPLE_SEATS[2])).toBe("3号位");
    expect(formatSeatEntrySeatless("North", null)).toBe("4号位");

    expect(formatRelativeSeatDisplay("East", "South", SAMPLE_SEATS)).toBe(
      "上家 · 小红",
    );
    expect(formatRelativeSeatDisplay("South", "South", SAMPLE_SEATS)).toBe(
      "自家 · AI 2",
    );
    expect(formatRelativeSeatDisplay("West", "South", SAMPLE_SEATS)).toBe(
      "下家",
    );
    expect(formatRelativeSeatDisplay("North", "South", SAMPLE_SEATS)).toBe(
      "对门 · AI北",
    );
    expect(formatRelativeSeatDisplay("East", null, SAMPLE_SEATS)).toBe("小红");
    expect(formatRelativeSeatDisplay("West", null, SAMPLE_SEATS)).toBe(
      "3号位",
    );
    expect(formatRelativeSeatDisplay("East", "South", null)).toBe("上家");

    expect(formatRelativeSeatParen("East", "South", SAMPLE_SEATS)).toBe(
      "上家（小红）",
    );
    expect(formatRelativeSeatParen("West", "South", SAMPLE_SEATS)).toBe("下家");
    expect(formatRelativeSeatParen("West", null, SAMPLE_SEATS)).toBe("3号位");
  });
});
