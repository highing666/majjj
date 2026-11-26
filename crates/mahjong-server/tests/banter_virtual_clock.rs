mod common;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{no_reaction_wall, tile, tiles};
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
    ClientMessage, SeatKind, SeatingRule, ServerMessage, TablePace, TablePhase, TableSnapshot,
};
use mahjong_server::session::{
    BoxedWallProvider, OutboundSignal, TableOptions, TableRuntime, WallProviderFactory,
};
use tokio::sync::{mpsc, Notify};

#[derive(Debug)]
enum PlannedReply {
    Immediate(Result<BanterOutput, LlmError>),
    Delayed(Duration, Result<BanterOutput, LlmError>),
    Gated(Arc<Notify>, Result<BanterOutput, LlmError>),
}

#[derive(Debug)]
struct ScriptedLlmClient {
    replies: Mutex<VecDeque<PlannedReply>>,
    captured_requests: Mutex<Vec<BanterGenerationRequest>>,
}

impl ScriptedLlmClient {
    fn new(replies: Vec<PlannedReply>) -> Arc<Self> {
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
            .unwrap_or_else(|| {
                PlannedReply::Immediate(Ok(BanterOutput {
                    mood: PersonaMood::Deadpan,
                    text: "默认补位吐槽".to_string(),
                }))
            });
        Box::pin(async move {
            match next {
                PlannedReply::Immediate(res) => res,
                PlannedReply::Delayed(delay, res) => {
                    tokio::time::sleep(delay).await;
                    res
                }
                PlannedReply::Gated(notify, res) => {
                    notify.notified().await;
                    res
                }
            }
        })
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
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}

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

#[tokio::test(start_paused = true)]
async fn highlight_sync_cap_bounds_meld_delay_emits_outage_notice_and_recovers_on_settlement() {
    let llm = ScriptedLlmClient::new(vec![
        // 1. HandStarted opening banter (fast)
        PlannedReply::Immediate(Ok(BanterOutput {
            mood: PersonaMood::Plotting,
            text: "开局手气来了，你们小心点炮！".to_string(),
        })),
        // 2. MeldDeclared (South Pon) hangs 5s -> must hit 1500ms sync cap and NOT stall AI discard!
        PlannedReply::Delayed(
            Duration::from_secs(5),
            Ok(BanterOutput {
                mood: PersonaMood::Smug,
                text: "太晚了不应该出现".to_string(),
            }),
        ),
        // 3. HandSettled recovers -> must emit LlmStatusNotice { available: true } + TableBanter!
        PlannedReply::Immediate(Ok(BanterOutput {
            mood: PersonaMood::Tilted,
            text: "这手自摸算你走运，下把加倍讨回来！".to_string(),
        })),
    ]);

    let options =
        TableOptions::with_wall_factory(11, east_discards_south_pons_then_east_tsumo_walls())
            .with_pacing(PacingMode::Immediate)
            .with_llm_client(llm.clone());
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
            expected_seq: start_snap.seq,
            client_action_id: "discard-1m".to_string(),
            action: PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        },
    )
    .await;

    // Give Rayon AI pool a tiny real-thread slice to decide Pon.
    for _ in 0..40 {
        tokio::task::yield_now().await;
        std::thread::sleep(Duration::from_millis(5));
    }
    let after_pon = east.drain_all();
    let saw_pon = after_pon.iter().any(|m| {
        matches!(
            m,
            ServerMessage::EventBatch { events, .. }
                if events.iter().any(|e| matches!(e.kind, PlayerEventKind::MeldDeclared { seat: Seat::South, .. }))
        )
    });
    assert!(saw_pon, "expected South Pon EventBatch, got {after_pon:?}");

    // At t = 1400ms (before 1500ms sync cap), South's follow-up discard is still held waiting for sync banter
    // and no outage notice has fired yet.
    tokio::time::advance(Duration::from_millis(1400)).await;
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
        "must not emit timeout notice before 1500ms cap: {mid_wait:?}"
    );

    // Crossing t = 1500ms triggers hard cap: emits LlmStatusNotice { available: false } (no fake banter!)
    // and unblocks South AI's follow-up discard!
    tokio::time::advance(Duration::from_millis(150)).await;
    for _ in 0..40 {
        tokio::task::yield_now().await;
        std::thread::sleep(Duration::from_millis(5));
    }
    let after_cap = east.drain_all();
    assert!(
        after_cap.iter().any(|m| matches!(
            m,
            ServerMessage::LlmStatusNotice {
                available: false,
                ..
            }
        )),
        "expected LlmStatusNotice {{ available: false }} after 1500ms cap, got {after_cap:?}"
    );
    assert!(
        !after_cap
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanter { .. })),
        "must never emit fake banter on timeout: {after_cap:?}"
    );
    assert!(
        after_cap.iter().any(|m| {
            matches!(
                m,
                ServerMessage::EventBatch { events, .. }
                    if events.iter().any(|e| matches!(e.kind, PlayerEventKind::TileDiscarded { seat: Seat::South, .. }))
            )
        }),
        "AI follow-up discard must proceed right after 1500ms cap without stalling gameplay: {after_cap:?}"
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
async fn takeover_and_disband_drop_in_flight_banter_without_ghost_lines() {
    let gate1 = Arc::new(Notify::new());
    let gate2 = Arc::new(Notify::new());
    let llm = ScriptedLlmClient::new(vec![
        PlannedReply::Gated(
            Arc::clone(&gate1),
            Ok(BanterOutput {
                mood: PersonaMood::Smug,
                text: "这句不该在接管后幽灵弹出！".to_string(),
            }),
        ),
        PlannedReply::Gated(
            Arc::clone(&gate2),
            Ok(BanterOutput {
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

    // Identify which AI seat is currently running the gated opening banter by checking all AI seats,
    // then have observer take over EVERY AI seat? One observer can take over 1 seat, so let's connect 3 observers
    // or take over the exact seat that received the HandStarted prompt.
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

    // Confirm taken-over seat still has mood == None via RequestSnapshot.
    observer
        .send(&runtime, &ClientMessage::RequestSnapshot)
        .await;
    let verify_snap = observer.recv_snapshot().await;
    assert_eq!(verify_snap.seats[target_seat.index()].mood, None);

    // Now test DisbandMatch while gate2 is in-flight: disconnect observer so East can disband solo,
    // disband first match, start match #2 (hits gate2), then immediately disband match #2!
    runtime.on_disconnect(observer.conn_id).await;
    east.send(&runtime, &ClientMessage::RequestDisbandMatch)
        .await;
    let lobby_snap = east.recv_snapshot().await;
    assert_eq!(lobby_snap.phase, TablePhase::Lobby);

    east.send(&runtime, &ClientMessage::StartMatch).await;
    let match2_snap = east.recv_snapshot().await;
    assert_eq!(match2_snap.phase, TablePhase::InMatch);
    yield_tasks().await;

    // Immediately disband match #2 while gate2 is in-flight.
    east.send(&runtime, &ClientMessage::RequestDisbandMatch)
        .await;
    let lobby_snap2 = east.recv_snapshot().await;
    assert_eq!(lobby_snap2.phase, TablePhase::Lobby);
    let _ = east.drain_all();

    // Release gate2 -> MUST be strictly dropped in Lobby!
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
