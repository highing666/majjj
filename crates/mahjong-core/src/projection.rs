//! Pure seat-based projection functions hiding internal table state and opponent secrets.

use crate::engine::{
    legal_actions_for_state, Match, RecordedEvent, TableEvent, TableEventKind, TablePhase,
    TableState,
};
use crate::protocol::{
    MeldView, PlayerEvent, PlayerEventKind, PlayerHandView, PlayerView, PublicPhase,
    PublicSeatView, RevealedMeld, TurnView,
};
use crate::ruleset::Ruleset;
use crate::seat::Seat;
use crate::wall::WallProvider;

/// Projects a [`Match`]'s current state into `seat`'s authoritative [`PlayerView`].
#[must_use]
pub fn project_view<R: Ruleset, W: WallProvider>(table: &Match<R, W>, seat: Seat) -> PlayerView {
    project_table_view(&table.state, &table.ruleset, seat)
}

/// Projects an opaque [`RecordedEvent`] into `seat`'s [`PlayerEvent`].
#[must_use]
pub fn project_event(event: &RecordedEvent, seat: Seat) -> PlayerEvent {
    project_table_event(&event.0, seat)
}

pub(crate) fn project_table_view<R: Ruleset>(
    state: &TableState,
    ruleset: &R,
    viewer: Seat,
) -> PlayerView {
    let viewer_state = &state.seats[viewer.index()];
    let hand = PlayerHandView {
        closed_tiles: viewer_state.closed_tiles.clone(),
        drawn_tile: viewer_state.drawn_tile,
    };

    let seats = std::array::from_fn(|idx| {
        let seat = Seat::from_index(idx).expect("array length is 4");
        let seat_state = &state.seats[idx];
        let is_owner = seat == viewer;
        PublicSeatView {
            seat,
            score: state.scores[idx],
            hand_count: seat_state.hand_count(),
            river: seat_state.river.clone(),
            melds: seat_state
                .melds
                .iter()
                .map(|m| project_meld(*m, is_owner))
                .collect(),
            last_draw_or_meld_seq: seat_state.last_draw_or_meld_seq,
        }
    });

    let (phase, active_seat, last_settlement, ready_seats, match_summary) = match &state.phase {
        TablePhase::TurnAction { actor, .. } => {
            (PublicPhase::Playing, Some(*actor), None, None, None)
        }
        TablePhase::AwaitingDiscardReactions(window) => (
            PublicPhase::Playing,
            Some(window.discarder),
            None,
            None,
            None,
        ),
        TablePhase::AwaitingRobKong { actor, .. } => {
            (PublicPhase::Playing, Some(*actor), None, None, None)
        }
        TablePhase::BetweenHands {
            last_settlement,
            ready_seats,
        } => (
            PublicPhase::BetweenHands,
            None,
            Some(last_settlement.clone()),
            Some(*ready_seats),
            None,
        ),
        TablePhase::MatchFinished {
            last_settlement,
            summary,
        } => (
            PublicPhase::MatchFinished,
            None,
            last_settlement.clone(),
            None,
            Some(*summary),
        ),
    };

    PlayerView {
        viewer,
        mode: state.mode,
        rule_profile: ruleset.profile(),
        phase,
        hand,
        seats,
        wall_remaining: state.wall_remaining(),
        turn: TurnView {
            wind_round: state.wind_round,
            hand_number: state.hand_number,
            dealer_streak: state.dealer_streak,
            dealer: state.dealer,
            active_seat,
        },
        legal_actions: legal_actions_for_state(state, ruleset, viewer),
        last_settlement,
        ready_seats,
        match_summary,
        seq: state.seq,
    }
}

pub(crate) fn project_table_event(event: &TableEvent, viewer: Seat) -> PlayerEvent {
    let kind = match &event.kind {
        TableEventKind::MatchStarted {
            mode,
            initial_dealer,
            initial_scores,
            rule_profile,
        } => PlayerEventKind::MatchStarted {
            mode: *mode,
            initial_dealer: *initial_dealer,
            initial_scores: *initial_scores,
            rule_profile: rule_profile.clone(),
        },
        TableEventKind::HandStarted {
            wind_round,
            hand_number,
            dealer_streak,
            dealer,
            hands,
            wall_remaining,
            scores,
        } => PlayerEventKind::HandStarted {
            wind_round: *wind_round,
            hand_number: *hand_number,
            dealer_streak: *dealer_streak,
            dealer: *dealer,
            hand: hands[viewer.index()].clone(),
            wall_remaining: *wall_remaining,
            scores: *scores,
        },
        TableEventKind::TileDrawn {
            seat,
            tile,
            is_replacement,
            wall_remaining,
        } => PlayerEventKind::TileDrawn {
            seat: *seat,
            tile: if *seat == viewer { Some(*tile) } else { None },
            is_replacement: *is_replacement,
            wall_remaining: *wall_remaining,
        },
        TableEventKind::TileDiscarded {
            seat,
            tile,
            is_tsumogiri,
        } => PlayerEventKind::TileDiscarded {
            seat: *seat,
            tile: *tile,
            is_tsumogiri: *is_tsumogiri,
        },
        TableEventKind::AddedKongAttempted { seat, tile } => PlayerEventKind::AddedKongAttempted {
            seat: *seat,
            tile: *tile,
        },
        TableEventKind::MeldDeclared {
            seat,
            meld,
            kong_score_deltas,
        } => PlayerEventKind::MeldDeclared {
            seat: *seat,
            meld: project_meld(*meld, *seat == viewer),
            kong_score_deltas: *kong_score_deltas,
        },
        TableEventKind::HandSettled { settlement } => PlayerEventKind::HandSettled {
            settlement: settlement.clone(),
        },
        TableEventKind::NextHandConfirmed { seat } => {
            PlayerEventKind::NextHandConfirmed { seat: *seat }
        }
        TableEventKind::MatchFinished { summary } => {
            PlayerEventKind::MatchFinished { summary: *summary }
        }
    };

    PlayerEvent {
        seq: event.seq,
        kind,
    }
}

fn project_meld(meld: RevealedMeld, is_owner: bool) -> MeldView {
    match meld {
        RevealedMeld::Pon {
            tile,
            from_seat,
            seq,
        } => MeldView::Pon {
            tile,
            from_seat,
            seq,
        },
        RevealedMeld::ExposedKong {
            tile,
            from_seat,
            seq,
        } => MeldView::ExposedKong {
            tile,
            from_seat,
            seq,
        },
        RevealedMeld::AddedKong {
            tile,
            from_seat,
            seq,
        } => MeldView::AddedKong {
            tile,
            from_seat,
            seq,
        },
        RevealedMeld::ConcealedKong { tile, seq } => MeldView::ConcealedKong {
            tile: if is_owner { Some(tile) } else { None },
            seq,
        },
        RevealedMeld::Chi {
            tiles,
            called_tile,
            from_seat,
            seq,
        } => MeldView::Chi {
            tiles,
            called_tile,
            from_seat,
            seq,
        },
    }
}
