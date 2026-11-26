import { matchModeLabel } from "../lobby/lobbyConfig";
import type { HandRecord } from "../protocol/generated/HandRecord";
import type { MatchRecord } from "../protocol/generated/MatchRecord";
import type { PlayerStats } from "../protocol/generated/PlayerStats";
import type { ScoreItem } from "../protocol/generated/ScoreItem";
import type { Seat } from "../protocol/generated/Seat";
import { seatIndex, seatLabel } from "../protocol/seats";
import { kongKindLabel, winKindLabel } from "../protocol/settlements";
import { formatRoundTitle, getTileLabel } from "../table/tiles";
import { formatScoreDelta, formatSeatDisplayName } from "../table/viewModel";

export type DeltaTone = "pos" | "neg" | "zero";

export interface PlayerSummaryCard {
  nickname: string;
  completedMatches: number;
  settledHands: number;
  abortedHands: number;
  wins: number;
  tsumoWins: number;
  dealIns: number;
  aiAssistedHands: number;
  cumulativeNetScore: number;
  cumulativeNetScoreText: string;
  cumulativeNetTone: DeltaTone;
  averageNetScoreText: string;
  averageHandNetScoreText: string;
}

export interface MatchSeatSummaryItem {
  seat: Seat;
  seatLabel: string;
  nickname: string;
  isAi: boolean;
  assistedByAi: boolean;
  finalScore: number;
  netDelta: number;
  netDeltaText: string;
  netTone: DeltaTone;
}

export interface MatchHistoryRow {
  matchId: string;
  timeText: string;
  modeLabel: string;
  handsPlayed: number;
  assistedByAi: boolean;
  seats: MatchSeatSummaryItem[];
}

export interface HandWinnerSummary {
  seat: Seat;
  displayName: string;
  winKindLabel: string;
  totalFan: number;
  fanItems: ScoreItem[];
}

export interface HandKongTransferSummary {
  kindLabel: string;
  actorLabel: string;
  fromLabel: string | null;
  tileLabel: string;
  actorDeltaText: string;
}

export interface HandSeatDeltaItem {
  seat: Seat;
  seatLabel: string;
  nickname: string;
  isAi: boolean;
  assistedByAi: boolean;
  tookOverAi: boolean;
  delta: number;
  deltaText: string;
  tone: DeltaTone;
}

export interface HandHistoryRow {
  key: string;
  matchId: string;
  handIndex: number;
  timeText: string;
  modeLabel: string;
  roundTitle: string;
  aborted: boolean;
  assistedByAi: boolean;
  outcomeLabel: string;
  winningTileLabel: string | null;
  discarderLabel: string | null;
  winners: HandWinnerSummary[];
  kongTransfers: HandKongTransferSummary[];
  seatDeltas: HandSeatDeltaItem[];
}

function deltaTone(value: number): DeltaTone {
  if (value > 0) {
    return "pos";
  }
  if (value < 0) {
    return "neg";
  }
  return "zero";
}

function pad2(n: number): string {
  return String(n).padStart(2, "0");
}

/**
 * Formats a Unix epoch timestamp in milliseconds for display (`YYYY-MM-DD HH:mm`).
 */
export function formatHistoryTimestamp(timestampMs: number): string {
  const date = new Date(timestampMs);
  if (Number.isNaN(date.getTime())) {
    return "未知时间";
  }
  const yyyy = date.getFullYear();
  const mm = pad2(date.getMonth() + 1);
  const dd = pad2(date.getDate());
  const hh = pad2(date.getHours());
  const mi = pad2(date.getMinutes());
  return `${yyyy}-${mm}-${dd} ${hh}:${mi}`;
}

/**
 * Formats a signed average net score to 1 decimal place (`+8.7`, `0.0`, `-5.0`).
 */
export function formatAverageScore(value: number): string {
  if (!Number.isFinite(value) || Math.abs(value) < 0.05) {
    return "0.0";
  }
  const fixed = value.toFixed(1);
  return value > 0 ? `+${fixed}` : fixed;
}

/**
 * Maps `PlayerStats[]` into formatted summary cards per normalized nickname.
 */
export function buildPlayerSummaryCards(
  players: readonly PlayerStats[],
): PlayerSummaryCard[] {
  return players.map((player) => ({
    nickname: player.nickname,
    completedMatches: player.completed_matches,
    settledHands: player.settled_hands,
    abortedHands: player.aborted_hands,
    wins: player.wins,
    tsumoWins: player.tsumo_wins,
    dealIns: player.deal_ins,
    aiAssistedHands: player.ai_assisted_hands,
    cumulativeNetScore: player.cumulative_net_score,
    cumulativeNetScoreText: formatScoreDelta(player.cumulative_net_score),
    cumulativeNetTone: deltaTone(player.cumulative_net_score),
    averageNetScoreText: formatAverageScore(player.average_net_score),
    averageHandNetScoreText: formatAverageScore(player.average_hand_net_score),
  }));
}

/**
 * Maps recent completed `MatchRecord[]` into display rows.
 */
export function buildMatchHistoryRows(
  matches: readonly MatchRecord[],
): MatchHistoryRow[] {
  return matches.map((match) => ({
    matchId: match.match_id,
    timeText: formatHistoryTimestamp(match.timestamp_ms),
    modeLabel: matchModeLabel(match.mode),
    handsPlayed: match.hands_played,
    assistedByAi: match.seats.some((s) => s.assisted_by_ai),
    seats: match.seats.map((s) => ({
      seat: s.seat,
      seatLabel: seatLabel(s.seat),
      nickname: s.nickname || (s.seat_kind === "Ai" ? "AI" : seatLabel(s.seat)),
      isAi: s.seat_kind === "Ai",
      assistedByAi: s.assisted_by_ai,
      finalScore: s.final_score,
      netDelta: s.net_delta,
      netDeltaText: formatScoreDelta(s.net_delta),
      netTone: deltaTone(s.net_delta),
    })),
  }));
}

/**
 * Maps recent settled `HandRecord[]` into display rows with `番`, `杠分`,
 * `AI 托管`, and `中途解散` metadata.
 */
export function buildHandHistoryRows(
  hands: readonly HandRecord[],
): HandHistoryRow[] {
  return hands.map((hand) => {
    const { settlement } = hand;
    const roundTitle = formatRoundTitle({
      wind_round: settlement.wind_round,
      hand_number: settlement.hand_number,
      dealer_streak: settlement.dealer_streak,
      dealer: settlement.dealer,
      active_seat: null,
    });

    let outcomeLabel = "荒庄";
    let winningTileLabel: string | null = null;
    let discarderLabel: string | null = null;
    let winners: HandWinnerSummary[] = [];

    if (settlement.outcome !== "DrawGame") {
      const win = settlement.outcome.Win;
      winningTileLabel = getTileLabel(win.winning_tile);
      discarderLabel =
        win.discarder !== null
          ? formatSeatDisplayName(win.discarder, hand.seats)
          : null;
      if (win.winners.length > 1) {
        outcomeLabel = "一炮多响";
      } else if (win.winners[0]) {
        outcomeLabel = winKindLabel(win.winners[0].win_kind);
      } else {
        outcomeLabel = "胡牌";
      }
      winners = win.winners.map((w) => ({
        seat: w.seat,
        displayName: formatSeatDisplayName(w.seat, hand.seats),
        winKindLabel: winKindLabel(w.win_kind),
        totalFan: w.total_fan,
        fanItems: w.items,
      }));
    }

    const kongTransfers: HandKongTransferSummary[] =
      settlement.kong_transfers.map((kt) => ({
        kindLabel: kongKindLabel(kt.kind),
        actorLabel: formatSeatDisplayName(kt.actor, hand.seats),
        fromLabel:
          kt.from_seat !== null
            ? formatSeatDisplayName(kt.from_seat, hand.seats)
            : null,
        tileLabel: getTileLabel(kt.tile),
        actorDeltaText: formatScoreDelta(
          kt.score_deltas[seatIndex(kt.actor)] ?? 0,
        ),
      }));

    const seatDeltas: HandSeatDeltaItem[] = hand.seats.map((seatRec, idx) => {
      const delta = settlement.score_deltas[idx] ?? 0;
      return {
        seat: seatRec.seat,
        seatLabel: seatLabel(seatRec.seat),
        nickname:
          seatRec.nickname ||
          (seatRec.seat_kind === "Ai" ? `AI ${idx + 1}` : seatLabel(seatRec.seat)),
        isAi: seatRec.seat_kind === "Ai",
        assistedByAi: seatRec.assisted_by_ai,
        tookOverAi: seatRec.took_over_ai,
        delta,
        deltaText: formatScoreDelta(delta),
        tone: deltaTone(delta),
      };
    });

    return {
      key: `${hand.match_id}:${hand.hand_index}`,
      matchId: hand.match_id,
      handIndex: hand.hand_index,
      timeText: formatHistoryTimestamp(hand.timestamp_ms),
      modeLabel: matchModeLabel(hand.mode),
      roundTitle,
      aborted: hand.aborted,
      assistedByAi: hand.seats.some((s) => s.assisted_by_ai),
      outcomeLabel,
      winningTileLabel,
      discarderLabel,
      winners,
      kongTransfers,
      seatDeltas,
    };
  });
}
