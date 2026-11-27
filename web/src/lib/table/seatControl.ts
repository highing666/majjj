import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import {
  formatRelativeSeatDisplay,
  formatRelativeSeatParen,
  formatSeatEntrySeatless,
  formatSeatlessDisplay,
  seatIndex,
} from "../protocol/seats";

export type TakeOverKind = "ai" | "offline_human";

export interface SeatControlItem {
  seat: Seat;
  displayName: string;
  aiAssisted: boolean;
  nextEnabled: boolean;
  actionLabel: string;
}

export interface SelfAiAssistControl {
  seat: Seat;
  aiAssisted: boolean;
  nextEnabled: boolean;
  actionLabel: string;
  statusLabel: string;
  hint: string;
  shouldCloseSettingsOnToggle: boolean;
}

export interface TakeOverSeatItem {
  seatInfo: SeatInfo;
  displayName: string;
  takeOverKind: TakeOverKind | null;
  statusLabel: string;
  actionLabel: string | null;
}

export interface TakeOverModalCopy {
  title: string;
  description: string;
  confirmLabel: string;
}

/**
 * Returns an offline/pause notice banner for `InMatch` when any human seat is disconnected,
 * distinguishing paused offline seats from seats temporarily assisted by AI.
 */
export function buildOfflinePauseBanner(
  snapshot: TableSnapshot,
): string | null {
  if (snapshot.phase !== "InMatch") {
    return null;
  }
  const offlineHumans = snapshot.seats.filter(
    (s) => s.kind === "Human" && !s.connected,
  );
  if (offlineHumans.length === 0) {
    return null;
  }

  const formatPaused = (s: SeatInfo): string =>
    formatRelativeSeatParen(s.seat, snapshot.your_seat, snapshot.seats);

  const pausedNames = offlineHumans
    .filter((s) => !s.ai_assisted)
    .map(formatPaused);
  const assistedNames = offlineHumans
    .filter((s) => s.ai_assisted)
    .map(formatPaused);

  if (pausedNames.length > 0 && assistedNames.length === 0) {
    if (snapshot.client_is_remote) {
      return `${pausedNames.join("、")} 已离线，牌桌已无限期暂停等待；请等待原玩家重连或由在座玩家暂交 AI 托管。`;
    }
    return `${pausedNames.join("、")} 已离线，牌桌已无限期暂停等待；可手动开启「暂由 AI 托管」或跨设备接管该座。`;
  }
  if (pausedNames.length === 0 && assistedNames.length > 0) {
    return `${assistedNames.join("、")} 当前离线并暂由 AI 托管中；原玩家重连后可一键收回控制权。`;
  }
  return `${pausedNames.join("、")} 已离线暂停等待；${assistedNames.join("、")} 离线且暂由 AI 托管中。`;
}

/**
 * Returns `true` when the local seated player is currently under temporary AI assist
 * and should see the prominent one-click reclaim button.
 */
export function shouldShowReclaimControl(snapshot: TableSnapshot): boolean {
  if (snapshot.phase !== "InMatch" || snapshot.your_seat === null) {
    return false;
  }
  return snapshot.seats[seatIndex(snapshot.your_seat)].ai_assisted;
}

/**
 * Builds the self AI assist control rendered inside Settings for a seated human during `InMatch`.
 * Returns `null` outside `InMatch` or for unseated observers.
 */
export function buildSelfAiAssistControl(
  snapshot: TableSnapshot | null,
): SelfAiAssistControl | null {
  if (
    snapshot === null ||
    snapshot.phase !== "InMatch" ||
    snapshot.your_seat === null
  ) {
    return null;
  }
  const seatInfo = snapshot.seats[seatIndex(snapshot.your_seat)];
  if (seatInfo.kind !== "Human") {
    return null;
  }
  if (seatInfo.ai_assisted) {
    return {
      seat: snapshot.your_seat,
      aiAssisted: true,
      nextEnabled: false,
      actionLabel: "收回控制权",
      statusLabel: "AI 托管中",
      hint: "当前本座正由 AI 暂时代打；点击右侧按钮立即取消 AI 托管并收回控制权。",
      shouldCloseSettingsOnToggle: false,
    };
  }
  return {
    seat: snapshot.your_seat,
    aiAssisted: false,
    nextEnabled: true,
    actionLabel: "本座暂交 AI 托管",
    statusLabel: "未开启",
    hint: "开启后由 AI 暂时代打本座，可随时在牌桌顶部一键收回控制权；本小局战绩仍计入本人并标记为 AI 托管小局。",
    shouldCloseSettingsOnToggle: true,
  };
}

/**
 * Builds top-bar AI assist controls for offline other human seats during `InMatch`.
 * Own-seat self-assist lives in Settings (and reclaims via `shouldShowReclaimControl`);
 * online other seats display their AI-assist badge directly on their nameplate.
 */
export function buildSeatControlItems(
  snapshot: TableSnapshot,
): SeatControlItem[] {
  if (snapshot.phase !== "InMatch" || snapshot.your_seat === null) {
    return [];
  }
  const yourSeat = snapshot.your_seat;
  const items: SeatControlItem[] = [];

  for (const seatInfo of snapshot.seats) {
    if (seatInfo.kind !== "Human") {
      continue;
    }
    const isSelf = seatInfo.seat === yourSeat;
    const isOffline = !seatInfo.connected;
    if (isSelf || !isOffline) {
      continue;
    }

    const nextEnabled = !seatInfo.ai_assisted;
    const actionLabel = seatInfo.ai_assisted ? "取消托管" : "暂由 AI 托管";

    items.push({
      seat: seatInfo.seat,
      displayName: formatRelativeSeatDisplay(
        seatInfo.seat,
        yourSeat,
        snapshot.seats,
      ),
      aiAssisted: seatInfo.ai_assisted,
      nextEnabled,
      actionLabel,
    });
  }

  return items;
}

/**
 * Builds seat cards and takeover eligibility for an unseated client (`your_seat === null`).
 */
export function buildTakeOverSeatItems(
  snapshot: TableSnapshot,
): TakeOverSeatItem[] {
  if (snapshot.your_seat !== null) {
    return [];
  }

  return snapshot.seats.map((seatInfo) =>
    toTakeOverSeatItem(
      seatInfo,
      snapshot.seats,
      snapshot.phase,
      snapshot.client_is_remote,
    ),
  );
}

/**
 * Returns offline human seats eligible for cross-device takeover when `yourSeat === null`
 * (used in `Finished` summary view where AI seat takeover does not apply).
 * Remote clients are strictly forbidden from taking over any offline human seat.
 */
export function getOfflineHumanTakeOverSeats(
  seats: readonly [SeatInfo, SeatInfo, SeatInfo, SeatInfo],
  yourSeat: Seat | null,
  clientIsRemote = false,
): TakeOverSeatItem[] {
  if (yourSeat !== null || clientIsRemote) {
    return [];
  }
  return seats
    .filter((seatInfo) => seatInfo.kind === "Human" && !seatInfo.connected)
    .map((seatInfo) => toTakeOverSeatItem(seatInfo, seats, "Finished", false));
}

function toTakeOverSeatItem(
  seatInfo: SeatInfo,
  seats: readonly [SeatInfo, SeatInfo, SeatInfo, SeatInfo],
  phase: TableSnapshot["phase"],
  clientIsRemote = false,
): TakeOverSeatItem {
  const displayName = formatSeatlessDisplay(seatInfo.seat, seats);
  if (seatInfo.kind === "Human") {
    if (!seatInfo.connected) {
      return {
        seatInfo,
        displayName,
        takeOverKind: clientIsRemote ? null : "offline_human",
        statusLabel: seatInfo.ai_assisted ? "已离线 · AI 托管中" : "已离线",
        actionLabel: clientIsRemote ? null : "接管离线座位",
      };
    }
    return {
      seatInfo,
      displayName,
      takeOverKind: null,
      statusLabel: seatInfo.ai_assisted ? "在线 · AI 托管中" : "在线人类",
      actionLabel: null,
    };
  }

  if (seatInfo.kind === "Ai") {
    const canTakeOver = phase === "InMatch";
    return {
      seatInfo,
      displayName,
      takeOverKind: canTakeOver ? "ai" : null,
      statusLabel: canTakeOver ? "AI 对局中" : "AI 座位",
      actionLabel: canTakeOver ? "接管入座" : null,
    };
  }

  return {
    seatInfo,
    displayName,
    takeOverKind: null,
    statusLabel: "空座",
    actionLabel: null,
  };
}

/**
 * Builds confirmation modal copy for taking over either an AI seat or an offline human seat.
 */
export function getTakeOverModalCopy(
  seatInfo: SeatInfo,
  kind: TakeOverKind,
): TakeOverModalCopy {
  const seatText = formatSeatEntrySeatless(seatInfo.seat, seatInfo);
  if (kind === "ai") {
    return {
      title: "确认接管 AI 座位？",
      description: `确认接管 ${seatText} 后将立即转为本机人类操作，丢弃该座未完成的 AI 思考，并从本小局起归属到当前玩家名下。`,
      confirmLabel: "确认接管入座",
    };
  }
  return {
    title: "确认接管离线座位？",
    description: `确认跨设备接管 ${seatText} 后，本机将立即获得控制权并结束托管状态，同时原旧设备的会话凭证会立即作废。`,
    confirmLabel: "确认接管该座",
  };
}
