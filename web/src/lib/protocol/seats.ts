import type { Seat } from "./generated/Seat";

export const ALL_SEATS: [Seat, Seat, Seat, Seat] = [
  "East",
  "South",
  "West",
  "North",
];

/**
 * Returns the canonical `0..3` seat index (`East=0`, `South=1`, `West=2`, `North=3`).
 */
export function seatIndex(seat: Seat): number {
  switch (seat) {
    case "East":
      return 0;
    case "South":
      return 1;
    case "West":
      return 2;
    case "North":
      return 3;
  }
}

/**
 * Returns Chinese seat label (`东座`/`南座`/`西座`/`北座`).
 */
export function seatLabel(seat: Seat): string {
  switch (seat) {
    case "East":
      return "东座";
    case "South":
      return "南座";
    case "West":
      return "西座";
    case "North":
      return "北座";
  }
}

/**
 * Returns single-character wind label for a seat (`东`/`南`/`西`/`北`).
 */
export function seatShortWind(seat: Seat): string {
  switch (seat) {
    case "East":
      return "东";
    case "South":
      return "南";
    case "West":
      return "西";
    case "North":
      return "北";
  }
}
