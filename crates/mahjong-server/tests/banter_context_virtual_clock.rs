mod common;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use common::{
    no_reaction_wall, pump_ai_turn, start_solo_east_table, tile, tiles, yield_tasks, PlannedReply,
    ScriptedLlmClient, TempDirGuard, VirtualClient,
};
use mahjong_core::{build_preset_wall, MatchMode, PlayerAction, PresetWallProvider, Seat};
use mahjong_server::banter::{build_system_prompt, BANTER_CHAIN_REPLY_DELAY};
use mahjong_server::llm::BanterOutput;
use mahjong_server::pacing::PacingMode;
use mahjong_server::persona::{PersonaId, PersonaMood};
use mahjong_server::persona_memory::{PersonaMemoryNoteTrigger, PersonaMemoryStore};
use mahjong_server::protocol::{ClientMessage, QuickEmote, ServerMessage, TablePhase};
use mahjong_server::session::{BoxedWallProvider, TableOptions, TableRuntime, WallProviderFactory};

const DEFAULT_CONTEXT_PERSONAS: [PersonaId; 3] = [
    PersonaId::QuickHothead,
    PersonaId::TeaSarcasm,
    PersonaId::TiltedMaster,
];

#[tokio::test(start_paused = true)]
async fn six_block_context_persists_across_hands_with_chronicle_river_trajectory_transcript_memory_and_time(
) {
    let temp_dir = TempDirGuard::new("majjj_six_block_ctx");
    let data_dir = temp_dir.path().to_path_buf();
    {
        let mut store = PersonaMemoryStore::open(&data_dir).expect("open persona memory");
        store.record_grudge_note(
            PersonaId::QuickHothead,
            "小王",
            "小王上回清一色把大川赢惨了",
            PersonaMemoryNoteTrigger::HighFan,
            1000,
        );
        store.record_grudge_note(
            PersonaId::QuickHothead,
            "老李",
            "老李未入座秘密旧账绝不能外传",
            PersonaMemoryNoteTrigger::MatchFinished,
            2000,
        );
    }

    // Hand 1: East instant high-fan Tsumo (清一色 + 碰碰胡 + 门前清 + 自摸).
    let east_h1 = tiles("111m 222m 333m 444m 55m");
    let other_h1 = tiles("68m 147s 147p E S W N B");
    let wall1 = build_preset_wall(&east_h1, &other_h1, &other_h1, &other_h1, &[], &[])
        .expect("hand 1 tsumo wall");

    // Hand 2: 2 clean rounds with no claims; East holds secret 红中 x3, West holds secret 发财 x2.
    let east_h2 = tiles("1p 4p 7m 8m 9m 6s 7s 8s 2p 3p 6p C C C");
    let south_h2 = tiles("123m 456m 78p 23s 55p 9s");
    let west_h2 = tiles("2m 5m 8m 5s 8s 3p 6p 9p E S W F F");
    let north_h2 = tiles("3m 6m 9m 4s 7s 1p 4p 7p E S W N B");
    let draws_h2 = tiles("9p B N 2m 5p N B 3m 6p B N 4m");
    let wall2 = build_preset_wall(&east_h2, &south_h2, &west_h2, &north_h2, &draws_h2, &[])
        .expect("hand 2 multi-round wall");

    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(
            Seat::East,
            vec![wall1.clone(), wall2.clone()],
        )))
    });

    let llm = ScriptedLlmClient::from_replies(vec![
        // Hand 1 QuickEmote reply -> speaks, follow-up stays silent.
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Mocking,
            text: "先别吹，看你这手有多大！".to_string(),
        })),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        // Hand 1 Settlement opener -> speaks, follow-up stays silent.
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Tilted,
            text: "清一色起手自摸，离谱！".to_string(),
        })),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        // Hand 2 Round 1: South (大川) speaks, follow-up silent, West silent, North silent.
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Plotting,
            text: "这手听口挺顺的嘛".to_string(),
        })),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Nervous))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        // Hand 2 Round 2: South (大川), West, North all return silent.
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Smug))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
    ]);

    let options = TableOptions::with_wall_factory(77, factory)
        .with_pacing(PacingMode::Immediate)
        .with_data_dir(data_dir)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let start_snap = start_solo_east_table(
        &runtime,
        &mut east,
        MatchMode::FreeTable,
        DEFAULT_CONTEXT_PERSONAS,
        false,
    )
    .await;

    // 1. Hand 1 QuickEmote -> recorded into cross-hand TableTranscript.
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::GoingBig,
        },
    )
    .await;
    yield_tasks().await;
    tokio::time::advance(BANTER_CHAIN_REPLY_DELAY + Duration::from_millis(100)).await;
    yield_tasks().await;
    let _ = east.drain_all();

    // 2. Hand 1 East Tsumo -> settles Hand 1, records MatchChronicle + settlement banter in TableTranscript.
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
    for msg in east.drain_all() {
        if let ServerMessage::EventBatch { to_seq, .. } = msg {
            settled_seq = to_seq;
        }
    }
    tokio::time::advance(BANTER_CHAIN_REPLY_DELAY + Duration::from_millis(100)).await;
    yield_tasks().await;
    let _ = east.drain_all();

    // 3. Confirm Hand 2 -> Chronicle & TableTranscript persist; trajectories reset for Hand 2.
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
    let mut cur_seq = settled_seq;
    for msg in east.drain_all() {
        if let ServerMessage::EventBatch { to_seq, .. } = msg {
            cur_seq = to_seq;
        }
    }

    // 4. Hand 2 Round 1 & Round 2 discards -> South (大川) speaks on R1, then checks in again on R2!
    for (idx, discard_tile) in [tile("1p"), tile("4p")].into_iter().enumerate() {
        east.send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: cur_seq,
                client_action_id: format!("h2-d-{idx}"),
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
        tokio::time::advance(BANTER_CHAIN_REPLY_DELAY + Duration::from_millis(100)).await;
        yield_tasks().await;
        let _ = east.drain_all();
    }

    // Find South (大川)'s Round 2 self-check-in request.
    let captured = llm.captured();
    let south_checkins: Vec<_> = captured
        .iter()
        .filter(|req| {
            req.system_prompt.contains("大川") && req.user_prompt.contains("自家出牌巡检")
        })
        .collect();
    assert!(
        south_checkins.len() >= 2,
        "expected >= 2 South self-check-ins across 2 rounds, got {}",
        south_checkins.len()
    );
    let r2_req = south_checkins[1];

    // System prompt contract assertions.
    assert!(r2_req.system_prompt.contains("大川"));
    assert!(r2_req.system_prompt.contains("2 到 18 字"));
    assert!(r2_req.system_prompt.contains("4 到 14 字"));
    assert!(r2_req.system_prompt.contains("speak: false"));
    assert!(r2_req.system_prompt.contains("不要在句首硬加对方姓名"));
    assert!(r2_req.system_prompt.contains("15% 到 25%"));
    assert!(!r2_req.system_prompt.contains("10 到 26 字"));
    assert!(!r2_req.system_prompt.contains("发牌员"));

    let up = &r2_req.user_prompt;
    // Block 1: Match chronicle persisted from Hand 1.
    assert!(
        up.contains("【1. 本场对局战况编年史】"),
        "missing Block 1:\n{up}"
    );
    assert!(up.contains("自由桌"), "missing mode:\n{up}");
    assert!(
        up.contains("本场已结 1 小局"),
        "missing settled count:\n{up}"
    );
    assert!(up.contains("第1名 上家 小王"), "missing ranking:\n{up}");
    assert!(
        up.contains("自摸") && up.contains("清一色"),
        "missing Hand 1 fan summary:\n{up}"
    );
    assert!(up.contains("庄家连庄"), "missing dealer streak note:\n{up}");

    // Block 2: Public table dynamics & river portrait.
    assert!(
        up.contains("【2. 本小局公开动态与牌河画像】"),
        "missing Block 2:\n{up}"
    );
    assert!(
        up.contains("牌墙进度：剩余"),
        "missing wall progress:\n{up}"
    );
    assert!(
        up.contains("各家牌河与节奏"),
        "missing river portrait:\n{up}"
    );
    assert!(
        up.contains("摸切") || up.contains("手切"),
        "missing discard cadence in Block 2:\n{up}"
    );

    // Block 3: Own hand, precomputed facts, opening hand assessment, trajectory & mood.
    assert!(
        up.contains("【3. 自家手牌、预计算手牌事实与牌运轨迹】"),
        "missing Block 3:\n{up}"
    );
    assert!(up.contains("自家手牌："), "missing own hand:\n{up}");
    assert!(
        up.contains("预计算手牌事实："),
        "missing precomputed facts:\n{up}"
    );
    assert!(
        up.contains("本小局起手评估："),
        "missing opening assessment:\n{up}"
    );
    assert!(
        up.contains("近几巡牌运轨迹：") && up.contains("第1巡") && up.contains("第2巡"),
        "missing multi-turn trajectory:\n{up}"
    );
    assert!(
        up.contains("自家当前牌友情绪：阴险憋大招（Plotting）"),
        "missing current PersonaMood in Block 3:\n{up}"
    );

    // Block 4: Cross-hand TableTranscript containing Hand 1 QuickEmote + Hand 1 banter + Hand 2 banter
    // + exact turns-since-last-spoke!
    assert!(
        up.contains("【4. 跨小局牌桌对话流"),
        "missing Block 4:\n{up}"
    );
    assert!(
        up.contains(QuickEmote::GoingBig.label()),
        "Hand 1 QuickEmote must persist into Hand 2 transcript:\n{up}"
    );
    assert!(
        up.contains("清一色起手自摸，离谱！"),
        "Hand 1 settlement banter must persist into Hand 2 transcript:\n{up}"
    );
    assert!(
        up.contains("这手听口挺顺的嘛"),
        "Hand 2 Round 1 banter must appear in Round 2 transcript:\n{up}"
    );
    assert!(
        up.contains("你距离上次开口已过 1 巡"),
        "turns-since-last-spoke must equal 1 turn after speaking in Round 1:\n{up}"
    );
    assert!(
        !up.contains("下家 小晚（心虚紧绷）"),
        "silent speak:false mood update must never be recorded into TableTranscript:\n{up}"
    );

    // Block 5: Scoped cross-match PersonaMemory (小王 included, unseated 老李 excluded).
    assert!(up.contains("【5. 跨对局牌友记忆"), "missing Block 5:\n{up}");
    assert!(
        up.contains("小王上回清一色把大川赢惨了"),
        "missing seated human's grudge note:\n{up}"
    );
    assert!(
        !up.contains("老李") && !up.contains("未入座秘密旧账"),
        "unseated player leaked into Block 5:\n{up}"
    );

    // Block 6: Host local weekday & time-of-day context.
    assert!(up.contains("【6. 现实时间语境】"), "missing Block 6:\n{up}");
    assert!(
        up.contains("当前现实时间：周"),
        "missing weekday/time:\n{up}"
    );

    // Natural separated direction + name (NO mechanical bracket labels) & NO "发牌员" bias.
    assert!(
        !up.contains("小王（上家）"),
        "mechanical bracket label found:\n{up}"
    );
    assert!(
        !up.contains("大川（自家）"),
        "mechanical bracket label found:\n{up}"
    );
    assert!(!up.contains("（自己）"), "banned self label found:\n{up}");
    assert!(!up.contains("发牌员"), "dealer-rigged bias found:\n{up}");

    // Strict privacy redline: opponent hidden tiles MUST NOT leak!
    assert!(
        !up.contains("红中") && !up.contains("发财"),
        "opponent concealed tiles leaked into mid-hand prompt:\n{up}"
    );
}

#[tokio::test(start_paused = true)]
async fn privacy_redline_never_leaks_opponent_concealed_hand_or_concealed_kong_across_midhand_prompts(
) {
    // East holds 4x 发财 (will declare ConcealedKong!) + 3x 红中 (stays concealed in closed hand).
    // South holds 123m 456m 789p 23s 55p (no 红中, no 发财).
    let east_14 = tiles("F F F F C C C 1p 4p 7m 8m 9m 2s 5s");
    let south_13 = tiles("123m 456m 789p 23s 55p");
    let west_13 = tiles("2m 5m 8m 3s 6s 9s 2p 5p 8p E S W B");
    let north_13 = tiles("3m 6m 9m 4s 7s 1p 3p 6p 9p E S N B");
    // Replacement draw for East after ConcealedKong is 9s (wall tail); normal draws for South/West/North/East.
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
    .expect("concealed kong privacy wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });

    let llm = ScriptedLlmClient::from_replies(vec![
        // 1. Reaction to East's ConcealedKong -> speaks (triggers 1s follow-up!).
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Nervous,
            text: "开局就暗杠，够狠的".to_string(),
        })),
        // 2. Follow-up on ConcealedKong -> silent.
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        // 3-5. South, West, North discard self-check-ins (while East's ConcealedKong is on the table in Block 2!).
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
    ]);

    let options = TableOptions::with_wall_factory(91, factory)
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let start_snap = start_solo_east_table(
        &runtime,
        &mut east,
        MatchMode::FreeTable,
        DEFAULT_CONTEXT_PERSONAS,
        false,
    )
    .await;

    // 1. East declares ConcealedKong(F).
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
    let mut cur_seq = start_snap.seq;
    for msg in east.drain_all() {
        if let ServerMessage::EventBatch { to_seq, .. } = msg {
            cur_seq = to_seq;
        }
    }
    tokio::time::advance(BANTER_CHAIN_REPLY_DELAY + Duration::from_millis(100)).await;
    yield_tasks().await;
    let _ = east.drain_all();

    // 2. East discards 1p -> South, West, North draw & discard (all 3 build Block 2 public melds portrait!).
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: cur_seq,
            client_action_id: "east-d-1p".to_string(),
            action: PlayerAction::Discard {
                tile: tile("1p"),
                from_drawn: false,
            },
        },
    )
    .await;
    pump_ai_turn().await;
    let _ = east.drain_all();

    let captured = llm.captured();
    assert!(
        captured.len() >= 5,
        "expected ConcealedKong reaction + follow-up + 3 AI check-ins (>= 5), got {}",
        captured.len()
    );

    // EVERY mid-hand prompt must mask East's ConcealedKong (发财) and hide East's concealed 红中!
    for (idx, req) in captured.iter().enumerate() {
        assert!(
            !req.user_prompt.contains("发财"),
            "prompt #{idx} leaked opponent ConcealedKong tile 发财:\n{}",
            req.user_prompt
        );
        assert!(
            !req.user_prompt.contains("红中"),
            "prompt #{idx} leaked opponent concealed tile 红中:\n{}",
            req.user_prompt
        );
        assert!(
            req.user_prompt.contains("暗杠一手（牌面保密未知）"),
            "prompt #{idx} missing masked ConcealedKong notice:\n{}",
            req.user_prompt
        );
    }

    // Speaker's own concealed tiles DO appear in South's check-in prompt.
    let south_checkin = captured
        .iter()
        .find(|req| req.system_prompt.contains("大川") && req.user_prompt.contains("自家出牌巡检"))
        .expect("South self-check-in present");
    assert!(
        south_checkin.user_prompt.contains("一万") && south_checkin.user_prompt.contains("五筒"),
        "speaker's own hand must be visible in Block 3:\n{}",
        south_checkin.user_prompt
    );
}

#[tokio::test(start_paused = true)]
async fn chronicle_and_table_transcript_reset_cleanly_on_disband_and_new_match() {
    let east_win = tiles("111m 222m 333m 444m CC");
    let other_13 = tiles("58m 147s 147p E S W N B");
    let wall_win =
        build_preset_wall(&east_win, &other_13, &other_13, &other_13, &[], &[]).expect("win wall");
    let wall_plain = no_reaction_wall();

    let match_num = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let factory: WallProviderFactory = Arc::new(move || {
        let idx = match_num.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let wall = if idx == 0 {
            wall_win.clone()
        } else {
            wall_plain.clone()
        };
        BoxedWallProvider(Box::new(PresetWallProvider::single(Seat::East, wall)))
    });

    let llm = ScriptedLlmClient::from_replies(vec![
        // Match 1 Hand 1 settlement opener speaks a unique marker line; follow-up silent.
        PlannedReply::Immediate(Ok(BanterOutput {
            speak: true,
            mood: PersonaMood::Tilted,
            text: "第一场专属暗语绝不能带进下一场".to_string(),
        })),
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        // Match 2 QuickEmote reply.
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
    ]);

    let options = TableOptions::with_wall_factory(123, factory)
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let m1_snap = start_solo_east_table(
        &runtime,
        &mut east,
        MatchMode::FreeTable,
        DEFAULT_CONTEXT_PERSONAS,
        false,
    )
    .await;

    // Match 1: East instant Tsumo -> settlement banter enters Match 1 transcript & chronicle.
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: m1_snap.seq,
            client_action_id: "m1-win".to_string(),
            action: PlayerAction::Tsumo,
        },
    )
    .await;
    yield_tasks().await;
    tokio::time::advance(BANTER_CHAIN_REPLY_DELAY + Duration::from_millis(100)).await;
    yield_tasks().await;
    let _ = east.drain_all();

    // Disband Match 1 back to Lobby.
    east.send(&runtime, &ClientMessage::RequestDisbandMatch)
        .await;
    let lobby_snap = east.recv_snapshot().await;
    assert_eq!(lobby_snap.phase, TablePhase::Lobby);
    let _ = east.drain_all();

    // Start Match 2 fresh and trigger a QuickEmote reply.
    llm.clear_captured();
    east.send(&runtime, &ClientMessage::StartMatch).await;
    let m2_snap = east.recv_snapshot().await;
    assert_eq!(m2_snap.phase, TablePhase::InMatch);
    let _ = east.drain_all();

    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
    yield_tasks().await;
    let _ = east.drain_all();

    let m2_reqs = llm.captured();
    assert_eq!(m2_reqs.len(), 1, "expected 1 QuickEmote reply in Match 2");
    let up = &m2_reqs[0].user_prompt;
    assert!(
        up.contains("本场已结 0 小局") && up.contains("尚无已结算战绩"),
        "Match 2 chronicle must start clean:\n{up}"
    );
    assert!(
        up.contains("你本场对局还没开过口"),
        "Match 2 last-spoken tracker must start clean:\n{up}"
    );
    assert!(
        !up.contains("第一场专属暗语"),
        "Match 1 transcript leaked into Match 2:\n{up}"
    );
}

#[test]
fn all_five_humanized_personas_have_distinct_rich_bios_and_new_prompt_contract() {
    let mut prompts = HashSet::new();
    for pid in PersonaId::ALL {
        let prompt = build_system_prompt(pid);
        assert!(
            prompt.contains(pid.display_name()),
            "missing persona name {}",
            pid.display_name()
        );
        assert!(prompt.contains("2 到 18 字"));
        assert!(prompt.contains("4 到 14 字"));
        assert!(!prompt.contains("10 到 26 字"));
        assert!(!prompt.contains("发牌员"));
        assert!(prompt.contains("speak: false"));
        assert!(prompt.contains("不要在句首硬加对方姓名"));
        assert!(prompt.contains("15% 到 25%"));
        assert!(
            prompts.insert(prompt),
            "duplicate system prompt for {pid:?}"
        );
    }
    assert_eq!(prompts.len(), 5);
}

#[tokio::test(start_paused = true)]
async fn off_turn_wakeups_preserve_hand_progression_and_never_inflate_stalled_streak() {
    // 3 humans (East, West, North) + 1 AI (South = 大川) so every off-turn QuickEmote deterministically wakes South!
    // South starts 1-shanten: 123m 456m 78p 23s 5p 8p 9s (needs 9p or 5p; on Turn 1 draws 5p -> enters tenpai!).
    // On Turn 2 South draws junk E -> discards E (1 stalled step: 本手原地踏步, stalled_streak == 1).
    let east_14 = tiles("1p 4p 7m 8m 9m 6s 7s 8s 2p 3p 6p C C C");
    let south_13 = tiles("123m 456m 789p 23s 5p 9s");
    let west_13 = tiles("2m 5m 8m 5s 8s 3p 6p 9p E S W F F");
    let north_13 = tiles("3m 6m 9m 4s 7s 1p 4p 7p E S W N B");
    // Draw order: South R1 draws 5p (enters tenpai! discards 9s), West draws N, North draws B, East draws 2m;
    // South R2 draws E (no improvement -> discards E, stalled_streak == 1), West draws N, North draws B, East draws 3m.
    let draws = tiles("5p N B 2m E N B 3m");
    let wall = build_preset_wall(&east_14, &south_13, &west_13, &north_13, &draws, &[])
        .expect("off-turn progression wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });

    let llm = ScriptedLlmClient::from_replies(vec![
        // 1. South R1 discard self-check-in (EnteredTenpai)
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Smug))),
        // 2. Off-turn QuickEmote #1 while South is freshly EnteredTenpai
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Smug))),
        // 3. South R2 discard self-check-in (Stalled, stalled_streak = 1)
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
        // 4. Off-turn QuickEmote #2 while South has stalled_streak = 1
        PlannedReply::Immediate(Ok(BanterOutput::silent(PersonaMood::Deadpan))),
    ]);

    let options = TableOptions::with_wall_factory(131, factory)
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let mut west = VirtualClient::connect(&runtime, "w1", "阿西").await;
    let mut north = VirtualClient::connect(&runtime, "n1", "阿北").await;
    for (client, seat) in [
        (&mut east, Seat::East),
        (&mut west, Seat::West),
        (&mut north, Seat::North),
    ] {
        client
            .send(&runtime, &ClientMessage::JoinSeat { seat: Some(seat) })
            .await;
        let _ = client.recv_snapshot().await;
    }
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
        &ClientMessage::SetSlowThinkNudge { enabled: false },
    )
    .await;
    east.send(&runtime, &ClientMessage::StartMatch).await;
    let mut cur_seq = east.recv_snapshot().await.seq;
    let _ = west.recv_snapshot().await;
    let _ = north.recv_snapshot().await;
    let _ = east.drain_all();

    // Round 1: East discards 1p -> South draws 5p and discards 9s, entering tenpai!
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: cur_seq,
            client_action_id: "r1-east".to_string(),
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

    // Off-turn QuickEmote immediately after South entered tenpai!
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::GoingBig,
        },
    )
    .await;
    yield_tasks().await;
    let _ = east.drain_all();

    let after_r1 = llm.captured();
    assert_eq!(after_r1.len(), 2, "expected R1 check-in + off-turn emote");
    assert!(
        after_r1[0].user_prompt.contains("本手进展：刚刚听牌"),
        "South R1 check-in should report EnteredTenpai:\n{}",
        after_r1[0].user_prompt
    );
    assert!(
        after_r1[1].user_prompt.contains("本手进展：刚刚听牌"),
        "off-turn QuickEmote MUST preserve South's EnteredTenpai progression without re-comparing against itself:\n{}",
        after_r1[1].user_prompt
    );
    assert!(
        !after_r1[1].user_prompt.contains("原地踏步")
            && !after_r1[1].user_prompt.contains("无效进张卡手"),
        "off-turn QuickEmote must NOT corrupt progression to Stalled:\n{}",
        after_r1[1].user_prompt
    );

    // Finish Round 1 (West & North discard) and play Round 2 so South takes a stalled step (stalled_streak == 1).
    for (client, cid, d_tile) in [
        (&west, "r1-west", tile("N")),
        (&north, "r1-north", tile("B")),
    ] {
        client
            .send(
                &runtime,
                &ClientMessage::SubmitAction {
                    expected_seq: cur_seq,
                    client_action_id: cid.to_string(),
                    action: PlayerAction::Discard {
                        tile: d_tile,
                        from_drawn: true,
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
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: cur_seq,
            client_action_id: "r2-east".to_string(),
            action: PlayerAction::Discard {
                tile: tile("2m"),
                from_drawn: true,
            },
        },
    )
    .await;
    pump_ai_turn().await;
    let _ = east.drain_all();

    // Advance past quick-emote cooldown and send a second off-turn QuickEmote!
    tokio::time::advance(Duration::from_secs(5)).await;
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
    yield_tasks().await;
    let _ = east.drain_all();

    let after_r2 = llm.captured();
    assert_eq!(
        after_r2.len(),
        4,
        "expected R2 check-in + 2nd off-turn emote"
    );
    assert!(
        after_r2[2].user_prompt.contains("本手进展：本手原地踏步"),
        "South R2 check-in should report single-step stall:\n{}",
        after_r2[2].user_prompt
    );
    assert!(
        after_r2[3].user_prompt.contains("本手进展：本手原地踏步")
            && !after_r2[3].user_prompt.contains("已连续2手"),
        "off-turn QuickEmote must NOT inflate stalled_streak from 1 to 2:\n{}",
        after_r2[3].user_prompt
    );
}
