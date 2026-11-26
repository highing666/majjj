import type { PersonaId } from "../protocol/generated/PersonaId";
import type { Seat } from "../protocol/generated/Seat";
import type { SeatInfo } from "../protocol/generated/SeatInfo";
import type { TableSnapshot } from "../protocol/generated/TableSnapshot";
import {
  getPersonaMeta,
  PERSONA_LIST,
  type PersonaMeta,
} from "../protocol/personas";
import {
  getRelativeSeats,
  seatIndex,
  seatNumberLabel,
  seatRelativeRole,
  type RelativeSlot,
} from "../protocol/seats";

export interface LobbyPersonaOption {
  readonly id: PersonaId;
  readonly name: string;
  readonly styleLabel: string;
  readonly tagline: string;
  readonly takenByOther: boolean;
}

export interface LobbyTableSlot {
  slot: RelativeSlot;
  seat: Seat;
  seatInfo: SeatInfo;
  label: string;
  isMine: boolean;
  personaMeta: PersonaMeta | null;
  personaOptions: readonly LobbyPersonaOption[];
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
 * Builds the 5 built-in persona picker options for `seat`, marking personas already
 * assigned to another non-human seat as `takenByOther: true`.
 */
export function buildLobbyPersonaOptions(
  seats: readonly [SeatInfo, SeatInfo, SeatInfo, SeatInfo],
  seat: Seat,
): LobbyPersonaOption[] {
  const takenElsewhere = new Set<PersonaId>();
  for (const info of seats) {
    if (info.seat !== seat && info.kind !== "Human" && info.persona_id !== null) {
      takenElsewhere.add(info.persona_id);
    }
  }
  return PERSONA_LIST.map((meta) => ({
    id: meta.id,
    name: meta.name,
    styleLabel: meta.styleLabel,
    tagline: meta.tagline,
    takenByOther: takenElsewhere.has(meta.id),
  }));
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
    const personaMeta =
      seatInfo.kind !== "Human" && seatInfo.persona_id !== null
        ? getPersonaMeta(seatInfo.persona_id)
        : null;
    const personaOptions =
      seatInfo.kind !== "Human"
        ? buildLobbyPersonaOptions(snapshot.seats, seat)
        : [];
    return {
      slot,
      seat,
      seatInfo,
      label,
      isMine: viewer === seat,
      personaMeta,
      personaOptions,
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
