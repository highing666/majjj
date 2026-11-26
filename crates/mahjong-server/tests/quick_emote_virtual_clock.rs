mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{no_reaction_wall, yield_tasks, ScriptedLlmClient, TempDirGuard, VirtualClient};
use mahjong_ai::AiDifficulty;
use mahjong_core::{MatchMode, PlayerAction, PresetWallProvider, Seat};
use mahjong_server::banter::{QUICK_EMOTE_COOLDOWN, SLOW_THINK_NUDGE_DELAY};
use mahjong_server::llm::{BanterGenerationRequest, BanterOutput, LlmError};
use mahjong_server::pacing::PacingMode;
use mahjong_server::persona::PersonaMood;
use mahjong_server::protocol::{
    ClientMessage, QuickEmote, SeatingRule, ServerMessage, TablePace, TablePhase,
};
use mahjong_server::session::{BoxedWallProvider, TableOptions, TableRuntime, WallProviderFactory};
use tokio::sync::Notify;

fn emote_nudge_responder(request: &BanterGenerationRequest) -> Result<BanterOutput, LlmError> {
    let user_prompt = &request.user_prompt;
    let (mood, text) = if user_prompt.contains("慢思考催牌调侃") {
        (
            PersonaMood::Mocking,
            "哟，这手牌是要算出花来吗？慢慢想别着急！".to_string(),
        )
    } else if user_prompt.contains("快捷互动回应") {
        let matched_label = QuickEmote::ALL
            .iter()
            .find(|e| user_prompt.contains(e.label()))
            .map_or("快捷互动", |e| e.label());
        (
            PersonaMood::Smug,
            format!("还「{matched_label}」呢？有本事下巡别点炮！"),
        )
    } else {
        (PersonaMood::Plotting, "开局手气稳得一批！".to_string())
    };
    Ok(BanterOutput { mood, text })
}

fn recording_emote_llm() -> Arc<ScriptedLlmClient> {
    ScriptedLlmClient::with_banter_fn(emote_nudge_responder)
}

fn gated_emote_llm(gate: Arc<Notify>) -> Arc<ScriptedLlmClient> {
    ScriptedLlmClient::with_gated_banter_fn(gate, emote_nudge_responder)
}

fn no_reaction_factory() -> WallProviderFactory {
    let wall = no_reaction_wall();
    Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    })
}

#[tokio::test(start_paused = true)]
async fn virtual_clock_quick_emote_cooldown_and_all_six_emote_ai_replies() {
    let llm = recording_emote_llm();
    let options = TableOptions::with_wall_factory(101, no_reaction_factory())
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);

    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let mut south = VirtualClient::connect(&runtime, "s1", "小李").await;

    // Lobby: quick emote must be rejected without disconnecting.
    east.send(
        &runtime,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = east.recv_snapshot().await;
    south
        .send(
            &runtime,
            &ClientMessage::JoinSeat {
                seat: Some(Seat::South),
            },
        )
        .await;
    let _ = south.recv_snapshot().await;
    let _ = east.drain_all();
    let _ = south.drain_all();

    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
    let lobby_reject = east.drain_all();
    assert!(
        lobby_reject
            .iter()
            .any(|m| matches!(m, ServerMessage::ActionRejected { .. })),
        "quick emote in Lobby must be rejected: {lobby_reject:?}"
    );

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
    let _ = south.recv_snapshot().await;
    assert_eq!(start_snap.phase, TablePhase::InMatch);
    assert!(start_snap.slow_think_nudge_enabled);

    // Disable slow-think nudge during this 25s multi-emote test so only quick-emote replies fire.
    east.send(
        &runtime,
        &ClientMessage::SetSlowThinkNudge { enabled: false },
    )
    .await;

    yield_tasks().await;
    let _ = east.drain_all();
    let _ = south.drain_all();

    // Send all 6 quick emotes spaced by QUICK_EMOTE_COOLDOWN; verify cooldown rejection in between.
    for (idx, emote) in QuickEmote::ALL.into_iter().enumerate() {
        if idx > 0 {
            tokio::time::advance(QUICK_EMOTE_COOLDOWN).await;
        }
        east.send(&runtime, &ClientMessage::SendQuickEmote { emote })
            .await;
        yield_tasks().await;

        let east_msgs = east.drain_all();
        let south_msgs = south.drain_all();

        // Both clients must see QuickEmoteBroadcast with exact label.
        for (who, msgs) in [("east", &east_msgs), ("south", &south_msgs)] {
            assert!(
                msgs.iter().any(|m| matches!(
                    m,
                    ServerMessage::QuickEmoteBroadcast {
                        seat: Seat::East,
                        emote: got_emote,
                        text,
                    } if *got_emote == emote && text == emote.label()
                )),
                "{who} missing QuickEmoteBroadcast for {emote:?}: {msgs:?}"
            );
        }

        // 2 AI seats (West & North) -> 1..=2 AI TableBanter replies referencing the emote and targeting East.
        let replies: Vec<_> = east_msgs
            .iter()
            .filter_map(|m| match m {
                ServerMessage::TableBanter {
                    seat,
                    text,
                    reply_to_seat,
                    ..
                } => Some((*seat, text.as_str(), *reply_to_seat)),
                _ => None,
            })
            .collect();
        assert!(
            (1..=2).contains(&replies.len()),
            "expected 1..=2 AI replies for {emote:?}, got {replies:?}"
        );
        for (ai_seat, text, reply_to) in &replies {
            assert!(matches!(ai_seat, Seat::West | Seat::North));
            assert_eq!(*reply_to, Some(Seat::East));
            assert!(
                text.contains(emote.label()),
                "AI reply should react to emote label {}, got: {text}",
                emote.label()
            );
        }

        // Immediate rapid spam within cooldown MUST be rejected with ActionRejected without broadcast or AI call.
        tokio::time::advance(Duration::from_millis(500)).await;
        let prompts_before = llm.captured().len();
        east.send(
            &runtime,
            &ClientMessage::SendQuickEmote {
                emote: QuickEmote::HurryUp,
            },
        )
        .await;
        yield_tasks().await;
        let spam_east = east.drain_all();
        let spam_south = south.drain_all();
        assert!(
            spam_east.iter().any(|m| matches!(
                m,
                ServerMessage::ActionRejected { reason, .. } if reason.contains("冷却")
            )),
            "expected cooldown ActionRejected, got {spam_east:?}"
        );
        assert!(
            !spam_south
                .iter()
                .any(|m| matches!(m, ServerMessage::QuickEmoteBroadcast { .. })),
            "cooldown-rejected emote must not broadcast to peers: {spam_south:?}"
        );
        assert_eq!(
            llm.captured().len(),
            prompts_before,
            "cooldown-rejected emote must not invoke LLM"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn virtual_clock_slow_think_nudge_fires_once_never_auto_plays_resets_on_action_and_respects_toggle(
) {
    let temp = TempDirGuard::new("majjj_nudge_virtual");
    let llm = recording_emote_llm();
    let options = TableOptions::with_wall_factory(202, no_reaction_factory())
        .with_pacing(PacingMode::Immediate)
        .with_data_dir(temp.path())
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);

    let mut east = VirtualClient::connect(&runtime, "e1", "慢手新手").await;
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
    let start_seq = start_snap.seq;
    assert!(start_snap.slow_think_nudge_enabled);

    // Drain initial HandStarted banter.
    yield_tasks().await;
    let _ = east.drain_all();

    // 1. Just before SLOW_THINK_NUDGE_DELAY (15.5s): zero nudge yet.
    tokio::time::advance(SLOW_THINK_NUDGE_DELAY - Duration::from_millis(500)).await;
    yield_tasks().await;
    let before_threshold = east.drain_all();
    assert!(
        !before_threshold
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "nudge must not fire before threshold: {before_threshold:?}"
    );

    // 2. Crossing 16s threshold: fires EXACTLY 1 playful nudge targeting East, NEVER auto-discards or enables ai_assisted!
    tokio::time::advance(Duration::from_millis(600)).await;
    yield_tasks().await;
    let on_threshold = east.drain_all();
    let nudges: Vec<_> = on_threshold
        .iter()
        .filter(|m| {
            matches!(
                m,
                ServerMessage::TableBanter {
                    reply_to_seat: Some(Seat::East),
                    ..
                }
            )
        })
        .collect();
    assert_eq!(
        nudges.len(),
        1,
        "expected exactly 1 slow-think nudge, got {on_threshold:?}"
    );
    assert!(
        !on_threshold
            .iter()
            .any(|m| matches!(m, ServerMessage::EventBatch { .. })),
        "slow-think nudge must NEVER auto-discard or mutate game events: {on_threshold:?}"
    );

    // 3. Wait another 60s on the same turn: MUST NOT fire a second nudge and still no auto-play!
    tokio::time::advance(Duration::from_secs(60)).await;
    yield_tasks().await;
    let long_wait = east.drain_all();
    assert!(
        !long_wait.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { .. } | ServerMessage::EventBatch { .. }
        )),
        "same turn must never nudge twice or auto-play even after 76s: {long_wait:?}"
    );

    // Confirm East is still on the exact same seq and NOT ai_assisted.
    east.send(&runtime, &ClientMessage::RequestSnapshot).await;
    let check_snap = east.recv_snapshot().await;
    assert_eq!(check_snap.seq, start_seq);
    assert!(!check_snap.seats[Seat::East.index()].ai_assisted);
    let first_discard = check_snap
        .player_view
        .as_ref()
        .expect("player_view")
        .legal_actions
        .iter()
        .find(|a| matches!(a, PlayerAction::Discard { .. }))
        .copied()
        .expect("legal discard action for East");

    // 4. Human discards -> turn advances through South/West/North AIs and returns to East at a new seq -> timer resets!
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: start_seq,
            client_action_id: "east-discard-1".to_string(),
            action: first_discard,
        },
    )
    .await;

    // Wait briefly (real thread slice for Rayon AI pool) until East receives its next turn.
    let mut turn2_snap = None;
    for _ in 0..80 {
        tokio::task::yield_now().await;
        std::thread::sleep(Duration::from_millis(5));
        let _ = east.drain_all();
        east.send(&runtime, &ClientMessage::RequestSnapshot).await;
        let snap = east.recv_snapshot().await;
        if snap.seq > start_seq
            && snap
                .player_view
                .as_ref()
                .is_some_and(|v| !v.legal_actions.is_empty())
        {
            turn2_snap = Some(snap);
            break;
        }
    }
    let turn2_snap = turn2_snap.expect("match should advance to East's next turn");
    let turn2_seq = turn2_snap.seq;
    let _ = east.drain_all();

    // Verify timer was cleanly reset for Turn 2: waiting 16s triggers 1 fresh nudge on Turn 2!
    tokio::time::advance(SLOW_THINK_NUDGE_DELAY + Duration::from_millis(100)).await;
    yield_tasks().await;
    let turn2_nudges: Vec<_> = east
        .drain_all()
        .into_iter()
        .filter(|m| {
            matches!(
                m,
                ServerMessage::TableBanter {
                    reply_to_seat: Some(Seat::East),
                    ..
                }
            )
        })
        .collect();
    assert_eq!(
        turn2_nudges.len(),
        1,
        "reset timer should trigger exactly 1 nudge on Turn 2"
    );

    // Advance to Turn 3 so we can verify toggling SetSlowThinkNudge { enabled: false } cancels an armed timer.
    let second_discard = turn2_snap
        .player_view
        .as_ref()
        .expect("turn2 player_view")
        .legal_actions
        .iter()
        .find(|a| matches!(a, PlayerAction::Discard { .. }))
        .copied()
        .expect("legal discard for Turn 2");
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: turn2_seq,
            client_action_id: "east-discard-2".to_string(),
            action: second_discard,
        },
    )
    .await;
    let mut turn3_reached = false;
    for _ in 0..80 {
        tokio::task::yield_now().await;
        std::thread::sleep(Duration::from_millis(5));
        let _ = east.drain_all();
        east.send(&runtime, &ClientMessage::RequestSnapshot).await;
        let snap = east.recv_snapshot().await;
        if snap.seq > turn2_seq
            && snap
                .player_view
                .as_ref()
                .is_some_and(|v| !v.legal_actions.is_empty())
        {
            turn3_reached = true;
            break;
        }
    }
    assert!(turn3_reached, "match should advance to East's Turn 3");
    let _ = east.drain_all();

    // Advance 10s (< 16s), then toggle SetSlowThinkNudge { enabled: false } -> MUST persist & suppress nudge!
    tokio::time::advance(Duration::from_secs(10)).await;
    yield_tasks().await;
    east.send(
        &runtime,
        &ClientMessage::SetSlowThinkNudge { enabled: false },
    )
    .await;
    let toggle_msgs = east.drain_all();
    assert!(
        toggle_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::LobbyUpdated {
                slow_think_nudge_enabled: false,
                ..
            }
        )),
        "SetSlowThinkNudge must broadcast LobbyUpdated with slow_think_nudge_enabled: false, got {toggle_msgs:?}"
    );

    // Advance another 30s -> ZERO nudge banter!
    tokio::time::advance(Duration::from_secs(30)).await;
    yield_tasks().await;
    let suppressed = east.drain_all();
    assert!(
        !suppressed
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "nudge must stay completely silent when toggled off: {suppressed:?}"
    );

    // Verify persistence across a fresh TableRuntime reopened from the same data_dir!
    let restarted_options = TableOptions::with_wall_factory(203, no_reaction_factory())
        .with_pacing(PacingMode::Immediate)
        .with_data_dir(temp.path())
        .with_llm_client(llm.clone());
    let restarted_runtime = TableRuntime::with_options(8789, None, restarted_options);
    let mut east_reloaded = VirtualClient::connect(&restarted_runtime, "e2", "慢手新手").await;
    east_reloaded
        .send(&restarted_runtime, &ClientMessage::RequestSnapshot)
        .await;
    let reloaded_snap = east_reloaded.recv_snapshot().await;
    assert!(
        !reloaded_snap.slow_think_nudge_enabled,
        "slow_think_nudge_enabled=false must persist across restart"
    );

    // Start match on restarted runtime and wait 30s -> STILL silent!
    east_reloaded
        .send(
            &restarted_runtime,
            &ClientMessage::JoinSeat {
                seat: Some(Seat::East),
            },
        )
        .await;
    let _ = east_reloaded.recv_snapshot().await;
    east_reloaded
        .send(&restarted_runtime, &ClientMessage::StartMatch)
        .await;
    let _ = east_reloaded.recv_snapshot().await;
    yield_tasks().await;
    let _ = east_reloaded.drain_all();

    tokio::time::advance(Duration::from_secs(30)).await;
    yield_tasks().await;
    let restarted_wait = east_reloaded.drain_all();
    assert!(
        !restarted_wait
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "persisted disabled nudge must remain silent after restart: {restarted_wait:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn virtual_clock_in_flight_quick_emote_reply_is_dropped_after_seat_takeover() {
    let gate = Arc::new(Notify::new());
    let llm = gated_emote_llm(Arc::clone(&gate));
    let options = TableOptions::with_wall_factory(303, no_reaction_factory())
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);

    let mut east = VirtualClient::connect(&runtime, "e1", "房主").await;
    let mut obs1 = VirtualClient::connect(&runtime, "o1", "接管1").await;
    let mut obs2 = VirtualClient::connect(&runtime, "o2", "接管2").await;
    let mut obs3 = VirtualClient::connect(&runtime, "o3", "接管3").await;

    east.send(
        &runtime,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = east.recv_snapshot().await;
    east.send(&runtime, &ClientMessage::StartMatch).await;
    let _ = east.recv_snapshot().await;
    yield_tasks().await;
    let _ = east.drain_all();

    // Send a quick emote while LLM is gated -> 2 AI reply tasks are in-flight.
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::GoingBig,
        },
    )
    .await;
    yield_tasks().await;
    let emote_msgs = east.drain_all();
    assert!(emote_msgs.iter().any(|m| matches!(
        m,
        ServerMessage::QuickEmoteBroadcast {
            seat: Seat::East,
            emote: QuickEmote::GoingBig,
            ..
        }
    )));

    // Take over all 3 AI seats before LLM returns -> bumps seat_generation and converts seats to Human!
    for (obs, seat) in [
        (&mut obs1, Seat::South),
        (&mut obs2, Seat::West),
        (&mut obs3, Seat::North),
    ] {
        obs.send(
            &runtime,
            &ClientMessage::TakeOverSeat {
                seat,
                confirm: true,
            },
        )
        .await;
        let _ = obs.recv_snapshot().await;
    }
    let _ = east.drain_all();

    // Unblock LLM -> all stale in-flight replies MUST be strictly dropped!
    gate.notify_waiters();
    yield_tasks().await;
    let after_release = east.drain_all();
    assert!(
        !after_release
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "stale quick-emote AI replies must be dropped after takeover: {after_release:?}"
    );
}
