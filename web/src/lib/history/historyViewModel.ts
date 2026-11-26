import { matchModeLabel } from "../lobby/lobbyConfig";
import type { HandRecord } from "../protocol/generated/HandRecord";
import type { MatchRecord } from "../protocol/generated/MatchRecord";
import type { PersonaId } from "../protocol/generated/PersonaId";
import type { PersonaMemoryNoteTrigger } from "../protocol/generated/PersonaMemoryNoteTrigger";
import type { PersonaMemoryResponse } from "../protocol/generated/PersonaMemoryResponse";
import type { PersonaMood } from "../protocol/generated/PersonaMood";
import type { PlayerStats } from "../protocol/generated/PlayerStats";
import type { ScoreItem } from "../protocol/generated/ScoreItem";
import type { Seat } from "../protocol/generated/Seat";
import { getPersonaMeta } from "../protocol/personas";
import { formatSeatlessDisplay, seatIndex } from "../protocol/seats";
import { kongKindLabel, winKindLabel } from "../protocol/settlements";
import { formatRoundTitle, getTileLabel } from "../table/tiles";
import { formatScoreDelta } from "../table/viewModel";

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
      nickname: formatSeatlessDisplay(s.seat, match.seats),
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
          ? formatSeatlessDisplay(win.discarder, hand.seats)
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
        displayName: formatSeatlessDisplay(w.seat, hand.seats),
        winKindLabel: winKindLabel(w.win_kind),
        totalFan: w.total_fan,
        fanItems: w.items,
      }));
    }

    const kongTransfers: HandKongTransferSummary[] =
      settlement.kong_transfers.map((kt) => ({
        kindLabel: kongKindLabel(kt.kind),
        actorLabel: formatSeatlessDisplay(kt.actor, hand.seats),
        fromLabel:
          kt.from_seat !== null
            ? formatSeatlessDisplay(kt.from_seat, hand.seats)
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
        nickname: formatSeatlessDisplay(seatRec.seat, hand.seats),
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

export interface PersonaGrudgeNoteItem {
  summary: string;
  timeText: string;
  trigger: PersonaMemoryNoteTrigger;
  triggerLabel: string;
}

export interface PersonaPlayerMemoryItem {
  playerNickname: string;
  handsPlayed: number;
  personaWins: number;
  playerWins: number;
  personaDealtIntoPlayer: number;
  playerDealtIntoPersona: number;
  highestFanSeen: number;
  netScoreDelta: number;
  netScoreDeltaText: string;
  netTone: DeltaTone;
  playerPonCount: number;
  playerKongCount: number;
  meldHabitText: string;
  updatedTimeText: string;
  grudgeNotes: PersonaGrudgeNoteItem[];
}

export interface PersonaMemoryCard {
  personaId: PersonaId;
  personaName: string;
  styleLabel: string;
  tagline: string;
  initialMood: PersonaMood;
  accentHex: string;
  totalNotes: number;
  players: PersonaPlayerMemoryItem[];
}

/**
 * Formats a `PersonaMemoryNoteTrigger` into a concise Chinese tag.
 */
export function personaMemoryTriggerLabel(
  trigger: PersonaMemoryNoteTrigger,
): string {
  switch (trigger) {
    case "HighFan":
      return "大番名场面";
    case "RobKongHu":
      return "抢杠胡";
    case "MultiRon":
      return "一炮多响";
    case "MatchFinished":
      return "终局复盘";
  }
}

/**
 * Maps `PersonaMemoryResponse` into display cards per `牌友角色`, with newest grudge notes first.
 */
export function buildPersonaMemoryCards(
  memory: PersonaMemoryResponse,
): PersonaMemoryCard[] {
  return memory.personas.map((entry) => {
    const meta = getPersonaMeta(entry.persona_id);
    let totalNotes = 0;
    const players: PersonaPlayerMemoryItem[] = entry.players.map((record) => {
      const ledger = record.ledger;
      totalNotes += record.grudge_notes.length;
      const grudgeNotes: PersonaGrudgeNoteItem[] = [...record.grudge_notes]
        .reverse()
        .map((note) => ({
          summary: note.summary,
          timeText: formatHistoryTimestamp(note.created_at_ms),
          trigger: note.trigger,
          triggerLabel: personaMemoryTriggerLabel(note.trigger),
        }));
      return {
        playerNickname: record.player_nickname,
        handsPlayed: ledger.hands_played,
        personaWins: ledger.persona_wins_against_player,
        playerWins: ledger.player_wins_against_persona,
        personaDealtIntoPlayer: ledger.persona_dealt_into_player,
        playerDealtIntoPersona: ledger.player_dealt_into_persona,
        highestFanSeen: ledger.highest_fan_seen,
        netScoreDelta: ledger.net_score_delta,
        netScoreDeltaText: formatScoreDelta(ledger.net_score_delta),
        netTone: deltaTone(ledger.net_score_delta),
        playerPonCount: ledger.player_pon_count,
        playerKongCount: ledger.player_kong_count,
        meldHabitText: `碰 ${ledger.player_pon_count} / 杠 ${ledger.player_kong_count}`,
        updatedTimeText: formatHistoryTimestamp(record.updated_at_ms),
        grudgeNotes,
      };
    });

    return {
      personaId: entry.persona_id,
      personaName: meta.name,
      styleLabel: meta.styleLabel,
      tagline: meta.tagline,
      initialMood: meta.initialMood,
      accentHex: meta.accentHex,
      totalNotes,
      players,
    };
  });
}
