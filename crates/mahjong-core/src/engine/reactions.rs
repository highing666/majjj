//! Discard and rob-kong reaction window construction, validation, and priority arbitration.

use crate::engine::{Match, RecordedEvent, SeatState, TableEventKind, TablePhase, TableState};
use crate::protocol::{
    KongKind, KongTransfer, MatchError, PlayerAction, RevealedMeld, TurnSource, WinKind,
};
use crate::ruleset::{
    DiscardReactionContext, ReactionPriority, RobKongContext, Ruleset, WinCandidate, WinEvalInput,
    WinSettlementDetails,
};
use crate::seat::Seat;
use crate::tile::Tile;
use crate::wall::WallProvider;

/// Per-seat reaction tracking inside an open reaction window.
#[derive(Debug, Clone)]
pub(crate) struct ReactionEntry {
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
    pub(crate) entries: Vec<ReactionEntry>,
}

/// Crate-private state for `TablePhase::AwaitingRobKong`.
#[derive(Debug, Clone)]
pub(crate) struct RobKongWindow {
    pub(crate) actor: Seat,
    pub(crate) tile: Tile,
    /// Ordered counter-clockwise from `actor`.
    pub(crate) entries: Vec<ReactionEntry>,
}

impl DiscardReactionWindow {
    /// Builds a discard reaction window iff at least one non-discarder seat has positive reactions.
    pub(crate) fn build<R: Ruleset>(
        state: &TableState,
        ruleset: &R,
        discarder: Seat,
        tile: Tile,
    ) -> Option<Self> {
        let wall_remaining = state.wall_remaining();
        let next_seat = discarder.next();
        let entries = collect_reaction_entries(discarder, |reactor| {
            let s = &state.seats[reactor.index()];
            ruleset.legal_discard_reactions(&DiscardReactionContext {
                actor: reactor,
                discarder,
                tile,
                closed_tiles: &s.closed_tiles,
                melds: &s.melds,
                wall_remaining,
                passed_hu_lock: s.passed_hu_lock,
                passed_pon_locked: s.passed_pon_lock.contains(&tile),
                is_next_seat: reactor == next_seat,
            })
        });

        (!entries.is_empty()).then_some(Self {
            discarder,
            tile,
            entries,
        })
    }

    pub(crate) fn legal_actions(&self, seat: Seat) -> Vec<PlayerAction> {
        entry_legal_actions(&self.entries, seat)
    }
}

impl RobKongWindow {
    /// Builds a rob-kong window iff at least one opponent can legally `RobKongHu`.
    pub(crate) fn build<R: Ruleset>(
        state: &TableState,
        ruleset: &R,
        actor: Seat,
        tile: Tile,
    ) -> Option<Self> {
        let wall_remaining = state.wall_remaining();
        let entries = collect_reaction_entries(actor, |reactor| {
            let s = &state.seats[reactor.index()];
            ruleset.legal_rob_kong_reactions(&RobKongContext {
                actor: reactor,
                kong_actor: actor,
                tile,
                closed_tiles: &s.closed_tiles,
                melds: &s.melds,
                wall_remaining,
                passed_hu_lock: s.passed_hu_lock,
            })
        });

        (!entries.is_empty()).then_some(Self {
            actor,
            tile,
            entries,
        })
    }

    pub(crate) fn legal_actions(&self, seat: Seat) -> Vec<PlayerAction> {
        entry_legal_actions(&self.entries, seat)
    }
}

fn collect_reaction_entries(
    origin: Seat,
    mut build_offered: impl FnMut(Seat) -> Vec<PlayerAction>,
) -> Vec<ReactionEntry> {
    let s1 = origin.next();
    let s2 = s1.next();
    let s3 = s2.next();
    [s1, s2, s3]
        .into_iter()
        .filter_map(|reactor| {
            let offered = build_offered(reactor);
            (!offered.is_empty()).then_some(ReactionEntry {
                seat: reactor,
                offered,
                submitted: None,
            })
        })
        .collect()
}

fn is_entry_pending(entries: &[ReactionEntry], seat: Seat) -> bool {
    entries
        .iter()
        .any(|entry| entry.seat == seat && entry.submitted.is_none())
}

fn entry_legal_actions(entries: &[ReactionEntry], seat: Seat) -> Vec<PlayerAction> {
    let Some(entry) = entries
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

fn best_submitted_priority<R: Ruleset>(
    entries: &[ReactionEntry],
    ruleset: &R,
) -> Option<(ReactionPriority, usize)> {
    let mut best: Option<(ReactionPriority, usize)> = None;
    for (idx, entry) in entries.iter().enumerate() {
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

fn entries_ready_to_resolve<R: Ruleset>(entries: &[ReactionEntry], ruleset: &R) -> bool {
    let any_pending = entries.iter().any(|e| e.submitted.is_none());
    if !any_pending {
        return true;
    }
    let Some((best_prio, earliest_best_idx)) = best_submitted_priority(entries, ruleset) else {
        return false;
    };
    let allow_multi_ron = ruleset.profile().allow_multi_ron;
    !entries.iter().enumerate().any(|(idx, entry)| {
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

fn validate_reaction_submission(
    entries: &[ReactionEntry],
    seat: Seat,
    action: PlayerAction,
    is_wrong_phase: impl FnOnce(&PlayerAction) -> bool,
) -> Result<(), MatchError> {
    if !is_entry_pending(entries, seat) {
        return Err(MatchError::NotYourTurn { seat });
    }
    if is_wrong_phase(&action) {
        return Err(MatchError::WrongPhase { seat, action });
    }
    if !entry_legal_actions(entries, seat).contains(&action) {
        return Err(MatchError::IllegalAction { seat, action });
    }
    Ok(())
}

fn record_reaction_submission<R: Ruleset>(
    entries: &mut [ReactionEntry],
    seats: &mut [SeatState; 4],
    ruleset: &R,
    seat: Seat,
    action: PlayerAction,
) -> bool {
    let entry = entries
        .iter_mut()
        .find(|entry| entry.seat == seat)
        .expect("is_pending verified entry exists");
    entry.submitted = Some(action);
    if action == PlayerAction::Pass {
        seats[seat.index()].record_pass_locks(&entry.offered);
    }
    entries_ready_to_resolve(entries, ruleset)
}

fn is_wrong_phase_for_discard_reaction(action: &PlayerAction) -> bool {
    !matches!(
        action,
        PlayerAction::Ron { .. }
            | PlayerAction::Pon { .. }
            | PlayerAction::ExposedKong { .. }
            | PlayerAction::Chi { .. }
            | PlayerAction::Pass
    )
}

fn is_wrong_phase_for_rob_kong_reaction(action: &PlayerAction) -> bool {
    !matches!(action, PlayerAction::RobKongHu { .. } | PlayerAction::Pass)
}

impl<R: Ruleset, W: WallProvider> Match<R, W> {
    pub(crate) fn apply_reaction(
        &mut self,
        seat: Seat,
        action: PlayerAction,
    ) -> Result<Vec<RecordedEvent>, MatchError> {
        let (entries, is_wrong_phase): (&[ReactionEntry], fn(&PlayerAction) -> bool) =
            match &self.state.phase {
                TablePhase::AwaitingDiscardReactions(window) => {
                    (&window.entries, is_wrong_phase_for_discard_reaction)
                }
                TablePhase::AwaitingRobKong(window) => {
                    (&window.entries, is_wrong_phase_for_rob_kong_reaction)
                }
                _ => return Err(MatchError::WrongPhase { seat, action }),
            };
        validate_reaction_submission(entries, seat, action, is_wrong_phase)?;

        let ready = {
            let entries = match &mut self.state.phase {
                TablePhase::AwaitingDiscardReactions(window) => &mut window.entries,
                TablePhase::AwaitingRobKong(window) => &mut window.entries,
                _ => unreachable!("verified reaction phase above"),
            };
            record_reaction_submission(entries, &mut self.state.seats, &self.ruleset, seat, action)
        };
        if !ready {
            return Ok(Vec::new());
        }

        let phase = std::mem::replace(
            &mut self.state.phase,
            TablePhase::TurnAction {
                actor: Seat::East,
                source: TurnSource::NormalDraw,
            },
        );
        let mut out = Vec::new();
        match phase {
            TablePhase::AwaitingDiscardReactions(window) => {
                self.resolve_discard_reaction_window(window, &mut out);
            }
            TablePhase::AwaitingRobKong(window) => {
                self.resolve_rob_kong_window(window, &mut out);
            }
            _ => unreachable!("verified reaction phase above"),
        }
        Ok(out)
    }

    fn resolve_discard_reaction_window(
        &mut self,
        window: DiscardReactionWindow,
        out: &mut Vec<RecordedEvent>,
    ) {
        let best_claim = best_submitted_priority(&window.entries, &self.ruleset).and_then(
            |(best_prio, best_idx)| {
                window.entries.get(best_idx).and_then(|entry| {
                    entry
                        .submitted
                        .map(|action| (best_prio, entry.seat, action))
                })
            },
        );

        match best_claim {
            Some((ReactionPriority::Hu, _, _)) => {
                self.settle_hu_reactions(
                    &window.entries,
                    window.discarder,
                    window.tile,
                    WinKind::Ron,
                    out,
                );
            }
            Some((ReactionPriority::PonOrKong, seat, PlayerAction::Pon { tile })) => {
                debug_assert_eq!(tile, window.tile);
                self.resolve_pon(seat, window.discarder, tile, out);
            }
            Some((ReactionPriority::PonOrKong, seat, PlayerAction::ExposedKong { tile })) => {
                debug_assert_eq!(tile, window.tile);
                self.resolve_exposed_kong(seat, window.discarder, tile, out);
            }
            Some((ReactionPriority::Chi, seat, PlayerAction::Chi { tile, with })) => {
                debug_assert_eq!(tile, window.tile);
                self.resolve_chi(seat, window.discarder, tile, with, out);
            }
            _ => {
                self.advance_after_unclaimed_discard(window.discarder, out);
            }
        }
    }

    fn resolve_rob_kong_window(&mut self, window: RobKongWindow, out: &mut Vec<RecordedEvent>) {
        match best_submitted_priority(&window.entries, &self.ruleset) {
            Some((ReactionPriority::Hu, _)) => {
                self.settle_hu_reactions(
                    &window.entries,
                    window.actor,
                    window.tile,
                    WinKind::RobKongHu,
                    out,
                );
            }
            _ => {
                self.complete_added_kong(window.actor, window.tile, out);
            }
        }
    }

    fn settle_hu_reactions(
        &mut self,
        entries: &[ReactionEntry],
        payer: Seat,
        tile: Tile,
        win_kind: WinKind,
        out: &mut Vec<RecordedEvent>,
    ) {
        let wall_remaining = self.state.wall_remaining();
        let allow_multi_ron = self.ruleset.profile().allow_multi_ron;
        let mut winners = Vec::new();

        for entry in entries {
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
                    extra_tile: Some(tile),
                    melds: &seat_state.melds,
                    win_kind,
                    turn_source: None,
                    wall_remaining,
                })
                .expect("legal Hu reaction must evaluate as a win");
            winners.push(WinCandidate {
                seat: entry.seat,
                win_kind,
                evaluated,
            });
            if !allow_multi_ron {
                break;
            }
        }

        self.settle_hand_with_details(
            Some(WinSettlementDetails {
                winners,
                discarder: Some(payer),
                winning_tile: tile,
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
        let meld = RevealedMeld::Pon {
            tile,
            from_seat: discarder,
            seq: self.state.seq + 1,
        };
        self.apply_discard_meld(actor, meld, &[tile, tile], None, out);
        self.state.phase = TablePhase::TurnAction {
            actor,
            source: TurnSource::AfterMeld,
        };
    }

    fn resolve_chi(
        &mut self,
        actor: Seat,
        discarder: Seat,
        tile: Tile,
        with: [Tile; 2],
        out: &mut Vec<RecordedEvent>,
    ) {
        let mut tiles = [tile, with[0], with[1]];
        tiles.sort_unstable();
        let meld = RevealedMeld::Chi {
            tiles,
            called_tile: tile,
            from_seat: discarder,
            seq: self.state.seq + 1,
        };
        self.apply_discard_meld(actor, meld, &with, None, out);
        self.state.phase = TablePhase::TurnAction {
            actor,
            source: TurnSource::AfterMeld,
        };
    }

    fn resolve_exposed_kong(
        &mut self,
        actor: Seat,
        discarder: Seat,
        tile: Tile,
        out: &mut Vec<RecordedEvent>,
    ) {
        let meld = RevealedMeld::ExposedKong {
            tile,
            from_seat: discarder,
            seq: self.state.seq + 1,
        };
        let deltas = self
            .ruleset
            .kong_score_deltas(KongKind::ExposedKong, actor, Some(discarder));
        self.state.kong_transfers.push(KongTransfer {
            kind: KongKind::ExposedKong,
            actor,
            from_seat: Some(discarder),
            tile,
            score_deltas: deltas,
        });
        self.apply_discard_meld(actor, meld, &[tile, tile, tile], Some(deltas), out);
        self.draw_from_wall_tail(actor, out);
    }

    fn apply_discard_meld(
        &mut self,
        actor: Seat,
        meld: RevealedMeld,
        consumed: &[Tile],
        kong_score_deltas: Option<[i32; 4]>,
        out: &mut Vec<RecordedEvent>,
    ) {
        let (discarder, tile, meld_seq) = match meld {
            RevealedMeld::Pon {
                tile,
                from_seat,
                seq,
            }
            | RevealedMeld::ExposedKong {
                tile,
                from_seat,
                seq,
            }
            | RevealedMeld::Chi {
                called_tile: tile,
                from_seat,
                seq,
                ..
            } => (from_seat, tile, seq),
            RevealedMeld::AddedKong { .. } | RevealedMeld::ConcealedKong { .. } => {
                unreachable!("apply_discard_meld only handles discard melds")
            }
        };

        self.state.seats[discarder.index()].claim_last_discard(tile, actor);
        {
            let actor_state = &mut self.state.seats[actor.index()];
            actor_state.consume_tiles(consumed);
            actor_state.record_declared_meld(meld, meld_seq);
        }

        let emitted_seq = Self::push_event(
            &mut self.state,
            out,
            TableEventKind::MeldDeclared {
                seat: actor,
                meld,
                kong_score_deltas,
            },
        );
        debug_assert_eq!(emitted_seq, meld_seq);
    }
}
