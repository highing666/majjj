import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import {
  getRelativeSeats,
  seatIndex,
  seatNumberLabel,
  seatRelativeRole,
  type RelativeSlot,
} from "../protocol/seats";

export interface LobbyTableSlot {
  slot: RelativeSlot;
  seat: Seat;
  seatInfo: SeatInfo;
  label: string;
  isMine: boolean;
}

export interface LobbyTableViewModel {
  slots: {
    top: LobbyTableSlot;
    left: LobbyTableSlot;
    right: LobbyTableSlot;
    bottom: LobbyTableSlot;
  };
  isSeated: boolean;
  shuffleNotice: string | null;
}

/**
 * Projects a lobby `TableSnapshot` into a top-down counter-clockwise 4-sided ring-table
 * view model, rotating to keep `your_seat` at `bottom` (`自家`) when seated.
 */
export function buildLobbyTableViewModel(
  snapshot: TableSnapshot,
): LobbyTableViewModel {
  const viewer = snapshot.your_seat;
  const isSeated = viewer !== null;
  const mapping = getRelativeSeats(viewer ?? "East");

  const makeSlot = (slot: RelativeSlot): LobbyTableSlot => {
    const seat = mapping[slot];
    const seatInfo = snapshot.seats[seatIndex(seat)];
    const label =
      viewer !== null ? seatRelativeRole(viewer, seat) : seatNumberLabel(seat);
    return {
      slot,
      seat,
      seatInfo,
      label,
      isMine: viewer === seat,
    };
  };

  const shuffleNotice =
    snapshot.config.seating === "Random"
      ? "当前已选「随机定座」，开始对局后将重新随机洗座。"
      : null;

  return {
    slots: {
      top: makeSlot("top"),
      left: makeSlot("left"),
      right: makeSlot("right"),
      bottom: makeSlot("bottom"),
    },
    isSeated,
    shuffleNotice,
  };
}
