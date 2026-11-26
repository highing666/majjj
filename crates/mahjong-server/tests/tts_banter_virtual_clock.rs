mod common;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{
    no_reaction_wall, pump_ai_turn, tile, tiles, yield_tasks, PlannedReply, ScriptedLlmClient,
    VirtualClient,
};
use mahjong_ai::AiDifficulty;
use mahjong_core::{
    build_preset_wall, MatchMode, PlayerAction, PlayerEventKind, PresetWallProvider, Seat,
};
use mahjong_server::banter::SLOW_THINK_NUDGE_DELAY;
use mahjong_server::banter_audio::BanterAudioCache;
use mahjong_server::llm::BanterOutput;
use mahjong_server::pacing::PacingMode;
use mahjong_server::persona::{PersonaId, PersonaMood};
use mahjong_server::protocol::{
    ClientMessage, QuickEmote, SeatKind, SeatingRule, ServerMessage, TablePace, TablePhase,
};
use mahjong_server::session::{BoxedWallProvider, TableOptions, TableRuntime, WallProviderFactory};
use mahjong_server::tts::{
    TtsAudioOutput, TtsClient, TtsError, TtsFuture, TtsHealthStatus, TtsSynthesisRequest,
    TTS_COOLDOWN_DURATION, TTS_IN_HAND_DEADLINE_MS, TTS_SETTLEMENT_DEADLINE_MS,
};
use tokio::sync::Notify;

fn make_sample_wav(tag: u8, payload_len: usize) -> Vec<u8> {
    let data_len = payload_len as u32;
    let riff_len = 36 + data_len;
    let sample_rate: u32 = 24_000;
    let byte_rate = sample_rate * 2;
    let mut out = Vec::with_capacity(44 + payload_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_len.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&2_u16.to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.resize(44 + payload_len, tag);
    out
}

#[derive(Debug)]
enum PlannedTtsReply {
    Immediate(Result<TtsAudioOutput, TtsError>),
    Delayed(Duration, Result<TtsAudioOutput, TtsError>),
    Gated(Arc<Notify>, Result<TtsAudioOutput, TtsError>),
}

struct ScriptedTtsClient {
    healthy: AtomicBool,
    fail_on_unhealthy_synth: AtomicBool,
    settlement_delay: Mutex<Option<(Duration, TtsAudioOutput)>>,
    health_calls: AtomicUsize,
    replies: Mutex<VecDeque<PlannedTtsReply>>,
    captured: Mutex<Vec<TtsSynthesisRequest>>,
}

impl std::fmt::Debug for ScriptedTtsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptedTtsClient").finish_non_exhaustive()
    }
}

impl ScriptedTtsClient {
    fn new(healthy: bool, replies: Vec<PlannedTtsReply>) -> Arc<Self> {
        Arc::new(Self {
            healthy: AtomicBool::new(healthy),
            fail_on_unhealthy_synth: AtomicBool::new(false),
            settlement_delay: Mutex::new(None),
            health_calls: AtomicUsize::new(0),
            replies: Mutex::new(replies.into()),
            captured: Mutex::new(Vec::new()),
        })
    }

    fn set_healthy(&self, healthy: bool) {
        self.healthy.store(healthy, Ordering::SeqCst);
    }

    fn set_fail_on_unhealthy_synth(&self, enabled: bool) {
        self.fail_on_unhealthy_synth
            .store(enabled, Ordering::SeqCst);
    }

    fn set_settlement_delay(&self, delay: Duration, output: TtsAudioOutput) {
        *self.settlement_delay.lock().expect("lock settlement delay") = Some((delay, output));
    }

    fn health_calls(&self) -> usize {
        self.health_calls.load(Ordering::SeqCst)
    }

    fn captured(&self) -> Vec<TtsSynthesisRequest> {
        self.captured.lock().expect("lock captured tts").clone()
    }
}

impl TtsClient for ScriptedTtsClient {
    fn endpoint(&self) -> &str {
        "http://virtual-tts.local:8081"
    }

    fn health_check(&self) -> TtsFuture<'_, Result<TtsHealthStatus, TtsError>> {
        self.health_calls.fetch_add(1, Ordering::SeqCst);
        let ok = self.healthy.load(Ordering::SeqCst);
        Box::pin(async move {
            if ok {
                Ok(TtsHealthStatus {
                    endpoint: "http://virtual-tts.local:8081".to_string(),
                    backend: Some("virtual-qwen3-tts".to_string()),
                    model: Some("Qwen3-TTS-12Hz-0.6B-CustomVoice".to_string()),
                })
            } else {
                Err(TtsError::Transport {
                    endpoint: "http://virtual-tts.local:8081".to_string(),
                    reason: "connection refused".to_string(),
                })
            }
        })
    }

    fn synthesize(
        &self,
        request: TtsSynthesisRequest,
    ) -> TtsFuture<'_, Result<TtsAudioOutput, TtsError>> {
        self.captured
            .lock()
            .expect("lock captured tts")
            .push(request.clone());
        let scripted = self.replies.lock().expect("lock tts replies").pop_front();
        let settlement_override = if request.deadline_ms == TTS_SETTLEMENT_DEADLINE_MS {
            self.settlement_delay
                .lock()
                .expect("lock settlement delay")
                .clone()
        } else {
            None
        };
        let fail_if_down = self.fail_on_unhealthy_synth.load(Ordering::SeqCst)
            && !self.healthy.load(Ordering::SeqCst);
        Box::pin(async move {
            if let Some(next) = scripted {
                return match next {
                    PlannedTtsReply::Immediate(res) => res,
                    PlannedTtsReply::Delayed(delay, res) => {
                        tokio::time::sleep(delay).await;
                        res
                    }
                    PlannedTtsReply::Gated(gate, res) => {
                        gate.notified().await;
                        res
                    }
                };
            }
            if fail_if_down {
                return Err(TtsError::Transport {
                    endpoint: "http://virtual-tts.local:8081".to_string(),
                    reason: "service unavailable".to_string(),
                });
            }
            if let Some((delay, output)) = settlement_override {
                tokio::time::sleep(delay).await;
                return Ok(output);
            }
            Ok(TtsAudioOutput {
                wav_bytes: make_sample_wav(0x2A, 960),
                duration_ms: 950,
            })
        })
    }
}

fn pon_then_tsumo_walls() -> WallProviderFactory {
    // Hand 1: East discards 1m; South Pons 1m (highlight).
    let east_14 = tiles("1m 4m 7m 1s 4s 7s 1p 4p 7p E S W N C");
    let south_13 = tiles("11m 222m 333m 55s 67s 9p");
    let west_13 = tiles("6m 9m 2s 5s 8s 2p 5p 8p E S W N B");
    let north_13 = tiles("6m 9m 3s 6s 9s 3p 6p 9p E S W N F");
    let wall1 =
        build_preset_wall(&east_14, &south_13, &west_13, &north_13, &[], &[]).expect("pon wall");

    // Hand 2: East instant Tsumo (settlement).
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

fn instant_tsumo_then_no_reaction_walls() -> WallProviderFactory {
    let east_win = tiles("111m 222m 333m 444m CC");
    let other_13 = tiles("58m 147s 147p E S W N B");
    let wall1 = build_preset_wall(&east_win, &other_13, &other_13, &other_13, &[], &[])
        .expect("tsumo wall");
    let wall2 = no_reaction_wall();
    Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::new(
            Seat::East,
            vec![wall1.clone(), wall2.clone()],
        )))
    })
}

async fn start_solo_match(
    runtime: &TableRuntime,
    east: &mut VirtualClient,
) -> mahjong_server::protocol::TableSnapshot {
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
async fn banter_broadcasts_immediately_without_waiting_on_tts_and_synthesizes_once() {
    let wav_opening = make_sample_wav(0x11, 1200);
    let wav_pon = make_sample_wav(0x22, 1600);
    let llm = ScriptedLlmClient::from_replies(vec![
        PlannedReply::Immediate(Ok(BanterOutput {
            mood: PersonaMood::Plotting,
            text: "开局这牌有点东西！".to_string(),
        })),
        PlannedReply::Immediate(Ok(BanterOutput {
            mood: PersonaMood::Smug,
            text: "碰你一万，节奏我拿捏了！".to_string(),
        })),
    ]);
    let tts = ScriptedTtsClient::new(
        true,
        vec![
            PlannedTtsReply::Immediate(Ok(TtsAudioOutput {
                wav_bytes: wav_opening.clone(),
                duration_ms: 820,
            })),
            // Slow TTS (1200ms) must NOT delay TableBanter or the 1.5s highlight sync window!
            PlannedTtsReply::Delayed(
                Duration::from_millis(1200),
                Ok(TtsAudioOutput {
                    wav_bytes: wav_pon.clone(),
                    duration_ms: 1180,
                }),
            ),
        ],
    );

    let options = TableOptions::with_wall_factory(101, pon_then_tsumo_walls())
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm)
        .with_tts_client(tts.clone());
    let runtime = TableRuntime::with_options(8789, None, options);

    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let mut observer = VirtualClient::connect(&runtime, "obs", "旁观者").await;
    let start_snap = start_solo_match(&runtime, &mut east).await;
    let _ = observer.recv_snapshot().await;
    yield_tasks().await;

    let open_msgs = east.drain_all();
    let _ = observer.drain_all();
    let open_id = open_msgs
        .iter()
        .find_map(|m| match m {
            ServerMessage::TableBanter { banter_id, .. } => Some(banter_id.clone()),
            _ => None,
        })
        .expect("opening TableBanter");
    assert_eq!(open_id, "b-1");
    assert!(
        open_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanterAudioReady { banter_id, audio_url, duration_ms, .. }
                if banter_id == "b-1" && audio_url == "/api/banter-audio/b-1" && *duration_ms == 820
        )),
        "opening TableBanterAudioReady missing: {open_msgs:?}"
    );
    assert_eq!(
        runtime.get_banter_audio("b-1").as_deref(),
        Some(wav_opening.as_slice())
    );

    // East discards 1m -> South Pons 1m. LLM is immediate while TTS takes 1200ms.
    // TableBanter AND South's follow-up discard must proceed IMMEDIATELY at t=0 without waiting 1200ms!
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: start_snap.seq,
            client_action_id: "d-1m".to_string(),
            action: PlayerAction::Discard {
                tile: tile("1m"),
                from_drawn: false,
            },
        },
    )
    .await;
    pump_ai_turn().await;

    let immediate_msgs = east.drain_all();
    let obs_immediate = observer.drain_all();
    let pon_banter_id = immediate_msgs
        .iter()
        .find_map(|m| match m {
            ServerMessage::TableBanter {
                banter_id,
                seat: Seat::South,
                text,
                ..
            } if text.contains("碰你一万") => Some(banter_id.clone()),
            _ => None,
        })
        .expect("immediate Pon TableBanter must arrive before TTS completes");
    assert_eq!(pon_banter_id, "b-2");
    assert!(
        immediate_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::EventBatch { events, .. }
                if events.iter().any(|e| matches!(e.kind, PlayerEventKind::TileDiscarded { seat: Seat::South, .. }))
        )),
        "highlight sync must unblock immediately once TableBanter is broadcast, not wait on TTS: {immediate_msgs:?}"
    );
    assert!(
        !immediate_msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanterAudioReady { banter_id, .. } if banter_id == "b-2")),
        "TableBanterAudioReady must not fire before TTS finishes"
    );
    assert!(obs_immediate
        .iter()
        .any(|m| matches!(m, ServerMessage::TableBanter { banter_id, .. } if banter_id == "b-2")));

    // Advance virtual clock 1200ms -> single synthesis completes, BOTH clients get TableBanterAudioReady.
    tokio::time::advance(Duration::from_millis(1200)).await;
    yield_tasks().await;

    let east_ready = east.drain_all();
    let obs_ready = observer.drain_all();
    for (label, msgs) in [("east", &east_ready), ("observer", &obs_ready)] {
        assert!(
            msgs.iter().any(|m| matches!(
                m,
                ServerMessage::TableBanterAudioReady {
                    banter_id,
                    seat: Seat::South,
                    audio_url,
                    duration_ms: 1180,
                } if banter_id == "b-2" && audio_url == "/api/banter-audio/b-2"
            )),
            "{label} missing TableBanterAudioReady for b-2: {msgs:?}"
        );
    }

    // Only ONE upstream TTS call per banter even with 2 connected clients!
    let captured = tts.captured();
    let b2_reqs: Vec<&TtsSynthesisRequest> =
        captured.iter().filter(|r| r.request_id == "b-2").collect();
    assert_eq!(b2_reqs.len(), 1);
    assert_eq!(b2_reqs[0].persona_id, PersonaId::QuickHothead);
    assert_eq!(b2_reqs[0].mood, PersonaMood::Smug);
    assert_eq!(b2_reqs[0].format, "wav");
    assert!(!b2_reqs[0].stream);
    assert_eq!(b2_reqs[0].deadline_ms, TTS_IN_HAND_DEADLINE_MS);
    assert_eq!(
        runtime.get_banter_audio("b-2").as_deref(),
        Some(wav_pon.as_slice())
    );
}

#[tokio::test(start_paused = true)]
async fn settlement_quick_emote_and_slow_think_nudge_use_expected_deadlines() {
    let wav_settle = make_sample_wav(0x33, 2000);
    let llm = ScriptedLlmClient::with_banter_fn(|req| {
        if req.user_prompt.contains("四家摊开手牌") || req.user_prompt.contains("结算") {
            Ok(BanterOutput {
                mood: PersonaMood::Tilted,
                text: "开局自摸这也太离谱了！".to_string(),
            })
        } else if req.user_prompt.contains("快捷互动") {
            Ok(BanterOutput {
                mood: PersonaMood::Mocking,
                text: "别急，这就打给你看！".to_string(),
            })
        } else {
            Ok(BanterOutput {
                mood: PersonaMood::Deadpan,
                text: "局中正常吐槽一句。".to_string(),
            })
        }
    });
    let tts = ScriptedTtsClient::new(true, vec![]);
    // Settlement synthesis takes 3500ms (> 2500ms in-hand cap, <= 5000ms settlement cap) -> MUST succeed!
    tts.set_settlement_delay(
        Duration::from_millis(3500),
        TtsAudioOutput {
            wav_bytes: wav_settle.clone(),
            duration_ms: 2100,
        },
    );

    let options = TableOptions::with_wall_factory(102, instant_tsumo_then_no_reaction_walls())
        .with_pacing(PacingMode::Immediate)
        .with_llm_client(llm)
        .with_tts_client(tts.clone());
    let runtime = TableRuntime::with_options(8789, None, options);
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let start_snap = start_solo_match(&runtime, &mut east).await;
    yield_tasks().await;
    let _ = east.drain_all();

    // Quick emote trigger -> in-hand deadline 2500ms.
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
    yield_tasks().await;
    let _ = east.drain_all();

    // Slow-think nudge after 16s -> in-hand deadline 2500ms.
    tokio::time::advance(SLOW_THINK_NUDGE_DELAY + Duration::from_millis(100)).await;
    yield_tasks().await;
    let _ = east.drain_all();
    let in_hand_tts_count = tts.captured().len();
    assert!(
        in_hand_tts_count >= 3,
        "expected opening + quick-emote + slow-think nudge TTS calls, got {in_hand_tts_count}"
    );

    // East wins by Tsumo -> settlement banter (and any settlement chain reply) gets 5000ms deadline.
    east.send(
        &runtime,
        &ClientMessage::SubmitAction {
            expected_seq: start_snap.seq,
            client_action_id: "win-tsumo".to_string(),
            action: PlayerAction::Tsumo,
        },
    )
    .await;
    yield_tasks().await;
    let settle_msgs = east.drain_all();
    let settle_banter_id = settle_msgs
        .iter()
        .find_map(|m| match m {
            ServerMessage::TableBanter {
                banter_id, text, ..
            } if text.contains("开局自摸") => Some(banter_id.clone()),
            _ => None,
        })
        .expect("settlement TableBanter");

    // Advance 3500ms -> still within 5000ms settlement deadline -> audio ready!
    tokio::time::advance(Duration::from_millis(3500)).await;
    yield_tasks().await;
    let after_settle_tts = east.drain_all();
    assert!(
        after_settle_tts.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanterAudioReady { banter_id, duration_ms: 2100, .. }
                if banter_id == &settle_banter_id
        )),
        "settlement audio taking 3500ms must succeed under 5000ms deadline: {after_settle_tts:?}"
    );
    assert_eq!(
        runtime.get_banter_audio(&settle_banter_id).as_deref(),
        Some(wav_settle.as_slice())
    );

    let captured = tts.captured();
    assert!(
        captured.len() > in_hand_tts_count,
        "expected settlement TTS call(s), got {captured:?}"
    );
    for req in &captured[..in_hand_tts_count] {
        assert_eq!(
            req.deadline_ms, TTS_IN_HAND_DEADLINE_MS,
            "all in-hand banter must use 2500ms deadline: {req:?}"
        );
    }
    for req in &captured[in_hand_tts_count..] {
        assert_eq!(
            req.deadline_ms, TTS_SETTLEMENT_DEADLINE_MS,
            "all settlement banter and follow-ups must use 5000ms deadline: {req:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn stale_tts_is_discarded_on_deadline_same_seat_override_next_hand_takeover_and_disband() {
    // Part A: In-hand deadline_ms (2500ms) expiry drops late audio.
    {
        let llm = ScriptedLlmClient::from_outputs(vec![BanterOutput {
            mood: PersonaMood::Smug,
            text: "超过两秒半的语音必须丢弃".to_string(),
        }]);
        let tts = ScriptedTtsClient::new(
            true,
            vec![PlannedTtsReply::Delayed(
                Duration::from_millis(3000),
                Ok(TtsAudioOutput {
                    wav_bytes: make_sample_wav(0x44, 1000),
                    duration_ms: 1000,
                }),
            )],
        );
        let wall = no_reaction_wall();
        let factory: WallProviderFactory = Arc::new(move || {
            BoxedWallProvider(Box::new(PresetWallProvider::single(
                Seat::East,
                wall.clone(),
            )))
        });
        let runtime = TableRuntime::with_options(
            8789,
            None,
            TableOptions::with_wall_factory(201, factory)
                .with_pacing(PacingMode::Immediate)
                .with_llm_client(llm)
                .with_tts_client(tts),
        );
        let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
        let _ = start_solo_match(&runtime, &mut east).await;
        yield_tasks().await;
        let _ = east.drain_all();

        tokio::time::advance(Duration::from_millis(3500)).await;
        yield_tasks().await;
        let msgs = east.drain_all();
        assert!(
            !msgs
                .iter()
                .any(|m| matches!(m, ServerMessage::TableBanterAudioReady { .. })),
            "expired >2500ms TTS must NOT broadcast TableBanterAudioReady: {msgs:?}"
        );
        assert!(runtime.get_banter_audio("b-1").is_none());
    }

    // Part B: Same-seat override discards older gated TTS once newer line commits for same seat.
    {
        let gate_old = Arc::new(Notify::new());
        let llm = ScriptedLlmClient::from_outputs(vec![
            BanterOutput {
                mood: PersonaMood::Deadpan,
                text: "第一句同座旧台词".to_string(),
            },
            BanterOutput {
                mood: PersonaMood::Smug,
                text: "第二句同座新台词覆盖！".to_string(),
            },
        ]);
        let tts = ScriptedTtsClient::new(
            true,
            vec![
                PlannedTtsReply::Gated(
                    Arc::clone(&gate_old),
                    Ok(TtsAudioOutput {
                        wav_bytes: make_sample_wav(0x55, 1000),
                        duration_ms: 700,
                    }),
                ),
                PlannedTtsReply::Immediate(Ok(TtsAudioOutput {
                    wav_bytes: make_sample_wav(0x66, 1000),
                    duration_ms: 900,
                })),
            ],
        );
        // Only ONE AI seat (South) so both opening and quick-emote reply come from Seat::South!
        let wall = no_reaction_wall();
        let factory: WallProviderFactory = Arc::new(move || {
            BoxedWallProvider(Box::new(PresetWallProvider::single(
                Seat::East,
                wall.clone(),
            )))
        });
        let runtime = TableRuntime::with_options(
            8789,
            None,
            TableOptions::with_wall_factory(202, factory)
                .with_pacing(PacingMode::Immediate)
                .with_llm_client(llm)
                .with_tts_client(tts),
        );
        let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
        let mut west = VirtualClient::connect(&runtime, "w1", "阿西").await;
        let mut north = VirtualClient::connect(&runtime, "n1", "阿北").await;
        east.send(
            &runtime,
            &ClientMessage::JoinSeat {
                seat: Some(Seat::East),
            },
        )
        .await;
        let _ = east.recv_snapshot().await;
        west.send(
            &runtime,
            &ClientMessage::JoinSeat {
                seat: Some(Seat::West),
            },
        )
        .await;
        let _ = west.recv_snapshot().await;
        north
            .send(
                &runtime,
                &ClientMessage::JoinSeat {
                    seat: Some(Seat::North),
                },
            )
            .await;
        let _ = north.recv_snapshot().await;
        east.send(
            &runtime,
            &ClientMessage::SetSeatPersona {
                seat: Seat::South,
                persona_id: Some(PersonaId::QuickHothead),
            },
        )
        .await;
        east.send(&runtime, &ClientMessage::StartMatch).await;
        let _ = east.recv_snapshot().await;
        yield_tasks().await;
        let first_msgs = east.drain_all();
        assert!(first_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanter { banter_id, seat: Seat::South, .. } if banter_id == "b-1"
        )));

        // Advance 500ms (still within 2500ms deadline) and trigger quick emote -> South emits b-2!
        tokio::time::advance(Duration::from_millis(500)).await;
        east.send(
            &runtime,
            &ClientMessage::SendQuickEmote {
                emote: QuickEmote::IsThatIt,
            },
        )
        .await;
        yield_tasks().await;
        let second_msgs = east.drain_all();
        assert!(second_msgs.iter().any(|m| matches!(
            m,
            ServerMessage::TableBanterAudioReady { banter_id, seat: Seat::South, .. } if banter_id == "b-2"
        )));

        // Now release b-1's gated TTS -> MUST be silently discarded because South moved to b-2!
        gate_old.notify_waiters();
        yield_tasks().await;
        let stale_msgs = east.drain_all();
        assert!(
            !stale_msgs.iter().any(|m| matches!(
                m,
                ServerMessage::TableBanterAudioReady { banter_id, .. } if banter_id == "b-1"
            )),
            "superseded same-seat TTS b-1 must be discarded: {stale_msgs:?}"
        );
        assert!(runtime.get_banter_audio("b-1").is_none());
        assert!(runtime.get_banter_audio("b-2").is_some());
    }

    // Part C: Next hand (ConfirmNextHand) discards previous hand's in-flight settlement TTS.
    {
        let gate_settle = Arc::new(Notify::new());
        let llm = ScriptedLlmClient::from_outputs(vec![
            BanterOutput {
                mood: PersonaMood::Deadpan,
                text: "开局".to_string(),
            },
            BanterOutput {
                mood: PersonaMood::Tilted,
                text: "上一小局结算迟到的语音".to_string(),
            },
        ]);
        let tts = ScriptedTtsClient::new(
            true,
            vec![
                PlannedTtsReply::Immediate(Ok(TtsAudioOutput {
                    wav_bytes: make_sample_wav(1, 512),
                    duration_ms: 400,
                })),
                PlannedTtsReply::Gated(
                    Arc::clone(&gate_settle),
                    Ok(TtsAudioOutput {
                        wav_bytes: make_sample_wav(2, 512),
                        duration_ms: 900,
                    }),
                ),
            ],
        );
        let runtime = TableRuntime::with_options(
            8789,
            None,
            TableOptions::with_wall_factory(203, instant_tsumo_then_no_reaction_walls())
                .with_pacing(PacingMode::Immediate)
                .with_llm_client(llm)
                .with_tts_client(tts),
        );
        let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
        let start_snap = start_solo_match(&runtime, &mut east).await;
        yield_tasks().await;
        let _ = east.drain_all();

        east.send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: start_snap.seq,
                client_action_id: "tsumo-h1".to_string(),
                action: PlayerAction::Tsumo,
            },
        )
        .await;
        yield_tasks().await;
        let settle_msgs = east.drain_all();
        let settled_seq = settle_msgs
            .iter()
            .filter_map(|m| match m {
                ServerMessage::EventBatch { to_seq, .. } => Some(*to_seq),
                _ => None,
            })
            .next_back()
            .expect("settled seq");

        // Confirm next hand BEFORE settlement TTS completes!
        east.send(
            &runtime,
            &ClientMessage::SubmitAction {
                expected_seq: settled_seq,
                client_action_id: "next-h2".to_string(),
                action: PlayerAction::ConfirmNextHand,
            },
        )
        .await;
        yield_tasks().await;
        let _ = east.drain_all();

        gate_settle.notify_waiters();
        yield_tasks().await;
        let after_next_hand = east.drain_all();
        assert!(
            !after_next_hand.iter().any(|m| matches!(
                m,
                ServerMessage::TableBanterAudioReady { banter_id, .. } if banter_id == "b-2"
            )),
            "previous hand TTS must be discarded after ConfirmNextHand: {after_next_hand:?}"
        );
        assert!(runtime.get_banter_audio("b-2").is_none());
    }

    // Part D: Takeover and Disband discard in-flight TTS cleanly.
    {
        let gate_takeover = Arc::new(Notify::new());
        let gate_disband = Arc::new(Notify::new());
        let llm = ScriptedLlmClient::from_outputs(vec![
            BanterOutput {
                mood: PersonaMood::Smug,
                text: "接管前的话".to_string(),
            },
            BanterOutput {
                mood: PersonaMood::Smug,
                text: "解散前的话".to_string(),
            },
        ]);
        let tts = ScriptedTtsClient::new(
            true,
            vec![
                PlannedTtsReply::Gated(
                    Arc::clone(&gate_takeover),
                    Ok(TtsAudioOutput {
                        wav_bytes: make_sample_wav(7, 512),
                        duration_ms: 500,
                    }),
                ),
                PlannedTtsReply::Gated(
                    Arc::clone(&gate_disband),
                    Ok(TtsAudioOutput {
                        wav_bytes: make_sample_wav(8, 512),
                        duration_ms: 500,
                    }),
                ),
            ],
        );
        let wall = no_reaction_wall();
        let factory: WallProviderFactory = Arc::new(move || {
            BoxedWallProvider(Box::new(PresetWallProvider::single(
                Seat::East,
                wall.clone(),
            )))
        });
        let runtime = TableRuntime::with_options(
            8789,
            None,
            TableOptions::with_wall_factory(204, factory)
                .with_pacing(PacingMode::Immediate)
                .with_llm_client(llm)
                .with_tts_client(tts),
        );
        let mut east = VirtualClient::connect(&runtime, "host", "房主").await;
        let mut observer = VirtualClient::connect(&runtime, "obs", "客串").await;
        let _ = start_solo_match(&runtime, &mut east).await;
        let _ = observer.recv_snapshot().await;
        yield_tasks().await;
        let open_msgs = east.drain_all();
        let _ = observer.drain_all();
        let (b1_id, b1_seat) = open_msgs
            .iter()
            .find_map(|m| match m {
                ServerMessage::TableBanter {
                    banter_id, seat, ..
                } => Some((banter_id.clone(), *seat)),
                _ => None,
            })
            .expect("opening banter");

        // Observer takes over that exact AI seat.
        observer
            .send(
                &runtime,
                &ClientMessage::TakeOverSeat {
                    seat: b1_seat,
                    confirm: true,
                },
            )
            .await;
        let snap = observer.recv_snapshot().await;
        assert_eq!(snap.seats[b1_seat.index()].kind, SeatKind::Human);
        let _ = east.drain_all();

        gate_takeover.notify_waiters();
        yield_tasks().await;
        assert!(
            !east.drain_all().iter().any(|m| matches!(
                m,
                ServerMessage::TableBanterAudioReady { banter_id, .. } if banter_id == &b1_id
            )),
            "TTS must be discarded after seat takeover"
        );
        assert!(runtime.get_banter_audio(&b1_id).is_none());

        // Disband match #2 while TTS is in-flight.
        runtime.on_disconnect(observer.conn_id).await;
        east.send(&runtime, &ClientMessage::RequestDisbandMatch)
            .await;
        let _ = east.recv_snapshot().await;
        east.send(&runtime, &ClientMessage::StartMatch).await;
        let _ = east.recv_snapshot().await;
        yield_tasks().await;
        let match2_msgs = east.drain_all();
        let b2_id = match2_msgs
            .iter()
            .find_map(|m| match m {
                ServerMessage::TableBanter { banter_id, .. } => Some(banter_id.clone()),
                _ => None,
            })
            .expect("match2 banter");

        east.send(&runtime, &ClientMessage::RequestDisbandMatch)
            .await;
        let lobby = east.recv_snapshot().await;
        assert_eq!(lobby.phase, TablePhase::Lobby);
        let _ = east.drain_all();

        gate_disband.notify_waiters();
        yield_tasks().await;
        assert!(
            !east.drain_all().iter().any(|m| matches!(
                m,
                ServerMessage::TableBanterAudioReady { banter_id, .. } if banter_id == &b2_id
            )),
            "TTS must be discarded after match disband"
        );
        assert!(runtime.get_banter_audio(&b2_id).is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn consecutive_failures_enter_15s_cooldown_skip_upstream_and_recover_via_health_probe() {
    let llm = ScriptedLlmClient::with_banter_fn(|_| {
        Ok(BanterOutput {
            mood: PersonaMood::Smug,
            text: "继续互怼不停歇！".to_string(),
        })
    });
    let tts = ScriptedTtsClient::new(
        true,
        vec![
            // 1st synthesis fails (does not trip yet).
            PlannedTtsReply::Immediate(Err(TtsError::Transport {
                endpoint: "http://virtual-tts.local:8081".to_string(),
                reason: "reset by peer".to_string(),
            })),
            // 2nd consecutive synthesis fails -> trips breaker -> TtsStatusNotice(false) + 15s cooldown!
            PlannedTtsReply::Immediate(Err(TtsError::HttpStatus {
                endpoint: "http://virtual-tts.local:8081".to_string(),
                status: 503,
                body: "loading".to_string(),
            })),
        ],
    );
    tts.set_fail_on_unhealthy_synth(true);

    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let runtime = TableRuntime::with_options(
        8789,
        None,
        TableOptions::with_wall_factory(301, factory)
            .with_pacing(PacingMode::Immediate)
            .with_llm_client(llm)
            .with_tts_client(tts.clone()),
    );
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let _ = start_solo_match(&runtime, &mut east).await;
    yield_tasks().await;

    // Startup health probe succeeded (1 call), opening banter triggered failure #1 (no outage yet).
    assert_eq!(tts.health_calls(), 1);
    assert_eq!(tts.captured().len(), 1);
    let msgs_1 = east.drain_all();
    assert!(!msgs_1
        .iter()
        .any(|m| matches!(m, ServerMessage::TtsStatusNotice { .. })));

    // Mark TTS unhealthy so the 15s probe will initially fail too.
    tts.set_healthy(false);

    // Quick emote #1 triggers failure #2 -> trips breaker -> emits TtsStatusNotice(available=false)!
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
    yield_tasks().await;
    let msgs_2 = east.drain_all();
    assert!(
        msgs_2.iter().any(|m| matches!(
            m,
            ServerMessage::TtsStatusNotice {
                available: false,
                ..
            }
        )),
        "2nd consecutive TTS failure must emit TtsStatusNotice(available=false): {msgs_2:?}"
    );
    // Advance slightly past any 650ms delayed follow-up reply from QuickEmote #1 before checking cooldown skip.
    tokio::time::advance(Duration::from_secs(1)).await;
    yield_tasks().await;
    let _ = east.drain_all();
    let calls_at_trip = tts.captured().len();
    assert!(calls_at_trip >= 2);

    // During 15s cooldown (+5s), sending another quick emote emits TableBanter normally
    // while COMPLETELY SKIPPING upstream TTS calls!
    tokio::time::advance(Duration::from_secs(5)).await;
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::HurryUp,
        },
    )
    .await;
    yield_tasks().await;
    let msgs_cooldown = east.drain_all();
    assert!(msgs_cooldown
        .iter()
        .any(|m| matches!(m, ServerMessage::TableBanter { .. })));
    assert_eq!(
        tts.captured().len(),
        calls_at_trip,
        "must skip upstream TTS synthesis during 15s cooldown"
    );

    // Advance past first 15s cooldown (+10s more = +16s total): probe runs while still unhealthy -> no recovery, no duplicate notice.
    tokio::time::advance(TTS_COOLDOWN_DURATION - Duration::from_secs(5)).await;
    yield_tasks().await;
    assert_eq!(tts.health_calls(), 2);
    let msgs_still_down = east.drain_all();
    assert!(!msgs_still_down
        .iter()
        .any(|m| matches!(m, ServerMessage::TtsStatusNotice { .. })));

    // Restore TTS health and advance another 15s -> background probe succeeds and broadcasts TtsStatusNotice(available=true)!
    tts.set_healthy(true);
    tokio::time::advance(TTS_COOLDOWN_DURATION).await;
    yield_tasks().await;
    assert_eq!(tts.health_calls(), 3);
    let msgs_recovered = east.drain_all();
    assert!(
        msgs_recovered.iter().any(|m| matches!(
            m,
            ServerMessage::TtsStatusNotice {
                available: true,
                ..
            }
        )),
        "expected recovery TtsStatusNotice(available=true): {msgs_recovered:?}"
    );

    // Subsequent quick emote resumes synthesizing audio!
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::NicePlay,
        },
    )
    .await;
    yield_tasks().await;
    let msgs_after_recovery = east.drain_all();
    assert!(
        tts.captured().len() > calls_at_trip,
        "synthesis must resume after recovery"
    );
    assert!(
        msgs_after_recovery
            .iter()
            .any(|m| matches!(m, ServerMessage::TableBanterAudioReady { .. })),
        "expected TableBanterAudioReady after TTS recovery: {msgs_after_recovery:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn startup_unhealthy_tts_notifies_hello_clients_without_blocking_and_recovers() {
    let llm = ScriptedLlmClient::with_banter_fn(|_| {
        Ok(BanterOutput {
            mood: PersonaMood::Deadpan,
            text: "语音没开照样吐槽".to_string(),
        })
    });
    let tts = ScriptedTtsClient::new(false, vec![]);
    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let runtime = TableRuntime::with_options(
        8789,
        None,
        TableOptions::with_wall_factory(401, factory)
            .with_pacing(PacingMode::Immediate)
            .with_llm_client(llm)
            .with_tts_client(tts.clone()),
    );
    // Let non-blocking startup probe fail.
    yield_tasks().await;
    assert_eq!(tts.health_calls(), 1);

    // Newly connecting client receives TtsStatusNotice(available=false) right after Hello snapshot.
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let hello_followup = east.drain_all();
    assert!(
        hello_followup.iter().any(|m| matches!(
            m,
            ServerMessage::TtsStatusNotice {
                available: false,
                ..
            }
        )),
        "client connecting after startup probe failure must receive TtsStatusNotice(false): {hello_followup:?}"
    );

    // Match starts cleanly and emits TableBanter without calling synthesize during cooldown.
    let _ = start_solo_match(&runtime, &mut east).await;
    yield_tasks().await;
    let open_msgs = east.drain_all();
    assert!(open_msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::TableBanter { .. })));
    assert_eq!(tts.captured().len(), 0);

    // TTS comes online -> 15s probe recovers automatically!
    tts.set_healthy(true);
    tokio::time::advance(TTS_COOLDOWN_DURATION + Duration::from_millis(50)).await;
    yield_tasks().await;
    let rec = east.drain_all();
    assert!(rec.iter().any(|m| matches!(
        m,
        ServerMessage::TtsStatusNotice {
            available: true,
            ..
        }
    )));
}

#[tokio::test(start_paused = true)]
async fn runtime_audio_cache_respects_lru_ttl_and_no_tts_emits_banter_id_only() {
    // 1. Custom 2-entry / 60s / <5MiB cache verifies eviction through TableRuntime::get_banter_audio.
    let cache = Arc::new(Mutex::new(BanterAudioCache::with_limits(
        2,
        Duration::from_secs(60),
        5 * 1024 * 1024,
    )));
    let llm = ScriptedLlmClient::with_banter_fn(|_| {
        Ok(BanterOutput {
            mood: PersonaMood::Smug,
            text: "缓存淘汰测试".to_string(),
        })
    });
    let tts = ScriptedTtsClient::new(true, vec![]);
    let wall = no_reaction_wall();
    let factory: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    });
    let runtime = TableRuntime::with_options(
        8789,
        None,
        TableOptions::with_wall_factory(501, factory)
            .with_pacing(PacingMode::Immediate)
            .with_llm_client(llm.clone())
            .with_tts_client(tts)
            .with_banter_audio_cache(cache),
    );
    let mut east = VirtualClient::connect(&runtime, "e1", "小王").await;
    let _ = start_solo_match(&runtime, &mut east).await;
    yield_tasks().await;
    let _ = east.drain_all();
    assert!(runtime.get_banter_audio("b-1").is_some());

    // Trigger 2 more banter lines (spaced by quick emote cooldown) to fill and evict b-1.
    tokio::time::advance(Duration::from_secs(5)).await;
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
    yield_tasks().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    east.send(
        &runtime,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::HurryUp,
        },
    )
    .await;
    yield_tasks().await;
    assert!(
        runtime.get_banter_audio("b-1").is_none(),
        "b-1 must be evicted once cache exceeds its entry cap"
    );

    // Advance 61s -> all remaining cached clips expire by TTL.
    tokio::time::advance(Duration::from_secs(61)).await;
    assert!(runtime.get_banter_audio("b-2").is_none());
    assert!(runtime.get_banter_audio("b-3").is_none());

    // 2. No TTS configured (`--no-tts` mode): TableBanter still carries banter_id, zero audio/status events.
    let wall2 = no_reaction_wall();
    let factory2: WallProviderFactory = Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall2.clone(),
        )))
    });
    let runtime_no_tts = TableRuntime::with_options(
        8789,
        None,
        TableOptions::with_wall_factory(502, factory2)
            .with_pacing(PacingMode::Immediate)
            .with_llm_client(llm),
    );
    let mut east2 = VirtualClient::connect(&runtime_no_tts, "e2", "老李").await;
    let _ = start_solo_match(&runtime_no_tts, &mut east2).await;
    yield_tasks().await;
    let msgs = east2.drain_all();
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::TableBanter { banter_id, .. } if banter_id == "b-1")));
    assert!(!msgs.iter().any(|m| matches!(
        m,
        ServerMessage::TableBanterAudioReady { .. } | ServerMessage::TtsStatusNotice { .. }
    )));
}
