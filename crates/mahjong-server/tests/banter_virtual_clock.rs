mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{
    no_reaction_wall, pump_ai_turn, tile, tiles, yield_tasks, PlannedReply, ScriptedLlmClient,
    VirtualClient,
};
use mahjong_ai::AiDifficulty;
use mahjong_core::{
    build_preset_wall, MatchMode, PlayerAction, PlayerEventKind, PresetWallProvider, Seat,
};
use mahjong_server::banter::ASYNC_BANTER_TIMEOUT;
use mahjong_server::llm::BanterOutput;
use mahjong_server::pacing::PacingMode;
use mahjong_server::persona::{PersonaId, PersonaMood};
use mahjong_server::protocol::{
    ClientMessage, SeatKind, SeatingRule, ServerMessage, TablePace, TablePhase,
};
use mahjong_server::session::{BoxedWallProvider, TableOptions, TableRuntime, WallProviderFactory};
use tokio::sync::Notify;

fn east_discards_south_pons_then_east_tsumo_walls() -> WallProviderFactory {
    // Hand 1: East discards 1m; South holds 11m + 222m 333m 55s 67s 9p (NOT tenpai on 1m,
    // so South Pons 1m instead of Ron'ing, advancing to 1-shanten / tenpai!).
    let east_14 = tiles("1m 4m 7m 1s 4s 7s 1p 4p 7p E S W N C");
    let south_13 = tiles("11m 222m 333m 55s 67s 9p");
    let west_13 = tiles("6m 9m 2s 5s 8s 2p 5p 8p E S W N B");
    let north_13 = tiles("6m 9m 3s 6s 9s 3p 6p 9p E S W N F");
    let wall1 =
        build_preset_wall(&east_14, &south_13, &west_13, &north_13, &[], &[]).expect("pon wall");

    // Hand 2: East deals instant Tsumo for settlement test.
    let east_win = tiles("111m 222m 333m 444m CC");
    let other_13 = tiles("58m 147s 147p E S W N B");
    let wall2 = build_preset_wall(&east_win, &other_13, &other_13, &other_13, &[], &[])
        .expect("tsumo wall");

    Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(
            Seat::East,
            vec![wall1.clone(), wall2.clone()],
        )))
    })
}

async fn setup_east_south_pon_table(
    seed: u64,
    llm: Arc<ScriptedLlmClient>,
) -> (TableRuntime, VirtualClient, u64) {
    let options =
        TableOptions::with_wall_factory(seed, east_discards_south_pons_then_east_tsumo_walls())
            .with_pacing(PacingMode::Immediate)
            .with_llm_client(llm);
    let runtime = TableRuntime::with_options(8789, None, options);

    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    east.send(
        &runtime,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = east.recv_snapshot().await;
    east.send(
        &runtime,
        &ClientMessage::SetSeatPersona {
            seat: Seat::South,
            persona_id: Some(PersonaId::QuickHothead),
        },
    )
    .await;
    east.send(
        &runtime,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::KeepLobbySeats,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    east.send(&runtime, &ClientMessage::StartMatch).await;
    let start_snap = east.recv_snapshot().await;
    assert_eq!(start_snap.phase, TablePhase::InMatch);
    (runtime, east, start_snap.seq)
}

#[tokio::test(start_paused = true)]
async fn highlight_sync_cap_unblocks_at_1500ms_without_false_outage_and_delivers_1800ms_banter() {
    let llm = ScriptedLlmClient::from_replies(vec![
        // 1. HandStarted opening banter (fast)
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Plotting,
            text: "开局手气来了，你们小心点炮！".to_string(),
        })),
        // 2. MeldDeclared (South Pon) takes 1800ms: slightly slower than 1500ms sync cap,
        // well within 6000ms ASYNC_BANTER_TIMEOUT -> gameplay unblocks at 1500ms with NO outage notice,
        // and the banter still arrives asynchronously at 1800ms!
        PlannedReply::Delayed(
            Duration::from_millis(1800),
            Ok(BanterOutput {
                speak: true,
                mood: PersonaMood::Smug,
                text: "碰你这张一万，照样稳稳接上！".to_string(),
            }),
        ),
        // 3. Follow-up chain reply after Pon banter commits
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Mocking,
            text: "刚碰一张就飘成这样？".to_string(),
        })),
    ]);

    let (runtime, mut east, start_seq) = setup_east_south_pon_table(11, llm.clone()).await;

    // Let async HandStarted banter finish.
    yield_tasks().await;
    let opening_msgs = east.drain_all();
    assert!(
        opening_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter {
                mood: PersonaMood::Plotting,
                text,
                ..
            } if text.contains("开局手气来了")
        )),
        "expected HandStarted opening TableBanter, got {opening_msgs:?}"
    );

    // East discards 1m -> South AI immediately Pons 1m (PacingMode::Immediate).
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: start_seq,
            client_action_id: "discard-1m".to_string(),
            action: PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        },
    )
    .await;
    pump_ai_turn().await;

    let after_pon = east.drain_all();
    assert!(
        after_pon.iter().any(|m| matches!(
            m,
            ServerMessage::EventBatch { events, .. }
                if events.iter().any(|e| matches!(e.kind, PlayerEventKind::MeldDeclared { seat: Seat::South, .. }))
        )),
        "expected South Pon EventBatch, got {after_pon:?}"
    );

    // At t = 1400ms (before 1500ms sync cap), South's follow-up discard is still held.
    tokio::time::advance(Duration::from_millis(1400)).await;
    yield_tasks().await;
    let mid_wait = east.drain_all();
    assert!(
        !mid_wait.iter().any(|m| matches!(
            m,
            ServerMessage::LlmStatusNotice { .. } | ServerMessage::TableBanter { .. }
        )),
        "must neither fail nor finish early at 1400ms: {mid_wait:?}"
    );

    // Crossing t = 1550ms (> 1500ms sync cap): gameplay unblocks (South discards) with ZERO false outage notice!
    tokio::time::advance(Duration::from_millis(150)).await;
    pump_ai_turn().await;
    let after_cap = east.drain_all();
    assert!(
        !after_cap
            .iter()
            .any(|m| matches!(m, ServerMessage::LlmStatusNotice { .. })),
        "1500ms sync cap must NOT emit a false LlmStatusNotice outage: {after_cap:?}"
    );
    assert!(
        after_cap.iter().any(|m| matches!(
            m,
            ServerMessage::EventBatch { events, .. }
                if events.iter().any(|e| matches!(e.kind, PlayerEventKind::TileDiscarded { seat: Seat::South, .. }))
        )),
        "AI follow-up discard must proceed right after 1500ms cap: {after_cap:?}"
    );

    // Crossing t = 1850ms (> 1800ms): the background highlight call finishes and still delivers TableBanter!
    tokio::time::advance(Duration::from_millis(300)).await;
    yield_tasks().await;
    let after_1800 = east.drain_all();
    assert!(
        after_1800.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter {
                seat: Seat::South,
                mood: PersonaMood::Smug,
                text,
                ..
            } if text.contains("碰你这张一万，照样稳稳接上")
        )),
        "1800ms highlight banter must still be delivered asynchronously after 1500ms unblock: {after_1800:?}"
    );
    assert!(
        !after_1800
            .iter()
            .any(|m| matches!(m, ServerMessage::LlmStatusNotice { .. })),
        "no outage notice should fire for an 1800ms reply: {after_1800:?}"
    );

    // Verify mid-hand prompts never leaked East's hidden red dragon (`红中`) into South's Pon prompt.
    let captured = llm.captured();
    assert!(captured.len() >= 2);
    assert!(
        !captured[1].user_prompt.contains("红中"),
        "mid-hand Pon prompt must never leak opponent hidden tiles: {}",
        captured[1].user_prompt
    );
}

#[tokio::test(start_paused = true)]
async fn true_llm_timeout_past_async_limit_emits_outage_notice_and_clears_on_recovery() {
    let llm = ScriptedLlmClient::from_replies(vec![
        // 1. HandStarted opening banter (fast)
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Plotting,
            text: "开局手气来了！".to_string(),
        })),
        // 2. MeldDeclared (South Pon) hangs past ASYNC_BANTER_TIMEOUT (20s) -> genuine timeout!
        PlannedReply::Delayed(
            ASYNC_BANTER_TIMEOUT + Duration::from_secs(5),
            Ok(BanterOutput {
                speak: true,
                mood: PersonaMood::Smug,
                text: "超过完整超时不应该出现".to_string(),
            }),
        ),
        // 3. Subsequent quick emote reply recovers -> emits LlmStatusNotice { available: true } + TableBanter!
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Tilted,
            text: "网卡刚恢复，这就来回喷你！".to_string(),
        })),
    ]);

    let (runtime, mut east, start_seq) = setup_east_south_pon_table(12, llm).await;
    // Disable slow-think nudge (16s) so waiting across the 20s ASYNC_BANTER_TIMEOUT doesn't fire a nudge mid-wait.
    east.send(
        &runtime,
        &ClientMessage::SetSlowThinkNudge { enabled: false },
    )
    .await;
    yield_tasks().await;
    let _ = east.drain_all();

    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: start_seq,
            client_action_id: "discard-1m".to_string(),
            action: PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        },
    )
    .await;
    pump_ai_turn().await;
    let _ = east.drain_all();

    // At 1600ms: gameplay unblocks, still NO false outage notice yet!
    tokio::time::advance(Duration::from_millis(1600)).await;
    pump_ai_turn().await;
    let after_cap = east.drain_all();
    assert!(
        !after_cap.iter().any(|m| matches!(
            m,
            ServerMessage::LlmStatusNotice {
                available: false,
                ..
            }
        )),
        "must not emit outage at 1600ms before full ASYNC_BANTER_TIMEOUT: {after_cap:?}"
    );
    assert!(
        after_cap.iter().any(|m| matches!(
            m,
            ServerMessage::EventBatch { events, .. }
                if events.iter().any(|e| matches!(e.kind, PlayerEventKind::TileDiscarded { seat: Seat::South, .. }))
        )),
        "gameplay must unblock at 1500ms even when LLM hangs 10s: {after_cap:?}"
    );

    // Crossing full ASYNC_BANTER_TIMEOUT (6000ms total): emits LlmStatusNotice { available: false } and NO fake banter!
    tokio::time::advance(ASYNC_BANTER_TIMEOUT).await;
    yield_tasks().await;
    let after_full_timeout = east.drain_all();
    assert!(
        after_full_timeout.iter().any(|m| matches!(
            m,
            ServerMessage::LlmStatusNotice {
                available: false,
                ..
            }
        )),
        "expected genuine LlmStatusNotice {{ available: false }} past 6000ms timeout, got {after_full_timeout:?}"
    );
    assert!(
        !after_full_timeout
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "must never emit fake banter on timeout: {after_full_timeout:?}"
    );

    // Triggering a fresh request (QuickEmote) succeeds -> emits LlmStatusNotice { available: true } + TableBanter!
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: mahjong_server::protocol::QuickEmote::IsThatIt,
        },
    )
    .await;
    yield_tasks().await;
    let recovery = east.drain_all();
    assert!(
        recovery.iter().any(|m| matches!(
            m,
            ServerMessage::LlmStatusNotice {
                available: true,
                ..
            }
        )),
        "expected recovery LlmStatusNotice {{ available: true }}, got {recovery:?}"
    );
    assert!(
        recovery.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { text, .. } if text.contains("网卡刚恢复")
        )),
        "expected recovered TableBanter, got {recovery:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn takeover_and_disband_drop_in_flight_banter_without_ghost_lines() {
    let gate1 = Arc::new(Notify::new());
    let gate2 = Arc::new(Notify::new());
    let llm = ScriptedLlmClient::from_replies(vec![
        PlannedReply::Gated(
            Arc::clone(&gate1),
            Ok(BanterOutput {
                speak: true,
                mood: PersonaMood::Smug,
                text: "这句不该在接管后幽灵弹出！".to_string(),
            }),
        ),
        PlannedReply::Gated(
            Arc::clone(&gate2),
            Ok(BanterOutput {
                speak: true,
                mood: PersonaMood::Tilted,
                text: "这句不该在解散后幽灵弹出！".to_string(),
            }),
        ),
    ]);

    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let options = TableOptions::with_wall_factory(7, factory)
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);

    let mut east = VirtualClient::connect(&runtime, "host", "房主").await;
    let mut observer = VirtualClient::connect(&runtime, "obs", "客串").await;
    east.send(
        &runtime,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = east.recv_snapshot().await;
    east.send(
        &runtime,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::KeepLobbySeats,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    east.send(&runtime, &ClientMessage::StartMatch).await;
    let start_snap = east.recv_snapshot().await;
    let _ = observer.recv_snapshot().await;
    yield_tasks().await;

    let prompt0 = llm.captured()[0].system_prompt.clone();
    let target_seat = start_snap
        .seats
        .iter()
        .find(|s| {
            s.kind == SeatKind::Ai
                && s.persona_id
                    .is_some_and(|pid| prompt0.contains(pid.display_name()))
        })
        .map(|s| s.seat)
        .expect("find speaking AI seat");

    // Take over that exact AI seat while its LLM request is still in-flight!
    observer
        .send(
            &runtime,
            &ClientMessage::TakeOverSeat {
                seat: target_seat,
                confirm: true,
            },
        )
        .await;
    let takeover_snap = observer.recv_snapshot().await;
    assert_eq!(takeover_snap.your_seat, Some(target_seat));
    assert_eq!(
        takeover_snap.seats[target_seat.index()].kind,
        SeatKind::Human
    );
    assert_eq!(takeover_snap.seats[target_seat.index()].mood, None);
    let _ = east.drain_all();
    let _ = observer.drain_all();

    // Release gate1 -> stale in-flight response MUST be strictly dropped!
    gate1.notify_waiters();
    yield_tasks().await;
    let east_after_takeover = east.drain_all();
    let obs_after_takeover = observer.drain_all();
    assert!(
        !east_after_takeover
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "ghost TableBanter leaked to East after takeover: {east_after_takeover:?}"
    );
    assert!(
        !obs_after_takeover
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "ghost TableBanter leaked to taken-over seat: {obs_after_takeover:?}"
    );

    observer
        .send(&runtime, &ClientMessage::RequestSnapshot)
        .await;
    let verify_snap = observer.recv_snapshot().await;
    assert_eq!(verify_snap.seats[target_seat.index()].mood, None);

    // Disband match #2 while gate2 is in-flight.
    runtime.on_disconnect(observer.conn_id).await;
    east.send(&runtime, &ClientMessage::RequestDisbandMatch)
        .await;
    let lobby_snap = east.recv_snapshot().await;
    assert_eq!(lobby_snap.phase, TablePhase::Lobby);

    east.send(&runtime, &ClientMessage::StartMatch).await;
    let match2_snap = east.recv_snapshot().await;
    assert_eq!(match2_snap.phase, TablePhase::InMatch);
    yield_tasks().await;

    east.send(&runtime, &ClientMessage::RequestDisbandMatch)
        .await;
    let lobby_snap2 = east.recv_snapshot().await;
    assert_eq!(lobby_snap2.phase, TablePhase::Lobby);
    let _ = east.drain_all();

    gate2.notify_waiters();
    yield_tasks().await;
    let after_disband = east.drain_all();
    assert!(
        !after_disband
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "ghost TableBanter leaked after disband: {after_disband:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn silent_mood_update_emits_persona_mood_updated_on_change_suppresses_noop_and_unblocks_sync()
{
    let llm = ScriptedLlmClient::from_replies(vec![
        // 1. HandStarted opening decision: speak: false with mood = Plotting (differs from initial Deadpan)
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Plotting))),
        // 2. MeldDeclared (South Pon, pending_sync): speak: false with mood = Tilted -> unblocks sync cleanly!
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Tilted))),
        // 3. Subsequent QuickEmote replies: speak: false with SAME mood (Tilted for South, Deadpan/Plotting unchanged)
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Tilted))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
    ]);

    let (runtime, mut east, start_seq) = setup_east_south_pon_table(19, llm.clone()).await;

    // 1. Opening decision: speak: false + changed mood -> emits PersonaMoodUpdated ONLY (zero TableBanter).
    yield_tasks().await;
    let opening_msgs = east.drain_all();
    assert!(
        !opening_msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "speak: false must never emit TableBanter: {opening_msgs:?}"
    );
    let opening_mood_updates: Vec<_> = opening_msgs
        .iter()
        .filter(|m| matches!(m, ServerMessage::PersonaMoodUpdated { .. }))
        .collect();
    assert_eq!(
        opening_mood_updates.len(),
        1,
        "expected exactly 1 PersonaMoodUpdated on changed silent mood: {opening_msgs:?}"
    );
    assert!(matches!(
        opening_mood_updates[0],
        ServerMessage::PersonaMoodUpdated {
            mood: PersonaMood::Plotting,
            ..
        }
    ));

    // 2. East discards 1m -> South Pons 1m (pending_sync highlight) -> South resolves immediately with
    // speak: false + Tilted -> emits PersonaMoodUpdated for South, zero TableBanter, zero follow-up,
    // AND unblocks South's discard without waiting for 1500ms sync cap!
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: start_seq,
            client_action_id: "silent-pon-1m".to_string(),
            action: PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        },
    )
    .await;
    pump_ai_turn().await;
    yield_tasks().await;

    let after_pon = east.drain_all();
    assert!(
        after_pon.iter().any(|m| matches!(
            m,
            ServerMessage::PersonaMoodUpdated {
                seat: Seat::South,
                persona_id: PersonaId::QuickHothead,
                mood: PersonaMood::Tilted,
            }
        )),
        "expected PersonaMoodUpdated for South Tilted, got {after_pon:?}"
    );
    assert!(
        !after_pon
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "silent Pon decision must not emit TableBanter: {after_pon:?}"
    );
    assert!(
        after_pon.iter().any(|m| matches!(
            m,
            ServerMessage::EventBatch { events, .. }
                if events.iter().any(|e| matches!(e.kind, PlayerEventKind::TileDiscarded { seat: Seat::South, .. }))
        )),
        "pending_sync must unblock immediately on speak: false without waiting for sync cap: {after_pon:?}"
    );

    // Advance past chain delay (1s): speak: false must NOT have scheduled any follow-up banter.
    let calls_before_chain_window = llm.captured().len();
    tokio::time::advance(Duration::from_millis(1000)).await;
    yield_tasks().await;
    assert_eq!(
        llm.captured().len(),
        calls_before_chain_window,
        "speak: false must not schedule any follow-up banter chain"
    );

    // Snapshot reflects South's updated silent mood.
    east.send(&runtime, &ClientMessage::RequestSnapshot).await;
    let snap = east.recv_snapshot().await;
    assert_eq!(
        snap.seats[Seat::South.index()].mood,
        Some(PersonaMood::Tilted)
    );
    let _ = east.drain_all();

    // 3. Same mood returned with speak: false -> silent no-op (zero PersonaMoodUpdated, zero TableBanter).
    let gate_takeover = Arc::new(Notify::new());
    let llm_noop = ScriptedLlmClient::from_replies(vec![
        // Opening: South (QuickHothead) starts with initial_mood = Smug -> returning silent Smug is a no-op!
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Smug))),
        // Second call gated for takeover test
        PlannedReply::Gated(
            Arc::clone(&gate_takeover),
            Ok(BanterOutput::silent(PersonaMood::Tilted)),
        ),
    ]);
    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let rt2 = TableRuntime::with_options(
        8789,
        None,
        TableOptions::with_wall_factory(23, factory)
            .with_pacing(PacingMode::Immediate)
            .with_llm_client(llm_noop.clone()),
    );
    let mut host = VirtualClient::connect(&rt2, "h2", "房主").await;
    let mut guest = VirtualClient::connect(&rt2, "g2", "客串").await;
    host.send(
        &rt2,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = host.recv_snapshot().await;
    host.send(
        &rt2,
        &ClientMessage::SetSeatPersona {
            seat: Seat::South,
            persona_id: Some(PersonaId::QuickHothead),
        },
    )
    .await;
    host.send(&rt2, &ClientMessage::StartMatch).await;
    let snap2 = host.recv_snapshot().await;
    let _ = guest.recv_snapshot().await;
    assert_eq!(
        snap2.seats[Seat::South.index()].mood,
        Some(PersonaMood::Smug)
    );
    yield_tasks().await;
    let opening_noop = host.drain_all();
    assert!(
        !opening_noop.iter().any(|m| matches!(
            m,
            ServerMessage::PersonaMoodUpdated { .. } | ServerMessage::TableBanter { .. }
        )),
        "unchanged silent mood must be a silent no-op: {opening_noop:?}"
    );

    // 4. Stale speak: false after seat takeover is dropped silently (zero PersonaMoodUpdated, mood stays None).
    host.send(
        &rt2,
        &ClientMessage::SendQuickEmote {
            emote: mahjong_server::protocol::QuickEmote::IsThatIt,
        },
    )
    .await;
    yield_tasks().await;
    let captured_after_emote = llm_noop.captured();
    assert!(captured_after_emote.len() >= 2);
    let prompt_emote = &captured_after_emote[1].system_prompt;
    let target_seat = snap2
        .seats
        .iter()
        .find(|s| {
            s.kind == SeatKind::Ai
                && s.persona_id
                    .is_some_and(|pid| prompt_emote.contains(pid.display_name()))
        })
        .map(|s| s.seat)
        .expect("find target AI seat");

    let _ = guest.drain_all();
    guest
        .send(
            &rt2,
            &ClientMessage::TakeOverSeat {
                seat: target_seat,
                confirm: true,
            },
        )
        .await;
    let _ = guest.recv_snapshot().await;
    let _ = host.drain_all();
    let _ = guest.drain_all();

    gate_takeover.notify_waiters();
    yield_tasks().await;
    let after_stale = host.drain_all();
    assert!(
        !after_stale.iter().any(|m| matches!(
            m,
            ServerMessage::PersonaMoodUpdated { seat, .. } if *seat == target_seat
        )),
        "stale silent mood update after takeover must be dropped: {after_stale:?}"
    );
    guest.send(&rt2, &ClientMessage::RequestSnapshot).await;
    let verify_snap = guest.recv_snapshot().await;
    assert_eq!(verify_snap.seats[target_seat.index()].mood, None);
}
