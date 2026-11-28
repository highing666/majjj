mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{
    no_reaction_wall, pump_ai_turn, tile, tiles, yield_tasks, PlannedReply, ScriptedLlmClient,
    VirtualClient,
};
use mahjong_ai::AiDifficulty;
use mahjong_core::{build_preset_wall, MatchMode, PlayerAction, PresetWallProvider, Seat};
use mahjong_server::banter::{BANTER_CHAIN_REPLY_DELAY, STRONG_RECENCY_MAX_SEQ_DRIFT};
use mahjong_server::llm::BanterOutput;
use mahjong_server::pacing::PacingMode;
use mahjong_server::persona::{PersonaId, PersonaMood};
use mahjong_server::protocol::{
    ClientMessage, QuickEmote, SeatingRule, ServerMessage, TablePace, TablePhase, TableSnapshot,
};
use mahjong_server::session::{BoxedWallProvider, TableOptions, TableRuntime, WallProviderFactory};
use tokio::sync::Notify;

async fn start_solo_east_table(runtime: &TableRuntime, east: &mut VirtualClient) -> TableSnapshot {
    east.send(
        runtime,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = east.recv_snapshot().await;
    east.send(
        runtime,
        &ClientMessage::SetSeatPersona {
            seat: Seat::South,
            persona_id: Some(PersonaId::QuickHothead),
        },
    )
    .await;
    east.send(
        runtime,
        &ClientMessage::SetSeatPersona {
            seat: Seat::West,
            persona_id: Some(PersonaId::TeaSarcasm),
        },
    )
    .await;
    east.send(
        runtime,
        &ClientMessage::SetSeatPersona {
            seat: Seat::North,
            persona_id: Some(PersonaId::ChaosInstigator),
        },
    )
    .await;
    east.send(
        runtime,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::KeepLobbySeats,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    east.send(runtime, &ClientMessage::StartMatch).await;
    let snap = east.recv_snapshot().await;
    assert_eq!(snap.phase, TablePhase::InMatch);
    snap
}

#[tokio::test(start_paused = true)]
async fn ai_self_checkin_fires_once_per_ai_discard_never_on_draw_and_dedups_single_in_flight() {
    // East holds 3 secret 红中 (never discarded); West holds 2 secret 发财 (never discarded).
    // No Pon/Kong/Ron across 3 full rounds so each round has exact 1 discard by East, South, West, North.
    let east_14 = tiles("1p 4p 7m 8m 9m 6s 7s 8s 2p 3p 6p C C C");
    let south_13 = tiles("123m 456m 78p 23s 55p 9s");
    let west_13 = tiles("2m 5m 8m 5s 8s 3p 6p 9p E S W F F");
    let north_13 = tiles("3m 6m 9m 4s 7s 1p 4p 7p E S W N B");
    // Draw order after East discards: South, West, North, East (4 tiles per round).
    let draws = tiles("9p B N 2m 5p N B 3m 6p B N 4m");
    let wall = build_preset_wall(&east_14, &south_13, &west_13, &north_13, &draws, &[])
        .expect("preset self-checkin wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });

    // Gate South's Round 1 self-check-in so South stays in-flight across Round 2!
    let south_gate = Arc::new(Notify::new());
    let llm = ScriptedLlmClient::from_replies(vec![
        // Round 1: South (gated), West (silent), North (silent)
        PlannedReply::Gated(
            Arc::clone(&south_gate),
            Ok(BanterOutput {
                speak: true,
                mood: PersonaMood::Plotting,
                text: "嘿嘿，这手牌突然顺起来了！".to_string(),
            }),
        ),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        // Round 2: South is STILL in-flight -> SKIPPED! Only West & North check in!
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        // Round 3 (after releasing South's gate): South, West, North all check in!
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Smug))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        // Follow-up stays silent.
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
    ]);

    let options = TableOptions::with_wall_factory(42, factory)
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let mut cur_seq = start_solo_east_table(&runtime, &mut east).await.seq;

    // 0 calls on HandStarted.
    yield_tasks().await;
    assert_eq!(llm.captured().len(), 0);

    // Round 1: East discards 1p -> South, West, North each draw & discard -> EXACTLY 3 LLM calls (1 per AI discard, NOT on draw!).
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: cur_seq,
            client_action_id: "east-r1".to_string(),
            action: PlayerAction::Discard {
                tile: tile("1p"),
                from_drawn: false,
            },
        },
    )
    .await;
    pump_ai_turn().await;
    for msg in east.drain_all() {
        if let ServerMessage::EventBatch { to_seq, .. } = msg {
            cur_seq = to_seq;
        }
    }
    let after_r1 = llm.captured();
    assert_eq!(
        after_r1.len(),
        3,
        "expected exactly 1 self-check-in per AI discard (3 total, never on draw), got {}",
        after_r1.len()
    );
    // Verify South's check-in carries precomputed hand facts and NEVER leaks opponent hidden 红中 / 发财.
    let south_req = &after_r1[0];
    assert!(south_req.system_prompt.contains("大川"));
    assert!(
        south_req.user_prompt.contains("自家出牌巡检")
            && south_req.user_prompt.contains("预计算手牌事实")
            && south_req.user_prompt.contains("一条")
            && south_req.user_prompt.contains("四条"),
        "South self-check-in missing own facts: {}",
        south_req.user_prompt
    );
    assert!(
        !south_req.user_prompt.contains("红中") && !south_req.user_prompt.contains("发财"),
        "mid-hand self-check-in leaked opponent hidden tiles: {}",
        south_req.user_prompt
    );

    // Round 2 while South is STILL in-flight: East discards 4p -> South's 2nd discard check-in is SILENTLY SKIPPED!
    // Only West and North check in (+2 calls, total = 5).
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: cur_seq,
            client_action_id: "east-r2".to_string(),
            action: PlayerAction::Discard {
                tile: tile("4p"),
                from_drawn: false,
            },
        },
    )
    .await;
    pump_ai_turn().await;
    for msg in east.drain_all() {
        if let ServerMessage::EventBatch { to_seq, .. } = msg {
            cur_seq = to_seq;
        }
    }
    let after_r2 = llm.captured();
    assert_eq!(
        after_r2.len(),
        5,
        "busy South seat must skip overlapping discard check-in (expected 3 + 2 = 5), got {}",
        after_r2.len()
    );
    assert!(
        !after_r2[3].system_prompt.contains("大川") && !after_r2[4].system_prompt.contains("大川"),
        "Round 2 calls must belong only to idle West & North"
    );

    // Release South's Round 1 gate -> South's self-check-in finishes and releases its slot!
    south_gate.notify_waiters();
    yield_tasks().await;
    let released_msgs = east.drain_all();
    assert!(
        released_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter {
                seat: Seat::South,
                mood: PersonaMood::Plotting,
                text,
                ..
            } if text.contains("顺起来了")
        )),
        "South's in-hand self-check-in must deliver cleanly once gate opens: {released_msgs:?}"
    );

    // Round 3: South is now idle again -> all 3 AIs check in (+3 calls, total = 8)!
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: cur_seq,
            client_action_id: "east-r3".to_string(),
            action: PlayerAction::Discard {
                tile: tile("2m"),
                from_drawn: false,
            },
        },
    )
    .await;
    pump_ai_turn().await;
    let _ = east.drain_all();
    let after_r3 = llm.captured();
    assert_eq!(
        after_r3.len(),
        8,
        "after releasing in-flight slot, South must check in again on Round 3 (5 + 3 = 8), got {}",
        after_r3.len()
    );
    assert!(
        after_r3[5].system_prompt.contains("大川"),
        "Round 3 first check-in should be South (大川)"
    );
}

#[tokio::test(start_paused = true)]
async fn key_table_moments_human_meld_concealed_kong_quick_emote_and_settlement_wake_one_idle_ai() {
    // Hand 1: East holds 4x 发财 (can declare ConcealedKong right on turn 1!) + 11m (can Pon South's 1m later).
    // Let's test ConcealedKong privacy + Human Pon victim preference!
    let east_14 = tiles("F F F F 1m 1m 4m 7m 1s 4s 7s 1p 4p 7p");
    let south_13 = tiles("1m 2m 5m 8m 2s 5s 8s 2p 5p 8p E S W");
    let west_13 = tiles("3m 6m 9m 3s 6s 9s 3p 6p 9p E S W B");
    let north_13 = tiles("2m 5m 8m 2s 5s 8s 2p 5p 8p E S N B");
    let draws = tiles("9m 3m 6m 9p");
    let wall = build_preset_wall(&east_14, &south_13, &west_13, &north_13, &draws, &[])
        .expect("human concealed kong wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });

    let gate_busy = Arc::new(Notify::new());
    let llm = ScriptedLlmClient::from_replies(vec![
        // 1. Human QuickEmote wakes 1 idle AI (keep gated so that AI is busy when Human declares ConcealedKong!)
        PlannedReply::Gated(
            Arc::clone(&gate_busy),
            Ok(BanterOutput::silent(PersonaMood::Deadpan)),
        ),
        // 2. Human ConcealedKong wakes 1 DIFFERENT idle AI (must skip the busy AI and hide 发财!)
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Nervous,
            text: "上来就暗杠，心里有点慌！".to_string(),
        })),
        // Follow-up stays silent.
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
    ]);

    let options = TableOptions::with_wall_factory(51, factory)
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let start_snap = start_solo_east_table(&runtime, &mut east).await;

    // 1. Human QuickEmote wakes EXACTLY 1 idle AI (gated in-flight).
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::GoingBig,
        },
    )
    .await;
    yield_tasks().await;
    let _ = east.drain_all();
    let after_emote = llm.captured();
    assert_eq!(
        after_emote.len(),
        1,
        "QuickEmote must wake exactly 1 idle AI"
    );
    let busy_persona_prompt = after_emote[0].system_prompt.clone();

    // 2. Human declares ConcealedKong(F) -> wakes EXACTLY 1 OTHER idle AI (skipping the busy AI),
    // and NEVER reveals 发财 in the prompt!
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: start_snap.seq,
            client_action_id: "east-ckong".to_string(),
            action: PlayerAction::ConcealedKong { tile: tile("F") },
        },
    )
    .await;
    yield_tasks().await;
    let kong_msgs = east.drain_all();
    assert!(
        kong_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { text, .. } if text.contains("上来就暗杠")
        )),
        "expected 1 idle AI to react to human ConcealedKong: {kong_msgs:?}"
    );
    let after_kong = llm.captured();
    assert_eq!(
        after_kong.len(),
        2,
        "Human ConcealedKong must wake exactly 1 idle AI"
    );
    assert_ne!(
        after_kong[1].system_prompt, busy_persona_prompt,
        "table-level wake-up must select from IDLE AI seats only"
    );
    assert!(
        after_kong[1]
            .user_prompt
            .contains("暗杠了一手（牌面保密未知）")
            && !after_kong[1].user_prompt.contains("发财"),
        "opponent ConcealedKong tile must stay hidden: {}",
        after_kong[1].user_prompt
    );

    gate_busy.notify_waiters();
    yield_tasks().await;
}

#[tokio::test(start_paused = true)]
async fn serial_followup_chain_waits_1s_caps_at_2_rounds_never_repeats_and_stops_on_silent() {
    // Hand 1: East instant Tsumo -> full 2-round serial chain (3 total lines from 3 distinct AIs).
    let east_win = tiles("111m 222m 333m 444m CC");
    let other_13 = tiles("58m 147s 147p E S W N B");
    let wall1 = build_preset_wall(&east_win, &other_13, &other_13, &other_13, &[], &[])
        .expect("tsumo wall");

    // Hand 2: Plain hand for QuickEmote chain early-stop test (round 1 returns speak: false!).
    let wall2 = no_reaction_wall();

    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(
            Seat::East,
            vec![wall1.clone(), wall2.clone()],
        )))
    });

    let llm = ScriptedLlmClient::from_replies(vec![
        // Hand 1 HandSettled: Opener + Follow-up #1 + Follow-up #2 (all speak: true)
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Tilted,
            text: "起手自摸红中？手气也太离谱了！".to_string(),
        })),
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Mocking,
            text: "别急嘛，这才第一小局呢。".to_string(),
        })),
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Plotting,
            text: "哈哈哈打起来打起来，瓜子我都备好了！".to_string(),
        })),
        // Hand 2 QuickEmote: Opener speaks -> Follow-up #1 returns speak: false (Tilted) -> chain ends!
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Smug,
            text: "有本事你先胡个大的看看！".to_string(),
        })),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Tilted))),
        // Should NEVER be consumed because chain ended on speak: false!
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Smug,
            text: "这句绝不该被调用！".to_string(),
        })),
    ]);

    let options = TableOptions::with_wall_factory(17, factory)
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let start_snap = start_solo_east_table(&runtime, &mut east).await;

    // Disable slow-think nudge so advancing 10s tests pure chain capping.
    east.send(
        &runtime,
        &ClientMessage::SetSlowThinkNudge { enabled: false },
    )
    .await;
    let _ = east.drain_all();

    // 1. Hand 1 Tsumo -> HandSettled opener fires immediately at t=0.
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: start_snap.seq,
            client_action_id: "h1-tsumo".to_string(),
            action: PlayerAction::Tsumo,
        },
    )
    .await;
    yield_tasks().await;

    let mut settled_seq = start_snap.seq;
    let mut settle_banters: Vec<(Seat, Option<Seat>, String)> = Vec::new();
    for msg in east.drain_all() {
        match msg {
            ServerMessage::EventBatch { to_seq, .. } => settled_seq = to_seq,
            ServerMessage::TableBanter {
                seat,
                reply_to_seat,
                text,
                ..
            } => settle_banters.push((seat, reply_to_seat, text)),
            _ => {}
        }
    }
    assert_eq!(
        settle_banters.len(),
        1,
        "expected immediate HandSettled opener"
    );
    let opener_seat = settle_banters[0].0;
    assert_eq!(settle_banters[0].1, None);

    // At +900ms (< 1s BANTER_CHAIN_REPLY_DELAY): follow-up #1 must NOT have fired yet!
    tokio::time::advance(BANTER_CHAIN_REPLY_DELAY - Duration::from_millis(100)).await;
    yield_tasks().await;
    assert!(
        east.drain_all().is_empty(),
        "follow-up must wait full ~1s delay before dispatching"
    );

    // Crossing +1050ms: Follow-up #1 fires from a DIFFERENT AI targeting opener!
    tokio::time::advance(Duration::from_millis(150)).await;
    yield_tasks().await;
    for msg in east.drain_all() {
        if let ServerMessage::TableBanter {
            seat,
            reply_to_seat,
            text,
            ..
        } = msg
        {
            settle_banters.push((seat, reply_to_seat, text));
        }
    }
    assert_eq!(settle_banters.len(), 2, "expected 1st follow-up at ~1s");
    let reply1_seat = settle_banters[1].0;
    assert_ne!(reply1_seat, opener_seat);
    assert_eq!(settle_banters[1].1, Some(opener_seat));

    // Crossing another +1050ms: Follow-up #2 fires from the 3rd DISTINCT AI targeting reply1!
    tokio::time::advance(BANTER_CHAIN_REPLY_DELAY + Duration::from_millis(50)).await;
    yield_tasks().await;
    for msg in east.drain_all() {
        if let ServerMessage::TableBanter {
            seat,
            reply_to_seat,
            text,
            ..
        } = msg
        {
            settle_banters.push((seat, reply_to_seat, text));
        }
    }
    assert_eq!(settle_banters.len(), 3, "expected 2nd follow-up at ~2s");
    let reply2_seat = settle_banters[2].0;
    assert_ne!(reply2_seat, opener_seat);
    assert_ne!(reply2_seat, reply1_seat);
    assert_eq!(settle_banters[2].1, Some(reply1_seat));

    // Advance 10 full seconds: MUST NEVER emit a 3rd follow-up (hard cap = 2 follow-up rounds / 3 utterances)!
    tokio::time::advance(Duration::from_secs(10)).await;
    yield_tasks().await;
    let extra = east.drain_all();
    assert!(
        !extra
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "chain must strictly cap at 2 follow-ups: {extra:?}"
    );

    // Verify Follow-up #2 prompt carries BOTH prior lines cleanly!
    let captured_h1 = llm.captured();
    assert_eq!(captured_h1.len(), 3);
    assert!(captured_h1[1]
        .user_prompt
        .contains("起手自摸红中？手气也太离谱了！"));
    assert!(
        captured_h1[2]
            .user_prompt
            .contains("起手自摸红中？手气也太离谱了！")
            && captured_h1[2]
                .user_prompt
                .contains("别急嘛，这才第一小局呢。"),
        "round-2 follow-up prompt should carry both prior chain lines: {}",
        captured_h1[2].user_prompt
    );

    // 2. Confirm Hand 2 -> QuickEmote opener speaks -> Follow-up #1 returns speak: false -> chain stops!
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: settled_seq,
            client_action_id: "confirm-h2".to_string(),
            action: PlayerAction::ConfirmNextHand,
        },
    )
    .await;
    yield_tasks().await;
    let _ = east.drain_all();

    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
    yield_tasks().await;
    let h2_opener = east.drain_all();
    assert!(
        h2_opener.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { text, .. } if text.contains("有本事你先胡个大的")
        )),
        "expected Hand 2 QuickEmote opener: {h2_opener:?}"
    );

    // At +1050ms: Follow-up #1 runs and returns speak: false (Tilted) -> emits PersonaMoodUpdated ONLY!
    tokio::time::advance(BANTER_CHAIN_REPLY_DELAY + Duration::from_millis(50)).await;
    yield_tasks().await;
    let h2_r1 = east.drain_all();
    assert!(
        h2_r1.iter().any(|m| matches!(
            m,
            ServerMessage::PersonaMoodUpdated {
                mood: PersonaMood::Tilted,
                ..
            }
        )),
        "silent follow-up with changed mood should still emit PersonaMoodUpdated: {h2_r1:?}"
    );
    assert!(
        !h2_r1
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "silent follow-up must not emit TableBanter: {h2_r1:?}"
    );

    // Advance 10s -> ZERO Follow-up #2 (total LLM calls stays 3 + 2 = 5)!
    tokio::time::advance(Duration::from_secs(10)).await;
    yield_tasks().await;
    assert_eq!(
        llm.captured().len(),
        5,
        "chain must stop immediately once a follow-up returns speak: false"
    );
    assert!(east.drain_all().is_empty());
}

#[tokio::test(start_paused = true)]
async fn tiered_staleness_drops_strong_recency_past_drift_while_self_checkin_and_settlement_survive(
) {
    // Hand 1: Long no-reaction wall for >12 steps of play within the same hand.
    // Hand 2: East instant Tsumo -> test ConfirmNextHand drops stale settlement banter & silent mood.
    // Hand 3: Plain wall.
    let east_h1 = tiles("1m 4m 7m 1s 4s 7s 1p 4p 7p E S W N C");
    let base_13 = tiles("2m 5m 8m 2s 5s 8s 2p 5p 8p E S W N");
    let draws_h1 = tiles(
        "3m 6m 9m 3s \
         6s 9s 3p 6p \
         9p B F C \
         1m 4m 7m 1s",
    );
    let wall1 = build_preset_wall(&east_h1, &base_13, &base_13, &base_13, &draws_h1, &[])
        .expect("long drift wall");

    let east_win = tiles("111m 222m 333m 444m CC");
    let other_13 = tiles("58m 147s 147p E S W N B");
    let wall2 = build_preset_wall(&east_win, &other_13, &other_13, &other_13, &[], &[])
        .expect("tsumo wall 2");
    let wall3 = no_reaction_wall();

    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall1.clone(),
        )))
    });

    // Part A: Strong-recency QuickEmote reply gated past STRONG_RECENCY_MAX_SEQ_DRIFT (>10 steps) -> DROPPED!
    // Part B: Self-check-in (max_seq_drift: None) gated across >12 steps in SAME hand -> DELIVERED!
    let gate_emote_stale = Arc::new(Notify::new());
    let gate_self_checkin = Arc::new(Notify::new());
    let llm = ScriptedLlmClient::from_replies(vec![
        // 1. QuickEmote reply (strong-recency, max_seq_drift = Some(10)) — gated while >10 steps elapse!
        PlannedReply::Gated(
            Arc::clone(&gate_emote_stale),
            Ok(BanterOutput {
                speak: true,
                mood: PersonaMood::Smug,
                text: "强时效过时回应不该再弹出！".to_string(),
            }),
        ),
        // 2. Round 1 first idle AI discard self-check-in (max_seq_drift = None) — gated across >12 steps!
        PlannedReply::Gated(
            Arc::clone(&gate_self_checkin),
            Ok(BanterOutput {
                speak: true,
                mood: PersonaMood::Plotting,
                text: "同小局自家巡检慢几巡也能自然落地！".to_string(),
            }),
        ),
    ]);

    let options = TableOptions::with_wall_factory(88, factory)
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let mut cur_seq = start_solo_east_table(&runtime, &mut east).await.seq;
    east.send(
        &runtime,
        &ClientMessage::SetSlowThinkNudge { enabled: false },
    )
    .await;
    let _ = east.drain_all();

    // 1. Send QuickEmote at cur_seq -> 1 idle AI enters gate_emote_stale (strong-recency).
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
    yield_tasks().await;
    let _ = east.drain_all();
    assert_eq!(llm.captured().len(), 1);

    // 2. Advance 2 full rounds (16 game events > STRONG_RECENCY_MAX_SEQ_DRIFT = 10) within Hand 1!
    // Discard closed tiles East definitely holds (`C` and `1m`), with `from_drawn: false`.
    let start_round_seq = cur_seq;
    for (idx, discard_tile) in [tile("C"), tile("1m")].into_iter().enumerate() {
        east.send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: cur_seq,
                client_action_id: format!("drift-d-{idx}"),
                action: PlayerAction::Discard {
                    tile: discard_tile,
                    from_drawn: false,
                },
            },
        )
        .await;
        pump_ai_turn().await;
        for msg in east.drain_all() {
            if let ServerMessage::EventBatch { to_seq, .. } = msg {
                cur_seq = to_seq;
            }
        }
    }
    assert!(
        cur_seq.saturating_sub(start_round_seq) > STRONG_RECENCY_MAX_SEQ_DRIFT,
        "expected > {STRONG_RECENCY_MAX_SEQ_DRIFT} seq drift, got {}",
        cur_seq.saturating_sub(start_round_seq)
    );

    // Release strong-recency QuickEmote gate -> MUST be DROPPED!
    gate_emote_stale.notify_waiters();
    yield_tasks().await;
    let after_stale_emote = east.drain_all();
    assert!(
        !after_stale_emote.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { text, .. } if text.contains("强时效过时回应")
        )),
        "strong-recency reaction past max_seq_drift must be dropped: {after_stale_emote:?}"
    );

    // Release long-lived self-check-in gate (which started in Round 1 and drifted >10 steps in same hand) -> MUST DELIVER!
    gate_self_checkin.notify_waiters();
    yield_tasks().await;
    let after_self = east.drain_all();
    assert!(
        after_self.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { text, .. } if text.contains("同小局自家巡检慢几巡也能自然落地")
        )),
        "self-check-in (max_seq_drift: None) must still deliver within same hand: {after_self:?}"
    );

    // Part C: Hand change (`ConfirmNextHand`) drops previous hand's in-flight settlement banter AND silent mood update!
    let gate_settle_next_hand = Arc::new(Notify::new());
    let llm_hand_guard = ScriptedLlmClient::from_replies(vec![
        // Settlement returns silent Tilted, gated across ConfirmNextHand -> must be dropped!
        PlannedReply::Gated(
            Arc::clone(&gate_settle_next_hand),
            Ok(BanterOutput::silent(PersonaMood::Tilted)),
        ),
    ]);
    let factory_h2: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(
            Seat::East,
            vec![wall2.clone(), wall3.clone()],
        )))
    });
    let rt2 = TableRuntime::with_options(
        8789,
        None,
        TableOptions::with_wall_factory(89, factory_h2)
            .with_pacing(PacingMode::Immediate)
            .with_llm_client(llm_hand_guard),
    );
    let mut east2 = VirtualClient::connect(&rt2, "e2", "小王").await;
    let snap2 = start_solo_east_table(&rt2, &mut east2).await;

    // East declares Tsumo -> HandSettled starts gated silent mood update.
    east2
        .send(
            &rt2,
            &ClientMessage::SubmitAction {
                expected_seq: snap2.seq,
                client_action_id: "h1-win".to_string(),
                action: PlayerAction::Tsumo,
            },
        )
        .await;
    yield_tasks().await;
    let settled_seq = east2
        .drain_all()
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::EventBatch { to_seq, .. } => Some(to_seq),
            _ => None,
        })
        .next_back()
        .expect("settled seq");

    // Advance into Hand 2 BEFORE settlement LLM finishes!
    east2
        .send(
            &rt2,
            &ClientMessage::SubmitAction {
                expected_seq: settled_seq,
                client_action_id: "confirm-h2".to_string(),
                action: PlayerAction::ConfirmNextHand,
            },
        )
        .await;
    yield_tasks().await;
    let _ = east2.drain_all();

    gate_settle_next_hand.notify_waiters();
    yield_tasks().await;
    let after_hand_change = east2.drain_all();
    assert!(
        !after_hand_change.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { .. } | ServerMessage::PersonaMoodUpdated { .. }
        )),
        "previous hand's in-flight settlement banter/silent mood must be dropped after ConfirmNextHand: {after_hand_change:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn human_pon_prefers_idle_ai_victim_and_humanized_ai_discard_triggers_checkin_once() {
    // East (Human) holds FF + junk; South (AI) holds near-tenpai hand and draws isolated F -> discards F!
    // Under PacingMode::Humanized:
    // 1. South draws F -> 0 LLM calls on draw.
    // 2. South discards F -> exactly 1 self-check-in for South (大川), even in Humanized mode.
    // 3. East (Human) Pons South's F -> wakes 1 idle AI preferring South (the victim whose discard was claimed)!
    let east_14 = tiles("W F F 1m 4m 7m 1s 4s 7s 1p 4p 7p E S");
    let south_13 = tiles("123m 456m 789s 23p 88p");
    let west_13 = tiles("2m 5m 8m 2s 5s 8s 2p 5p 9p E S N B");
    let north_13 = tiles("3m 6m 9m 3s 6s 9s 3p 6p 9p E N B C");
    let draws = tiles("F C B 9m");
    let wall = build_preset_wall(&east_14, &south_13, &west_13, &north_13, &draws, &[])
        .expect("human pon wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });

    let llm = ScriptedLlmClient::from_replies(vec![
        // 1. South's own discard self-check-in (silent so South is idle when East Pons!)
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Smug))),
        // 2. East's Human Pon reaction (must prefer idle AI victim South = 大川!)
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Tilted,
            text: "刚打的发财就被你碰走！".to_string(),
        })),
        // Follow-up stays silent.
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
    ]);

    let options = TableOptions::with_wall_factory(64, factory)
        .with_pacing(PacingMode::Humanized)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let start_snap = start_solo_east_table(&runtime, &mut east).await;

    // East discards W -> after discard release delay, South draws F, thinks, and discards F!
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: start_snap.seq,
            client_action_id: "east-w".to_string(),
            action: PlayerAction::Discard {
                tile: tile("W"),
                from_drawn: false,
            },
        },
    )
    .await;

    // Advance virtual clock in steps until East receives the Pon legal action for South's F.
    let mut pon_seq = None;
    for _ in 0..30 {
        tokio::time::advance(Duration::from_millis(400)).await;
        pump_ai_turn().await;
        for msg in east.drain_all() {
            if let ServerMessage::EventBatch {
                to_seq,
                updated_legal_actions,
                ..
            } = msg
            {
                if updated_legal_actions.contains(&PlayerAction::Pon { tile: tile("F") }) {
                    pon_seq = Some(to_seq);
                }
            }
        }
        if pon_seq.is_some() {
            break;
        }
    }
    let pon_seq = pon_seq.expect("East should be offered Pon on South's discarded F");

    // Exactly 1 LLM call so far (South's post-discard self-check-in; zero on draw!).
    let before_pon = llm.captured();
    assert_eq!(
        before_pon.len(),
        1,
        "expected exactly 1 AI self-check-in after South discarded in Humanized mode, got {}",
        before_pon.len()
    );
    assert!(before_pon[0].system_prompt.contains("大川"));
    assert!(before_pon[0].user_prompt.contains("自家出牌巡检"));

    // East declares Pon(F) -> wakes 1 idle AI preferring South (victim)!
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: pon_seq,
            client_action_id: "east-pon-f".to_string(),
            action: PlayerAction::Pon { tile: tile("F") },
        },
    )
    .await;
    yield_tasks().await;
    let pon_msgs = east.drain_all();
    assert!(
        pon_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter {
                seat: Seat::South,
                persona_id: PersonaId::QuickHothead,
                mood: PersonaMood::Tilted,
                text,
                ..
            } if text.contains("发财就被你碰走")
        )),
        "Human Pon off South's discard must wake idle victim South (大川): {pon_msgs:?}"
    );
    assert_eq!(
        llm.captured().len(),
        2,
        "Human Pon must wake exactly 1 idle AI"
    );
}
