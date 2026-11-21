use std::cell::RefCell;
use std::rc::Rc;

use mahjong_core::{
    project_event, project_view, AnyRuleset, HandOutcome, Match, MatchMode, MeldView, PlayerAction,
    PlayerEvent, PlayerEventKind, PlayerView, PublicPhase, RevealedMeld, Ruleset, Seat,
    SeededWallProvider, Tile, TuidaohuRuleset, WallProvider,
};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// Wraps `SeededWallProvider` via the public `WallProvider` trait to record each
/// 136-tile wall so the fuzzer can assert exact `[4u8; 34]` multiset conservation
/// at `HandSettled` without touching any private `Match` state.
struct RecordingWallProvider {
    inner: SeededWallProvider,
    latest_wall: Rc<RefCell<Vec<Tile>>>,
}

impl RecordingWallProvider {
    fn new(seed: u64, latest_wall: Rc<RefCell<Vec<Tile>>>) -> Self {
        Self {
            inner: SeededWallProvider::new(seed),
            latest_wall,
        }
    }
}

impl WallProvider for RecordingWallProvider {
    fn initial_dealer(&mut self) -> Seat {
        self.inner.initial_dealer()
    }

    fn next_wall(&mut self, deck: &[Tile]) -> Vec<Tile> {
        let wall = self.inner.next_wall(deck);
        *self.latest_wall.borrow_mut() = wall.clone();
        wall
    }
}

struct HandWallTracker {
    wall: Vec<Tile>,
    head: usize,
    tail: usize,
}

impl HandWallTracker {
    fn from_wall(wall: Vec<Tile>) -> Self {
        assert_eq!(wall.len(), Tile::DECK_SIZE);
        Self {
            wall,
            head: 53,
            tail: Tile::DECK_SIZE,
        }
    }

    fn record_draw(&mut self, drawn: Option<Tile>, is_replacement: bool, expected_rem: u8) {
        let actual = if is_replacement {
            self.tail -= 1;
            self.wall[self.tail]
        } else {
            let t = self.wall[self.head];
            self.head += 1;
            t
        };
        if let Some(tile) = drawn {
            assert_eq!(tile, actual, "drawn tile must match wall order");
        }
        assert_eq!(
            self.tail - self.head,
            usize::from(expected_rem),
            "tracked wall_remaining mismatch"
        );
    }

    fn remaining_slice(&self) -> &[Tile] {
        &self.wall[self.head..self.tail]
    }
}

#[derive(Default)]
struct FuzzStats {
    east_only_hands: usize,
    free_table_hands: usize,
    wins: usize,
    draw_games: usize,
    exposed_kongs: usize,
    added_kongs: usize,
    concealed_kongs: usize,
    pons_declared: usize,
    actions_applied: usize,
}

fn pick_rng_index(rng: &mut ChaCha8Rng, len: usize) -> usize {
    (rng.next_u32() as usize) % len
}

fn choose_playing_action(legal: &[PlayerAction], rng: &mut ChaCha8Rng) -> PlayerAction {
    debug_assert!(!legal.is_empty());
    let roll = rng.next_u32() % 100;

    // 75% of the time take wins immediately when available; 25% allow uniform random (including Pass)
    // so over-water locks and multi-turn paths are also exercised.
    if roll < 75 {
        if let Some(win) = legal.iter().copied().find(|a| {
            matches!(
                a,
                PlayerAction::Tsumo | PlayerAction::Ron { .. } | PlayerAction::RobKongHu { .. }
            )
        }) {
            return win;
        }
    }

    // 60% of the time prefer kongs/pons so kongs, AfterMeld, ReplacementDraw, and RobKongHu
    // occur frequently alongside normal discards and Pass.
    if roll < 60 {
        let claim_or_kong: Vec<PlayerAction> = legal
            .iter()
            .copied()
            .filter(|a| {
                matches!(
                    a,
                    PlayerAction::ConcealedKong { .. }
                        | PlayerAction::AddedKong { .. }
                        | PlayerAction::ExposedKong { .. }
                        | PlayerAction::Pon { .. }
                )
            })
            .collect();
        if !claim_or_kong.is_empty() {
            return claim_or_kong[pick_rng_index(rng, claim_or_kong.len())];
        }
    }

    legal[pick_rng_index(rng, legal.len())]
}

fn assert_views_invariants(
    views: &[PlayerView; 4],
    expected_total_score: i32,
    expected_seq: u64,
    prev_draw_or_meld_seqs: &mut [u64; 4],
) {
    let reference = &views[0];
    assert_eq!(reference.seq, expected_seq);

    // Zero-sum score conservation across all 4 seats.
    let score_sum: i32 = reference.seats.iter().map(|s| s.score).sum();
    assert_eq!(
        score_sum, expected_total_score,
        "score sum drifted from {expected_total_score}"
    );

    // Never expose internal reaction-waiting phases.
    assert!(matches!(
        reference.phase,
        PublicPhase::Playing | PublicPhase::BetweenHands | PublicPhase::MatchFinished
    ));

    // Zero deadlock / terminal emptiness check.
    let any_legal = views.iter().any(|v| !v.legal_actions.is_empty());
    if reference.phase == PublicPhase::MatchFinished {
        assert!(
            !any_legal,
            "MatchFinished must expose zero legal actions to all seats"
        );
    } else {
        assert!(
            any_legal,
            "deadlock: zero legal actions while phase is {:?}",
            reference.phase
        );
    }

    // Check 136-tile conservation & per-seat privacy/consistency invariants for ALL 4 views.
    for (idx, view) in views.iter().enumerate() {
        let viewer = Seat::from_index(idx).unwrap();
        assert_eq!(view.viewer, viewer);
        assert_eq!(view.seq, expected_seq);
        assert_eq!(view.phase, reference.phase);
        assert_eq!(view.turn, reference.turn);
        assert_eq!(view.wall_remaining, reference.wall_remaining);
        assert_eq!(view.ready_seats, reference.ready_seats);
        assert_eq!(view.last_settlement, reference.last_settlement);
        assert_eq!(view.match_summary, reference.match_summary);

        // Viewer's own hand invariants.
        assert!(
            view.hand.closed_tiles.windows(2).all(|w| w[0] <= w[1]),
            "viewer closed_tiles must stay sorted"
        );
        let viewer_seat_view = &view.seats[idx];
        assert_eq!(
            usize::from(viewer_seat_view.hand_count),
            view.hand.closed_tiles.len() + usize::from(view.hand.drawn_tile.is_some()),
            "viewer hand_count must equal closed_tiles.len() + drawn_tile.is_some()"
        );

        let mut total_hands = 0usize;
        let mut total_unclaimed_river = 0usize;
        let mut total_melds = 0usize;

        for seat_view in &view.seats {
            total_hands += usize::from(seat_view.hand_count);
            total_unclaimed_river += seat_view
                .river
                .iter()
                .filter(|d| d.claimed_by.is_none())
                .count();
            for meld in &seat_view.melds {
                match meld {
                    MeldView::Pon { .. } | MeldView::Chi { .. } => total_melds += 3,
                    MeldView::ExposedKong { .. } | MeldView::AddedKong { .. } => total_melds += 4,
                    MeldView::ConcealedKong { tile, .. } => {
                        total_melds += 4;
                        if seat_view.seat == viewer {
                            assert!(
                                tile.is_some(),
                                "owner must see concrete tile of ConcealedKong"
                            );
                        } else {
                            assert_eq!(
                                *tile, None,
                                "opponent ConcealedKong must project tile: None"
                            );
                        }
                    }
                }
            }

            let s_idx = seat_view.seat.index();
            assert!(
                seat_view.last_draw_or_meld_seq <= view.seq,
                "last_draw_or_meld_seq ({}) > view.seq ({})",
                seat_view.last_draw_or_meld_seq,
                view.seq
            );
            assert!(
                seat_view.last_draw_or_meld_seq >= prev_draw_or_meld_seqs[s_idx],
                "last_draw_or_meld_seq regressed for {:?}: {} < {}",
                seat_view.seat,
                seat_view.last_draw_or_meld_seq,
                prev_draw_or_meld_seqs[s_idx]
            );
        }

        assert_eq!(
            total_hands + total_unclaimed_river + total_melds + usize::from(view.wall_remaining),
            Tile::DECK_SIZE,
            "136-tile conservation violated in {:?}'s view",
            viewer
        );
    }

    for seat_view in &reference.seats {
        prev_draw_or_meld_seqs[seat_view.seat.index()] = seat_view.last_draw_or_meld_seq;
    }
}

fn assert_settled_multiset_136(
    view: &PlayerView,
    remaining_wall: &[Tile],
    expected_total_score: i32,
) {
    let settlement = view
        .last_settlement
        .as_ref()
        .expect("settlement must be present at HandSettled");
    assert_eq!(
        settlement.score_deltas.iter().sum::<i32>(),
        0,
        "hand score_deltas must sum to 0"
    );
    assert_eq!(
        settlement.new_scores.iter().sum::<i32>(),
        expected_total_score
    );
    for (idx, seat_view) in view.seats.iter().enumerate() {
        assert_eq!(seat_view.score, settlement.new_scores[idx]);
    }

    let mut counts = [0u8; Tile::COUNT];
    for revealed in &settlement.revealed_hands {
        for &tile in &revealed.closed_tiles {
            counts[tile.index()] += 1;
        }
        if let Some(drawn) = revealed.drawn_tile {
            counts[drawn.index()] += 1;
        }
        for meld in &revealed.melds {
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
    }
    for seat_view in &view.seats {
        for entry in &seat_view.river {
            if entry.claimed_by.is_none() {
                counts[entry.tile.index()] += 1;
            }
        }
    }
    for &tile in remaining_wall {
        counts[tile.index()] += 1;
    }

    assert_eq!(
        counts,
        [4u8; Tile::COUNT],
        "exact [4u8; 34] multiset conservation violated at HandSettled"
    );
}

fn play_fuzz_match<R: Ruleset>(
    mode: MatchMode,
    ruleset: R,
    wall_seed: u64,
    action_seed: u64,
    free_table_target_hands: usize,
    stats: &mut FuzzStats,
) -> Vec<Tile> {
    let expected_total_score = match mode {
        MatchMode::EastOnly => 400,
        MatchMode::FreeTable => 0,
    };
    let mut rng = ChaCha8Rng::seed_from_u64(action_seed);
    let latest_wall = Rc::new(RefCell::new(Vec::new()));
    let provider = RecordingWallProvider::new(wall_seed, Rc::clone(&latest_wall));

    let mut game = Match::new(mode, ruleset, provider).expect("match initialization succeeds");
    let mut tracker = HandWallTracker::from_wall(latest_wall.borrow().clone());
    let mut prev_seq = 0u64;
    let mut prev_draw_or_meld = [0u64; 4];

    let process_events = |events: &[mahjong_core::RecordedEvent],
                          prev_seq: &mut u64,
                          prev_draw_or_meld: &mut [u64; 4],
                          expected_draw_or_meld: &mut [u64; 4],
                          tracker: &mut HandWallTracker,
                          stats: &mut FuzzStats| {
        for ev in events {
            assert_eq!(ev.seq(), *prev_seq + 1, "event seq must increment by +1");
            *prev_seq = ev.seq();
            assert!(!format!("{ev:?}").contains("closed_tiles"));

            for seat in Seat::ALL {
                let pe = project_event(ev, seat);
                assert_eq!(pe.seq, ev.seq());
                match &pe.kind {
                    PlayerEventKind::HandStarted { hand, .. } => {
                        assert!(hand.closed_tiles.windows(2).all(|w| w[0] <= w[1]));
                        assert_eq!(hand.drawn_tile, None);
                    }
                    PlayerEventKind::TileDrawn {
                        seat: drawer, tile, ..
                    } => {
                        if *drawer == seat {
                            assert!(tile.is_some(), "drawer must receive drawn tile");
                        } else {
                            assert_eq!(*tile, None, "opponent TileDrawn must project tile: None");
                        }
                    }
                    PlayerEventKind::MeldDeclared {
                        seat: actor,
                        meld: MeldView::ConcealedKong { tile, .. },
                        ..
                    } => {
                        if *actor == seat {
                            assert!(tile.is_some(), "owner must see ConcealedKong tile");
                        } else {
                            assert_eq!(
                                *tile, None,
                                "opponent ConcealedKong event must project tile: None"
                            );
                        }
                    }
                    _ => {}
                }
            }

            // Update wall tracker / expected last_draw_or_meld_seq / stats.
            let pe_east = project_event(ev, Seat::East);
            match pe_east.kind {
                PlayerEventKind::HandStarted { .. } => {
                    if ev.seq() > 2 {
                        *tracker = HandWallTracker::from_wall(latest_wall.borrow().clone());
                    }
                    *prev_draw_or_meld = [0u64; 4];
                    *expected_draw_or_meld = [0u64; 4];
                }
                PlayerEventKind::TileDrawn {
                    seat: drawer,
                    is_replacement,
                    wall_remaining,
                    ..
                } => {
                    let drawn_tile = match project_event(ev, drawer).kind {
                        PlayerEventKind::TileDrawn { tile, .. } => tile,
                        _ => unreachable!(),
                    };
                    tracker.record_draw(drawn_tile, is_replacement, wall_remaining);
                    expected_draw_or_meld[drawer.index()] = ev.seq();
                }
                PlayerEventKind::MeldDeclared { seat, meld, .. } => {
                    expected_draw_or_meld[seat.index()] = ev.seq();
                    match meld {
                        MeldView::Pon { .. } => stats.pons_declared += 1,
                        MeldView::ExposedKong { .. } => stats.exposed_kongs += 1,
                        MeldView::AddedKong { .. } => stats.added_kongs += 1,
                        MeldView::ConcealedKong { .. } => stats.concealed_kongs += 1,
                        MeldView::Chi { .. } => {}
                    }
                }
                PlayerEventKind::HandSettled { settlement } => {
                    match mode {
                        MatchMode::EastOnly => stats.east_only_hands += 1,
                        MatchMode::FreeTable => stats.free_table_hands += 1,
                    }
                    match settlement.outcome {
                        HandOutcome::Win { .. } => stats.wins += 1,
                        HandOutcome::DrawGame => stats.draw_games += 1,
                    }
                }
                _ => {}
            }
        }
    };

    let mut expected_draw_or_meld = [0u64; 4];
    process_events(
        game.events(),
        &mut prev_seq,
        &mut prev_draw_or_meld,
        &mut expected_draw_or_meld,
        &mut tracker,
        stats,
    );

    let mut views = Seat::ALL.map(|s| project_view(&game, s));
    assert_views_invariants(
        &views,
        expected_total_score,
        prev_seq,
        &mut prev_draw_or_meld,
    );
    assert_eq!(
        views[0].seats.each_ref().map(|s| s.last_draw_or_meld_seq),
        expected_draw_or_meld
    );

    let mut hands_in_this_match = 0usize;
    loop {
        let phase = views[0].phase;
        if phase == PublicPhase::MatchFinished {
            let summary = views[0]
                .match_summary
                .expect("MatchSummary present at MatchFinished");
            assert_eq!(
                summary.final_scores.iter().sum::<i32>(),
                expected_total_score
            );
            assert_eq!(summary.net_deltas.iter().sum::<i32>(), 0);
            break;
        }

        let (actor, action) = match phase {
            PublicPhase::Playing => {
                let mut candidates: Vec<usize> = (0..4)
                    .filter(|&idx| !views[idx].legal_actions.is_empty())
                    .collect();
                assert!(!candidates.is_empty());
                let chosen_idx = candidates.swap_remove(pick_rng_index(&mut rng, candidates.len()));
                let seat = Seat::from_index(chosen_idx).unwrap();
                let act = choose_playing_action(&views[chosen_idx].legal_actions, &mut rng);
                (seat, act)
            }
            PublicPhase::BetweenHands => {
                if mode == MatchMode::FreeTable && hands_in_this_match >= free_table_target_hands {
                    let seat = Seat::from_index(pick_rng_index(&mut rng, 4)).unwrap();
                    (seat, PlayerAction::EndMatch)
                } else {
                    let unconfirmed: Vec<usize> = (0..4)
                        .filter(|&idx| {
                            views[idx]
                                .legal_actions
                                .contains(&PlayerAction::ConfirmNextHand)
                        })
                        .collect();
                    let chosen_idx = unconfirmed[pick_rng_index(&mut rng, unconfirmed.len())];
                    let seat = Seat::from_index(chosen_idx).unwrap();
                    (seat, PlayerAction::ConfirmNextHand)
                }
            }
            PublicPhase::MatchFinished => unreachable!(),
        };

        let emitted = game
            .apply_action(actor, action)
            .expect("action selected from legal_actions must succeed");
        stats.actions_applied += 1;

        let was_playing = phase == PublicPhase::Playing;
        process_events(
            &emitted,
            &mut prev_seq,
            &mut prev_draw_or_meld,
            &mut expected_draw_or_meld,
            &mut tracker,
            stats,
        );

        views = Seat::ALL.map(|s| project_view(&game, s));
        assert_views_invariants(
            &views,
            expected_total_score,
            prev_seq,
            &mut prev_draw_or_meld,
        );
        assert_eq!(
            views[0].seats.each_ref().map(|s| s.last_draw_or_meld_seq),
            expected_draw_or_meld
        );

        if was_playing
            && matches!(
                views[0].phase,
                PublicPhase::BetweenHands | PublicPhase::MatchFinished
            )
        {
            hands_in_this_match += 1;
            assert_settled_multiset_136(&views[0], tracker.remaining_slice(), expected_total_score);
        }
    }

    tracker.wall
}

fn run_mode_fuzz_batches(
    mode: MatchMode,
    min_hands: usize,
    wall_seed_base: u64,
    action_seed_base: u64,
    stats: &mut FuzzStats,
) {
    let mut idx = 0u64;
    let hand_count = |s: &FuzzStats| match mode {
        MatchMode::EastOnly => s.east_only_hands,
        MatchMode::FreeTable => s.free_table_hands,
    };
    while hand_count(stats) < min_hands {
        let wall_seed = wall_seed_base + idx;
        let action_seed = action_seed_base + idx;
        let target_hands = if mode == MatchMode::FreeTable {
            6 + (idx as usize % 5)
        } else {
            0
        };
        if idx.is_multiple_of(2) {
            play_fuzz_match(
                mode,
                AnyRuleset::default(),
                wall_seed,
                action_seed,
                target_hands,
                stats,
            );
        } else {
            play_fuzz_match(
                mode,
                TuidaohuRuleset,
                wall_seed,
                action_seed,
                target_hands,
                stats,
            );
        }
        idx += 1;
    }
}

#[test]
fn fixed_seed_1000_plus_hands_fuzz_invariants() {
    let mut stats = FuzzStats::default();

    run_mode_fuzz_batches(
        MatchMode::EastOnly,
        550,
        0x5EED_E457_0000_0000,
        0xAC71_0400_0000_0000,
        &mut stats,
    );
    run_mode_fuzz_batches(
        MatchMode::FreeTable,
        550,
        0x5EED_F4EE_0000_0000,
        0xAC71_F4EE_0000_0000,
        &mut stats,
    );

    let total_hands = stats.east_only_hands + stats.free_table_hands;
    assert!(
        stats.east_only_hands >= 500,
        "expected >= 500 EastOnly hands, got {}",
        stats.east_only_hands
    );
    assert!(
        stats.free_table_hands >= 500,
        "expected >= 500 FreeTable hands, got {}",
        stats.free_table_hands
    );
    assert!(
        total_hands >= 1000,
        "expected >= 1000 settled hands, got {total_hands}"
    );
    assert!(stats.draw_games > 0, "fuzz must exercise DrawGame");
    assert!(stats.wins > 0, "fuzz must exercise Win settlements");
    assert!(stats.pons_declared > 0, "fuzz must exercise Pon melds");
    assert!(
        stats.exposed_kongs > 0 && stats.added_kongs > 0 && stats.concealed_kongs > 0,
        "fuzz must exercise all three kong kinds (exposed={}, added={}, concealed={})",
        stats.exposed_kongs,
        stats.added_kongs,
        stats.concealed_kongs
    );
}

#[test]
fn fuzz_seed_replay_produces_identical_views_and_events() {
    fn replay(
        mode: MatchMode,
        wall_seed: u64,
        action_seed: u64,
    ) -> (Vec<[PlayerView; 4]>, Vec<[PlayerEvent; 4]>) {
        let mut rng = ChaCha8Rng::seed_from_u64(action_seed);
        let mut game = Match::new(
            mode,
            AnyRuleset::default(),
            SeededWallProvider::new(wall_seed),
        )
        .unwrap();
        let mut all_views = vec![Seat::ALL.map(|s| project_view(&game, s))];
        let mut all_events: Vec<[PlayerEvent; 4]> = game
            .events()
            .iter()
            .map(|ev| Seat::ALL.map(|s| project_event(ev, s)))
            .collect();
        let mut hands = 0usize;

        loop {
            let views = Seat::ALL.map(|s| project_view(&game, s));
            match views[0].phase {
                PublicPhase::Playing => {
                    let candidates: Vec<usize> = (0..4)
                        .filter(|&idx| !views[idx].legal_actions.is_empty())
                        .collect();
                    let chosen_idx = candidates[pick_rng_index(&mut rng, candidates.len())];
                    let seat = Seat::from_index(chosen_idx).unwrap();
                    let action = choose_playing_action(&views[chosen_idx].legal_actions, &mut rng);
                    let emitted = game.apply_action(seat, action).unwrap();
                    for ev in &emitted {
                        all_events.push(Seat::ALL.map(|s| project_event(ev, s)));
                    }
                    let next_views = Seat::ALL.map(|s| project_view(&game, s));
                    if matches!(
                        next_views[0].phase,
                        PublicPhase::BetweenHands | PublicPhase::MatchFinished
                    ) {
                        hands += 1;
                    }
                    all_views.push(next_views);
                }
                PublicPhase::BetweenHands => {
                    let action_seat_and_act = if mode == MatchMode::FreeTable && hands >= 4 {
                        (Seat::East, PlayerAction::EndMatch)
                    } else {
                        let seat = Seat::ALL
                            .into_iter()
                            .find(|&s| {
                                views[s.index()]
                                    .legal_actions
                                    .contains(&PlayerAction::ConfirmNextHand)
                            })
                            .unwrap();
                        (seat, PlayerAction::ConfirmNextHand)
                    };
                    let emitted = game
                        .apply_action(action_seat_and_act.0, action_seat_and_act.1)
                        .unwrap();
                    for ev in &emitted {
                        all_events.push(Seat::ALL.map(|s| project_event(ev, s)));
                    }
                    all_views.push(Seat::ALL.map(|s| project_view(&game, s)));
                }
                PublicPhase::MatchFinished => break,
            }
        }
        (all_views, all_events)
    }

    assert_eq!(
        replay(MatchMode::EastOnly, 777_111, 888_111),
        replay(MatchMode::EastOnly, 777_111, 888_111)
    );
    assert_eq!(
        replay(MatchMode::FreeTable, 777_222, 888_222),
        replay(MatchMode::FreeTable, 777_222, 888_222)
    );
}
