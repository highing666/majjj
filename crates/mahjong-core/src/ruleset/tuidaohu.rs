//! Initial Tuidaohu (`推倒胡`) ruleset implementation.

use crate::protocol::{
    HandOutcome, HandSettlement, KongKind, MatchMode, PlayerAction, RevealedMeld, RuleProfile,
    ScoreItem, TurnSource, WinKind, WinnerSettlement,
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
        let can_tsumo = matches!(
            ctx.source,
            TurnSource::InitialDealer | TurnSource::NormalDraw | TurnSource::ReplacementDraw
        ) && self
            .evaluate_win(&WinEvalInput {
                closed_tiles: ctx.closed_tiles,
                extra_tile: ctx.drawn_tile,
                melds: ctx.melds,
                win_kind: WinKind::Tsumo,
                turn_source: Some(ctx.source),
                wall_remaining: ctx.wall_remaining,
            })
            .is_some();
        if can_tsumo {
            actions.push(PlayerAction::Tsumo);
        }

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

    fn legal_discard_reactions(&self, ctx: &DiscardReactionContext<'_>) -> Vec<PlayerAction> {
        if ctx.actor == ctx.discarder {
            return Vec::new();
        }

        let mut actions = Vec::new();
        if !ctx.passed_hu_lock
            && self
                .evaluate_win(&WinEvalInput {
                    closed_tiles: ctx.closed_tiles,
                    extra_tile: Some(ctx.tile),
                    melds: ctx.melds,
                    win_kind: WinKind::Ron,
                    turn_source: None,
                    wall_remaining: ctx.wall_remaining,
                })
                .is_some()
        {
            actions.push(PlayerAction::Ron { tile: ctx.tile });
        }

        if ctx.wall_remaining > 0
            && !ctx.passed_pon_locked
            && ctx.closed_tiles.iter().filter(|&&t| t == ctx.tile).count() >= 2
        {
            actions.push(PlayerAction::Pon { tile: ctx.tile });
        }

        actions
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

    fn evaluate_win(&self, input: &WinEvalInput<'_>) -> Option<EvaluatedWin> {
        if input.melds.len() > 4 {
            return None;
        }
        let expected_closed = (4 - input.melds.len()) * 3 + 2;
        let actual_closed = input.closed_tiles.len() + usize::from(input.extra_tile.is_some());
        if actual_closed != expected_closed {
            return None;
        }

        let mut closed_counts = [0u8; Tile::COUNT];
        for &tile in input.closed_tiles {
            closed_counts[tile.index()] += 1;
        }
        if let Some(extra) = input.extra_tile {
            closed_counts[extra.index()] += 1;
        }

        let mut total_counts = closed_counts;
        for meld in input.melds {
            add_meld_counts(meld, &mut total_counts);
        }
        if total_counts.iter().any(|&c| c > 4) {
            return None;
        }

        let profile = self.profile();
        let fan_cap = profile.fan_cap;
        let mut best: Option<CandidateEval> = None;

        if input.melds.is_empty() {
            if is_thirteen_orphans(&closed_counts) {
                let mut items = vec![score_item("thirteen_orphans", "十三幺", 4)];
                append_situational_items(input, &mut items);
                best = pick_best(best, finalize_candidate(items, fan_cap));
            }
            if let Some(has_quad) =
                check_seven_pairs(&closed_counts, profile.allow_four_of_kind_seven_pairs)
            {
                let mut items = Vec::new();
                append_flush_item(&total_counts, &mut items);
                if has_quad {
                    items.push(score_item("luxury_seven_pairs", "豪华七对", 2));
                } else {
                    items.push(score_item("seven_pairs", "七对", 1));
                }
                append_situational_items(input, &mut items);
                best = pick_best(best, finalize_candidate(items, fan_cap));
            }
        }

        let (has_standard, has_all_pungs_closed) =
            analyze_standard_decompositions(&mut closed_counts);
        if has_standard {
            let is_all_pungs = input.melds.iter().all(meld_is_pung_or_kong) && has_all_pungs_closed;
            let is_menqing = input.melds.iter().all(meld_keeps_menqing);

            let mut items = Vec::new();
            append_flush_item(&total_counts, &mut items);
            if is_all_pungs {
                items.push(score_item("all_pungs", "碰碰胡", 1));
            }
            if is_menqing {
                items.push(score_item("menqing", "门前清", 1));
            }
            append_situational_items(input, &mut items);
            if items.is_empty() {
                items.push(score_item("basic_hu", "基本胡", 0));
            }
            best = pick_best(best, finalize_candidate(items, fan_cap));
        }

        best.map(|cand| cand.evaluated)
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

struct CandidateEval {
    evaluated: EvaluatedWin,
    raw_fan: u16,
}

fn finalize_candidate(items: Vec<ScoreItem>, fan_cap: u8) -> CandidateEval {
    let raw_fan: u16 = items.iter().map(|item| u16::from(item.fan)).sum();
    let total_fan = raw_fan.min(u16::from(fan_cap)) as u8;
    let base_points = 1i32 << total_fan;
    CandidateEval {
        evaluated: EvaluatedWin {
            items,
            total_fan,
            base_points,
        },
        raw_fan,
    }
}

fn pick_best(best: Option<CandidateEval>, candidate: CandidateEval) -> Option<CandidateEval> {
    match best {
        None => Some(candidate),
        Some(prev) => {
            if (candidate.evaluated.total_fan, candidate.raw_fan)
                > (prev.evaluated.total_fan, prev.raw_fan)
            {
                Some(candidate)
            } else {
                Some(prev)
            }
        }
    }
}

fn is_thirteen_orphans(counts: &[u8; Tile::COUNT]) -> bool {
    counts.iter().enumerate().all(|(idx, &count)| {
        let tile = Tile::new(idx as u8).expect("index bounded by Tile::COUNT");
        if tile.is_terminal_or_honor() {
            count >= 1
        } else {
            count == 0
        }
    })
}

fn check_seven_pairs(counts: &[u8; Tile::COUNT], allow_quad: bool) -> Option<bool> {
    let mut has_quad = false;
    for &c in counts {
        match c {
            0 | 2 => {}
            4 if allow_quad => has_quad = true,
            _ => return None,
        }
    }
    Some(has_quad)
}

fn score_item(code: &str, label: &str, fan: u8) -> ScoreItem {
    ScoreItem {
        code: code.to_string(),
        label: label.to_string(),
        fan,
    }
}

fn add_meld_counts(meld: &RevealedMeld, counts: &mut [u8; Tile::COUNT]) {
    match *meld {
        RevealedMeld::Pon { tile, .. } => counts[tile.index()] += 3,
        RevealedMeld::ExposedKong { tile, .. }
        | RevealedMeld::AddedKong { tile, .. }
        | RevealedMeld::ConcealedKong { tile, .. } => counts[tile.index()] += 4,
        RevealedMeld::Chi { tiles, .. } => {
            for tile in tiles {
                counts[tile.index()] += 1;
            }
        }
    }
}

fn meld_is_pung_or_kong(meld: &RevealedMeld) -> bool {
    matches!(
        meld,
        RevealedMeld::Pon { .. }
            | RevealedMeld::ExposedKong { .. }
            | RevealedMeld::AddedKong { .. }
            | RevealedMeld::ConcealedKong { .. }
    )
}

fn meld_keeps_menqing(meld: &RevealedMeld) -> bool {
    matches!(meld, RevealedMeld::ConcealedKong { .. })
}

fn append_flush_item(total_counts: &[u8; Tile::COUNT], items: &mut Vec<ScoreItem>) {
    let has_man = total_counts[0..9].iter().any(|&c| c > 0);
    let has_bamboo = total_counts[9..18].iter().any(|&c| c > 0);
    let has_dot = total_counts[18..27].iter().any(|&c| c > 0);
    let has_honor = total_counts[27..Tile::COUNT].iter().any(|&c| c > 0);
    let number_suits = u8::from(has_man) + u8::from(has_bamboo) + u8::from(has_dot);

    if number_suits == 0 && has_honor {
        items.push(score_item("all_honors", "字一色", 4));
    } else if number_suits == 1 && !has_honor {
        items.push(score_item("full_flush", "清一色", 2));
    } else if number_suits == 1 && has_honor {
        items.push(score_item("half_flush", "混一色", 1));
    }
}

fn append_situational_items(input: &WinEvalInput<'_>, items: &mut Vec<ScoreItem>) {
    if input.win_kind == WinKind::Tsumo && input.turn_source == Some(TurnSource::ReplacementDraw) {
        items.push(score_item("self_draw_after_kong", "杠上开花", 1));
    }
    if input.win_kind == WinKind::RobKongHu {
        items.push(score_item("rob_kong_hu", "抢杠胡", 1));
    }
    if input.win_kind == WinKind::Tsumo
        && input.wall_remaining == 0
        && matches!(
            input.turn_source,
            Some(TurnSource::NormalDraw | TurnSource::ReplacementDraw)
        )
    {
        items.push(score_item("last_tile_tsumo", "海底捞月", 1));
    }
    if input.win_kind == WinKind::Ron && input.wall_remaining == 0 {
        items.push(score_item("last_tile_ron", "河底捞鱼", 1));
    }
}

fn analyze_standard_decompositions(counts: &mut [u8; Tile::COUNT]) -> (bool, bool) {
    let mut found_any = false;
    let mut found_all_pungs = false;

    for pair_idx in 0..Tile::COUNT {
        if counts[pair_idx] >= 2 {
            counts[pair_idx] -= 2;
            partition_melds(counts, true, &mut found_any, &mut found_all_pungs);
            counts[pair_idx] += 2;
        }
    }

    (found_any, found_all_pungs)
}

fn partition_melds(
    counts: &mut [u8; Tile::COUNT],
    all_triplets_so_far: bool,
    found_any: &mut bool,
    found_all_pungs: &mut bool,
) {
    let Some(first) = counts.iter().position(|&c| c > 0) else {
        *found_any = true;
        if all_triplets_so_far {
            *found_all_pungs = true;
        }
        return;
    };

    if counts[first] >= 3 {
        counts[first] -= 3;
        partition_melds(counts, all_triplets_so_far, found_any, found_all_pungs);
        counts[first] += 3;
    }

    if first < 27 && first % 9 <= 6 && counts[first + 1] > 0 && counts[first + 2] > 0 {
        counts[first] -= 1;
        counts[first + 1] -= 1;
        counts[first + 2] -= 1;
        partition_melds(counts, false, found_any, found_all_pungs);
        counts[first] += 1;
        counts[first + 1] += 1;
        counts[first + 2] += 1;
    }
}
