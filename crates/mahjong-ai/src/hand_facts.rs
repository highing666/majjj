//! Pure synchronous, read-only hand-facts analyzer consuming only a single seat's [`PlayerView`].
//!
//! Precomputes exact shanten (across standard, `七对`/`豪华七对`, and `十三幺`), exact tenpai waits
//! with publicly visible remaining copy counts (`0..=4`) and Tuidaohu winning fan, realistic
//! high-fan directions, and progression delta relative to a previous snapshot.

use mahjong_core::{PlayerView, Tile, TurnSource, WinKind};
use serde::{Deserialize, Serialize};

use crate::scoring::{
    self, MeldSummary, ScoringInput, ScoringRules, ALL_HONORS, ALL_PUNGS, FULL_FLUSH, HALF_FLUSH,
    LUXURY_SEVEN_PAIRS, SEVEN_PAIRS, THIRTEEN_ORPHANS,
};
use crate::shanten::{self, ShantenBreakdown};
use crate::shared::TILE_MASK_34;

/// High-fan Tuidaohu pattern direction achieved or realistically in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HandDirection {
    /// `十三幺` (4 fan).
    ThirteenOrphans,
    /// `字一色` (4 fan).
    AllHonors,
    /// `清一色` (2 fan).
    FullFlush,
    /// `豪华七对` (2 fan).
    LuxurySevenPairs,
    /// `混一色` (1 fan).
    HalfFlush,
    /// `碰碰胡` (1 fan).
    AllPungs,
    /// `七对` (1 fan).
    SevenPairs,
}

impl HandDirection {
    /// Canonical Chinese label for this high-fan hand pattern.
    #[must_use]
    pub const fn label_zh(self) -> &'static str {
        match self {
            Self::ThirteenOrphans => "十三幺",
            Self::AllHonors => "字一色",
            Self::FullFlush => "清一色",
            Self::LuxurySevenPairs => "豪华七对",
            Self::HalfFlush => "混一色",
            Self::AllPungs => "碰碰胡",
            Self::SevenPairs => "七对",
        }
    }
}

/// Exact waiting tile fact when a seat is in tenpai (`shanten == 0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitFact {
    /// The waiting tile that completes a legal winning hand.
    pub tile: Tile,
    /// Publicly visible remaining unseen copies (`0..=4`) derived strictly from [`PlayerView`].
    pub remaining_visible: u8,
    /// Intrinsic Tuidaohu capped winning fan on completing with `tile` (excluding situational draws).
    pub winning_fan: u8,
    /// Intrinsic Tuidaohu fan bitmask for completing with `tile`.
    pub fan_mask: u16,
}

/// Progression of a seat's hand relative to its previous [`HandFacts`] snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HandProgression {
    /// First analysis snapshot in the hand (no prior snapshot).
    Initial,
    /// Hand is already complete (`shanten == -1`, can win immediately).
    CanWin,
    /// Hand transitioned from non-tenpai (`shanten > 0`) into tenpai (`shanten == 0`).
    EnteredTenpai,
    /// Shanten strictly decreased, or stayed equal while visible ukeire strictly increased.
    Improved,
    /// Shanten and visible ukeire remained unchanged.
    Stalled,
    /// Shanten strictly increased, or stayed equal while visible ukeire decreased.
    Regressed,
}

/// Precomputed factual summary of one seat's hand, derived strictly from its own [`PlayerView`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandFacts {
    /// Minimum shanten across standard, seven pairs, and thirteen orphans (`-1` = complete, `0` = tenpai).
    pub shanten: i8,
    /// Standard (`4 melds + 1 pair`) shanten.
    pub standard_shanten: i8,
    /// Seven pairs (`七对` / `豪华七对`) shanten when `existing_melds == 0`, else `None`.
    pub seven_pairs_shanten: Option<i8>,
    /// Thirteen orphans (`十三幺`) shanten when `existing_melds == 0`, else `None`.
    pub thirteen_orphans_shanten: Option<i8>,
    /// Whether the hand is already a complete winning hand (`shanten == -1`).
    pub is_winning: bool,
    /// Whether the hand is in tenpai (`shanten == 0` with at least one legal winning wait).
    pub is_tenpai: bool,
    /// Whether `门前清` is currently intact (no open melds).
    pub is_menqing: bool,
    /// For a pre-discard (`% 3 == 2`) non-winning hand, the best discard achieving `shanten`
    /// with maximal visible ukeire (and highest winning fan on ties).
    pub best_discard: Option<Tile>,
    /// Total publicly visible remaining copies across all necessary/waiting tiles.
    pub ukeire: u16,
    /// Exact legal waiting tiles when `is_tenpai` (sorted by `Tile::index`).
    pub waits: Vec<WaitFact>,
    /// Achieved or realistically attainable high-fan pattern directions (sorted by priority).
    pub potential_directions: Vec<HandDirection>,
    /// Progression relative to `previous` snapshot.
    pub progression: HandProgression,
    /// Consecutive turns without strict shanten or ukeire improvement.
    pub stalled_streak: u8,
}

/// Analyzes a single seat's private + public [`PlayerView`] into exact [`HandFacts`].
///
/// Strictly read-only and synchronous; consumes only `view` (never any opponent hidden tiles)
/// and an optional `previous` snapshot from an earlier turn of the same hand.
#[must_use]
pub fn analyze_hand_facts(view: &PlayerView, previous: Option<&HandFacts>) -> HandFacts {
    let counts = shanten::hand_counts(view);
    let total_closed: usize = counts.iter().map(|&c| usize::from(c)).sum();
    let viewer_melds = &view.seats[view.viewer.index()].melds;
    let existing_melds = viewer_melds.len();
    let allow_quad = view.rule_profile.allow_four_of_kind_seven_pairs;
    let remaining = shanten::compute_remaining(view);
    let meld_summary = MeldSummary::from_meld_views(viewer_melds);
    let scoring_rules = ScoringRules::from_profile(&view.rule_profile);

    let expected_waiting = 4usize.saturating_sub(existing_melds) * 3 + 1;
    let expected_turn = expected_waiting + 1;

    if total_closed == expected_turn {
        analyze_turn_hand(
            &counts,
            existing_melds,
            allow_quad,
            &remaining,
            &meld_summary,
            scoring_rules,
            view.wall_remaining,
            previous,
        )
    } else if total_closed == expected_waiting {
        analyze_waiting_hand(
            &counts,
            existing_melds,
            allow_quad,
            &remaining,
            &meld_summary,
            scoring_rules,
            view.wall_remaining,
            previous,
        )
    } else {
        // Defensive fallback for irregular/partial test views (never panics).
        let (breakdown, mask) = shanten::necessary_tiles_mask(&counts, existing_melds, allow_quad);
        let ukeire = shanten::ukeire_from_mask(mask, &remaining);
        let potential_directions =
            detect_potential_directions(&counts, &meld_summary, breakdown, allow_quad);
        let (progression, stalled_streak) =
            compute_progression(breakdown.min, false, false, ukeire, previous);
        HandFacts {
            shanten: breakdown.min,
            standard_shanten: breakdown.standard,
            seven_pairs_shanten: breakdown.seven_pairs,
            thirteen_orphans_shanten: breakdown.thirteen_orphans,
            is_winning: false,
            is_tenpai: false,
            is_menqing: meld_summary.is_menqing,
            best_discard: None,
            ukeire,
            waits: Vec::new(),
            potential_directions,
            progression,
            stalled_streak,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn analyze_waiting_hand(
    counts: &[u8; Tile::COUNT],
    existing_melds: usize,
    allow_quad: bool,
    remaining: &[u8; Tile::COUNT],
    meld_summary: &MeldSummary,
    scoring_rules: ScoringRules,
    wall_remaining: u8,
    previous: Option<&HandFacts>,
) -> HandFacts {
    let (breakdown, mask) = shanten::necessary_tiles_mask(counts, existing_melds, allow_quad);
    let waits = if breakdown.min == 0 {
        collect_waits(
            counts,
            mask,
            remaining,
            meld_summary,
            scoring_rules,
            wall_remaining,
        )
    } else {
        Vec::new()
    };

    let is_tenpai = breakdown.min == 0 && !waits.is_empty();
    let ukeire = if is_tenpai {
        waits.iter().map(|w| u16::from(w.remaining_visible)).sum()
    } else {
        shanten::ukeire_from_mask(mask, remaining)
    };

    let potential_directions = if is_tenpai {
        directions_from_waits(&waits)
    } else {
        detect_potential_directions(counts, meld_summary, breakdown, allow_quad)
    };

    let (progression, stalled_streak) =
        compute_progression(breakdown.min, false, is_tenpai, ukeire, previous);

    HandFacts {
        shanten: breakdown.min,
        standard_shanten: breakdown.standard,
        seven_pairs_shanten: breakdown.seven_pairs,
        thirteen_orphans_shanten: breakdown.thirteen_orphans,
        is_winning: false,
        is_tenpai,
        is_menqing: meld_summary.is_menqing,
        best_discard: None,
        ukeire,
        waits,
        potential_directions,
        progression,
        stalled_streak,
    }
}

#[allow(clippy::too_many_arguments)]
fn analyze_turn_hand(
    counts: &[u8; Tile::COUNT],
    existing_melds: usize,
    allow_quad: bool,
    remaining: &[u8; Tile::COUNT],
    meld_summary: &MeldSummary,
    scoring_rules: ScoringRules,
    wall_remaining: u8,
    previous: Option<&HandFacts>,
) -> HandFacts {
    let breakdown = shanten::compute_shanten_breakdown(counts, existing_melds, allow_quad);

    // Check if the 14-3m hand is already a complete winning hand (`shanten == -1`).
    if breakdown.min == -1 {
        let win_input = ScoringInput {
            win_kind: WinKind::Tsumo,
            turn_source: Some(TurnSource::NormalDraw),
            wall_remaining: wall_remaining.max(1),
        };
        if let Some(win_eval) =
            scoring::evaluate_win(counts, meld_summary, win_input, scoring_rules)
        {
            let potential_directions = directions_from_fan_mask(win_eval.fan_mask);
            let (progression, stalled_streak) = compute_progression(-1, true, false, 0, previous);
            return HandFacts {
                shanten: -1,
                standard_shanten: breakdown.standard,
                seven_pairs_shanten: breakdown.seven_pairs,
                thirteen_orphans_shanten: breakdown.thirteen_orphans,
                is_winning: true,
                is_tenpai: false,
                is_menqing: meld_summary.is_menqing,
                best_discard: None,
                ukeire: 0,
                waits: Vec::new(),
                potential_directions,
                progression,
                stalled_streak,
            };
        }
    }

    // Evaluate every candidate discard to find the best post-discard state.
    let mut work = *counts;
    let mut best_choice: Option<PostDiscardCandidate> = None;

    for idx in 0..Tile::COUNT {
        if work[idx] == 0 {
            continue;
        }
        let tile = Tile::new(idx as u8).expect("idx bounded by Tile::COUNT");
        work[idx] -= 1;
        let (post_bd, post_mask) = shanten::necessary_tiles_mask(&work, existing_melds, allow_quad);
        let post_waits = if post_bd.min == 0 {
            collect_waits(
                &work,
                post_mask,
                remaining,
                meld_summary,
                scoring_rules,
                wall_remaining,
            )
        } else {
            Vec::new()
        };
        let post_tenpai = post_bd.min == 0 && !post_waits.is_empty();
        let post_ukeire: u16 = if post_tenpai {
            post_waits
                .iter()
                .map(|w| u16::from(w.remaining_visible))
                .sum()
        } else {
            shanten::ukeire_from_mask(post_mask, remaining)
        };
        let max_wait_fan = post_waits.iter().map(|w| w.winning_fan).max().unwrap_or(0);
        let post_counts = work;
        work[idx] += 1;

        let candidate = PostDiscardCandidate {
            discard: tile,
            breakdown: post_bd,
            is_tenpai: post_tenpai,
            ukeire: post_ukeire,
            max_wait_fan,
            waits: post_waits,
            post_counts,
        };

        let is_better = match &best_choice {
            None => true,
            Some(cur) => {
                // Lower shanten > tenpai with legal waits > higher ukeire > higher max fan.
                (
                    -candidate.breakdown.min,
                    candidate.is_tenpai,
                    candidate.ukeire,
                    candidate.max_wait_fan,
                ) > (
                    -cur.breakdown.min,
                    cur.is_tenpai,
                    cur.ukeire,
                    cur.max_wait_fan,
                )
            }
        };
        if is_better {
            best_choice = Some(candidate);
        }
    }

    let chosen = best_choice.expect("non-empty turn hand always has at least one discard");
    let potential_directions = if chosen.is_tenpai {
        directions_from_waits(&chosen.waits)
    } else {
        detect_potential_directions(
            &chosen.post_counts,
            meld_summary,
            chosen.breakdown,
            allow_quad,
        )
    };
    let (progression, stalled_streak) = compute_progression(
        chosen.breakdown.min,
        false,
        chosen.is_tenpai,
        chosen.ukeire,
        previous,
    );

    HandFacts {
        shanten: chosen.breakdown.min,
        standard_shanten: chosen.breakdown.standard,
        seven_pairs_shanten: chosen.breakdown.seven_pairs,
        thirteen_orphans_shanten: chosen.breakdown.thirteen_orphans,
        is_winning: false,
        is_tenpai: chosen.is_tenpai,
        is_menqing: meld_summary.is_menqing,
        best_discard: Some(chosen.discard),
        ukeire: chosen.ukeire,
        waits: chosen.waits,
        potential_directions,
        progression,
        stalled_streak,
    }
}

struct PostDiscardCandidate {
    discard: Tile,
    breakdown: ShantenBreakdown,
    is_tenpai: bool,
    ukeire: u16,
    max_wait_fan: u8,
    waits: Vec<WaitFact>,
    post_counts: [u8; Tile::COUNT],
}

fn collect_waits(
    closed_counts: &[u8; Tile::COUNT],
    necessary_mask: u64,
    remaining: &[u8; Tile::COUNT],
    meld_summary: &MeldSummary,
    scoring_rules: ScoringRules,
    wall_remaining: u8,
) -> Vec<WaitFact> {
    let mut waits = Vec::new();
    let mut bits = necessary_mask & TILE_MASK_34;
    let input = ScoringInput {
        win_kind: WinKind::Ron,
        turn_source: None,
        wall_remaining: wall_remaining.max(1),
    };

    while bits != 0 {
        let idx = bits.trailing_zeros() as usize;
        bits &= bits - 1;
        let tile = Tile::new(idx as u8).expect("bit index bounded by Tile::COUNT");
        if let Some(eval) = scoring::evaluate_win_with_extra(
            closed_counts,
            tile,
            meld_summary,
            input,
            scoring_rules,
        ) {
            waits.push(WaitFact {
                tile,
                remaining_visible: remaining[idx],
                winning_fan: eval.total_fan,
                fan_mask: eval.fan_mask,
            });
        }
    }

    waits
}

fn directions_from_fan_mask(mask: u16) -> Vec<HandDirection> {
    let mut out = Vec::new();
    if (mask & THIRTEEN_ORPHANS) != 0 {
        out.push(HandDirection::ThirteenOrphans);
    }
    if (mask & ALL_HONORS) != 0 {
        out.push(HandDirection::AllHonors);
    }
    if (mask & FULL_FLUSH) != 0 {
        out.push(HandDirection::FullFlush);
    }
    if (mask & LUXURY_SEVEN_PAIRS) != 0 {
        out.push(HandDirection::LuxurySevenPairs);
    }
    if (mask & HALF_FLUSH) != 0 {
        out.push(HandDirection::HalfFlush);
    }
    if (mask & ALL_PUNGS) != 0 {
        out.push(HandDirection::AllPungs);
    }
    if (mask & SEVEN_PAIRS) != 0 {
        out.push(HandDirection::SevenPairs);
    }
    out
}

fn directions_from_waits(waits: &[WaitFact]) -> Vec<HandDirection> {
    let mut combined_mask = 0u16;
    for w in waits {
        combined_mask |= w.fan_mask;
    }
    // If any wait upgrades to FullFlush or AllHonors or LuxurySevenPairs, keep the highest
    // form when all waits share it; if mixed waits exist (e.g., waiting on a suit tile for
    // FullFlush or an honor for HalfFlush), keeping both or deduplicating is faithful.
    let mut out = directions_from_fan_mask(combined_mask);
    if out.contains(&HandDirection::FullFlush)
        && waits.iter().all(|w| (w.fan_mask & FULL_FLUSH) != 0)
    {
        out.retain(|d| *d != HandDirection::HalfFlush);
    }
    if out.contains(&HandDirection::LuxurySevenPairs)
        && waits.iter().all(|w| (w.fan_mask & LUXURY_SEVEN_PAIRS) != 0)
    {
        out.retain(|d| *d != HandDirection::SevenPairs);
    }
    out
}

fn detect_potential_directions(
    closed_counts: &[u8; Tile::COUNT],
    meld_summary: &MeldSummary,
    breakdown: ShantenBreakdown,
    allow_quad: bool,
) -> Vec<HandDirection> {
    let mut out = Vec::new();

    // 1. Thirteen orphans (`十三幺`) — closed hand within 3 shanten.
    if meld_summary.count == 0 && breakdown.thirteen_orphans.is_some_and(|s| s <= 3) {
        out.push(HandDirection::ThirteenOrphans);
    }

    // Combined counts across closed tiles and declared melds for flush / honors / all-pungs checks.
    let mut total_counts = [0u8; Tile::COUNT];
    let mut total_tiles = 0u8;
    for idx in 0..Tile::COUNT {
        let c = closed_counts[idx].saturating_add(meld_summary.counts[idx]);
        total_counts[idx] = c;
        total_tiles = total_tiles.saturating_add(c);
    }

    let suit_counts = [
        total_counts[0..9].iter().copied().sum::<u8>(),
        total_counts[9..18].iter().copied().sum::<u8>(),
        total_counts[18..27].iter().copied().sum::<u8>(),
    ];
    let honor_count: u8 = total_counts[27..Tile::COUNT].iter().copied().sum();

    let meld_suits = [
        meld_summary.counts[0..9].iter().any(|&c| c > 0),
        meld_summary.counts[9..18].iter().any(|&c| c > 0),
        meld_summary.counts[18..27].iter().any(|&c| c > 0),
    ];
    let meld_has_honor = meld_summary.counts[27..Tile::COUNT].iter().any(|&c| c > 0);
    let meld_suit_kinds = meld_suits.iter().filter(|&&present| present).count();

    // 2. All honors (`字一色`): no number-suit melds and >= 10 honors.
    if meld_suit_kinds == 0 && total_tiles >= 10 && honor_count >= 10 {
        out.push(HandDirection::AllHonors);
    } else if meld_suit_kinds <= 1 && total_tiles >= 10 {
        // 3. Full flush (`清一色`) vs Half flush (`混一色`).
        let mut best_suit: Option<(usize, u8)> = None;
        for (s_idx, &cnt) in suit_counts.iter().enumerate() {
            let compatible_with_melds = meld_suit_kinds == 0 || meld_suits[s_idx];
            if compatible_with_melds && best_suit.is_none_or(|(_, best)| cnt > best) {
                best_suit = Some((s_idx, cnt));
            }
        }
        if let Some((_, best_cnt)) = best_suit {
            let off_full = total_tiles.saturating_sub(best_cnt);
            let off_half = total_tiles.saturating_sub(best_cnt.saturating_add(honor_count));
            if !meld_has_honor && best_cnt >= 10 && off_full <= 3 {
                out.push(HandDirection::FullFlush);
            } else if best_cnt >= 7 && honor_count >= 1 && off_half <= 2 {
                out.push(HandDirection::HalfFlush);
            }
        }
    }

    // 4. Seven pairs / Luxury seven pairs (`豪华七对` / `七对`) — closed hand within 2 shanten.
    if meld_summary.count == 0 && breakdown.seven_pairs.is_some_and(|s| s <= 2) {
        let has_quad = closed_counts.contains(&4);
        if allow_quad && has_quad {
            out.push(HandDirection::LuxurySevenPairs);
        } else {
            out.push(HandDirection::SevenPairs);
        }
    }

    // 5. All pungs (`碰碰胡`) — no Chi melds, strong triplet/pair concentration.
    if meld_summary.all_pungs_or_kongs {
        let closed_triplets = closed_counts.iter().filter(|&&c| c >= 3).count() as u8;
        let closed_pairs = closed_counts.iter().filter(|&&c| c == 2).count() as u8;
        let total_triplets = meld_summary.count.saturating_add(closed_triplets);
        let total_blocks = total_triplets.saturating_add(closed_pairs);
        if (total_triplets >= 2 && total_blocks >= 4)
            || (total_triplets >= 3 && breakdown.standard <= 2)
        {
            out.push(HandDirection::AllPungs);
        }
    }

    out.sort();
    out.dedup();
    out
}

fn compute_progression(
    shanten: i8,
    is_winning: bool,
    is_tenpai: bool,
    ukeire: u16,
    previous: Option<&HandFacts>,
) -> (HandProgression, u8) {
    if is_winning {
        return (HandProgression::CanWin, 0);
    }
    let Some(prev) = previous else {
        return (HandProgression::Initial, 0);
    };

    if is_tenpai && !prev.is_tenpai {
        return (HandProgression::EnteredTenpai, 0);
    }

    if shanten < prev.shanten {
        return (HandProgression::Improved, 0);
    }
    if shanten > prev.shanten {
        return (
            HandProgression::Regressed,
            prev.stalled_streak.saturating_add(1),
        );
    }

    // Same shanten: compare visible ukeire.
    if ukeire > prev.ukeire {
        (HandProgression::Improved, 0)
    } else if ukeire < prev.ukeire {
        (
            HandProgression::Regressed,
            prev.stalled_streak.saturating_add(1),
        )
    } else {
        (
            HandProgression::Stalled,
            prev.stalled_streak.saturating_add(1),
        )
    }
}

#[cfg(test)]
mod tests {
    use mahjong_core::{
        DiscardEntry, MatchMode, MeldView, PlayerHandView, PlayerView, PublicPhase, PublicSeatView,
        Ruleset, Seat, Tile, TuidaohuRuleset, TurnView, Wind,
    };

    use super::*;
    use crate::test_common::{tile, tiles};

    fn make_view(
        closed: &str,
        drawn: Option<&str>,
        own_melds: Vec<MeldView>,
        rivers: [Vec<DiscardEntry>; 4],
        other_melds: [Vec<MeldView>; 4],
        allow_quad_7p: bool,
    ) -> PlayerView {
        let mut profile = TuidaohuRuleset.profile();
        profile.allow_four_of_kind_seven_pairs = allow_quad_7p;
        let viewer = Seat::South;
        let closed_tiles = tiles(closed);
        let drawn_tile = drawn.map(tile);
        let mut seats = Seat::ALL.map(|seat| PublicSeatView {
            seat,
            score: 0,
            hand_count: 13,
            river: Vec::new(),
            melds: Vec::new(),
            last_draw_or_meld_seq: 0,
        });
        for seat in Seat::ALL {
            seats[seat.index()].river = rivers[seat.index()].clone();
            seats[seat.index()].melds = other_melds[seat.index()].clone();
        }
        seats[viewer.index()].melds = own_melds;
        seats[viewer.index()].hand_count =
            (closed_tiles.len() + usize::from(drawn_tile.is_some())) as u8;

        PlayerView {
            viewer,
            mode: MatchMode::EastOnly,
            rule_profile: profile,
            phase: PublicPhase::Playing,
            hand: PlayerHandView {
                closed_tiles,
                drawn_tile,
            },
            seats,
            wall_remaining: 40,
            turn: TurnView {
                wind_round: Wind::East,
                hand_number: 1,
                dealer_streak: 0,
                dealer: Seat::East,
                active_seat: Some(viewer),
            },
            legal_actions: Vec::new(),
            last_settlement: None,
            ready_seats: None,
            match_summary: None,
            seq: 10,
        }
    }

    fn discard(t: &str, claimed_by: Option<Seat>) -> DiscardEntry {
        DiscardEntry {
            tile: tile(t),
            is_tsumogiri: false,
            claimed_by,
            seq: 1,
        }
    }

    fn empty_rivers() -> [Vec<DiscardEntry>; 4] {
        [Vec::new(), Vec::new(), Vec::new(), Vec::new()]
    }

    fn empty_melds() -> [Vec<MeldView>; 4] {
        [Vec::new(), Vec::new(), Vec::new(), Vec::new()]
    }

    #[test]
    fn standard_multi_wait_tenpai_computes_exact_waits_visible_remaining_and_fan() {
        // 123m 456m 789p 23s 55p -> ryanmen tenpai waiting on 1s and 4s (门前清 = 1 fan).
        // Put 2x 1s in East's unclaimed river, 1x 1s claimed (must NOT double-count!),
        // and a West Pon of 4s (3 copies visible -> 1 copy remaining).
        let mut rivers = empty_rivers();
        rivers[Seat::East.index()] = vec![
            discard("1s", None),
            discard("1s", None),
            discard("1s", Some(Seat::North)),
        ];
        let mut other_melds = empty_melds();
        other_melds[Seat::West.index()] = vec![MeldView::Pon {
            tile: tile("4s"),
            from_seat: Seat::North,
            seq: 4,
        }];
        // Opponent ConcealedKong with tile: None must be ignored cleanly.
        other_melds[Seat::North.index()] = vec![MeldView::ConcealedKong { tile: None, seq: 5 }];

        let view = make_view(
            "123m 456m 789p 23s 55p",
            None,
            Vec::new(),
            rivers,
            other_melds,
            true,
        );
        let facts = analyze_hand_facts(&view, None);

        assert_eq!(facts.shanten, 0);
        assert!(facts.is_tenpai);
        assert!(!facts.is_winning);
        assert!(facts.is_menqing);
        assert_eq!(facts.best_discard, None);
        assert_eq!(facts.waits.len(), 2);

        // 1s: 4 - 2 unclaimed = 2 remaining; 4s: 4 - 3 Pon = 1 remaining.
        assert_eq!(facts.waits[0].tile, tile("1s"));
        assert_eq!(facts.waits[0].remaining_visible, 2);
        assert_eq!(facts.waits[0].winning_fan, 1); // 门前清
        assert_eq!(facts.waits[1].tile, tile("4s"));
        assert_eq!(facts.waits[1].remaining_visible, 1);
        assert_eq!(facts.waits[1].winning_fan, 1);
        assert_eq!(facts.ukeire, 3);
        assert_eq!(facts.progression, HandProgression::Initial);
        assert_eq!(facts.stalled_streak, 0);
    }

    #[test]
    fn dead_wait_tenpai_reports_zero_remaining_visible_copies() {
        // Waiting on 5p tanki, while all 4 remaining? Wait: we hold 1x 5p, rivers have 3x 5p -> 0 left!
        let mut rivers = empty_rivers();
        rivers[Seat::East.index()] = vec![discard("5p", None), discard("5p", None)];
        rivers[Seat::West.index()] = vec![discard("5p", None)];

        let view = make_view(
            "123m 456m 789s 234s 5p",
            None,
            Vec::new(),
            rivers,
            empty_melds(),
            true,
        );
        let facts = analyze_hand_facts(&view, None);
        assert!(facts.is_tenpai);
        assert_eq!(facts.waits.len(), 1);
        assert_eq!(facts.waits[0].tile, tile("5p"));
        assert_eq!(facts.waits[0].remaining_visible, 0);
        assert_eq!(facts.ukeire, 0);
    }

    #[test]
    fn seven_pairs_and_luxury_seven_pairs_respect_allow_quad_rule() {
        // 1111m 22m 33p 44p 55s 7s -> 6 pairs (quad counts as 2 pairs) waiting on 7s for 豪华七对!
        let view_allow = make_view(
            "1111m 22m 33p 44p 55s 7s",
            None,
            Vec::new(),
            empty_rivers(),
            empty_melds(),
            true,
        );
        let facts_allow = analyze_hand_facts(&view_allow, None);
        assert_eq!(facts_allow.shanten, 0);
        assert_eq!(facts_allow.seven_pairs_shanten, Some(0));
        assert!(facts_allow.is_tenpai);
        assert_eq!(facts_allow.waits.len(), 1);
        assert_eq!(facts_allow.waits[0].tile, tile("7s"));
        assert_eq!(facts_allow.waits[0].remaining_visible, 3);
        assert_eq!(facts_allow.waits[0].winning_fan, 2); // 豪华七对 (2 fan, excludes 门前清)
        assert_eq!(
            facts_allow.potential_directions,
            vec![HandDirection::LuxurySevenPairs]
        );

        // Plain 7 pairs: 11m 44m 22p 55p 88s 33s E -> waiting on E (1 fan 七对).
        let view_plain = make_view(
            "11m 44m 22p 55p 88s 33s E",
            None,
            Vec::new(),
            empty_rivers(),
            empty_melds(),
            true,
        );
        let facts_plain = analyze_hand_facts(&view_plain, None);
        assert_eq!(facts_plain.shanten, 0);
        assert_eq!(facts_plain.seven_pairs_shanten, Some(0));
        assert_eq!(facts_plain.waits.len(), 1);
        assert_eq!(facts_plain.waits[0].tile, tile("E"));
        assert_eq!(facts_plain.waits[0].winning_fan, 1);
        assert_eq!(
            facts_plain.potential_directions,
            vec![HandDirection::SevenPairs]
        );

        // Same quad hand with allow_quad_7p == false is NOT tenpai for 7 pairs!
        let view_disallow = make_view(
            "1111m 22m 33p 44p 55s 7s",
            None,
            Vec::new(),
            empty_rivers(),
            empty_melds(),
            false,
        );
        let facts_disallow = analyze_hand_facts(&view_disallow, None);
        assert!(facts_disallow.seven_pairs_shanten.unwrap() > 0);
        assert!(!facts_disallow.is_tenpai);
    }

    #[test]
    fn thirteen_orphans_13_wait_and_1_shanten_detection() {
        // Classic 13-wait 十三幺!
        let view_13w = make_view(
            "19m 19s 19p E S W N C F B",
            None,
            Vec::new(),
            empty_rivers(),
            empty_melds(),
            true,
        );
        let facts_13w = analyze_hand_facts(&view_13w, None);
        assert_eq!(facts_13w.shanten, 0);
        assert_eq!(facts_13w.thirteen_orphans_shanten, Some(0));
        assert!(facts_13w.is_tenpai);
        assert_eq!(facts_13w.waits.len(), 13);
        assert!(facts_13w.waits.iter().all(|w| w.winning_fan == 4));
        assert!(facts_13w.waits.iter().all(|w| w.remaining_visible == 3));
        assert_eq!(facts_13w.ukeire, 39);
        assert_eq!(
            facts_13w.potential_directions,
            vec![HandDirection::ThirteenOrphans]
        );

        // 1-shanten 十三幺 (missing B, holding extra 5m).
        let view_1sh = make_view(
            "19m 19s 19p E S W N C F 5m",
            None,
            Vec::new(),
            empty_rivers(),
            empty_melds(),
            true,
        );
        let facts_1sh = analyze_hand_facts(&view_1sh, None);
        assert_eq!(facts_1sh.shanten, 1);
        assert_eq!(facts_1sh.thirteen_orphans_shanten, Some(1));
        assert!(!facts_1sh.is_tenpai);
        assert!(facts_1sh
            .potential_directions
            .contains(&HandDirection::ThirteenOrphans));
    }

    #[test]
    fn fourteen_tile_turn_hand_handles_winning_and_pre_discard_tenpai() {
        // 1. Already winning 14-tile 清一色 hand (`111m 222m 333m 444m 5m` + drawn `5m`).
        let view_win = make_view(
            "111m 222m 333m 444m 5m",
            Some("5m"),
            Vec::new(),
            empty_rivers(),
            empty_melds(),
            true,
        );
        let facts_win = analyze_hand_facts(&view_win, None);
        assert_eq!(facts_win.shanten, -1);
        assert!(facts_win.is_winning);
        assert!(!facts_win.is_tenpai);
        assert_eq!(facts_win.progression, HandProgression::CanWin);
        assert!(facts_win
            .potential_directions
            .contains(&HandDirection::FullFlush));
        assert!(facts_win
            .potential_directions
            .contains(&HandDirection::AllPungs));

        // 2. 14-tile hand ready to enter tenpai after discarding `E`:
        //    `123m 456m 789p 23s 55p` + drawn `E` -> best discard is `E`, waits on `1s` & `4s`!
        let view_pre_tenpai = make_view(
            "123m 456m 789p 23s 55p",
            Some("E"),
            Vec::new(),
            empty_rivers(),
            empty_melds(),
            true,
        );
        let facts_pre = analyze_hand_facts(&view_pre_tenpai, None);
        assert_eq!(facts_pre.shanten, 0);
        assert!(facts_pre.is_tenpai);
        assert!(!facts_pre.is_winning);
        assert_eq!(facts_pre.best_discard, Some(tile("E")));
        let wait_tiles: Vec<Tile> = facts_pre.waits.iter().map(|w| w.tile).collect();
        assert_eq!(wait_tiles, vec![tile("1s"), tile("4s")]);
        assert_eq!(facts_pre.ukeire, 8);
    }

    #[test]
    fn big_hand_directions_detect_full_flush_half_flush_all_honors_and_all_pungs() {
        // 1-shanten 清一色 (11万 + 2条)
        let view_full = make_view(
            "123m 456m 78m 99m 2m 4s 8s",
            None,
            Vec::new(),
            empty_rivers(),
            empty_melds(),
            true,
        );
        let facts_full = analyze_hand_facts(&view_full, None);
        assert!(facts_full
            .potential_directions
            .contains(&HandDirection::FullFlush));
        assert!(!facts_full
            .potential_directions
            .contains(&HandDirection::HalfFlush));

        // 1-shanten 混一色 (with Pon of 東)
        let view_half = make_view(
            "123p 45p 78p 99p 2s",
            None,
            vec![MeldView::Pon {
                tile: tile("E"),
                from_seat: Seat::East,
                seq: 1,
            }],
            empty_rivers(),
            empty_melds(),
            true,
        );
        let facts_half = analyze_hand_facts(&view_half, None);
        assert!(!facts_half.is_menqing);
        assert!(facts_half
            .potential_directions
            .contains(&HandDirection::HalfFlush));

        // Open 碰碰胡 tenpai (Pon 9m + 111m 222p 33s 44s -> waits on 3s, 4s)
        let view_pung = make_view(
            "111m 222p 33s 44s",
            None,
            vec![MeldView::Pon {
                tile: tile("9m"),
                from_seat: Seat::West,
                seq: 1,
            }],
            empty_rivers(),
            empty_melds(),
            true,
        );
        let facts_pung = analyze_hand_facts(&view_pung, None);
        assert!(facts_pung.is_tenpai);
        assert_eq!(
            facts_pung.potential_directions,
            vec![HandDirection::AllPungs]
        );
        assert_eq!(facts_pung.waits.len(), 2);

        // 字一色 tenpai (Pon E + SSS WWW NNN C -> waiting on C, 4 fan)
        let view_honors = make_view(
            "SSS WWW NNN C",
            None,
            vec![MeldView::Pon {
                tile: tile("E"),
                from_seat: Seat::North,
                seq: 1,
            }],
            empty_rivers(),
            empty_melds(),
            true,
        );
        let facts_honors = analyze_hand_facts(&view_honors, None);
        assert!(facts_honors.is_tenpai);
        assert!(facts_honors
            .potential_directions
            .contains(&HandDirection::AllHonors));
        assert_eq!(facts_honors.waits[0].winning_fan, 4);
    }

    #[test]
    fn progression_tracks_stalled_streak_improvement_tenpai_and_regression() {
        // Turn 0: 2-shanten hand
        let v0 = make_view(
            "12m 45m 78p 23s 55p E S W",
            None,
            Vec::new(),
            empty_rivers(),
            empty_melds(),
            true,
        );
        let f0 = analyze_hand_facts(&v0, None);
        assert_eq!(f0.progression, HandProgression::Initial);
        assert_eq!(f0.stalled_streak, 0);
        assert!(f0.shanten >= 1);

        // Turn 1: identical hand -> Stalled, streak = 1
        let f1 = analyze_hand_facts(&v0, Some(&f0));
        assert_eq!(f1.progression, HandProgression::Stalled);
        assert_eq!(f1.stalled_streak, 1);

        // Turn 2: identical hand -> Stalled, streak = 2
        let f2 = analyze_hand_facts(&v0, Some(&f1));
        assert_eq!(f2.progression, HandProgression::Stalled);
        assert_eq!(f2.stalled_streak, 2);

        // Turn 3: shanten drops from 3 -> 1 -> Improved, streak resets to 0
        let v_imp = make_view(
            "123m 456m 78p 23s 55p E S",
            None,
            Vec::new(),
            empty_rivers(),
            empty_melds(),
            true,
        );
        let f3 = analyze_hand_facts(&v_imp, Some(&f2));
        assert_eq!(f3.shanten, 1);
        assert_eq!(f3.progression, HandProgression::Improved);
        assert_eq!(f3.stalled_streak, 0);

        // Turn 4: enters tenpai -> EnteredTenpai, streak = 0
        let v_tenpai = make_view(
            "123m 456m 789p 23s 55p",
            None,
            Vec::new(),
            empty_rivers(),
            empty_melds(),
            true,
        );
        let f4 = analyze_hand_facts(&v_tenpai, Some(&f3));
        assert_eq!(f4.shanten, 0);
        assert_eq!(f4.progression, HandProgression::EnteredTenpai);
        assert_eq!(f4.stalled_streak, 0);

        // Turn 5: folds back to 1-shanten -> Regressed, streak = 1
        let f5 = analyze_hand_facts(&v_imp, Some(&f4));
        assert_eq!(f5.progression, HandProgression::Regressed);
        assert_eq!(f5.stalled_streak, 1);
    }
}
