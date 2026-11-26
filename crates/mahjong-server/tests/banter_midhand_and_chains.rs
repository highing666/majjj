mod common;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{tile, tiles};
use mahjong_ai::AiDifficulty;
use mahjong_core::{
    build_preset_wall, MatchMode, PlayerAction, PlayerEventKind, PresetWallProvider, Seat,
};
use mahjong_server::llm::{
    BanterGenerationRequest, BanterOutput, LlmClient, LlmError, LlmFuture, LlmHealthStatus,
};
use mahjong_server::pacing::PacingMode;
use mahjong_server::persona::{PersonaId, PersonaMood};
use mahjong_server::protocol::{
    ClientMessage, SeatingRule, ServerMessage, TablePace, TablePhase, TableSnapshot,
};
use mahjong_server::session::{
    BoxedWallProvider, OutboundSignal, TableOptions, TableRuntime, WallProviderFactory,
};
use tokio::sync::mpsc;

#[derive(Debug)]
struct ScriptedLlmClient {
    replies: Mutex<VecDeque<BanterOutput>>,
    captured_requests: Mutex<Vec<BanterGenerationRequest>>,
}

impl ScriptedLlmClient {
    fn new(replies: Vec<BanterOutput>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into()),
            captured_requests: Mutex::new(Vec::new()),
        })
    }

    fn captured(&self) -> Vec<BanterGenerationRequest> {
        self.captured_requests
            .lock()
            .expect("lock captured")
            .clone()
    }
}

impl LlmClient for ScriptedLlmClient {
    fn endpoint(&self) -> &str {
        "http://virtual-llm.local:8080"
    }

    fn health_check(&self) -> LlmFuture<'_, Result<LlmHealthStatus, LlmError>> {
        Box::pin(async {
            Ok(LlmHealthStatus {
                endpoint: "http://virtual-llm.local:8080".to_string(),
                model_id: "virtual-qwen".to_string(),
            })
        })
    }

    fn generate_banter(
        &self,
        request: BanterGenerationRequest,
    ) -> LlmFuture<'_, Result<BanterOutput, LlmError>> {
        self.captured_requests
            .lock()
            .expect("lock captured")
            .push(request);
        let next = self
            .replies
            .lock()
            .expect("lock replies")
            .pop_front()
            .unwrap_or_else(|| BanterOutput {
                mood: PersonaMood::Deadpan,
                text: "默认补刀台词".to_string(),
            });
        Box::pin(async move { Ok(next) })
    }
}

struct VirtualClient {
    conn_id: u64,
    tx: mpsc::UnboundedSender<OutboundSignal>,
    rx: mpsc::UnboundedReceiver<OutboundSignal>,
}

impl VirtualClient {
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

    fn drain_all(&mut self) -> Vec<ServerMessage> {
        let mut out = Vec::new();
        while let Ok(sig) = self.rx.try_recv() {
            match sig {
                OutboundSignal::Message(m) => out.push(*m),
                OutboundSignal::ReplaceAndClose => out.push(ServerMessage::SessionReplaced),
            }
        }
        out
    }

    async fn recv_snapshot(&mut self) -> TableSnapshot {
        loop {
            match self.rx.recv().await.expect("outbound msg") {
                OutboundSignal::Message(msg) => match *msg {
                    ServerMessage::TableSnapshot(snap) => return snap,
                    ServerMessage::LobbyUpdated { .. } | ServerMessage::TableBanter { .. } => {
                        continue
                    }
                    other => panic!("expected TableSnapshot, got {other:?}"),
                },
                OutboundSignal::ReplaceAndClose => panic!("unexpected SessionReplaced"),
            }
        }
    }
}

async fn yield_tasks() {
    for _ in 0..12 {
        tokio::task::yield_now().await;
    }
}

async fn pump_ai_turn() {
    for _ in 0..40 {
        tokio::task::yield_now().await;
        std::thread::sleep(Duration::from_millis(5));
    }
}

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
async fn midhand_tenpai_inflection_injects_exact_hand_facts_without_leaking_opponent_hidden_tiles()
{
    // East holds 3 secret 红中 (never discarded); West holds 2 secret 发财 (never discarded).
    // South starts 1-shanten (`123m 456m 78p 23s 55p 9s`) and draws `9p` on its first draw,
    // immediately entering tenpai (waiting on `1s` and `4s`, 门前清)!
    let east_14 = tiles("1p 4p 7m 8m 9m 6s 7s 8s 2p 3p 6p C C C");
    let south_13 = tiles("123m 456m 78p 23s 55p 9s");
    let west_13 = tiles("2m 5m 8m 5s 8s 3p 6p 9p E S W F F");
    let north_13 = tiles("3m 6m 9m 4s 7s 1p 4p 7p E S W N B");
    // Draw sequence: South draws 9p (enters tenpai!), West draws B, North draws N, East draws 2m...
    let draws = tiles("9p B N 2m 5m 8m 3p 6p");
    let wall = build_preset_wall(&east_14, &south_13, &west_13, &north_13, &draws, &[])
        .expect("preset tenpai wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });

    let llm = ScriptedLlmClient::new(vec![
        BanterOutput {
            mood: PersonaMood::Smug,
            text: "开局这牌稳了，看我表演！".to_string(),
        },
        BanterOutput {
            mood: PersonaMood::Plotting,
            text: "嘿嘿，这手牌突然顺起来了，你们小心点！".to_string(),
        },
    ]);

    let options = TableOptions::with_wall_factory(42, factory)
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let start_snap = start_solo_east_table(&runtime, &mut east).await;

    // Drain opening banter and advance past the 3.5s per-seat cooldown.
    yield_tasks().await;
    let _ = east.drain_all();
    tokio::time::advance(Duration::from_millis(4000)).await;

    // East discards 1p -> South draws 9p and enters tenpai!
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: start_snap.seq,
            client_action_id: "east-discard-1p".to_string(),
            action: PlayerAction::Discard {
                tile: tile("1p"),
                from_drawn: false,
            },
        },
    )
    .await;
    pump_ai_turn().await;

    let msgs = east.drain_all();
    assert!(
        msgs.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter {
                seat: Seat::South,
                persona_id: PersonaId::QuickHothead,
                mood: PersonaMood::Plotting,
                reply_to_seat: None,
                ..
            }
        )),
        "expected South mid-hand EnteredTenpai banter, got {msgs:?}"
    );

    let captured = llm.captured();
    assert!(
        captured.len() >= 2,
        "expected HandStarted + South EnteredTenpai prompts, got {}",
        captured.len()
    );
    let tenpai_req = &captured[1];
    assert!(
        tenpai_req.system_prompt.contains("预计算手牌事实")
            && tenpai_req.system_prompt.contains("严禁捏造")
            && tenpai_req.system_prompt.contains("10 到 26 字"),
        "system prompt missing anti-hallucination/conciseness rules: {}",
        tenpai_req.system_prompt
    );
    assert!(
        tenpai_req.user_prompt.contains("刚刚进入听牌状态")
            && tenpai_req.user_prompt.contains("预计算手牌事实")
            && tenpai_req.user_prompt.contains("一条")
            && tenpai_req.user_prompt.contains("四条")
            && tenpai_req.user_prompt.contains("九筒"),
        "tenpai prompt missing South's own hand or precomputed facts: {}",
        tenpai_req.user_prompt
    );
    assert!(
        !tenpai_req.user_prompt.contains("红中") && !tenpai_req.user_prompt.contains("发财"),
        "mid-hand prompt leaked opponent hidden tiles: {}",
        tenpai_req.user_prompt
    );
}

#[tokio::test(start_paused = true)]
async fn stalled_draw_inflection_fires_and_per_seat_cooldown_blocks_until_3500ms_elapsed() {
    // Craft a wall with NO Pon/Kong/Ron interference across 5 full rounds:
    // South starts at 2-shanten (`12m 45m 78p 23s 55p E S W`), draws 5 consecutive dead honors
    // (`B`, `F`, `C`, `N`, `B` — none help South's hand and West/North hold only singletons).
    let east_14 = tiles("1m 3m 6m 8m 9m 1s 4s 6s 8s 9s 1p 4p 6p 9p");
    let south_13 = tiles("12m 45m 78p 23s 55p E S W");
    let west_13 = tiles("2m 5m 7m 9m 1s 4s 7s 9s 1p 3p 6p 8p 9p");
    let north_13 = tiles("3m 6m 8m 9m 2s 5s 7s 8s 2p 4p 6p 7p 9p");
    // Round draw order after East's discard: South, West, North, East (4 tiles per round).
    let draws = tiles(
        "B C F 7m \
         F B C 3s \
         C N B 5s \
         N F N 2p \
         B C F 3p",
    );
    let wall = build_preset_wall(&east_14, &south_13, &west_13, &north_13, &draws, &[])
        .expect("stalled draws wall");
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });

    let llm = ScriptedLlmClient::new(vec![
        BanterOutput {
            mood: PersonaMood::Smug,
            text: "开局先打个招呼！".to_string(),
        },
        BanterOutput {
            mood: PersonaMood::Tilted,
            text: "连摸三手烂牌，发牌员今天存心演我！".to_string(),
        },
        BanterOutput {
            mood: PersonaMood::Tilted,
            text: "冷却都转完了还给我发烂牌，真服了！".to_string(),
        },
    ]);

    let options = TableOptions::with_wall_factory(99, factory)
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let mut cur_seq = start_solo_east_table(&runtime, &mut east).await.seq;

    yield_tasks().await;
    let _ = east.drain_all();
    // Move past opening cooldown (`t = 4000ms`).
    tokio::time::advance(Duration::from_millis(4000)).await;

    let east_discards = [tile("9p"), tile("9s"), tile("9m"), tile("1p"), tile("1s")];
    let mut south_banter_count_per_round = Vec::new();

    for (round_idx, discard_tile) in east_discards.into_iter().enumerate() {
        // Timing plan:
        // Rounds 0, 1, 2 at t = 4000ms -> Round 2 (3rd stalled draw!) triggers South banter!
        // Round 3 at t = 6000ms (only +2000ms < 3500ms cooldown) -> MUST be blocked!
        // Round 4 at t = 7600ms (+3600ms >= 3500ms cooldown) -> MUST fire again!
        if round_idx == 3 {
            tokio::time::advance(Duration::from_millis(2000)).await;
        } else if round_idx == 4 {
            tokio::time::advance(Duration::from_millis(1600)).await;
        }

        east.send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: cur_seq,
                client_action_id: format!("east-r{round_idx}"),
                action: PlayerAction::Discard {
                    tile: discard_tile,
                    from_drawn: false,
                },
            },
        )
        .await;
        pump_ai_turn().await;

        let msgs = east.drain_all();
        for msg in &msgs {
            if let ServerMessage::EventBatch { to_seq, .. } = msg {
                cur_seq = *to_seq;
            }
        }
        let south_lines = msgs
            .iter()
            .filter(|m| {
                matches!(
                    m,
                    ServerMessage::TableBanter {
                        seat: Seat::South,
                        ..
                    }
                )
            })
            .count();
        south_banter_count_per_round.push(south_lines);
    }

    // Rounds 0 & 1: streak 1 & 2 (< 3) -> 0 lines.
    // Round 2: streak == 3 -> 1 line!
    // Round 3 (+2000ms < 3500ms cooldown): streak == 4 -> BLOCKED (0 lines)!
    // Round 4 (+3600ms >= 3500ms cooldown): streak == 5 -> ALLOWED (1 line)!
    assert_eq!(
        south_banter_count_per_round,
        vec![0, 0, 1, 0, 1],
        "expected stalled-draw inflection on round 2, cooldown block on round 3, and re-trigger on round 4"
    );
}

#[tokio::test(start_paused = true)]
async fn highlight_banter_chains_set_reply_to_seat_and_cap_strictly_at_1_to_2_rounds() {
    // Hand 1: East starts with instant Tsumo -> HandSettled triggers opener + 2 follow-up replies (cap = 2).
    let east_win = tiles("111m 222m 333m 444m CC");
    let other_13 = tiles("58m 147s 147p E S W N B");
    let wall1 = build_preset_wall(&east_win, &other_13, &other_13, &other_13, &[], &[])
        .expect("tsumo wall");

    // Hand 2: East discards 1m (keeping hidden 红中); South Pons 1m -> Pon triggers opener + 1 follow-up reply (cap = 1).
    let east_h2 = tiles("1m 4m 7m 1s 4s 7s 1p 4p 7p E S W N C");
    let south_h2 = tiles("11m 222m 333m 55s 67s 9p");
    let west_h2 = tiles("6m 9m 2s 5s 8s 2p 5p 8p E S W N B");
    let north_h2 = tiles("6m 9m 3s 6s 9s 3p 6p 9p E S W N F");
    let wall2 =
        build_preset_wall(&east_h2, &south_h2, &west_h2, &north_h2, &[], &[]).expect("pon wall");

    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(
            Seat::East,
            vec![wall1.clone(), wall2.clone()],
        )))
    });

    let llm = ScriptedLlmClient::new(vec![
        // Hand 1 opening
        BanterOutput {
            mood: PersonaMood::Smug,
            text: "第一小局开打！".to_string(),
        },
        // Hand 1 settlement opener + 2 follow-up replies
        BanterOutput {
            mood: PersonaMood::Tilted,
            text: "起手天胡自摸？这发牌员绝对收红包了！".to_string(),
        },
        BanterOutput {
            mood: PersonaMood::Mocking,
            text: "哎哟急眼哥别破防呀，手气不好就认命嘛。".to_string(),
        },
        BanterOutput {
            mood: PersonaMood::Plotting,
            text: "哈哈哈打起来打起来，我正好瓜子都嗑上了！".to_string(),
        },
        // Hand 2 opening
        BanterOutput {
            mood: PersonaMood::Deadpan,
            text: "第二小局重新做人。".to_string(),
        },
        // Hand 2 Pon opener + 1 follow-up reply
        BanterOutput {
            mood: PersonaMood::Smug,
            text: "碰你这张一万，节奏直接起飞！".to_string(),
        },
        BanterOutput {
            mood: PersonaMood::Mocking,
            text: "碰个一万也这么得意，可别回头点炮哭鼻子哦。".to_string(),
        },
    ]);

    let options = TableOptions::with_wall_factory(17, factory)
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let start_snap = start_solo_east_table(&runtime, &mut east).await;

    // Finish Hand 1 opening banter and advance past cooldown so all 3 AI seats are eligible for chain.
    yield_tasks().await;
    let _ = east.drain_all();
    tokio::time::advance(Duration::from_millis(4000)).await;

    // 1. East declares Tsumo -> HandSettled!
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

    // Advance 700ms (> 650ms chain beat) -> Follow-up reply #1!
    tokio::time::advance(Duration::from_millis(700)).await;
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
    assert_eq!(
        settle_banters.len(),
        2,
        "expected 1st follow-up chain reply"
    );
    let reply1_seat = settle_banters[1].0;
    assert_ne!(reply1_seat, opener_seat);
    assert_eq!(settle_banters[1].1, Some(opener_seat));

    // Advance another 700ms -> Follow-up reply #2!
    tokio::time::advance(Duration::from_millis(700)).await;
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
    assert_eq!(
        settle_banters.len(),
        3,
        "expected 2nd follow-up chain reply"
    );
    let reply2_seat = settle_banters[2].0;
    assert_ne!(reply2_seat, opener_seat);
    assert_ne!(reply2_seat, reply1_seat);
    assert_eq!(settle_banters[2].1, Some(reply1_seat));

    // Advance 10 full seconds -> MUST NEVER emit a 3rd follow-up reply (strict cap = 2)!
    tokio::time::advance(Duration::from_secs(10)).await;
    yield_tasks().await;
    let extra_after_cap = east.drain_all();
    assert!(
        !extra_after_cap
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "chain must strictly stop after 2 follow-up replies, got {extra_after_cap:?}"
    );

    // Verify follow-up prompts referenced the previous line verbatim.
    let captured_h1 = llm.captured();
    assert_eq!(captured_h1.len(), 4); // 1 opening + 1 opener + 2 follow-ups
    assert!(captured_h1[2]
        .user_prompt
        .contains("起手天胡自摸？这发牌员绝对收红包了！"));
    assert!(captured_h1[3]
        .user_prompt
        .contains("哎哟急眼哥别破防呀，手气不好就认命嘛。"));

    // 2. Confirm Hand 2 -> East discards 1m -> South Pons 1m (Pon chain capped at 1 follow-up, privacy-safe!).
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
    let mut h2_seq = settled_seq;
    for msg in east.drain_all() {
        if let ServerMessage::EventBatch { to_seq, .. } = msg {
            h2_seq = to_seq;
        }
    }
    tokio::time::advance(Duration::from_millis(4000)).await;

    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: h2_seq,
            client_action_id: "h2-discard-1m".to_string(),
            action: PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        },
    )
    .await;
    pump_ai_turn().await;

    let mut pon_banters: Vec<(Seat, Option<Seat>, String)> = Vec::new();
    let mut saw_pon = false;
    for msg in east.drain_all() {
        match msg {
            ServerMessage::EventBatch { events, .. }
                if events.iter().any(|e| {
                    matches!(
                        e.kind,
                        PlayerEventKind::MeldDeclared {
                            seat: Seat::South,
                            ..
                        }
                    )
                }) =>
            {
                saw_pon = true;
            }
            ServerMessage::TableBanter {
                seat,
                reply_to_seat,
                text,
                ..
            } => pon_banters.push((seat, reply_to_seat, text)),
            _ => {}
        }
    }
    assert!(saw_pon, "expected South Pon event");
    assert_eq!(pon_banters.len(), 1);
    assert_eq!(pon_banters[0].0, Seat::South);
    assert_eq!(pon_banters[0].1, None);

    // Advance 700ms -> Pon chain emits 1 follow-up reply referencing South!
    tokio::time::advance(Duration::from_millis(700)).await;
    yield_tasks().await;
    for msg in east.drain_all() {
        if let ServerMessage::TableBanter {
            seat,
            reply_to_seat,
            text,
            ..
        } = msg
        {
            pon_banters.push((seat, reply_to_seat, text));
        }
    }
    assert_eq!(pon_banters.len(), 2, "expected 1 follow-up reply after Pon");
    assert_ne!(pon_banters[1].0, Seat::South);
    assert_eq!(pon_banters[1].1, Some(Seat::South));

    // Advance 10s -> Pon chain strictly caps at 1 follow-up reply (no further replies)!
    tokio::time::advance(Duration::from_secs(10)).await;
    yield_tasks().await;
    let extra_pon = east.drain_all();
    assert!(
        !extra_pon
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "Pon chain must cap strictly at 1 follow-up reply, got {extra_pon:?}"
    );

    // Mid-hand Pon follow-up prompt must reference South's line, include replier's own hand facts,
    // and NEVER leak East's hidden red dragon (`红中`).
    let all_captured = llm.captured();
    let pon_reply_req = all_captured.last().expect("pon reply prompt");
    assert!(pon_reply_req
        .user_prompt
        .contains("碰你这张一万，节奏直接起飞！"));
    assert!(pon_reply_req.user_prompt.contains("预计算手牌事实"));
    assert!(
        !pon_reply_req.user_prompt.contains("红中"),
        "mid-hand chain reply leaked East's hidden tile: {}",
        pon_reply_req.user_prompt
    );
}
