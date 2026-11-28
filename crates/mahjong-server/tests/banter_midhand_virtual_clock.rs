mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{
    no_reaction_wall, pump_ai_turn, start_solo_east_table, tile, tiles, yield_tasks, PlannedReply,
    ScriptedLlmClient, VirtualClient,
};
use mahjong_ai::DecisionBudget;
use mahjong_core::{
    build_preset_wall, HandOutcome, HandSettlement, MatchMode, PlayerAction, PlayerEventKind,
    PresetWallProvider, Seat,
};
use mahjong_server::banter::{
    BANTER_CHAIN_REPLY_DELAY, SLOW_THINK_NUDGE_DELAY, STRONG_RECENCY_MAX_SEQ_DRIFT,
};
use mahjong_server::llm::BanterOutput;
use mahjong_server::pacing::PacingMode;
use mahjong_server::persona::{PersonaId, PersonaMood};
use mahjong_server::protocol::{ClientMessage, QuickEmote, ServerMessage, TablePace};
use mahjong_server::session::{BoxedWallProvider, TableOptions, TableRuntime, WallProviderFactory};
use tokio::sync::Notify;

const DEFAULT_MIDHAND_PERSONAS: [PersonaId; 3] = [
    PersonaId::QuickHothead,
    PersonaId::TeaSarcasm,
    PersonaId::ChaosInstigator,
];

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
    let mut cur_seq = start_solo_east_table(
        &runtime,
        &mut east,
        MatchMode::FreeTable,
        DEFAULT_MIDHAND_PERSONAS,
        true,
    )
    .await
    .seq;

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
    let start_snap = start_solo_east_table(
        &runtime,
        &mut east,
        MatchMode::FreeTable,
        DEFAULT_MIDHAND_PERSONAS,
        true,
    )
    .await;

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
            .contains("暗杠一手（牌面保密未知）")
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
    let start_snap = start_solo_east_table(
        &runtime,
        &mut east,
        MatchMode::FreeTable,
        DEFAULT_MIDHAND_PERSONAS,
        true,
    )
    .await;

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
    let mut cur_seq = start_solo_east_table(
        &runtime,
        &mut east,
        MatchMode::FreeTable,
        DEFAULT_MIDHAND_PERSONAS,
        true,
    )
    .await
    .seq;
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
    let snap2 = start_solo_east_table(
        &rt2,
        &mut east2,
        MatchMode::FreeTable,
        DEFAULT_MIDHAND_PERSONAS,
        true,
    )
    .await;

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
    let start_snap = start_solo_east_table(
        &runtime,
        &mut east,
        MatchMode::FreeTable,
        DEFAULT_MIDHAND_PERSONAS,
        true,
    )
    .await;

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

#[tokio::test(start_paused = true)]
async fn slow_think_nudge_drops_if_human_acts_or_enables_ai_assist_before_llm_returns() {
    // Part A: SlowThinkNudge fires (LLM gated) -> human discards (seq advances by >= 1) -> LLM returns -> DROPPED (max_seq_drift == Some(0))!
    // Part B: SlowThinkNudge fires on a fresh turn (LLM gated) -> human enables AI assist (enabled: true) -> LLM returns -> DROPPED!
    let gate_after_action = Arc::new(Notify::new());
    let gate_after_assist = Arc::new(Notify::new());
    let llm = ScriptedLlmClient::from_replies(vec![
        // 1. Turn 1 SlowThinkNudge (gated until East discards) -> MUST DROP!
        PlannedReply::Gated(
            Arc::clone(&gate_after_action),
            Ok(BanterOutput::spoken(
                PersonaMood::Mocking,
                "出完牌后这句催牌绝不该弹出！",
            )),
        ),
        // 2-4. South, West, North Round 1 self-check-ins (silent so all 3 are idle on East's Turn 2)
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        // 5. Turn 2 SlowThinkNudge (gated until East enables AI assist) -> MUST DROP!
        PlannedReply::Gated(
            Arc::clone(&gate_after_assist),
            Ok(BanterOutput::spoken(
                PersonaMood::Mocking,
                "托管接管后这句催牌绝不该弹出！",
            )),
        ),
        // Any subsequent AI self-check-ins stay silent.
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
    ]);

    let east_14 = tiles("1m 4m 7m 1s 4s 7s 1p 4p 7p E S W N C");
    let base_13 = tiles("2m 5m 8m 2s 5s 8s 2p 5p 8p E S W N");
    let draws = tiles("3m 6m 9m 3s 6s 9s 3p 6p");
    let wall = build_preset_wall(&east_14, &base_13, &base_13, &base_13, &draws, &[])
        .expect("nudge staleness wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });

    let options = TableOptions::with_wall_factory(211, factory)
        .with_pacing(PacingMode::Humanized)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let start_snap = start_solo_east_table(
        &runtime,
        &mut east,
        MatchMode::FreeTable,
        DEFAULT_MIDHAND_PERSONAS,
        true,
    )
    .await;

    // Part A: Wait past SLOW_THINK_NUDGE_DELAY so Turn 1 SlowThinkNudge is in-flight (gated).
    tokio::time::advance(SLOW_THINK_NUDGE_DELAY + Duration::from_millis(100)).await;
    yield_tasks().await;
    assert_eq!(
        llm.captured().len(),
        1,
        "Turn 1 SlowThinkNudge should be in-flight"
    );

    // East acts (discards a legal tile) BEFORE the gated SlowThinkNudge LLM returns!
    let legal_discard = start_snap
        .player_view
        .as_ref()
        .expect("player_view")
        .legal_actions
        .iter()
        .find(|a| matches!(a, PlayerAction::Discard { .. }))
        .copied()
        .expect("legal discard on Turn 1");
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: start_snap.seq,
            client_action_id: "east-act-before-nudge".to_string(),
            action: legal_discard,
        },
    )
    .await;
    yield_tasks().await;
    let discard_msgs = east.drain_all();
    assert!(
        discard_msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::EventBatch { .. })),
        "East discard must be accepted before releasing nudge gate: {discard_msgs:?}"
    );

    // Release Turn 1 SlowThinkNudge gate -> MUST be dropped because seq already advanced!
    gate_after_action.notify_waiters();
    yield_tasks().await;
    let after_a = east.drain_all();
    assert!(
        !after_a.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { text, .. } if text.contains("出完牌后这句催牌")
        )),
        "SlowThinkNudge must drop if human already acted (max_seq_drift == Some(0)): {after_a:?}"
    );

    // Advance virtual clock through South/West/North humanized turns until East gets Turn 2.
    let mut turn2_ready = false;
    for _ in 0..40 {
        tokio::time::advance(Duration::from_millis(500)).await;
        pump_ai_turn().await;
        let _ = east.drain_all();
        east.send(&runtime, &ClientMessage::RequestSnapshot).await;
        let snap = east.recv_snapshot().await;
        if snap.seq > start_snap.seq
            && snap
                .player_view
                .as_ref()
                .is_some_and(|v| !v.legal_actions.is_empty())
        {
            turn2_ready = true;
            break;
        }
    }
    assert!(turn2_ready, "expected East to reach Turn 2");
    let _ = east.drain_all();

    // Part B: Wait past SLOW_THINK_NUDGE_DELAY on Turn 2 so SlowThinkNudge #2 is in-flight (gated).
    let calls_before_nudge2 = llm.captured().len();
    tokio::time::advance(SLOW_THINK_NUDGE_DELAY + Duration::from_millis(100)).await;
    yield_tasks().await;
    assert_eq!(
        llm.captured().len(),
        calls_before_nudge2 + 1,
        "Turn 2 SlowThinkNudge should be in-flight"
    );

    // Human enables AI assist (enabled: true) BEFORE LLM returns (and before AI finishes thinking)!
    east.send(
        &runtime,
        &ClientMessage::ToggleAiAssist {
            seat: Seat::East,
            enabled: true,
        },
    )
    .await;
    let _ = east.drain_all();

    // Immediately release Turn 2 SlowThinkNudge gate while AI assist is active (even before AI discards) -> MUST DROP!
    gate_after_assist.notify_waiters();
    yield_tasks().await;
    let after_b = east.drain_all();
    assert!(
        !after_b.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { text, .. } if text.contains("托管接管后这句催牌")
        )),
        "SlowThinkNudge must drop when human enables AI assist: {after_b:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn humanized_final_tile_draw_game_skips_stale_ai_discard_checkin_and_wakes_settlement_cleanly(
) {
    // Construct a deterministic 136-tile wall guaranteed to reach DrawGame on South (AI)'s 69th draw:
    // - All 4 seats start with 1m 5m 9m 1s 5s 9s 1p 5p 9p E S W N (52 tiles; all 4 copies used).
    // - Dead wall (14) + East draws hold ALL 3m/7m/3s/7s/3p/7p so AIs can NEVER form sequences.
    // - Each AI draws at most 2 copies of at most 3 tile kinds (never 3 copies, never 7 pairs) -> ZERO wins!
    let base_13 = tiles("1m 5m 9m 1s 5s 9s 1p 5p 9p E S W N");
    let mut east_14 = base_13.clone();
    east_14.push(tile("C"));

    let tail = tiles("3m 3m 3m 3m 7m 7m 7m 7m 3s 3s 3s 3s 7s 7s");
    let east_draws = tiles("7s 7s 3p 3p 3p 3p 7p 7p 7p 7p 2m 4m 6m 8m 2s 4s 6s");
    let south_draws = tiles("2m 4m 6m 8m 2s 4s 6s 8s 2p 4p 6p 8p C F B 8s 2p 4p");
    let west_draws = tiles("2m 4m 6m 8m 2s 4s 6s 8s 2p 4p 6p 8p C F B 6p 8p");
    let north_draws = tiles("2m 4m 6m 8m 2s 4s 6s 8s 2p 4p 6p 8p C F B F B");

    let mut draws = Vec::with_capacity(69);
    for r in 0..17 {
        draws.push(south_draws[r]);
        draws.push(west_draws[r]);
        draws.push(north_draws[r]);
        draws.push(east_draws[r]);
    }
    draws.push(south_draws[17]);

    let wall = build_preset_wall(&east_14, &base_13, &base_13, &base_13, &draws, &tail)
        .expect("deterministic draw-game 136 wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });

    // Respond silently to all routine check-ins, and speak on HandSettled (小局结算复盘)!
    let llm = ScriptedLlmClient::with_banter_fn(|req| {
        if req.user_prompt.contains("小局结算复盘") {
            Ok(BanterOutput::spoken(
                PersonaMood::Deadpan,
                "牌墙摸穿了，荒庄白忙活！",
            ))
        } else {
            Ok(BanterOutput::silent(PersonaMood::Deadpan))
        }
    });

    let options = TableOptions::with_wall_factory(307, factory)
        .with_pacing(PacingMode::Humanized)
        .with_ai_budget(DecisionBudget::Deterministic { max_samples: 4 })
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let _ = start_solo_east_table(
        &runtime,
        &mut east,
        MatchMode::FreeTable,
        DEFAULT_MIDHAND_PERSONAS,
        false,
    )
    .await;
    east.send(
        &runtime,
        &ClientMessage::SetPace {
            pace: TablePace::Fast,
        },
    )
    .await;
    let _ = east.drain_all();

    // Play the hand in PacingMode::Humanized: East always discards its drawn tile until wall_remaining == 1,
    // then step in 50ms increments (< 120ms Fast discard_release_delay) to observe the exact split
    // between final AI TileDiscarded and deferred HandSettled(DrawGame).
    let mut action_idx = 0usize;
    let mut saw_draw_game = false;
    let mut saw_settlement_banter = false;
    let mut final_discard_seat = None;
    let mut calls_before_final_discard = 0usize;
    let mut calls_right_after_final_discard = None;

    for _ in 0..400 {
        east.send(&runtime, &ClientMessage::RequestSnapshot).await;
        let snap = east.recv_snapshot().await;
        let Some(view) = snap.player_view.as_ref() else {
            break;
        };
        if let Some(discard) = view
            .legal_actions
            .iter()
            .find(|a| {
                matches!(
                    a,
                    PlayerAction::Discard {
                        from_drawn: true,
                        ..
                    }
                )
            })
            .or_else(|| {
                view.legal_actions
                    .iter()
                    .find(|a| matches!(a, PlayerAction::Discard { .. }))
            })
            .copied()
        {
            action_idx += 1;
            east.send(
                &runtime,
                &ClientMessage::SubmitAction {
                    expected_seq: snap.seq,
                    client_action_id: format!("dg-{action_idx}"),
                    action: discard,
                },
            )
            .await;
        } else if view.legal_actions.contains(&PlayerAction::Pass) {
            action_idx += 1;
            east.send(
                &runtime,
                &ClientMessage::SubmitAction {
                    expected_seq: snap.seq,
                    client_action_id: format!("dg-pass-{action_idx}"),
                    action: PlayerAction::Pass,
                },
            )
            .await;
        }

        if view.wall_remaining == 0 && final_discard_seat.is_none() {
            calls_before_final_discard = llm.captured().len();
        }

        let step = if view.wall_remaining <= 1 {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(600)
        };
        tokio::time::advance(step).await;
        yield_tasks().await;
        std::thread::sleep(Duration::from_millis(2));
        yield_tasks().await;

        for msg in east.drain_all() {
            match msg {
                ServerMessage::EventBatch { events, .. } => {
                    for ev in events {
                        match ev.kind {
                            PlayerEventKind::TileDiscarded { seat, .. }
                                if seat != Seat::East && view.wall_remaining == 0 =>
                            {
                                final_discard_seat = Some(seat);
                                calls_right_after_final_discard = Some(llm.captured().len());
                            }
                            PlayerEventKind::HandSettled {
                                settlement:
                                    HandSettlement {
                                        outcome: HandOutcome::DrawGame,
                                        ..
                                    },
                            } => {
                                saw_draw_game = true;
                            }
                            _ => {}
                        }
                    }
                }
                ServerMessage::TableBanter { text, .. } if text.contains("荒庄白忙活") => {
                    saw_settlement_banter = true;
                }
                _ => {}
            }
        }

        if saw_draw_game {
            yield_tasks().await;
            for msg in east.drain_all() {
                if let ServerMessage::TableBanter { text, .. } = msg {
                    if text.contains("荒庄白忙活") {
                        saw_settlement_banter = true;
                    }
                }
            }
            break;
        }
    }

    assert!(saw_draw_game, "expected hand to settle as DrawGame");
    assert!(
        saw_settlement_banter,
        "HandSettled(DrawGame) must wake an idle AI cleanly without being blocked"
    );
    let final_discard_seat =
        final_discard_seat.expect("expected final AI TileDiscarded at wall_remaining == 0");
    assert_eq!(
        calls_right_after_final_discard,
        Some(calls_before_final_discard),
        "final-tile DrawGame discard by {final_discard_seat:?} must NOT schedule a stale AiDiscard self-check-in"
    );
    let captured = llm.captured();
    let last_req = captured.last().expect("at least one LLM call");
    assert!(
        last_req.user_prompt.contains("小局结算复盘") && last_req.user_prompt.contains("荒庄"),
        "final LLM call must be HandSettled(DrawGame) recap, not a stale AiDiscard check-in:\n{}",
        last_req.user_prompt
    );
    // Verify the final-discarding AI NEVER got a stale AiDiscard check-in on that final wall_remaining == 0 discard.
    assert!(
        !captured.iter().any(|req| {
            req.seat == Some(final_discard_seat)
                && req.user_prompt.contains("自家出牌巡检")
                && req.user_prompt.contains("牌墙进度：剩余 0 张")
        }),
        "no AiDiscard self-check-in may fire for {final_discard_seat:?} on the final wall_remaining == 0 DrawGame discard"
    );
}

#[tokio::test(start_paused = true)]
async fn delayed_followup_reselects_currently_idle_ai_and_sees_quick_emote_arriving_during_1s_delay(
) {
    // East holds 4x 发财 so East can declare ConcealedKong(F) at t=0 (waking 1 opener AI),
    // while keeping East's turn open to act & send QuickEmote DURING the 1s follow-up delay!
    let east_14 = tiles("F F F F 1p 4p 7m 8m 9m 2s 5s 8s 3p 6p");
    let south_13 = tiles("123m 456m 789p 23s 55p");
    let west_13 = tiles("2m 5m 8m 3s 6s 9s 2p 5p 8p E S W B");
    let north_13 = tiles("3m 6m 9m 4s 7s 1p 3p 6p 9p E S N B");
    let draws = tiles("8p N B 2m");
    let tail_draws = tiles("9s");
    let wall = build_preset_wall(
        &east_14,
        &south_13,
        &west_13,
        &north_13,
        &draws,
        &tail_draws,
    )
    .expect("deferred followup wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });

    // Gate for occupying one of the remaining AIs during the 1s delay via QuickEmote reply!
    let busy_gate = Arc::new(Notify::new());
    let llm = ScriptedLlmClient::from_replies(vec![
        // 1. ConcealedKong(F) opener at t=0 -> speaks immediately, scheduling 1s delayed follow-up.
        PlannedReply::Immediate(Ok(BanterOutput::spoken(
            PersonaMood::Nervous,
            "开局暗杠这么猛！",
        ))),
        // 2. QuickEmote::GoingBig arriving at t=+300ms (DURING the 1s delay!) -> GATED so that AI is BUSY at t=1s!
        PlannedReply::Gated(
            Arc::clone(&busy_gate),
            Ok(BanterOutput::silent(PersonaMood::Deadpan)),
        ),
        // 3. Delayed follow-up firing at t=+1050ms -> MUST pick a DIFFERENT currently-idle AI and see QuickEmote in Block 4!
        PlannedReply::Immediate(Ok(BanterOutput::spoken(
            PersonaMood::Mocking,
            "我还闲着呢，顺嘴接一句！",
        ))),
        // Round-2 follow-up stays silent.
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
    ]);

    let options = TableOptions::with_wall_factory(409, factory)
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let start_snap = start_solo_east_table(
        &runtime,
        &mut east,
        MatchMode::FreeTable,
        DEFAULT_MIDHAND_PERSONAS,
        false,
    )
    .await;

    // 1. At t=0: East declares ConcealedKong(F) -> Opener AI speaks immediately.
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: start_snap.seq,
            client_action_id: "ckong-opener".to_string(),
            action: PlayerAction::ConcealedKong { tile: tile("F") },
        },
    )
    .await;
    yield_tasks().await;
    let opener_seat = east
        .drain_all()
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::TableBanter { seat, .. } => Some(seat),
            _ => None,
        })
        .expect("expected ConcealedKong opener banter");
    assert_eq!(llm.captured().len(), 1);

    // 2. At t=+300ms (DURING the 1s follow-up delay!): East sends QuickEmote::GoingBig.
    //    This BOTH appends GoingBig into TableTranscript AND occupies 1 AI seat (gated busy) across t=1s!
    tokio::time::advance(Duration::from_millis(300)).await;
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::GoingBig,
        },
    )
    .await;
    yield_tasks().await;
    let _ = east.drain_all();
    assert_eq!(llm.captured().len(), 2);
    let busy_seat = llm.captured()[1]
        .seat
        .expect("BanterGenerationRequest carries speaker seat");
    assert_ne!(
        busy_seat, opener_seat,
        "test setup expects QuickEmote during 1s delay to occupy a non-opener follow-up candidate"
    );

    // 3. Advance past the 1s follow-up delay (total t = 1100ms) while `busy_seat` is STILL gated in-flight!
    tokio::time::advance(BANTER_CHAIN_REPLY_DELAY - Duration::from_millis(200)).await;
    yield_tasks().await;
    let followup_msgs = east.drain_all();
    let followup_seat = followup_msgs
        .iter()
        .find_map(|m| match m {
            ServerMessage::TableBanter {
                seat,
                reply_to_seat: Some(reply_to),
                text,
                ..
            } if *reply_to == opener_seat && text.contains("顺嘴接一句") => Some(*seat),
            _ => None,
        })
        .expect("deferred follow-up must select an idle AI instead of aborting when another candidate is busy");

    assert_ne!(followup_seat, opener_seat);
    assert_ne!(
        followup_seat, busy_seat,
        "follow-up must skip the AI that became busy during the 1s delay"
    );

    // Verify the follow-up AI's freshly built prompt contains the QuickEmote that arrived DURING the 1s delay!
    let captured = llm.captured();
    assert_eq!(captured.len(), 3);
    assert_eq!(captured[2].seat, Some(followup_seat));
    assert!(
        captured[2].user_prompt.contains("【4. 跨小局牌桌对话流")
            && captured[2]
                .user_prompt
                .contains(QuickEmote::GoingBig.label()),
        "follow-up prompt built AFTER 1s delay must contain QuickEmote arriving during delay:\n{}",
        captured[2].user_prompt
    );

    busy_gate.notify_waiters();
    yield_tasks().await;
}
