//! Discard reaction window construction, validation, and priority arbitration.

use crate::engine::{Match, RecordedEvent, SeatState, TableEventKind, TablePhase, TableState};
use crate::protocol::{MatchError, PlayerAction, RevealedMeld, TurnSource, WinKind};
use crate::ruleset::{
    DiscardReactionContext, ReactionPriority, Ruleset, WinCandidate, WinEvalInput,
    WinSettlementDetails,
};
use crate::seat::Seat;
use crate::tile::Tile;
use crate::wall::WallProvider;

/// Per-seat reaction tracking inside an open discard reaction window.
#[derive(Debug, Clone)]
pub(crate) struct DiscardReactionEntry {
    pub(crate) seat: Seat,
    pub(crate) offered: Vec<PlayerAction>,
    pub(crate) submitted: Option<PlayerAction>,
}

/// Crate-private state for `TablePhase::AwaitingDiscardReactions`.
#[derive(Debug, Clone)]
pub(crate) struct DiscardReactionWindow {
    pub(crate) discarder: Seat,
    pub(crate) tile: Tile,
    /// Ordered counter-clockwise from `discarder`.
    pub(crate) entries: Vec<DiscardReactionEntry>,
}

impl DiscardReactionWindow {
    /// Builds a reaction window iff at least one non-discarder seat has positive reactions.
    pub(crate) fn build<R: Ruleset>(
        state: &TableState,
        ruleset: &R,
        discarder: Seat,
        tile: Tile,
    ) -> Option<Self> {
        let wall_remaining = state.wall_remaining();
        let next_seat = discarder.next();
        let entries: Vec<DiscardReactionEntry> =
            [next_seat, next_seat.next(), next_seat.next().next()]
                .into_iter()
                .filter_map(|reactor| {
                    let s = &state.seats[reactor.index()];
                    let offered = ruleset.legal_discard_reactions(&DiscardReactionContext {
                        actor: reactor,
                        discarder,
                        tile,
                        closed_tiles: &s.closed_tiles,
                        melds: &s.melds,
                        wall_remaining,
                        passed_hu_lock: s.passed_hu_lock,
                        passed_pon_locked: s.passed_pon_lock.contains(&tile),
                        is_next_seat: reactor == next_seat,
                    });
                    if offered.is_empty() {
                        None
                    } else {
                        Some(DiscardReactionEntry {
                            seat: reactor,
                            offered,
                            submitted: None,
                        })
                    }
                })
                .collect();

        if entries.is_empty() {
            None
        } else {
            Some(Self {
                discarder,
                tile,
                entries,
            })
        }
    }

    pub(crate) fn is_pending(&self, seat: Seat) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.seat == seat && entry.submitted.is_none())
    }

    pub(crate) fn legal_actions(&self, seat: Seat) -> Vec<PlayerAction> {
        let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.seat == seat && entry.submitted.is_none())
        else {
            return Vec::new();
        };
        let mut actions = entry.offered.clone();
        if !actions.contains(&PlayerAction::Pass) {
            actions.push(PlayerAction::Pass);
        }
        actions
    }

    fn is_ready_to_resolve<R: Ruleset>(&self, ruleset: &R) -> bool {
        let any_pending = self.entries.iter().any(|e| e.submitted.is_none());
        if !any_pending {
            return true;
        }
        let Some((best_prio, earliest_best_idx)) = self.best_submitted_priority(ruleset) else {
            return false;
        };
        let allow_multi_ron = ruleset.profile().allow_multi_ron;
        !self.entries.iter().enumerate().any(|(idx, entry)| {
            if entry.submitted.is_some() {
                return false;
            }
            let max_prio = entry
                .offered
                .iter()
                .map(|a| ruleset.reaction_priority(a))
                .max()
                .unwrap_or(ReactionPriority::Pass);
            if max_prio > best_prio {
                return true;
            }
            if max_prio < best_prio {
                return false;
            }
            if best_prio == ReactionPriority::Hu && !allow_multi_ron {
                idx < earliest_best_idx
            } else {
                true
            }
        })
    }

    fn best_submitted_priority<R: Ruleset>(
        &self,
        ruleset: &R,
    ) -> Option<(ReactionPriority, usize)> {
        let mut best: Option<(ReactionPriority, usize)> = None;
        for (idx, entry) in self.entries.iter().enumerate() {
            let Some(action) = entry.submitted.as_ref() else {
                continue;
            };
            let prio = ruleset.reaction_priority(action);
            if prio == ReactionPriority::Pass {
                continue;
            }
            match best {
                None => best = Some((prio, idx)),
                Some((prev_prio, _)) if prio > prev_prio => best = Some((prio, idx)),
                _ => {}
            }
        }
        best
    }
}

pub(crate) fn is_wrong_phase_for_discard_reaction(action: &PlayerAction) -> bool {
    !matches!(
        action,
        PlayerAction::Ron { .. }
            | PlayerAction::Pon { .. }
            | PlayerAction::ExposedKong { .. }
            | PlayerAction::Chi { .. }
            | PlayerAction::Pass
    )
}

fn record_pass_locks(seat_state: &mut SeatState, offered: &[PlayerAction]) {
    for action in offered {
        match *action {
            PlayerAction::Ron { .. } | PlayerAction::RobKongHu { .. } => {
                seat_state.passed_hu_lock = true;
            }
            PlayerAction::Pon { tile } if !seat_state.passed_pon_lock.contains(&tile) => {
                seat_state.passed_pon_lock.push(tile);
            }
            _ => {}
        }
    }
}

impl<R: Ruleset, W: WallProvider> Match<R, W> {
    pub(crate) fn apply_discard_reaction(
        &mut self,
        seat: Seat,
        action: PlayerAction,
    ) -> Result<Vec<RecordedEvent>, MatchError> {
        let window = match &self.state.phase {
            TablePhase::AwaitingDiscardReactions(window) => window,
            _ => return Err(MatchError::WrongPhase { seat, action }),
        };

        if !window.is_pending(seat) {
            return Err(MatchError::NotYourTurn { seat });
        }
        if is_wrong_phase_for_discard_reaction(&action) {
            return Err(MatchError::WrongPhase { seat, action });
        }
        if !window.legal_actions(seat).contains(&action) {
            return Err(MatchError::IllegalAction { seat, action });
        }

        let ready = {
            let TablePhase::AwaitingDiscardReactions(window) = &mut self.state.phase else {
                unreachable!("verified AwaitingDiscardReactions phase");
            };
            let entry = window
                .entries
                .iter_mut()
                .find(|entry| entry.seat == seat)
                .expect("is_pending verified entry exists");
            entry.submitted = Some(action);
            if action == PlayerAction::Pass {
                record_pass_locks(&mut self.state.seats[seat.index()], &entry.offered);
            }
            window.is_ready_to_resolve(&self.ruleset)
        };

        if !ready {
            return Ok(Vec::new());
        }

        let TablePhase::AwaitingDiscardReactions(window) = std::mem::replace(
            &mut self.state.phase,
            TablePhase::TurnAction {
                actor: Seat::East,
                source: TurnSource::NormalDraw,
            },
        ) else {
            unreachable!("verified AwaitingDiscardReactions phase");
        };

        let mut out = Vec::new();
        self.resolve_discard_reaction_window(window, &mut out);
        Ok(out)
    }

    fn resolve_discard_reaction_window(
        &mut self,
        window: DiscardReactionWindow,
        out: &mut Vec<RecordedEvent>,
    ) {
        let Some((best_prio, _)) = window.best_submitted_priority(&self.ruleset) else {
            if self.state.wall_remaining() == 0 {
                self.settle_hand_with_details(None, out);
            } else {
                self.draw_from_wall_head(window.discarder.next(), out);
            }
            return;
        };

        match best_prio {
            ReactionPriority::Pass => unreachable!("filtered out above"),
            ReactionPriority::Hu => {
                self.resolve_ron(&window, out);
            }
            ReactionPriority::PonOrKong => {
                let winner = window
                    .entries
                    .iter()
                    .find(|entry| {
                        entry
                            .submitted
                            .as_ref()
                            .is_some_and(|a| self.ruleset.reaction_priority(a) == best_prio)
                    })
                    .expect("best_submitted_priority guarantees a matching entry");
                match winner.submitted.expect("winner submitted action") {
                    PlayerAction::Pon { tile } => {
                        debug_assert_eq!(tile, window.tile);
                        self.resolve_pon(winner.seat, window.discarder, tile, out);
                    }
                    other => todo!("reaction {other:?} is handled in a later ticket"),
                }
            }
            ReactionPriority::Chi => {
                todo!("Chi is disabled in v1 Tuidaohu")
            }
        }
    }

    fn resolve_ron(&mut self, window: &DiscardReactionWindow, out: &mut Vec<RecordedEvent>) {
        let wall_remaining = self.state.wall_remaining();
        let allow_multi_ron = self.ruleset.profile().allow_multi_ron;
        let mut winners = Vec::new();

        for entry in &window.entries {
            let Some(action) = entry.submitted.as_ref() else {
                continue;
            };
            if self.ruleset.reaction_priority(action) != ReactionPriority::Hu {
                continue;
            }
            let seat_state = &self.state.seats[entry.seat.index()];
            let evaluated = self
                .ruleset
                .evaluate_win(&WinEvalInput {
                    closed_tiles: &seat_state.closed_tiles,
                    extra_tile: Some(window.tile),
                    melds: &seat_state.melds,
                    win_kind: WinKind::Ron,
                    turn_source: None,
                    wall_remaining,
                })
                .expect("legal Ron reaction must evaluate as a win");
            winners.push(WinCandidate {
                seat: entry.seat,
                win_kind: WinKind::Ron,
                evaluated,
            });
            if !allow_multi_ron {
                break;
            }
        }

        self.settle_hand_with_details(
            Some(WinSettlementDetails {
                winners,
                discarder: Some(window.discarder),
                winning_tile: window.tile,
            }),
            out,
        );
    }

    fn resolve_pon(
        &mut self,
        actor: Seat,
        discarder: Seat,
        tile: Tile,
        out: &mut Vec<RecordedEvent>,
    ) {
        let meld_seq = self.state.seq + 1;
        let meld = RevealedMeld::Pon {
            tile,
            from_seat: discarder,
            seq: meld_seq,
        };

        {
            let discarder_entry = self.state.seats[discarder.index()]
                .river
                .last_mut()
                .expect("discarder river holds the claimed discard");
            debug_assert_eq!(discarder_entry.tile, tile);
            debug_assert_eq!(discarder_entry.claimed_by, None);
            discarder_entry.claimed_by = Some(actor);
        }

        {
            let actor_state = &mut self.state.seats[actor.index()];
            for _ in 0..2 {
                let pos = actor_state
                    .closed_tiles
                    .iter()
                    .position(|&t| t == tile)
                    .expect("legal Pon verified 2 matching tiles in closed_tiles");
                actor_state.closed_tiles.remove(pos);
            }
            actor_state.melds.push(meld);
            actor_state.last_draw_or_meld_seq = meld_seq;
            actor_state.passed_hu_lock = false;
        }

        let emitted_seq = Self::push_event(
            &mut self.state,
            out,
            TableEventKind::MeldDeclared {
                seat: actor,
                meld,
                kong_score_deltas: None,
            },
        );
        debug_assert_eq!(emitted_seq, meld_seq);

        self.state.phase = TablePhase::TurnAction {
            actor,
            source: TurnSource::AfterMeld,
        };
    }
}
