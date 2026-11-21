//! Pure deterministic 5-state table engine (`Match<R, W>`).

mod progression;
mod reactions;

use core::fmt;
use std::collections::VecDeque;

use crate::projection;
use crate::protocol::{
    DiscardEntry, HandSettlement, KongKind, KongTransfer, MatchError, MatchMode, MatchSummary,
    PlayerAction, PlayerEvent, PlayerHandView, PlayerView, RevealedMeld, RuleProfile, TurnSource,
    WinKind,
};
use crate::ruleset::{Ruleset, TurnContext, WinCandidate, WinEvalInput, WinSettlementDetails};
use crate::seat::{Seat, Wind};
use crate::tile::Tile;
use crate::wall::{validate_wall, WallProvider};
use reactions::{DiscardReactionWindow, RobKongWindow};

/// Opaque wrapper over an internal `TableEvent`, exposing only its sequence number.
///
/// Use [`crate::project_event`] or [`Match::project_event`] to project a seat-safe [`PlayerEvent`].
#[derive(Clone)]
pub struct RecordedEvent(pub(crate) TableEvent);

impl RecordedEvent {
    /// Returns the monotonically increasing sequence number of this event.
    #[must_use]
    pub fn seq(&self) -> u64 {
        self.0.seq
    }
}

impl fmt::Debug for RecordedEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordedEvent")
            .field("seq", &self.0.seq)
            .finish()
    }
}

/// Strictly crate-private internal state for one seat during an active hand.
#[derive(Debug, Clone)]
pub(crate) struct SeatState {
    pub(crate) closed_tiles: Vec<Tile>,
    pub(crate) drawn_tile: Option<Tile>,
    pub(crate) river: Vec<DiscardEntry>,
    pub(crate) melds: Vec<RevealedMeld>,
    pub(crate) last_draw_or_meld_seq: u64,
    pub(crate) passed_hu_lock: bool,
    pub(crate) passed_pon_lock: Vec<Tile>,
}

impl SeatState {
    fn empty() -> Self {
        Self {
            closed_tiles: Vec::new(),
            drawn_tile: None,
            river: Vec::new(),
            melds: Vec::new(),
            last_draw_or_meld_seq: 0,
            passed_hu_lock: false,
            passed_pon_lock: Vec::new(),
        }
    }

    pub(crate) fn hand_count(&self) -> u8 {
        (self.closed_tiles.len() + usize::from(self.drawn_tile.is_some())) as u8
    }

    pub(crate) fn consume_tiles_for_kong(&mut self, tile: Tile, count: usize) {
        let from_closed = if self.drawn_tile == Some(tile) {
            self.drawn_tile = None;
            count - 1
        } else {
            count
        };
        for _ in 0..from_closed {
            let pos = self
                .closed_tiles
                .iter()
                .position(|&t| t == tile)
                .expect("legal kong verified matching tiles in closed_tiles");
            self.closed_tiles.remove(pos);
        }
        if let Some(drawn) = self.drawn_tile.take() {
            self.closed_tiles.push(drawn);
            self.closed_tiles.sort_unstable();
        }
    }
}

/// Strictly crate-private 5-state internal FSM.
#[derive(Debug, Clone)]
pub(crate) enum TablePhase {
    TurnAction {
        actor: Seat,
        source: TurnSource,
    },
    AwaitingDiscardReactions(DiscardReactionWindow),
    AwaitingRobKong(RobKongWindow),
    BetweenHands {
        last_settlement: HandSettlement,
        ready_seats: [bool; 4],
    },
    MatchFinished {
        last_settlement: Option<HandSettlement>,
        summary: MatchSummary,
    },
}

/// Strictly crate-private authoritative table state.
#[derive(Debug, Clone)]
pub(crate) struct TableState {
    pub(crate) mode: MatchMode,
    pub(crate) wind_round: Wind,
    pub(crate) hand_number: u8,
    pub(crate) dealer_streak: u32,
    pub(crate) dealer: Seat,
    pub(crate) hands_played: u32,
    pub(crate) initial_scores: [i32; 4],
    pub(crate) scores: [i32; 4],
    pub(crate) seats: [SeatState; 4],
    pub(crate) wall: VecDeque<Tile>,
    pub(crate) dead_wall_size: u8,
    pub(crate) kong_transfers: Vec<KongTransfer>,
    pub(crate) phase: TablePhase,
    pub(crate) seq: u64,
}

impl TableState {
    pub(crate) fn wall_remaining(&self) -> u8 {
        self.wall
            .len()
            .saturating_sub(usize::from(self.dead_wall_size)) as u8
    }
}

/// Strictly crate-private authoritative event record prior to seat projection.
#[derive(Debug, Clone)]
pub(crate) struct TableEvent {
    pub(crate) seq: u64,
    pub(crate) kind: TableEventKind,
}

/// Strictly crate-private authoritative event payload variants.
#[derive(Debug, Clone)]
pub(crate) enum TableEventKind {
    MatchStarted {
        mode: MatchMode,
        initial_dealer: Seat,
        initial_scores: [i32; 4],
        rule_profile: RuleProfile,
    },
    HandStarted {
        wind_round: Wind,
        hand_number: u8,
        dealer_streak: u32,
        dealer: Seat,
        hands: [PlayerHandView; 4],
        wall_remaining: u8,
        scores: [i32; 4],
    },
    TileDrawn {
        seat: Seat,
        tile: Tile,
        is_replacement: bool,
        wall_remaining: u8,
    },
    TileDiscarded {
        seat: Seat,
        tile: Tile,
        is_tsumogiri: bool,
    },
    AddedKongAttempted {
        seat: Seat,
        tile: Tile,
    },
    MeldDeclared {
        seat: Seat,
        meld: RevealedMeld,
        kong_score_deltas: Option<[i32; 4]>,
    },
    HandSettled {
        settlement: HandSettlement,
    },
    NextHandConfirmed {
        seat: Seat,
    },
    MatchFinished {
        summary: MatchSummary,
    },
}

/// Pure synchronous 4-player mahjong table state machine.
pub struct Match<R: Ruleset, W: WallProvider> {
    pub(crate) ruleset: R,
    wall_provider: W,
    pub(crate) state: TableState,
    events: Vec<RecordedEvent>,
}

impl<R: Ruleset, W: WallProvider> fmt::Debug for Match<R, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Match")
            .field("mode", &self.state.mode)
            .field("wind_round", &self.state.wind_round)
            .field("hand_number", &self.state.hand_number)
            .field("dealer", &self.state.dealer)
            .field("seq", &self.state.seq)
            .finish()
    }
}

impl<R: Ruleset, W: WallProvider> Match<R, W> {
    /// Initializes a new match, validates the first wall, deals hand 1, and records
    /// `MatchStarted` (`seq = 1`) and `HandStarted` (`seq = 2`).
    ///
    /// # Errors
    ///
    /// Returns `MatchError::InvalidWall` if `wall_provider` does not produce a valid
    /// 136-tile multiset matching `ruleset.deck()`.
    pub fn new(mode: MatchMode, ruleset: R, mut wall_provider: W) -> Result<Self, MatchError> {
        let profile = ruleset.profile();
        let deck = ruleset.deck();
        let initial_dealer = wall_provider.initial_dealer();
        let wall_tiles = wall_provider.next_wall(&deck);
        validate_wall(&wall_tiles, &deck)?;

        let initial_scores = match mode {
            MatchMode::EastOnly => profile.starting_scores_east_only,
            MatchMode::FreeTable => profile.starting_scores_free_table,
        };

        let mut state = TableState {
            mode,
            wind_round: Wind::East,
            hand_number: 1,
            dealer_streak: 0,
            dealer: initial_dealer,
            hands_played: 0,
            initial_scores,
            scores: initial_scores,
            seats: std::array::from_fn(|_| SeatState::empty()),
            wall: VecDeque::new(),
            dead_wall_size: profile.dead_wall_size,
            kong_transfers: Vec::new(),
            phase: TablePhase::TurnAction {
                actor: initial_dealer,
                source: TurnSource::InitialDealer,
            },
            seq: 0,
        };

        let mut events = Vec::new();
        Self::push_event(
            &mut state,
            &mut events,
            TableEventKind::MatchStarted {
                mode,
                initial_dealer,
                initial_scores,
                rule_profile: profile,
            },
        );
        Self::deal_hand_into_state(&mut state, &mut events, wall_tiles);

        Ok(Self {
            ruleset,
            wall_provider,
            state,
            events,
        })
    }

    /// Returns all recorded opaque events emitted since match creation.
    #[must_use]
    pub fn events(&self) -> &[RecordedEvent] {
        &self.events
    }

    /// Returns the latest monotonically increasing event sequence number.
    #[must_use]
    pub fn seq(&self) -> u64 {
        self.state.seq
    }

    /// Applies an authoritative player action and returns only the events emitted by this step.
    ///
    /// # Errors
    ///
    /// Returns a `MatchError` without mutating state if the action is illegal, out of turn,
    /// invalid for the current phase, or if dealing the next hand yields an invalid wall.
    pub fn apply_action(
        &mut self,
        seat: Seat,
        action: PlayerAction,
    ) -> Result<Vec<RecordedEvent>, MatchError> {
        if matches!(
            (&self.state.phase, action),
            (
                TablePhase::TurnAction { .. }
                    | TablePhase::AwaitingDiscardReactions(_)
                    | TablePhase::AwaitingRobKong { .. },
                PlayerAction::ConfirmNextHand | PlayerAction::EndMatch
            )
        ) {
            return Err(MatchError::WrongPhase { seat, action });
        }

        let emitted = match &self.state.phase {
            TablePhase::TurnAction { actor, source } => {
                let actor = *actor;
                let source = *source;
                if seat != actor {
                    return Err(MatchError::NotYourTurn { seat });
                }
                if is_wrong_phase_for_turn_action(&action) {
                    return Err(MatchError::WrongPhase { seat, action });
                }
                if !self.legal_actions_for(seat).contains(&action) {
                    return Err(MatchError::IllegalAction { seat, action });
                }
                self.apply_turn_action(actor, source, action)?
            }
            TablePhase::AwaitingDiscardReactions(_) => self.apply_discard_reaction(seat, action)?,
            TablePhase::AwaitingRobKong(_) => self.apply_rob_kong_reaction(seat, action)?,
            TablePhase::BetweenHands { .. } => self.apply_between_hands_action(seat, action)?,
            TablePhase::MatchFinished { .. } => {
                return Err(MatchError::MatchAlreadyFinished);
            }
        };

        self.events.extend(emitted.iter().cloned());
        Ok(emitted)
    }

    fn apply_turn_action(
        &mut self,
        actor: Seat,
        source: TurnSource,
        action: PlayerAction,
    ) -> Result<Vec<RecordedEvent>, MatchError> {
        let mut out = Vec::new();
        match action {
            PlayerAction::Discard { tile, from_drawn } => {
                let seat_state = &mut self.state.seats[actor.index()];
                if from_drawn {
                    debug_assert_eq!(seat_state.drawn_tile, Some(tile));
                    seat_state.drawn_tile = None;
                } else {
                    let pos = seat_state
                        .closed_tiles
                        .iter()
                        .position(|&c| c == tile)
                        .expect("legal_actions verified tile is in closed_tiles");
                    seat_state.closed_tiles.remove(pos);
                    if let Some(drawn) = seat_state.drawn_tile.take() {
                        seat_state.closed_tiles.push(drawn);
                        seat_state.closed_tiles.sort_unstable();
                    }
                }

                let discard_seq = Self::push_event(
                    &mut self.state,
                    &mut out,
                    TableEventKind::TileDiscarded {
                        seat: actor,
                        tile,
                        is_tsumogiri: from_drawn,
                    },
                );
                self.state.seats[actor.index()].river.push(DiscardEntry {
                    tile,
                    is_tsumogiri: from_drawn,
                    claimed_by: None,
                    seq: discard_seq,
                });

                self.advance_after_discard(actor, tile, &mut out);
                Ok(out)
            }
            PlayerAction::Tsumo => {
                let seat_state = &self.state.seats[actor.index()];
                let winning_tile = seat_state.drawn_tile.unwrap_or_else(|| {
                    *seat_state
                        .closed_tiles
                        .last()
                        .expect("winning hand holds at least one closed tile")
                });
                let evaluated = self
                    .ruleset
                    .evaluate_win(&WinEvalInput {
                        closed_tiles: &seat_state.closed_tiles,
                        extra_tile: seat_state.drawn_tile,
                        melds: &seat_state.melds,
                        win_kind: WinKind::Tsumo,
                        turn_source: Some(source),
                        wall_remaining: self.state.wall_remaining(),
                    })
                    .expect("legal_actions verified Tsumo is a valid win");

                self.settle_hand_with_details(
                    Some(WinSettlementDetails {
                        winners: vec![WinCandidate {
                            seat: actor,
                            win_kind: WinKind::Tsumo,
                            evaluated,
                        }],
                        discarder: None,
                        winning_tile,
                    }),
                    &mut out,
                );
                Ok(out)
            }
            PlayerAction::ConcealedKong { tile } => {
                let meld_seq = self.state.seq + 1;
                let deltas = self
                    .ruleset
                    .kong_score_deltas(KongKind::ConcealedKong, actor, None);
                self.state.kong_transfers.push(KongTransfer {
                    kind: KongKind::ConcealedKong,
                    actor,
                    from_seat: None,
                    tile,
                    score_deltas: deltas,
                });

                let meld = RevealedMeld::ConcealedKong {
                    tile,
                    seq: meld_seq,
                };
                {
                    let seat_state = &mut self.state.seats[actor.index()];
                    seat_state.consume_tiles_for_kong(tile, 4);
                    seat_state.melds.push(meld);
                    seat_state.last_draw_or_meld_seq = meld_seq;
                    seat_state.passed_hu_lock = false;
                }

                let emitted_seq = Self::push_event(
                    &mut self.state,
                    &mut out,
                    TableEventKind::MeldDeclared {
                        seat: actor,
                        meld,
                        kong_score_deltas: Some(deltas),
                    },
                );
                debug_assert_eq!(emitted_seq, meld_seq);

                self.draw_from_wall_tail(actor, &mut out);
                Ok(out)
            }
            PlayerAction::AddedKong { tile } => {
                if let Some(window) = RobKongWindow::build(&self.state, &self.ruleset, actor, tile)
                {
                    Self::push_event(
                        &mut self.state,
                        &mut out,
                        TableEventKind::AddedKongAttempted { seat: actor, tile },
                    );
                    self.state.phase = TablePhase::AwaitingRobKong(window);
                } else {
                    self.complete_added_kong(actor, tile, &mut out);
                }
                Ok(out)
            }
            _ => Err(MatchError::IllegalAction {
                seat: actor,
                action,
            }),
        }
    }

    pub(crate) fn complete_added_kong(
        &mut self,
        actor: Seat,
        tile: Tile,
        out: &mut Vec<RecordedEvent>,
    ) {
        let meld_seq = self.state.seq + 1;
        let deltas = self
            .ruleset
            .kong_score_deltas(KongKind::AddedKong, actor, None);
        self.state.kong_transfers.push(KongTransfer {
            kind: KongKind::AddedKong,
            actor,
            from_seat: None,
            tile,
            score_deltas: deltas,
        });

        let upgraded_meld = {
            let seat_state = &mut self.state.seats[actor.index()];
            seat_state.consume_tiles_for_kong(tile, 1);
            let pon_slot = seat_state
                .melds
                .iter_mut()
                .find(|m| matches!(m, RevealedMeld::Pon { tile: t, .. } if *t == tile))
                .expect("legal AddedKong verified matching Pon meld");
            let RevealedMeld::Pon { from_seat, .. } = *pon_slot else {
                unreachable!("matched RevealedMeld::Pon above");
            };
            let upgraded = RevealedMeld::AddedKong {
                tile,
                from_seat,
                seq: meld_seq,
            };
            *pon_slot = upgraded;
            seat_state.last_draw_or_meld_seq = meld_seq;
            seat_state.passed_hu_lock = false;
            upgraded
        };

        let emitted_seq = Self::push_event(
            &mut self.state,
            out,
            TableEventKind::MeldDeclared {
                seat: actor,
                meld: upgraded_meld,
                kong_score_deltas: Some(deltas),
            },
        );
        debug_assert_eq!(emitted_seq, meld_seq);

        self.draw_from_wall_tail(actor, out);
    }

    fn advance_after_discard(&mut self, discarder: Seat, tile: Tile, out: &mut Vec<RecordedEvent>) {
        if let Some(window) =
            DiscardReactionWindow::build(&self.state, &self.ruleset, discarder, tile)
        {
            self.state.phase = TablePhase::AwaitingDiscardReactions(window);
            return;
        }

        if self.state.wall_remaining() == 0 {
            self.settle_hand_with_details(None, out);
            return;
        }

        let next_seat = discarder.next();
        self.draw_from_wall_head(next_seat, out);
    }

    pub(crate) fn draw_from_wall_head(&mut self, seat: Seat, out: &mut Vec<RecordedEvent>) {
        self.draw_tile(seat, false, out);
    }

    pub(crate) fn draw_from_wall_tail(&mut self, seat: Seat, out: &mut Vec<RecordedEvent>) {
        self.draw_tile(seat, true, out);
    }

    fn draw_tile(&mut self, seat: Seat, from_tail: bool, out: &mut Vec<RecordedEvent>) {
        let drawn = if from_tail {
            self.state.wall.pop_back()
        } else {
            self.state.wall.pop_front()
        }
        .expect("wall_remaining > 0 verified before draw");
        let wall_remaining = self.state.wall_remaining();
        let draw_seq = Self::push_event(
            &mut self.state,
            out,
            TableEventKind::TileDrawn {
                seat,
                tile: drawn,
                is_replacement: from_tail,
                wall_remaining,
            },
        );

        let seat_state = &mut self.state.seats[seat.index()];
        seat_state.drawn_tile = Some(drawn);
        seat_state.last_draw_or_meld_seq = draw_seq;
        seat_state.passed_hu_lock = false;
        seat_state.passed_pon_lock.clear();

        self.state.phase = TablePhase::TurnAction {
            actor: seat,
            source: if from_tail {
                TurnSource::ReplacementDraw
            } else {
                TurnSource::NormalDraw
            },
        };
    }

    /// Projects the current authoritative table state into `seat`'s view.
    #[must_use]
    pub fn project_view(&self, seat: Seat) -> PlayerView {
        projection::project_table_view(&self.state, &self.ruleset, seat)
    }

    /// Projects an opaque `RecordedEvent` into `seat`'s perspective.
    #[must_use]
    pub fn project_event(&self, event: &RecordedEvent, seat: Seat) -> PlayerEvent {
        projection::project_table_event(&event.0, seat)
    }

    pub(crate) fn push_event(
        state: &mut TableState,
        out: &mut Vec<RecordedEvent>,
        kind: TableEventKind,
    ) -> u64 {
        state.seq += 1;
        let seq = state.seq;
        out.push(RecordedEvent(TableEvent { seq, kind }));
        seq
    }

    pub(crate) fn legal_actions_for(&self, seat: Seat) -> Vec<PlayerAction> {
        legal_actions_for_state(&self.state, &self.ruleset, seat)
    }
}

pub(crate) fn legal_actions_for_state<R: Ruleset>(
    state: &TableState,
    ruleset: &R,
    seat: Seat,
) -> Vec<PlayerAction> {
    match &state.phase {
        TablePhase::TurnAction { actor, source } => {
            if seat != *actor {
                return Vec::new();
            }
            let seat_state = &state.seats[seat.index()];
            ruleset.legal_turn_actions(&TurnContext {
                closed_tiles: &seat_state.closed_tiles,
                drawn_tile: seat_state.drawn_tile,
                melds: &seat_state.melds,
                source: *source,
                wall_remaining: state.wall_remaining(),
            })
        }
        TablePhase::AwaitingDiscardReactions(window) => window.legal_actions(seat),
        TablePhase::AwaitingRobKong(window) => window.legal_actions(seat),
        TablePhase::BetweenHands { ready_seats, .. } => {
            progression::legal_between_hands_actions(state, *ready_seats, seat)
        }
        TablePhase::MatchFinished { .. } => Vec::new(),
    }
}

fn is_wrong_phase_for_turn_action(action: &PlayerAction) -> bool {
    matches!(
        action,
        PlayerAction::Ron { .. }
            | PlayerAction::RobKongHu { .. }
            | PlayerAction::Pon { .. }
            | PlayerAction::ExposedKong { .. }
            | PlayerAction::Chi { .. }
            | PlayerAction::Pass
            | PlayerAction::ConfirmNextHand
            | PlayerAction::EndMatch
    )
}
