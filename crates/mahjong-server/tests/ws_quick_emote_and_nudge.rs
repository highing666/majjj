mod common;

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use common::{
    connect_and_hello, http_get, no_reaction_wall, recv_msg, recv_snapshot, send_msg,
    ScriptedLlmClient, TempDirGuard,
};
use hyper::StatusCode;
use mahjong_ai::AiDifficulty;
use mahjong_core::{MatchMode, PresetWallProvider, Seat};
use mahjong_server::history_dto::PreferencesData;
use mahjong_server::llm::{BanterGenerationRequest, BanterOutput, LlmError};
use mahjong_server::pacing::PacingMode;
use mahjong_server::persona::PersonaMood;
use mahjong_server::protocol::{
    ClientMessage, QuickEmote, SeatKind, SeatingRule, ServerMessage, TablePace, TablePhase,
};
use mahjong_server::session::{BoxedWallProvider, TableOptions, WallProviderFactory};
use mahjong_server::ServerConfig;

fn emote_nudge_responder(request: &BanterGenerationRequest) -> Result<BanterOutput, LlmError> {
    let user_prompt = &request.user_prompt;
    let (mood, text) = if user_prompt.contains("快捷互动回应") {
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

fn no_reaction_factory() -> WallProviderFactory {
    let wall = no_reaction_wall();
    Arc::new(move || {
        BoxedWallProvider(Box::new(PresetWallProvider::single(
            Seat::East,
            wall.clone(),
        )))
    })
}

#[tokio::test]
async fn real_ws_quick_emotes_broadcast_trigger_ai_replies_and_nudge_persists_across_restart() {
    let temp = TempDirGuard::new("majjj_ws_quick_emote");
    let llm = ScriptedLlmClient::with_banter_fn(emote_nudge_responder);

    let server = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_data_dir(temp.path())
        .with_llm_client(llm.clone())
        .spawn_with_options(
            TableOptions::with_wall_factory(404, no_reaction_factory())
                .with_pacing(PacingMode::Immediate),
        )
        .await
        .expect("spawn server 1");

    let mut east = connect_and_hello(server.addr(), "ws-e1", "小王").await;
    let mut south = connect_and_hello(server.addr(), "ws-s1", "小李").await;

    send_msg(
        &mut east,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::East),
        },
    )
    .await;
    let _ = recv_snapshot(&mut east).await;
    send_msg(
        &mut south,
        &ClientMessage::JoinSeat {
            seat: Some(Seat::South),
        },
    )
    .await;
    let _ = recv_snapshot(&mut south).await;

    send_msg(
        &mut east,
        &ClientMessage::UpdateLobbyConfig {
            mode: MatchMode::FreeTable,
            seating: SeatingRule::KeepLobbySeats,
            ai_difficulty: AiDifficulty::Strong,
            pace: TablePace::Standard,
        },
    )
    .await;
    send_msg(&mut east, &ClientMessage::StartMatch).await;
    let start_snap = recv_snapshot(&mut east).await;
    let _ = recv_snapshot(&mut south).await;
    assert_eq!(start_snap.phase, TablePhase::InMatch);
    assert!(start_snap.slow_think_nudge_enabled);

    // Send QuickEmote::DealerRigged over real WS.
    send_msg(
        &mut east,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::DealerRigged,
        },
    )
    .await;

    // East receives QuickEmoteBroadcast + 2 AI TableBanter replies referencing 「发牌员演我」.
    let mut east_saw_broadcast = false;
    let mut ai_reply_seats = Vec::new();
    while !(east_saw_broadcast && ai_reply_seats.len() >= 2) {
        match recv_msg(&mut east).await {
            ServerMessage::QuickEmoteBroadcast { seat, emote, text } => {
                assert_eq!(seat, Seat::East);
                assert_eq!(emote, QuickEmote::DealerRigged);
                assert_eq!(text, "发牌员演我");
                east_saw_broadcast = true;
            }
            ServerMessage::TableBanter {
                seat,
                text,
                reply_to_seat: Some(Seat::East),
                ..
            } => {
                assert!(text.contains("发牌员演我"));
                ai_reply_seats.push(seat);
            }
            ServerMessage::TableBanter { .. }
            | ServerMessage::TableBanterAudioReady { .. }
            | ServerMessage::TableBanterAudioFallback { .. }
            | ServerMessage::TtsStatusNotice { .. }
            | ServerMessage::LobbyUpdated { .. }
            | ServerMessage::ServerPing { .. } => {}
            other => panic!("unexpected msg on east WS: {other:?}"),
        }
    }
    assert_eq!(ai_reply_seats.len(), 2);
    assert_ne!(ai_reply_seats[0], ai_reply_seats[1]);

    // South peer also received QuickEmoteBroadcast.
    loop {
        match recv_msg(&mut south).await {
            ServerMessage::QuickEmoteBroadcast { seat, emote, text } => {
                assert_eq!(seat, Seat::East);
                assert_eq!(emote, QuickEmote::DealerRigged);
                assert_eq!(text, "发牌员演我");
                break;
            }
            ServerMessage::TableBanter { .. }
            | ServerMessage::TableBanterAudioReady { .. }
            | ServerMessage::TableBanterAudioFallback { .. }
            | ServerMessage::TtsStatusNotice { .. }
            | ServerMessage::LobbyUpdated { .. }
            | ServerMessage::ServerPing { .. } => {}
            other => panic!("unexpected msg on south WS: {other:?}"),
        }
    }

    // Immediate second quick emote hits 4s cooldown -> ActionRejected on East without closing WS!
    send_msg(
        &mut east,
        &ClientMessage::SendQuickEmote {
            emote: QuickEmote::IsThatIt,
        },
    )
    .await;
    loop {
        match recv_msg(&mut east).await {
            ServerMessage::ActionRejected { reason, .. } => {
                assert!(reason.contains("冷却"), "unexpected reason: {reason}");
                break;
            }
            ServerMessage::TableBanter { .. }
            | ServerMessage::TableBanterAudioReady { .. }
            | ServerMessage::TableBanterAudioFallback { .. }
            | ServerMessage::TtsStatusNotice { .. }
            | ServerMessage::LobbyUpdated { .. }
            | ServerMessage::ServerPing { .. } => {}
            other => panic!("expected ActionRejected during cooldown, got {other:?}"),
        }
    }

    // Toggle slow_think_nudge_enabled = false over real WS -> verify both HTTP /api/preferences and server restart!
    send_msg(
        &mut east,
        &ClientMessage::SetSlowThinkNudge { enabled: false },
    )
    .await;
    loop {
        match recv_msg(&mut south).await {
            ServerMessage::LobbyUpdated {
                slow_think_nudge_enabled,
                ..
            } => {
                assert!(!slow_think_nudge_enabled);
                break;
            }
            ServerMessage::TableBanter { .. }
            | ServerMessage::TableBanterAudioReady { .. }
            | ServerMessage::TableBanterAudioFallback { .. }
            | ServerMessage::TtsStatusNotice { .. }
            | ServerMessage::ServerPing { .. } => {}
            other => panic!("expected LobbyUpdated on south WS, got {other:?}"),
        }
    }

    let (status, _, body) = http_get(server.addr(), "/api/preferences").await;
    assert_eq!(status, StatusCode::OK);
    let prefs: PreferencesData = serde_json::from_slice(&body).expect("parse PreferencesData");
    assert!(!prefs.slow_think_nudge_enabled);

    let _ = east.close(None).await;
    let _ = south.close(None).await;
    server.shutdown().await;

    // Restart server against the exact same data_dir -> new TableSnapshot MUST report slow_think_nudge_enabled == false!
    let server2 = ServerConfig::default()
        .with_bind(IpAddr::V4(Ipv4Addr::LOCALHOST))
        .with_port(0)
        .with_data_dir(temp.path())
        .with_llm_client(llm)
        .spawn_with_options(
            TableOptions::with_wall_factory(405, no_reaction_factory())
                .with_pacing(PacingMode::Immediate),
        )
        .await
        .expect("spawn server 2");

    let mut ws2 = connect_and_hello(server2.addr(), "ws-rejoin", "小王").await;
    send_msg(&mut ws2, &ClientMessage::RequestSnapshot).await;
    let snap2 = recv_snapshot(&mut ws2).await;
    assert!(!snap2.slow_think_nudge_enabled);
    assert_eq!(snap2.seats[Seat::East.index()].kind, SeatKind::Empty);

    let _ = ws2.close(None).await;
    server2.shutdown().await;
}
