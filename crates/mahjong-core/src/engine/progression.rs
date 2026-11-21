//! Multi-hand progression, `BetweenHands` confirmation, and match-completion logic.

use std::collections::VecDeque;

use crate::engine::{Match, RecordedEvent, SeatState, TableEventKind, TablePhase, TableState};
use crate::protocol::{
    HandOutcome, HandSettlement, MatchError, MatchMode, MatchSummary, PlayerAction, PlayerHandView,
    RevealedHand, TurnSource,
};
use crate::ruleset::{
    HandProgression, HandProgressionInput, Ruleset, SettleHandInput, WinSettlementDetails,
};
use crate::seat::Seat;
use crate::tile::Tile;
use crate::wall::{validate_wall, WallProvider};

impl<R: Ruleset, W: WallProvider> Match<R, W> {
    /// Settles the active hand, emits `HandSettled`, and transitions to either
    /// `BetweenHands` or directly to `MatchFinished` per `Ruleset::advance_hand`.
    pub(crate) fn settle_hand_with_details(
        &mut self,
        win_details: Option<WinSettlementDetails>,
        out: &mut Vec<RecordedEvent>,
    ) {
        let revealed_hands = std::array::from_fn(|idx| {
            let seat = Seat::from_index(idx).expect("array length is 4");
            let s = &self.state.seats[idx];
            RevealedHand {
                seat,
                closed_tiles: s.closed_tiles.clone(),
                drawn_tile: s.drawn_tile,
                melds: s.melds.clone(),
            }
        });

        let settlement = self.ruleset.settle_hand(SettleHandInput {
            wind_round: self.state.wind_round,
            hand_number: self.state.hand_number,
            dealer_streak: self.state.dealer_streak,
            dealer: self.state.dealer,
            current_scores: self.state.scores,
            kong_transfers: self.state.kong_transfers.clone(),
            revealed_hands,
            win_details,
        });

        self.state.scores = settlement.new_scores;
        self.state.hands_played += 1;
        Self::push_event(
            &mut self.state,
            out,
            TableEventKind::HandSettled {
                settlement: settlement.clone(),
            },
        );

        match self.next_progression(&settlement.outcome) {
            HandProgression::NextHand { .. } => {
                self.state.phase = TablePhase::BetweenHands {
                    last_settlement: settlement,
                    ready_seats: [false; 4],
                };
            }
            HandProgression::FinishMatch => {
                self.finish_match(Some(settlement), out);
            }
        }
    }

    pub(super) fn apply_between_hands_action(
        &mut self,
        seat: Seat,
        action: PlayerAction,
    ) -> Result<Vec<RecordedEvent>, MatchError> {
        let (last_settlement, ready_seats) = match &self.state.phase {
            TablePhase::BetweenHands {
                last_settlement,
                ready_seats,
            } => (last_settlement.clone(), *ready_seats),
            _ => return Err(MatchError::WrongPhase { seat, action }),
        };

        let mut out = Vec::new();
        match action {
            PlayerAction::ConfirmNextHand => {
                self.confirm_next_hand(seat, last_settlement, ready_seats, &mut out)?;
                Ok(out)
            }
            PlayerAction::EndMatch => {
                if self.state.mode != MatchMode::FreeTable {
                    return Err(MatchError::IllegalAction { seat, action });
                }
                self.finish_match(Some(last_settlement), &mut out);
                Ok(out)
            }
            _ => Err(MatchError::WrongPhase { seat, action }),
        }
    }

    fn confirm_next_hand(
        &mut self,
        seat: Seat,
        last_settlement: HandSettlement,
        ready_seats: [bool; 4],
        out: &mut Vec<RecordedEvent>,
    ) -> Result<(), MatchError> {
        if ready_seats[seat.index()] {
            return Err(MatchError::AlreadyConfirmed { seat });
        }

        let all_ready = (0..4).all(|idx| idx == seat.index() || ready_seats[idx]);
        if !all_ready {
            if let TablePhase::BetweenHands { ready_seats, .. } = &mut self.state.phase {
                ready_seats[seat.index()] = true;
            }
            Self::push_event(
                &mut self.state,
                out,
                TableEventKind::NextHandConfirmed { seat },
            );
            return Ok(());
        }

        let HandProgression::NextHand {
            wind_round,
            hand_number,
            dealer_streak,
            dealer,
        } = self.next_progression(&last_settlement.outcome)
        else {
            Self::push_event(
                &mut self.state,
                out,
                TableEventKind::NextHandConfirmed { seat },
            );
            self.finish_match(Some(last_settlement), out);
            return Ok(());
        };

        // Validate the upcoming wall before mutating table phase or emitting NextHandConfirmed.
        let deck = self.ruleset.deck();
        let wall_tiles = self.wall_provider.next_wall(&deck);
        validate_wall(&wall_tiles, &deck)?;

        Self::push_event(
            &mut self.state,
            out,
            TableEventKind::NextHandConfirmed { seat },
        );
        self.state.wind_round = wind_round;
        self.state.hand_number = hand_number;
        self.state.dealer_streak = dealer_streak;
        self.state.dealer = dealer;
        Self::deal_hand_into_state(&mut self.state, out, wall_tiles);
        Ok(())
    }

    fn finish_match(
        &mut self,
        last_settlement: Option<HandSettlement>,
        out: &mut Vec<RecordedEvent>,
    ) {
        let net_deltas =
            std::array::from_fn(|idx| self.state.scores[idx] - self.state.initial_scores[idx]);
        let summary = MatchSummary {
            mode: self.state.mode,
            hands_played: self.state.hands_played,
            initial_scores: self.state.initial_scores,
            final_scores: self.state.scores,
            net_deltas,
        };
        Self::push_event(
            &mut self.state,
            out,
            TableEventKind::MatchFinished { summary },
        );
        self.state.phase = TablePhase::MatchFinished {
            last_settlement,
            summary,
        };
    }

    fn next_progression(&self, outcome: &HandOutcome) -> HandProgression {
        self.ruleset.advance_hand(HandProgressionInput {
            mode: self.state.mode,
            wind_round: self.state.wind_round,
            hand_number: self.state.hand_number,
            dealer_streak: self.state.dealer_streak,
            dealer: self.state.dealer,
            outcome,
        })
    }

    /// Deals a validated 136-tile wall into `state`, resets per-hand seat fields,
    /// enters `TurnAction` for `state.dealer`, and emits `HandStarted`.
    pub(super) fn deal_hand_into_state(
        state: &mut TableState,
        out: &mut Vec<RecordedEvent>,
        wall_tiles: Vec<Tile>,
    ) {
        let mut cursor = 0usize;
        let dealer = state.dealer;
        let deal_order = [
            (dealer, 14usize),
            (dealer.next(), 13usize),
            (dealer.next().next(), 13usize),
            (dealer.next().next().next(), 13usize),
        ];

        for (seat, count) in deal_order {
            let mut closed = wall_tiles[cursor..cursor + count].to_vec();
            closed.sort_unstable();
            cursor += count;
            state.seats[seat.index()] = SeatState {
                closed_tiles: closed,
                drawn_tile: None,
                river: Vec::new(),
                melds: Vec::new(),
                last_draw_or_meld_seq: 0,
                passed_hu_lock: false,
                passed_pon_lock: Vec::new(),
            };
        }

        state.wall = VecDeque::from(wall_tiles[cursor..].to_vec());
        state.kong_transfers.clear();
        state.phase = TablePhase::TurnAction {
            actor: dealer,
            source: TurnSource::InitialDealer,
        };

        let hands = std::array::from_fn(|idx| PlayerHandView {
            closed_tiles: state.seats[idx].closed_tiles.clone(),
            drawn_tile: None,
        });

        Self::push_event(
            state,
            out,
            TableEventKind::HandStarted {
                wind_round: state.wind_round,
                hand_number: state.hand_number,
                dealer_streak: state.dealer_streak,
                dealer,
                hands,
                wall_remaining: state.wall_remaining(),
                scores: state.scores,
            },
        );
    }
}

/// Returns legal `BetweenHands` actions for `seat` consistent with `apply_between_hands_action`.
pub(super) fn legal_between_hands_actions(
    state: &TableState,
    ready_seats: [bool; 4],
    seat: Seat,
) -> Vec<PlayerAction> {
    let mut actions = Vec::new();
    if !ready_seats[seat.index()] {
        actions.push(PlayerAction::ConfirmNextHand);
    }
    if state.mode == MatchMode::FreeTable {
        actions.push(PlayerAction::EndMatch);
    }
    actions
}
