//! Initial Tuidaohu (`推倒胡`) ruleset implementation.

use crate::protocol::{
    HandOutcome, HandSettlement, KongKind, MatchMode, PlayerAction, RuleProfile, WinKind,
    WinnerSettlement,
};
use crate::ruleset::{
    DiscardReactionContext, EvaluatedWin, HandProgression, HandProgressionInput, ReactionPriority,
    RobKongContext, Ruleset, SettleHandInput, TurnContext, WinEvalInput,
};
use crate::seat::Seat;
use crate::tile::Tile;

/// Standard 4-player Tuidaohu ruleset (`推倒胡`).
#[derive(Debug, Clone, Copy, Default)]
pub struct TuidaohuRuleset;

impl Ruleset for TuidaohuRuleset {
    fn profile(&self) -> RuleProfile {
        RuleProfile {
            name: "Tuidaohu".to_string(),
            allow_chi: false,
            allow_four_of_kind_seven_pairs: true,
            allow_multi_ron: true,
            fan_cap: 4,
            wall_size: Tile::DECK_SIZE as u8,
            dead_wall_size: 0,
            starting_scores_east_only: [100; 4],
            starting_scores_free_table: [0; 4],
        }
    }

    fn deck(&self) -> Vec<Tile> {
        Tile::standard_deck_136()
    }

    fn legal_turn_actions(&self, ctx: &TurnContext<'_>) -> Vec<PlayerAction> {
        let mut actions = Vec::new();
        let mut last_seen: Option<Tile> = None;
        for &tile in ctx.closed_tiles {
            if last_seen != Some(tile) {
                actions.push(PlayerAction::Discard {
                    tile,
                    from_drawn: false,
                });
                last_seen = Some(tile);
            }
        }
        if let Some(drawn) = ctx.drawn_tile {
            actions.push(PlayerAction::Discard {
                tile: drawn,
                from_drawn: true,
            });
        }
        actions
    }

    fn legal_discard_reactions(&self, _ctx: &DiscardReactionContext<'_>) -> Vec<PlayerAction> {
        Vec::new()
    }

    fn legal_rob_kong_reactions(&self, _ctx: &RobKongContext<'_>) -> Vec<PlayerAction> {
        Vec::new()
    }

    fn reaction_priority(&self, action: &PlayerAction) -> ReactionPriority {
        match action {
            PlayerAction::Tsumo | PlayerAction::Ron { .. } | PlayerAction::RobKongHu { .. } => {
                ReactionPriority::Hu
            }
            PlayerAction::Pon { .. } | PlayerAction::ExposedKong { .. } => {
                ReactionPriority::PonOrKong
            }
            PlayerAction::Chi { .. } => ReactionPriority::Chi,
            _ => ReactionPriority::Pass,
        }
    }

    fn evaluate_win(&self, _input: &WinEvalInput<'_>) -> Option<EvaluatedWin> {
        None
    }

    fn kong_score_deltas(&self, kind: KongKind, actor: Seat, from_seat: Option<Seat>) -> [i32; 4] {
        let mut deltas = [0i32; 4];
        match kind {
            KongKind::ExposedKong => {
                if let Some(payer) = from_seat {
                    deltas[actor.index()] += 2;
                    deltas[payer.index()] -= 2;
                }
            }
            KongKind::AddedKong => {
                for seat in Seat::ALL {
                    if seat != actor {
                        deltas[seat.index()] -= 1;
                        deltas[actor.index()] += 1;
                    }
                }
            }
            KongKind::ConcealedKong => {
                for seat in Seat::ALL {
                    if seat != actor {
                        deltas[seat.index()] -= 2;
                        deltas[actor.index()] += 2;
                    }
                }
            }
        }
        deltas
    }

    fn settle_hand(&self, input: SettleHandInput) -> HandSettlement {
        let mut score_deltas = [0i32; 4];
        for transfer in &input.kong_transfers {
            for (idx, delta) in transfer.score_deltas.iter().enumerate() {
                score_deltas[idx] += delta;
            }
        }

        let outcome = match input.win_details {
            None => HandOutcome::DrawGame,
            Some(details) => {
                let mut winners = Vec::with_capacity(details.winners.len());
                for candidate in details.winners {
                    match candidate.win_kind {
                        WinKind::Tsumo => {
                            for seat in Seat::ALL {
                                if seat != candidate.seat {
                                    score_deltas[seat.index()] -= candidate.evaluated.base_points;
                                    score_deltas[candidate.seat.index()] +=
                                        candidate.evaluated.base_points;
                                }
                            }
                        }
                        WinKind::Ron | WinKind::RobKongHu => {
                            if let Some(payer) = details.discarder {
                                score_deltas[payer.index()] -= candidate.evaluated.base_points;
                                score_deltas[candidate.seat.index()] +=
                                    candidate.evaluated.base_points;
                            }
                        }
                    }
                    winners.push(WinnerSettlement {
                        seat: candidate.seat,
                        win_kind: candidate.win_kind,
                        items: candidate.evaluated.items,
                        total_fan: candidate.evaluated.total_fan,
                        base_points: candidate.evaluated.base_points,
                    });
                }
                HandOutcome::Win {
                    winners,
                    discarder: details.discarder,
                    winning_tile: details.winning_tile,
                }
            }
        };

        let mut new_scores = input.current_scores;
        for (score, delta) in new_scores.iter_mut().zip(score_deltas.iter()) {
            *score += delta;
        }

        HandSettlement {
            wind_round: input.wind_round,
            hand_number: input.hand_number,
            dealer_streak: input.dealer_streak,
            dealer: input.dealer,
            outcome,
            kong_transfers: input.kong_transfers,
            score_deltas,
            new_scores,
            revealed_hands: input.revealed_hands,
        }
    }

    fn advance_hand(&self, input: HandProgressionInput<'_>) -> HandProgression {
        let dealer_won = match input.outcome {
            HandOutcome::Win { winners, .. } => winners.iter().any(|w| w.seat == input.dealer),
            HandOutcome::DrawGame => false,
        };

        if dealer_won {
            return HandProgression::NextHand {
                wind_round: input.wind_round,
                hand_number: input.hand_number,
                dealer_streak: input.dealer_streak + 1,
                dealer: input.dealer,
            };
        }

        if input.mode == MatchMode::EastOnly && input.hand_number >= 4 {
            return HandProgression::FinishMatch;
        }

        HandProgression::NextHand {
            wind_round: input.wind_round,
            hand_number: input.hand_number + 1,
            dealer_streak: 0,
            dealer: input.dealer.next(),
        }
    }
}
