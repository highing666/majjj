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
use mahjong_server::banter::{ASYNC_BANTER_TIMEOUT, SLOW_THINK_NUDGE_DELAY};
use mahjong_server::llm::BanterOutput;
use mahjong_server::pacing::PacingMode;
use mahjong_server::persona::{PersonaId, PersonaMood};
use mahjong_server::protocol::{
    ClientMessage, QuickEmote, SeatKind, SeatingRule, ServerMessage, TablePace, TablePhase,
};
use mahjong_server::session::{BoxedWallProvider, TableOptions, TableRuntime, WallProviderFactory};
use tokio::sync::Notify;

fn east_discards_south_pons_then_east_tsumo_walls() -> WallProviderFactory {
    // Hand 1: East discards 1m; South holds 11m + 222m 333m 55s 67s 9p (Pons 1m, then discards).
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
async fn autonomous_banter_is_fully_nonblocking_with_no_opening_spam_and_delivers_slow_replies() {
    let llm = ScriptedLlmClient::from_replies(vec![
        // 1. South's post-discard self-check-in takes 3500ms: gameplay MUST advance immediately at t=0
        // without blocking on LLM, emit ZERO false outage notices, and still deliver South's banter at 3500ms!
        PlannedReply::Delayed(
            Duration::from_millis(3500),
            Ok(BanterOutput {
                speak: true,
                mood: PersonaMood::Smug,
                text: "慢想三秒半照样能落地，绝不卡出牌！".to_string(),
            }),
        ),
        // West / North routine check-ins stay silent so assertions remain crisp.
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        // Follow-up chain reply stays silent.
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
    ]);

    let (runtime, mut east, start_seq) = setup_east_south_pon_table(11, llm.clone()).await;

    // 1. No forced HandStarted opening banter fires on StartMatch!
    yield_tasks().await;
    let opening_msgs = east.drain_all();
    assert!(
        !opening_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { .. } | ServerMessage::PersonaMoodUpdated { .. }
        )),
        "HandStarted must NOT force opening banter: {opening_msgs:?}"
    );
    assert!(
        llm.captured().is_empty(),
        "StartMatch must not invoke LLM automatically"
    );

    // 2. East discards 1m -> South Pons 1m AND immediately discards at t=0 (ZERO sync blocking!),
    // triggering South's async self-check-in (3500ms) while gameplay continues unobstructed through West & North!
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

    let immediate_msgs = east.drain_all();
    assert!(
        immediate_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::EventBatch { events, .. }
                if events.iter().any(|e| matches!(e.kind, PlayerEventKind::MeldDeclared { seat: Seat::South, .. }))
        )),
        "expected South Pon EventBatch immediately at t=0: {immediate_msgs:?}"
    );
    assert!(
        immediate_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::EventBatch { events, .. }
                if events.iter().any(|e| matches!(e.kind, PlayerEventKind::TileDiscarded { seat: Seat::South, .. }))
        )),
        "South post-Pon discard must advance immediately at t=0 without waiting on 3500ms LLM: {immediate_msgs:?}"
    );
    assert!(
        !immediate_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::LlmStatusNotice { .. } | ServerMessage::TableBanter { .. }
        )),
        "slow 3500ms LLM must neither emit false outage nor finish at t=0: {immediate_msgs:?}"
    );

    // 3. Advance to t = 3600ms (> 3500ms): South's slow self-check-in lands asynchronously within the same hand!
    tokio::time::advance(Duration::from_millis(3600)).await;
    yield_tasks().await;
    let after_3600 = east.drain_all();
    assert!(
        after_3600.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter {
                seat: Seat::South,
                mood: PersonaMood::Smug,
                text,
                ..
            } if text.contains("慢想三秒半照样能落地")
        )),
        "3500ms self-check-in banter must deliver asynchronously in same hand: {after_3600:?}"
    );
    assert!(
        !after_3600
            .iter()
            .any(|m| matches!(m, ServerMessage::LlmStatusNotice { .. })),
        "healthy 3500ms reply must never emit LlmStatusNotice: {after_3600:?}"
    );

    // Verify mid-hand prompts never leaked East's hidden red dragon (`红中`).
    let captured = llm.captured();
    assert!(!captured.is_empty());
    assert!(
        !captured[0].user_prompt.contains("红中"),
        "mid-hand self-check-in prompt must never leak opponent hidden tiles: {}",
        captured[0].user_prompt
    );
}

#[tokio::test(start_paused = true)]
async fn true_llm_timeout_past_async_limit_emits_outage_notice_and_clears_on_recovery() {
    let llm = ScriptedLlmClient::from_replies(vec![
        // 1. QuickEmote reply hangs past ASYNC_BANTER_TIMEOUT (20s) -> genuine timeout!
        PlannedReply::Delayed(
            ASYNC_BANTER_TIMEOUT + Duration::from_secs(5),
            Ok(BanterOutput {
                speak: true,
                mood: PersonaMood::Smug,
                text: "超过完整超时不应该出现".to_string(),
            }),
        ),
        // 2. Subsequent QuickEmote reply recovers -> emits LlmStatusNotice { available: true } + TableBanter!
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Tilted,
            text: "网卡刚恢复，这就来回喷你！".to_string(),
        })),
        // Follow-up stays silent.
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
    ]);

    let (runtime, mut east, _start_seq) = setup_east_south_pon_table(12, llm).await;
    // Disable slow-think nudge (16s) so waiting across the 20s ASYNC_BANTER_TIMEOUT stays isolated.
    east.send(
        &runtime,
        &ClientMessage::SetSlowThinkNudge { enabled: false },
    )
    .await;
    yield_tasks().await;
    let _ = east.drain_all();

    // Trigger a hanging LLM call via QuickEmote.
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::NicePlay,
        },
    )
    .await;
    yield_tasks().await;
    let _ = east.drain_all();

    // At 5s (< 20s ASYNC_BANTER_TIMEOUT): still NO false outage notice!
    tokio::time::advance(Duration::from_secs(5)).await;
    yield_tasks().await;
    let mid_wait = east.drain_all();
    assert!(
        !mid_wait.iter().any(|m| matches!(
            m,
            ServerMessage::LlmStatusNotice {
                available: false,
                ..
            }
        )),
        "must not emit outage before full 20s ASYNC_BANTER_TIMEOUT: {mid_wait:?}"
    );

    // Crossing full ASYNC_BANTER_TIMEOUT (t = 5s + 16s = 21s, strictly between 20s timeout and 25s reply):
    // emits LlmStatusNotice { available: false } and NO fake banter!
    tokio::time::advance(Duration::from_secs(16)).await;
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
        "expected genuine LlmStatusNotice {{ available: false }} past 20s timeout, got {after_full_timeout:?}"
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
            emote: QuickEmote::IsThatIt,
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
async fn takeover_ai_assist_hand_change_and_disband_drop_banter_and_silent_mood() {
    let gate_takeover_speak = Arc::new(Notify::new());
    let gate_takeover_silent = Arc::new(Notify::new());
    let gate_nudge_assist = Arc::new(Notify::new());
    let gate_disband_silent = Arc::new(Notify::new());
    let llm = ScriptedLlmClient::from_replies(vec![
        PlannedReply::Gated(
            Arc::clone(&gate_takeover_speak),
            Ok(BanterOutput {
                speak: true,
                mood: PersonaMood::Smug,
                text: "这句不该在接管后幽灵弹出！".to_string(),
            }),
        ),
        PlannedReply::Gated(
            Arc::clone(&gate_takeover_silent),
            Ok(BanterOutput::silent(PersonaMood::Tilted)),
        ),
        PlannedReply::Gated(
            Arc::clone(&gate_nudge_assist),
            Ok(BanterOutput {
                speak: true,
                mood: PersonaMood::Mocking,
                text: "开启托管后这句催牌不该再弹出！".to_string(),
            }),
        ),
        PlannedReply::Gated(
            Arc::clone(&gate_disband_silent),
            Ok(BanterOutput::silent(PersonaMood::Plotting)),
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
    let mut observer2 = VirtualClient::connect(&runtime, "obs2", "客串二").await;
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
    let _ = observer2.recv_snapshot().await;
    yield_tasks().await;
    let _ = east.drain_all();

    // 1. Trigger a gated speak: true QuickEmote reply and take over that exact AI seat before LLM finishes.
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
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

    let _ = observer.drain_all();
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
    assert_eq!(
        takeover_snap.seats[target_seat.index()].kind,
        SeatKind::Human
    );
    assert_eq!(takeover_snap.seats[target_seat.index()].mood, None);
    let _ = east.drain_all();
    let _ = observer.drain_all();

    gate_takeover_speak.notify_waiters();
    yield_tasks().await;
    let east_after_takeover = east.drain_all();
    assert!(
        !east_after_takeover.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { .. } | ServerMessage::PersonaMoodUpdated { .. }
        )),
        "ghost TableBanter leaked after takeover: {east_after_takeover:?}"
    );

    // 2. Trigger a gated speak: false silent mood update on a second AI seat, then take it over before LLM finishes.
    tokio::time::advance(Duration::from_secs(5)).await;
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::NicePlay,
        },
    )
    .await;
    yield_tasks().await;
    let prompt1 = llm.captured()[1].system_prompt.clone();
    let target_seat2 = start_snap
        .seats
        .iter()
        .find(|s| {
            s.kind == SeatKind::Ai
                && s.seat != target_seat
                && s.persona_id
                    .is_some_and(|pid| prompt1.contains(pid.display_name()))
        })
        .map(|s| s.seat)
        .expect("find second AI seat");

    let _ = observer2.drain_all();
    observer2
        .send(
            &runtime,
            &ClientMessage::TakeOverSeat {
                seat: target_seat2,
                confirm: true,
            },
        )
        .await;
    let _ = observer2.recv_snapshot().await;
    let _ = east.drain_all();

    gate_takeover_silent.notify_waiters();
    yield_tasks().await;
    let after_silent_takeover = east.drain_all();
    assert!(
        !after_silent_takeover.iter().any(|m| matches!(
            m,
            ServerMessage::PersonaMoodUpdated { seat, .. } if *seat == target_seat2
        )),
        "stale silent PersonaMoodUpdated leaked after takeover: {after_silent_takeover:?}"
    );
    observer2
        .send(&runtime, &ClientMessage::RequestSnapshot)
        .await;
    let verify_snap2 = observer2.recv_snapshot().await;
    assert_eq!(verify_snap2.seats[target_seat2.index()].mood, None);

    // 3. Slow-think nudge fires while East is waiting -> East toggles AI assist ON then OFF before LLM returns ->
    // stale nudge MUST be dropped!
    tokio::time::advance(SLOW_THINK_NUDGE_DELAY + Duration::from_millis(100)).await;
    yield_tasks().await;
    assert_eq!(
        llm.captured().len(),
        3,
        "slow-think nudge should be in-flight"
    );
    east.send(
        &runtime,
        &ClientMessage::ToggleAiAssist {
            seat: Seat::East,
            enabled: true,
        },
    )
    .await;
    east.send(
        &runtime,
        &ClientMessage::ToggleAiAssist {
            seat: Seat::East,
            enabled: false,
        },
    )
    .await;
    let _ = east.drain_all();

    gate_nudge_assist.notify_waiters();
    yield_tasks().await;
    let after_assist = east.drain_all();
    assert!(
        !after_assist.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { text, .. } if text.contains("开启托管后这句催牌")
        )),
        "stale slow-think nudge must drop after AI assist toggle: {after_assist:?}"
    );

    // 4. Disband match #2 while a silent mood update is in-flight -> must be dropped cleanly!
    runtime.on_disconnect(observer.conn_id).await;
    runtime.on_disconnect(observer2.conn_id).await;
    east.send(&runtime, &ClientMessage::RequestDisbandMatch)
        .await;
    let lobby_snap = east.recv_snapshot().await;
    assert_eq!(lobby_snap.phase, TablePhase::Lobby);

    east.send(&runtime, &ClientMessage::StartMatch).await;
    let match2_snap = east.recv_snapshot().await;
    assert_eq!(match2_snap.phase, TablePhase::InMatch);
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::GoingBig,
        },
    )
    .await;
    yield_tasks().await;
    let _ = east.drain_all();

    east.send(&runtime, &ClientMessage::RequestDisbandMatch)
        .await;
    let lobby_snap2 = east.recv_snapshot().await;
    assert_eq!(lobby_snap2.phase, TablePhase::Lobby);
    let _ = east.drain_all();

    gate_disband_silent.notify_waiters();
    yield_tasks().await;
    let after_disband = east.drain_all();
    assert!(
        !after_disband.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { .. } | ServerMessage::PersonaMoodUpdated { .. }
        )),
        "ghost TableBanter/PersonaMoodUpdated leaked after disband: {after_disband:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn silent_mood_update_emits_persona_mood_updated_on_change_and_suppresses_noop() {
    let llm = ScriptedLlmClient::from_replies(vec![
        // 1. South's post-discard self-check-in: speak: false with mood = Tilted (differs from initial Smug)
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Tilted))),
        // 2. West's post-discard self-check-in: speak: false with mood = Plotting
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Plotting))),
        // 3. North's post-discard self-check-in: speak: false with mood = Deadpan
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
    ]);

    let (runtime, mut east, start_seq) = setup_east_south_pon_table(19, llm.clone()).await;

    // East discards 1m -> South Pons & discards (returns silent Tilted) -> emits PersonaMoodUpdated ONLY,
    // zero TableBanter, zero follow-up chain!
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

    let after_round = east.drain_all();
    assert!(
        after_round.iter().any(|m| matches!(
            m,
            ServerMessage::PersonaMoodUpdated {
                seat: Seat::South,
                persona_id: PersonaId::QuickHothead,
                mood: PersonaMood::Tilted,
            }
        )),
        "expected PersonaMoodUpdated for South Tilted, got {after_round:?}"
    );
    assert!(
        !after_round
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "silent decision must not emit TableBanter: {after_round:?}"
    );

    // Advance past chain delay (1.5s): speak: false must NOT have scheduled any follow-up banter.
    let calls_before_chain_window = llm.captured().len();
    tokio::time::advance(Duration::from_millis(1500)).await;
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

    // Unchanged mood returned with speak: false -> silent no-op (zero PersonaMoodUpdated, zero TableBanter).
    let llm_noop = ScriptedLlmClient::from_replies(vec![
        // Single AI (South, QuickHothead) starts with initial_mood = Smug -> returning silent Smug is a no-op!
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Smug))),
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
            .with_llm_client(llm_noop),
    );
    let mut host = VirtualClient::connect(&rt2, "h2", "房主").await;
    let mut west = VirtualClient::connect(&rt2, "w2", "阿西").await;
    let mut north = VirtualClient::connect(&rt2, "n2", "阿北").await;
    for (c, seat) in [
        (&mut host, Seat::East),
        (&mut west, Seat::West),
        (&mut north, Seat::North),
    ] {
        c.send(&rt2, &ClientMessage::JoinSeat { seat: Some(seat) })
            .await;
        let _ = c.recv_snapshot().await;
    }
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
    assert_eq!(
        snap2.seats[Seat::South.index()].mood,
        Some(PersonaMood::Smug)
    );
    let _ = host.drain_all();

    host.send(
        &rt2,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
    yield_tasks().await;
    let noop_msgs = host.drain_all();
    assert!(
        !noop_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::PersonaMoodUpdated { .. } | ServerMessage::TableBanter { .. }
        )),
        "unchanged silent mood must be a silent no-op: {noop_msgs:?}"
    );
}
