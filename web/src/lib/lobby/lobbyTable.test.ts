import { describe, expect, it } from "vitest";
import type { LobbyConfig } from "../protocol/generated/LobbyConfig";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import { ALL_SEATS } from "../protocol/seats";
import { SEATING_RULE_OPTIONS } from "./lobbyConfig";
import { buildLobbyTableViewModel } from "./lobbyTable";

function makeSeats(
  overrides: Partial<Record<Seat, Partial<SeatInfo>>> = {},
): [SeatInfo, SeatInfo, SeatInfo, SeatInfo] {
  return ALL_SEATS.map((seat) => ({
    seat,
    kind: "Empty",
    nickname: "",
    connected: false,
    ai_assisted: false,
    ai_difficulty: null,
    ai_style: null,
    persona_id: null,
    mood: null,
    voice_pack_id: null,
    ...overrides[seat],
  })) as [SeatInfo, SeatInfo, SeatInfo, SeatInfo];
}

function makeLobbySnapshot(
  overrides: Partial<TableSnapshot> = {},
  configOverrides: Partial<LobbyConfig> = {},
): TableSnapshot {
  return {
    table_id: "table-1",
    phase: "Lobby",
    config: {
      mode: "EastOnly",
      seating: "KeepLobbySeats",
      ai_difficulty: "Strong",
      pace: "Standard",
      ...configOverrides,
    },
    pace: "Standard",
    seats: makeSeats(),
    your_seat: null,
    seat_token: "token-1",
    player_view: null,
    pending_destructive_op: null,
    seq: 0,
    server_version: "0.1.0",
    lan_urls: {
      preferred_ipv4_url: null,
      ipv4_urls: [],
      local_hostname_url: null,
    },
    host_is_loopback: true,
    ...overrides,
  };
}

describe("lobbyTable", () => {
  it("projects unseated lobby slots counter-clockwise as 1号位..4号位 without wind labels", () => {
    const vm = buildLobbyTableViewModel(makeLobbySnapshot({ your_seat: null }));

    expect(vm.isSeated).toBe(false);
    expect(vm.shuffleNotice).toBeNull();
    expect(vm.slots.bottom).toMatchObject({
      slot: "bottom",
      seat: "East",
      label: "1号位",
      isMine: false,
    });
    expect(vm.slots.right).toMatchObject({
      slot: "right",
      seat: "South",
      label: "2号位",
      isMine: false,
    });
    expect(vm.slots.top).toMatchObject({
      slot: "top",
      seat: "West",
      label: "3号位",
      isMine: false,
    });
    expect(vm.slots.left).toMatchObject({
      slot: "left",
      seat: "North",
      label: "4号位",
      isMine: false,
    });
  });

  it("rotates the ring table when seated or moving seats so viewer is always bottom 自家", () => {
    const seats = makeSeats({
      South: { kind: "Human", nickname: "小红", connected: true },
      North: { kind: "Human", nickname: "小明", connected: true },
    });

    const southVm = buildLobbyTableViewModel(
      makeLobbySnapshot({ your_seat: "South", seats }),
    );
    expect(southVm.isSeated).toBe(true);
    expect(southVm.slots.bottom).toMatchObject({
      seat: "South",
      label: "自家",
      isMine: true,
    });
    expect(southVm.slots.right).toMatchObject({
      seat: "West",
      label: "下家",
      isMine: false,
    });
    expect(southVm.slots.top).toMatchObject({
      seat: "North",
      label: "对门",
      isMine: false,
    });
    expect(southVm.slots.left).toMatchObject({
      seat: "East",
      label: "上家",
      isMine: false,
    });

    // Moving to West rotates perspective counter-clockwise immediately.
    const movedSeats = makeSeats({
      West: { kind: "Human", nickname: "小红", connected: true },
      North: { kind: "Human", nickname: "小明", connected: true },
    });
    const westVm = buildLobbyTableViewModel(
      makeLobbySnapshot({ your_seat: "West", seats: movedSeats }),
    );
    expect(westVm.slots.bottom).toMatchObject({
      seat: "West",
      label: "自家",
      isMine: true,
    });
    expect(westVm.slots.right).toMatchObject({
      seat: "North",
      label: "下家",
    });
    expect(westVm.slots.top).toMatchObject({
      seat: "East",
      label: "对门",
    });
    expect(westVm.slots.left).toMatchObject({
      seat: "South",
      label: "上家",
    });
  });

  it("shows explicit shuffle notice only when seating rule is Random", () => {
    const keepVm = buildLobbyTableViewModel(
      makeLobbySnapshot({}, { seating: "KeepLobbySeats" }),
    );
    const randomVm = buildLobbyTableViewModel(
      makeLobbySnapshot({}, { seating: "Random" }),
    );
    const oppositeVm = buildLobbyTableViewModel(
      makeLobbySnapshot({}, { seating: "FixedOpposite" }),
    );
    const adjacentVm = buildLobbyTableViewModel(
      makeLobbySnapshot({}, { seating: "FixedAdjacent" }),
    );

    expect(keepVm.shuffleNotice).toBeNull();
    expect(oppositeVm.shuffleNotice).toBeNull();
    expect(adjacentVm.shuffleNotice).toBeNull();
    expect(randomVm.shuffleNotice).toContain("随机定座");
  });

  it("exposes wind-free SEATING_RULE_OPTIONS with KeepLobbySeats first as default", () => {
    expect(SEATING_RULE_OPTIONS).toEqual([
      { value: "KeepLobbySeats", label: "按大厅所选座次（默认）" },
      { value: "Random", label: "随机定座" },
      { value: "FixedOpposite", label: "双人固定对门" },
      { value: "FixedAdjacent", label: "双人固定上下家" },
    ]);
    for (const option of SEATING_RULE_OPTIONS) {
      expect(option.label).not.toMatch(/[东南西北]/);
    }
  });

  it("projects assigned persona metadata and marks personas taken by other seats as unavailable", () => {
    const seats = makeSeats({
      East: { kind: "Human", nickname: "小红", connected: true },
      South: { kind: "Empty", persona_id: "QuickHothead" },
      West: { kind: "Empty", persona_id: "TeaSarcasm" },
      North: { kind: "Empty", persona_id: null },
    });

    const vm = buildLobbyTableViewModel(
      makeLobbySnapshot({ your_seat: "East", seats }),
    );

    expect(vm.slots.bottom.personaMeta).toBeNull();
    expect(vm.slots.bottom.personaOptions).toEqual([]);

    expect(vm.slots.right.personaMeta).toMatchObject({
      id: "QuickHothead",
      name: "急眼快攻哥",
      styleLabel: "快攻",
    });
    expect(
      vm.slots.right.personaOptions.find((o) => o.id === "QuickHothead")
        ?.takenByOther,
    ).toBe(false);
    expect(
      vm.slots.right.personaOptions.find((o) => o.id === "TeaSarcasm")
        ?.takenByOther,
    ).toBe(true);

    expect(vm.slots.left.personaMeta).toBeNull();
    expect(
      vm.slots.left.personaOptions
        .filter((o) => o.takenByOther)
        .map((o) => o.id),
    ).toEqual(["QuickHothead", "TeaSarcasm"]);
  });
});
