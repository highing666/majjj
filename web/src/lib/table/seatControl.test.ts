import { describe, expect, it } from "vitest";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import { ALL_SEATS } from "../protocol/seats";
import {
  buildOfflinePauseBanner,
  buildSeatControlItems,
  buildTakeOverSeatItems,
  getOfflineHumanTakeOverSeats,
  getTakeOverModalCopy,
  shouldShowReclaimControl,
} from "./seatControl";

function makeSeat(
  seat: Seat,
  overrides: Partial<SeatInfo> = {},
): SeatInfo {
  return {
    seat,
    kind: "Ai",
    nickname: `AI ${ALL_SEATS.indexOf(seat) + 1}`,
    connected: true,
    ai_assisted: false,
    ai_difficulty: "Strong",
    ai_style: "Balanced",
    persona_id: null,
    mood: null,
    voice_pack_id: null,
    ...overrides,
  };
}

function makeSnapshot(overrides: Partial<TableSnapshot> = {}): TableSnapshot {
  return {
    table_id: "tbl-1",
    phase: "InMatch",
    config: {
      mode: "FreeTable",
      seating: "FixedAdjacent",
      ai_difficulty: "Strong",
      pace: "Standard",
    },
    pace: "Standard",
    slow_think_nudge_enabled: true,
    seats: [
      makeSeat("East", {
        kind: "Human",
        nickname: "东家",
        connected: true,
        ai_difficulty: null,
        ai_style: null,
      }),
      makeSeat("South", {
        kind: "Human",
        nickname: "南家",
        connected: false,
        ai_difficulty: null,
        ai_style: null,
      }),
      makeSeat("West"),
      makeSeat("North"),
    ],
    your_seat: "East",
    seat_token: "tok-1",
    player_view: null,
    pending_destructive_op: null,
    seq: 4,
    server_version: "0.1.0",
    lan_urls: {
      preferred_ipv4_url: null,
      ipv4_urls: [],
      local_hostname_url: null,
    },
    host_is_loopback: true,
    client_is_remote: false,
    ...overrides,
  };
}

describe("seatControl view-model helpers", () => {
  it("builds offline pause banner only when at least one human seat is disconnected InMatch", () => {
    const pausedSnap = makeSnapshot();
    expect(buildOfflinePauseBanner(pausedSnap)).toContain("下家（南家）");
    expect(buildOfflinePauseBanner(pausedSnap)).toContain("暂停");
    expect(buildOfflinePauseBanner(pausedSnap)).not.toMatch(/东座|南座|西座|北座/);

    const viewerlessPaused = makeSnapshot({ your_seat: null });
    expect(buildOfflinePauseBanner(viewerlessPaused)).toContain("南家");
    expect(buildOfflinePauseBanner(viewerlessPaused)).not.toMatch(/东座|南座|西座|北座/);

    const aiAssistedSnap = makeSnapshot({
      seats: [
        makeSeat("East", {
          kind: "Human",
          nickname: "东家",
          connected: true,
          ai_difficulty: null,
          ai_style: null,
        }),
        makeSeat("South", {
          kind: "Human",
          nickname: "南家",
          connected: false,
          ai_assisted: true,
          ai_difficulty: "Strong",
          ai_style: "Balanced",
        }),
        makeSeat("West"),
        makeSeat("North"),
      ],
    });
    expect(buildOfflinePauseBanner(aiAssistedSnap)).toContain("AI 托管");

    const allOnline = makeSnapshot({
      seats: [
        makeSeat("East", { kind: "Human", nickname: "东家", connected: true, ai_difficulty: null, ai_style: null }),
        makeSeat("South", { kind: "Human", nickname: "南家", connected: true, ai_difficulty: null, ai_style: null }),
        makeSeat("West"),
        makeSeat("North"),
      ],
    });
    expect(buildOfflinePauseBanner(allOnline)).toBeNull();
    expect(buildOfflinePauseBanner(makeSnapshot({ phase: "Lobby" }))).toBeNull();
  });

  it("detects when your seat is AI-assisted and exposes one-click reclaim", () => {
    const snap = makeSnapshot({
      your_seat: "South",
      seats: [
        makeSeat("East", { kind: "Human", nickname: "东家", connected: true, ai_difficulty: null, ai_style: null }),
        makeSeat("South", {
          kind: "Human",
          nickname: "南家",
          connected: true,
          ai_assisted: true,
          ai_difficulty: "Strong",
          ai_style: "Balanced",
        }),
        makeSeat("West"),
        makeSeat("North"),
      ],
    });
    expect(shouldShowReclaimControl(snap)).toBe(true);
    expect(shouldShowReclaimControl(makeSnapshot())).toBe(false);
    expect(shouldShowReclaimControl(makeSnapshot({ your_seat: null }))).toBe(false);
  });

  it("builds toggleable AI assist controls for offline human seats and own seat", () => {
    const items = buildSeatControlItems(makeSnapshot());
    expect(items).toEqual([
      {
        seat: "East",
        displayName: "自家 · 东家",
        isSelf: true,
        isOffline: false,
        aiAssisted: false,
        nextEnabled: true,
        actionLabel: "本座暂交 AI 托管",
      },
      {
        seat: "South",
        displayName: "下家 · 南家",
        isSelf: false,
        isOffline: true,
        aiAssisted: false,
        nextEnabled: true,
        actionLabel: "暂由 AI 托管",
      },
    ]);
  });

  it("builds observer takeover items for AI seats InMatch and offline human seats across phases", () => {
    const observerInMatch = makeSnapshot({ your_seat: null });
    const items = buildTakeOverSeatItems(observerInMatch);

    expect(items.find((i) => i.seatInfo.seat === "East")).toMatchObject({
      displayName: "东家",
      takeOverKind: null,
      statusLabel: "在线人类",
      actionLabel: null,
    });
    expect(items.find((i) => i.seatInfo.seat === "South")).toMatchObject({
      displayName: "南家",
      takeOverKind: "offline_human",
      statusLabel: "已离线",
      actionLabel: "接管离线座位",
    });
    expect(items.find((i) => i.seatInfo.seat === "West")).toMatchObject({
      displayName: "AI 3",
      takeOverKind: "ai",
      statusLabel: "AI 对局中",
      actionLabel: "接管入座",
    });
    const namelessObserverSnap = makeSnapshot({
      your_seat: null,
      seats: [
        makeSeat("East", { kind: "Human", nickname: "", connected: true, ai_difficulty: null, ai_style: null }),
        makeSeat("South", { kind: "Human", nickname: "南家", connected: false, ai_difficulty: null, ai_style: null }),
        makeSeat("West"),
        makeSeat("North"),
      ],
    });
    expect(
      buildTakeOverSeatItems(namelessObserverSnap).find((i) => i.seatInfo.seat === "East")?.displayName,
    ).toBe("1号位");

    const observerLobby = makeSnapshot({ phase: "Lobby", your_seat: null });
    const lobbyItems = buildTakeOverSeatItems(observerLobby);
    expect(lobbyItems.find((i) => i.seatInfo.seat === "West")?.takeOverKind).toBeNull();
    expect(lobbyItems.find((i) => i.seatInfo.seat === "South")?.takeOverKind).toBe(
      "offline_human",
    );

    const observerFinished = makeSnapshot({ phase: "Finished", your_seat: null });
    const finishedItems = buildTakeOverSeatItems(observerFinished);
    expect(finishedItems.find((i) => i.seatInfo.seat === "West")?.takeOverKind).toBeNull();
    expect(finishedItems.find((i) => i.seatInfo.seat === "South")).toMatchObject({
      takeOverKind: "offline_human",
      actionLabel: "接管离线座位",
    });

    expect(
      getOfflineHumanTakeOverSeats(observerFinished.seats, null).map(
        (i) => i.seatInfo.seat,
      ),
    ).toEqual(["South"]);
    expect(
      getOfflineHumanTakeOverSeats(observerFinished.seats, "East"),
    ).toEqual([]);
  });

  it("produces glossary-compliant confirmation copy for AI and offline human takeovers", () => {
    const aiCopy = getTakeOverModalCopy(makeSeat("West"), "ai");
    expect(aiCopy.title).toBe("确认接管 AI 座位？");
    expect(aiCopy.description).toContain("AI 3");
    expect(aiCopy.description).toContain("小局");
    expect(aiCopy.description).not.toMatch(/东座|南座|西座|北座/);
    expect(aiCopy.confirmLabel).toBe("确认接管入座");

    const offlineCopy = getTakeOverModalCopy(
      makeSeat("South", {
        kind: "Human",
        nickname: "南家",
        connected: false,
        ai_difficulty: null,
        ai_style: null,
      }),
      "offline_human",
    );
    expect(offlineCopy.title).toBe("确认接管离线座位？");
    expect(offlineCopy.description).toContain("南家");
    expect(offlineCopy.description).toContain("旧设备");
    expect(offlineCopy.description).not.toMatch(/东座|南座|西座|北座/);
    expect(offlineCopy.confirmLabel).toBe("确认接管该座");

    const namelessOfflineCopy = getTakeOverModalCopy(
      makeSeat("West", {
        kind: "Human",
        nickname: "",
        connected: false,
        ai_difficulty: null,
        ai_style: null,
      }),
      "offline_human",
    );
    expect(namelessOfflineCopy.description).toContain("3号位");
  });

  it("hides offline human takeover for remote clients while keeping mid-match AI seat takeover", () => {
    const remoteObserverInMatch = makeSnapshot({
      your_seat: null,
      host_is_loopback: false,
      client_is_remote: true,
    });

    // Offline pause banner for remote clients must not suggest cross-device takeover.
    const banner = buildOfflinePauseBanner(remoteObserverInMatch);
    expect(banner).toContain("已离线");
    expect(banner).not.toContain("跨设备接管");

    const items = buildTakeOverSeatItems(remoteObserverInMatch);
    // Offline human seat South MUST NOT be take-overable by a remote client.
    expect(items.find((i) => i.seatInfo.seat === "South")).toMatchObject({
      displayName: "南家",
      takeOverKind: null,
      statusLabel: "已离线",
      actionLabel: null,
    });
    // AI seat West MUST remain take-overable mid-match by a remote client.
    expect(items.find((i) => i.seatInfo.seat === "West")).toMatchObject({
      displayName: "AI 3",
      takeOverKind: "ai",
      statusLabel: "AI 对局中",
      actionLabel: "接管入座",
    });

    // Finished summary offline human helper returns empty list for remote clients.
    expect(
      getOfflineHumanTakeOverSeats(remoteObserverInMatch.seats, null, true),
    ).toEqual([]);
  });
});
