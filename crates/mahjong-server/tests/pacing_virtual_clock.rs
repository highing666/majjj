mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{no_reaction_wall, tile, tiles};

use mahjong_ai::{AiDifficulty, DecisionBudget};
use mahjong_core::{
    build_preset_wall, HandOutcome, MatchMode, MeldView, PlayerAction, PlayerEventKind,
    PresetWallProvider, PublicPhase, Seat, Tile, WinKind,
};
use mahjong_server::pacing::{
    action_delay_range_ms, discard_release_delay, hand_start_buffer, meld_follow_up_min,
    overlapped_wait, PacingMode,
};
use mahjong_server::protocol::{
    ClientMessage, LobbyConfig, SeatingRule, ServerMessage, TablePace, TablePhase, TableSnapshot,
};
use mahjong_server::session::{
    BoxedWallProvider, OutboundSignal, TableOptions, TableRuntime, WallProviderFactory,
};
use tokio::sync::mpsc;

#[test]
fn pacing_table_matches_spec_for_standard_fast_and_immediate() {
    assert_eq!(
        discard_release_delay(PacingMode::Humanized, TablePace::Standard),
        Duration::from_millis(200)
    );
    assert_eq!(
        discard_release_delay(PacingMode::Humanized, TablePace::Fast),
        Duration::from_millis(120)
    );
    assert_eq!(
        discard_release_delay(PacingMode::Immediate, TablePace::Standard),
        Duration::ZERO
    );
    assert_eq!(
        discard_release_delay(PacingMode::Immediate, TablePace::Fast),
        Duration::ZERO
    );

    assert_eq!(
        hand_start_buffer(PacingMode::Humanized, TablePace::Standard),
        Duration::from_millis(650)
    );
    assert_eq!(
        hand_start_buffer(PacingMode::Humanized, TablePace::Fast),
        Duration::from_millis(400)
    );
    assert_eq!(
        hand_start_buffer(PacingMode::Immediate, TablePace::Standard),
        Duration::ZERO
    );

    assert_eq!(
        meld_follow_up_min(PacingMode::Humanized, TablePace::Standard),
        Duration::from_millis(700)
    );
    assert_eq!(
        meld_follow_up_min(PacingMode::Humanized, TablePace::Fast),
        Duration::from_millis(420)
    );
    assert_eq!(
        meld_follow_up_min(PacingMode::Immediate, TablePace::Fast),
        Duration::ZERO
    );

    let tile_1m = Tile::new(0).expect("valid tile");
    let tile_2m = Tile::new(1).expect("valid tile");
    let tile_3m = Tile::new(2).expect("valid tile");

    assert_eq!(
        action_delay_range_ms(
            PlayerAction::Discard {
                tile: tile_1m,
                from_drawn: true
            },
            TablePace::Standard
        ),
        Some((380, 580))
    );
    assert_eq!(
        action_delay_range_ms(
            PlayerAction::Discard {
                tile: tile_1m,
                from_drawn: true
            },
            TablePace::Fast
        ),
        Some((230, 350))
    );

    assert_eq!(
        action_delay_range_ms(
            PlayerAction::Discard {
                tile: tile_1m,
                from_drawn: false
            },
            TablePace::Standard
        ),
        Some((520, 850))
    );
    assert_eq!(
        action_delay_range_ms(
            PlayerAction::Discard {
                tile: tile_1m,
                from_drawn: false
            },
            TablePace::Fast
        ),
        Some((310, 510))
    );

    for meld_action in [
        PlayerAction::Pon { tile: tile_1m },
        PlayerAction::ExposedKong { tile: tile_1m },
        PlayerAction::AddedKong { tile: tile_1m },
        PlayerAction::ConcealedKong { tile: tile_1m },
        PlayerAction::Chi {
            tile: tile_1m,
            with: [tile_2m, tile_3m],
        },
    ] {
        assert_eq!(
            action_delay_range_ms(meld_action, TablePace::Standard),
            Some((420, 680))
        );
        assert_eq!(
            action_delay_range_ms(meld_action, TablePace::Fast),
            Some((250, 410))
        );
    }

    for win_action in [
        PlayerAction::Tsumo,
        PlayerAction::Ron { tile: tile_1m },
        PlayerAction::RobKongHu { tile: tile_1m },
    ] {
        assert_eq!(
            action_delay_range_ms(win_action, TablePace::Standard),
            Some((350, 550))
        );
        assert_eq!(
            action_delay_range_ms(win_action, TablePace::Fast),
            Some((210, 330))
        );
    }

    for zero_action in [
        PlayerAction::Pass,
        PlayerAction::ConfirmNextHand,
        PlayerAction::EndMatch,
    ] {
        assert_eq!(
            action_delay_range_ms(zero_action, TablePace::Standard),
            None
        );
        assert_eq!(action_delay_range_ms(zero_action, TablePace::Fast), None);
    }
}

#[tokio::test(start_paused = true)]
async fn overlapped_wait_subtracts_elapsed_think_time_and_clamps_to_zero() {
    let started_at = tokio::time::Instant::now();
    let target = Duration::from_millis(500);

    // elapsed = 0 -> full target wait
    assert_eq!(
        overlapped_wait(started_at, started_at, target),
        Duration::from_millis(500)
    );

    // 0 < elapsed < target -> remaining only
    tokio::time::advance(Duration::from_millis(180)).await;
    assert_eq!(
        overlapped_wait(started_at, tokio::time::Instant::now(), target),
        Duration::from_millis(320)
    );

    // elapsed == target -> zero
    tokio::time::advance(Duration::from_millis(320)).await;
    assert_eq!(
        overlapped_wait(started_at, tokio::time::Instant::now(), target),
        Duration::ZERO
    );

    // elapsed > target -> zero (never negative/underflow)
    tokio::time::advance(Duration::from_millis(45)).await;
    assert_eq!(
        overlapped_wait(started_at, tokio::time::Instant::now(), target),
        Duration::ZERO
    );
}

#[test]
fn table_options_defaults_to_humanized_and_supports_with_pacing() {
    let default_opts = TableOptions::default();
    assert_eq!(default_opts.pacing(), PacingMode::Humanized);

    let seeded_opts = TableOptions::seeded(42);
    assert_eq!(seeded_opts.pacing(), PacingMode::Humanized);

    let immediate_opts = seeded_opts.with_pacing(PacingMode::Immediate);
    assert_eq!(immediate_opts.pacing(), PacingMode::Immediate);
}

struct TestClient {
    conn_id: u64,
    tx: mpsc::UnboundedSender<OutboundSignal>,
    rx: mpsc::UnboundedReceiver<OutboundSignal>,
}

impl TestClient {
    async fn connect(runtime: &TableRuntime, client_id: &str, nickname: &str) -> Self {
        let conn_id = runtime.allocate_conn_id().await;
        let (tx, rx) = mpsc::unbounded_channel();
        let mut client = Self { conn_id, tx, rx };
        client
            .send(
                runtime,
                &ClientMessage::Hello {
                    client_id: client_id.to_string(),
                    nickname: nickname.to_string(),
                    table_id: None,
                    seat_token: None,
                },
            )
            .await;
        let _ = client.recv_snapshot().await;
        client
    }

    async fn send(&self, runtime: &TableRuntime, msg: &ClientMessage) {
        let raw = serde_json::to_string(msg).expect("serialize ClientMessage");
        let _ = runtime
            .handle_raw_text(self.conn_id, &raw, true, &self.tx)
            .await;
    }

    fn try_recv_msg(&mut self) -> Option<ServerMessage> {
        match self.rx.try_recv() {
            Ok(OutboundSignal::Message(msg)) => Some(*msg),
            Ok(OutboundSignal::ReplaceAndClose) => Some(ServerMessage::SessionReplaced),
            Err(_) => None,
        }
    }

    async fn recv_msg(&mut self) -> ServerMessage {
        match self.rx.recv().await.expect("outbound signal channel open") {
            OutboundSignal::Message(msg) => *msg,
            OutboundSignal::ReplaceAndClose => ServerMessage::SessionReplaced,
        }
    }

    async fn recv_snapshot(&mut self) -> TableSnapshot {
        loop {
            match self.recv_msg().await {
                ServerMessage::TableSnapshot(snap) => return snap,
                ServerMessage::LobbyUpdated { .. } => continue,
                other => panic!("expected TableSnapshot, got {other:?}"),
            }
        }
    }

    async fn recv_event_batch(
        &mut self,
    ) -> (u64, u64, Vec<mahjong_core::PlayerEvent>, Vec<PlayerAction>) {
        loop {
            match self.recv_msg().await {
                ServerMessage::EventBatch {
                    from_seq,
                    to_seq,
                    events,
                    updated_legal_actions,
                } => return (from_seq, to_seq, events, updated_legal_actions),
                ServerMessage::LobbyUpdated { .. } => continue,
                other => panic!("expected EventBatch, got {other:?}"),
            }
        }
    }

    async fn await_legal_actions(&mut self, required: Option<PlayerAction>) -> u64 {
        loop {
            match self.recv_msg().await {
                ServerMessage::EventBatch {
                    to_seq,
                    updated_legal_actions,
                    ..
                } if !updated_legal_actions.is_empty()
                    && required
                        .as_ref()
                        .is_none_or(|req| updated_legal_actions.contains(req)) =>
                {
                    return to_seq;
                }
                _ => continue,
            }
        }
    }

    fn drain_pending(&mut self) {
        while self.try_recv_msg().is_some() {}
    }
}

async fn setup_four_human_match(
    seed: u64,
    factory: WallProviderFactory,
    mode: MatchMode,
    pace: TablePace,
) -> (TableRuntime, [TestClient; 4], [TableSnapshot; 4]) {
    let runtime = TableRuntime::try_with_options_and_threads(
        0,
        None,
        TableOptions::with_wall_factory(seed, factory),
        1,
    )
    .expect("create runtime");

    let mut clients = [
        TestClient::connect(&runtime, "c0", "East").await,
        TestClient::connect(&runtime, "c1", "South").await,
        TestClient::connect(&runtime, "c2", "West").await,
        TestClient::connect(&runtime, "c3", "North").await,
    ];
    for (idx, seat) in Seat::ALL.into_iter().enumerate() {
        clients[idx]
            .send(&runtime, &ClientMessage::JoinSeat { seat: Some(seat) })
            .await;
        let _ = clients[idx].recv_snapshot().await;
    }
    for c in &mut clients {
        c.drain_pending();
    }

    clients[0]
        .send(
            &runtime,
            &ClientMessage::UpdateLobbyConfig {
                mode,
                seating: SeatingRule::FixedAdjacent,
                ai_difficulty: AiDifficulty::Strong,
                pace,
            },
        )
        .await;
    for c in &mut clients {
        c.drain_pending();
    }

    clients[0].send(&runtime, &ClientMessage::StartMatch).await;
    let snaps = [
        clients[0].recv_snapshot().await,
        clients[1].recv_snapshot().await,
        clients[2].recv_snapshot().await,
        clients[3].recv_snapshot().await,
    ];
    (runtime, clients, snaps)
}

#[tokio::test(start_paused = true)]
async fn unclaimed_discard_splits_into_two_beats_and_switches_live_with_set_pace() {
    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let (runtime, mut clients, snaps) =
        setup_four_human_match(1, factory, MatchMode::FreeTable, TablePace::Standard).await;
    assert_eq!(snaps[0].seq, 2);

    // East discards 31 (C) at t = 0 -> immediate batch contains ONLY TileDiscarded, empty legal_actions.
    clients[0]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: 2,
                client_action_id: "east-discard-c".to_string(),
                action: PlayerAction::Discard {
                    tile: tile("C"),
                    from_drawn: false,
                },
            },
        )
        .await;

    for client in &mut clients {
        let (from_seq, to_seq, events, legal) = client.recv_event_batch().await;
        assert_eq!((from_seq, to_seq), (3, 3));
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].kind,
            PlayerEventKind::TileDiscarded {
                seat: Seat::East,
                tile: t,
                is_tsumogiri: false,
            } if t == tile("C")
        ));
        assert!(
            legal.is_empty(),
            "first beat must not leak upcoming TileDrawn legal_actions"
        );
    }

    // At t = 199 ms, TileDrawn is still withheld.
    tokio::time::advance(Duration::from_millis(199)).await;
    tokio::task::yield_now().await;
    for client in &mut clients {
        assert!(
            client.try_recv_msg().is_none(),
            "TileDrawn must not release before 200ms in Standard pace"
        );
    }

    // At t = 200 ms, TileDrawn (seq = 4) and South's legal_actions are released.
    tokio::time::advance(Duration::from_millis(1)).await;
    for (idx, client) in clients.iter_mut().enumerate() {
        let (from_seq, to_seq, events, legal) = client.recv_event_batch().await;
        assert_eq!((from_seq, to_seq), (4, 4));
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].kind,
            PlayerEventKind::TileDrawn {
                seat: Seat::South,
                tile: ref drawn,
                is_replacement: false,
                wall_remaining: 82,
            } if (idx == Seat::South.index()) == drawn.is_some()
        ));
        if idx == Seat::South.index() {
            assert!(!legal.is_empty(), "South receives legal actions on release");
        } else {
            assert!(legal.is_empty());
        }
    }

    // Switch live to Fast pace mid-hand -> next unclaimed discard releases at 120 ms!
    clients[0]
        .send(
            &runtime,
            &ClientMessage::SetPace {
                pace: TablePace::Fast,
            },
        )
        .await;
    for client in &mut clients {
        let msg = client.recv_msg().await;
        assert!(matches!(
            msg,
            ServerMessage::LobbyUpdated {
                config: LobbyConfig {
                    pace: TablePace::Fast,
                    ..
                },
                ..
            }
        ));
    }

    let south_snap = {
        clients[1]
            .send(&runtime, &ClientMessage::RequestSnapshot)
            .await;
        clients[1].recv_snapshot().await
    };
    let south_drawn = south_snap
        .player_view
        .expect("south view")
        .hand
        .drawn_tile
        .expect("south drawn tile");
    clients[1]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: 4,
                client_action_id: "south-tsumogiri".to_string(),
                action: PlayerAction::Discard {
                    tile: south_drawn,
                    from_drawn: true,
                },
            },
        )
        .await;

    for client in &mut clients {
        let (from_seq, to_seq, events, legal) = client.recv_event_batch().await;
        assert_eq!((from_seq, to_seq), (5, 5));
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].kind,
            PlayerEventKind::TileDiscarded {
                seat: Seat::South,
                is_tsumogiri: true,
                ..
            }
        ));
        assert!(legal.is_empty());
    }

    tokio::time::advance(Duration::from_millis(119)).await;
    tokio::task::yield_now().await;
    for client in &mut clients {
        assert!(
            client.try_recv_msg().is_none(),
            "TileDrawn must not release before 120ms in Fast pace"
        );
    }

    tokio::time::advance(Duration::from_millis(1)).await;
    for (idx, client) in clients.iter_mut().enumerate() {
        let (from_seq, to_seq, events, legal) = client.recv_event_batch().await;
        assert_eq!((from_seq, to_seq), (6, 6));
        assert!(matches!(
            events[0].kind,
            PlayerEventKind::TileDrawn {
                seat: Seat::West,
                wall_remaining: 81,
                ..
            }
        ));
        assert_eq!(!legal.is_empty(), idx == Seat::West.index());
    }
}

#[tokio::test(start_paused = true)]
async fn zero_wall_draw_game_releases_discard_first_and_settlement_after_beat() {
    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let (runtime, mut clients, snaps) =
        setup_four_human_match(2, factory, MatchMode::FreeTable, TablePace::Standard).await;
    let mut seq = snaps[0].seq;
    let mut active_idx = Seat::East.index();

    // First discard (East discards C), then 82 tsumogiri draws+discards until wall_remaining == 0.
    for step in 0..83u32 {
        let action = if step == 0 {
            PlayerAction::Discard {
                tile: tile("C"),
                from_drawn: false,
            }
        } else {
            clients[active_idx]
                .send(&runtime, &ClientMessage::RequestSnapshot)
                .await;
            let snap = clients[active_idx].recv_snapshot().await;
            let view = snap.player_view.expect("active view");
            assert_eq!(view.wall_remaining, (83 - step) as u8);
            PlayerAction::Discard {
                tile: view.hand.drawn_tile.expect("drawn tile"),
                from_drawn: true,
            }
        };

        clients[active_idx]
            .send(
                &runtime,
                &ClientMessage::SubmitAction {
                    expected_seq: seq,
                    client_action_id: format!("step-{step}"),
                    action,
                },
            )
            .await;

        for client in &mut clients {
            let (from_seq, to_seq, events, _) = client.recv_event_batch().await;
            assert_eq!((from_seq, to_seq), (seq + 1, seq + 1));
            assert!(matches!(
                events[0].kind,
                PlayerEventKind::TileDiscarded { .. }
            ));
        }
        seq += 1;

        tokio::time::advance(Duration::from_millis(200)).await;
        for client in &mut clients {
            let (from_seq, to_seq, events, _) = client.recv_event_batch().await;
            assert_eq!((from_seq, to_seq), (seq + 1, seq + 1));
            assert!(matches!(events[0].kind, PlayerEventKind::TileDrawn { .. }));
        }
        seq += 1;
        active_idx = (active_idx + 1) % 4;
    }

    // Now wall_remaining == 0 and active_idx (North) holds the 84th discard.
    clients[active_idx]
        .send(&runtime, &ClientMessage::RequestSnapshot)
        .await;
    let final_pre = clients[active_idx].recv_snapshot().await;
    let final_view = final_pre.player_view.expect("north view");
    assert_eq!(final_view.wall_remaining, 0);
    let last_tile = final_view.hand.drawn_tile.expect("last drawn tile");

    clients[active_idx]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: seq,
                client_action_id: "final-discard".to_string(),
                action: PlayerAction::Discard {
                    tile: last_tile,
                    from_drawn: true,
                },
            },
        )
        .await;

    // Beat 1 (t = 0): ONLY TileDiscarded, empty legal_actions.
    for client in &mut clients {
        let (from_seq, to_seq, events, legal) = client.recv_event_batch().await;
        assert_eq!((from_seq, to_seq), (seq + 1, seq + 1));
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0].kind,
            PlayerEventKind::TileDiscarded { .. }
        ));
        assert!(legal.is_empty());
    }
    let discard_seq = seq + 1;

    // Mid-window at t = 199 ms: RequestSnapshot is still Playing with no settlement!
    tokio::time::advance(Duration::from_millis(199)).await;
    tokio::task::yield_now().await;
    assert!(clients[0].try_recv_msg().is_none());
    clients[0]
        .send(&runtime, &ClientMessage::RequestSnapshot)
        .await;
    let mid_snap = clients[0].recv_snapshot().await;
    assert_eq!(mid_snap.phase, TablePhase::InMatch);
    assert_eq!(mid_snap.seq, discard_seq);
    let mid_view = mid_snap.player_view.expect("mid view");
    assert_eq!(mid_view.phase, PublicPhase::Playing);
    assert!(mid_view.last_settlement.is_none());
    assert!(mid_view.legal_actions.is_empty());

    // Beat 2 (t = 200 ms): HandSettled(DrawGame) releases.
    tokio::time::advance(Duration::from_millis(1)).await;
    for client in &mut clients {
        let (from_seq, to_seq, events, legal) = client.recv_event_batch().await;
        assert_eq!((from_seq, to_seq), (discard_seq + 1, discard_seq + 1));
        assert!(matches!(
            events[0].kind,
            PlayerEventKind::HandSettled { ref settlement }
                if settlement.outcome == HandOutcome::DrawGame
        ));
        assert!(legal.contains(&PlayerAction::ConfirmNextHand));
    }
}

#[tokio::test(start_paused = true)]
async fn pending_window_snapshot_reconnect_idempotency_and_premature_submit_stay_consistent() {
    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let (runtime, mut clients, snaps) =
        setup_four_human_match(3, factory, MatchMode::FreeTable, TablePace::Standard).await;
    let table_id = snaps[0].table_id.clone();
    let south_token = snaps[1].seat_token.clone().expect("south token");

    clients[0]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: 2,
                client_action_id: "east-discard-c".to_string(),
                action: PlayerAction::Discard {
                    tile: tile("C"),
                    from_drawn: false,
                },
            },
        )
        .await;
    for client in &mut clients {
        let _ = client.recv_event_batch().await;
    }

    // 1. Mid-window RequestSnapshot for East & South shows seq = 3, discarded C in river,
    //    empty legal_actions, and zero TileDrawn leak.
    clients[0]
        .send(&runtime, &ClientMessage::RequestSnapshot)
        .await;
    let east_mid = clients[0].recv_snapshot().await;
    assert_eq!(east_mid.seq, 3);
    let east_view = east_mid.player_view.expect("east view");
    assert_eq!(east_view.seq, 3);
    assert_eq!(east_view.hand.closed_tiles.len(), 13);
    assert_eq!(east_view.hand.drawn_tile, None);
    assert_eq!(east_view.seats[Seat::East.index()].river.len(), 1);
    assert_eq!(east_view.seats[Seat::East.index()].river[0].tile, tile("C"));
    assert!(east_view.legal_actions.is_empty());

    clients[1]
        .send(&runtime, &ClientMessage::RequestSnapshot)
        .await;
    let south_mid = clients[1].recv_snapshot().await;
    assert_eq!(south_mid.seq, 3);
    let south_view = south_mid.player_view.expect("south view");
    assert_eq!(south_view.seats[Seat::South.index()].hand_count, 13);
    assert_eq!(south_view.hand.drawn_tile, None);
    assert_eq!(south_view.wall_remaining, 83);
    assert!(south_view.legal_actions.is_empty());

    // 2. Duplicate retry of "east-discard-c" is still idempotently ignored.
    clients[0]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: 2,
                client_action_id: "east-discard-c".to_string(),
                action: PlayerAction::Discard {
                    tile: tile("C"),
                    from_drawn: false,
                },
            },
        )
        .await;
    tokio::task::yield_now().await;
    assert!(clients[0].try_recv_msg().is_none());

    // 3. Premature SubmitAction with expected_seq = 3 or 4 gets ActionRejected { current_seq: 3 }.
    for bad_seq in [3u64, 4u64] {
        clients[1]
            .send(
                &runtime,
                &ClientMessage::SubmitAction {
                    expected_seq: bad_seq,
                    client_action_id: format!("premature-{bad_seq}"),
                    action: PlayerAction::Discard {
                        tile: tile("1m"),
                        from_drawn: false,
                    },
                },
            )
            .await;
        assert!(matches!(
            clients[1].recv_msg().await,
            ServerMessage::ActionRejected { current_seq: 3, .. }
        ));
    }

    // 4. Reconnect via Hello during pending window restores public pre-release view cleanly.
    let re_conn_id = runtime.allocate_conn_id().await;
    let (re_tx, re_rx) = mpsc::unbounded_channel();
    let mut south_re = TestClient {
        conn_id: re_conn_id,
        tx: re_tx,
        rx: re_rx,
    };
    south_re
        .send(
            &runtime,
            &ClientMessage::Hello {
                client_id: "c1".to_string(),
                nickname: "South".to_string(),
                table_id: Some(table_id),
                seat_token: Some(south_token),
            },
        )
        .await;
    let re_snap = south_re.recv_snapshot().await;
    assert_eq!(re_snap.seq, 3);
    let re_view = re_snap.player_view.expect("reconnected south view");
    assert_eq!(re_view.hand.drawn_tile, None);
    assert!(re_view.legal_actions.is_empty());

    // Advance to 200 ms -> South's new connection receives the delayed TileDrawn.
    tokio::time::advance(Duration::from_millis(200)).await;
    let (from_seq, to_seq, events, legal) = south_re.recv_event_batch().await;
    assert_eq!((from_seq, to_seq), (4, 4));
    assert!(matches!(
        events[0].kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::South,
            tile: Some(_),
            ..
        }
    ));
    assert!(!legal.is_empty());
}

#[tokio::test(start_paused = true)]
async fn disband_and_rematch_during_pending_release_never_pollute_new_state() {
    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let (runtime, mut clients, _) =
        setup_four_human_match(4, factory, MatchMode::FreeTable, TablePace::Standard).await;

    // Trigger pending release window.
    clients[0]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: 2,
                client_action_id: "east-discard-c".to_string(),
                action: PlayerAction::Discard {
                    tile: tile("C"),
                    from_drawn: false,
                },
            },
        )
        .await;
    for client in &mut clients {
        let _ = client.recv_event_batch().await;
    }

    // Disband match immediately while timer is pending (multi-human mutual confirmation).
    clients[0]
        .send(&runtime, &ClientMessage::RequestDisbandMatch)
        .await;
    let mut op_id = None;
    for client in &mut clients {
        let snap = client.recv_snapshot().await;
        op_id = snap.pending_destructive_op.map(|op| op.op_id);
    }
    let op_id = op_id.expect("pending disband op");

    for client in &clients[1..] {
        client
            .send(
                &runtime,
                &ClientMessage::ConfirmDestructiveOp {
                    op_id: op_id.clone(),
                    accept: true,
                },
            )
            .await;
    }
    for client in &mut clients {
        loop {
            let snap = client.recv_snapshot().await;
            if snap.phase == TablePhase::Lobby {
                assert_eq!(snap.seq, 0);
                break;
            }
        }
    }

    // Immediately start a brand new match before the old 200ms timer expires!
    clients[0].send(&runtime, &ClientMessage::StartMatch).await;
    for client in &mut clients {
        let snap = client.recv_snapshot().await;
        assert_eq!(snap.phase, TablePhase::InMatch);
        assert_eq!(snap.seq, 2);
    }

    // Advance past 200ms (and 1s): zero stale EventBatch may arrive from the disbanded match!
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    for client in &mut clients {
        assert!(
            client.try_recv_msg().is_none(),
            "expired timer from disbanded match must not emit stale events"
        );
    }
}

async fn setup_seated_humans_match(
    seed: u64,
    factory: WallProviderFactory,
    human_seats: &[Seat],
    pace: TablePace,
) -> (TableRuntime, Vec<TestClient>, Vec<TableSnapshot>) {
    let runtime = TableRuntime::try_with_options_and_threads(
        0,
        None,
        TableOptions::with_wall_factory(seed, factory)
            .with_ai_budget(DecisionBudget::Deterministic { max_samples: 8 }),
        1,
    )
    .expect("create runtime");

    let mut clients = Vec::with_capacity(human_seats.len());
    for (idx, &seat) in human_seats.iter().enumerate() {
        let mut client =
            TestClient::connect(&runtime, &format!("h{idx}"), &format!("Player{idx}")).await;
        client
            .send(&runtime, &ClientMessage::JoinSeat { seat: Some(seat) })
            .await;
        let _ = client.recv_snapshot().await;
        clients.push(client);
    }
    for c in &mut clients {
        c.drain_pending();
    }

    clients[0]
        .send(
            &runtime,
            &ClientMessage::UpdateLobbyConfig {
                mode: MatchMode::FreeTable,
                seating: SeatingRule::FixedAdjacent,
                ai_difficulty: AiDifficulty::Strong,
                pace,
            },
        )
        .await;
    for c in &mut clients {
        c.drain_pending();
    }

    clients[0].send(&runtime, &ClientMessage::StartMatch).await;
    let mut snaps = Vec::with_capacity(clients.len());
    for c in &mut clients {
        snaps.push(c.recv_snapshot().await);
    }
    (runtime, clients, snaps)
}

async fn assert_ai_only_reaction_all_pass_alignment(pace: TablePace, expected_ms: u64) {
    // South AI holds 0-shanten SevenPairs (11s 22s 33m 44m 55p 66p 7p):
    // when East discards 1s, South AI is offered Pon(1s) / Pass (no Ron) and chooses Pass.
    let wall = build_preset_wall(
        &tiles("1s 1m 4m 7m 2s 4s 7s 1p 4p 7p E S W N"),
        &tiles("1s 1s 2s 2s 3m 3m 4m 4m 5p 5p 6p 6p 7p"),
        &tiles("2m 5m 8m 3s 6s 9s 2p 8p 9p E S W N"),
        &tiles("2m 5m 8m 3s 6s 9s 2p 8p 9p C F B E"),
        &tiles("3p 9m 9m 9m"),
        &[],
    )
    .expect("valid wall");

    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let (runtime, mut clients, snaps) =
        setup_seated_humans_match(11, factory, &[Seat::East], pace).await;
    assert_eq!(snaps[0].your_seat, Some(Seat::East));

    let discard_start = tokio::time::Instant::now();
    clients[0]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: snaps[0].seq,
                client_action_id: format!("discard-1s-{pace:?}"),
                action: PlayerAction::Discard {
                    tile: tile("1s"),
                    from_drawn: false,
                },
            },
        )
        .await;

    let (f1, t1, ev1, legal1) = clients[0].recv_event_batch().await;
    assert_eq!((f1, t1), (3, 3));
    assert!(matches!(
        ev1[0].kind,
        PlayerEventKind::TileDiscarded {
            seat: Seat::East,
            tile: t,
            ..
        } if t == tile("1s")
    ));
    assert!(legal1.is_empty());
    assert_eq!(
        tokio::time::Instant::now().duration_since(discard_start),
        Duration::ZERO
    );

    // Next batch is South's TileDrawn after all AIs Pass: must align exactly to 200ms / 120ms.
    let (f2, t2, ev2, _) = clients[0].recv_event_batch().await;
    assert_eq!((f2, t2), (4, 4));
    assert!(matches!(
        ev2[0].kind,
        PlayerEventKind::TileDrawn {
            seat: Seat::South,
            ..
        }
    ));
    assert_eq!(
        tokio::time::Instant::now().duration_since(discard_start),
        Duration::from_millis(expected_ms)
    );
}

#[tokio::test(start_paused = true)]
async fn ai_only_reaction_all_pass_matches_unclaimed_discard_beat_standard() {
    assert_ai_only_reaction_all_pass_alignment(TablePace::Standard, 200).await;
}

#[tokio::test(start_paused = true)]
async fn ai_only_reaction_all_pass_matches_unclaimed_discard_beat_fast() {
    assert_ai_only_reaction_all_pass_alignment(TablePace::Fast, 120).await;
}

#[tokio::test(start_paused = true)]
async fn human_reactor_receives_prompt_at_t0_and_releases_draw_immediately_on_pass() {
    let wall = build_preset_wall(
        &tiles("1s 1m 4m 7m 2s 4s 7s 1p 4p 7p E S W N"),
        &tiles("1s 1s 2s 2s 3m 3m 4m 4m 5p 5p 6p 6p 7p"),
        &tiles("2m 5m 8m 3s 6s 9s 2p 8p 9p E S W N"),
        &tiles("2m 5m 8m 3s 6s 9s 2p 8p 9p C F B E"),
        &tiles("3p 9m 9m 9m"),
        &[],
    )
    .expect("valid wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let (runtime, mut clients, snaps) =
        setup_seated_humans_match(12, factory, &[Seat::East, Seat::South], TablePace::Standard)
            .await;
    assert_eq!(snaps[0].your_seat, Some(Seat::East));
    assert_eq!(snaps[1].your_seat, Some(Seat::South));

    clients[0]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: snaps[0].seq,
                client_action_id: "east-discard-1s".to_string(),
                action: PlayerAction::Discard {
                    tile: tile("1s"),
                    from_drawn: false,
                },
            },
        )
        .await;

    let (_, _, _, legal_east) = clients[0].recv_event_batch().await;
    let (f_s, t_s, ev_s, legal_south) = clients[1].recv_event_batch().await;
    assert_eq!((f_s, t_s), (3, 3));
    assert!(matches!(
        ev_s[0].kind,
        PlayerEventKind::TileDiscarded { .. }
    ));
    assert!(legal_east.is_empty());
    assert!(legal_south.contains(&PlayerAction::Pon { tile: tile("1s") }));
    assert!(legal_south.contains(&PlayerAction::Pass));

    // Human thinks for 500ms without fake server delay; then submits Pass -> immediate TileDrawn!
    tokio::time::advance(Duration::from_millis(500)).await;
    let pass_at = tokio::time::Instant::now();
    clients[1]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: 3,
                client_action_id: "south-pass".to_string(),
                action: PlayerAction::Pass,
            },
        )
        .await;

    for (idx, client) in clients.iter_mut().enumerate() {
        let (f, t, ev, legal) = client.recv_event_batch().await;
        assert_eq!((f, t), (4, 4));
        assert!(matches!(
            ev[0].kind,
            PlayerEventKind::TileDrawn {
                seat: Seat::South,
                ..
            }
        ));
        assert_eq!(!legal.is_empty(), idx == 1);
    }
    assert_eq!(
        tokio::time::Instant::now().duration_since(pass_at),
        Duration::ZERO
    );
}

async fn assert_dealer_ai_hand_started_buffer(
    pace: TablePace,
    min_total_ms: u64,
    max_total_ms: u64,
) {
    // FixedAdjacent places the solo Human at East; PresetWallProvider makes South (AI) the dealer.
    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::South,
            wall.clone(),
        )))
    });
    let (runtime, mut clients, snaps) =
        setup_seated_humans_match(21, factory, &[Seat::East], pace).await;
    assert_eq!(snaps[0].your_seat, Some(Seat::East));
    let match_started_at = tokio::time::Instant::now();

    let (f, t, evs, _) = clients[0].recv_event_batch().await;
    assert_eq!((f, t), (3, 3));
    assert!(matches!(
        evs[0].kind,
        PlayerEventKind::TileDiscarded {
            seat: Seat::South,
            is_tsumogiri: false,
            ..
        }
    ));
    let elapsed_ms = tokio::time::Instant::now()
        .duration_since(match_started_at)
        .as_millis() as u64;
    assert!(
        (min_total_ms..=max_total_ms).contains(&elapsed_ms),
        "dealer AI first discard elapsed {elapsed_ms}ms not in [{min_total_ms}, {max_total_ms}] for {pace:?}"
    );
    clients[0]
        .send(&runtime, &ClientMessage::RequestDisbandMatch)
        .await;
}

#[tokio::test(start_paused = true)]
async fn hand_started_applies_buffer_before_dealer_ai_first_discard_standard() {
    // Standard: 650ms buffer + [520..850]ms 手切 = [1170..1500]ms
    assert_dealer_ai_hand_started_buffer(TablePace::Standard, 1170, 1500).await;
}

#[tokio::test(start_paused = true)]
async fn hand_started_applies_buffer_before_dealer_ai_first_discard_fast() {
    // Fast: 400ms buffer + [310..510]ms 手切 = [710..910]ms
    assert_dealer_ai_hand_started_buffer(TablePace::Fast, 710, 910).await;
}

async fn assert_ai_tsumogiri_and_tedashi_and_live_pace_switch() {
    // East (Human) discards C (unclaimed).
    // South (AI) holds 111m 234s 567p 88p E E -> draws F (useless isolated honor) and 摸切s F.
    // West (AI) holds 234m 567m 789m 22p 2s B -> draws 3s (completes 23s) and 手切s B!
    let wall = build_preset_wall(
        &tiles("C 4m 7m 9m 1s 4s 7s 1p 4p 7p E S W N"),
        &tiles("1m 1m 1m 2s 3s 4s 5p 6p 7p 8p 8p E E"),
        &tiles("2m 3m 4m 5m 6m 7m 7m 8m 9m 2p 2p 2s B"),
        &tiles("2m 5m 8m 1s 4s 7s 1p 4p 7p S W N C"),
        &tiles("F 3s 9p 9p"),
        &[],
    )
    .expect("valid wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let (runtime, mut clients, snaps) =
        setup_seated_humans_match(22, factory, &[Seat::East], TablePace::Standard).await;

    // East discards C -> TileDrawn for South releases at +200ms.
    clients[0]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: snaps[0].seq,
                client_action_id: "e-c".to_string(),
                action: PlayerAction::Discard {
                    tile: tile("C"),
                    from_drawn: false,
                },
            },
        )
        .await;
    let _ = clients[0].recv_event_batch().await;
    let _ = clients[0].recv_event_batch().await;
    let south_draw_at = tokio::time::Instant::now();

    // South AI 摸切s F -> [380..580]ms in Standard.
    let (_, _, ev_south_disc, _) = clients[0].recv_event_batch().await;
    assert!(matches!(
        ev_south_disc[0].kind,
        PlayerEventKind::TileDiscarded {
            seat: Seat::South,
            tile: t,
            is_tsumogiri: true,
        } if t == tile("F")
    ));
    let south_elapsed = tokio::time::Instant::now()
        .duration_since(south_draw_at)
        .as_millis() as u64;
    assert!(
        (380..=580).contains(&south_elapsed),
        "Standard AI 摸切 took {south_elapsed}ms"
    );

    // West TileDrawn at +200ms -> switch live to Fast before West finishes, or check Standard 手切 first:
    let _ = clients[0].recv_event_batch().await;
    let west_draw_at = tokio::time::Instant::now();
    let (_, _, ev_west_disc, _) = clients[0].recv_event_batch().await;
    assert!(matches!(
        ev_west_disc[0].kind,
        PlayerEventKind::TileDiscarded {
            seat: Seat::West,
            tile: t,
            is_tsumogiri: false,
        } if t == tile("B")
    ));
    let west_elapsed = tokio::time::Instant::now()
        .duration_since(west_draw_at)
        .as_millis() as u64;
    assert!(
        (520..=850).contains(&west_elapsed),
        "Standard AI 手切 took {west_elapsed}ms"
    );

    // Now switch live to Fast! North draws at +200ms (since West discarded in Standard).
    clients[0]
        .send(
            &runtime,
            &ClientMessage::SetPace {
                pace: TablePace::Fast,
            },
        )
        .await;
    let _ = clients[0].recv_msg().await; // LobbyUpdated(Fast)
    let _ = clients[0].recv_event_batch().await; // North TileDrawn
    let north_draw_at = tokio::time::Instant::now();
    let (_, _, ev_north_disc, _) = clients[0].recv_event_batch().await;
    let north_elapsed = tokio::time::Instant::now()
        .duration_since(north_draw_at)
        .as_millis() as u64;
    match ev_north_disc[0].kind {
        PlayerEventKind::TileDiscarded {
            seat: Seat::North,
            is_tsumogiri: true,
            ..
        } => assert!(
            (230..=350).contains(&north_elapsed),
            "Fast AI 摸切 took {north_elapsed}ms"
        ),
        PlayerEventKind::TileDiscarded {
            seat: Seat::North,
            is_tsumogiri: false,
            ..
        } => assert!(
            (310..=510).contains(&north_elapsed),
            "Fast AI 手切 took {north_elapsed}ms"
        ),
        ref other => panic!("unexpected North event {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn ai_tsumogiri_tedashi_and_live_set_pace_follow_spec_ranges() {
    assert_ai_tsumogiri_and_tedashi_and_live_pace_switch().await;
}

async fn assert_ai_pon_and_followup_floor(
    pace: TablePace,
    pon_range: (u64, u64),
    followup_min_ms: u64,
    followup_max_ms: u64,
) {
    // South AI holds 55m 123s 567p 99p 23m E -> East discards 5m -> South AI Pons 5m into immediate tenpai, then discards E.
    let wall = build_preset_wall(
        &tiles("5m 1m 4m 7m 1s 4s 7s 1p 4p 7p E S W N"),
        &tiles("5m 5m 1s 2s 3s 5p 6p 7p 9p 9p 2m 3m E"),
        &tiles("2m 3m 6m 8m 9m 2s 8s 9s 2p 3p 5p E S"),
        &tiles("2m 3m 6m 8m 9m 2s 8s 9s 2p 3p 5p W N"),
        &tiles("C F B"),
        &[],
    )
    .expect("valid wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let (runtime, mut clients, snaps) =
        setup_seated_humans_match(23, factory, &[Seat::East], pace).await;

    clients[0]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: snaps[0].seq,
                client_action_id: "e-5m".to_string(),
                action: PlayerAction::Discard {
                    tile: tile("5m"),
                    from_drawn: false,
                },
            },
        )
        .await;
    let _ = clients[0].recv_event_batch().await; // TileDiscarded(5m)
    let discard_at = tokio::time::Instant::now();

    let (_, _, ev_pon, _) = clients[0].recv_event_batch().await;
    assert!(matches!(
        ev_pon[0].kind,
        PlayerEventKind::MeldDeclared {
            seat: Seat::South,
            meld: MeldView::Pon { .. },
            ..
        }
    ));
    let pon_ms = tokio::time::Instant::now()
        .duration_since(discard_at)
        .as_millis() as u64;
    assert!(
        (pon_range.0..=pon_range.1).contains(&pon_ms),
        "{pace:?} AI Pon took {pon_ms}ms, expected {pon_range:?}"
    );

    let meld_at = tokio::time::Instant::now();
    let (_, _, ev_follow, _) = clients[0].recv_event_batch().await;
    assert!(matches!(
        ev_follow[0].kind,
        PlayerEventKind::TileDiscarded {
            seat: Seat::South,
            ..
        }
    ));
    let followup_ms = tokio::time::Instant::now()
        .duration_since(meld_at)
        .as_millis() as u64;
    assert!(
        (followup_min_ms..=followup_max_ms).contains(&followup_ms),
        "{pace:?} AI MeldDeclared follow-up took {followup_ms}ms, expected [{followup_min_ms}, {followup_max_ms}]"
    );
}

#[tokio::test(start_paused = true)]
async fn ai_pon_and_meld_followup_floor_standard() {
    assert_ai_pon_and_followup_floor(TablePace::Standard, (420, 680), 700, 850).await;
}

#[tokio::test(start_paused = true)]
async fn ai_pon_and_meld_followup_floor_fast() {
    assert_ai_pon_and_followup_floor(TablePace::Fast, (250, 410), 420, 510).await;
}

#[tokio::test(start_paused = true)]
async fn ai_concealed_kong_and_followup_floor_standard() {
    // East discards C; South draws 4th 1m -> declares ConcealedKong(1m) -> replacement draw -> follow-up discard >= 700ms.
    let wall = build_preset_wall(
        &tiles("C 2m 4m 7m 1s 4s 7s 1p 4p 7p E S W N"),
        &tiles("1m 1m 1m 2s 3s 4s 5p 6p 7p 8p 8p E E"),
        &tiles("2m 5m 8m 3s 6s 9s 2p 3p 9p S W N C"),
        &tiles("3m 6m 9m 2s 5s 8s 2p 3p 9p F B C F"),
        &tiles("1m"),
        &tiles("B"),
    )
    .expect("valid wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let (runtime, mut clients, snaps) =
        setup_seated_humans_match(24, factory, &[Seat::East], TablePace::Standard).await;

    clients[0]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: snaps[0].seq,
                client_action_id: "e-c".to_string(),
                action: PlayerAction::Discard {
                    tile: tile("C"),
                    from_drawn: false,
                },
            },
        )
        .await;
    let _ = clients[0].recv_event_batch().await; // TileDiscarded(C)
    let _ = clients[0].recv_event_batch().await; // South TileDrawn(1m) at +200ms
    let draw_at = tokio::time::Instant::now();

    let (_, _, ev_kong, _) = clients[0].recv_event_batch().await;
    assert!(matches!(
        ev_kong[0].kind,
        PlayerEventKind::MeldDeclared {
            seat: Seat::South,
            meld: MeldView::ConcealedKong { .. },
            ..
        }
    ));
    let kong_ms = tokio::time::Instant::now()
        .duration_since(draw_at)
        .as_millis() as u64;
    assert!(
        (420..=680).contains(&kong_ms),
        "Standard AI ConcealedKong took {kong_ms}ms"
    );

    let kong_at = tokio::time::Instant::now();
    let (_, _, ev_follow, _) = clients[0].recv_event_batch().await;
    assert!(matches!(
        ev_follow[0].kind,
        PlayerEventKind::TileDiscarded {
            seat: Seat::South,
            ..
        }
    ));
    let followup_ms = tokio::time::Instant::now()
        .duration_since(kong_at)
        .as_millis() as u64;
    assert!(
        (700..=850).contains(&followup_ms),
        "Standard AI ConcealedKong follow-up took {followup_ms}ms"
    );
}

async fn assert_ai_win_timing(pace: TablePace, expected_range: (u64, u64)) {
    // 1. Ron: East discards 5m -> South AI wins Ron(5m).
    let ron_wall = build_preset_wall(
        &tiles("5m 1m 4m 7m 1s 4s 7s 1p 4p 7p E S W N"),
        &tiles("5m 1m 2m 3m 1s 2s 3s 4p 5p 6p 8p 8p 8p"),
        &tiles("2m 6m 9m 5s 8s 9s 2p 3p 9p E S W N"),
        &tiles("3m 7m 9m 6s 8s 9s 2p 3p 9p C F B C"),
        &tiles("1s"),
        &[],
    )
    .expect("valid ron wall");
    let ron_factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            ron_wall.clone(),
        )))
    });
    let (runtime, mut clients, snaps) =
        setup_seated_humans_match(25, ron_factory, &[Seat::East], pace).await;
    clients[0]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: snaps[0].seq,
                client_action_id: "e-5m".to_string(),
                action: PlayerAction::Discard {
                    tile: tile("5m"),
                    from_drawn: false,
                },
            },
        )
        .await;
    let _ = clients[0].recv_event_batch().await;
    let ron_start = tokio::time::Instant::now();
    let (_, _, ev_ron, _) = clients[0].recv_event_batch().await;
    assert!(matches!(
        ev_ron[0].kind,
        PlayerEventKind::HandSettled {
            settlement: mahjong_core::HandSettlement {
                outcome: HandOutcome::Win { ref winners, .. },
                ..
            }
        } if winners[0].seat == Seat::South && winners[0].win_kind == WinKind::Ron
    ));
    let ron_ms = tokio::time::Instant::now()
        .duration_since(ron_start)
        .as_millis() as u64;
    assert!(
        (expected_range.0..=expected_range.1).contains(&ron_ms),
        "{pace:?} AI Ron took {ron_ms}ms, expected {expected_range:?}"
    );

    // 2. Tsumo: East discards C -> South AI draws 5m and wins Tsumo.
    let tsumo_wall = build_preset_wall(
        &tiles("C 1m 4m 7m 1s 4s 7s 1p 4p 7p E S W N"),
        &tiles("5m 1m 2m 3m 1s 2s 3s 4p 5p 6p 8p 8p 8p"),
        &tiles("2m 6m 9m 5s 8s 9s 2p 3p 9p E S W N"),
        &tiles("3m 7m 9m 6s 8s 9s 2p 3p 9p F B C F"),
        &tiles("5m"),
        &[],
    )
    .expect("valid tsumo wall");
    let tsumo_factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            tsumo_wall.clone(),
        )))
    });
    let (runtime2, mut clients2, snaps2) =
        setup_seated_humans_match(26, tsumo_factory, &[Seat::East], pace).await;
    clients2[0]
        .send(
            &runtime2,
            &ClientMessage::SubmitAction {
                expected_seq: snaps2[0].seq,
                client_action_id: "e-c".to_string(),
                action: PlayerAction::Discard {
                    tile: tile("C"),
                    from_drawn: false,
                },
            },
        )
        .await;
    let _ = clients2[0].recv_event_batch().await;
    let _ = clients2[0].recv_event_batch().await; // South TileDrawn
    let tsumo_start = tokio::time::Instant::now();
    let (_, _, ev_tsumo, _) = clients2[0].recv_event_batch().await;
    assert!(matches!(
        ev_tsumo[0].kind,
        PlayerEventKind::HandSettled {
            settlement: mahjong_core::HandSettlement {
                outcome: HandOutcome::Win { ref winners, .. },
                ..
            }
        } if winners[0].seat == Seat::South && winners[0].win_kind == WinKind::Tsumo
    ));
    let tsumo_ms = tokio::time::Instant::now()
        .duration_since(tsumo_start)
        .as_millis() as u64;
    assert!(
        (expected_range.0..=expected_range.1).contains(&tsumo_ms),
        "{pace:?} AI Tsumo took {tsumo_ms}ms, expected {expected_range:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn ai_ron_and_tsumo_match_win_window_standard() {
    assert_ai_win_timing(TablePace::Standard, (350, 550)).await;
}

#[tokio::test(start_paused = true)]
async fn ai_ron_and_tsumo_match_win_window_fast() {
    assert_ai_win_timing(TablePace::Fast, (210, 330)).await;
}

#[tokio::test(start_paused = true)]
async fn ai_rob_kong_hu_matches_win_window_standard() {
    // 2 Humans (East, West via FixedOpposite) + 2 AIs (South, North).
    // West (Human) discards 5m -> East (Human) Pons 5m -> East draws 5m and declares AddedKong(5m)
    // -> South (AI, holding locked 46m wait on 5m) declares RobKongHu(5m) in [350..550]ms.
    let wall = build_preset_wall(
        &tiles("55m 234s 567888p EEE"),
        &tiles("46m 111m 234m 789m 9m C"),
        &tiles("5m 666s 777s 888s F SS"),
        &tiles("222p 333p 444p 33s NN"),
        &tiles("W 9p 9m 9s 1s 5m"),
        &tiles("1s"),
    )
    .expect("valid preset wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let runtime = TableRuntime::try_with_options_and_threads(
        0,
        None,
        TableOptions::with_wall_factory(27, factory)
            .with_ai_budget(DecisionBudget::Deterministic { max_samples: 8 }),
        1,
    )
    .expect("create runtime");

    let mut east = TestClient::connect(&runtime, "h0", "East").await;
    let mut west = TestClient::connect(&runtime, "h1", "West").await;
    for client in [&mut east, &mut west] {
        client
            .send(&runtime, &ClientMessage::JoinSeat { seat: None })
            .await;
        let _ = client.recv_snapshot().await;
    }
    east.drain_pending();
    west.drain_pending();
    east.send(
        &runtime,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::FixedOpposite,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    east.drain_pending();
    west.drain_pending();

    east.send(&runtime, &ClientMessage::StartMatch).await;
    let s_east = east.recv_snapshot().await;
    let s_west = west.recv_snapshot().await;
    assert_eq!(s_east.your_seat, Some(Seat::East));
    assert_eq!(s_west.your_seat, Some(Seat::West));

    // 1. East discards E.
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: s_east.seq,
            client_action_id: "rob-1".to_string(),
            action: PlayerAction::Discard {
                tile: tile("E"),
                from_drawn: false,
            },
        },
    )
    .await;

    // Wait until West receives TileDrawn (or legal actions).
    let mut west_seq = west.await_legal_actions(None).await;

    // 2. West discards 5m -> East receives Pon(5m) legal action immediately.
    west.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: west_seq,
            client_action_id: "rob-2".to_string(),
            action: PlayerAction::Discard {
                tile: tile("5m"),
                from_drawn: false,
            },
        },
    )
    .await;
    let east_pon_seq = east
        .await_legal_actions(Some(PlayerAction::Pon { tile: tile("5m") }))
        .await;

    // 3. East Pons 5m, then discards E.
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: east_pon_seq,
            client_action_id: "rob-3".to_string(),
            action: PlayerAction::Pon { tile: tile("5m") },
        },
    )
    .await;
    let east_after_pon_seq = east.await_legal_actions(None).await;
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: east_after_pon_seq,
            client_action_id: "rob-4".to_string(),
            action: PlayerAction::Discard {
                tile: tile("E"),
                from_drawn: false,
            },
        },
    )
    .await;

    // 4. West draws 9s, discards 9s -> North draws 1s, discards 1s -> East draws 5m!
    west_seq = west.await_legal_actions(None).await;
    west.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: west_seq,
            client_action_id: "rob-5".to_string(),
            action: PlayerAction::Discard {
                tile: tile("9s"),
                from_drawn: true,
            },
        },
    )
    .await;

    let east_added_kong_seq = east
        .await_legal_actions(Some(PlayerAction::AddedKong { tile: tile("5m") }))
        .await;

    // 5. East submits AddedKong(5m) -> AddedKongAttempted -> South AI RobKongHu(5m) in [350..550]ms!
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: east_added_kong_seq,
            client_action_id: "rob-6".to_string(),
            action: PlayerAction::AddedKong { tile: tile("5m") },
        },
    )
    .await;
    let (_, _, ev_attempt, _) = east.recv_event_batch().await;
    assert!(
        matches!(
            ev_attempt[0].kind,
            PlayerEventKind::AddedKongAttempted {
                seat: Seat::East,
                tile: t
            } if t == tile("5m")
        ),
        "expected AddedKongAttempted(5m), got {ev_attempt:?}"
    );
    let attempt_at = tokio::time::Instant::now();
    let (_, _, ev_rob, _) = east.recv_event_batch().await;
    assert!(matches!(
        ev_rob[0].kind,
        PlayerEventKind::HandSettled {
            settlement: mahjong_core::HandSettlement {
                outcome: HandOutcome::Win { ref winners, .. },
                ..
            }
        } if winners[0].seat == Seat::South && winners[0].win_kind == WinKind::RobKongHu
    ));
    let rob_ms = tokio::time::Instant::now()
        .duration_since(attempt_at)
        .as_millis() as u64;
    assert!(
        (350..=550).contains(&rob_ms),
        "Standard AI RobKongHu took {rob_ms}ms"
    );
}

#[tokio::test(start_paused = true)]
async fn disband_and_rematch_while_ai_is_thinking_never_pollute_new_match() {
    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let (runtime, mut clients, snaps) =
        setup_seated_humans_match(28, factory, &[Seat::East], TablePace::Standard).await;

    // East discards C -> +200ms South AI starts thinking & sleeping (~380..850ms).
    clients[0]
        .send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: snaps[0].seq,
                client_action_id: "e-c".to_string(),
                action: PlayerAction::Discard {
                    tile: tile("C"),
                    from_drawn: false,
                },
            },
        )
        .await;
    let _ = clients[0].recv_event_batch().await;
    let _ = clients[0].recv_event_batch().await; // South TileDrawn -> South AI is now in-flight!

    // Advance 100ms while South AI is sleeping, then disband and immediately rematch!
    tokio::time::advance(Duration::from_millis(100)).await;
    clients[0]
        .send(&runtime, &ClientMessage::RequestDisbandMatch)
        .await;
    let lobby_snap = clients[0].recv_snapshot().await;
    assert_eq!(lobby_snap.phase, TablePhase::Lobby);

    clients[0].send(&runtime, &ClientMessage::StartMatch).await;
    let new_snap = clients[0].recv_snapshot().await;
    assert_eq!(new_snap.phase, TablePhase::InMatch);
    assert_eq!(new_snap.seq, 2);
    assert_eq!(new_snap.your_seat, Some(Seat::East));

    // Advance 2s: expired AI job from match #1 must not emit any stale EventBatch into match #2!
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;
    assert!(
        clients[0].try_recv_msg().is_none(),
        "stale AI task from disbanded match must not emit into new match"
    );
}
